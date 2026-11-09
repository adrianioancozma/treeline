import { describe, expect, it } from "vitest";
import { gitNeedsSecurityUpdate, outdatedGitMessage, parseGitVersion } from "./gitVersion";

describe("git version check", () => {
  it("reads the usual version lines", () => {
    expect(parseGitVersion("git version 2.43.0.windows.1")).toEqual([2, 43, 0]);
    expect(parseGitVersion("git version 2.50.1")).toEqual([2, 50, 1]);
    expect(parseGitVersion("git version 2.39.5 (Apple Git-154)")).toEqual([2, 39, 5]);
    expect(parseGitVersion("nonsense")).toBeNull();
  });

  it("flags releases older than the fixes, in every maintained line", () => {
    for (const v of ["2.43.0.windows.1", "2.43.6", "2.44.3", "2.45.3", "2.46.3", "2.47.2", "2.48.1", "2.49.0", "2.50.0", "2.39.5", "2.30.0", "1.9.0"]) {
      expect(gitNeedsSecurityUpdate(`git version ${v}`), v).toBe(true);
    }
  });

  it("accepts releases that carry the fixes", () => {
    for (const v of ["2.43.7", "2.44.4", "2.45.4", "2.46.4", "2.47.3", "2.48.2", "2.49.1", "2.50.1", "2.51.0", "2.60.2", "3.0.0"]) {
      expect(gitNeedsSecurityUpdate(`git version ${v}`), v).toBe(false);
    }
  });

  it("stays quiet when the version is unknown", () => {
    expect(gitNeedsSecurityUpdate(null)).toBe(false);
    expect(gitNeedsSecurityUpdate("git version ???")).toBe(false);
  });

  it("builds a readable notice", () => {
    expect(outdatedGitMessage("git version 2.43.0.windows.1")).toContain("Your Git (2.43.0.windows.1) is older");
  });
});
