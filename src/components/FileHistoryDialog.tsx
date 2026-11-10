import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorMessage } from "../api";
import { fullDate, relativeTime } from "../lib/format";
import type { FileHistoryRequest } from "../uiStore";
import type { BlameLine, FileCommit } from "../types";
import { DiffView } from "./DiffView";
import { ErrorBox } from "./ErrorBox";
import { Modal } from "./Modal";

const PAGE = 200;
const UNCOMMITTED = /^0+$/;

/** History of one file ("Log Selected…") and who last changed each line ("Blame"). */
export function FileHistoryDialog({ req, onClose }: { req: FileHistoryRequest; onClose: () => void }) {
  const { repo, file } = req;
  const [mode, setMode] = useState(req.mode);
  const [commits, setCommits] = useState<FileCommit[]>([]);
  const [done, setDone] = useState(false);
  const [loading, setLoading] = useState(true);
  const [sel, setSel] = useState<string | null>(null);
  const [diff, setDiff] = useState("");
  const [blame, setBlame] = useState<BlameLine[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadMore = useCallback(
    async (skip: number) => {
      setLoading(true);
      try {
        const page = await api.fileLog(repo, file, skip, PAGE);
        setCommits((c) => (skip === 0 ? page : [...c, ...page]));
        setDone(page.length < PAGE);
        if (skip === 0 && page[0]) setSel(page[0].sha);
      } catch (e) {
        setError(errorMessage(e));
        setDone(true);
      } finally {
        setLoading(false);
      }
    },
    [repo, file],
  );

  useEffect(() => {
    void loadMore(0);
  }, [loadMore]);

  useEffect(() => {
    if (mode !== "blame" || blame) return;
    let live = true;
    api
      .blame(repo, file, req.rev ?? null)
      .then((b) => live && setBlame(b))
      .catch((e) => live && setError(errorMessage(e)));
    return () => {
      live = false;
    };
  }, [mode, blame, repo, file, req.rev]);

  const index = commits.findIndex((c) => c.sha === sel);
  useEffect(() => {
    const c = commits[index];
    if (!c) {
      setDiff("");
      return;
    }
    // If the file had another name before this commit (a rename), diff against that name too.
    const older = commits[index + 1];
    const oldFile = older && older.file !== c.file ? older.file : null;
    let live = true;
    api
      .commitDiff(repo, c.sha, c.parents[0] ?? null, c.file, oldFile, 3)
      .then((t) => live && setDiff(t))
      .catch((e) => live && setError(errorMessage(e)));
    return () => {
      live = false;
    };
  }, [repo, commits, index]);

  const known = useMemo(() => new Set(commits.map((c) => c.sha)), [commits]);
  const openFromBlame = (sha: string) => {
    if (!known.has(sha)) return;
    setSel(sha);
    setMode("history");
  };

  return (
    <Modal
      title={<span title={file}>{file}</span>}
      label={`History of ${file}`}
      className="wide"
      onClose={onClose}
      headerExtra={
        <div className="seg" role="tablist">
          <button role="tab" aria-selected={mode === "history"} className={mode === "history" ? "active" : ""} onClick={() => setMode("history")}>
            History
          </button>
          <button role="tab" aria-selected={mode === "blame"} className={mode === "blame" ? "active" : ""} onClick={() => setMode("blame")}>
            Blame
          </button>
        </div>
      }
    >
        {error && <ErrorBox text={error} compact />}

        {mode === "history" ? (
          <div className="fh-body">
            <div className="fh-list" role="listbox" aria-label="Commits that changed this file">
              {commits.map((c, i) => {
                const older = commits[i + 1];
                return (
                  <button key={c.sha} role="option" aria-selected={c.sha === sel} className={`fh-commit${c.sha === sel ? " active" : ""}`} onClick={() => setSel(c.sha)}>
                    <span className="subject ellipsis">{c.subject}</span>
                    <span className="muted small ellipsis">
                      {c.author} · <span title={fullDate(c.time)}>{relativeTime(c.time)}</span> · <code>{c.sha.slice(0, 7)}</code>
                      {older && older.file !== c.file ? ` · renamed from ${older.file}` : ""}
                    </span>
                  </button>
                );
              })}
              {loading && <p className="muted small pad">Loading…</p>}
              {!loading && commits.length === 0 && !error && <p className="muted small pad">No commits have changed this file yet.</p>}
              {!loading && !done && (
                <button className="btn tiny" onClick={() => void loadMore(commits.length)}>
                  Load more
                </button>
              )}
            </div>
            <div className="fh-diff">{sel ? <DiffView text={diff} fileLabel={file} /> : <div className="empty">Select a commit.</div>}</div>
          </div>
        ) : (
          <div className="blame" role="table" aria-label="Who last changed each line">
            {!blame && !error && <p className="muted small pad">Working out who changed each line…</p>}
            {blame?.map((b, i) => {
              const same = i > 0 && blame[i - 1].sha === b.sha;
              const uncommitted = UNCOMMITTED.test(b.sha);
              return (
                <div key={b.line} role="row" className={`blame-row${same ? " same" : ""}`}>
                  {same ? (
                    <span className="blame-who" aria-hidden="true" />
                  ) : (
                    <button
                      className="blame-who"
                      disabled={uncommitted}
                      title={uncommitted ? "Not committed yet" : `${b.summary}\n${fullDate(b.time)}\n${b.sha}`}
                      onClick={() => openFromBlame(b.sha)}
                    >
                      {uncommitted ? "Not committed yet" : `${b.author} · ${relativeTime(b.time)} · ${b.sha.slice(0, 7)}`}
                    </button>
                  )}
                  <span className="blame-no">{b.line}</span>
                  <code className="blame-text">{b.text || " "}</code>
                </div>
              );
            })}
          </div>
        )}
        <div className="modal-actions">
          <button className="btn" onClick={onClose}>
            Close
          </button>
        </div>
    </Modal>
  );
}
