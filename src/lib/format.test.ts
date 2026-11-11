import { describe, expect, it } from "vitest";
import { classifyRef, listDate, parseTrack, pathParts, relativeTime, splitPath } from "./format";

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

  it("names a folder (git reports untracked folders with a trailing slash)", () => {
    expect(splitPath("src/components/very/")).toEqual({ dir: "src/components/", name: "very/" });
    expect(splitPath("docs/")).toEqual({ dir: "", name: "docs/" });
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

describe("listDate", () => {
  const now = Date.UTC(2026, 10, 10, 12, 0, 0); // 10 Nov 2026, 12:00 UTC
  const ago = (s: number) => now / 1000 - s;
  it("stays relative for the last week", () => {
    expect(listDate(ago(2 * 3600), now, "en-GB")).toBe("2 h ago");
    expect(listDate(ago(6 * 86400), now, "en-GB")).toBe("6 d ago");
  });
  it("shows day and month for older commits of this year", () => {
    expect(listDate(Date.UTC(2026, 0, 11, 9) / 1000, now, "en-GB")).toBe("11 Jan");
  });
  it("adds the year for commits from earlier years", () => {
    expect(listDate(Date.UTC(2025, 10, 28, 9) / 1000, now, "en-GB")).toBe("28 Nov 2025");
  });
  it("switches to a date at exactly seven days", () => {
    expect(listDate(ago(7 * 86400), now, "en-GB")).toBe("3 Nov");
  });
});

describe("pathParts", () => {
  it("keeps the last two folders whole and leaves the rest to be shortened", () => {
    expect(pathParts("/Users/me/dev/work/clients/acme-web")).toEqual({ head: "/Users/me/dev/work", tail: "/clients/acme-web" });
  });
  it("keeps a short path whole", () => {
    expect(pathParts("/repo")).toEqual({ head: "", tail: "/repo" });
    expect(pathParts("dev/repo")).toEqual({ head: "", tail: "dev/repo" });
  });
  it("handles Windows paths and a trailing separator", () => {
    expect(pathParts("C:\\Users\\me\\src\\acme-web")).toEqual({ head: "C:\\Users\\me", tail: "\\src\\acme-web" });
    expect(pathParts("/a/b/c/")).toEqual({ head: "/a", tail: "/b/c/" });
  });
});
