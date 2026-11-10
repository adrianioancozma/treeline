//! Repository operations: staging, commits, history, branches, tags, remotes, stashes,
//! merge / rebase / cherry-pick, and conflict resolution.

use crate::git::{
    blocking, ensure_allowed, opt_ref_arg, ref_arg, run_git, run_git_ok_codes, run_git_stdin, run_op, stream_git, OpResult, LITERAL,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use tauri::AppHandle;
pub use treeline_core::ops_parse::{FileEntry, parse_name_status, Commit, parse_log, RefInfo, parse_refs, StashEntry, parse_stashes, parse_merge_tree, merge_msg_branch, FileCommit, parse_file_log, BlameLine, parse_blame};

fn nul_list(files: &[String]) -> Vec<u8> {
    let mut buf = Vec::new();
    for f in files {
        buf.extend_from_slice(f.as_bytes());
        buf.push(0);
    }
    buf
}

// ---------------------------------------------------------------------------
// Working copy: stage / unstage / discard / commit
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn stage_files(path: String, files: Vec<String>) -> Result<(), String> {
    blocking(move || {
        ensure_allowed(&path)?;
        if files.is_empty() {
            return Ok(());
        }
        run_git_stdin(
            &path,
            [LITERAL, "add", "-A", "--pathspec-from-file=-", "--pathspec-file-nul"],
            &nul_list(&files),
        )
        .map(|_| ())
    })
    .await
}

#[tauri::command]
pub async fn unstage_files(path: String, files: Vec<String>) -> Result<(), String> {
    blocking(move || {
        ensure_allowed(&path)?;
        if files.is_empty() {
            return Ok(());
        }
        // `reset` (unlike `restore --staged`) also works before the first commit.
        run_git_stdin(
            &path,
            [LITERAL, "reset", "-q", "--pathspec-from-file=-", "--pathspec-file-nul"],
            &nul_list(&files),
        )
        .map(|_| ())
    })
    .await
}

#[tauri::command]
pub async fn stage_all(path: String) -> Result<(), String> {
    blocking(move || {
        ensure_allowed(&path)?;
        run_git(&path, ["add", "-A"]).map(|_| ())
    }).await
}

#[tauri::command]
pub async fn unstage_all(path: String) -> Result<(), String> {
    blocking(move || {
        ensure_allowed(&path)?;
        run_git(&path, ["reset", "-q"]).map(|_| ())
    }).await
}

/// Throws away working-tree changes: `tracked` files are restored, `untracked` ones deleted.
#[tauri::command]
pub async fn discard_files(
    path: String,
    tracked: Vec<String>,
    untracked: Vec<String>,
) -> Result<(), String> {
    blocking(move || {
        ensure_allowed(&path)?;
        if !tracked.is_empty() {
            run_git_stdin(
                &path,
                [LITERAL, "restore", "--worktree", "--pathspec-from-file=-", "--pathspec-file-nul"],
                &nul_list(&tracked),
            )?;
        }
        for chunk in untracked.chunks(50) {
            let mut args = vec![LITERAL.to_string(), "clean".to_string(), "-fdq".to_string(), "--".to_string()];
            args.extend(chunk.iter().cloned());
            run_git(&path, args)?;
        }
        Ok(())
    })
    .await
}

/// Applies a unified diff to the index (stage/unstage a hunk or lines) or the working tree.
#[tauri::command]
pub async fn apply_patch(
    path: String,
    patch: String,
    cached: bool,
    reverse: bool,
) -> Result<(), String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let mut args = vec!["apply", "--recount", "--whitespace=nowarn"];
        if cached {
            args.push("--cached");
        }
        if reverse {
            args.push("--reverse");
        }
        args.push("-");
        run_git_stdin(&path, args, patch.as_bytes()).map(|_| ())
    })
    .await
}

#[tauri::command]
pub async fn commit(path: String, message: String, amend: bool) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let mut args = vec!["commit".to_string()];
        if amend {
            args.push("--amend".into());
        }
        if amend && message.trim().is_empty() {
            args.push("--no-edit".into());
            return run_op(&path, args, &[]);
        }
        args.extend(["-F".to_string(), "-".to_string()]);
        let out = run_git_stdin(&path, args, message.as_bytes());
        Ok(match out {
            Ok(o) => OpResult { ok: true, output: o.trim().to_string() },
            Err(e) => OpResult { ok: false, output: e },
        })
    })
    .await
}

#[tauri::command]
pub async fn head_message(path: String) -> Result<String, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        Ok(run_git(&path, ["log", "-1", "--format=%B"])
            .unwrap_or_default()
            .trim()
            .to_string())
    })
    .await
}

// ---------------------------------------------------------------------------
// Diffs
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn file_diff(
    path: String,
    file: String,
    staged: bool,
    untracked: bool,
    context: u32,
) -> Result<String, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let ctx = format!("-U{context}");
        if untracked {
            return run_git_ok_codes(
                &path,
                [LITERAL, "diff", "--no-index", "--no-ext-diff", "--no-textconv", "--no-color", &ctx, "--", "/dev/null", &file],
                &[1],
            );
        }
        let mut args = vec![LITERAL, "diff", "--no-ext-diff", "--no-textconv", "--no-color", &ctx];
        if staged {
            args.push("--cached");
        }
        args.extend(["--", &file]);
        run_git(&path, args)
    })
    .await
}

/// Files changed by `sha` relative to `parent` (the first parent; `None` for a root commit).
#[tauri::command]
pub async fn commit_files(
    path: String,
    sha: String,
    parent: Option<String>,
) -> Result<Vec<FileEntry>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&sha)?;
        opt_ref_arg(&parent)?;
        let raw = match &parent {
            Some(p) => run_git(&path, ["diff", "--name-status", "-M", "-z", p, &sha])?,
            None => run_git(
                &path,
                ["diff-tree", "--root", "--no-commit-id", "--name-status", "-r", "-M", "-z", &sha],
            )?,
        };
        Ok(parse_name_status(&raw))
    })
    .await
}

#[tauri::command]
pub async fn commit_diff(
    path: String,
    sha: String,
    parent: Option<String>,
    file: String,
    old_file: Option<String>,
    context: u32,
) -> Result<String, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&sha)?;
        opt_ref_arg(&parent)?;
        let ctx = format!("-U{context}");
        let mut specs = vec![file.clone()];
        if let Some(o) = old_file {
            specs.push(o);
        }
        let mut args: Vec<String> = match &parent {
            Some(p) => vec![LITERAL.into(), "diff".into(), "--no-ext-diff".into(), "--no-textconv".into(), "--no-color".into(), "-M".into(), ctx, p.clone(), sha],
            None => vec![LITERAL.into(), "show".into(), "--format=".into(), "--no-ext-diff".into(), "--no-textconv".into(), "--no-color".into(), "--root".into(), ctx, sha],
        };
        args.push("--".into());
        args.extend(specs);
        run_git(&path, args)
    })
    .await
}

#[derive(Serialize)]
pub struct CommitDetail {
    pub message: String,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub committer: String,
    pub parents: Vec<String>,
}

#[tauri::command]
pub async fn commit_detail(path: String, sha: String) -> Result<CommitDetail, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&sha)?;
        let raw = run_git(
            &path,
            ["show", "-s", "--format=%an%x1f%ae%x1f%at%x1f%cn%x1f%P%x1f%B", &sha],
        )?;
        let f: Vec<&str> = raw.splitn(6, '\x1f').collect();
        if f.len() < 6 {
            return Err("Unexpected output from git show".into());
        }
        Ok(CommitDetail {
            author: f[0].into(),
            email: f[1].into(),
            time: f[2].parse().unwrap_or(0),
            committer: f[3].into(),
            parents: f[4].split_whitespace().map(String::from).collect(),
            message: f[5].trim().to_string(),
        })
    })
    .await
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

const LOG_FORMAT: &str = "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%D%x1f%s%x1e";

#[tauri::command]
pub async fn git_log(
    path: String,
    skip: u32,
    limit: u32,
    all: bool,
    query: Option<String>,
    file: Option<String>,
) -> Result<Vec<Commit>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let mut args: Vec<String> = vec![
            LITERAL.into(),
            "log".into(),
            "--topo-order".into(),
            "--no-color".into(),
            "--decorate=short".into(),
            LOG_FORMAT.into(),
            format!("--skip={skip}"),
            format!("-n{limit}"),
        ];
        if all && file.is_none() {
            // Stashes are shown in the sidebar; their internal commits would only clutter the graph.
            args.push("--exclude=refs/stash".into());
            args.push("--all".into());
        }
        if let Some(q) = query.filter(|q| !q.trim().is_empty()) {
            // Plain text match: "[WIP]" or "a.b" must not be interpreted (or rejected) as a regex.
            args.push("--fixed-strings".into());
            args.push("-i".into());
            args.push(format!("--grep={q}"));
        }
        if let Some(f) = file {
            args.push("--follow".into());
            args.push("--".into());
            args.push(f);
        }
        // A repository with no commits yet makes `git log` fail; that is just "no history".
        match run_git(&path, args) {
            Ok(raw) => Ok(parse_log(&raw)),
            Err(e) if e.contains("does not have any commits") || e.contains("bad default revision") => {
                Ok(Vec::new())
            }
            Err(e) => Err(e),
        }
    })
    .await
}

/// Which commits the History filters select. Empty fields do not filter.
#[derive(Deserialize, Debug, Default)]
pub struct LogFilter {
    /// Matches the author's name or e-mail (plain text, any case).
    pub author: Option<String>,
    /// Matches the message (plain text, any case) or the start of the commit id.
    pub text: Option<String>,
    /// ISO 8601 time with an offset, e.g. `2026-11-01T09:30:00+02:00`.
    pub since: Option<String>,
    pub until: Option<String>,
}

fn non_empty(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

/// Dates reach Git as one `--since=` argument; only the characters of an ISO 8601 time are allowed.
fn iso_time(v: &str) -> Result<&str, String> {
    let ok = v.len() <= 40
        && v.chars().all(|c| c.is_ascii_digit() || "T:+-Z .".contains(c))
        && v.starts_with(|c: char| c.is_ascii_digit());
    if ok {
        Ok(v)
    } else {
        Err(format!("Not a valid date: {v}"))
    }
}

/// `git log` arguments that print the ids of the commits matching every filter (AND), over the same
/// set of commits the history list shows. The message filter is left out when `with_text` is false.
fn log_filter_args(all: bool, f: &LogFilter, with_text: bool) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = vec![LITERAL.into(), "log".into(), "--no-color".into(), "--format=%H".into()];
    if all {
        args.push("--exclude=refs/stash".into());
        args.push("--all".into());
    }
    // Plain text, not a pattern: "a.b" or "[WIP]" must match literally.
    args.push("--fixed-strings".into());
    args.push("--regexp-ignore-case".into());
    if let Some(a) = non_empty(&f.author) {
        args.push(format!("--author={a}"));
    }
    if with_text {
        if let Some(t) = non_empty(&f.text) {
            args.push(format!("--grep={t}"));
        }
    }
    if let Some(v) = non_empty(&f.since) {
        args.push(format!("--since={}", iso_time(v)?));
    }
    if let Some(v) = non_empty(&f.until) {
        args.push(format!("--until={}", iso_time(v)?));
    }
    Ok(args)
}

