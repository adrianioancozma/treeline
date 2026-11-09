/** The History filters: who, what and when. The commit list is never filtered, only highlighted. */

/** Quick "counted back from now" choices, plus a custom from/to range. "" = any time. */
export type DateRange = "" | "1h" | "24h" | "7d" | "30d" | "custom";

export const DATE_RANGES: { value: DateRange; label: string }[] = [
  { value: "", label: "Any time" },
  { value: "1h", label: "Last hour" },
  { value: "24h", label: "Last 24 hours" },
  { value: "7d", label: "Last 7 days" },
  { value: "30d", label: "Last 30 days" },
  { value: "custom", label: "Custom range…" },
];

const SECONDS: Record<string, number> = { "1h": 3600, "24h": 86400, "7d": 7 * 86400, "30d": 30 * 86400 };

export interface HistoryFilters {
  /** Commit message text or the start of a commit id. */
  text: string;
  /** Author name or e-mail. */
  author: string;
  range: DateRange;
  /** Custom range bounds as typed in a datetime-local field ("2026-11-01T09:30"), local time. */
  from: string;
  to: string;
}

export const noFilters = (): HistoryFilters => ({ text: "", author: "", range: "", from: "", to: "" });

/** The date part of the filters in use: a custom range counts only once a bound is filled in. */
export function dateFilterActive(f: HistoryFilters): boolean {
  return f.range === "custom" ? !!(f.from || f.to) : f.range !== "";
}

export function filtersActive(f: HistoryFilters): boolean {
  return !!(f.text.trim() || f.author.trim() || dateFilterActive(f));
}

const pad = (n: number, w = 2) => String(Math.abs(n)).padStart(w, "0");

/** A moment as ISO 8601 in local time with its offset ("2026-11-01T09:30:00+02:00"). */
export function isoWithOffset(d: Date): string {
  const off = -d.getTimezoneOffset();
  const sign = off >= 0 ? "+" : "-";
  return (
    `${pad(d.getFullYear(), 4)}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` +
    `T${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}` +
    `${sign}${pad(Math.floor(Math.abs(off) / 60))}:${pad(Math.abs(off) % 60)}`
  );
}

/** Reads a datetime-local value as local time; null when empty or not a date. */
export function parseLocal(value: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?$/.exec(value);
  if (!m) return null;
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]), Number(m[4]), Number(m[5]), Number(m[6] ?? 0));
  return Number.isNaN(d.getTime()) ? null : d;
}

/** The `since` / `until` bounds (ISO with offset) the filters stand for at time `now`. */
export function dateWindow(f: HistoryFilters, now: Date = new Date()): { since?: string; until?: string } {
  if (f.range === "custom") {
    const from = parseLocal(f.from);
    const to = parseLocal(f.to);
    return { since: from ? isoWithOffset(from) : undefined, until: to ? isoWithOffset(to) : undefined };
  }
  const back = SECONDS[f.range];
  return back ? { since: isoWithOffset(new Date(now.getTime() - back * 1000)) } : {};
}

/** The authors seen in the loaded commits, most active first, one entry per name. */
export function authorsOf(log: { author: string }[]): string[] {
  const n = new Map<string, number>();
  for (const c of log) if (c.author) n.set(c.author, (n.get(c.author) ?? 0) + 1);
  return [...n.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([a]) => a);
}
