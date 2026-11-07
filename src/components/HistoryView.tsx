import { useEffect, useMemo, useRef, useState } from "react";
import { api, errorMessage } from "../api";
import { repoActions } from "../actions";
import { useDebounced, useSplit } from "../hooks";
import { computeGraph, reachableFrom, type GraphRow } from "../lib/graph";
import { classifyRef, fullDate, relativeTime, splitPath } from "../lib/format";
import { useApp } from "../store";
import { openMenu, useUi } from "../uiStore";
import type { Commit, CommitDetail, FileEntry, Tab } from "../types";
import { DiffView } from "./DiffView";
import { Icon } from "./Icon";

const ROW_H = 28;
const LANE_W = 14;
const PALETTE = ["#4c8dff", "#e5a23c", "#4cc38a", "#c678dd", "#ff6b6b", "#2bb5c8", "#d9822b", "#8fbf3f"];
const WIP = "WIP";

function GraphCell({ row, width }: { row: GraphRow; width: number }) {
  const x = (lane: number) => lane * LANE_W + LANE_W / 2 + 2;
  const y = (v: number) => v * ROW_H;
  const color = (c: number) => PALETTE[c % PALETTE.length];
  return (
    <svg width={width} height={ROW_H} className="graph" aria-hidden="true">
      {row.segs.map((s, i) => {
        const straight = s.x1 === s.x2;
        const d = straight
          ? `M${x(s.x1)} ${y(s.y1)}V${y(s.y2)}`
          : `M${x(s.x1)} ${y(s.y1)}C${x(s.x1)} ${y((s.y1 + s.y2) / 2)} ${x(s.x2)} ${y((s.y1 + s.y2) / 2)} ${x(s.x2)} ${y(s.y2)}`;
        return <path key={i} d={d} stroke={color(s.color)} strokeWidth="2" fill="none" />;
      })}
      <circle cx={x(row.lane)} cy={ROW_H / 2} r="4.5" fill={color(row.color)} stroke="var(--bg)" strokeWidth="1.5" />
    </svg>
  );
}