/// The ids of every commit matching the History filters. The list itself is not filtered (so the
/// graph stays exactly as it is); the view uses these ids to dim the rest.
#[tauri::command]
pub async fn git_log_matches(path: String, all: bool, filter: LogFilter) -> Result<Vec<String>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let run = |args: Vec<String>| match run_git(&path, args) {
            Ok(raw) => Ok(raw.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect::<Vec<_>>()),
            Err(e) if e.contains("does not have any commits") || e.contains("bad default revision") => Ok(Vec::new()),
            Err(e) => Err(e),
        };
        let mut ids = run(log_filter_args(all, &filter, true)?)?;
        // "a1b2c3" also finds the commit with that id (the other filters still apply).
        if let Some(t) = non_empty(&filter.text) {
            let t = t.to_ascii_lowercase();
            if t.len() >= 4 && is_hex_id(&t) {
                let seen: std::collections::HashSet<String> = ids.iter().cloned().collect();
                for id in run(log_filter_args(all, &filter, false)?)? {
                    if id.starts_with(&t) && !seen.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
        Ok(ids)
    })
    .await
}

// ---------------------------------------------------------------------------
// Refs: branches, remote branches, tags
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_refs(path: String) -> Result<Vec<RefInfo>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let raw = run_git(
            &path,
            [
                "for-each-ref",
                "--format=%(refname)%1f%(refname:short)%1f%(objectname)%1f%(upstream:short)%1f%(upstream:track)%1f%(HEAD)%1f%(*objectname)",
                "refs/heads",
                "refs/remotes",
                "refs/tags",
            ],
        )?;
        Ok(parse_refs(&raw))
    })
    .await
}

#[tauri::command]
pub async fn checkout(path: String, target: String, force: bool) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&target)?;
        // `git checkout <remote-branch-short-name>` DWIM-creates a tracking branch.
        let mut args = vec!["checkout"];
        if force {
            args.push("--force");
        }
        args.push(&target);
        run_op(&path, args, &[])
    })
    .await
}

/// Checks out a remote branch such as `origin/feature` as a local tracking branch.
#[tauri::command]
pub async fn checkout_remote(path: String, remote_branch: String) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&remote_branch)?;
        let local = remote_branch
            .split_once('/')
            .map(|(_, b)| b.to_string())
            .unwrap_or_else(|| remote_branch.clone());
        let exists = run_git(&path, ["rev-parse", "--verify", "-q", &format!("refs/heads/{local}")]).is_ok();
        if exists {
            let out = run_op(&path, ["checkout", &local], &[])?;
            // An existing local branch with no upstream starts tracking the one it was checked out from.
            let has_upstream = run_op(&path, ["rev-parse", "--abbrev-ref", &format!("{local}@{{u}}")], &[])?.ok;
            if out.ok && !has_upstream {
                run_op(&path, ["branch", "--set-upstream-to", &remote_branch, &local], &[])?;
            }
            Ok(out)
        } else {
            run_op(&path, ["checkout", "--track", "-b", &local, &remote_branch], &[])
        }
    })
    .await
}

/// Sets which remote branch a local branch tracks; `upstream: None` stops it tracking anything.
#[tauri::command]
pub async fn set_upstream(path: String, branch: String, upstream: Option<String>) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&branch)?;
        opt_ref_arg(&upstream)?;
        match upstream.filter(|u| !u.is_empty()) {
            Some(u) => run_op(&path, ["branch", "--set-upstream-to", &u, &branch], &[]),
            None => run_op(&path, ["branch", "--unset-upstream", &branch], &[]),
        }
    })
    .await
}

#[tauri::command]
pub async fn create_branch(
    path: String,
    name: String,
    start: Option<String>,
    checkout: bool,
) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&name)?;
        opt_ref_arg(&start)?;
        let mut args = if checkout {
            vec!["checkout".to_string(), "-b".into(), name]
        } else {
            vec!["branch".to_string(), name]
        };
        if let Some(s) = start.filter(|s| !s.is_empty()) {
            args.push(s);
        }
        run_op(&path, args, &[])
    })
    .await
}

#[tauri::command]
pub async fn rename_branch(path: String, old: String, new: String) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&old)?;
        ref_arg(&new)?;
        run_op(&path, ["branch", "-m", &old, &new], &[])
    }).await
}

#[tauri::command]
pub async fn delete_branch(path: String, name: String, force: bool) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&name)?;
        run_op(&path, ["branch", if force { "-D" } else { "-d" }, &name], &[])
    }).await
}

// ---------------------------------------------------------------------------
// Merged branches: which local branches are already part of a target branch
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug)]
pub struct MergedBranch {
    pub name: String,
    /// Tip commit of the branch.
    pub sha: String,
    /// Currently checked out, so it cannot be deleted.
    pub head: bool,
    /// The branch tip *is* the target tip: nothing was ever committed on it, so "merged" is trivial.
    pub at_target: bool,
    pub author: String,
    pub time: i64,
    pub subject: String,
}

#[derive(Serialize, Debug)]
pub struct BranchHistory {
    /// The branch's own commits, newest first.
    pub commits: Vec<Commit>,
    /// The merge commit that brought the branch into the target; `None` when it was fast-forwarded
    /// (no merge commit exists) or has no commits of its own.
    pub merge: Option<Commit>,
    /// More commits exist than were returned.
    pub truncated: bool,
}

#[derive(Serialize, Debug)]
pub struct BranchDeleteResult {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

/// Resolves a branch name or commit to its full sha, or explains that it does not exist.
fn resolve_commit(path: &str, rev: &str) -> Result<String, String> {
    let missing = || format!("'{rev}' is not a branch or commit in this repository.");
    if rev.is_empty() || rev.starts_with('-') {
        return Err(missing());
    }
    let spec = format!("{rev}^{{commit}}");
    run_git(path, [LITERAL, "rev-parse", "--verify", "--quiet", &spec])
        .map(|s| s.trim().to_string())
        .map_err(|_| missing())
}

/// Local branches whose tip is already reachable from `target` (`git branch --merged`), newest first.
#[tauri::command]
pub async fn merged_branches(path: String, target: String) -> Result<Vec<MergedBranch>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let target_sha = resolve_commit(&path, &target)?;
        let raw = run_git(
            &path,
            [
                LITERAL,
                "for-each-ref",
                "--merged",
                &target_sha,
                "--sort=-committerdate",
                "--format=%(refname)%1f%(objectname)%1f%(HEAD)%1f%(authorname)%1f%(committerdate:unix)%1f%(contents:subject)",
                "refs/heads",
            ],
        )?;
        Ok(raw
            .lines()
            .filter_map(|line| {
                let f: Vec<&str> = line.split('\x1f').collect();
                if f.len() < 6 {
                    return None;
                }
                let name = f[0].strip_prefix("refs/heads/")?.to_string();
                if name == target {
                    return None;
                }
                Some(MergedBranch {
                    at_target: f[1] == target_sha,
                    sha: f[1].into(),
                    head: f[2] == "*",
                    author: f[3].into(),
                    time: f[4].parse().unwrap_or(0),
                    subject: f[5].into(),
                    name,
                })
            })
            .collect())
    })
    .await
}

/// The commits a merged branch contributed, and the merge that brought it into `target`.
#[tauri::command]
pub async fn branch_history(path: String, name: String, target: String, limit: u32) -> Result<BranchHistory, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let target_sha = resolve_commit(&path, &target)?;
        let branch = format!("refs/heads/{name}");
        let tip = resolve_commit(&path, &branch)?;
        let log = |extra: Vec<String>| -> Result<Vec<Commit>, String> {
            let mut args: Vec<String> = vec![LITERAL.into(), "log".into(), "--topo-order".into(), "--no-color".into(), "--decorate=short".into(), LOG_FORMAT.into()];
            args.extend(extra);
            Ok(parse_log(&run_git(&path, args)?))
        };

        // The first merge commit between the branch tip and the target is the one that took it in.
        let merge_sha = if tip == target_sha {
            None
        } else {
            let range = format!("{tip}..{target_sha}");
            run_git(&path, [LITERAL, "log", "--ancestry-path", "--merges", "--reverse", "--format=%H", &range])?
                .lines()
                .next()
                .map(str::to_string)
        };
        let merge = match &merge_sha {
            Some(m) => log(vec!["-n1".into(), m.clone()])?.into_iter().next(),
            None => None,
        };

        // With a merge commit, the branch's own work is everything its second parent adds over the
        // first. Without one (fast-forward) the fork point is unknowable, so show the latest commits.
        let range = match &merge_sha {
            Some(m) => format!("{m}^1..{tip}"),
            None => tip.clone(),
        };
        let mut commits = log(vec![format!("-n{}", limit + 1), range])?;
        let truncated = commits.len() > limit as usize;
        commits.truncate(limit as usize);
        Ok(BranchHistory { commits, merge, truncated })
    })
    .await
}

/// Deletes local branches that are merged into `target`. Each branch is checked again right before
/// deleting, so a branch that gained commits since the list was built is kept.
#[tauri::command]
pub async fn delete_merged_branches(path: String, target: String, names: Vec<String>) -> Result<Vec<BranchDeleteResult>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        for item in &names {
            ref_arg(item)?;
        }
        let target_sha = resolve_commit(&path, &target)?;
        let mut results = Vec::new();
        for name in names {
            let full = format!("refs/heads/{name}");
            let merged = run_git(&path, [LITERAL, "merge-base", "--is-ancestor", &full, &target_sha]).is_ok();
            let (ok, message) = if merged {
                // `-D` because plain `-d` compares with the upstream or HEAD, not with `target`.
                match run_op(&path, ["branch", "-D", &name], &[]) {
                    Ok(r) => (r.ok, r.output.trim().to_string()),
                    Err(e) => (false, e),
                }
            } else {
                (false, format!("{name} is not merged into {target} any more, so it was kept."))
            };
            results.push(BranchDeleteResult { name, ok, message });
        }
        Ok(results)
    })
    .await
}

