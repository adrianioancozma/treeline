import { describe, expect, it } from "vitest";
import { computeGraph } from "./graph";

describe("computeGraph", () => {
  it("keeps a linear history in one lane", () => {
    const rows = computeGraph([
      { sha: "c", parents: ["b"] },
      { sha: "b", parents: ["a"] },
      { sha: "a", parents: [] },
    ]);
    expect(rows.map((r) => r.lane)).toEqual([0, 0, 0]);
    expect(Math.max(...rows.map((r) => r.lanes))).toBe(1);
  });

  it("opens a second lane for a merge and closes it when the branches meet", () => {
    // m merges b into a-line; both descend from root
    const rows = computeGraph([
      { sha: "m", parents: ["a", "b"] },
      { sha: "b", parents: ["root"] },
      { sha: "a", parents: ["root"] },
      { sha: "root", parents: [] },
    ]);
    expect(rows[0].lane).toBe(0);
    expect(rows[1].lane).toBe(1); // b lives in the side lane
    expect(rows[2].lane).toBe(0);
    expect(rows[3].lane).toBe(0);
    // the side lane merges into root from the right
    expect(rows[3].segs.some((s) => s.x1 === 1 && s.x2 === 0 && s.y2 === 0.5)).toBe(true);
  });

  it("gives separate branch tips separate lanes", () => {
    const rows = computeGraph([
      { sha: "x", parents: ["base"] },
      { sha: "y", parents: ["base"] },
      { sha: "base", parents: [] },
    ]);
    expect(rows[0].lane).toBe(0);
    expect(rows[1].lane).toBe(1);
    expect(rows[2].lane).toBe(0);
  });

  it("handles an empty list", () => {
    expect(computeGraph([])).toEqual([]);
  });
});

import { reachableFrom } from "./graph";

describe("reachableFrom", () => {
  const commits = [
    { sha: "m", parents: ["a", "b"] },
    { sha: "b", parents: ["root"] },
    { sha: "a", parents: ["root"] },
    { sha: "x", parents: ["root"] },
    { sha: "root", parents: [] },
  ];
  it("collects the tip and all ancestors, across merges", () => {
    expect([...reachableFrom(commits, "m")].sort()).toEqual(["a", "b", "m", "root"]);
  });
  it("excludes commits on other branches", () => {
    expect(reachableFrom(commits, "a").has("x")).toBe(false);
    expect(reachableFrom(commits, "x").has("a")).toBe(false);
  });
  it("tolerates a tip that is not loaded", () => {
    expect(reachableFrom(commits, "missing").size).toBe(0);
  });
});
