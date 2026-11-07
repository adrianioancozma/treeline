//! Repository operations: staging, commits, history, branches, tags, remotes, stashes,
//! merge / rebase / cherry-pick, and conflict resolution.

use crate::git::{
    blocking, run_git, run_git_ok_codes, run_git_stdin, run_op, stream_git, OpResult, LITERAL,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use tauri::AppHandle;

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
    blocking(move || run_git(&path, ["add", "-A"]).map(|_| ())).await
}

#[tauri::command]
pub async fn unstage_all(path: String) -> Result<(), String> {
    blocking(move || run_git(&path, ["reset", "-q"]).map(|_| ())).await
}

/// Throws away working-tree changes: `tracked` files are restored, `untracked` ones deleted.
#[tauri::command]
pub async fn discard_files(
    path: String,
    tracked: Vec<String>,
    untracked: Vec<String>,
) -> Result<(), String> {
    blocking(move || {
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
        let ctx = format!("-U{context}");
        if untracked {
            return run_git_ok_codes(
                &path,
                [LITERAL, "diff", "--no-index", "--no-color", &ctx, "--", "/dev/null", &file],
                &[1],
            );
        }
        let mut args = vec![LITERAL, "diff", "--no-color", &ctx];
        if staged {
            args.push("--cached");
        }
        args.extend(["--", &file]);
        run_git(&path, args)
    })
    .await
}

#[derive(Serialize, Debug, PartialEq)]
pub struct FileEntry {
    pub status: String,
    pub path: String,
    pub old_path: Option<String>,
}

fn parse_name_status(raw: &str) -> Vec<FileEntry> {
    let mut out = Vec::new();
    let mut tokens = raw.split('\0').filter(|t| !t.is_empty());
    while let Some(status) = tokens.next() {
        let first = status.chars().next().unwrap_or('M');
        if first == 'R' || first == 'C' {
            let old = tokens.next().map(str::to_string);
            if let Some(new) = tokens.next() {
                out.push(FileEntry { status: first.to_string(), path: new.to_string(), old_path: old });
            }
        } else if let Some(p) = tokens.next() {
            out.push(FileEntry { status: first.to_string(), path: p.to_string(), old_path: None });
        }
    }
    out
}

/// Files changed by `sha` relative to `parent` (the first parent; `None` for a root commit).
#[tauri::command]
pub async fn commit_files(
    path: String,
    sha: String,
    parent: Option<String>,
) -> Result<Vec<FileEntry>, String> {
    blocking(move || {
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
        let ctx = format!("-U{context}");
        let mut specs = vec![file.clone()];
        if let Some(o) = old_file {
            specs.push(o);
        }
        let mut args: Vec<String> = match &parent {
            Some(p) => vec![LITERAL.into(), "diff".into(), "--no-color".into(), "-M".into(), ctx, p.clone(), sha],
            None => vec![LITERAL.into(), "show".into(), "--format=".into(), "--no-color".into(), "--root".into(), ctx, sha],
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

#[derive(Serialize, Debug)]
pub struct Commit {
    pub sha: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub refs: Vec<String>,
    pub subject: String,
}

fn parse_log(raw: &str) -> Vec<Commit> {
    raw.split('\x1e')
        .filter_map(|rec| {
            let rec = rec.trim_matches(|c| c == '\n' || c == '\r');
            if rec.is_empty() {
                return None;
            }
            let f: Vec<&str> = rec.split('\x1f').collect();
            if f.len() < 7 {
                return None;
            }
            Some(Commit {
                sha: f[0].into(),
                parents: f[1].split_whitespace().map(String::from).collect(),
                author: f[2].into(),
                email: f[3].into(),
                time: f[4].parse().unwrap_or(0),
                refs: f[5]
                    .split(", ")
                    .filter(|r| !r.is_empty())
                    .map(String::from)
                    .collect(),
                subject: f[6].trim().into(),
            })
        })
        .collect()
}

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

// ---------------------------------------------------------------------------
// Refs: branches, remote branches, tags
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug)]
pub struct RefInfo {
    /// `branch`, `remote` or `tag`.
    pub kind: String,
    pub name: String,
    pub sha: String,
    pub upstream: Option<String>,
    pub track: Option<String>,
    pub head: bool,
}

fn parse_refs(raw: &str) -> Vec<RefInfo> {
    raw.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\x1f').collect();
            if f.len() < 7 {
                return None;
            }
            let (full, short) = (f[0], f[1]);
            let (kind, name) = if let Some(n) = full.strip_prefix("refs/heads/") {
                ("branch", n.to_string())
            } else if let Some(n) = full.strip_prefix("refs/remotes/") {
                if n.ends_with("/HEAD") {
                    return None;
                }
                ("remote", n.to_string())
            } else if let Some(n) = full.strip_prefix("refs/tags/") {
                ("tag", n.to_string())
            } else {
                return None;
            };
            let _ = short;
            // Annotated tags point at a tag object; `*objectname` is the commit underneath.
            let sha = if !f[6].is_empty() { f[6] } else { f[2] };
            Some(RefInfo {
                kind: kind.into(),
                name,
                sha: sha.into(),
                upstream: Some(f[3].to_string()).filter(|s| !s.is_empty()),
                track: Some(f[4].to_string()).filter(|s| !s.is_empty()),
                head: f[5] == "*",
            })
        })
        .collect()
}

