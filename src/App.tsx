import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { UPDATE_CHECK_INTERVAL_MS, checkForUpdate, createUpdateWatch } from "./updater";
import { createAutoFetch, startAutoFetchTimer } from "./lib/autoFetch";
import { useApp } from "./store";
import { useUi } from "./uiStore";
import { openGuide } from "./guide";
import type { OpProgress } from "./types";
import { TabBar } from "./components/TabBar";
import { Home, pickAndOpenRepo } from "./components/Home";
import { RepoView } from "./components/RepoView";
import { StatusBar } from "./components/StatusBar";
import { CloneDialog } from "./components/CloneDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { FileHistoryDialog } from "./components/FileHistoryDialog";
import { MergedBranchesDialog } from "./components/MergedBranchesDialog";
import { MultiMergeDialog } from "./components/MultiMergeDialog";
import { CommandPalette } from "./components/CommandPalette";
import { ContextMenuHost, FormDialogHost, Toasts } from "./components/Overlays";
import { RebaseDialogHost } from "./components/RebaseDialog";
import "./App.css";

export default function App() {
  const ready = useApp((s) => s.ready);
  const tabs = useApp((s) => s.tabs);
  const activeId = useApp((s) => s.activeId);
  const theme = useApp((s) => s.theme);
  const cloning = useUi((s) => s.clone);
  const settingsOpen = useUi((s) => s.settings);
  const fileHistory = useUi((s) => s.fileHistory);
  const mergedBranches = useUi((s) => s.mergedBranches);
  const multiMerge = useUi((s) => s.multiMerge);

  useEffect(() => {
    void useApp.getState().init();
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.dataset.theme = theme;
  }, [theme]);

  // Event wiring: file-system changes, network progress, window focus.
  useEffect(() => {
    const subs = [
      listen<string>("repo-changed", ({ payload }) => {
        const s = useApp.getState();
        const tab = s.tabs.find((t) => t.id === s.activeId);
        if (tab?.path === payload) void s.reload(tab.id);
      }),
      listen<OpProgress>("op-progress", ({ payload }) => useApp.getState().setProgress(payload)),
      getCurrentWindow().onFocusChanged(({ payload: focused }) => {
        if (focused) void useApp.getState().reload();
      }),
    ];
    return () => subs.forEach((p) => void p.then((f) => f()));
  }, []);

  // Background fetch: the file watcher only sees local changes, so without this, branches merged or
  // pushed on the server (GitHub/GitLab web UI, teammates) never show up until you press Fetch.
  // Silent: no toast and no progress bar, and failures (offline, expired credentials) are ignored.
  useEffect(() => {
    const tick = createAutoFetch({
      activeTab: () => {
        const s = useApp.getState();
        const tab = s.tabs.find((t) => t.id === s.activeId);
        if (!tab?.path || tab.host) return null; // no background fetch on another computer yet (read-only)
        const d = s.data[tab.id];
        return { id: tab.id, path: tab.path, hasRemotes: d ? d.remotes.length > 0 : null };
      },
      fetch: (id, path) => api.fetch(id, path, true),
      reload: (id) => useApp.getState().reload(id),
      isHidden: () => document.visibilityState === "hidden",
    });
    // Restarts itself whenever the interval is changed in Settings.
    const stopTimer = startAutoFetchTimer(tick);
    const unfocus = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) void tick();
    });
    return () => {
      stopTimer();
      void unfocus.then((f) => f());
    };
  }, []);

  // Look for a newer version of Treeline shortly after start and then every few hours; a hit puts a
  // badge on the Settings button. Nothing is downloaded or installed without the user's say-so.
  useEffect(() => {
    const tick = createUpdateWatch({
      check: checkForUpdate,
      onAvailable: (update) => useUi.setState({ update }),
    });
    const first = window.setTimeout(() => void tick(), 10_000);
    const timer = window.setInterval(() => void tick(), UPDATE_CHECK_INTERVAL_MS);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F1") {
        e.preventDefault();
        void openGuide().catch((err) => useApp.getState().notify("error", String(err)));
        return;
      }
      if (!(e.ctrlKey || e.metaKey)) return;
      if (e.key === "?" || (e.key === "/" && e.shiftKey)) {
        e.preventDefault();
        void openGuide().catch((err) => useApp.getState().notify("error", String(err)));
        return;
      }
      const app = useApp.getState();
      const key = e.key.toLowerCase();
      if (key === "t") app.newTab();
      else if (key === "w" && app.activeId) app.closeTab(app.activeId);
      else if (key === "o") void pickAndOpenRepo(app.openRepo, (m) => app.notify("error", m));
      else if (key === "k" || (key === "p" && e.shiftKey)) app.setPalette(!app.paletteOpen);
      else if (key === "r" && !e.shiftKey) void app.reload();
      else if (e.key === "Tab") app.cycleTab(e.shiftKey ? -1 : 1);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!ready) return <div className="splash">Loading…</div>;

  const active = tabs.find((t) => t.id === activeId);
  return (
    <div className="app">
      <TabBar />
      <main className="main">{active?.path ? <RepoView key={active.id} tab={active} /> : <Home />}</main>
      <StatusBar />
      <ContextMenuHost />
      <FormDialogHost />
      <RebaseDialogHost />
      <Toasts />
      <CommandPalette onClone={() => useUi.setState({ clone: true })} />
      {cloning && <CloneDialog onClose={() => useUi.setState({ clone: false })} />}
      {fileHistory && (
        <FileHistoryDialog key={`${fileHistory.repo}:${fileHistory.file}:${fileHistory.mode}`} req={fileHistory} onClose={() => useUi.setState({ fileHistory: null })} />
      )}
      {multiMerge && <MultiMergeDialog key={multiMerge.tabId} tabId={multiMerge.tabId} initialSource={multiMerge.source} onClose={() => useUi.setState({ multiMerge: null })} />}
      {mergedBranches && <MergedBranchesDialog key={mergedBranches.tabId} tabId={mergedBranches.tabId} onClose={() => useUi.setState({ mergedBranches: null })} />}
      {settingsOpen && <SettingsDialog onClose={() => useUi.setState({ settings: false })} />}
    </div>
  );
}
