import { useEffect, useMemo, useState } from "react";
import { buildPatch, canSelectLines, countChanges, isChange, parseDiff } from "../lib/diff";

export type PatchKind = "stage" | "unstage" | "discard";

interface Props {
  /** Raw unified diff text. */
  text: string;
  /** Which hunk/line actions to offer. Empty = read-only. */
  actions?: PatchKind[];
  /** Called with the built patch and what to do with it. */
  onPatch?: (kind: PatchKind, patch: string) => void;
  loading?: boolean;
  fileLabel?: string;
}

const MAX_LINES = 4000;
const LABEL: Record<PatchKind, string> = { stage: "Stage", unstage: "Unstage", discard: "Discard" };

export function DiffView({ text, actions = [], onPatch, loading, fileLabel }: Props) {
  const diff = useMemo(() => parseDiff(text), [text]);
  const stats = useMemo(() => countChanges(diff), [diff]);
  // selected line keys: "hunk:line"
  const [sel, setSel] = useState<Set<string>>(new Set());
  const [showAll, setShowAll] = useState(false);
  useEffect(() => {
    setSel(new Set());
    setShowAll(false);
  }, [text]);

  const total = diff.hunks.reduce((n, h) => n + h.lines.length, 0);
  const editable = actions.length > 0 && !!onPatch;

  if (loading && !text) return <div className="empty">Loading diff…</div>;
  if (diff.binary) return <div className="empty">Binary file — no text diff to show.</div>;
  if (diff.hunks.length === 0)
    return <div className="empty">{text ? "No textual changes (mode or whitespace only)." : "No changes."}</div>;

  const toggle = (h: number, i: number) =>
    setSel((s) => {
      const n = new Set(s);
      const k = `${h}:${i}`;
      n.has(k) ? n.delete(k) : n.add(k);
      return n;
    });
  const selectedIn = (h: number) =>
    new Set([...sel].filter((k) => k.startsWith(`${h}:`)).map((k) => Number(k.split(":")[1])));

  let budget = showAll ? Infinity : MAX_LINES;

  return (
    <div className="diff">
      <div className="diff-head">
        <span className="diff-file">{fileLabel}</span>
        <span className="stat add">+{stats.added}</span>
        <span className="stat del">−{stats.removed}</span>
      </div>
      <div className="diff-body">
        {diff.hunks.map((h, hi) => {
          if (budget <= 0) return null;
          const lines = h.lines.slice(0, budget);
          budget -= lines.length;
          const picked = selectedIn(hi);
          const selectable = editable && canSelectLines(h);
          return (
            <section key={hi} className="hunk">
              <div className="hunk-head">
                <span className="hunk-title">
                  {h.header.slice(0, h.header.indexOf("@@", 2) + 2)} <em>{h.context}</em>
                </span>
                {editable && (
                  <span className="hunk-actions">
                    {actions.map((k) => (
                      <button
                        key={k}
                        className={`btn tiny${k === "discard" ? " danger-text" : ""}`}
                        onClick={() => {
                          const p = buildPatch(diff, hi, null, k !== "stage");
                          if (p) onPatch!(k, p);
                        }}
                      >
                        {LABEL[k]} hunk
                      </button>
                    ))}
                    {picked.size > 0 &&
                      actions.map((k) => (
                        <button
                          key={`l${k}`}
                          className={`btn tiny${k === "discard" ? " danger-text" : " primary"}`}
                          onClick={() => {
                            const p = buildPatch(diff, hi, picked, k !== "stage");
                            if (p) {
                              onPatch!(k, p);
                              setSel(new Set());
                            }
                          }}
                        >
                          {LABEL[k]} {picked.size} line{picked.size > 1 ? "s" : ""}
                        </button>
                      ))}
                  </span>
                )}
              </div>
              <table className="lines">
                <tbody>
                  {lines.map((l, li) => {
                    const on = sel.has(`${hi}:${li}`);
                    return (
                      <tr
                        key={li}
                        className={`${l.type}${on ? " picked" : ""}`}
                        onClick={selectable && isChange(l) ? () => toggle(hi, li) : undefined}
                      >
                        {editable && (
                          <td className="gutter-check">
                            {selectable && isChange(l) && <input type="checkbox" tabIndex={-1} readOnly checked={on} aria-label="Select line" />}
                          </td>
                        )}
                        <td className="num">{l.oldNo ?? ""}</td>
                        <td className="num">{l.newNo ?? ""}</td>
                        <td className="sign">{l.type === "add" ? "+" : l.type === "del" ? "−" : ""}</td>
                        <td className="code">{l.text || " "}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </section>
          );
        })}
        {!showAll && total > MAX_LINES && (
          <div className="diff-more">
            Showing the first {MAX_LINES} of {total} lines.{" "}
            <button className="btn tiny" onClick={() => setShowAll(true)}>
              Show all
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
