import { useEffect, useMemo, useState } from "react";
import { api, errorMessage } from "../api";
import { buildResolved, conflictCount, parseConflicts, type Choice } from "../lib/conflict";
import { useApp } from "../store";
import { ErrorBox } from "./ErrorBox";
import type { ConflictFile, Tab } from "../types";

const CHOICES: { id: Choice; label: string }[] = [
  { id: "ours", label: "Use ours" },
  { id: "theirs", label: "Use theirs" },
  { id: "both", label: "Both" },
  { id: "both-rev", label: "Both (reversed)" },
];

type SideNames = { ours: string | null; theirs: string | null };

/** "release-4 (ours)" when the branch is known, otherwise just "ours". */
const who = (side: "ours" | "theirs", names: SideNames) => (names[side] ? `${names[side]} (${side})` : `"${side}"`);

/** Git writes HEAD for our side of a conflict; prefer the real branch name. */
const nameOf = (label: string, fallback: string | null) => (!label || label === "HEAD" ? fallback : label);

/** What "ours" and "theirs" mean depends on the operation in progress. */
const sideMeaning = (state: string | null | undefined, names: SideNames) =>
  state === "rebase"
    ? "During a rebase this is reversed: ours is the branch you are rebasing onto, theirs is your own commit being replayed."
    : `While merging, ours is the branch you have checked out${names.ours ? ` (${names.ours})` : ""} and theirs is the branch being merged in${names.theirs ? ` (${names.theirs})` : ""}.`;

/** Hint for a per-conflict choice. */
function choiceHint(choice: Choice, names: SideNames): string {
  const o = who("ours", names);
  const t = who("theirs", names);
  switch (choice) {
    case "ours":
      return `Keep only the lines from ${o} for this conflict. The lines from ${t} are thrown away.`;
    case "theirs":
      return `Keep only the lines from ${t} for this conflict. The lines from ${o} are thrown away.`;
    case "both":
      return `Keep both: the lines from ${o} first, then the lines from ${t}. Nothing is thrown away, so check that the result still makes sense.`;
    case "both-rev":
      return `Keep both: the lines from ${t} first, then the lines from ${o}. Nothing is thrown away, so check that the result still makes sense.`;
    default:
      return "";
  }
}

/** Hint for taking one whole side of the file. */
function sideHint(side: "ours" | "theirs", names: SideNames, state: string | null | undefined): string {
  const other = side === "ours" ? "theirs" : "ours";
  return `Take the whole file exactly as it is on ${who(side, names)} and stage it. Every change from ${who(other, names)} to this file is thrown away, including anything git already resolved. If ${who(side, names)} deleted the file, the deletion is staged. ${sideMeaning(state, names)}`;
}

