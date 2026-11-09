import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import type {
  BlameLine,
  BranchDeleteResult,
  MergeApplied,
  MergePreview,
  BranchHistory,
  Commit,
  CommitDetail,
  ConflictFile,
  FileCommit,
  FileEntry,
  GitInfo,
  MergedBranch,
  OpResult,
  PersistedState,
  RebaseCommit,
  RebaseStep,
  RefInfo,
  RepoStatus,
  StashEntry,
  Submodule,
} from "./types";

export const api = {
  // repository
  /** The operating system's folder picker, shown by the backend. Only folders chosen here (or opened before) can be used. */
  pickFolder: (title: string, purpose: "repo" | "destination") =>
    invoke<string | null>("pick_folder", { title, purpose }),
  toplevel: (path: string) => invoke<string>("git_toplevel", { path }),
  status: (path: string) => invoke<RepoStatus>("repo_status", { path }),
  submodules: (path: string) => invoke<Submodule[]>("list_submodules", { path }),
  initSubmodule: (path: string, submodule: string) =>
    invoke<OpResult>("init_submodule", { path, submodule }),
  switchSubmodulesBranch: (path: string, branch: string) =>
    invoke<OpResult>("switch_submodules_branch", { path, branch }),
  updateSubmodules: (path: string, remote: boolean) =>
    invoke<OpResult>("update_submodules", { path, remote }),
  clone: (id: string, url: string, dest: string, recursive: boolean) =>
    invoke<{ path: string; warning: string | null }>("clone_repo", { id, url, dest, recursive }),

  // working copy
  stage: (path: string, files: string[]) => invoke<void>("stage_files", { path, files }),
  unstage: (path: string, files: string[]) => invoke<void>("unstage_files", { path, files }),
  stageAll: (path: string) => invoke<void>("stage_all", { path }),
  unstageAll: (path: string) => invoke<void>("unstage_all", { path }),
  discard: (path: string, tracked: string[], untracked: string[]) =>
    invoke<void>("discard_files", { path, tracked, untracked }),
  applyPatch: (path: string, patch: string, cached: boolean, reverse: boolean) =>
    invoke<void>("apply_patch", { path, patch, cached, reverse }),
  commit: (path: string, message: string, amend: boolean) =>
    invoke<OpResult>("commit", { path, message, amend }),
  headMessage: (path: string) => invoke<string>("head_message", { path }),

  // diffs & history
  fileDiff: (path: string, file: string, staged: boolean, untracked: boolean, context = 3) =>
    invoke<string>("file_diff", { path, file, staged, untracked, context }),
  commitFiles: (path: string, sha: string, parent: string | null) =>
    invoke<FileEntry[]>("commit_files", { path, sha, parent }),
  commitDiff: (
    path: string,
    sha: string,
    parent: string | null,
    file: string,
    oldFile: string | null,
    context = 3,
  ) => invoke<string>("commit_diff", { path, sha, parent, file, oldFile, context }),
  commitDetail: (path: string, sha: string) => invoke<CommitDetail>("commit_detail", { path, sha }),
  log: (
    path: string,
    skip: number,
    limit: number,
    all: boolean,
    query?: string,
    file?: string,
  ) => invoke<Commit[]>("git_log", { path, skip, limit, all, query: query ?? null, file: file ?? null }),
  fileLog: (path: string, file: string, skip: number, limit: number) =>
    invoke<FileCommit[]>("file_log", { path, file, skip, limit }),
  blame: (path: string, file: string, rev: string | null) => invoke<BlameLine[]>("git_blame", { path, file, rev }),
  refs: (path: string) => invoke<RefInfo[]>("list_refs", { path }),
  remotes: (path: string) => invoke<string[]>("list_remotes", { path }),

  // branches & tags
  checkout: (path: string, target: string, force = false) =>
    invoke<OpResult>("checkout", { path, target, force }),
  checkoutRemote: (path: string, remoteBranch: string) =>
    invoke<OpResult>("checkout_remote", { path, remoteBranch }),
  createBranch: (path: string, name: string, start: string | null, checkout: boolean) =>
    invoke<OpResult>("create_branch", { path, name, start, checkout }),
  renameBranch: (path: string, old: string, name: string) =>
    invoke<OpResult>("rename_branch", { path, old, new: name }),
  deleteBranch: (path: string, name: string, force: boolean) =>
    invoke<OpResult>("delete_branch", { path, name, force }),
  mergedBranches: (path: string, target: string) => invoke<MergedBranch[]>("merged_branches", { path, target }),
  branchHistory: (path: string, name: string, target: string, limit: number) =>
    invoke<BranchHistory>("branch_history", { path, name, target, limit }),
  deleteMergedBranches: (path: string, target: string, names: string[]) =>
    invoke<BranchDeleteResult[]>("delete_merged_branches", { path, target, names }),
  createTag: (path: string, name: string, rev: string | null, message: string | null) =>
    invoke<OpResult>("create_tag", { path, name, rev, message }),
  deleteTag: (path: string, name: string) => invoke<OpResult>("delete_tag", { path, name }),

  // remotes
  fetch: (id: string, path: string, prune: boolean) =>
    invoke<OpResult>("fetch", { id, path, prune }),
  fetchRef: (id: string, path: string, remote: string, refspec: string | null) =>
    invoke<OpResult>("fetch_ref", { id, path, remote, refspec }),
  pull: (id: string, path: string, mode: "merge" | "rebase" | "ff-only") =>
    invoke<OpResult>("pull", { id, path, mode }),
  push: (
    id: string,
    path: string,
    o: { remote?: string; refspec?: string; force?: boolean; setUpstream?: boolean; tags?: boolean },
  ) =>
    invoke<OpResult>("push", {
      id,
      path,
      remote: o.remote ?? null,
      refspec: o.refspec ?? null,
      force: !!o.force,
      setUpstream: !!o.setUpstream,
      tags: !!o.tags,
    }),
  deleteRemoteBranch: (id: string, path: string, remote: string, branch: string) =>
    invoke<OpResult>("delete_remote_branch", { id, path, remote, branch }),

  // merge family
  previewMerge: (path: string, source: string, targets: string[]) =>
    invoke<MergePreview[]>("preview_merge", { path, source, targets }),
  mergeIntoBranches: (path: string, source: string, targets: string[]) =>
    invoke<MergeApplied[]>("merge_into_branches", { path, source, targets }),
  repoRisks: (path: string) => invoke<string[]>("repo_risks", { path }),
  checkMergeResult: (path: string, source: string, target: string) =>
    invoke<string[]>("check_merge_result", { path, source, target }),
  prefilledConflicts: (path: string) => invoke<string[]>("prefilled_conflicts", { path }),
  enableRerere: (path: string) => invoke<boolean>("enable_rerere", { path }),
  mergeReusingResolutions: (path: string, source: string, targets: string[]) =>
    invoke<MergeApplied[]>("merge_reusing_resolutions", { path, source, targets }),
  merge: (path: string, rev: string, noFf: boolean, squash: boolean) =>
    invoke<OpResult>("merge", { path, rev, noFf, squash }),
  rebase: (path: string, onto: string) => invoke<OpResult>("rebase", { path, onto }),
  cherryPick: (path: string, sha: string, mainline: number | null) =>
    invoke<OpResult>("cherry_pick", { path, sha, mainline }),
  revert: (path: string, sha: string, mainline: number | null) =>
    invoke<OpResult>("revert", { path, sha, mainline }),
  reset: (path: string, rev: string, mode: "soft" | "mixed" | "hard") =>
    invoke<OpResult>("reset", { path, rev, mode }),
  sequencer: (path: string, state: string, action: "continue" | "skip" | "abort") =>
    invoke<OpResult>("sequencer_action", { path, state, action }),
  rebaseTodo: (path: string, base: string | null) =>
    invoke<RebaseCommit[]>("rebase_todo", { path, base }),
  rebaseInteractive: (path: string, base: string | null, steps: RebaseStep[]) =>
    invoke<OpResult>("rebase_interactive", { path, base, steps }),

  // stash
  stashes: (path: string) => invoke<StashEntry[]>("stash_list", { path }),
  stashPush: (path: string, message: string | null, includeUntracked: boolean) =>
    invoke<OpResult>("stash_push", { path, message, includeUntracked }),
  stashAction: (path: string, reference: string, action: "apply" | "pop" | "drop") =>
    invoke<OpResult>("stash_action", { path, reference, action }),

  // conflicts
  conflictFile: (path: string, file: string) => invoke<ConflictFile>("conflict_file", { path, file }),
  resolveSide: (path: string, file: string, side: "ours" | "theirs") =>
    invoke<void>("resolve_side", { path, file, side }),
  saveResolution: (path: string, file: string, content: string) =>
    invoke<void>("save_resolution", { path, file, content }),

  removeLockFile: (path: string, lock: string) => invoke<void>("remove_lock_file", { path, lock }),

  // OS
  reveal: (path: string) => invoke<void>("reveal_in_file_manager", { path }),
  terminal: (path: string) => invoke<void>("open_terminal", { path }),
  watch: (path: string) => invoke<void>("watch_repo", { path }),
  unwatch: () => invoke<void>("unwatch_repo"),

  /** The installed app version (from tauri.conf.json). */
  appVersion: () => getVersion(),

  // git executable
  gitInfo: () => invoke<GitInfo>("git_info"),
  /** Saves (and verifies) the git executable to use; null/empty goes back to automatic detection. */
  setGitPath: (path: string | null) => invoke<GitInfo>("set_git_path", { path }),

  // persistence
  async loadState(): Promise<PersistedState | null> {
    const json = await invoke<string | null>("load_state");
    if (!json) return null;
    try {
      return JSON.parse(json.replace(/^﻿/, "")) as PersistedState; // tolerate a UTF-8 BOM
    } catch {
      return null; // a corrupt state file must never block start-up
    }
  },
  saveState: (state: PersistedState) => invoke<void>("save_state", { json: JSON.stringify(state) }),
};

/** Tauri rejects with a plain string; normalise anything thrown into a message. */
export function errorMessage(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export function baseName(path: string): string {
  const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

export function joinPath(dir: string, rel: string): string {
  return `${dir.replace(/[\\/]+$/, "")}/${rel}`;
}
