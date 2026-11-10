import { describe, expect, it } from "vitest";
import { autoSize, graphWidths, historyListHeights } from "./paneSize";

describe("autoSize", () => {
  it("follows the automatic size until the user has chosen one", () => {
    expect(autoSize(null, 120, 40, 300)).toBe(120);
  });
  it("uses the size the user chose", () => {
    expect(autoSize(200, 120, 40, 300)).toBe(200);
  });
  it("clamps a remembered size to the current range", () => {
    expect(autoSize(500, 120, 40, 300)).toBe(300);
    expect(autoSize(10, 120, 40, 300)).toBe(40);
  });
});

describe("graphWidths", () => {
  const LANE = 14;
  it("keeps the old automatic width: up to 12 lanes", () => {
    expect(graphWidths(3, LANE).auto).toBe(3 * LANE + 8);
    expect(graphWidths(20, LANE).auto).toBe(12 * LANE + 8);
  });
  it("has no maximum: the user decides how wide the column is", () => {
    expect(graphWidths(20, LANE).max).toBe(Infinity);
    expect(graphWidths(3, LANE).max).toBe(Infinity);
  });
  it("can shrink to a single lane", () => {
    expect(graphWidths(20, LANE).min).toBe(1 * LANE + 8);
    expect(graphWidths(1, LANE).min).toBe(1 * LANE + 8);
  });
});

describe("historyListHeights", () => {
  // The area holds the list, a 6 px splitter and the commit detail panel (at least 220 px).
  it("gives the list 70% of the area by default", () => {
    expect(historyListHeights(1000).auto).toBe(700);
    expect(historyListHeights(1100).auto).toBe(770);
  });
  it("grows with a tall window: no fixed maximum, only room for the detail panel", () => {
    expect(historyListHeights(1600).max).toBe(1600 - 6 - 220);
    expect(historyListHeights(1600).auto).toBe(1120);
  });
  it("never squeezes the detail panel below its minimum, even by default", () => {
    const h = historyListHeights(600);
    expect(autoSize(null, h.auto, h.min, h.max)).toBe(600 - 6 - 220);
  });
  it("keeps a usable list on a very short window", () => {
    expect(historyListHeights(300)).toEqual({ auto: 210, min: 140, max: 140 });
  });
  it("uses the old default until the area is measured", () => {
    expect(historyListHeights(0, 900)).toEqual({ auto: 540, min: 140, max: Infinity });
  });
});
