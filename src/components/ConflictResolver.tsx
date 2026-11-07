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

export function ConflictResolver({ tab, file }: { tab: Tab; file: string }) {
  const path = tab.path!;
  const { run, notify } = useApp.getState();
  const [data, setData] = useState<ConflictFile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [choices, setChoices] = useState<(Choice | undefined)[]>([]);
  const [result, setResult] = useState("");

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
          <button className="btn" onClick={() => side("ours")}>
            Keep ours
          </button>
          <button className="btn" onClick={() => side("theirs")}>
            Keep theirs
          </button>
        </div>
      </div>
    );
  if (!data) return <div className="empty">Loading conflict…</div>;

  const hasMarkersLeft = /^(<<<<<<<|=======|>>>>>>>)/m.test(result);

  if (total === 0) {
    return (
      <div className="resolver">
        <header className="resolver-head">
          <h2>{file}</h2>
        </header>
        <div className="empty">
          <p>This conflict has no inline markers — one side added or deleted the file.</p>
          <div className="row center">
            <button className="btn" onClick={() => side("ours")}>
              Keep ours
            </button>
            <button className="btn" onClick={() => side("theirs")}>
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
          <button className="btn tiny" onClick={() => chooseAll("ours")}>
            All ours
          </button>
          <button className="btn tiny" onClick={() => chooseAll("theirs")}>
            All theirs
          </button>
          <button
            className="btn primary"
            disabled={resolved < total || hasMarkersLeft}
            title={resolved < total ? "Resolve every conflict first" : "Write the file and stage it"}
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
                    <button key={c.id} className={`btn tiny${picked === c.id ? " primary" : ""}`} onClick={() => choose(idx, c.id)}>
                      {c.label}
                    </button>
                  ))}
                </span>
              </div>
              <div className="conflict-cols">
                <div className={`side ours${picked === "ours" || picked === "both" ? " chosen" : ""}`}>
                  <div className="side-title">Ours · {s.oursLabel || "current"}</div>
                  <pre>{s.ours || " "}</pre>
                </div>
                <div className={`side theirs${picked === "theirs" || picked === "both" ? " chosen" : ""}`}>
                  <div className="side-title">Theirs · {s.theirsLabel || "incoming"}</div>
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
