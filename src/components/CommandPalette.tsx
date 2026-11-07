import { useEffect, useMemo, useRef, useState } from "react";
import { repoActions } from "../actions";
import { errorMessage } from "../api";
import { useApp } from "../store";
import { useUi } from "../uiStore";
import { GUIDE, openGuide } from "../guide";
import { pickAndOpenRepo } from "./Home";

interface Cmd {
  id: string;
  label: string;
  hint?: string;
  run: () => void;
}

export function CommandPalette({ onClone }: { onClone: () => void }) {
  const open = useApp((s) => s.paletteOpen);
  if (!open) return null;
  return <Palette onClone={onClone} />;
}

function Palette({ onClone }: { onClone: () => void }) {
  const s = useApp.getState();
  const [q, setQ] = useState("");
  const [idx, setIdx] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const close = () => useApp.getState().setPalette(false);

  const cmds = useMemo<Cmd[]>(() => {
    const out: Cmd[] = [
      { id: "open", label: "Open repository…", hint: "Ctrl+O", run: () => void pickAndOpenRepo(s.openRepo, (m) => s.notify("error", m)) },
      { id: "clone", label: "Clone repository…", run: onClone },
      { id: "settings", label: "Settings… (Git program)", run: () => useUi.setState({ settings: true }) },
      { id: "tab", label: "New tab", hint: "Ctrl+T", run: s.newTab },
      { id: "guide", label: "Open the user guide", hint: "F1", run: () => void openGuide() },
      { id: "guide-errors", label: "Guide: when something goes wrong", run: () => void openGuide(GUIDE.errors) },
      { id: "guide-workflow", label: "Guide: everyday workflow", run: () => void openGuide(GUIDE.workflow) },
      { id: "guide-shortcuts", label: "Guide: keyboard shortcuts", run: () => void openGuide(GUIDE.shortcuts) },
      {
        id: "reset-layout",
        label: "Reset pane sizes",
        run: () => {
          try {
            Object.keys(localStorage)
              .filter((k) => k.startsWith("split:"))
              .forEach((k) => localStorage.removeItem(k));
          } catch {
            /* storage unavailable: nothing was remembered anyway */
          }
          location.reload();
        },
      },
      { id: "theme-dark", label: "Theme: dark", run: () => s.setTheme("dark") },
      { id: "theme-light", label: "Theme: light", run: () => s.setTheme("light") },
      { id: "theme-system", label: "Theme: follow system", run: () => s.setTheme("system") },
    ];
    const tab = s.tabs.find((t) => t.id === s.activeId);
    if (tab?.path) {
      const a = repoActions(tab.id);
      const d = s.data[tab.id];
      out.push(
        { id: "close", label: "Close tab", hint: "Ctrl+W", run: () => s.closeTab(tab.id) },
        { id: "view-files", label: "Show file status", run: () => s.setUi(tab.id, { view: "files" }) },
        { id: "view-history", label: "Show history", run: () => s.setUi(tab.id, { view: "history" }) },
        { id: "fetch", label: "Fetch", run: () => void a.fetch() },
        { id: "pull", label: "Pull (merge)", run: () => void a.pull("merge") },
        { id: "pull-rebase", label: "Pull (rebase)", run: () => void a.pull("rebase") },
        { id: "push", label: "Push…", run: a.push },
        { id: "branch", label: "New branch…", run: () => a.newBranch() },
        { id: "merge", label: "Merge branch…", run: a.merge },
        { id: "stash", label: "Stash changes…", run: a.stash },
        { id: "tag", label: "New tag…", run: () => a.newTag() },
        { id: "sub-update", label: "Update all submodules", run: () => void a.updateSubmodules() },
        { id: "terminal", label: "Open terminal here", run: a.terminal },
        { id: "reveal", label: "Show in file manager", run: a.reveal },
        { id: "reload", label: "Refresh", run: () => void s.reload(tab.id) },
      );
      for (const r of d?.refs ?? []) {
        if (r.kind === "branch" && !r.head) out.push({ id: `co-${r.name}`, label: `Check out ${r.name}`, run: () => void a.checkout(r.name) });
      }
    }
    for (const b of s.bookmarks) {
      out.push({
        id: `bm-${b.id}`,
        label: `Open bookmark: ${b.name}`,
        hint: b.path,
        run: () => s.openRepo(b.path).catch((e) => s.notify("error", errorMessage(e))),
      });
    }
    for (const t of s.tabs) {
      if (t.path && t.id !== s.activeId) out.push({ id: `tab-${t.id}`, label: `Switch to ${t.name}`, run: () => s.setActive(t.id) });
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const shown = useMemo(() => {
    const terms = q.toLowerCase().split(/\s+/).filter(Boolean);
    return cmds.filter((c) => terms.every((t) => c.label.toLowerCase().includes(t))).slice(0, 40);
  }, [q, cmds]);

  useEffect(() => input.current?.focus(), []);
  useEffect(() => setIdx(0), [q]);

  const exec = (c?: Cmd) => {
    if (!c) return;
    close();
    // Run after the palette closes so dialogs it opens are not dismissed by the same click.
    setTimeout(c.run, 0);
  };

  return (
    <div className="modal-backdrop top" onMouseDown={close}>
      <div className="palette" role="dialog" aria-label="Command palette" onMouseDown={(e) => e.stopPropagation()}>
        <input
          ref={input}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Type a command, branch or bookmark…"
          aria-label="Command"
          onKeyDown={(e) => {
            if (e.key === "Escape") close();
            else if (e.key === "ArrowDown") (e.preventDefault(), setIdx((i) => Math.min(shown.length - 1, i + 1)));
            else if (e.key === "ArrowUp") (e.preventDefault(), setIdx((i) => Math.max(0, i - 1)));
            else if (e.key === "Enter") exec(shown[idx]);
          }}
        />
        <ul role="listbox">
          {shown.map((c, i) => (
            <li key={c.id} role="option" aria-selected={i === idx} className={i === idx ? "active" : ""} onMouseEnter={() => setIdx(i)} onClick={() => exec(c)}>
              <span>{c.label}</span>
              {c.hint && <span className="hint-key">{c.hint}</span>}
            </li>
          ))}
          {shown.length === 0 && <li className="muted">No matching commands.</li>}
        </ul>
      </div>
    </div>
  );
}
