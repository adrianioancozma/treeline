import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, errorMessage } from "../api";
import type { GitInfo } from "../types";

/** App settings. For now: which Git program the app runs. */
export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const [info, setInfo] = useState<GitInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.gitInfo().then(setInfo, (e) => setError(errorMessage(e)));
  }, []);

  const apply = useCallback(async (path: string | null) => {
    setBusy(true);
    setError(null);
    try {
      setInfo(await api.setGitPath(path));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const browse = async () => {
    const file = await open({
      multiple: false,
      directory: false,
      title: "Locate the Git program (git.exe)",
      filters: [{ name: "Git", extensions: ["exe"] }, { name: "All files", extensions: ["*"] }],
    });
    if (typeof file === "string") await apply(file);
  };

  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-label="Settings"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.key === "Escape" && onClose()}
      >
        <h2>Settings</h2>
        <h3 className="small">Git program</h3>
        {info ? (
          <>
            <p className="small" role="status">
              {info.version ? `✓ ${info.version}` : "✗ Git could not be started."}
            </p>
            <p className="muted small">
              {info.custom ? "Using the program you chose:" : "Found automatically:"} <code>{info.path}</code>
            </p>
          </>
        ) : (
          <p className="muted small">Checking…</p>
        )}
        <div className="row">
          <button className="btn" onClick={() => void browse()} disabled={busy}>
            Change…
          </button>
          <button className="btn" onClick={() => void apply(null)} disabled={busy || !info?.custom}>
            Use automatic detection
          </button>
        </div>
        {error && (
          <p className="small" role="alert" style={{ color: "var(--danger, #e5534b)" }}>
            {error}
          </p>
        )}
        <div className="modal-actions">
          <button className="btn primary" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
