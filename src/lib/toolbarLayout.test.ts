import { describe, expect, it } from "vitest";
import { fileManagerName, toolbarLayout } from "./toolbarLayout";

describe("toolbarLayout", () => {
  it("shows every daily action with its label on a wide window", () => {
    expect(toolbarLayout(1440)).toEqual({ overflow: [], labels: true });
    expect(toolbarLayout(1150)).toEqual({ overflow: [], labels: true });
  });
  it("moves Stash and Tag into More first", () => {
    expect(toolbarLayout(1149)).toEqual({ overflow: ["stash", "tag"], labels: true });
    expect(toolbarLayout(1000)).toEqual({ overflow: ["stash", "tag"], labels: true });
  });
  it("then Branch and Merge, still with labels", () => {
    expect(toolbarLayout(999)).toEqual({ overflow: ["branch", "merge", "stash", "tag"], labels: true });
    expect(toolbarLayout(860)).toEqual({ overflow: ["branch", "merge", "stash", "tag"], labels: true });
  });
  it("drops the labels only on a very narrow window", () => {
    expect(toolbarLayout(859)).toEqual({ overflow: ["branch", "merge", "stash", "tag"], labels: false });
  });
  it("keeps the full layout until the width is known", () => {
    expect(toolbarLayout(0)).toEqual({ overflow: [], labels: true });
  });
});

describe("fileManagerName", () => {
  it("names the platform's file manager", () => {
    expect(fileManagerName("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")).toBe("Finder");
    expect(fileManagerName("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("Explorer");
    expect(fileManagerName("Mozilla/5.0 (X11; Linux x86_64)")).toBe("file manager");
  });
});