export function HistoryView({ tab }: { tab: Tab }) {
  const d = useApp((s) => s.data[tab.id])!;
  const { setUi, loadLog } = useApp.getState();
  const a = repoActions(tab.id);
  const status = d.status;
  const remotes = d.remotes;
  const [height, onSplit] = useSplit("history-top", Math.round(window.innerHeight * 0.42), 140, 800, "y");
  const [q, setQ] = useState(d.ui.query);
  const dq = useDebounced(q, 350);
  const scroller = useRef<HTMLDivElement>(null);
  const [scroll, setScroll] = useState(0);
  const [viewH, setViewH] = useState(400);

  useEffect(() => {
    if (dq !== d.ui.query) setUi(tab.id, { query: dq });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dq]);

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewH(el.clientHeight));
    ro.observe(el);
    setViewH(el.clientHeight);
    return () => ro.disconnect();
  }, []);

  // Uncommitted changes appear as a pseudo-commit on top of HEAD, like Sourcetree.
  const wipCount = status?.files.length ?? 0;
  const showWip = wipCount > 0 && !!status?.head && !d.ui.query;
  const rowsData: Commit[] = useMemo(() => {
    if (!showWip) return d.log;
    const wip: Commit = { sha: WIP, parents: [status!.head!], author: "", email: "", time: 0, refs: [], subject: "" };
    return [wip, ...d.log];
  }, [d.log, showWip, status]);

  const graph = useMemo(() => computeGraph(rowsData), [rowsData]);

  // Highlight one branch/tag: everything reachable from its tip stays bright, the rest is dimmed.
  const focusRef = d.ui.focusRef;
  const focusTip = focusRef ? d.refs.find((r) => r.name === focusRef)?.sha : undefined;
  const highlight = useMemo(
    () => (focusTip ? reachableFrom(d.log, focusTip) : null),
    [focusTip, d.log],
  );
  const focusIndex = focusTip ? rowsData.findIndex((c) => c.sha === focusTip) : -1;
  // Scroll the tip into view when the focus changes (not on every refresh).
  useEffect(() => {
    const el = scroller.current;
    if (el && focusIndex >= 0) el.scrollTop = Math.max(0, focusIndex * ROW_H - ROW_H * 2);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusRef, d.logLoaded]);
  const maxLanes = Math.min(12, Math.max(1, ...graph.map((r) => r.lanes)));
  const graphW = maxLanes * LANE_W + 8;

  const first = Math.max(0, Math.floor(scroll / ROW_H) - 6);
  const last = Math.min(rowsData.length, Math.ceil((scroll + viewH) / ROW_H) + 6);

  useEffect(() => {
    if (last > rowsData.length - 80 && !d.logDone && !d.logLoading && d.logLoaded) void loadLog(tab.id, false);
  }, [last, rowsData.length, d.logDone, d.logLoading, d.logLoaded, loadLog, tab.id]);

  const selected = d.ui.selCommit;
  const compare = d.ui.compareCommit;
  // The older of the two commits is the base, so the difference always reads old → new.
  const pair = useMemo(() => {
    if (!selected || !compare) return null;
    const a = rowsData.findIndex((c) => c.sha === selected);
    const b = rowsData.findIndex((c) => c.sha === compare);
    return a >= 0 && b >= 0 && a < b ? { base: compare, target: selected } : { base: selected, target: compare };
  }, [selected, compare, rowsData]);
  const commitBySha = useMemo(() => new Map(d.log.map((c) => [c.sha, c])), [d.log]);

  const commitMenu = (e: React.MouseEvent, c: Commit) => {
    const isHead = status?.head === c.sha;
    openMenu(e, [
      { label: "Copy SHA", onClick: () => a.copy(c.sha) },
      { separator: true },
      { label: "Check out this commit", onClick: () => a.checkout(c.sha), disabled: isHead },
      { label: "New branch here…", onClick: () => a.newBranch(c.sha) },
      { label: "New tag here…", onClick: () => a.newTag(c.sha) },
      { separator: true },
      { label: "Cherry-pick", onClick: () => a.cherryPick(c) },
      { label: "Revert commit", onClick: () => a.revert(c) },
      { label: "Merge into current branch…", onClick: () => a.merge(c.sha) },
      { separator: true },
      { label: "Reset current branch to here — soft", onClick: () => a.reset(c.sha, "soft") },
      { label: "Reset current branch to here — mixed", onClick: () => a.reset(c.sha, "mixed") },
      { label: "Reset current branch to here — hard", danger: true, onClick: () => a.reset(c.sha, "hard") },
      { separator: true },
      { label: "Interactive rebase from here…", onClick: () => a.interactiveRebase(c) },
    ]);
  };

  return (
    <div className="history" style={{ gridTemplateRows: `${height}px 6px 1fr` }}>
      <div className="history-top">
        <div className="history-bar">
          <div className="filter-row grow">
            <Icon name="search" size={14} />
            <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search commit messages…" aria-label="Search commits" />
          </div>
          <label className="check inline">
            <input type="checkbox" checked={d.ui.historyAll} onChange={(e) => setUi(tab.id, { historyAll: e.target.checked })} />
            All branches
          </label>
          {focusRef && (
            <span className="chip focus-chip" title="Commits reachable from this ref are highlighted">
              Highlighting <strong>{focusRef}</strong>
              {focusTip && !highlight?.has(focusTip) ? " (not in loaded history)" : ""}
              <button className="icon-btn" aria-label="Clear highlight" onClick={() => setUi(tab.id, { focusRef: null })}>
                <Icon name="x" size={12} />
              </button>
            </span>
          )}
          <span className="muted small">{d.log.length}{d.logDone ? "" : "+"} commits</span>
        </div>
        <div className="history-head" style={{ gridTemplateColumns: `${graphW}px 1fr 150px 110px 64px` }}>
          <span>Graph</span>
          <span>Description</span>
          <span>Author</span>
          <span>Date</span>
          <span>Commit</span>
        </div>
        <div className="vlist" ref={scroller} onScroll={(e) => setScroll(e.currentTarget.scrollTop)}>
          {d.logLoaded && rowsData.length === 0 && <div className="empty">No commits{d.ui.query ? " match your search" : " yet"}.</div>}
          {!d.logLoaded && <div className="empty">Loading history…</div>}
          <div style={{ height: rowsData.length * ROW_H, position: "relative" }}>
            {rowsData.slice(first, last).map((c, k) => {
              const i = first + k;
              const isWip = c.sha === WIP;
              const refs = c.refs.map((r) => classifyRef(r, remotes)).filter((r): r is NonNullable<typeof r> => !!r);
              return (
                <div
                  key={c.sha}
                  className={`crow${selected === c.sha ? " active" : ""}${compare === c.sha ? " compare" : ""}${isWip ? " wip" : ""}${highlight && !(isWip ? focusRef === status?.branch : highlight.has(c.sha)) ? " dim" : ""}`}
                  style={{ top: i * ROW_H, height: ROW_H, gridTemplateColumns: `${graphW}px 1fr 150px 110px 64px` }}
                  onClick={(e) => {
                    if (isWip) return setUi(tab.id, { view: "files" });
                    // Ctrl+click a second commit to compare it with the selected one.
                    if ((e.ctrlKey || e.metaKey) && selected && !selected.startsWith("stash@") && selected !== c.sha)
                      return setUi(tab.id, { compareCommit: compare === c.sha ? null : c.sha });
                    setUi(tab.id, { selCommit: c.sha, compareCommit: null });
                  }}
                  onContextMenu={(e) => !isWip && commitMenu(e, c)}
                >
                  <GraphCell row={graph[i]} width={graphW} />
                  <span className="subject">
                    {refs.map((r) => (
                      <span key={r.label + r.kind} className={`ref ${r.kind}`}>
                        {r.label}
                      </span>
                    ))}
                    {isWip ? <em>Uncommitted changes ({wipCount})</em> : c.subject}
                  </span>
                  <span className="muted ellipsis">{c.author}</span>
                  <span className="muted ellipsis" title={isWip ? "" : fullDate(c.time)}>
                    {isWip ? "" : relativeTime(c.time)}
                  </span>
                  <span className="sha">{isWip ? "" : c.sha.slice(0, 7)}</span>
                </div>
              );
            })}
          </div>
        </div>
      </div>
      <div className="splitter h" onMouseDown={onSplit} role="separator" aria-orientation="horizontal" />
      {pair ? (
        <CommitDetailPane
          key={`${pair.base}..${pair.target}`}
          tab={tab}
          sha={pair.target}
          commit={commitBySha.get(pair.target)}
          base={pair.base}
          baseCommit={commitBySha.get(pair.base)}
          onClear={() => setUi(tab.id, { compareCommit: null })}
        />
      ) : selected ? (
        <CommitDetailPane key={selected} tab={tab} sha={selected} commit={commitBySha.get(selected)} />
      ) : (
        <div className="empty">
          <Icon name="history" size={28} />
          <p>Select a commit to see its changes.</p>
        </div>
      )}
    </div>
  );
}

