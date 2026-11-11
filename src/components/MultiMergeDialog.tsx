import { useMemo, useState } from "react";
import { repoActions } from "../actions";
import { api, errorMessage } from "../api";
import { useApp } from "../store";
import type { MergeApplied, MergePreview } from "../types";
import { ErrorBox } from "./ErrorBox";
import { Icon } from "./Icon";
import { Modal } from "./Modal";

type Step = "pick" | "review" | "done";

const LABEL: Record<MergePreview["status"], string> = {
  "up-to-date": "Already up to date",
  "fast-forward": "Fast-forward",
  clean: "Clean merge",
  conflict: "Conflict",
  error: "Cannot merge",
};

const APPLIED_LABEL: Record<MergeApplied["status"], string> = {
  merged: "Merged",
  "fast-forwarded": "Fast-forwarded",
  "up-to-date": "Already up to date",
  conflict: "Skipped: conflict",
  failed: "Failed",
};

const OK_STATES = ["clean", "fast-forward", "merged", "fast-forwarded"];
const BAD_STATES = ["conflict", "error", "failed"];
const pillClass = (st: string) => (OK_STATES.includes(st) ? "ok" : BAD_STATES.includes(st) ? "bad" : "tag");

/**
 * Merge one branch into several: pick the source and the targets, see which merges are clean and
 * which would conflict (nothing is changed yet), then apply only the ones that are safe.
 */