#[tauri::command]
pub async fn create_tag(
    path: String,
    name: String,
    rev: Option<String>,
    message: Option<String>,
) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&name)?;
        opt_ref_arg(&rev)?;
        let mut args = vec!["tag".to_string()];
        if let Some(m) = message.filter(|m| !m.trim().is_empty()) {
            args.extend(["-a".to_string(), "-m".to_string(), m]);
        }
        args.push(name);
        if let Some(r) = rev.filter(|r| !r.is_empty()) {
            args.push(r);
        }
        run_op(&path, args, &[])
    })
    .await
}

#[tauri::command]
pub async fn delete_tag(path: String, name: String) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&name)?;
        run_op(&path, ["tag", "-d", &name], &[])
    }).await
}

// ---------------------------------------------------------------------------
// Remotes: fetch / pull / push
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn fetch(app: AppHandle, id: String, path: String, prune: bool) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let mut args: Vec<String> = vec!["fetch".into(), "--all".into(), "--progress".into()];
        if prune {
            args.push("--prune".into());
        }
        stream_git(&crate::git::WindowProgress(&app), &id, Some(&path), &args)
    })
    .await
}

/// Fetches one ref (or everything from one remote when `refspec` is empty).
/// A refspec such as `feature:feature` fast-forwards a local branch without checking it out;
/// git refuses it unless it is a fast-forward, so local work is never overwritten.
#[tauri::command]
pub async fn fetch_ref(
    app: AppHandle,
    id: String,
    path: String,
    remote: String,
    refspec: Option<String>,
) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&remote)?;
        opt_ref_arg(&refspec)?;
        let mut args: Vec<String> = vec!["fetch".into(), "--progress".into(), remote];
        if let Some(r) = refspec.filter(|r| !r.is_empty()) {
            args.push(r);
        }
        stream_git(&crate::git::WindowProgress(&app), &id, Some(&path), &args)
    })
    .await
}

/// `mode` is `merge`, `rebase` or `ff-only`.
#[tauri::command]
pub async fn pull(app: AppHandle, id: String, path: String, mode: String) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let mut args: Vec<String> = vec!["pull".into(), "--progress".into()];
        match mode.as_str() {
            // A dirty tree would otherwise make `pull --rebase` refuse to start.
            "rebase" => args.extend(["--rebase".to_string(), "--autostash".to_string()]),
            "ff-only" => args.push("--ff-only".into()),
            _ => args.push("--no-rebase".into()),
        }
        stream_git(&crate::git::WindowProgress(&app), &id, Some(&path), &args)
    })
    .await
}

#[tauri::command]
pub async fn push(
    app: AppHandle,
    id: String,
    path: String,
    remote: Option<String>,
    refspec: Option<String>,
    force: bool,
    set_upstream: bool,
    tags: bool,
) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        opt_ref_arg(&remote)?;
        opt_ref_arg(&refspec)?;
        let mut args: Vec<String> = vec!["push".into(), "--progress".into()];
        if force {
            args.push("--force-with-lease".into());
        }
        if set_upstream {
            args.push("--set-upstream".into());
        }
        if tags {
            args.push("--tags".into());
        }
        if let Some(r) = remote.filter(|r| !r.is_empty()) {
            args.push(r);
            if let Some(s) = refspec.filter(|s| !s.is_empty()) {
                args.push(s);
            }
        }
        stream_git(&crate::git::WindowProgress(&app), &id, Some(&path), &args)
    })
    .await
}

#[tauri::command]
pub async fn list_remotes(path: String) -> Result<Vec<String>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        Ok(run_git(&path, ["remote"])?
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn delete_remote_branch(
    app: AppHandle,
    id: String,
    path: String,
    remote: String,
    branch: String,
) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&remote)?;
        ref_arg(&branch)?;
        let args: Vec<String> = vec!["push".into(), remote, "--delete".into(), branch];
        stream_git(&crate::git::WindowProgress(&app), &id, Some(&path), &args)
    })
    .await
}

// ---------------------------------------------------------------------------
// Merge / rebase / cherry-pick / revert / reset
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn merge(path: String, rev: String, no_ff: bool, squash: bool) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&rev)?;
        let mut args = vec!["merge"];
        if squash {
            args.push("--squash");
        } else if no_ff {
            args.push("--no-ff");
        }
        args.extend(["--no-edit", &rev]);
        run_op(&path, args, &[("GIT_EDITOR", "true".into())])
    })
    .await
}

#[derive(Serialize, Debug, Clone)]
pub struct MergePreview {
    pub target: String,
    /// `up-to-date`, `fast-forward`, `clean`, `conflict` or `error`.
    pub status: String,
    /// Files that would conflict (only for `conflict`).
    pub files: Vec<String>,
    /// Explanation for `error`, or a note such as "checked out in another worktree".
    pub message: String,
}

#[derive(Serialize, Debug)]
pub struct MergeApplied {
    pub target: String,
    pub ok: bool,
    /// What happened: `merged`, `fast-forwarded`, `up-to-date`, `conflict` (left untouched) or `failed`.
    pub status: String,
    pub message: String,
    /// Files where the merge result kept the target branch's own version and dropped the source's
    /// change (for example "Keep ours" while resolving). Empty for a normal merge.
    pub discarded: Vec<String>,
}

/// Branches checked out in a worktree other than the one at `path` (updating them would desync it).
fn other_worktree_branches(path: &str) -> Vec<String> {
    let raw = run_git(path, ["worktree", "list", "--porcelain"]).unwrap_or_default();
    let mut out = Vec::new();
    let mut first = true;
    for block in raw.split("\n\n") {
        let branch = block.lines().find_map(|l| l.strip_prefix("branch refs/heads/"));
        if !first {
            if let Some(b) = branch {
                out.push(b.to_string());
            }
        }
        first = false;
    }
    out
}

/// Dry-runs merging `source_sha` into `target` without touching any working tree.
fn classify_merge(path: &str, source_sha: &str, target: &str) -> MergePreview {
    let mk = |status: &str, files: Vec<String>, message: &str| MergePreview {
        target: target.to_string(),
        status: status.into(),
        files,
        message: message.into(),
    };
    let target_ref = format!("refs/heads/{target}");
    let target_sha = match run_git(path, ["rev-parse", "--verify", "--quiet", &target_ref]) {
        Ok(s) => s.trim().to_string(),
        Err(_) => return mk("error", vec![], "Branch not found."),
    };
    if run_git(path, ["merge-base", "--is-ancestor", source_sha, &target_sha]).is_ok() {
        return mk("up-to-date", vec![], "");
    }
    if run_git(path, ["merge-base", "--is-ancestor", &target_sha, source_sha]).is_ok() {
        return mk("fast-forward", vec![], "");
    }
    match run_git_ok_codes(
        path,
        ["merge-tree", "--write-tree", "--name-only", "--no-messages", &target_sha, source_sha],
        &[1],
    ) {
        Ok(out) => {
            let (_, files) = parse_merge_tree(&out);
            if files.is_empty() { mk("clean", vec![], "") } else { mk("conflict", files, "") }
        }
        Err(e) => mk("error", vec![], &e),
    }
}

/// Tests merging `source` into each target branch without changing anything, so the user can see
/// which merges are clean and which would conflict (and in which files) before applying them.
#[tauri::command]
pub async fn preview_merge(path: String, source: String, targets: Vec<String>) -> Result<Vec<MergePreview>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let sha = resolve_commit(&path, &source)?;
        let busy = other_worktree_branches(&path);
        let out = std::thread::scope(|scope| {
            let handles: Vec<_> = targets
                .iter()
                .map(|t| {
                    let (path, sha, busy) = (&path, &sha, &busy);
                    scope.spawn(move || {
                        let mut p = classify_merge(path, sha, t);
                        if busy.contains(t) && matches!(p.status.as_str(), "clean" | "fast-forward") {
                            p.status = "error".into();
                            p.message = "Checked out in another worktree; merge it there.".into();
                        }
                        p
                    })
                })
                .collect();
            handles.into_iter().zip(&targets).map(|(h, t)| h.join().unwrap_or_else(|_| MergePreview {
                target: t.clone(), status: "error".into(), files: vec![], message: "Preview crashed.".into(),
            })).collect::<Vec<_>>()
        });
        Ok(out)
    })
    .await
}

/// Merges `source` into every target branch. Targets are re-checked first; any that would now
/// conflict are left untouched. Branches that are not checked out are updated directly (no
/// checkout, no working-tree changes); the checked-out branch gets a normal `git merge`.
#[tauri::command]
pub async fn merge_into_branches(path: String, source: String, targets: Vec<String>) -> Result<Vec<MergeApplied>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        for item in &targets {
            ref_arg(item)?;
        }
        let sha = resolve_commit(&path, &source)?;
        let current = run_git(&path, ["symbolic-ref", "--quiet", "--short", "HEAD"]).map(|s| s.trim().to_string()).unwrap_or_default();
        let busy = other_worktree_branches(&path);
        let mut results = Vec::new();
        for t in targets {
            let r = |ok: bool, status: &str, message: String| MergeApplied { target: t.clone(), ok, status: status.into(), message, discarded: vec![] };
            if busy.contains(&t) {
                results.push(r(false, "failed", "Checked out in another worktree; merge it there.".into()));
                continue;
            }
            let p = classify_merge(&path, &sha, &t);
            let target_ref = format!("refs/heads/{t}");
            let msg = format!("Merge branch '{source}' into {t}");
            let res = match p.status.as_str() {
                "up-to-date" => r(true, "up-to-date", "Already contains the source.".into()),
                "conflict" => r(false, "conflict", format!("Would conflict in {} file(s); left unchanged.", p.files.len())),
                "error" => r(false, "failed", p.message),
                status => {
                    let ff = status == "fast-forward";
                    if t == current {
                        let mut args = vec!["merge", "--no-edit"];
                        if ff { args.push("--ff-only"); }
                        args.push(&sha);
                        match run_op(&path, args, &[("GIT_EDITOR", "true".into())]) {
                            Ok(o) if o.ok => r(true, if ff { "fast-forwarded" } else { "merged" }, String::new()),
                            Ok(o) => {
                                let _ = run_git(&path, ["merge", "--abort"]);
                                r(false, "failed", o.output.trim().to_string())
                            }
                            Err(e) => r(false, "failed", e),
                        }
                    } else {
                        let old = run_git(&path, ["rev-parse", &target_ref]).map(|s| s.trim().to_string());
                        let new = old.as_ref().map_err(|e| e.clone()).and_then(|old| {
                            if ff {
                                Ok(sha.clone())
                            } else {
                                let tree = run_git_ok_codes(&path, ["merge-tree", "--write-tree", "--no-messages", old, &sha], &[])
                                    .map(|o| o.lines().next().unwrap_or("").trim().to_string())?;
                                run_git(&path, ["commit-tree", &tree, "-p", old, "-p", &sha, "-m", &msg]).map(|s| s.trim().to_string())
                            }
                        });
                        match (old, new) {
                            (Ok(old), Ok(new)) => match run_git(&path, ["update-ref", "-m", &msg, &target_ref, &new, &old]) {
                                Ok(_) => r(true, if ff { "fast-forwarded" } else { "merged" }, String::new()),
                                Err(e) => r(false, "failed", e),
                            },
                            (Err(e), _) | (_, Err(e)) => r(false, "failed", e),
                        }
                    }
                }
            };
            results.push(res);
        }
        Ok(results)
    })
    .await
}

