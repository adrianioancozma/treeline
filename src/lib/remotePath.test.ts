import { describe, expect, it } from "vitest";
import { filterFolders, intoFolder, openTarget, splitInput, withTilde } from "./remotePath";

describe("the path typed in the remote folder picker", () => {
  it("is the folder to list plus what to filter its folders by", () => {
    expect(splitInput("~/dev/")).toEqual({ dir: "~/dev", filter: "" });
    expect(splitInput("~/dev/p-v")).toEqual({ dir: "~/dev", filter: "p-v" });
    expect(splitInput("/srv/git/")).toEqual({ dir: "/srv/git", filter: "" });
    expect(splitInput("/sr")).toEqual({ dir: "/", filter: "sr" });
    expect(splitInput("/")).toEqual({ dir: "/", filter: "" });
  });
  it("treats ~ as the home folder, and an empty field as home too", () => {
    expect(splitInput("~")).toEqual({ dir: "~", filter: "" });
    expect(splitInput("~/")).toEqual({ dir: "~", filter: "" });
    expect(splitInput("")).toEqual({ dir: "~", filter: "" });
  });
});

describe("going into a folder", () => {
  it("adds its name and a slash, ready to list it", () => {
    expect(intoFolder("~", "dev")).toBe("~/dev/");
    expect(intoFolder("~/dev", "p-vs-np")).toBe("~/dev/p-vs-np/");
    expect(intoFolder("/", "srv")).toBe("/srv/");
  });
});

describe("showing a path", () => {
  it("writes the home folder as ~", () => {
    expect(withTilde("/home/a", "/home/a")).toBe("~");
    expect(withTilde("/home/a/dev", "/home/a")).toBe("~/dev");
    expect(withTilde("/home/ab", "/home/a")).toBe("/home/ab");
    expect(withTilde("/srv", null)).toBe("/srv");
  });
});

describe("filtering folders", () => {
  const f = (name: string, repo = false) => ({ name, repo });
  const all = [f("ariadne", true), f("apm"), f("cex-fe", true), f("p-vs-np", true), f("Praemon")];
  it("puts names that start with the text first, then names that contain it, ignoring case", () => {
    expect(filterFolders(all, "p").map((e) => e.name)).toEqual(["p-vs-np", "Praemon", "apm"]);
    expect(filterFolders(all, "NP").map((e) => e.name)).toEqual(["p-vs-np"]);
  });
  it("keeps everything for an empty filter", () => {
    expect(filterFolders(all, "")).toEqual(all);
  });
});

describe("what Open opens", () => {
  const listing = { path: "/home/a/dev", repo: false };
  it("is the folder the user selected, repository or not", () => {
    expect(openTarget(listing, { name: "p-vs-np", repo: true })).toBe("/home/a/dev/p-vs-np");
    expect(openTarget(listing, { name: "notes", repo: false })).toBe("/home/a/dev/notes");
  });
  it("is the listed folder itself when nothing is selected and it is a repository", () => {
    expect(openTarget({ path: "/home/a/dev/p-vs-np", repo: true }, undefined)).toBe("/home/a/dev/p-vs-np");
    expect(openTarget(listing, undefined)).toBeNull();
    expect(openTarget(null, { name: "x", repo: true })).toBeNull();
  });
  it("joins at the root without a double slash", () => {
    expect(openTarget({ path: "/", repo: false }, { name: "srv", repo: false })).toBe("/srv");
  });
});
