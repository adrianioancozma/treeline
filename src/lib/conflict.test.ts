import { describe, expect, it } from "vitest";
import { buildResolved, conflictCount, parseConflicts } from "./conflict";

const FILE = "top\n<<<<<<< HEAD\nours line\n=======\ntheirs line\n>>>>>>> feature\nbottom\n";

describe("parseConflicts", () => {
  it("splits text and conflicts", () => {
    const s = parseConflicts(FILE);
    expect(s).toHaveLength(3);
    expect(conflictCount(s)).toBe(1);
    const c = s[1];
    expect(c.kind === "conflict" && c.ours).toBe("ours line\n");
    expect(c.kind === "conflict" && c.theirs).toBe("theirs line\n");
    expect(c.kind === "conflict" && c.theirsLabel).toBe("feature");
  });

  it("reads diff3 base sections", () => {
    const s = parseConflicts("<<<<<<< a\nx\n||||||| base\nb\n=======\ny\n>>>>>>> b\n");
    expect(s[0].kind === "conflict" && s[0].base).toBe("b\n");
  });

  it("round-trips text without conflicts", () => {
    expect(buildResolved(parseConflicts("a\nb\n"), [])).toBe("a\nb\n");
  });
});

describe("buildResolved", () => {
  const segs = parseConflicts(FILE);
  it("applies each choice", () => {
    expect(buildResolved(segs, ["ours"])).toBe("top\nours line\nbottom\n");
    expect(buildResolved(segs, ["theirs"])).toBe("top\ntheirs line\nbottom\n");
    expect(buildResolved(segs, ["both"])).toBe("top\nours line\ntheirs line\nbottom\n");
    expect(buildResolved(segs, ["both-rev"])).toBe("top\ntheirs line\nours line\nbottom\n");
  });

  it("keeps markers for unresolved conflicts", () => {
    expect(buildResolved(segs, [undefined])).toContain("<<<<<<< HEAD");
  });
});