/// Files where the merge commit at the tip of `target` (whose second parent is `source_sha`) kept the
/// branch's own version and dropped the source's change. Empty when the tip is not such a merge.
pub(crate) fn discarded_by_merge(path: &str, source_sha: &str, target: &str) -> Vec<String> {
    let names = |a: &str, b: &str| -> Vec<String> {
        run_git(path, ["diff", "--name-only", "-z", a, b])
            .unwrap_or_default()
            .split('\0')
            .filter(|f| !f.is_empty())
            .map(str::to_string)
            .collect()
    };
    let tip_ref = format!("refs/heads/{target}");
    let Ok(line) = run_git(path, ["rev-list", "--parents", "-n", "1", &tip_ref]) else { return vec![] };
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() != 3 || parts[2] != source_sha {
        return vec![];
    }
    let (tip, p1, p2) = (parts[0], parts[1], parts[2]);
    let Ok(base) = run_git(path, ["merge-base", p1, p2]) else { return vec![] };
    let changed_by_source = names(base.trim(), p2);
    let differs_from_ours = names(p1, tip);
    let differs_from_source = names(p2, tip);
    changed_by_source
        .into_iter()
        .filter(|f| !differs_from_ours.contains(f) && differs_from_source.contains(f))
        .collect()
}

/// After a hand-resolved merge: did the result drop the source's changes in some files?
#[tauri::command]
pub async fn check_merge_result(path: String, source: String, target: String) -> Result<Vec<String>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let sha = resolve_commit(&path, &source)?;
        Ok(discarded_by_merge(&path, &sha, &target))
    })
    .await
}

/// True when git (rerere) already filled in `file` from an earlier resolution: it is still unmerged
/// but its content has no conflict markers. Only meaningful for a file that is currently unmerged.
fn rerere_prefilled(path: &str, file: &str, work: &str) -> bool {
    if work.lines().any(|l| l.starts_with("<<<<<<<")) {
        return false;
    }
    // `rerere remaining` lists unmerged paths rerere could NOT fill in (including delete/modify
    // conflicts), so an unmerged path with no markers that is absent from it was auto-resolved.
    run_git(path, ["-c", "rerere.enabled=true", "rerere", "remaining"])
        .map(|s| !s.lines().any(|l| l.trim() == file))
        .unwrap_or(false)
}

/// Conflicted files that git has already filled in from a remembered resolution and that still need
/// to be reviewed and staged.
#[tauri::command]
pub async fn prefilled_conflicts(path: String) -> Result<Vec<String>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let raw = run_git(&path, ["diff", "--name-only", "-z", "--diff-filter=U"])?;
        Ok(raw
            .split('\0')
            .filter(|f| !f.is_empty())
            .filter(|f| {
                safe_real_path(&path, f)
                    .ok()
                    .and_then(|full| fs::read_to_string(full).ok())
                    .map(|w| rerere_prefilled(&path, f, &w))
                    .unwrap_or(false)
            })
            .map(str::to_string)
            .collect())
    })
    .await
}

/// Turns on `rerere` (reuse recorded resolution) for this repository so that git remembers how
/// conflicts are resolved. Returns true when it was switched on now, false if already enabled.
#[tauri::command]
pub async fn enable_rerere(path: String) -> Result<bool, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let current = run_git(&path, ["config", "--get", "rerere.enabled"]).unwrap_or_default();
        if current.trim() == "true" {
            return Ok(false);
        }
        run_git(&path, ["config", "--local", "rerere.enabled", "true"])?;
        Ok(true)
    })
    .await
}

/// Merges `source` into `target` by checking `target` out and letting rerere replay recorded
/// resolutions. Succeeds only when every conflict has a recorded resolution; otherwise the merge is
/// aborted and the branch is left exactly as it was. Always returns to the branch that was checked out.
fn merge_reusing_resolution(path: &str, source_sha: &str, target: &str) -> MergeApplied {
    let res = |ok: bool, status: &str, message: &str| MergeApplied {
        target: target.to_string(),
        ok,
        status: status.into(),
        message: message.into(),
        discarded: vec![],
    };
    let env = [("GIT_EDITOR", "true".to_string())];
    if !run_git(path, ["status", "--porcelain", "--untracked-files=no"]).map(|s| s.trim().is_empty()).unwrap_or(false) {
        return res(false, "failed", "Commit or stash your uncommitted changes first.");
    }
    let original = run_git(path, ["symbolic-ref", "--quiet", "--short", "HEAD"])
        .or_else(|_| run_git(path, ["rev-parse", "HEAD"]))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    match run_op(path, ["checkout", target], &[]) {
        Ok(o) if o.ok => {}
        Ok(o) => return res(false, "failed", o.output.trim()),
        Err(e) => return res(false, "failed", &e),
    }
    let outcome = (|| -> MergeApplied {
        let merged = match run_op(path, ["-c", "rerere.enabled=true", "merge", "--no-edit", source_sha], &env) {
            Ok(o) => o,
            Err(e) => return res(false, "failed", &e),
        };
        if merged.ok {
            return res(true, "merged", "");
        }
        let in_merge = run_git(path, ["rev-parse", "-q", "--verify", "MERGE_HEAD"]).is_ok();
        let remaining = run_git(path, ["-c", "rerere.enabled=true", "rerere", "remaining"]).unwrap_or_default();
        let unmerged = run_git(path, ["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default();
        if !in_merge || !remaining.trim().is_empty() || unmerged.trim().is_empty() {
            if in_merge {
                let _ = run_git(path, ["merge", "--abort"]);
                return res(false, "conflict", "No remembered resolution covers this conflict; left unchanged.");
            }
            return res(false, "failed", merged.output.trim());
        }
        let mut add: Vec<&str> = vec![LITERAL, "add", "--"];
        add.extend(unmerged.lines().map(str::trim).filter(|l| !l.is_empty()));
        let committed = run_git(path, add)
            .and_then(|_| run_git(path, ["-c", "rerere.enabled=true", "commit", "--no-edit"]));
        match committed {
            Ok(_) => {
                let mut done = res(true, "merged", "Applied your earlier conflict resolution.");
                done.discarded = discarded_by_merge(path, source_sha, target);
                done
            }
            Err(e) => {
                let _ = run_git(path, ["merge", "--abort"]);
                res(false, "failed", &e)
            }
        }
    })();
    if !original.is_empty() && original != target {
        let _ = run_git(path, ["checkout", &original]);
    }
    outcome
}

/// For branches whose merge conflicts: replay the resolutions git recorded earlier (see
/// `enable_rerere`). Branches whose conflicts are not all covered are left untouched.
#[tauri::command]
pub async fn merge_reusing_resolutions(path: String, source: String, targets: Vec<String>) -> Result<Vec<MergeApplied>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        for item in &targets {
            ref_arg(item)?;
        }
        let sha = resolve_commit(&path, &source)?;
        let busy = other_worktree_branches(&path);
        Ok(targets
            .iter()
            .map(|t| {
                if busy.contains(t) {
                    MergeApplied { target: t.clone(), ok: false, status: "failed".into(), message: "Checked out in another worktree; merge it there.".into(), discarded: vec![] }
                } else {
                    merge_reusing_resolution(&path, &sha, t)
                }
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn rebase(path: String, onto: String) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&onto)?;
        run_op(&path, ["rebase", "--autostash", &onto], &[("GIT_EDITOR", "true".into())])
    }).await
}

#[tauri::command]
pub async fn cherry_pick(path: String, sha: String, mainline: Option<u32>) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&sha)?;
        let mut args = vec!["cherry-pick".to_string()];
        if let Some(m) = mainline {
            args.extend(["-m".to_string(), m.to_string()]);
        }
        args.push(sha);
        run_op(&path, args, &[("GIT_EDITOR", "true".into())])
    })
    .await
}

#[tauri::command]
pub async fn revert(path: String, sha: String, mainline: Option<u32>) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&sha)?;
        let mut args = vec!["revert".to_string(), "--no-edit".to_string()];
        if let Some(m) = mainline {
            args.extend(["-m".to_string(), m.to_string()]);
        }
        args.push(sha);
        run_op(&path, args, &[("GIT_EDITOR", "true".into())])
    })
    .await
}

/// `mode` is `soft`, `mixed` or `hard`.
#[tauri::command]
pub async fn reset(path: String, rev: String, mode: String) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&rev)?;
        let flag = match mode.as_str() {
            "soft" => "--soft",
            "hard" => "--hard",
            _ => "--mixed",
        };
        run_op(&path, ["reset", flag, &rev], &[])
    })
    .await
}

/// Continue / skip / abort whatever operation is in progress.
/// `state` is `merge`, `rebase`, `cherry-pick` or `revert`; `action` is `continue`, `skip` or `abort`.
#[tauri::command]
pub async fn sequencer_action(path: String, state: String, action: String) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let envs = [("GIT_EDITOR", "true".to_string())];
        match (state.as_str(), action.as_str()) {
            ("merge", "abort") => run_op(&path, ["merge", "--abort"], &envs),
            ("merge", "continue") => run_op(&path, ["commit", "--no-edit"], &envs),
            ("rebase", a) | ("cherry-pick", a) | ("revert", a) => {
                let flag = format!("--{a}");
                run_op(&path, [state.as_str(), flag.as_str()], &envs)
            }
            _ => Err(format!("Unsupported action {action} for {state}")),
        }
    })
    .await
}

#[derive(Serialize)]
pub struct RebaseCommit {
    pub sha: String,
    pub subject: String,
    /// Full commit message (subject and body), so a reword never drops the body.
    pub message: String,
    pub is_merge: bool,
}

