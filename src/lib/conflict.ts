export type Segment =
  | { kind: "text"; text: string }
  | {
      kind: "conflict";
      ours: string;
      theirs: string;
      base: string | null;
      oursLabel: string;
      theirsLabel: string;
    };

export type Choice = "ours" | "theirs" | "both" | "both-rev" | "base";

// Splits into lines keeping their line endings. (No regex lookbehind: older Safari engines on macOS lack it.)
const lines = (s: string) => s.match(/[^\n]*\n|[^\n]+$/g) ?? [];
const isMarker = (line: string, m: string) =>
  line.startsWith(m) && (line.length === m.length || /^[ \r\n]/.test(line[m.length]));

/** Splits a file containing conflict markers into plain text and conflict segments. */
export function parseConflicts(text: string): Segment[] {
  const out: Segment[] = [];
  let buf = "";
  let state: "text" | "ours" | "base" | "theirs" = "text";
  let ours = "";
  let base: string | null = null;
  let theirs = "";
  let oursLabel = "";

  for (const line of lines(text)) {
    if (state === "text") {
      if (isMarker(line, "<<<<<<<")) {
        if (buf) out.push({ kind: "text", text: buf });
        buf = "";
        state = "ours";
        ours = "";
        base = null;
        theirs = "";
        oursLabel = line.slice(7).trim();
      } else buf += line;
    } else if (state === "ours") {
      if (isMarker(line, "|||||||")) {
        state = "base";
        base = "";
      } else if (isMarker(line, "=======")) state = "theirs";
      else ours += line;
    } else if (state === "base") {
      if (isMarker(line, "=======")) state = "theirs";
      else base += line;
    } else if (isMarker(line, ">>>>>>>")) {
      out.push({
        kind: "conflict",
        ours,
        theirs,
        base,
        oursLabel,
        theirsLabel: line.slice(7).trim(),
      });
      state = "text";
    } else theirs += line;
  }
  // An unterminated conflict is kept verbatim rather than silently dropped.
  if (state !== "text") {
    buf += "<<<<<<< " + oursLabel + "\n" + ours + (base !== null ? "||||||| \n" + base : "") + "=======\n" + theirs;
  }
  if (buf) out.push({ kind: "text", text: buf });
  return out;
}

export function conflictCount(segments: Segment[]): number {
  return segments.filter((s) => s.kind === "conflict").length;
}

function pick(c: Extract<Segment, { kind: "conflict" }>, choice: Choice): string {
  switch (choice) {
    case "ours":
      return c.ours;
    case "theirs":
      return c.theirs;
    case "both":
      return c.ours + c.theirs;
    case "both-rev":
      return c.theirs + c.ours;
    case "base":
      return c.base ?? "";
  }
}

/**
 * Builds the resolved text. `choices[i]` belongs to the i-th conflict; an unresolved
 * conflict is written back with its markers so nothing is lost.
 */
export function buildResolved(segments: Segment[], choices: (Choice | undefined)[]): string {
  let i = 0;
  return segments
    .map((s) => {
      if (s.kind === "text") return s.text;
      const choice = choices[i++];
      if (choice) return pick(s, choice);
      return (
        `<<<<<<< ${s.oursLabel}\n${s.ours}` +
        (s.base !== null ? `||||||| base\n${s.base}` : "") +
        `=======\n${s.theirs}>>>>>>> ${s.theirsLabel}\n`
      );
    })
    .join("");
}
