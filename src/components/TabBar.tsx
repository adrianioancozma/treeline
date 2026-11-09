import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { useApp } from "../store";
import { useUi } from "../uiStore";
import { openGuide } from "../guide";
import { Icon } from "./Icon";

export function TabBar() {
  const { tabs, activeId, setActive, closeTab, newTab, moveTab, data } = useApp();
  const bar = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);

  // The mouse wheel scrolls the tabs sideways. A native listener, because React's wheel handler is
  // passive and could not stop the page behind from scrolling.
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (el.scrollWidth <= el.clientWidth || Math.abs(e.deltaX) > Math.abs(e.deltaY)) return;
      e.preventDefault();
      el.scrollLeft += e.deltaY;
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  // The selected (or newly opened) tab is always brought into view.
  useEffect(() => {
    scroller.current?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView?.({ inline: "nearest", block: "nearest" });
  }, [activeId, tabs.length]);
  /** The tab being dragged and the slot (0 … tabs.length) it would drop into. */
  const [drag, setDrag] = useState<{ id: string; slot: number } | null>(null);

  // Pointer events instead of HTML5 drag-and-drop: the webview's file-drop handling can swallow the latter.
  const startDrag = (e: ReactPointerEvent, id: string) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest(".tab-close")) return;
    const startX = e.clientX;
    let moving = false;
    let slot = 0;
    const slotAt = (x: number) => {
      const els = Array.from(bar.current?.querySelectorAll<HTMLElement>(".tab") ?? []);
      const i = els.findIndex((el) => x < el.getBoundingClientRect().left + el.offsetWidth / 2);
      return i < 0 ? els.length : i;
    };
    const move = (ev: PointerEvent) => {
      if (!moving && Math.abs(ev.clientX - startX) < 5) return;
      moving = true;
      slot = slotAt(ev.clientX);
      setDrag({ id, slot });
    };
    const end = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      setDrag(null);
      if (!moving) return;
      const from = useApp.getState().tabs.findIndex((t) => t.id === id);
      moveTab(id, slot > from ? slot - 1 : slot);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
  };

  return (
    <div className="tabbar" role="tablist" ref={bar}>
      <div className="tabs" ref={scroller}>
      {tabs.map((t, i) => {
        const s = data[t.id]?.status;
        const dirty = (s?.files.length ?? 0) > 0;
        return (
          <div
            key={t.id}
            role="tab"
            aria-selected={t.id === activeId}
            className={`tab${t.id === activeId ? " active" : ""}${drag?.id === t.id ? " dragging" : ""}${
              drag?.slot === i ? " drop-before" : ""
            }${drag && drag.slot === tabs.length && i === tabs.length - 1 ? " drop-after" : ""}`}
            title={t.path ? `${t.name}
${t.path}` : "New tab"}
            onPointerDown={(e) => startDrag(e, t.id)}
            onClick={() => setActive(t.id)}
            onAuxClick={(e) => e.button === 1 && closeTab(t.id)}
          >
            {t.path && <Icon name="branch" size={13} />}
            <span className="tab-name">{t.name}</span>
            {dirty && <span className="dirty" title={`${s!.files.length} uncommitted change(s)`} />}
            <button
              className="icon-btn tab-close"
              aria-label={`Close ${t.name}`}
              onClick={(e) => {
                e.stopPropagation();
                closeTab(t.id);
              }}
            >
              <Icon name="x" size={13} />
            </button>
          </div>
        );
      })}
      </div>
      <button className="icon-btn tab-new" aria-label="New tab (Ctrl+T)" title="New tab (Ctrl+T)" onClick={newTab}>
        <Icon name="plus" size={16} />
      </button>
      <span className="grow" />
      <button
        className="icon-btn"
        aria-label="Open the user guide (F1)"
        title="User guide (F1)"
        onClick={() => void openGuide().catch((e) => useApp.getState().notify("error", String(e)))}
      >
        <Icon name="help" size={16} />
      </button>
      <button
        className="icon-btn"
        aria-label="Settings"
        title="Settings (Git program)"
        onClick={() => useUi.setState({ settings: true })}
      >
        <Icon name="gear" size={16} />
      </button>
      <button className="icon-btn" aria-label="Command palette (Ctrl+K)" title="Command palette (Ctrl+K)" onClick={() => useApp.getState().setPalette(true)}>
        <Icon name="command" size={16} />
      </button>
      <button
        className="icon-btn"
        aria-label="Toggle theme"
        title="Toggle light / dark theme"
        onClick={() => {
          const { theme, setTheme } = useApp.getState();
          const dark = theme === "dark" || (theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
          setTheme(dark ? "light" : "dark");
        }}
      >
        <Icon name="sun" size={16} />
      </button>
    </div>
  );
}
