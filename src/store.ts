import { create } from "zustand";
import { api, baseName, errorMessage } from "./api";
import { gitNeedsSecurityUpdate, outdatedGitMessage } from "./lib/gitVersion";
import { forgetSections } from "./uiStore";
import { noFilters, type HistoryFilters } from "./lib/historyFilter";
import { checkForUpdate } from "./updater";
import { addRecent, type RecentRepo } from "./lib/recent";
import type {
  Bookmark,
  Commit,
  ErrorCtx,
  OpProgress,
  OpResult,
  RefInfo,
  RepoStatus,
  StashEntry,
  Submodule,
  Tab,
  ThemeMode,
} from "./types";

export type View = "files" | "history";

export interface FileSel {
  path: string;
  kind: "staged" | "unstaged" | "conflict";
  untracked: boolean;
}

export interface RepoUi {
  view: View;
  sel: FileSel | null;
  selCommit: string | null;
  /** A second commit picked with Ctrl+click; the history then shows the difference between the two. */
  compareCommit: string | null;
  historyAll: boolean;
  /** History filters (who, what, when). They highlight commits; the list and graph stay whole. */
  filters: HistoryFilters;
  /** Unsent commit message, kept per tab. */
  draft: string;
  amend: boolean;
  /** Branch/tag whose history is highlighted in the graph (sidebar click); null = none. */
  focusRef: string | null;
}

export interface RepoData {
  status?: RepoStatus;
  submodules: Submodule[];
  refs: RefInfo[];
  stashes: StashEntry[];
  remotes: string[];
  log: Commit[];
  logDone: boolean;
  logLoading: boolean;
  logLoaded: boolean;
  sig: string;
  loading: boolean;
  error?: string;
  busy: string | null;
  ui: RepoUi;
}

export interface Toast {
  id: number;
  kind: "info" | "error" | "success";
  text: string;
  ctx?: ErrorCtx;
}

export interface DialogField {
  key: string;
  label: string;
  type: "text" | "textarea" | "checkbox" | "select";
  value?: string | boolean;
  placeholder?: string;
  options?: { value: string; label: string }[];
  /** Only shown while this other field is truthy. */
  showIf?: string;
}

export interface DialogSpec {
  title: string;
  description?: string;
  fields: DialogField[];
  submitLabel: string;
  danger?: boolean;
  onSubmit: (values: Record<string, string | boolean>) => void | Promise<void>;
}

const TRUSTED_REPOS_KEY = "trusted-repos";

/** Folders the user already confirmed they trust, so the warning about program-running settings shows once. */
function isTrustedRepo(path: string): boolean {
  try {
    return (JSON.parse(localStorage.getItem(TRUSTED_REPOS_KEY) ?? "[]") as string[]).includes(path);
  } catch {
    return false;
  }
}

function trustRepo(path: string): void {
  try {
    const list = JSON.parse(localStorage.getItem(TRUSTED_REPOS_KEY) ?? "[]") as string[];
    if (!list.includes(path)) localStorage.setItem(TRUSTED_REPOS_KEY, JSON.stringify([...list, path].slice(-500)));
  } catch {
    /* storage unavailable: the question is simply asked again next time */
  }
}

const LOG_PAGE = 400;
const emptyUi = (): RepoUi => ({
  view: "files",
  sel: null,
  selCommit: null,
  compareCommit: null,
  historyAll: true,
  filters: noFilters(),
  draft: "",
  amend: false,
  focusRef: null,
});
const emptyRepo = (): RepoData => ({
  submodules: [],
  refs: [],
  stashes: [],
  remotes: [],
  log: [],
  logDone: false,
  logLoading: false,
  logLoaded: false,
  sig: "",
  loading: false,
  busy: null,
  ui: emptyUi(),
});

interface AppState {
  ready: boolean;
  tabs: Tab[];
  activeId: string | null;
  bookmarks: Bookmark[];
  /** Repositories opened lately, newest first; Home shows the ones that are not bookmarked. */
  recent: RecentRepo[];
  theme: ThemeMode;
  data: Record<string, RepoData>;
  toasts: Toast[];
  progress: OpProgress | null;
  dialog: DialogSpec | null;
  paletteOpen: boolean;