export function MultiMergeDialog({ tabId, initialSource, onClose }: { tabId: string; initialSource?: string; onClose: () => void }) {
  const refs = useApp((s) => s.data[tabId]?.refs);
  const repo = useApp((s) => s.tabs.find((t) => t.id === tabId)?.path ?? "");
  const currentBranch = useApp((s) => s.data[tabId]?.status?.branch ?? null);
  const dirty = useApp((s) => (s.data[tabId]?.status?.files.length ?? 0) > 0);

  const local = useMemo(() => (refs ?? []).filter((r) => r.kind === "branch").map((r) => r.name), [refs]);
  const sources = useMemo(() => [...local, ...(refs ?? []).filter((r) => r.kind === "remote").map((r) => r.name)], [refs, local]);

  const [source, setSource] = useState(initialSource ?? currentBranch ?? local[0] ?? "");
  const [targets, setTargets] = useState<Set<string>>(new Set());
  const [step, setStep] = useState<Step>("pick");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [previews, setPreviews] = useState<MergePreview[]>([]);
  const [apply, setApply] = useState<Set<string>>(new Set());
  const [results, setResults] = useState<MergeApplied[]>([]);
  /** The branch being resolved by hand; while set the dialog is hidden but keeps its state. */
  const [paused, setPaused] = useState<string | null>(null);
  /** True once the user has resolved a conflict here, so git has something to replay. */
  const [learned, setLearned] = useState(false);
  /** Merges where the result dropped the source's changes in some files, shown until the dialog closes. */
  const [warnings, setWarnings] = useState<{ target: string; files: string[] }[]>([]);

  const candidates = local.filter((b) => b !== source);
  const picked = candidates.filter((b) => targets.has(b));
  const toggle = (set: Set<string>, setter: (s: Set<string>) => void, name: string) => {
    const next = new Set(set);
    if (!next.delete(name)) next.add(name);
    setter(next);
  };

  const check = async () => {
    setBusy(true);
    setError(null);
    try {
      const list = await api.previewMerge(repo, source, picked);
      setPreviews(list);
      setApply(new Set(list.filter((p) => p.status === "clean" || p.status === "fast-forward").map((p) => p.target)));
      setStep("review");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      const res = await api.mergeIntoBranches(repo, source, [...apply]);
      setResults(res);
      setStep("done");
      await useApp.getState().reload(tabId);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  /** Hides the dialog (state is kept), turns on rerere so git remembers the fix, and starts the merge. */
  const resolve = async (target: string) => {
    setPaused(target);
    try {
      if (await api.enableRerere(repo)) {
        useApp.getState().notify("info", "Git will now remember how you resolve conflicts in this repository (rerere), so the fix can be reused on the other branches.");
      }
      setLearned(true);
    } catch (e) {
      setError(errorMessage(e));
    }
    await repoActions(tabId).resolveMerge(target, source);
    // Git may already have filled the files in from the earlier resolution; say so before the user sees a file with no markers.
    try {
      const filled = await api.prefilledConflicts(repo);
      if (filled.length > 0) {
        useApp
          .getState()
          .notify("info", `Git filled in ${filled.join(", ")} from your earlier resolution. Open it under Commit, review the result, then choose “Use this result”. Don't pick Keep ours/theirs unless you want to discard it.`);
      }
    } catch {
      /* informational only */
    }
  };

  /** Brings the dialog back and re-tests every branch, so the one just resolved shows as up to date. */
  const resume = async () => {
    const target = paused;
    setPaused(null);
    setResults([]);
    if (target) {
      try {
        const files = await api.checkMergeResult(repo, source, target);
        setWarnings((w) => [...w.filter((x) => x.target !== target), ...(files.length ? [{ target, files }] : [])]);
      } catch {
        /* the re-test below still runs */
      }
    }
    await check();
  };

  /** Replays remembered resolutions on the branches that still conflict. */
  const reuse = async () => {
    const list = stillConflicting.map((p) => p.target);
    if (list.length === 0) return;
    setBusy(true);
    setError(null);
    try {
      const res = await api.mergeReusingResolutions(repo, source, list);
      setResults((prev) => [...prev.filter((r) => !list.includes(r.target)), ...res]);
      setWarnings((w) => [...w.filter((x) => !list.includes(x.target)), ...res.filter((r) => r.discarded.length > 0).map((r) => ({ target: r.target, files: r.discarded }))]);
      setStep("done");
      await useApp.getState().reload(tabId);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const conflicts = previews.filter((p) => p.status === "conflict");
  const handled = new Set(results.map((r) => r.target));
  const stillConflicting = conflicts.filter((p) => !handled.has(p.target) || results.some((r) => r.target === p.target && r.status === "conflict"));
  const mergeInProgress = useApp((s) => s.data[tabId]?.status?.state === "merge");
  const mergeable = previews.filter((p) => p.status === "clean" || p.status === "fast-forward");
  const skipped = results.filter((r) => !r.ok);
  const needsDirtyCare = dirty && apply.has(currentBranch ?? "\0");
  const allUpToDate = previews.length > 0 && previews.every((p) => p.status === "up-to-date");
  const stepIdx = step === "pick" ? 0 : step === "review" ? 1 : 2;

  const files = (list: string[]) => (
    <span className="mm-files" aria-label="Conflicting files">
      {list.map((f) => (
        <code key={f}>{f}</code>
      ))}
    </span>
  );

  const warningBanner = warnings.length > 0 && (
    <div className="mm-banner" role="alert">
      <strong>Check these merges: the changes from {source} did not land in every file.</strong>
      <ul style={{ margin: "6px 0 0 18px" }}>
        {warnings.map((w) => (
          <li key={w.target}>
            <code>{w.target}</code> kept its own version of{" "}
            {w.files.map((f, i) => (
              <span key={f}>
                {i > 0 && ", "}
                <code>{f}</code>
              </span>
            ))}
            . The fix is missing there. Use <em>Revert</em> on that merge commit and merge again, or edit the file(s).
          </li>
        ))}
      </ul>
    </div>
  );

  const reuseButton = learned && stillConflicting.length > 0 && (
    <button
      className="btn"
      disabled={busy}
      title="Git replays how you resolved the same conflict before. Branches it cannot fully resolve are left untouched."
      onClick={() => void reuse()}
    >
      {busy ? "Applying…" : `Apply my earlier resolution to ${stillConflicting.length}`}
    </button>
  );

  if (paused) {
    return (
      <div className="mm-paused" role="status">
        <div className="grow">
          <strong>Resolving {paused}</strong> for the multi-branch merge.{" "}
          {mergeInProgress
            ? "Fix the conflicts, stage the files and commit the merge. Then press Resume."
            : "No merge is in progress. Press Resume to continue with the other branches."}
        </div>
        <button className="btn" onClick={onClose}>
          Cancel
        </button>
        <button className="btn primary" disabled={mergeInProgress || busy} title={mergeInProgress ? "Finish the merge first" : undefined} onClick={() => void resume()}>
          Resume
        </button>
      </div>
    );
  }

  return (
    <Modal
      title="Merge into multiple branches"
      className="mm"
      onClose={onClose}
      busy={busy}
      headerExtra={
        <div className="mm-steps" aria-label="Progress">
          {["Choose", "Review", "Done"].map((label, i) => (
            <span key={label} style={{ display: "contents" }}>
              {i > 0 && <span className="bar" />}
              <span className={`s${i === stepIdx ? " on" : ""}${i < stepIdx ? " done" : ""}`}>
                <span className="n">{i < stepIdx ? "✓" : i + 1}</span>
                {label}
              </span>
            </span>
          ))}
        </div>
      }
    >
        {error && <ErrorBox text={error} compact />}

        {step === "pick" && (
          <>
            <div className="mm-source">
              <span>Merge from</span>
              <select
                value={source}
                onChange={(e) => {
                  setSource(e.target.value);
                  setTargets((t) => new Set([...t].filter((b) => b !== e.target.value)));
                }}
                aria-label="Source branch"
              >
                {sources.map((b) => (
                  <option key={b} value={b}>
                    {b}
                    {b === currentBranch ? " (current)" : ""}
                  </option>
                ))}
              </select>
            </div>
            <p className="mm-hint">
              Choose the branches that should receive <code>{source}</code>. Nothing is changed yet; the next step tests every merge and warns you about conflicts.
            </p>
            <div className="mm-list" role="group" aria-label="Target branches">
              {candidates.length === 0 && <p className="mm-hint">There are no other local branches.</p>}
              {candidates.map((b) => (
                <label key={b} className={`mm-row${targets.has(b) ? " sel" : ""}`}>
                  <input type="checkbox" checked={targets.has(b)} onChange={() => toggle(targets, setTargets, b)} />
                  <span className="mm-name">
                    <Icon name="branch" size={14} />
                    <span className="t">{b}</span>
                    {b === currentBranch && <span className="mm-pill tag">checked out</span>}
                  </span>
                </label>
              ))}
            </div>
            <div className="mm-foot">
              <button className="btn" disabled={candidates.length === 0} onClick={() => setTargets(picked.length === candidates.length ? new Set() : new Set(candidates))}>
                {picked.length === candidates.length && candidates.length > 0 ? "Select none" : "Select all"}
              </button>
              <span className="count">{picked.length} selected</span>
              <span className="grow" />
              <button className="btn" onClick={onClose}>
                Cancel
              </button>
              <button className="btn primary" disabled={picked.length === 0 || !source || busy} onClick={() => void check()}>
                {busy ? "Checking…" : "Next: check for conflicts"}
              </button>
            </div>
          </>
        )}

        {step === "review" && (
          <>
            <div className={`mm-banner${conflicts.length === 0 ? " ok" : ""}`}>
              {allUpToDate ? (
                <>
                  {previews.length === 1 ? "That branch already contains" : `All ${previews.length} branches already contain`} <code>{source}</code>. There is nothing left to merge.
                </>
              ) : conflicts.length > 0 ? (
                <>
                  <strong>
                    {conflicts.length} of {previews.length} branch{previews.length === 1 ? "" : "es"} would conflict
                  </strong>{" "}
                  and will not be merged. Resolve them afterwards with <em>Resolve…</em>, or leave them alone.
                </>
              ) : (
                <>No conflicts found. Nothing has been changed yet.</>
              )}
            </div>
            {warningBanner}
            <div className="mm-list">
              {previews.map((p) => {
                const can = p.status === "clean" || p.status === "fast-forward";
                const body = (
                  <>
                    <span className="mm-main">
                      <span className="mm-name">
                        <Icon name="branch" size={14} />
                        <span className="t">{p.target}</span>
                        <span className={`mm-pill ${pillClass(p.status)}`}>{LABEL[p.status]}</span>
                      </span>
                      {p.status === "conflict" && files(p.files)}
                      {p.message && <span className="mm-note">{p.message}</span>}
                    </span>
                    {p.status === "conflict" && (
                      <button className="btn tiny" title={`Check out ${p.target} and merge ${source} there, so you can resolve the conflicts`} onClick={() => resolve(p.target)}>
                        Resolve…
                      </button>
                    )}
                  </>
                );
                return can ? (
                  <label key={p.target} className={`mm-row${apply.has(p.target) ? " sel" : ""}`}>
                    <input type="checkbox" checked={apply.has(p.target)} onChange={() => toggle(apply, setApply, p.target)} aria-label={`Merge into ${p.target}`} />
                    {body}
                  </label>
                ) : (
                  <div key={p.target} className={`mm-row off${p.status === "conflict" || p.status === "error" ? " bad" : ""}`}>
                    {body}
                  </div>
                );
              })}
            </div>
            {needsDirtyCare && (
              <div className="mm-banner info">
                <code>{currentBranch}</code> is checked out and you have uncommitted changes. Git may refuse to merge into it; commit or stash first if so.
              </div>
            )}
            {mergeable.length === 0 && !allUpToDate && <div className="mm-banner info">Nothing can be merged automatically.</div>}
            <div className="mm-foot">
              <button className="btn" disabled={busy} onClick={() => setStep("pick")}>
                Back
              </button>
              <span className="grow" />
              {reuseButton}
              <button className="btn" disabled={busy} onClick={onClose}>
                {mergeable.length === 0 ? "Close" : "Cancel"}
              </button>
              {mergeable.length > 0 && (
                <button className="btn primary" disabled={apply.size === 0 || busy} onClick={() => void run()}>
                  {busy ? "Merging…" : `Merge into ${apply.size} branch${apply.size === 1 ? "" : "es"}`}
                </button>
              )}
            </div>
          </>
        )}

        {step === "done" && (
          <>
            <div className={`mm-banner${skipped.length === 0 && conflicts.length === 0 ? " ok" : " info"}`}>
              {results.length - skipped.length} of {previews.length} merged. Nothing was pushed.
            </div>
            {warningBanner}
            <div className="mm-list">
              {results.map((r) => (
                <div key={r.target} className={`mm-row off${r.ok ? "" : " bad"}`}>
                  <span className="mm-main">
                    <span className="mm-name">
                      <Icon name="branch" size={14} />
                      <span className="t">{r.target}</span>
                      <span className={`mm-pill ${r.discarded.length > 0 ? "bad" : pillClass(r.status)}`}>
                        {r.discarded.length > 0 ? "Merged, but changes dropped" : APPLIED_LABEL[r.status]}
                      </span>
                    </span>
                    {r.message && <span className="mm-note">{r.message}</span>}
                  </span>
                  {r.status === "conflict" && (
                    <button className="btn tiny" onClick={() => resolve(r.target)}>
                      Resolve…
                    </button>
                  )}
                </div>
              ))}
              {conflicts
                .filter((p) => !handled.has(p.target))
                .map((p) => (
                  <div key={p.target} className="mm-row off bad">
                    <span className="mm-main">
                      <span className="mm-name">
                        <Icon name="branch" size={14} />
                        <span className="t">{p.target}</span>
                        <span className="mm-pill bad">Not merged: conflict</span>
                      </span>
                      {files(p.files)}
                    </span>
                    <button className="btn tiny" onClick={() => resolve(p.target)}>
                      Resolve…
                    </button>
                  </div>
                ))}
            </div>
            <div className="mm-foot">
              <span className="grow" />
              {reuseButton}
              <button className="btn primary" onClick={onClose}>
                Done
              </button>
            </div>
          </>
        )}
    </Modal>
  );
}