/// Commits that an interactive rebase onto `base` would replay, oldest first.
/// Merge commits are excluded: git's default rebase drops them, and a `pick` of one fails mid-way.
#[tauri::command]
pub async fn rebase_todo(path: String, base: Option<String>) -> Result<Vec<RebaseCommit>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        opt_ref_arg(&base)?;
        let mut args = vec![
            "log".to_string(),
            "--reverse".into(),
            "--no-merges".into(),
            "--no-color".into(),
            "--format=%H%x1f%P%x1f%s%x1f%B%x1e".into(),
        ];
        match base {
            Some(b) => args.push(format!("{b}..HEAD")),
            None => args.push("HEAD".into()),
        }
        Ok(run_git(&path, args)?
            .split('\x1e')
            .filter_map(|rec| {
                let rec = rec.trim_matches(|c| c == '\n' || c == '\r');
                let f: Vec<&str> = rec.splitn(4, '\x1f').collect();
                (f.len() == 4).then(|| RebaseCommit {
                    sha: f[0].into(),
                    subject: f[2].into(),
                    message: f[3].trim().to_string(),
                    is_merge: f[1].split_whitespace().count() > 1,
                })
            })
            .collect())
    })
    .await
}

#[derive(Deserialize)]
pub struct RebaseStep {
    /// pick, reword, edit, squash, fixup or drop.
    pub action: String,
    pub sha: String,
    pub message: Option<String>,
}

/// A path made safe to place between single quotes in a shell command line (git runs `exec` lines of a
/// rebase todo through `sh`): backslashes become slashes, and a single quote is closed, escaped and
/// reopened, so a folder name such as `it's; calc` stays one literal argument.
fn sh_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/").replace('\'', "'\\''")
}

/// A commit id as it appears in a rebase todo line: only hex digits, so it cannot carry a newline
/// (which would add a line of its own, such as an `exec`) or a space.
fn is_hex_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Runs `git rebase -i` non-interactively by handing git a pre-written todo list.
/// A `reword` is performed as `pick` + `exec git commit --amend -F <file>`.
#[tauri::command]
pub async fn rebase_interactive(
    path: String,
    base: Option<String>,
    steps: Vec<RebaseStep>,
) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        opt_ref_arg(&base)?;
        // Keep the todo and reword messages inside the git dir: a paused rebase (conflict, `edit`)
        // runs the remaining `exec` lines later, so these files must outlive this call.
        let git_dir = PathBuf::from(run_git(&path, ["rev-parse", "--absolute-git-dir"])?.trim());
        let work = git_dir.join("treeline-rebase");
        let _ = fs::remove_dir_all(&work);
        fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        let mut todo = String::new();
        for (i, s) in steps.iter().enumerate() {
            if !is_hex_id(&s.sha) {
                return Err("A rebase step has an invalid commit id.".into());
            }
            match s.action.as_str() {
                "reword" => {
                    let msg_file = work.join(format!("msg{i}.txt"));
                    fs::write(&msg_file, s.message.clone().unwrap_or_default())
                        .map_err(|e| e.to_string())?;
                    todo.push_str(&format!("pick {}\n", s.sha));
                    todo.push_str(&format!(
                        "exec git commit --amend -q -F '{}'\n",
                        sh_path(&msg_file)
                    ));
                }
                a @ ("pick" | "edit" | "squash" | "fixup" | "drop") => {
                    todo.push_str(&format!("{a} {}\n", s.sha));
                }
                other => return Err(format!("Unknown rebase action: {other}")),
            }
        }
        let todo_file = work.join("todo.txt");
        fs::write(&todo_file, todo).map_err(|e| e.to_string())?;
        let envs = [
            ("GIT_SEQUENCE_EDITOR", format!("cp '{}'", sh_path(&todo_file))),
            ("GIT_EDITOR", "true".to_string()),
        ];
        let mut args = vec!["rebase".to_string(), "-i".into(), "--autostash".into()];
        match base {
            Some(b) => args.push(b),
            None => args.push("--root".into()),
        }
        run_op(&path, args, &envs)
    })
    .await
}

// ---------------------------------------------------------------------------
// Stash
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn stash_list(path: String) -> Result<Vec<StashEntry>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        Ok(parse_stashes(&run_git(
            &path,
            ["stash", "list", "--format=%gd%x1f%gs%x1f%at"],
        )?))
    })
    .await
}

#[tauri::command]
pub async fn stash_push(
    path: String,
    message: Option<String>,
    include_untracked: bool,
) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let mut args = vec!["stash".to_string(), "push".into()];
        if include_untracked {
            args.push("-u".into());
        }
        if let Some(m) = message.filter(|m| !m.trim().is_empty()) {
            args.extend(["-m".to_string(), m]);
        }
        run_op(&path, args, &[])
    })
    .await
}

/// `action` is `apply`, `pop` or `drop`.
#[tauri::command]
pub async fn stash_action(path: String, reference: String, action: String) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&reference)?;
        if !matches!(action.as_str(), "apply" | "pop" | "drop") {
            return Err(format!("Unknown stash action: {action}"));
        }
        run_op(&path, ["stash", action.as_str(), reference.as_str()], &[])
    })
    .await
}

// ---------------------------------------------------------------------------
// Conflicts
// ---------------------------------------------------------------------------

/// Resolves a repo-relative path and rejects anything that escapes the repository.
fn safe_join(repo: &str, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute()
        || rel_path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_) | Component::RootDir))
    {
        return Err("Path is outside the repository".into());
    }
    Ok(Path::new(repo).join(rel_path))
}

/// Like `safe_join`, but also refuses a symbolic link at the file or at any folder above it inside the
/// repository. A repository can contain a link such as `a.txt -> ~/.ssh/authorized_keys`; reading or
/// writing through it would reach outside the repository.
fn safe_real_path(repo: &str, rel: &str) -> Result<PathBuf, String> {
    let full = safe_join(repo, rel)?;
    let mut cur = PathBuf::from(repo);
    for comp in Path::new(rel).components() {
        cur.push(comp);
        match fs::symlink_metadata(&cur) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(format!(
                    "{rel} is (or sits inside) a symbolic link, so it is not edited here: it could point outside the repository. Use Keep ours or Keep theirs for the whole file."
                ));
            }
            _ => {}
        }
    }
    // Windows junctions and unusual mounts do not always report as symlinks: also require the real
    // location of the containing folder to stay inside the real repository folder.
    let root = fs::canonicalize(repo).map_err(|e| e.to_string())?;
    if let Some(parent) = full.parent() {
        if let Ok(real) = fs::canonicalize(parent) {
            if !real.starts_with(&root) {
                return Err(format!("{rel} resolves outside the repository, so it was left alone."));
            }
        }
    }
    Ok(full)
}

#[derive(Serialize)]
pub struct ConflictFile {
    /// Working-tree content, including conflict markers.
    pub work: String,
    pub ours: Option<String>,
    pub theirs: Option<String>,
    pub base: Option<String>,
    /// Git already filled this file in from a remembered resolution; it only needs review and staging.
    pub prefilled: bool,
    /// While merging: the branch being merged in (the "theirs" side). `None` for other operations.
    pub theirs_name: Option<String>,
}

/// During a merge, the name of the branch being merged in.
fn incoming_name(path: &str) -> Option<String> {
    let dir = PathBuf::from(run_git(path, ["rev-parse", "--absolute-git-dir"]).ok()?.trim());
    if !dir.join("MERGE_HEAD").exists() {
        return None;
    }
    let msg = fs::read_to_string(dir.join("MERGE_MSG")).unwrap_or_default();
    if let Some(name) = msg.lines().next().and_then(merge_msg_branch) {
        return Some(name);
    }
    run_git(
        path,
        ["name-rev", "--name-only", "--no-undefined", "--refs=refs/heads/*", "--refs=refs/remotes/*", "MERGE_HEAD"],
    )
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
}

#[tauri::command]
pub async fn conflict_file(path: String, file: String) -> Result<ConflictFile, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let full = safe_real_path(&path, &file)?;
        // Refuse non-UTF-8 files: a lossy decode would permanently alter bytes outside the
        // conflict region when the resolution is written back.
        let work = match fs::read(&full) {
            Ok(bytes) => String::from_utf8(bytes).map_err(|_| {
                "This file is not valid UTF-8, so it cannot be merged line by line here. \
                 Keep ours or theirs for the whole file, or resolve it in an editor."
                    .to_string()
            })?,
            Err(_) => String::new(),
        };
        let stage = |n: u8| run_git(&path, ["show", &format!(":{n}:{file}")]).ok();
        let (ours, theirs) = (stage(2), stage(3));
        let prefilled = ours.is_some() && theirs.is_some() && rerere_prefilled(&path, &file, &work);
        let theirs_name = incoming_name(&path);
        Ok(ConflictFile { work, ours, theirs, base: stage(1), prefilled, theirs_name })
    })
    .await
}

/// Takes one whole side of a conflicted file (`ours` / `theirs`) and stages the result.
/// If that side deleted the file, the deletion is staged instead.
#[tauri::command]
pub async fn resolve_side(path: String, file: String, side: String) -> Result<(), String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let (flag, stage) = if side == "theirs" { ("--theirs", "3") } else { ("--ours", "2") };
        // Only fall back to deleting the file when that side genuinely has no version of it
        // (it deleted the file). Any other checkout failure must be reported, never turned into a delete.
        let unmerged = run_git(&path, [LITERAL, "ls-files", "-u", "--", &file])?;
        let side_exists = unmerged
            .lines()
            .any(|l| l.split_whitespace().nth(2) == Some(stage));
        if side_exists {
            run_git(&path, [LITERAL, "checkout", flag, "--", &file])?;
            run_git(&path, [LITERAL, "add", "--", &file]).map(|_| ())
        } else {
            run_git(&path, [LITERAL, "rm", "-f", "--", &file]).map(|_| ())
        }
    })
    .await
}

/// Removes a stale `*.lock` file that git reported (for example `index.lock` left by a crashed
/// process). Only `.lock` files inside this repository's git directory can be removed.
#[tauri::command]
pub async fn remove_lock_file(path: String, lock: String) -> Result<(), String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let lock_path = PathBuf::from(&lock);
        if lock_path.extension().and_then(|e| e.to_str()) != Some("lock") {
            return Err("Only .lock files can be removed.".into());
        }
        let git_dir = fs::canonicalize(run_git(&path, ["rev-parse", "--absolute-git-dir"])?.trim())
            .map_err(|e| e.to_string())?;
        let target = fs::canonicalize(&lock_path).map_err(|_| "The lock file is already gone.".to_string())?;
        if !target.starts_with(&git_dir) {
            return Err("That lock file is not inside this repository's git folder, so it was left alone.".into());
        }
        fs::remove_file(&target).map_err(|e| format!("Could not remove the lock file: {e}"))
    })
    .await
}

