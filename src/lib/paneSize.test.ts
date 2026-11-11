import { describe, expect, it } from "vitest";
import { autoSize, graphWidths } from "./paneSize";

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
