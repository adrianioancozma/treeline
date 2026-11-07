import { describe, expect, it } from "vitest";
import { buildRefTree } from "./refTree";

describe("buildRefTree", () => {
  it("groups refs into folders by their slashes", () => {
    const names = ["main", "feature/login", "feature/signup", "release/2024/rc1", "bugfix/x"];
    const root = buildRefTree(names, (n) => n);
    expect(root.count).toBe(5);
    expect(root.items.map((i) => i.label)).toEqual(["main"]);
    expect(root.folders.map((f) => f.name)).toEqual(["bugfix", "feature", "release"]);
    const feature = root.folders[1];
    expect(feature.count).toBe(2);
    expect(feature.items.map((i) => i.label)).toEqual(["login", "signup"]);
    const release = root.folders[2];
    expect(release.folders[0].path).toBe("release/2024");
    expect(release.folders[0].items[0]).toEqual({ label: "rc1", item: "release/2024/rc1" });
  });

  it("handles an empty list", () => {
    const root = buildRefTree<string>([], (n) => n);
    expect(root.count).toBe(0);
    expect(root.folders).toEqual([]);
  });
});