#[tauri::command]
pub async fn list_refs(path: String) -> Result<Vec<RefInfo>, String> {
    blocking(move || {
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
        let local = remote_branch
            .split_once('/')
            .map(|(_, b)| b.to_string())
            .unwrap_or_else(|| remote_branch.clone());
        let exists = run_git(&path, ["rev-parse", "--verify", "-q", &format!("refs/heads/{local}")]).is_ok();
        if exists {
            run_op(&path, ["checkout", &local], &[])
        } else {
            run_op(&path, ["checkout", "--track", "-b", &local, &remote_branch], &[])
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
    blocking(move || run_op(&path, ["branch", "-m", &old, &new], &[])).await
}

#[tauri::command]
pub async fn delete_branch(path: String, name: String, force: bool) -> Result<OpResult, String> {
    blocking(move || run_op(&path, ["branch", if force { "-D" } else { "-d" }, &name], &[])).await
}

#[tauri::command]
pub async fn create_tag(
    path: String,
    name: String,
    rev: Option<String>,
    message: Option<String>,
) -> Result<OpResult, String> {
    blocking(move || {
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
    blocking(move || run_op(&path, ["tag", "-d", &name], &[])).await
}

// ---------------------------------------------------------------------------
// Remotes: fetch / pull / push
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn fetch(app: AppHandle, id: String, path: String, prune: bool) -> Result<OpResult, String> {
    blocking(move || {
        let mut args: Vec<String> = vec!["fetch".into(), "--all".into(), "--progress".into()];
        if prune {
            args.push("--prune".into());
        }
        stream_git(&app, &id, Some(&path), &args)
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
        let mut args: Vec<String> = vec!["fetch".into(), "--progress".into(), remote];
        if let Some(r) = refspec.filter(|r| !r.is_empty()) {
            args.push(r);
        }
        stream_git(&app, &id, Some(&path), &args)
    })
    .await
}

/// `mode` is `merge`, `rebase` or `ff-only`.
#[tauri::command]
pub async fn pull(app: AppHandle, id: String, path: String, mode: String) -> Result<OpResult, String> {
    blocking(move || {
        let mut args: Vec<String> = vec!["pull".into(), "--progress".into()];
        match mode.as_str() {
            // A dirty tree would otherwise make `pull --rebase` refuse to start.
            "rebase" => args.extend(["--rebase".to_string(), "--autostash".to_string()]),
            "ff-only" => args.push("--ff-only".into()),
            _ => args.push("--no-rebase".into()),
        }
        stream_git(&app, &id, Some(&path), &args)
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
        stream_git(&app, &id, Some(&path), &args)
    })
    .await
}

#[tauri::command]
pub async fn list_remotes(path: String) -> Result<Vec<String>, String> {
    blocking(move || {
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
        let args: Vec<String> = vec!["push".into(), remote, "--delete".into(), branch];
        stream_git(&app, &id, Some(&path), &args)
    })
    .await
}

// ---------------------------------------------------------------------------
// Merge / rebase / cherry-pick / revert / reset
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn merge(path: String, rev: String, no_ff: bool, squash: bool) -> Result<OpResult, String> {
    blocking(move || {
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

#[tauri::command]
pub async fn rebase(path: String, onto: String) -> Result<OpResult, String> {
    blocking(move || run_op(&path, ["rebase", "--autostash", &onto], &[("GIT_EDITOR", "true".into())])).await
}

#[tauri::command]
pub async fn cherry_pick(path: String, sha: String, mainline: Option<u32>) -> Result<OpResult, String> {
    blocking(move || {
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

fn sh_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
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
        // Keep the todo and reword messages inside the git dir: a paused rebase (conflict, `edit`)
        // runs the remaining `exec` lines later, so these files must outlive this call.
        let git_dir = PathBuf::from(run_git(&path, ["rev-parse", "--absolute-git-dir"])?.trim());
        let work = git_dir.join("apmgit-rebase");
        let _ = fs::remove_dir_all(&work);
        fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        let mut todo = String::new();
        for (i, s) in steps.iter().enumerate() {
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

#[derive(Serialize, Debug)]
pub struct StashEntry {
    #[serde(rename = "ref")]
    pub reference: String,
    pub message: String,
    pub time: i64,
}

fn parse_stashes(raw: &str) -> Vec<StashEntry> {
    raw.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.splitn(3, '\x1f').collect();
            (f.len() == 3).then(|| StashEntry {
                reference: f[0].into(),
                message: f[1].into(),
                time: f[2].parse().unwrap_or(0),
            })
        })
        .collect()
}

#[tauri::command]
pub async fn stash_list(path: String) -> Result<Vec<StashEntry>, String> {
    blocking(move || {
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

#[derive(Serialize)]
pub struct ConflictFile {
    /// Working-tree content, including conflict markers.
    pub work: String,
    pub ours: Option<String>,
    pub theirs: Option<String>,
    pub base: Option<String>,
}

#[tauri::command]
pub async fn conflict_file(path: String, file: String) -> Result<ConflictFile, String> {
    blocking(move || {
        let full = safe_join(&path, &file)?;
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
        Ok(ConflictFile { work, ours: stage(2), theirs: stage(3), base: stage(1) })
    })
    .await
}

/// Takes one whole side of a conflicted file (`ours` / `theirs`) and stages the result.
/// If that side deleted the file, the deletion is staged instead.
#[tauri::command]
pub async fn resolve_side(path: String, file: String, side: String) -> Result<(), String> {
    blocking(move || {
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
        let full = safe_join(&path, &file)?;
        fs::write(&full, content).map_err(|e| e.to_string())?;
        run_git(&path, [LITERAL, "add", "--", &file]).map(|_| ())
    })
    .await
}

// ---------------------------------------------------------------------------
// OS integration
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn reveal_in_file_manager(path: String) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

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

    fn repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        git(d.path(), &["init", "-q", "-b", "main"]);
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
}
