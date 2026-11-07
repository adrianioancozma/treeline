import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { api, errorMessage } from "../api";
import { useApp } from "../store";
import { ErrorBox } from "./ErrorBox";
import type { OpProgress as CloneProgress } from "../types";

/** "https://host/group/my-repo.git" or "git@host:group/my-repo.git" → "my-repo" */
function repoNameFromUrl(url: string): string {
  const last = url.trim().replace(/[\\/]+$/, "").split(/[\\/:]/).pop() ?? "";
  return last.replace(/\.git$/i, "");
}

export function CloneDialog({ onClose }: { onClose: () => void }) {
  const openRepo = useApp((s) => s.openRepo);
  const [url, setUrl] = useState("");
  const [parent, setParent] = useState("");
  const [name, setName] = useState("");
  const [nameEdited, setNameEdited] = useState(false);
  const [recursive, setRecursive] = useState(true);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<CloneProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const jobId = useRef(crypto.randomUUID());

  useEffect(() => {
    const unlisten = listen<CloneProgress>("op-progress", (e) => {
      if (e.payload.id === jobId.current) setProgress(e.payload);
    });
    return () => void unlisten.then((f) => f());
  }, []);

  const onUrl = (v: string) => {
    setUrl(v);
    if (!nameEdited) setName(repoNameFromUrl(v));
  };

  const browse = async () => {
    const dir = await open({ directory: true, multiple: false, title: "Choose destination folder" });
    if (typeof dir === "string") setParent(dir);
  };

  const canClone = url.trim() && parent && name.trim() && !busy;

  const clone = async () => {
    setBusy(true);
    setError(null);
    try {
      const dest = `${parent.replace(/[\\/]+$/, "")}/${name.trim()}`;
      const out = await api.clone(jobId.current, url.trim(), dest, recursive);
      await openRepo(out.path);
      if (out.warning) useApp.getState().notify("error", out.warning);
      onClose();
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onMouseDown={() => !busy && onClose()}>
      <div
        className="modal"
        role="dialog"
        aria-label="Clone repository"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape" && !busy) onClose();
          if (e.key === "Enter" && canClone) void clone();
        }}
      >
        <h2>Clone repository</h2>
        <label>
          Repository URL
          <input
            autoFocus
            value={url}
            placeholder="https://… or git@…"
            onChange={(e) => onUrl(e.target.value)}
            disabled={busy}
          />
        </label>
        <label>
          Destination folder
          <div className="row">
            <input value={parent} onChange={(e) => setParent(e.target.value)} disabled={busy} />
            <button className="btn" onClick={browse} disabled={busy}>
              Browse…
            </button>
          </div>
        </label>
        <label>
          Folder name
          <input
            value={name}
            onChange={(e) => {
              setName(e.target.value);
              setNameEdited(true);
            }}
            disabled={busy}
          />
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={recursive}
            onChange={(e) => setRecursive(e.target.checked)}
            disabled={busy}
          />
          Clone submodules too
        </label>

        {busy && (
          <div className="progress" aria-live="polite">
            <div className="bar">
              <div className="bar-fill" style={{ width: `${progress?.percent ?? 5}%` }} />
            </div>
            <span className="muted small">{progress?.line ?? "Starting…"}</span>
          </div>
        )}
        {error && <ErrorBox text={error} compact />}

        <div className="modal-actions">
          <button className="btn" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button className="btn primary" onClick={clone} disabled={!canClone}>
            {busy ? "Cloning…" : "Clone"}
          </button>
        </div>
      </div>
    </div>
  );
}