/// Writes a resolved file and stages it.
#[tauri::command]
pub async fn save_resolution(path: String, file: String, content: String) -> Result<(), String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let full = safe_real_path(&path, &file)?;
        fs::write(&full, content).map_err(|e| e.to_string())?;
        run_git(&path, [LITERAL, "add", "--", &file]).map(|_| ())
    })
    .await
}

// ---------------------------------------------------------------------------
// OS integration
// ---------------------------------------------------------------------------

/// The file manager and terminal launchers must only ever be given an existing folder. Explorer and
/// `open` would otherwise *run* a program or shortcut they are handed, and a value starting with `-`
/// would be read as an option. An absolute, existing directory rules both out.
fn require_existing_folder(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    if p.is_absolute() && p.is_dir() {
        Ok(())
    } else {
        Err("That folder does not exist.".into())
    }
}

#[tauri::command]
pub fn reveal_in_file_manager(path: String) -> Result<(), String> {
    ensure_allowed(&path)?;
    require_existing_folder(&path)?;
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("explorer");
        c.arg(path.replace('/', "\\"));
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(&path);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&path);
        c
    };
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_terminal(path: String) -> Result<(), String> {
    ensure_allowed(&path)?;
    require_existing_folder(&path)?;
    #[cfg(target_os = "windows")]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        // Start cmd.exe in the folder in a new console window; the path never goes through a shell.
        let mut c = std::process::Command::new("cmd.exe");
        c.current_dir(&path).creation_flags(0x0000_0010); // CREATE_NEW_CONSOLE
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.args(["-a", "Terminal", &path]);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("x-terminal-emulator");
        c.current_dir(&path);
        c
    };
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// File history and blame
// ---------------------------------------------------------------------------

const FILE_LOG_FORMAT: &str = "--format=%x1e%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%D%x1f%s%x1f";

/// The commits that changed one file, newest first, following it across renames.
#[tauri::command]
pub async fn file_log(path: String, file: String, skip: u32, limit: u32) -> Result<Vec<FileCommit>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let args: Vec<String> = vec![
            LITERAL.into(),
            "log".into(),
            "--follow".into(),
            "--name-only".into(),
            "--no-color".into(),
            FILE_LOG_FORMAT.into(),
            format!("--skip={skip}"),
            format!("-n{limit}"),
            "--".into(),
            file,
        ];
        match run_git(&path, args) {
            Ok(raw) => Ok(parse_file_log(&raw)),
            Err(e) if e.contains("does not have any commits") || e.contains("bad default revision") => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    })
    .await
}

