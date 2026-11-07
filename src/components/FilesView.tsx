import { useEffect, useRef, useState } from "react";
import { api, errorMessage } from "../api";
import { repoActions } from "../actions";
import { useSplit } from "../hooks";
import { splitPath } from "../lib/format";
import { useApp } from "../store";
import type { FileSel } from "../store";
import { openMenu, useUi } from "../uiStore";
import type { FileChange, Tab } from "../types";
import { ConflictResolver } from "./ConflictResolver";
import { DiffView, type PatchKind } from "./DiffView";
import { Icon } from "./Icon";

function badge(f: FileChange, staged: boolean): { code: string; kind: string; title: string } {
  if (f.conflicted) return { code: "!", kind: "conflict", title: "Conflict" };
  if (f.untracked) return { code: "U", kind: "added", title: "Untracked" };
  const code = staged ? f.index : f.worktree;
  const map: Record<string, [string, string]> = {
    M: ["modified", "Modified"],
    A: ["added", "Added"],
    D: ["deleted", "Deleted"],
    R: ["renamed", "Renamed"],
    C: ["added", "Copied"],
    T: ["modified", "Type changed"],
  };
  const [kind, title] = map[code] ?? ["modified", "Changed"];
  return { code, kind, title };
}

const CONFLICT_LABELS: Record<string, string> = {
  UU: "both modified",
  AA: "both added",
  DD: "both deleted",
  AU: "added by us",
  UA: "added by them",
  DU: "deleted by us",
  UD: "deleted by them",
};

