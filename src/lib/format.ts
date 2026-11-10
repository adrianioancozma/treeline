const UNITS: [number, string][] = [
  [60, "s"],
  [60, "min"],
  [24, "h"],
  [30, "d"],
  [12, "mo"],
  [Infinity, "y"],
];

/** "3 h ago" style relative time from a unix timestamp in seconds. */
export function relativeTime(unixSeconds: number, now = Date.now()): string {
  let diff = Math.max(0, Math.floor(now / 1000 - unixSeconds));
  if (diff < 45) return "just now";
  for (const [size, label] of UNITS) {
    if (diff < size) {
      return `${Math.max(1, Math.round(diff))} ${label} ago`;
    }
    diff /= size;
  }
  return "";
}

export function fullDate(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function splitPath(p: string): { dir: string; name: string } {
  // Git reports an untracked folder as "a/b/" : name it "b/" inside "a/" instead of an empty name.
  const end = p.endsWith("/") ? p.length - 1 : p.length;
  const i = p.lastIndexOf("/", end - 1);
  return i < 0 ? { dir: "", name: p } : { dir: p.slice(0, i + 1), name: p.slice(i + 1) };
}

/** Parses `%(upstream:track)` output such as "[ahead 2, behind 1]". */
export function parseTrack(track: string | null): { ahead: number; behind: number; gone: boolean } {
  if (!track) return { ahead: 0, behind: 0, gone: false };
  return {
    ahead: Number(/ahead (\d+)/.exec(track)?.[1] ?? 0),
    behind: Number(/behind (\d+)/.exec(track)?.[1] ?? 0),
    gone: track.includes("gone"),
  };
}

export type RefKind = "head" | "branch" | "remote" | "tag";

/** Classifies a `%D` decoration such as "HEAD -> main", "origin/main" or "tag: v1". */
export function classifyRef(ref: string, remotes: string[]): { label: string; kind: RefKind } | null {
  if (ref === "HEAD") return { label: "HEAD", kind: "head" };
  if (ref.startsWith("HEAD -> ")) return { label: ref.slice(8), kind: "head" };
  if (ref.startsWith("tag: ")) return { label: ref.slice(5), kind: "tag" };
  const remote = remotes.find((r) => ref.startsWith(r + "/"));
  return { label: ref, kind: remote ? "remote" : "branch" };
}
