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

describe("git-missing explanation", () => {
  it("offers Locate Git and shows the Windows folder with its backslashes", () => {
    const ex = explainError("Could not run git (is it installed and on PATH?): program not found");
    expect(ex.id).toBe("git-missing");
    expect(ex.actions).toContain("choose-git");
    expect(ex.fixes.join(" ")).toContain(String.raw`C:\Program Files\Git\cmd`);
  });
});

// The texts Rust builds for a computer reached over ssh (src-tauri/core/remote-errors.json, checked on the Rust side too).
import golden from "../../src-tauri/core/remote-errors.json";
import { remoteErrorHost } from "./explain";

describe("connection to another computer", () => {
  const start = (reason: string) => `Treeline could not connect to “${golden.host}”: ${reason}`;

  it("explains each kind of failure with its own text", () => {
    expect(id(golden.start)).toBe("remote-unreachable");
    expect(id(golden.lost)).toBe("remote-lost");
    expect(id(golden.cancelled)).toBe("remote-closed");
    expect(id(golden.timeout)).toBe("remote-slow");
    expect(id(start("adrian@linux-box.local: Permission denied (publickey).")) ).toBe("remote-auth");
    expect(id(start("Host key verification failed."))).toBe("remote-auth");
    expect(id(start("the helper sent to it is older than this Treeline: it lacks file_log."))).toBe("remote-helper-old");
    expect(id(start("Treeline has no helper for Darwin arm64 yet."))).toBe("remote-helper-missing");
    expect(id(start("it did not answer within 25 seconds."))).toBe("remote-timeout");
    expect(id(start("ssh: Could not resolve hostname linux-box.local: nodename nor servname provided"))).toBe("remote-unreachable");
  });

  it("names the computer and says what to do", () => {
    const e = explainError(golden.start);
    expect(e.host).toBe(golden.host);
    expect(e.title).toContain(golden.host);
    expect(e.fixes.length).toBeGreaterThan(0);
    expect(e.why).not.toMatch(/Git reported an error/);
  });

  it("leaves git's own network and sign-in errors to the existing explanations", () => {
    expect(id("ssh: connect to host github.com port 22: Connection refused\nfatal: Could not read from remote repository.")).toBe("network");
    expect(id("git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.")).toBe("auth");
    expect(id(`fatal: something mentions ${golden.lost}`)).not.toBe("remote-lost"); // only at the start
    expect(id(`fatal: something mentions ${golden.start}`)).not.toBe("remote-unreachable");
  });

  it("finds the computer's name in the text", () => {
    for (const host of ["adrian@linux-box.local", "10.0.0.5", "a_b-2", "host."]) {
      expect(remoteErrorHost(`Treeline lost the connection to “${host}”.`)).toBe(host);
      expect(remoteErrorHost(`Treeline could not connect to “${host}”: ssh: No route to host`)).toBe(host);
    }
    expect(remoteErrorHost(golden.timeout)).toBe(golden.host);
    expect(remoteErrorHost(golden.cancelled)).toBe(golden.host);
    expect(remoteErrorHost("fatal: not a git repository")).toBeNull();
    expect(remoteErrorHost(`see: ${golden.lost}`)).toBeNull();
  });
});
