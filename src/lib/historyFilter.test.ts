import { describe, expect, it } from "vitest";
import { authorsOf, dateWindow, filtersActive, isoWithOffset, noFilters, parseLocal } from "./historyFilter";

describe("history filters", () => {
  it("is inactive until something is filled in", () => {
    expect(filtersActive(noFilters())).toBe(false);
    expect(filtersActive({ ...noFilters(), text: " fix " })).toBe(true);
    expect(filtersActive({ ...noFilters(), author: "ann" })).toBe(true);
    expect(filtersActive({ ...noFilters(), range: "7d" })).toBe(true);
    // A custom range with no bound yet filters nothing.
    expect(filtersActive({ ...noFilters(), range: "custom" })).toBe(false);
    expect(filtersActive({ ...noFilters(), range: "custom", from: "2026-11-01T09:30" })).toBe(true);
  });

  it("counts relative ranges back from now", () => {
    const now = new Date(2026, 10, 9, 15, 0, 0);
    const since = (range: "1h" | "24h" | "7d" | "30d") => new Date(dateWindow({ ...noFilters(), range }, now).since!).getTime();
    expect(now.getTime() - since("1h")).toBe(3600_000);
    expect(now.getTime() - since("24h")).toBe(86_400_000);
    expect(now.getTime() - since("7d")).toBe(7 * 86_400_000);
    expect(now.getTime() - since("30d")).toBe(30 * 86_400_000);
    expect(dateWindow({ ...noFilters(), range: "1h" }, now).until).toBeUndefined();
    expect(dateWindow(noFilters(), now)).toEqual({});
  });

  it("turns a custom range with date and time into exact moments", () => {
    const w = dateWindow({ ...noFilters(), range: "custom", from: "2026-11-01T09:30", to: "2026-11-05T17:00" });
    expect(new Date(w.since!).getTime()).toBe(new Date(2026, 10, 1, 9, 30).getTime());
    expect(new Date(w.until!).getTime()).toBe(new Date(2026, 10, 5, 17, 0).getTime());
    // One bound is enough.
    expect(dateWindow({ ...noFilters(), range: "custom", to: "2026-11-05T17:00" }).since).toBeUndefined();
  });

  it("writes ISO times with an offset and rejects bad input", () => {
    expect(isoWithOffset(new Date(2026, 0, 2, 3, 4, 5))).toMatch(/^2026-01-02T03:04:05[+-]\d\d:\d\d$/);
    expect(parseLocal("2026-11-01T09:30")).not.toBeNull();
    expect(parseLocal("")).toBeNull();
    expect(parseLocal("yesterday")).toBeNull();
    expect(dateWindow({ ...noFilters(), range: "custom", from: "nonsense" })).toEqual({ since: undefined, until: undefined });
  });

  it("lists authors, most active first", () => {
    expect(authorsOf([{ author: "Bob" }, { author: "Ann" }, { author: "Ann" }, { author: "" }])).toEqual(["Ann", "Bob"]);
  });
});
