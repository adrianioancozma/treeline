import { describe, expect, it } from "vitest";
import { addRecent, RECENT_MAX, visibleRecent } from "./recent";

const repo = (n: string) => ({ path: `/dev/${n}`, name: n });

describe("addRecent", () => {
  it("puts the repository just opened first", () => {
    expect(addRecent([repo("a"), repo("b")], repo("c"))).toEqual([repo("c"), repo("a"), repo("b")]);
  });
  it("moves a repository opened again to the front instead of listing it twice", () => {
    expect(addRecent([repo("a"), repo("b"), repo("c")], repo("b"))).toEqual([repo("b"), repo("a"), repo("c")]);
  });
  it("keeps at most eight repositories, dropping the oldest", () => {
    const full = Array.from({ length: RECENT_MAX }, (_, i) => repo(`r${i}`));
    const next = addRecent(full, repo("new"));
    expect(RECENT_MAX).toBe(8);
    expect(next).toHaveLength(8);
    expect(next[0]).toEqual(repo("new"));
    expect(next.map((r) => r.name)).not.toContain("r7");
  });
  it("starts a list from nothing", () => {
    expect(addRecent([], repo("a"))).toEqual([repo("a")]);
  });
});

describe("visibleRecent", () => {
  it("leaves out repositories that are already bookmarked, keeping the order", () => {
    expect(visibleRecent([repo("a"), repo("b"), repo("c")], [{ path: "/dev/b" }])).toEqual([repo("a"), repo("c")]);
  });
  it("shows the whole list when nothing is bookmarked", () => {
    expect(visibleRecent([repo("a")], [])).toEqual([repo("a")]);
  });
});
