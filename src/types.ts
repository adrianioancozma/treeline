import type { RecentRepo } from "./lib/recent";
export interface FileChange {
  path: string;
  /** Staged state letter, "." when unchanged. */
  index: string;
  /** Working-tree state letter, "." when unchanged. */
  worktree: string;
  orig_path: string | null;
  untracked: boolean;
  conflicted: boolean;
}

export type RepoState = "merge" | "rebase" | "cherry-pick" | "revert";

export interface RepoStatus {
  branch: string | null;
  head: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  files: FileChange[];
  state: RepoState | null;
}

export interface Submodule {
  path: string;
  sha: string;
  state: "ok" | "uninitialized" | "modified" | "conflict";
  describe: string | null;
}

export interface Commit {
  sha: string;
  parents: string[];
  author: string;
  email: string;
  time: number;
  refs: string[];
  subject: string;
}

export interface CommitDetail {
  message: string;
  author: string;
  email: string;
  time: number;
  committer: string;
  parents: string[];
}

export interface FileEntry {
  status: string;
  path: string;
  old_path: string | null;
}

export interface RefInfo {
  kind: "branch" | "remote" | "tag";
  name: string;
  sha: string;
  upstream: string | null;
  track: string | null;
  head: boolean;
}

export interface StashEntry {
  ref: string;
  message: string;
  time: number;
}

export interface OpResult {
  ok: boolean;
  output: string;
}

export interface ConflictFile {
  work: string;
  ours: string | null;
  theirs: string | null;
  base: string | null;
  /** Git filled the file in from a remembered resolution; it only needs review and staging. */
  prefilled?: boolean;
  /** While merging: the branch being merged in (the "theirs" side). */
  theirsName?: string | null;
}

export interface RebaseCommit {
  sha: string;
  subject: string;
  message: string;
  is_merge: boolean;
}

export interface RebaseStep {
  action: "pick" | "reword" | "edit" | "squash" | "fixup" | "drop";
  sha: string;
  message?: string;
}

export interface Tab {
  id: string;
  /** Repository folder, or null for a "new tab" home page. */
  path: string | null;
  name: string;
  /** The computer the folder is on, for a repository opened over ssh; absent for this computer. */
  host?: string;
}

export interface Bookmark {
  id: string;
  name: string;
  path: string;
}

export type ThemeMode = "system" | "dark" | "light";

export interface PersistedState {
  tabs: Tab[];
  activeId: string | null;
  bookmarks: Bookmark[];
  /** Repositories opened lately, newest first (older saved states have none). */
  recent?: RecentRepo[];
  theme?: ThemeMode;
}

/** What was being attempted when an error occurred; lets the error panel offer a matching (force) fix. */
export type ErrorCtx =
  | { op: "push"; remote: string; refspec: string; branch: string }
  | { op: "delete-branch"; name: string }
  | { op: "checkout"; target: string };

export interface OpProgress {
  id: string;
  line: string;
  percent: number | null;
}

/** Which git the app runs and whether it works. */
export interface GitInfo {
  /** Output of `git --version`; null when git cannot be started. */
  version: string | null;
  /** The executable that will be run. */
  path: string;
  /** The path the user chose, if any. */
  custom: string | null;
}

/** A commit that changed one file; `file` is the file's name at that commit (renames change it). */
export interface FileCommit extends Commit {
  file: string;
}

/** One line of a file and the commit that last changed it. */
export interface BlameLine {
  sha: string;
  author: string;
  time: number;
  summary: string;
  line: number;
  text: string;
}

/** A local branch that is already part of the target branch. */
export interface MergedBranch {
  name: string;
  sha: string;
  /** Checked out right now, so it cannot be deleted. */
  head: boolean;
  /** Its tip is the target's tip: nothing was ever committed on it. */
  atTarget: boolean;
  author: string;
  time: number;
  subject: string;
}

export interface BranchHistory {
  /** The branch's own commits, newest first. */
  commits: Commit[];
  /** The merge commit that took the branch into the target; null when it was fast-forwarded. */
  merge: Commit | null;
  truncated: boolean;
}

export interface MergePreview {
  target: string;
  status: "up-to-date" | "fast-forward" | "clean" | "conflict" | "error";
  files: string[];
  message: string;
}

export interface MergeApplied {
  target: string;
  ok: boolean;
  status: "merged" | "fast-forwarded" | "up-to-date" | "conflict" | "failed";
  message: string;
  /** Files where the merge kept the branch's own version and dropped the source's change. */
  discarded: string[];
}

export interface BranchDeleteResult {
  name: string;
  ok: boolean;
  message: string;
}
