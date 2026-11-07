import { describe, expect, it } from "vitest";
import { classifyRef, parseTrack, relativeTime, splitPath } from "./format";

describe("format helpers", () => {
  it("formats relative time", () => {
    const now = 1_700_000_000_000;
    expect(relativeTime(now / 1000 - 10, now)).toBe("just now");
    expect(relativeTime(now / 1000 - 5 * 60, now)).toBe("5 min ago");
    expect(relativeTime(now / 1000 - 3 * 3600, now)).toBe("3 h ago");
    expect(relativeTime(now / 1000 - 2 * 86400, now)).toBe("2 d ago");
  });

  it("splits paths", () => {
    expect(splitPath("src/a/b.ts")).toEqual({ dir: "src/a/", name: "b.ts" });
    expect(splitPath("README.md")).toEqual({ dir: "", name: "README.md" });
  });

  it("parses tracking info", () => {
    expect(parseTrack("[ahead 2, behind 1]")).toEqual({ ahead: 2, behind: 1, gone: false });
    expect(parseTrack("[gone]").gone).toBe(true);
    expect(parseTrack(null)).toEqual({ ahead: 0, behind: 0, gone: false });
  });

  it("classifies decorations", () => {
    expect(classifyRef("HEAD -> main", ["origin"])).toEqual({ label: "main", kind: "head" });
    expect(classifyRef("origin/dev", ["origin"])?.kind).toBe("remote");
    expect(classifyRef("tag: v1", [])).toEqual({ label: "v1", kind: "tag" });
    expect(classifyRef("feature/x", ["origin"])?.kind).toBe("branch");
  });
});
