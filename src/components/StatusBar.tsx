import { useAppVersion } from "../hooks";
import { useApp } from "../store";

export function StatusBar() {
  const { tabs, activeId, data, progress } = useApp();
  const tab = tabs.find((t) => t.id === activeId);
  const d = activeId ? data[activeId] : undefined;
  const version = useAppVersion();
  const busy = d?.busy ?? (d?.loading ? "Refreshing…" : null);
  return (
    <footer className="statusbar">
      <span className="status-left">
        {busy ? (
          <>
            <span className="spinner" aria-hidden="true" /> {busy}
          </>
        ) : (
          <span className="muted">Ready</span>
        )}
        {busy && progress && progress.id === activeId && (
          <>
            <span className="bar mini">
              <span className="bar-fill" style={{ width: `${progress.percent ?? 8}%` }} />
            </span>
            <span className="muted ellipsis">{progress.line}</span>
          </>
        )}
      </span>
      <span className="muted ellipsis">{tab?.path ?? ""}</span>
      {version && (
        <span className="muted" title="Treeline version">
          v{version}
        </span>
      )}
    </footer>
  );
}