  init: () => Promise<void>;
  newTab: () => void;
  closeTab: (id: string) => void;
  setActive: (id: string) => void;
  cycleTab: (step: 1 | -1) => void;
  /** Moves a tab so that it ends up at `toIndex` in the tab bar. */
  moveTab: (id: string, toIndex: number) => void;
  /** Opens a repo in the active home tab (or a new tab). Throws if it is not a repo. */
  openRepo: (path: string, afterTabId?: string) => Promise<void>;
  reload: (id?: string) => Promise<void>;
  loadLog: (id: string, reset: boolean) => Promise<void>;
  setUi: (id: string, patch: Partial<RepoUi>) => void;
  /** Runs a repository operation, surfaces its result as a toast, and refreshes the repo. */
  run: (
    id: string,
    label: string,
    fn: () => Promise<OpResult | void>,
    opts?: { success?: string; quiet?: boolean; ctx?: ErrorCtx },
  ) => Promise<boolean>;
  addBookmark: (path: string) => void;
  removeBookmark: (id: string) => void;
  removeRecent: (path: string) => void;
  setTheme: (t: ThemeMode) => void;
  notify: (kind: Toast["kind"], text: string, ctx?: ErrorCtx) => void;
  dismiss: (id: number) => void;
  setProgress: (p: OpProgress | null) => void;
  openDialog: (d: DialogSpec) => void;
  closeDialog: () => void;
  setPalette: (open: boolean) => void;
}

const uid = () => crypto.randomUUID();
let toastSeq = 0;
const inflight = new Set<string>();
const queued = new Set<string>();
const logQueued = new Set<string>();