/// Who last changed each line of a file, in the working copy or (with `rev`) at a given commit.
#[tauri::command]
pub async fn git_blame(path: String, file: String, rev: Option<String>) -> Result<Vec<BlameLine>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        opt_ref_arg(&rev)?;
        let mut args: Vec<String> = vec![LITERAL.into(), "blame".into(), "--line-porcelain".into()];
        if let Some(r) = rev.filter(|r| !r.is_empty()) {
            args.push(r);
        }
        args.push("--".into());
        args.push(file);
        run_git(&path, args).map(|raw| parse_blame(&raw))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn launchers_only_accept_existing_folders() {
        let dir = std::env::temp_dir();
        assert!(require_existing_folder(&dir.to_string_lossy()).is_ok());
        let file = dir.join(format!("treeline-launch-{}.exe", std::process::id()));
        fs::write(&file, "x").unwrap();
        assert!(require_existing_folder(&file.to_string_lossy()).is_err(), "a program file must not be launched");
        let _ = fs::remove_file(&file);
        for bad in ["", "relative/folder", "-a", "--help", "C:/definitely/not/here"] {
            assert!(require_existing_folder(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn rebase_todo_lines_cannot_be_injected() {
        // A folder name with a quote must stay inside the quotes of the exec line.
        let p = Path::new("C:\\Users\\me\\it's; calc; '\\x\\.git");
        let quoted = sh_path(p);
        assert_eq!(quoted, "C:/Users/me/it'\\''s; calc; '\\''/x/.git");
        // Evaluating the line the way sh does must give back exactly one argument equal to the original path.
        let line = format!("echo '{quoted}'");
        if let Ok(out) = std::process::Command::new("sh").arg("-c").arg(&line).output() {
            if out.status.success() {
                assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), "C:/Users/me/it's; calc; '/x/.git");
            }
        }
        assert!(is_hex_id("0123456789abcdefABCDEF"));
        for bad in ["", "abc\nexec calc", "abc def", "abc;calc", "../x", "HEAD"] {
            assert!(!is_hex_id(bad), "{bad:?}");
        }
    }

    #[test]
    fn history_filters_build_safe_arguments() {
        let f = LogFilter {
            author: Some("  ann ".into()),
            text: Some("fix".into()),
            since: Some("2026-11-01T09:30:00+02:00".into()),
            until: None,
        };
        let a = log_filter_args(true, &f, true).unwrap();
        assert!(a.contains(&"--author=ann".to_string()) && a.contains(&"--grep=fix".to_string()));
        assert!(a.contains(&"--since=2026-11-01T09:30:00+02:00".to_string()) && a.contains(&"--fixed-strings".to_string()));
        assert!(!log_filter_args(true, &f, false).unwrap().iter().any(|x| x.starts_with("--grep")));
        for bad in ["--exec=x", "yesterday; calc", "2026-11-01\n--all", "-1"] {
            let f = LogFilter { since: Some(bad.into()), ..Default::default() };
            assert!(log_filter_args(false, &f, true).is_err(), "{bad:?}");
        }
        assert_eq!(log_filter_args(false, &LogFilter::default(), true).unwrap().len(), 6);
    }

    #[test]
    fn history_filters_find_commits_by_author_text_and_date() {
        let dir = std::env::temp_dir().join(format!("treeline-lf-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.to_string_lossy().to_string();
        let g = |args: &[&str]| run_git(&p, args.iter().copied()).unwrap();
        g(&["init", "-q", "-b", "main"]);
        let commit = |name: &str, msg: &str, date: &str| {
            fs::write(dir.join("f.txt"), msg).unwrap();
            g(&["add", "."]);
            let who = format!("user.name={name}");
            let mail = format!("user.email={}@x.org", name.to_lowercase());
            let when = format!("{date}T12:00:00+00:00");
            let out = crate::git::git_cmd()
                .args(["-C", &p, "-c", &who, "-c", &mail, "commit", "-qm", msg])
                .env("GIT_AUTHOR_DATE", &when)
                .env("GIT_COMMITTER_DATE", &when)
                .output()
                .unwrap();
            assert!(out.status.success());
        };
        commit("Ann", "Add login [WIP]", "2026-01-10");
        commit("Bob", "Fix login", "2026-02-10");
        commit("Ann", "Fix logout", "2026-03-10");
        let count = |f: LogFilter| run_git(&p, log_filter_args(true, &f, true).unwrap()).unwrap().lines().count();
        assert_eq!(count(LogFilter::default()), 3);
        assert_eq!(count(LogFilter { author: Some("ann".into()), ..Default::default() }), 2);
        assert_eq!(count(LogFilter { author: Some("bob@x.org".into()), ..Default::default() }), 1);
        assert_eq!(count(LogFilter { text: Some("[wip]".into()), ..Default::default() }), 1);
        assert_eq!(count(LogFilter { text: Some("fix".into()), author: Some("ann".into()), ..Default::default() }), 1);
        let range = LogFilter {
            since: Some("2026-02-01T00:00:00+00:00".into()),
            until: Some("2026-03-01T00:00:00+00:00".into()),
            ..Default::default()
        };
        assert_eq!(count(range), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_the_branch_from_a_merge_message() {
        assert_eq!(merge_msg_branch("Merge branch 'bugfix' into release-4").as_deref(), Some("bugfix"));
        assert_eq!(merge_msg_branch("Merge branch 'feature/x'").as_deref(), Some("feature/x"));
        assert_eq!(merge_msg_branch("Merge remote-tracking branch 'origin/develop' into x").as_deref(), Some("origin/develop"));
        assert_eq!(merge_msg_branch("Merge commit 'abc123'"), None);
        assert_eq!(merge_msg_branch("Fix something"), None);
    }

    #[test]
    fn parses_merge_tree_output() {
        assert_eq!(parse_merge_tree("abc123\n"), ("abc123".into(), vec![]));
        let (t, f) = parse_merge_tree("abc123\na.txt\nb/c.txt\na.txt\n");
        assert_eq!(t, "abc123");
        assert_eq!(f, vec!["a.txt", "b/c.txt"]);
    }

    #[test]
    fn previews_merges_against_a_real_repo() {
        let dir = std::env::temp_dir().join(format!("treeline-mp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.to_string_lossy().to_string();
        let g = |args: &[&str]| {
            run_git(&p, ["-c", "user.name=t", "-c", "user.email=t@t"].iter().chain(args.iter()).copied()).unwrap()
        };
        g(&["init", "-q", "-b", "main"]);
        fs::write(dir.join("a.txt"), "one\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "base"]);
        for b in ["rel-ff", "rel-clean", "rel-conflict"] {
            g(&["branch", b]);
        }
        g(&["checkout", "-q", "-b", "fix"]);
        fs::write(dir.join("a.txt"), "fixed\n").unwrap();
        g(&["commit", "-qam", "fix"]);
        g(&["checkout", "-q", "rel-clean"]);
        fs::write(dir.join("b.txt"), "other\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "b"]);
        g(&["checkout", "-q", "rel-conflict"]);
        fs::write(dir.join("a.txt"), "different\n").unwrap();
        g(&["commit", "-qam", "c"]);
        g(&["checkout", "-q", "fix"]);

        let sha = resolve_commit(&p, "fix").unwrap();
        assert_eq!(classify_merge(&p, &sha, "rel-ff").status, "fast-forward");
        assert_eq!(classify_merge(&p, &sha, "rel-clean").status, "clean");
        let c = classify_merge(&p, &sha, "rel-conflict");
        assert_eq!((c.status.as_str(), c.files), ("conflict", vec!["a.txt".to_string()]));
        assert_eq!(classify_merge(&p, &sha, "fix").status, "up-to-date");
        assert_eq!(classify_merge(&p, &sha, "nope").status, "error");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn replays_a_recorded_resolution_on_another_branch() {
        let dir = std::env::temp_dir().join(format!("treeline-rr-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.to_string_lossy().to_string();
        let g = |args: &[&str]| {
            run_git(&p, ["-c", "user.name=t", "-c", "user.email=t@t"].iter().chain(args.iter()).copied())
        };
        g(&["init", "-q", "-b", "main"]).unwrap();
        g(&["config", "rerere.enabled", "true"]).unwrap();
        fs::write(dir.join("a.txt"), "one\n").unwrap();
        g(&["add", "."]).unwrap();
        g(&["commit", "-qm", "base"]).unwrap();
        for b in ["rel-a", "rel-b", "rel-c"] {
            g(&["checkout", "-q", "-b", b, "main"]).unwrap();
            // rel-c conflicts differently, so the recorded resolution must not apply to it.
            fs::write(dir.join("a.txt"), if b == "rel-c" { "another\n" } else { "different\n" }).unwrap();
            g(&["commit", "-qam", b]).unwrap();
        }
        g(&["checkout", "-q", "-b", "fix", "main"]).unwrap();
        fs::write(dir.join("a.txt"), "fixed\n").unwrap();
        g(&["commit", "-qam", "fix"]).unwrap();
        let sha = resolve_commit(&p, "fix").unwrap();

        // The user resolves the conflict once, on rel-a.
        g(&["checkout", "-q", "rel-a"]).unwrap();
        assert!(g(&["merge", "--no-edit", "fix"]).is_err());
        fs::write(dir.join("a.txt"), "resolved\n").unwrap();
        g(&["add", "a.txt"]).unwrap();
        g(&["commit", "-qm", "merge fix into rel-a"]).unwrap();
        g(&["checkout", "-q", "fix"]).unwrap();

        let ok = merge_reusing_resolution(&p, &sha, "rel-b");
        assert!(ok.ok, "{}", ok.message);
        assert_eq!(g(&["show", "rel-b:a.txt"]).unwrap().trim(), "resolved");
        assert_eq!(g(&["symbolic-ref", "--short", "HEAD"]).unwrap().trim(), "fix", "returns to the original branch");

        let no = merge_reusing_resolution(&p, &sha, "rel-c");
        assert_eq!(no.status, "conflict", "{}", no.message);
        assert_eq!(g(&["show", "rel-c:a.txt"]).unwrap().trim(), "another", "unresolvable branch is untouched");
        assert!(g(&["status", "--porcelain"]).unwrap().trim().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_prefilled_conflicts_and_dropped_changes() {
        let dir = std::env::temp_dir().join(format!("treeline-pf-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.to_string_lossy().to_string();
        let g = |args: &[&str]| {
            run_git(&p, ["-c", "user.name=t", "-c", "user.email=t@t"].iter().chain(args.iter()).copied())
        };
        g(&["init", "-q", "-b", "main"]).unwrap();
        g(&["config", "rerere.enabled", "true"]).unwrap();
        fs::write(dir.join("a.txt"), "one\n").unwrap();
        g(&["add", "."]).unwrap();
        g(&["commit", "-qm", "base"]).unwrap();
        for b in ["rel-4", "rel-5"] {
            g(&["checkout", "-q", "-b", b, "main"]).unwrap();
            fs::write(dir.join("a.txt"), "different\n").unwrap();
            g(&["commit", "-qam", b]).unwrap();
        }
        g(&["checkout", "-q", "-b", "fix", "main"]).unwrap();
        fs::write(dir.join("a.txt"), "fixed\n").unwrap();
        g(&["commit", "-qam", "fix"]).unwrap();
        let sha = resolve_commit(&p, "fix").unwrap();

        // First conflict resolved by hand and recorded.
        g(&["checkout", "-q", "rel-4"]).unwrap();
        assert!(g(&["merge", "--no-edit", "fix"]).is_err());
        let a = fs::read_to_string(dir.join("a.txt")).unwrap();
        assert!(a.contains("<<<<<<<"), "a real conflict has markers");
        assert!(!rerere_prefilled(&p, "a.txt", &a));
        fs::write(dir.join("a.txt"), "different\nfixed\n").unwrap();
        g(&["add", "a.txt"]).unwrap();
        g(&["commit", "-qm", "merge into rel-4"]).unwrap();
        assert!(discarded_by_merge(&p, &sha, "rel-4").is_empty(), "a combined result keeps the change");

        // The same conflict on rel-5: git fills the file in, with no markers, but it is still unmerged.
        g(&["checkout", "-q", "rel-5"]).unwrap();
        assert!(g(&["merge", "--no-edit", "fix"]).is_err());
        let a = fs::read_to_string(dir.join("a.txt")).unwrap();
        assert!(!a.contains("<<<<<<<"), "rerere replaced the markers: {a}");
        assert!(rerere_prefilled(&p, "a.txt", &a));

        // Choosing "ours" at that point drops the fix, and the check says so.
        g(&["checkout", "--ours", "--", "a.txt"]).unwrap();
        g(&["add", "a.txt"]).unwrap();
        g(&["commit", "-qm", "merge into rel-5"]).unwrap();
        assert_eq!(discarded_by_merge(&p, &sha, "rel-5"), vec!["a.txt".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_name_status_with_renames() {
        let raw = "M\0a.ts\0R100\0old.ts\0new.ts\0A\0dir/b c.ts\0";
        let f = parse_name_status(raw);
        assert_eq!(f.len(), 3);
        assert_eq!(f[0], FileEntry { status: "M".into(), path: "a.ts".into(), old_path: None });
        assert_eq!(f[1].old_path.as_deref(), Some("old.ts"));
        assert_eq!(f[1].path, "new.ts");
        assert_eq!(f[2].path, "dir/b c.ts");
    }

    #[test]
    fn parses_log_records() {
        let raw = "abc\x1fp1 p2\x1fAda\x1fa@x.io\x1f1700000000\x1fHEAD -> main, tag: v1\x1fFix: thing\x1e\n\
                   def\x1f\x1fBob\x1fb@x.io\x1f1600000000\x1f\x1fRoot\x1e\n";
        let c = parse_log(raw);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].parents, vec!["p1", "p2"]);
        assert_eq!(c[0].refs, vec!["HEAD -> main", "tag: v1"]);
        assert!(c[1].parents.is_empty());
        assert_eq!(c[1].subject, "Root");
    }

    #[test]
    fn parses_refs_and_skips_remote_head() {
        let raw = "refs/heads/main\x1fmain\x1faaa\x1forigin/main\x1f[ahead 1]\x1f*\x1f\n\
                   refs/remotes/origin/HEAD\x1forigin\x1fbbb\x1f\x1f\x1f \x1f\n\
                   refs/remotes/origin/dev\x1forigin/dev\x1fccc\x1f\x1f\x1f \x1f\n\
                   refs/tags/v1\x1fv1\x1fddd\x1f\x1f\x1f \x1feee\n";
        let r = parse_refs(raw);
        assert_eq!(r.len(), 3);
        assert!(r[0].head);
        assert_eq!(r[0].track.as_deref(), Some("[ahead 1]"));
        assert_eq!(r[1].name, "origin/dev");
        assert_eq!(r[2].sha, "eee");
    }

    /// Makes a symbolic link, or returns false when the platform or account does not allow it
    /// (Windows needs developer mode or elevated rights), in which case the test is skipped.
    fn try_symlink(target: &Path, link: &Path) -> bool {
        #[cfg(unix)]
        let r = std::os::unix::fs::symlink(target, link);
        #[cfg(windows)]
        let r = if target.is_dir() {
            // A junction needs no special rights, unlike a symbolic link, and is also a way out of a repository.
            std::os::windows::fs::symlink_dir(target, link).or_else(|_| {
                let ok = std::process::Command::new("cmd")
                    .args(["/C", "mklink", "/J"])
                    .arg(link)
                    .arg(target)
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if ok { Ok(()) } else { Err(std::io::Error::other("mklink failed")) }
            })
        } else {
            std::os::windows::fs::symlink_file(target, link)
        };
        r.is_ok()
    }

    #[test]
    fn refuses_files_reached_through_symlinks() {
        let base = std::env::temp_dir().join(format!("treeline-sl-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        let outside = base.join("outside");
        fs::create_dir_all(repo.join("sub")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "secret").unwrap();
        fs::write(repo.join("sub").join("ok.txt"), "fine").unwrap();
        let r = repo.to_string_lossy().to_string();
        assert!(safe_real_path(&r, "sub/ok.txt").is_ok(), "a normal file is fine");
        assert!(safe_real_path(&r, "new-file.txt").is_ok(), "a file that does not exist yet is fine");
        if try_symlink(&outside.join("secret.txt"), &repo.join("link.txt")) {
            assert!(safe_real_path(&r, "link.txt").is_err(), "a link to a file outside");
        } else {
            eprintln!("skipped file-symlink check: cannot create symlinks here");
        }
        if try_symlink(&outside, &repo.join("linkdir")) {
            assert!(safe_real_path(&r, "linkdir/secret.txt").is_err(), "a file inside a linked folder");
        } else {
            eprintln!("skipped folder-symlink check: cannot create symlinks here");
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(safe_join("/repo", "../etc/passwd").is_err());
        assert!(safe_join("/repo", "a/../../b").is_err());
        assert!(safe_join("/repo", "src/a.ts").is_ok());
    }

    // ---- integration tests against real git ----------------------------------------------

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=Test", "-c", "user.email=t@example.com"])
            .args(args)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// A fresh repository with its own identity. The code under test runs plain `git commit`
    /// (rebase reword, merge continue …), so the identity must live in the repository itself;
    /// tests must never depend on the developer's global git configuration or on signing setup.
    fn repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        // Commands only accept folders the user opened; a test repository stands in for one.
        crate::git::allow_repo(&d.path().to_string_lossy());
        git(d.path(), &["init", "-q", "-b", "main"]);
        git(d.path(), &["config", "user.name", "Test"]);
        git(d.path(), &["config", "user.email", "t@example.com"]);
        git(d.path(), &["config", "commit.gpgsign", "false"]);
        d
    }

    fn p(d: &tempfile::TempDir) -> String {
        d.path().to_string_lossy().into_owned()
    }

    #[test]
    fn stage_commit_and_unstage_flow() {
        let d = repo();
        fs::write(d.path().join("a b.txt"), "one\n").unwrap();
        let path = p(&d);
        run_git_stdin(&path, ["add", "-A", "--pathspec-from-file=-", "--pathspec-file-nul"], &nul_list(&["a b.txt".into()])).unwrap();
        let s = git(d.path(), &["status", "--porcelain"]); assert!(s.starts_with("A ") && s.contains("a b.txt"), "{s}");
        // unstage before the first commit must work
        run_git_stdin(&path, ["reset", "-q", "--pathspec-from-file=-", "--pathspec-file-nul"], &nul_list(&["a b.txt".into()])).unwrap();
        assert!(git(d.path(), &["status", "--porcelain"]).starts_with("??"));
    }

    #[test]
    fn line_level_patch_applies_to_index() {
        let d = repo();
        fs::write(d.path().join("f.txt"), "1\n2\n3\n").unwrap();
        git(d.path(), &["add", "."]);
        git(d.path(), &["commit", "-qm", "init"]);
        fs::write(d.path().join("f.txt"), "1\nTWO\n3\nFOUR\n").unwrap();
        let path = p(&d);
        // stage only the first hunk-worth: change 2 -> TWO
        let patch = "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,3 @@\n 1\n-2\n+TWO\n 3\n";
        run_git_stdin(&path, ["apply", "--cached", "--recount", "-"], patch.as_bytes()).unwrap();
        let staged = git(d.path(), &["diff", "--cached"]);
        assert!(staged.contains("+TWO") && !staged.contains("+FOUR"));
    }

    #[test]
    fn merge_conflict_is_reported_and_resolvable() {
        let d = repo();
        let path = p(&d);
        fs::write(d.path().join("c.txt"), "base\n").unwrap();
        git(d.path(), &["add", "."]);
        git(d.path(), &["commit", "-qm", "base"]);
        git(d.path(), &["checkout", "-qb", "feature"]);
        fs::write(d.path().join("c.txt"), "feature\n").unwrap();
        git(d.path(), &["commit", "-qam", "feature"]);
        git(d.path(), &["checkout", "-q", "main"]);
        fs::write(d.path().join("c.txt"), "main\n").unwrap();
        git(d.path(), &["commit", "-qam", "main"]);

        let res = run_op(&path, ["merge", "--no-edit", "feature"], &[("GIT_EDITOR", "true".into())]).unwrap();
        assert!(!res.ok);
        let status = crate::git::parse_status(&run_git(&path, ["status", "--porcelain=v2", "--branch", "-z"]).unwrap());
        assert!(status.files.iter().any(|f| f.conflicted));

        let cf = fs::read_to_string(d.path().join("c.txt")).unwrap();
        assert!(cf.contains("<<<<<<<"));
        assert_eq!(run_git(&path, ["show", ":2:c.txt"]).unwrap(), "main\n");
        assert_eq!(run_git(&path, ["show", ":3:c.txt"]).unwrap(), "feature\n");

        fs::write(d.path().join("c.txt"), "resolved\n").unwrap();
        run_git(&path, ["add", "--", "c.txt"]).unwrap();
        let cont = run_op(&path, ["commit", "--no-edit"], &[("GIT_EDITOR", "true".into())]).unwrap();
        assert!(cont.ok, "{}", cont.output);
    }

    #[test]
    fn interactive_rebase_squashes_and_rewords() {
        let d = repo();
        let path = p(&d);
        for (i, n) in ["a", "b", "c"].iter().enumerate() {
            fs::write(d.path().join(format!("{n}.txt")), n).unwrap();
            git(d.path(), &["add", "."]);
            git(d.path(), &["commit", "-qm", &format!("commit {i}")]);
        }
        let shas: Vec<String> = git(d.path(), &["log", "--reverse", "--format=%H"])
            .lines()
            .map(String::from)
            .collect();
        // keep commit 0, squash 1 into it, reword 2
        let dir = tempfile::tempdir().unwrap();
        let msg = dir.path().join("m.txt");
        fs::write(&msg, "renamed three").unwrap();
        let todo = dir.path().join("todo.txt");
        fs::write(
            &todo,
            format!(
                "pick {}\nsquash {}\npick {}\nexec git commit --amend -q -F '{}'\n",
                shas[0], shas[1], shas[2], sh_path(&msg)
            ),
        )
        .unwrap();
        let res = run_op(
            &path,
            ["rebase", "-i", "--root"],
            &[
                ("GIT_SEQUENCE_EDITOR", format!("cp '{}'", sh_path(&todo))),
                ("GIT_EDITOR", "true".into()),
            ],
        )
        .unwrap();
        assert!(res.ok, "{}", res.output);
        let log = git(d.path(), &["log", "--format=%s"]);
        assert_eq!(log.lines().count(), 2);
        assert!(log.starts_with("renamed three"));
    }

    #[test]
    fn paused_rebase_can_still_reword_after_continue() {
        // The reword message files must outlive the command: a pause at `edit` runs the
        // remaining `exec git commit --amend -F <file>` lines only after `rebase --continue`.
        let d = repo();
        let path = p(&d);
        for n in ["a", "b", "c"] {
            fs::write(d.path().join(format!("{n}.txt")), n).unwrap();
            git(d.path(), &["add", "."]);
            git(d.path(), &["commit", "-qm", &format!("commit {n}")]);
        }
        let shas: Vec<String> = git(d.path(), &["log", "--reverse", "--format=%H"])
            .lines()
            .map(String::from)
            .collect();
        let step = |action: &str, sha: &str, message: Option<&str>| RebaseStep {
            action: action.into(),
            sha: sha.into(),
            message: message.map(String::from),
        };
        let res = tauri::async_runtime::block_on(rebase_interactive(
            path.clone(),
            None,
            vec![
                step("edit", &shas[0], None),
                step("pick", &shas[1], None),
                step("reword", &shas[2], Some("third, reworded")),
            ],
        ))
        .unwrap();
        // Paused at the `edit` step: git reports success but leaves the rebase in progress.
        assert!(res.ok, "{}", res.output);
        assert!(d.path().join(".git/rebase-merge").exists(), "rebase should be paused at `edit`");
        let cont = tauri::async_runtime::block_on(sequencer_action(path.clone(), "rebase".into(), "continue".into())).unwrap();
        assert!(cont.ok, "{}", cont.output);
        let log = git(d.path(), &["log", "--format=%s"]);
        assert!(log.starts_with("third, reworded"), "log was: {log}");
        assert_eq!(log.lines().count(), 3);
    }

    #[test]
    fn removes_only_lock_files_inside_the_git_dir() {
        let d = repo();
        let path = p(&d);
        let lock = d.path().join(".git/index.lock");
        fs::write(&lock, "").unwrap();
        // outside the git dir, or not a .lock file: refused
        let outside = d.path().join("notes.lock");
        fs::write(&outside, "").unwrap();
        assert!(tauri::async_runtime::block_on(remove_lock_file(path.clone(), outside.to_string_lossy().into())).is_err());
        assert!(outside.exists());
        assert!(tauri::async_runtime::block_on(remove_lock_file(path.clone(), d.path().join("a.txt").to_string_lossy().into())).is_err());
        // the real stale lock: removed
        tauri::async_runtime::block_on(remove_lock_file(path, lock.to_string_lossy().into())).unwrap();
        assert!(!lock.exists());
    }

    #[test]
    fn stash_with_untracked_files_leaves_a_clean_tree() {
        // Regression: a globally-set GIT_LITERAL_PATHSPECS made `stash -u` leave untracked files behind.
        let d = repo();
        let path = p(&d);
        fs::write(d.path().join("a.txt"), "1").unwrap();
        git(d.path(), &["add", "."]);
        git(d.path(), &["commit", "-qm", "init"]);
        fs::write(d.path().join("new.txt"), "x").unwrap();
        fs::create_dir(d.path().join("dir")).unwrap();
        fs::write(d.path().join("dir/inner.txt"), "y").unwrap();
        let res = run_op(&path, ["stash", "push", "-u", "-m", "wip"], &[]).unwrap();
        assert!(res.ok, "{}", res.output);
        assert!(!d.path().join("new.txt").exists());
        assert!(!d.path().join("dir").exists());
        assert!(git(d.path(), &["status", "--porcelain"]).trim().is_empty());
    }

    #[test]
    fn staging_treats_glob_like_names_literally() {
        let d = repo();
        let path = p(&d);
        // `[1]` and `{}` are pathspec/glob-ish; a file literally named this must stage on its own.
        let name = "report[1].txt";
        fs::write(d.path().join(name), "a").unwrap();
        fs::write(d.path().join("report1.txt"), "b").unwrap();
        run_git_stdin(
            &path,
            [LITERAL, "add", "-A", "--pathspec-from-file=-", "--pathspec-file-nul"],
            &nul_list(&[name.to_string()]),
        )
        .unwrap();
        let staged = git(d.path(), &["diff", "--cached", "--name-only"]);
        assert_eq!(staged.trim(), name);
    }

    #[test]
    fn stash_round_trip() {
        let d = repo();
        let path = p(&d);
        fs::write(d.path().join("s.txt"), "1").unwrap();
        git(d.path(), &["add", "."]);
        git(d.path(), &["commit", "-qm", "init"]);
        fs::write(d.path().join("s.txt"), "2").unwrap();
        run_op(&path, ["stash", "push", "-m", "wip"], &[]).unwrap();
        let list = parse_stashes(&run_git(&path, ["stash", "list", "--format=%gd%x1f%gs%x1f%at"]).unwrap());
        assert_eq!(list.len(), 1);
        assert!(list[0].message.contains("wip"));
        assert_eq!(fs::read_to_string(d.path().join("s.txt")).unwrap(), "1");
    }

    #[test]
    fn file_log_follows_renames_and_blame_names_the_author() {
        let d = repo();
        let path = p(&d);
        fs::write(d.path().join("old.txt"), "one\ntwo\n").unwrap();
        git(d.path(), &["add", "."]);
        git(d.path(), &["-c", "user.name=Alice", "commit", "-qm", "create"]);
        git(d.path(), &["mv", "old.txt", "new.txt"]);
        git(d.path(), &["commit", "-qm", "rename"]);
        fs::write(d.path().join("new.txt"), "one\ntwo\nthree\n").unwrap();
        git(d.path(), &["add", "."]);
        git(d.path(), &["-c", "user.name=Bob", "commit", "-qm", "add three"]);

        let raw = run_git(&path, [LITERAL, "log", "--follow", "--name-only", "--no-color", FILE_LOG_FORMAT, "--", "new.txt"]).unwrap();
        let log = parse_file_log(&raw);
        let subjects: Vec<&str> = log.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["add three", "rename", "create"], "{raw:?}");
        assert_eq!(log[0].file, "new.txt");
        assert_eq!(log[2].file, "old.txt", "name at the oldest commit");

        let blame = parse_blame(&run_git(&path, [LITERAL, "blame", "--line-porcelain", "--", "new.txt"]).unwrap());
        assert_eq!(blame.len(), 3);
        assert_eq!((blame[0].author.as_str(), blame[0].text.as_str()), ("Alice", "one"));
        assert_eq!((blame[2].author.as_str(), blame[2].summary.as_str(), blame[2].line), ("Bob", "add three", 3));
    }
}
