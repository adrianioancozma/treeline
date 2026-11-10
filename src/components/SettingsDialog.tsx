import { useCallback, useEffect, useState } from "react";
import { useUi } from "../uiStore";
import { open } from "@tauri-apps/plugin-dialog";
import { api, errorMessage } from "../api";
import type { Update } from "@tauri-apps/plugin-updater";
import { useAppVersion } from "../hooks";
import { checkForUpdate, installUpdate } from "../updater";
import type { GitInfo } from "../types";
import { FETCH_SECOND_CHOICES, fetchSecondsLabel, loadFetchSeconds, saveFetchSeconds } from "../lib/autoFetch";

/** App settings. For now: which Git program the app runs. */
export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const version = useAppVersion();
  const [info, setInfo] = useState<GitInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  type UpdateState =
    | { kind: "idle" }
    | { kind: "checking" }
    | { kind: "latest" }
    | { kind: "available"; update: Update }
    | { kind: "installing"; percent: number | null }
    | { kind: "failed"; message: string };
  const [fetchSeconds, setFetchSeconds] = useState(loadFetchSeconds);
  const found = useUi((s) => s.update);
  const [upd, setUpd] = useState<UpdateState>(found ? { kind: "available", update: found } : { kind: "idle" });

  const checkUpdates = async () => {
    setUpd({ kind: "checking" });
    try {
      const update = await checkForUpdate();
      if (update) useUi.setState({ update });
      setUpd(update ? { kind: "available", update } : { kind: "latest" });
    } catch (e) {
      setUpd({ kind: "failed", message: errorMessage(e) });
    }
  };

  const install = async (update: Update) => {
    setUpd({ kind: "installing", percent: null });
    try {
      await installUpdate(update, (percent) => setUpd({ kind: "installing", percent }));
    } catch (e) {
      setUpd({ kind: "failed", message: errorMessage(e) });
    }
  };

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
        <h3 className="small">Background fetch</h3>
        <p className="muted small">
          Checks the server for new commits and branches in the tab you are looking at, so merges done on GitHub or GitLab show up by themselves.
          It only downloads; it never changes your files.
        </p>
        <label className="small">
          Check the server{" "}
          <select
            value={fetchSeconds}
            aria-label="How often to fetch in the background"
            onChange={(e) => {
              const n = Number(e.target.value);
              setFetchSeconds(n);
              saveFetchSeconds(n);
            }}
          >
            {FETCH_SECOND_CHOICES.map((sec) => (
              <option key={sec} value={sec}>
                {fetchSecondsLabel(sec)}
              </option>
            ))}
          </select>
        </label>
        {fetchSeconds > 0 && fetchSeconds < 60 && (
          <p className="muted small">Very short intervals use more network and battery, and some servers limit how often you may contact them.</p>
        )}
        <h3 className="small">About and updates</h3>
        <p className="muted small">Treeline{version ? ` version ${version}` : ""}</p>
        {upd.kind === "available" ? (
          <>
            <p className="small" role="status">
              Version {upd.update.version} is available.
            </p>
            {upd.update.body && <p className="muted small" style={{ whiteSpace: "pre-wrap" }}>{upd.update.body}</p>}
            <p className="muted small">Installing closes the app and starts the new version.</p>
            <div className="row">
              <button className="btn primary" onClick={() => void install(upd.update)}>
                Install and restart
              </button>
              <button className="btn" onClick={() => setUpd({ kind: "idle" })}>
                Not now
              </button>
            </div>
          </>
        ) : upd.kind === "installing" ? (
          <div className="progress" aria-live="polite">
            <div className="bar">
              <div className="bar-fill" style={{ width: `${upd.percent ?? 5}%` }} />
            </div>
            <span className="muted small">{upd.percent === null ? "Downloading…" : `Downloading… ${upd.percent}%`}</span>
          </div>
        ) : (
          <>
            <div className="row">
              <button className="btn" onClick={() => void checkUpdates()} disabled={upd.kind === "checking"}>
                {upd.kind === "checking" ? "Checking…" : "Check for updates"}
              </button>
            </div>
            {upd.kind === "latest" && (
              <p className="small" role="status">
                ✓ You have the latest version.
              </p>
            )}
            {upd.kind === "failed" && (
              <p className="small" role="alert" style={{ color: "var(--danger, #e5534b)" }}>
                Could not check for updates: {upd.message}
              </p>
            )}
          </>
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
