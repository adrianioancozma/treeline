import { useApp } from "../store";
import { Icon } from "./Icon";

export function TabBar() {
  const { tabs, activeId, setActive, closeTab, newTab, data } = useApp();
  return (
    <div className="tabbar" role="tablist">
      {tabs.map((t) => {
        const s = data[t.id]?.status;
        const dirty = (s?.files.length ?? 0) > 0;
        return (
          <div
            key={t.id}
            role="tab"
            aria-selected={t.id === activeId}
            className={`tab${t.id === activeId ? " active" : ""}`}
            title={t.path ?? "New tab"}
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
      <button className="icon-btn tab-new" aria-label="New tab (Ctrl+T)" title="New tab (Ctrl+T)" onClick={newTab}>
        <Icon name="plus" size={16} />
      </button>
      <span className="grow" />
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
