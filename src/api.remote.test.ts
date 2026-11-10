import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn() }));
import { api, setRemoteHost } from "./api";

describe("calls for a repository on another computer", () => {
  beforeEach(() => {
    invoke.mockReset().mockResolvedValue(null);
    setRemoteHost("/home/a/repo", null);
  });

  it("go to the local backend unchanged for a local repository", async () => {
    await api.commitDiff("/Users/me/repo", "abc", null, "a.txt", "old.txt", 3);
    expect(invoke).toHaveBeenCalledWith("commit_diff", { path: "/Users/me/repo", sha: "abc", parent: null, file: "a.txt", oldFile: "old.txt", context: 3 });
  });

  it("go through the helper connection, with the parameter names the helper uses", async () => {
    setRemoteHost("/home/a/repo", "linux-box.local");
    await api.commitDiff("/home/a/repo", "abc", null, "a.txt", "old.txt", 3);
    expect(invoke).toHaveBeenCalledWith("remote_call", {
      host: "linux-box.local",
      method: "commit_diff",
      params: { path: "/home/a/repo", sha: "abc", parent: null, file: "a.txt", old_file: "old.txt", context: 3 },
    });
    await api.status("/home/a/repo");
    expect(invoke).toHaveBeenLastCalledWith("remote_call", { host: "linux-box.local", method: "repo_status", params: { path: "/home/a/repo" } });
  });

  it("send writes there too, where the backend refuses them, never to the local git", async () => {
    setRemoteHost("/home/a/repo", "linux-box.local");
    await api.stage("/home/a/repo", ["a.txt"]);
    expect(invoke.mock.calls[0][0]).toBe("remote_call");
    expect(invoke.mock.calls[0][1]).toMatchObject({ method: "stage_files" });
  });

  it("stop going to the helper once the repository is no longer remote", async () => {
    setRemoteHost("/home/a/repo", "linux-box.local");
    setRemoteHost("/home/a/repo", null);
    await api.status("/home/a/repo");
    expect(invoke).toHaveBeenCalledWith("repo_status", { path: "/home/a/repo" });
  });
});