export const useApp = create<AppState>((set, get) => {
  const patchRepo = (id: string, patch: Partial<RepoData>) =>
    set((s) =>
      s.tabs.some((t) => t.id === id)
        ? { data: { ...s.data, [id]: { ...(s.data[id] ?? emptyRepo()), ...patch } } }
        : s,
    );

  /** Watches only the active repo for local changes; nothing is polled (server changes: see lib/autoFetch.ts). */
  const watchActive = () => {
    const { tabs, activeId } = get();
    const tab = tabs.find((t) => t.id === activeId);
    if (tab?.path) void api.watch(tab.path).catch(() => undefined);
    else void api.unwatch().catch(() => undefined);
  };

  return {
    ready: false,
    tabs: [],
    activeId: null,
    bookmarks: [],
    recent: [],
    theme: "system",
    data: {},
    toasts: [],
    progress: null,
    dialog: null,
    paletteOpen: false,

    async init() {
      const saved = await api.loadState().catch(() => null);
      const tabs = saved?.tabs ?? [];
      const activeId =
        saved?.activeId && tabs.some((t) => t.id === saved.activeId)
          ? saved.activeId
          : (tabs[0]?.id ?? null);
      set({
        tabs,
        activeId,
        bookmarks: saved?.bookmarks ?? [],
        recent: saved?.recent ?? [],
        theme: saved?.theme ?? "system",
        ready: true,
      });
      if (tabs.length === 0) get().newTab();
      else {
        watchActive();
        void get().reload();
      }
      // The app runs the system Git, so an old Git means old clone and submodule bugs. Say so once per version.
      void api
        .gitInfo()
        .then((info) => {
          const version = info.version;
          if (!version || !gitNeedsSecurityUpdate(version)) return;
          try {
            if (localStorage.getItem("git-update-warned") === version) return;
            localStorage.setItem("git-update-warned", version);
          } catch {
            /* storage unavailable: the notice may repeat on the next start */
          }
          get().notify("info", outdatedGitMessage(version));
        })
        .catch(() => undefined);
      // Quiet start-up check: only speaks up when a newer version exists; failures (offline, no release yet) stay silent.
      void checkForUpdate()
        .then((u) => u && get().notify("info", `Version ${u.version} is available. Open Settings (the gear icon) to install it.`))
        .catch(() => undefined);
    },

    newTab() {
      const tab: Tab = { id: uid(), path: null, name: "New tab" };
      set((s) => ({ tabs: [...s.tabs, tab], activeId: tab.id }));
      watchActive();
    },

    closeTab(id) {
      const { tabs, activeId } = get();
      const idx = tabs.findIndex((t) => t.id === id);
      if (idx < 0) return;
      const next = tabs.filter((t) => t.id !== id);
      const data = { ...get().data };
      delete data[id];
      forgetSections(id);
      const newActive =
        activeId === id ? (next[Math.min(idx, next.length - 1)]?.id ?? null) : activeId;
      set({ tabs: next, activeId: newActive, data });
      if (next.length === 0) get().newTab();
      else {
        watchActive();
        if (newActive !== activeId) void get().reload();
      }
    },

    setActive(id) {
      set({ activeId: id });
      watchActive();
      void get().reload(id);
    },

    moveTab(id, toIndex) {
      const tabs = get().tabs;
      const from = tabs.findIndex((t) => t.id === id);
      const to = Math.max(0, Math.min(toIndex, tabs.length - 1));
      if (from < 0 || from === to) return;
      const next = [...tabs];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      set({ tabs: next });
    },

    cycleTab(step) {
      const { tabs, activeId } = get();
      if (tabs.length < 2) return;
      const idx = tabs.findIndex((t) => t.id === activeId);
      get().setActive(tabs[(idx + step + tabs.length) % tabs.length].id);
    },

    async openRepo(path, afterTabId) {
      const top = await api.toplevel(path); // throws a friendly message if not a repo
      const { tabs, activeId } = get();
      const existing = tabs.find((t) => t.path === top);
      const remember = () => set((s) => ({ recent: addRecent(s.recent, { path: top, name: baseName(top) }) }));
      if (existing) {
        get().setActive(existing.id);
        remember();
        return;
      }
      // A folder handed to you with its own .git can carry settings or hooks that make git run programs.
      // Ask once before opening such a repository (nothing in it has been run yet at this point).
      // If the check itself fails, ask anyway: opening silently would trust a repository we could not look at.
      const risks = await api
        .repoRisks(top)
        .catch((e) => [`Treeline could not check this repository's settings (${errorMessage(e)}).`]);
      if (risks.length > 0 && !isTrustedRepo(top)) {
        const shown = risks.slice(0, 8).map((r) => `• ${r}`).join("\n");
        const more = risks.length > 8 ? `\n• …and ${risks.length - 8} more` : "";
        get().openDialog({
          title: "This repository can run programs",
          description:
            `Its own settings or hooks can make git run programs on your computer when you commit, merge, check out or fetch:\n\n${shown}${more}\n\n` +
            "Open it only if you made this folder or got it from a source you trust. Treeline remembers your answer for this folder.",
          fields: [],
          submitLabel: "Open anyway",
          danger: true,
          onSubmit: async () => {
            trustRepo(top);
            await get().openRepo(path, afterTabId);
          },
        });
        return;
      }
      const active = tabs.find((t) => t.id === activeId);
      const name = baseName(top);
      const parentIdx = afterTabId ? tabs.findIndex((t) => t.id === afterTabId) : -1;
      if (parentIdx >= 0) {
        // A submodule opens right next to its parent repository, not at the far end of the tab bar.
        const tab: Tab = { id: uid(), path: top, name };
        const next = [...tabs];
        next.splice(parentIdx + 1, 0, tab);
        set({ tabs: next, activeId: tab.id });
      } else if (active && active.path === null) {
        set({ tabs: tabs.map((t) => (t.id === active.id ? { ...t, path: top, name } : t)) });
      } else {
        const tab: Tab = { id: uid(), path: top, name };
        set({ tabs: [...tabs, tab], activeId: tab.id });
      }
      remember();
      watchActive();
      await get().reload();
    },

    /** Refreshes one repo. Concurrent calls coalesce into one trailing refresh. */
    async reload(id) {
      const tabId = id ?? get().activeId;
      const tab = get().tabs.find((t) => t.id === tabId);
      if (!tab || !tab.path) return;
      if (inflight.has(tab.id)) {
        queued.add(tab.id);
        return;
      }
      inflight.add(tab.id);
      const path = tab.path;
      patchRepo(tab.id, { loading: true, error: undefined });
      try {
        const [status, submodules, refs, stashes, remotes] = await Promise.all([
          api.status(path),
          api.submodules(path),
          api.refs(path),
          api.stashes(path),
          api.remotes(path),
        ]);
        // Names matter as well as shas: switching between two branches on one commit, or renaming a
        // branch, changes the graph's labels without changing any commit.
        const sig = [
          status.head,
          status.branch,
          status.state,
          ...refs.map((r) => `${r.kind}:${r.name}:${r.sha}`),
          stashes.length,
        ].join("|");
        const prev = get().data[tab.id] ?? emptyRepo();
        const gotConflicts = status.files.some((f) => f.conflicted) && !prev.status?.files.some((f) => f.conflicted);
        patchRepo(tab.id, {
          status,
          submodules,
          refs,
          stashes,
          remotes,
          loading: false,
          sig,
          ...(gotConflicts ? { ui: { ...prev.ui, view: "files" as View } } : {}),
        });
        const cur = get().data[tab.id];
        const changed = prev.sig !== sig;
        // Reload history when it is visible and missing/stale, or cached and now stale.
        if ((cur.ui.view === "history" && !cur.logLoaded) || (cur.logLoaded && changed)) {
          void get().loadLog(tab.id, true);
        }
      } catch (e) {
        patchRepo(tab.id, { error: errorMessage(e), loading: false });
      } finally {
        inflight.delete(tab.id);
        if (queued.delete(tab.id)) void get().reload(tab.id);
      }
    },

    async loadLog(id, reset) {
      const tab = get().tabs.find((t) => t.id === id);
      const d = get().data[id];
      if (!tab?.path || !d) return;
      if (d.logLoading) {
        // A reset requested mid-flight (new search text, filter toggle, post-commit refresh) must not
        // be dropped; run it as soon as the current load settles.
        if (reset) logQueued.add(id);
        return;
      }
      if (!reset && d.logDone) return;
      patchRepo(id, { logLoading: true });
      try {
        const skip = reset ? 0 : d.log.length;
        const page = await api.log(tab.path, skip, LOG_PAGE, d.ui.historyAll);
        const cur = get().data[id];
        patchRepo(id, {
          log: reset ? page : [...cur.log, ...page],
          logDone: page.length < LOG_PAGE,
          logLoaded: true,
          logLoading: false,
        });
        // A rewritten history (rebase, reset) can drop the selected commit; clear it instead of erroring.
        const sel = get().data[id]?.ui.selCommit;
        if (reset && sel && !sel.startsWith("stash@") && page.length < LOG_PAGE && !page.some((c) => c.sha === sel)) {
          get().setUi(id, { selCommit: null, compareCommit: null });
        }
      } catch (e) {
        // logDone stops the "load more" effect from retrying a failing query in a tight loop.
        patchRepo(id, { logLoading: false, logDone: true, error: errorMessage(e) });
      } finally {
        if (logQueued.delete(id)) void get().loadLog(id, true);
      }
    },

    setUi(id, patch) {
      const d = get().data[id] ?? emptyRepo();
      patchRepo(id, { ui: { ...d.ui, ...patch } });
      const next = { ...d.ui, ...patch };
      const filterChanged =
        patch.historyAll !== undefined && patch.historyAll !== d.ui.historyAll;
      if ((next.view === "history" && !d.logLoaded) || (next.view === "history" && filterChanged)) {
        void get().loadLog(id, true);
      }
    },

    async run(id, label, fn, opts) {
      const tab = get().tabs.find((t) => t.id === id);
      if (!tab?.path) return false;
      patchRepo(id, { busy: label });
      let ok = true;
      try {
        const res = await fn();
        if (res && typeof res === "object" && "ok" in res) {
          ok = res.ok;
          const text = res.output.trim();
          if (!res.ok) get().notify("error", text || `${label} failed`, opts?.ctx);
          else if (opts?.success) get().notify("success", opts.success);
          else if (!opts?.quiet && text) get().notify("info", text.split("\n").slice(-3).join("\n"));
        } else if (opts?.success) get().notify("success", opts.success);
      } catch (e) {
        ok = false;
        get().notify("error", errorMessage(e), opts?.ctx);
      } finally {
        patchRepo(id, { busy: null });
        set({ progress: null });
        await get().reload(id);
      }
      return ok;
    },

    addBookmark(path) {
      set((s) =>
        s.bookmarks.some((b) => b.path === path)
          ? s
          : { bookmarks: [...s.bookmarks, { id: uid(), name: baseName(path), path }] },
      );
    },

    removeRecent(path) {
      set((s) => ({ recent: s.recent.filter((r) => r.path !== path) }));
    },

    removeBookmark(id) {
      set((s) => ({ bookmarks: s.bookmarks.filter((b) => b.id !== id) }));
    },

    setTheme(theme) {
      set({ theme });
    },

    notify(kind, text, ctx) {
      // Errors carry explanations and fixes, so they stay until dismissed; an identical error is shown once.
      if (kind === "error" && get().toasts.some((t) => t.kind === "error" && t.text === text)) return;
      const id = ++toastSeq;
      set((s) => ({ toasts: [...s.toasts.slice(-3), { id, kind, text, ctx }] }));
      if (kind !== "error") setTimeout(() => get().dismiss(id), 5000);
    },
    dismiss(id) {
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
    },
    setProgress: (progress) => set({ progress }),
    openDialog: (dialog) => set({ dialog }),
    closeDialog: () => set({ dialog: null }),
    setPalette: (paletteOpen) => set({ paletteOpen }),
  };
});

// Persist tabs/bookmarks/theme (debounced) whenever they change.
let timer: ReturnType<typeof setTimeout> | undefined;
useApp.subscribe((s, prev) => {
  if (!s.ready) return;
  if (
    s.tabs === prev.tabs &&
    s.activeId === prev.activeId &&
    s.bookmarks === prev.bookmarks &&
    s.recent === prev.recent &&
    s.theme === prev.theme
  )
    return;
  clearTimeout(timer);
  timer = setTimeout(() => {
    void api
      .saveState({ tabs: s.tabs, activeId: s.activeId, bookmarks: s.bookmarks, recent: s.recent, theme: s.theme })
      .catch(() => undefined);
  }, 300);
});

/** Convenience: the data and tab for the active repo. */
export function useActive() {
  const tab = useApp((s) => s.tabs.find((t) => t.id === s.activeId));
  const data = useApp((s) => (s.activeId ? s.data[s.activeId] : undefined));
  return { tab, data };
}
