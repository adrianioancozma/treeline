import { ErrorBox } from "./ErrorBox";
import { remoteErrorHost } from "../lib/explain";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, errorMessage } from "../api";
import { useApp } from "../store";
import { Modal } from "./Modal";
import { filterFolders, intoFolder, openTarget, splitInput, withTilde } from "../lib/remotePath";

type Listing = Awaited<ReturnType<typeof api.remoteList>>;

const lastDirKey = (host: string) => `remote-last-dir:${host}`;
function lastDir(host: string): string {
  try {
    return localStorage.getItem(lastDirKey(host)) ?? "~/";
  } catch {
    return "~/";
  }
}
function rememberDir(host: string, dir: string) {
  try {
    localStorage.setItem(lastDirKey(host), dir);
  } catch {
    /* storage unavailable: start from home next time */
  }
}

/**
 * Picks a repository on another computer, like VS Code's remote folder input: a computer from ~/.ssh/config, a path
 * field whose last part filters the folders listed below, ↑/↓ to choose, Enter to go in (or open a repository),
 * Backspace on an empty filter to go up. Opening still asks for confirmation in a native dialog.
 */
export function RemoteBrowser({ onClose }: { onClose: () => void }) {
  const { openRemote, notify, setBrowsingHost } = useApp();
  const [hosts, setHosts] = useState<string[]>([]);
  const [host, setHost] = useState("");
  const [input, setInput] = useState("~/");
  const [listing, setListing] = useState<Listing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [showHidden, setShowHidden] = useState(false);
  const [recent, setRecent] = useState<string[]>([]);
  const [retry, setRetry] = useState(0); // asks for the listing again after Reconnect
  const [sel, setSel] = useState(-1); // nothing selected until a click or ↓
  const pathRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    void api.sshHosts().then((h) => {
      setHosts(h);
      if (h.length > 0) setHost((cur) => cur || h[0]);
    }, () => undefined);
  }, []);

  // The connection to the computer being browsed is kept while the browser is on it (see `setBrowsingHost`).
  useEffect(() => {
    setBrowsingHost(host || null);
  }, [host, setBrowsingHost]);
  useEffect(() => () => setBrowsingHost(null), [setBrowsingHost]);

  // A new computer: start where the user was last time on it.
  useEffect(() => {
    if (!host) return;
    setInput(lastDir(host));
    setListing(null);
    void api.remoteRecent(host).then(setRecent, () => setRecent([]));
  }, [host]);

  const { dir, filter } = splitInput(input);
  useEffect(() => {
    if (!host) return;
    let stale = false;
    setLoading(true);
    const t = setTimeout(() => {
      api
        .remoteList(host, dir, showHidden)
        .then((l) => !stale && (setListing(l), setError(null)))
        .catch((e) => !stale && (setListing(null), setError(errorMessage(e))))
        .finally(() => !stale && setLoading(false));
    }, 150);
    return () => {
      stale = true;
      clearTimeout(t);
    };
  }, [host, dir, showHidden, retry]);

  const shown = useMemo(() => filterFolders(listing?.entries ?? [], filter), [listing, filter]);
  useEffect(() => setSel(-1), [dir, filter]);
  const here = listing ? withTilde(listing.path, listing.home) : dir;

  const open = async (path: string) => {
    setBusy(true);
    try {
      rememberDir(host, `${here}/`);
      await openRemote(host, path);
      onClose();
    } catch (e) {
      const m = errorMessage(e);
      if (m !== "Cancelled.") notify("error", m);
    } finally {
      setBusy(false);
    }
  };
  const choose = (i: number) => {
    const e = shown[i];
    if (!e || !listing) return;
    if (e.repo) void open(openTarget(listing, e)!);
    else setInput(intoFolder(here, e.name));
  };
  const up = () => {
    if (listing?.parent) setInput(`${withTilde(listing.parent, listing.home)}/`.replace(/^\/\/$/, "/"));
  };

  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") (e.preventDefault(), setSel((s) => Math.min(s + 1, shown.length - 1)));
    else if (e.key === "ArrowUp") (e.preventDefault(), setSel((s) => Math.max(s - 1, 0)));
    else if (e.key === "Enter") {
      e.preventDefault();
      // Without a selection: the first match of what was typed, else the listed folder if it is a repository.
      const i = sel >= 0 ? sel : filter && shown.length > 0 ? 0 : -1;
      if (i >= 0) choose(i);
      else if (listing?.repo) void open(listing.path);
    } else if (e.key === "Backspace" && filter === "" && input.endsWith("/") && dir !== "/" && dir !== "~") {
      e.preventDefault();
      up();
    }
  };

  // A click selects (hovering does not), and Open opens the selected folder.
  const target = openTarget(listing, shown[sel]);

  return (
    <Modal title="Open remote repository" onClose={onClose} busy={busy} className="remote-browser">
      <div className="field">
        <label htmlFor="rb-host">Computer</label>
        <input
          id="rb-host"
          list="rb-hosts"
          value={host}
          placeholder="linux-box.local or user@host"
          onChange={(e) => setHost(e.target.value.trim())}
          spellCheck={false}
        />
        <datalist id="rb-hosts">
          {hosts.map((h) => (
            <option key={h} value={h} />
          ))}
        </datalist>
      </div>
      <div className="field">
        <label htmlFor="rb-path">Folder {loading && <span className="muted small">· loading…</span>}</label>
        <input
          id="rb-path"
          ref={pathRef}
          className="mono"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={onKey}
          autoFocus
          spellCheck={false}
          aria-describedby="rb-help"
        />
        <p id="rb-help" className="muted small">
          Type to filter · ↑ ↓ to choose · Enter goes in (or opens a repository) · Backspace goes up
        </p>
      </div>
      <div className="rb-list" role="listbox" aria-label={`Folders in ${here}`}>
        {listing?.parent && (
          <div className="rb-row" role="option" aria-selected={false} onClick={up}>
            <span className="rb-name">..</span>
          </div>
        )}
        {shown.map((e, i) => (
          <div
            key={e.name}
            className={`rb-row${i === sel ? " sel" : ""}`}
            role="option"
            aria-selected={i === sel}
            onClick={() => setSel(i)}
            onDoubleClick={() => choose(i)}
          >
            <span className="rb-name">{e.name}</span>
            {e.repo && <span className="rb-repo">git repo</span>}
          </div>
        ))}
        {error && remoteErrorHost(error) ? (
          <ErrorBox text={error} compact onRetry={() => setRetry((n) => n + 1)} />
        ) : (
          error && <p className="rb-error">{error}</p>
        )}
        {!error && listing && shown.length === 0 && <p className="muted small pad">No folders{filter ? ` starting with “${filter}”` : ""}.</p>}
        {listing?.truncated && <p className="muted small pad">Only the first 2000 folders are shown; type to narrow them down.</p>}
      </div>
      {recent.length > 0 && (
        <p className="muted small rb-recent">
          Recent on {host}:{" "}
          {recent.slice(-3).map((r) => (
            <button key={r} className="link" onClick={() => void open(r)} disabled={busy}>
              {withTilde(r, listing?.home ?? null)}
            </button>
          ))}
        </p>
      )}
      <div className="modal-actions rb-actions">
        <label className="check">
          <input type="checkbox" checked={showHidden} onChange={(e) => setShowHidden(e.target.checked)} /> Show hidden folders
        </label>
        <span className="grow" />
        <button className="btn" onClick={onClose} disabled={busy}>
          Cancel
        </button>
        <button className="btn primary" onClick={() => target && void open(target)} disabled={!target || busy}>
          Open
        </button>
      </div>
    </Modal>
  );
}
