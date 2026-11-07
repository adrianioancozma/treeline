import { describe, expect, it } from "vitest";
import { explainError } from "./explain";

const id = (s: string) => explainError(s).id;

describe("explainError", () => {
  it("recognises a stale index lock and extracts its path", () => {
    const e = explainError(
      "fatal: Unable to create 'C:/repo/.git/index.lock': File exists.\n\nAnother git process seems to be running",
    );
    expect(e.id).toBe("index-lock");
    expect(e.lockPath).toBe("C:/repo/.git/index.lock");
    expect(e.actions).toContain("remove-lock");
  });

  it("explains rejected pushes and offers pull", () => {
    const e = explainError("! [rejected]        main -> main (non-fast-forward)");
    expect(e.id).toBe("non-fast-forward");
    // safe fixes first; the risky one is last
    expect(e.actions).toEqual(["pull", "pull-rebase", "force-push"]);
    expect(e.fixes.join(" ")).toMatch(/rewrote history on purpose/);
    expect(e.fixes.length).toBeGreaterThan(1);
  });

  it("maps common failures", () => {
    expect(id("fatal: Authentication failed for 'https://x/y.git/'")).toBe("auth");
    expect(id("git@host: Permission denied (publickey).")).toBe("auth");
    expect(id("fatal: unable to access 'https://x/': Could not resolve host: x")).toBe("network");
    expect(id("error: Your local changes to the following files would be overwritten by checkout:")).toBe("overwritten-by-checkout");
    expect(id("CONFLICT (content): Merge conflict in a.txt\nAutomatic merge failed")).toBe("conflict");
    expect(id("fatal: refusing to merge unrelated histories")).toBe("unrelated-histories");
    expect(id("fatal: The current branch feature has no upstream branch.")).toBe("no-upstream");
    expect(id("error: The branch 'x' is not fully merged.")).toBe("not-fully-merged");
    expect(id("Author identity unknown\n\n*** Please tell me who you are.")).toBe("identity");
    expect(id("fatal: clone of 'a' into submodule path 'b' failed\nfatal: transport 'file' not allowed")).toBe("protocol-file");
    expect(id("remote: Repository not found.")).toBe("repo-not-found");
  });

  it("does not mistake an auth 'Permission denied' for a locked file", () => {
    expect(id("git@github.com: Permission denied (publickey).")).toBe("auth");
    expect(id("error: unable to unlink old 'a.txt': Permission denied")).toBe("file-locked");
  });

  it("offers force only as the last option, and only where it can make sense", () => {
    const last = (s: string) => {
      const a = explainError(s).actions;
      return a[a.length - 1];
    };
    expect(last("error: The branch 'x' is not fully merged.")).toBe("force-delete");
    expect(last("error: Your local changes to the following files would be overwritten by checkout:")).toBe("force-checkout");
    // never for lock files, auth, conflicts, network …
    for (const s of [
      "fatal: Unable to create '/r/.git/index.lock': File exists.",
      "fatal: Authentication failed",
      "CONFLICT (content): Merge conflict in a",
      "fatal: Could not resolve host: x",
    ]) {
      expect(explainError(s).actions.some((a) => a.startsWith("force"))).toBe(false);
    }
  });

  it("always returns an explanation with fixes, even for unknown errors", () => {
    const e = explainError("some brand new git failure");
    expect(e.id).toBe("unknown");
    expect(e.title).toBeTruthy();
    expect(e.fixes.length).toBeGreaterThan(0);
  });
});
