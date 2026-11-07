import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useApp } from "./store";
import { useUi } from "./uiStore";
import type { OpProgress } from "./types";
import { TabBar } from "./components/TabBar";
import { Home, pickAndOpenRepo } from "./components/Home";
import { RepoView } from "./components/RepoView";
import { StatusBar } from "./components/StatusBar";
import { CloneDialog } from "./components/CloneDialog";
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

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
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
    </div>
  );
}