export function FilesView({ tab }: { tab: Tab }) {
  const d = useApp((s) => s.data[tab.id]);
  const { run, setUi, notify } = useApp.getState();
  const a = repoActions(tab.id);
  const path = tab.path!;
  const status = d?.status;
  const sel = d?.ui.sel ?? null;
  const [width, onSplit] = useSplit("files-left", 340, 240, 620, "x");
  const [diffText, setDiffText] = useState("");
  const [diffLoading, setDiffLoading] = useState(false);
  const [filter, setFilter] = useState("");

  const files = status?.files ?? [];
  const match = (f: FileChange) => !filter || f.path.toLowerCase().includes(filter.toLowerCase());
  const conflicts = files.filter((f) => f.conflicted);
  const staged = files.filter((f) => !f.untracked && !f.conflicted && f.index !== ".").filter(match);
  const unstaged = files.filter((f) => !f.conflicted && (f.untracked || f.worktree !== ".")).filter(match);

  // Keep the selection valid, or fall back to the first sensible file.
  useEffect(() => {
    if (!status) return;
    const exists = (s: FileSel) =>
      s.kind === "conflict"
        ? conflicts.some((f) => f.path === s.path)
        : s.kind === "staged"
          ? staged.some((f) => f.path === s.path)
          : unstaged.some((f) => f.path === s.path);
    if (sel && exists(sel)) return;
    const first: FileSel | null = conflicts[0]
      ? { path: conflicts[0].path, kind: "conflict", untracked: false }
      : unstaged[0]
        ? { path: unstaged[0].path, kind: "unstaged", untracked: unstaged[0].untracked }
        : staged[0]
          ? { path: staged[0].path, kind: "staged", untracked: false }
          : null;
    if (JSON.stringify(first) !== JSON.stringify(sel)) setUi(tab.id, { sel: first });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status]);

  // Load the diff for the selected file. A token ignores out-of-order responses.
  const token = useRef(0);
  useEffect(() => {
    // Collapsed untracked directories ("dir/") have no single diff.
    if (!sel || sel.kind === "conflict" || sel.path.endsWith("/")) {
      setDiffText("");
      return;
    }
    const mine = ++token.current;
    setDiffLoading(true);
    api
      .fileDiff(path, sel.path, sel.kind === "staged", sel.untracked, 3)
      .then((t) => mine === token.current && (setDiffText(t), setDiffLoading(false)))
      .catch((e) => {
        if (mine !== token.current) return;
        setDiffText("");
        setDiffLoading(false);
        notify("error", errorMessage(e));
      });
  }, [path, sel?.path, sel?.kind, sel?.untracked, status, notify]);

  const select = (f: FileChange, kind: FileSel["kind"]) =>
    setUi(tab.id, { sel: { path: f.path, kind, untracked: f.untracked } });

  /** A rename touches two paths; staging only the new one would leave the deletion behind. */
  const withOrig = (fs: FileChange[]) => fs.flatMap((f) => (f.orig_path ? [f.path, f.orig_path] : [f.path]));
  const stage = (paths: string[]) => run(tab.id, "Staging…", () => api.stage(path, paths), { quiet: true });
  const unstage = (paths: string[]) => run(tab.id, "Unstaging…", () => api.unstage(path, paths), { quiet: true });

  const discard = (fs: FileChange[]) =>
    useApp.getState().openDialog({
      title: fs.length === 1 ? `Discard changes to ${fs[0].path}?` : `Discard changes to ${fs.length} files?`,
      description: "This permanently throws away the changes. It cannot be undone.",
      fields: [],
      submitLabel: "Discard",
      danger: true,
      onSubmit: () =>
        void run(
          tab.id,
          "Discarding…",
          () =>
            api.discard(
              path,
              fs.filter((f) => !f.untracked).map((f) => f.path),
              fs.filter((f) => f.untracked).map((f) => f.path),
            ),
          { quiet: true },
        ),
    });

  const onPatch = (kind: PatchKind, patch: string) => {
    const go = () =>
      run(tab.id, kind === "stage" ? "Staging…" : kind === "unstage" ? "Unstaging…" : "Discarding…", () =>
        api.applyPatch(path, patch, kind !== "discard", kind !== "stage"), { quiet: true });
    if (kind !== "discard") return void go();
    useApp.getState().openDialog({
      title: "Discard these changes?",
      description: "The selected changes are permanently removed from the file.",
      fields: [],
      submitLabel: "Discard",
      danger: true,
      onSubmit: () => void go(),
    });
  };

  const fileMenu = (e: React.MouseEvent, f: FileChange, kind: FileSel["kind"]) =>
    openMenu(e, [
      kind === "staged"
        ? { label: "Unstage", onClick: () => unstage(withOrig([f])) }
        : { label: "Stage", onClick: () => stage(withOrig([f])) },
      ...(kind === "unstaged" ? [{ label: "Discard changes…", danger: true, onClick: () => discard([f]) }] : []),
      { separator: true },
      { label: "Copy path", onClick: () => a.copy(f.path) },
      ...(f.untracked
        ? []
        : [
            { separator: true },
            { label: "File history…", onClick: () => useUi.setState({ fileHistory: { repo: tab.path!, file: f.path, mode: "history" } }) },
            { label: "Blame (who changed each line)…", onClick: () => useUi.setState({ fileHistory: { repo: tab.path!, file: f.path, mode: "blame" } }) },
          ]),
    ]);

  const row = (f: FileChange, kind: FileSel["kind"]) => {
    const b = badge(f, kind === "staged");
    const { dir, name } = splitPath(f.path);
    const active = sel?.path === f.path && sel.kind === kind;
    return (
      <li
        key={`${kind}:${f.path}`}
        className={`file-row${active ? " active" : ""}`}
        onClick={() => select(f, kind)}
        onDoubleClick={() => (kind === "staged" ? unstage(withOrig([f])) : kind === "unstaged" ? stage(withOrig([f])) : undefined)}
        onContextMenu={(e) => fileMenu(e, f, kind)}
        title={f.orig_path ? `${f.orig_path} → ${f.path}` : f.path}
      >
        <span className={`badge ${b.kind}`} title={b.title}>
          {b.code}
        </span>
        <span className="file-path">
          <span className="name">{name}</span>
          <span className="dir">{dir}</span>
        </span>
        {kind === "conflict" ? (
          <span className="muted small">{CONFLICT_LABELS[f.index + f.worktree] ?? "conflict"}</span>
        ) : (
          <span className="row-actions">
            {kind === "unstaged" && (
              <button className="icon-btn" title="Discard changes" aria-label="Discard changes" onClick={(e) => (e.stopPropagation(), discard([f]))}>
                <Icon name="undo" size={14} />
              </button>
            )}
            <button
              className="icon-btn"
              title={kind === "staged" ? "Unstage" : "Stage"}
              aria-label={kind === "staged" ? "Unstage" : "Stage"}
              onClick={(e) => (e.stopPropagation(), kind === "staged" ? unstage(withOrig([f])) : stage(withOrig([f])))}
            >
              <Icon name={kind === "staged" ? "x" : "plus"} size={14} />
            </button>
          </span>
        )}
      </li>
    );
  };

  const state = status?.state;
  const stateLabel = { merge: "Merge", rebase: "Rebase", "cherry-pick": "Cherry-pick", revert: "Revert" }[state ?? "merge"];

  return (
    <div className="files-view" style={{ gridTemplateColumns: `${width}px 6px 1fr` }}>
      <div className="files-left">
        {state && (
          <div className="banner warn" role="status">
            <Icon name="warn" size={16} />
            <span>
              <strong>{stateLabel} in progress</strong>
              {conflicts.length > 0 ? ` — ${conflicts.length} conflict${conflicts.length > 1 ? "s" : ""} to resolve` : " — ready to continue"}
            </span>
            <span className="banner-actions">
              <button className="btn tiny primary" disabled={conflicts.length > 0} onClick={() => a.sequencer(state, "continue")}>
                Continue
              </button>
              {state !== "merge" && (
                <button className="btn tiny" onClick={() => a.sequencer(state, "skip")}>
                  Skip
                </button>
              )}
              <button className="btn tiny danger-text" onClick={() => a.sequencer(state, "abort")}>
                Abort
              </button>
            </span>
          </div>
        )}

        <div className="filter-row">
          <Icon name="search" size={14} />
          <input placeholder="Filter files…" value={filter} onChange={(e) => setFilter(e.target.value)} aria-label="Filter files" />
        </div>

        <div className="file-lists">
          {conflicts.length > 0 && (
            <section>
              <h3 className="list-title conflict-title">Conflicts <span className="count">{conflicts.length}</span></h3>
              <ul>{conflicts.map((f) => row(f, "conflict"))}</ul>
            </section>
          )}
          <section>
            <h3 className="list-title">
              Staged <span className="count">{staged.length === 0 ? "— nothing staged" : staged.length}</span>
              {staged.length > 0 && (
                <button className="btn tiny" onClick={() => run(tab.id, "Unstaging…", () => api.unstageAll(path), { quiet: true })}>
                  Unstage all
                </button>
              )}
            </h3>
            {staged.length > 0 && <ul>{staged.map((f) => row(f, "staged"))}</ul>}
          </section>
          <section>
            <h3 className="list-title">
              Unstaged <span className="count">{unstaged.length === 0 ? "— none" : unstaged.length}</span>
              {unstaged.length > 0 && (
                <button className="btn tiny" onClick={() => run(tab.id, "Staging…", () => api.stageAll(path), { quiet: true })}>
                  Stage all
                </button>
              )}
            </h3>
            {unstaged.length > 0 ? (
              <ul>{unstaged.map((f) => row(f, "unstaged"))}</ul>
            ) : (
              files.length === 0 && <p className="muted small pad">Working tree clean ✓</p>
            )}
          </section>
        </div>

        <CommitBox tab={tab} stagedCount={files.filter((f) => !f.untracked && !f.conflicted && f.index !== ".").length} conflicts={conflicts.length} />
      </div>

      <div className="splitter" onMouseDown={onSplit} role="separator" aria-orientation="vertical" />

      <div className="files-right">
        {sel?.kind === "conflict" ? (
          <ConflictResolver key={sel.path} tab={tab} file={sel.path} />
        ) : sel ? (
          <DiffView
            text={diffText}
            loading={diffLoading}
            fileLabel={sel.path}
            actions={sel.untracked ? [] : sel.kind === "staged" ? ["unstage"] : ["stage", "discard"]}
            onPatch={onPatch}
          />
        ) : (
          <div className="empty">
            <Icon name="check" size={28} />
            <p>No changes to show.</p>
            <p className="muted small">Edit a file in your editor and it will appear here automatically.</p>
          </div>
        )}
      </div>
      {d?.loading && !status && <div className="empty">Loading…</div>}
    </div>
  );
}

