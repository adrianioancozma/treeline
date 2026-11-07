import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorMessage } from "../api";
import { fullDate, relativeTime } from "../lib/format";
import { useApp } from "../store";
import type { BranchHistory, MergedBranch } from "../types";
import { ErrorBox } from "./ErrorBox";
import { Icon } from "./Icon";

const HISTORY_LIMIT = 100;
const USUAL_TARGETS = ["main", "master", "develop"];

/**
 * Local branches that are already part of a target branch, so deleting them loses nothing, with the
 * history of each one. Ticked branches are deleted after a second click.
 */
export function MergedBranchesDialog({ tabId, onClose }: { tabId: string; onClose: () => void }) {
  const tab = useApp((s) => s.tabs.find((t) => t.id === tabId));
  const refs = useApp((s) => s.data[tabId]?.refs);
  const currentBranch = useApp((s) => s.data[tabId]?.status?.branch ?? null);
  const repo = tab?.path ?? "";

  const targets = useMemo(() => (refs ?? []).filter((r) => r.kind === "branch" || r.kind === "remote").map((r) => r.name), [refs]);
  const [target, setTarget] = useState(() => USUAL_TARGETS.find((t) => targets.includes(t)) ?? currentBranch ?? targets[0] ?? "");

  const [branches, setBranches] = useState<MergedBranch[] | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [focus, setFocus] = useState<string | null>(null);
  const [history, setHistory] = useState<BranchHistory | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const load = useCallback(async () => {
    setBranches(null);
    setError(null);
    try {
      const list = await api.mergedBranches(repo, target);
      setBranches(list);
      setChecked(new Set());
      setFocus((f) => (list.some((b) => b.name === f) ? f : (list[0]?.name ?? null)));
    } catch (e) {
      setBranches([]);
      setError(errorMessage(e));
    }
  }, [repo, target]);

  useEffect(() => {
    if (target) void load();
  }, [load, target]);

  useEffect(() => {
    setHistory(null);
    if (!focus) return;
    let live = true;
    api
      .branchHistory(repo, focus, target, HISTORY_LIMIT)
      .then((h) => live && setHistory(h))
      .catch((e) => live && setError(errorMessage(e)));
    return () => {
      live = false;
    };
  }, [repo, focus, target]);

  useEffect(() => setConfirming(false), [checked, target]);

  const deletable = (branches ?? []).filter((b) => !b.head);
  const toggle = (name: string) =>
    setChecked((c) => {
      const next = new Set(c);
      if (!next.delete(name)) next.add(name);
      return next;
    });
  const allChecked = deletable.length > 0 && deletable.every((b) => checked.has(b.name));

  const remove = async () => {
    setDeleting(true);
    try {
      const results = await api.deleteMergedBranches(repo, target, [...checked]);
      const failed = results.filter((r) => !r.ok);
      const done = results.length - failed.length;
      const { notify, reload } = useApp.getState();
      if (done > 0) notify("success", `Deleted ${done} merged branch${done === 1 ? "" : "es"}`);
      if (failed.length > 0) notify("error", failed.map((r) => `${r.name}: ${r.message}`).join("\n"));
      await reload(tabId);
      await load();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setDeleting(false);
      setConfirming(false);
    }
  };

  const focused = branches?.find((b) => b.name === focus);

  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <div
        className="modal wide"
        role="dialog"
        aria-label="Merged branches"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.key === "Escape" && onClose()}
      >
        <div className="fh-head">
          <h2>Merged branches</h2>
          <label className="mb-target">
            <span>Merged into</span>
            <select value={target} onChange={(e) => setTarget(e.target.value)} aria-label="Target branch">
              {targets.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </label>
          <button className="btn" onClick={onClose}>
            Close
          </button>
        </div>
        <p className="muted small">
          These branches are already part of <code>{target}</code>, so deleting them loses no commits. A branch that was squash-merged or rebased
          is not recognised, because its original commits never reached <code>{target}</code>; check those by hand.
        </p>
        {error && <ErrorBox text={error} compact />}

        <div className="fh-body">
          <div className="fh-list" role="listbox" aria-label="Merged branches" aria-multiselectable="true">
            {branches === null && <p className="muted small pad">Looking for merged branches…</p>}
            {branches?.length === 0 && !error && <p className="muted small pad">No branches are merged into {target} yet.</p>}
            {branches?.map((b) => (
              <div key={b.name} role="option" aria-selected={b.name === focus} className={`mb-row${b.name === focus ? " active" : ""}`}>
                <input
                  type="checkbox"
                  checked={checked.has(b.name)}
                  disabled={b.head}
                  onChange={() => toggle(b.name)}
                  aria-label={`Delete ${b.name}`}
                  title={b.head ? "Checked out, so it cannot be deleted" : `Delete ${b.name}`}
                />
                <button className="mb-name" onClick={() => setFocus(b.name)}>
                  <span className="subject ellipsis">
                    <Icon name="branch" size={13} /> {b.name}
                    {b.head && <span className="mb-badge">checked out</span>}
                    {b.atTarget && !b.head && (
                      <span className="mb-badge" title={`Its tip is the same commit as ${target}: nothing was committed on it`}>
                        no commits of its own
                      </span>
                    )}
                  </span>
                  <span className="muted small ellipsis">
                    {b.author} · <span title={fullDate(b.time)}>{relativeTime(b.time)}</span> · {b.subject}
                  </span>
                </button>
              </div>
            ))}
          </div>

          <div className="fh-list mb-history" aria-label="History of the selected branch">
            {focused && !history && <p className="muted small pad">Loading history…</p>}
            {!focused && <div className="empty">Select a branch to see its history.</div>}
            {focused && history && (
              <>
                <p className="small pad">
                  {history.merge ? (
                    <>
                      Merged into <code>{target}</code> by <code>{history.merge.sha.slice(0, 7)}</code> “{history.merge.subject}” ·{" "}
                      {history.merge.author}, <span title={fullDate(history.merge.time)}>{relativeTime(history.merge.time)}</span>.
                    </>
                  ) : focused.atTarget ? (
                    <>
                      <code>{focused.name}</code> points at the same commit as <code>{target}</code>. Below is the history leading up to it.
                    </>
                  ) : (
                    <>
                      Fast-forwarded into <code>{target}</code>, so there is no merge commit. Below are the latest commits on{" "}
                      <code>{focused.name}</code>.
                    </>
                  )}
                </p>
                {history.commits.map((c) => (
                  <div key={c.sha} className="fh-commit">
                    <span className="subject ellipsis" title={c.subject}>
                      {c.subject}
                    </span>
                    <span className="muted small ellipsis">
                      {c.author} · <span title={fullDate(c.time)}>{relativeTime(c.time)}</span> · <code>{c.sha.slice(0, 7)}</code>
                    </span>
                  </div>
                ))}
                {history.truncated && <p className="muted small pad">Showing the latest {HISTORY_LIMIT} commits.</p>}
              </>
            )}
          </div>
        </div>

        <div className="modal-actions mb-actions">
          <button
            className="btn"
            disabled={deletable.length === 0 || deleting}
            onClick={() => setChecked(allChecked ? new Set() : new Set(deletable.map((b) => b.name)))}
          >
            {allChecked ? "Select none" : "Select all"}
          </button>
          <span className="grow" />
          {confirming ? (
            <>
              <span className="small">
                Delete {checked.size} branch{checked.size === 1 ? "" : "es"}? Their commits stay in <code>{target}</code>.
              </span>
              <button className="btn" disabled={deleting} onClick={() => setConfirming(false)}>
                Cancel
              </button>
              <button className="btn danger" disabled={deleting} onClick={() => void remove()}>
                {deleting ? "Deleting…" : "Delete"}
              </button>
            </>
          ) : (
            <button className="btn danger-outline" disabled={checked.size === 0} onClick={() => setConfirming(true)}>
              Delete {checked.size > 0 ? checked.size : ""} selected…
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