function CommitDetailPane({
  tab,
  sha,
  commit,
  base,
  baseCommit,
  onClear,
}: {
  tab: Tab;
  sha: string;
  commit?: Commit;
  /** When set, show the difference between `base` and `sha` instead of what `sha` changed. */
  base?: string;
  baseCommit?: Commit;
  onClear?: () => void;
}) {
  const path = tab.path!;
  const notify = useApp.getState().notify;
  const a = repoActions(tab.id);
  const [detail, setDetail] = useState<CommitDetail | null>(null);
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [file, setFile] = useState<FileEntry | null>(null);
  const [diff, setDiff] = useState("");
  const [width, onSplit] = useSplit("detail-left", 380, 240, 700, "x");
  const parent = base ?? detail?.parents[0] ?? null;

  useEffect(() => {
    let live = true;
    (async () => {
      try {
        const det = await api.commitDetail(path, sha);
        if (!live) return;
        setDetail(det);
        const fl = await api.commitFiles(path, sha, base ?? det.parents[0] ?? null);
        if (!live) return;
        setFiles(fl);
        setFile(fl[0] ?? null);
      } catch (e) {
        if (live) notify("error", errorMessage(e));
      }
    })();
    return () => {
      live = false;
    };
  }, [path, sha, base, notify]);

  useEffect(() => {
    if (!file || !detail) {
      setDiff("");
      return;
    }
    let live = true;
    api
      .commitDiff(path, sha, parent, file.path, file.old_path, 3)
      .then((t) => live && setDiff(t))
      .catch((e) => live && notify("error", errorMessage(e)));
    return () => {
      live = false;
    };
  }, [path, sha, parent, file, detail, notify]);

  const [title, ...bodyLines] = (detail?.message ?? commit?.subject ?? "").split("\n");
  return (
    <div className="detail" style={{ gridTemplateColumns: `${width}px 6px 1fr` }}>
      <div className="detail-left">
        <div className="detail-meta">
          {base && (
            <div className="compare-bar">
              <strong>Comparing</strong> <code>{base.slice(0, 7)}</code> → <code>{sha.slice(0, 7)}</code>
              <button className="btn tiny" onClick={onClear} title="Back to the single commit">
                Clear
              </button>
              <p className="muted small ellipsis" title={baseCommit?.subject}>
                from: {baseCommit?.subject ?? base.slice(0, 7)}
              </p>
            </div>
          )}
          <h3 className="commit-title">{base ? `to: ${title}` : title}</h3>
          {bodyLines.join("\n").trim() && <pre className="commit-body">{bodyLines.join("\n").trim()}</pre>}
          {detail && (
            <p className="commit-line small">
              <strong>{detail.author}</strong>
              <span className="muted"> · {fullDate(detail.time)} · </span>
              <button className="link" onClick={() => a.copy(sha)} title="Copy full SHA">
                {sha.slice(0, 7)} <Icon name="copy" size={12} />
              </button>
              {detail.parents.length > 0 && (
                <span className="muted">
                  {" "}· {detail.parents.length > 1 ? "parents" : "parent"} {detail.parents.map((p) => p.slice(0, 7)).join(", ")}
                </span>
              )}
            </p>
          )}
        </div>
        <h3 className="list-title">
          Changed files <span className="count">{files.length}</span>
        </h3>
        <ul className="file-lists">
          {files.map((f) => {
            const { dir, name } = splitPath(f.path);
            const kind = { A: "added", D: "deleted", R: "renamed", M: "modified" }[f.status] ?? "modified";
            return (
              <li
                key={f.path}
                className={`file-row${file?.path === f.path ? " active" : ""}`}
                onClick={() => setFile(f)}
                onContextMenu={(e) =>
                  openMenu(e, [
                    { label: "File history…", onClick: () => useUi.setState({ fileHistory: { repo: path, file: f.path, mode: "history" } }) },
                    {
                      label: "Blame at this commit…",
                      disabled: f.status === "D",
                      onClick: () => useUi.setState({ fileHistory: { repo: path, file: f.path, mode: "blame", rev: sha } }),
                    },
                  ])
                }
                title={f.old_path ? `${f.old_path} → ${f.path}` : f.path}
              >
                <span className={`badge ${kind}`}>{f.status}</span>
                <span className="file-path">
                  <span className="name">{name}</span>
                  <span className="dir">{dir}</span>
                </span>
              </li>
            );
          })}
        </ul>
      </div>
      <div className="splitter" onMouseDown={onSplit} role="separator" aria-orientation="vertical" />
      <div className="detail-right">
        {file ? <DiffView text={diff} fileLabel={file.path} /> : <div className="empty">No file changes in this commit.</div>}
      </div>
    </div>
  );
}
