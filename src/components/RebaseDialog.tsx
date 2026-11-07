import { useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import { useApp } from "../store";
import { useUi } from "../uiStore";
import type { RebaseStep } from "../types";
import { Icon } from "./Icon";

type Action = RebaseStep["action"];
interface Row {
  sha: string;
  subject: string;
  action: Action;
  message: string;
  isMerge: boolean;
}

const ACTIONS: { id: Action; label: string }[] = [
  { id: "pick", label: "Pick" },
  { id: "reword", label: "Reword" },
  { id: "edit", label: "Edit" },
  { id: "squash", label: "Squash" },
  { id: "fixup", label: "Fixup" },
  { id: "drop", label: "Drop" },
];

export function RebaseDialogHost() {
  const req = useUi((s) => s.rebase);
  return req ? <RebaseDialog key={req.tabId + req.base} {...req} /> : null;
}

/** Rows are shown newest first (like the history); git's todo list is oldest first. */
function RebaseDialog({ tabId, base, title }: { tabId: string; base: string | null; title: string }) {
  const tab = useApp((s) => s.tabs.find((t) => t.id === tabId));
  const [rows, setRows] = useState<Row[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const close = () => useUi.setState({ rebase: null });

  useEffect(() => {
    if (!tab?.path) return;
    api
      .rebaseTodo(tab.path, base)
      .then((cs) =>
        setRows(
          cs
            .map((c) => ({ sha: c.sha, subject: c.subject, action: "pick" as Action, message: c.message, isMerge: c.is_merge }))
            .reverse(),
        ),
      )
      .catch((e) => setError(errorMessage(e)));
  }, [tab?.path, base]);

  const update = (i: number, patch: Partial<Row>) => setRows((r) => r!.map((x, k) => (k === i ? { ...x, ...patch } : x)));
  const move = (i: number, dir: -1 | 1) =>
    setRows((r) => {
      const j = i + dir;
      if (!r || j < 0 || j >= r.length) return r;
      const n = [...r];
      [n[i], n[j]] = [n[j], n[i]];
      return n;
    });

  const ordered = rows ? [...rows].reverse() : [];
  // The oldest kept commit cannot be squashed/fixed up into nothing.
  const firstKept = ordered.find((r) => r.action !== "drop");
  const invalid = !!firstKept && (firstKept.action === "squash" || firstKept.action === "fixup");

  const start = async () => {
    if (!tab?.path || !rows) return;
    close();
    const steps: RebaseStep[] = ordered.map((r) => ({
      action: r.action,
      sha: r.sha,
      message: r.action === "reword" ? r.message : undefined,
    }));
    await useApp.getState().run(tabId, "Rebasing…", () => api.rebaseInteractive(tab.path!, base, steps), { success: "Rebase complete" });
  };

  return (
    <div className="modal-backdrop" onMouseDown={close}>
      <div className="modal wide" role="dialog" aria-label={title} onMouseDown={(e) => e.stopPropagation()} onKeyDown={(e) => e.key === "Escape" && close()}>
        <h2>{title}</h2>
        <p className="muted small">
          Merge commits are not listed (a rebase flattens them). Reorder with the arrows and choose what happens to each commit. Newest commit is at the top. This rewrites history —
          avoid it for commits you have already pushed and shared.
        </p>
        {error && <p className="error">{error}</p>}
        {!rows && !error && <p className="muted">Loading commits…</p>}
        {rows && rows.length === 0 && <p className="muted">There are no commits to rebase.</p>}
        <ul className="rebase-list">
          {rows?.map((r, i) => (
            <li key={r.sha} className={r.action === "drop" ? "dropped" : ""}>
              <span className="move">
                <button className="icon-btn" aria-label="Move up" disabled={i === 0} onClick={() => move(i, -1)}>
                  <Icon name="chevron" size={12} />
                </button>
                <button className="icon-btn down" aria-label="Move down" disabled={i === rows.length - 1} onClick={() => move(i, 1)}>
                  <Icon name="chevron" size={12} />
                </button>
              </span>
              <select value={r.action} onChange={(e) => update(i, { action: e.target.value as Action })} aria-label={`Action for ${r.subject}`}>
                {ACTIONS.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.label}
                  </option>
                ))}
              </select>
              <span className="sha">{r.sha.slice(0, 7)}</span>
              {r.action === "reword" ? (
                <textarea rows={3} value={r.message} onChange={(e) => update(i, { message: e.target.value })} aria-label="New message" />
              ) : (
                <span className="ellipsis grow">{r.subject}</span>
              )}
            </li>
          ))}
        </ul>
        {invalid && <p className="error small">The first commit cannot be squashed or fixed up — there is nothing before it to combine with.</p>}
        <div className="modal-actions">
          <button className="btn" onClick={close}>
            Cancel
          </button>
          <button className="btn primary" disabled={!rows || rows.length === 0 || invalid} onClick={() => void start()}>
            Start rebase
          </button>
        </div>
      </div>
    </div>
  );
}