function CommitBox({ tab, stagedCount, conflicts }: { tab: Tab; stagedCount: number; conflicts: number }) {
  const d = useApp((s) => s.data[tab.id]);
  const { setUi, run } = useApp.getState();
  const a = repoActions(tab.id);
  const path = tab.path!;
  const msg = d?.ui.draft ?? "";
  const amend = d?.ui.amend ?? false;
  const first = msg.split("\n")[0] ?? "";
  const busy = !!d?.busy;
  const canCommit = !busy && conflicts === 0 && (amend ? true : stagedCount > 0 && msg.trim().length > 0);

  const doCommit = async (push: boolean) => {
    const ok = await run(tab.id, "Committing…", () => api.commit(path, msg, amend), { success: amend ? "Commit amended" : "Committed" });
    if (ok) {
      setUi(tab.id, { draft: "", amend: false });
      if (push) a.push();
    }
  };

  const toggleAmend = async (on: boolean) => {
    setUi(tab.id, { amend: on });
    if (on && !msg.trim()) {
      const m = await api.headMessage(path).catch(() => "");
      setUi(tab.id, { amend: true, draft: m });
    }
  };

  return (
    <div className="commit-box">
      <textarea
        value={msg}
        placeholder={amend ? "Amend the last commit message…" : "Commit message (Ctrl+Enter to commit)"}
        aria-label="Commit message"
        onChange={(e) => setUi(tab.id, { draft: e.target.value })}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && canCommit) void doCommit(false);
        }}
        rows={3}
      />
      <div className="commit-meta">
        <span className={first.length > 72 ? "warn-text small" : "muted small"}>{first.length}/72</span>
        <label className="check inline">
          <input type="checkbox" checked={amend} onChange={(e) => void toggleAmend(e.target.checked)} />
          Amend last commit
        </label>
      </div>
      <div className="commit-actions">
        <button className="btn primary grow" disabled={!canCommit} onClick={() => void doCommit(false)}>
          {amend ? "Amend commit" : `Commit${stagedCount ? ` (${stagedCount})` : ""}`}
        </button>
        <button className="btn" disabled={!canCommit} onClick={() => void doCommit(true)} title="Commit, then open the push dialog">
          Commit &amp; push…
        </button>
      </div>
      {conflicts > 0 && <p className="warn-text small">Resolve all conflicts before committing.</p>}
    </div>
  );
}