export function ConflictResolver({ tab, file }: { tab: Tab; file: string }) {
  const path = tab.path!;
  const { run, notify } = useApp.getState();
  const [data, setData] = useState<ConflictFile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [choices, setChoices] = useState<(Choice | undefined)[]>([]);
  const [result, setResult] = useState("");
  const opState = useApp((s) => s.data[tab.id]?.status?.state);
  const branch = useApp((s) => s.data[tab.id]?.status?.branch ?? null);

  useEffect(() => {
    let live = true;
    api
      .conflictFile(path, file)
      .then((c) => live && setData(c))
      .catch((e) => live && setError(errorMessage(e)));
    return () => {
      live = false;
    };
  }, [path, file]);

  // Branch names for the two sides. During a rebase HEAD is detached and the roles swap, so no names are shown.
  const names: SideNames = {
    ours: opState === "rebase" ? null : branch,
    theirs: opState === "rebase" ? null : (data?.theirsName ?? null),
  };

  const segments = useMemo(() => (data ? parseConflicts(data.work) : []), [data]);
  const total = conflictCount(segments);
  const resolved = choices.filter(Boolean).length;

  // The result preview follows the choices; hand edits stay until the next choice.
  useEffect(() => {
    if (data) setResult(buildResolved(segments, choices));
  }, [segments, choices, data]);

  const choose = (i: number, c: Choice) =>
    setChoices((prev) => {
      const next = [...prev];
      next[i] = c;
      return next;
    });
  const chooseAll = (c: Choice) => setChoices(Array.from({ length: total }, () => c));

  const side = (s: "ours" | "theirs") =>
    run(tab.id, "Resolving…", () => api.resolveSide(path, file, s), { success: `Took ${s} for ${file}` });

  const save = () =>
    run(tab.id, "Saving resolution…", () => api.saveResolution(path, file, result), { success: `Resolved ${file}` });

  if (error)
    return (
      <div className="empty">
        <ErrorBox text={error} compact />
        <div className="row center">
          <button className="btn" title={sideHint("ours", names, opState)} onClick={() => side("ours")}>
            Keep ours
          </button>
          <button className="btn" title={sideHint("theirs", names, opState)} onClick={() => side("theirs")}>
            Keep theirs
          </button>
        </div>
      </div>
    );
  if (!data) return <div className="empty">Loading conflict…</div>;

  const hasMarkersLeft = /^(<<<<<<<|=======|>>>>>>>)/m.test(result);

  if (total === 0 && data.prefilled) {
    return (
      <div className="resolver">
        <header className="resolver-head">
          <div>
            <h2>{file}</h2>
            <span className="muted small">Filled in from your earlier resolution</span>
          </div>
          <button className="btn primary" title="Write the result shown below (including any edits you make) to the file and stage it, keeping the resolution git remembered. It does not commit; you still commit the merge afterwards." onClick={() => run(tab.id, "Saving resolution…", () => api.saveResolution(path, file, result), { success: `Resolved ${file}` })}>
            Use this result
          </button>
        </header>
        <div className="mm-banner info" style={{ margin: "0 12px" }}>
          Git recognised this conflict and applied the way you resolved it before, so there are no conflict markers. Review the result below, then press{" "}
          <strong>Use this result</strong>. <strong>Keep ours</strong> or <strong>Keep theirs</strong> would throw that resolution away.
        </div>
        <div className="result-pane">
          <div className="side-title">Result (editable)</div>
          <textarea value={result} onChange={(e) => setResult(e.target.value)} spellCheck={false} aria-label="Resolved file content" />
        </div>
        <div className="row center" style={{ padding: 12 }}>
          <button className="btn" title={"Discards the resolution git remembered. " + sideHint("ours", names, opState)} onClick={() => side("ours")}>
            Keep ours instead
          </button>
          <button className="btn" title={"Discards the resolution git remembered. " + sideHint("theirs", names, opState)} onClick={() => side("theirs")}>
            Keep theirs instead
          </button>
        </div>
      </div>
    );
  }

  if (total === 0) {
    return (
      <div className="resolver">
        <header className="resolver-head">
          <h2>{file}</h2>
        </header>
        <div className="empty">
          <p>This conflict has no inline markers — one side added or deleted the file.</p>
          <div className="row center">
            <button className="btn" title={sideHint("ours", names, opState)} onClick={() => side("ours")}>
              Keep ours
            </button>
            <button className="btn" title={sideHint("theirs", names, opState)} onClick={() => side("theirs")}>
              Keep theirs
            </button>
          </div>
        </div>
      </div>
    );
  }

  let ci = -1;
  return (
    <div className="resolver">
      <header className="resolver-head">
        <div>
          <h2>{file}</h2>
          <span className="muted small">
            {resolved} of {total} conflict{total > 1 ? "s" : ""} resolved
          </span>
        </div>
        <div className="row">
          <button className="btn tiny" title={`Use the lines from ${who("ours", names)} for every conflict in this file. The lines from ${who("theirs", names)} are thrown away in all of them.`} onClick={() => chooseAll("ours")}>
            All ours
          </button>
          <button className="btn tiny" title={`Use the lines from ${who("theirs", names)} for every conflict in this file. The lines from ${who("ours", names)} are thrown away in all of them.`} onClick={() => chooseAll("theirs")}>
            All theirs
          </button>
          <button
            className="btn primary"
            disabled={resolved < total || hasMarkersLeft}
            title={resolved < total ? `Choose ours, theirs or both for each of the ${total} conflicts first (${resolved} done).` : "Write the result shown below to the file and stage it. It does not commit; you still commit the merge afterwards."}
            onClick={() => (hasMarkersLeft ? notify("error", "Conflict markers remain in the result.") : save())}
          >
            Mark resolved
          </button>
        </div>
      </header>

      <div className="resolver-body">
        {segments.map((s, i) => {
          if (s.kind === "text") {
            const lines = s.text.replace(/\n$/, "").split("\n");
            const shown = lines.length > 6 ? [...lines.slice(0, 3), "⋯", ...lines.slice(-3)] : lines;
            return (
              <pre key={i} className="ctx-block">
                {shown.join("\n")}
              </pre>
            );
          }
          ci++;
          const idx = ci;
          const picked = choices[idx];
          return (
            <section key={i} className={`conflict-block${picked ? " done" : ""}`}>
              <div className="conflict-bar">
                <strong>Conflict {idx + 1}</strong>
                <span className="choices">
                  {CHOICES.map((c) => (
                    <button key={c.id} className={`btn tiny${picked === c.id ? " primary" : ""}`} title={choiceHint(c.id, { ours: nameOf(s.oursLabel, names.ours), theirs: names.theirs ?? nameOf(s.theirsLabel, null) })} onClick={() => choose(idx, c.id)}>
                      {c.label}
                    </button>
                  ))}
                </span>
              </div>
              <div className="conflict-cols">
                <div className={`side ours${picked === "ours" || picked === "both" ? " chosen" : ""}`}>
                  <div className="side-title">Ours · {nameOf(s.oursLabel, names.ours) || "current"}</div>
                  <pre>{s.ours || " "}</pre>
                </div>
                <div className={`side theirs${picked === "theirs" || picked === "both" ? " chosen" : ""}`}>
                  <div className="side-title">Theirs · {names.theirs ?? (nameOf(s.theirsLabel, null) || "incoming")}</div>
                  <pre>{s.theirs || " "}</pre>
                </div>
              </div>
            </section>
          );
        })}
      </div>

      <div className="result-pane">
        <div className="side-title">Result (editable)</div>
        <textarea value={result} onChange={(e) => setResult(e.target.value)} spellCheck={false} aria-label="Resolved file content" />
      </div>
    </div>
  );
}
