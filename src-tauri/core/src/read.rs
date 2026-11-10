//! The read-only commands, without Tauri: what one reload of a repository and the history and diff views ask for.
//! The desktop app calls them through thin Tauri commands; a helper next to a remote repository calls them directly.

use crate::git::{LITERAL, run_git, run_git_ok_codes};
use serde::Serialize;
use crate::ops_parse::*;
use crate::status::*;
use crate::trust::ensure_allowed;
use std::path::PathBuf;


pub fn list_refs(path: String) -> Result<Vec<RefInfo>, String> {
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
    }

pub fn stash_list(path: String) -> Result<Vec<StashEntry>, String> {
        ensure_allowed(&path)?;
        Ok(parse_stashes(&run_git(
            &path,
            ["stash", "list", "--format=%gd%x1f%gs%x1f%at"],
        )?))
    }

pub fn list_remotes(path: String) -> Result<Vec<String>, String> {
        ensure_allowed(&path)?;
        Ok(run_git(&path, ["remote"])?
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

pub fn repo_status(path: String) -> Result<RepoStatus, String> {
        ensure_allowed(&path)?;
        let raw = run_git(
            &path,
            [
                "status",
                "--porcelain=v2",
                "--branch",
                "--untracked-files=normal",
                "-z",
            ],
        )?;
        let mut status = parse_status(&raw);
        status.state = repo_state(&path);
        Ok(status)
    }

/// Detects an in-progress merge / rebase / cherry-pick / revert from the git directory.
pub fn repo_state(path: &str) -> Option<String> {
    let dir = PathBuf::from(
        run_git(path, ["rev-parse", "--absolute-git-dir"])
            .ok()?
            .trim(),
    );
    if dir.join("rebase-merge").exists() || dir.join("rebase-apply").exists() {
        Some("rebase".into())
    } else if dir.join("MERGE_HEAD").exists() {
        Some("merge".into())
    } else if dir.join("CHERRY_PICK_HEAD").exists() {
        Some("cherry-pick".into())
    } else if dir.join("REVERT_HEAD").exists() {
        Some("revert".into())
    } else {
        None
    }
}

pub fn git_log(path: String,
    skip: u32,
    limit: u32,
    all: bool,
    query: Option<String>,
    file: Option<String>,) -> Result<Vec<Commit>, String> {
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
    }

pub fn commit_files(path: String,
    sha: String,
    parent: Option<String>,) -> Result<Vec<FileEntry>, String> {
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
    }

pub fn commit_diff(path: String,
    sha: String,
    parent: Option<String>,
    file: String,
    old_file: Option<String>,
    context: u32,) -> Result<String, String> {
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
    }

pub fn file_diff(path: String,
    file: String,
    staged: bool,
    untracked: bool,
    context: u32,) -> Result<String, String> {
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
    }

pub fn head_message(path: String) -> Result<String, String> {
        ensure_allowed(&path)?;
        Ok(run_git(&path, ["log", "-1", "--format=%B"])
            .unwrap_or_default()
            .trim()
            .to_string())
    }

pub fn list_submodules(path: String) -> Result<Vec<Submodule>, String> {
        ensure_allowed(&path)?;
        // A repo without a .gitmodules has none; skip the process entirely.
        if !PathBuf::from(&path).join(".gitmodules").exists() {
            return Ok(Vec::new());
        }
        // `git submodule status` costs 0.6–0.9 s on real repos. One `ls-files` for the recorded
        // commits plus a parallel `rev-parse HEAD` per checked-out submodule is several times faster.
        let raw = run_git(&path, ["ls-files", "--stage", "-z"])?;
        let links = parse_gitlinks(&raw);
        let root = PathBuf::from(&path);
        let mut out: Vec<Submodule> = std::thread::scope(|scope| {
            let handles: Vec<_> = links
                .iter()
                .map(|(sha, sub_path, conflicted)| {
                    let dir = root.join(sub_path);
                    scope.spawn(move || {
                        let state = if *conflicted {
                            "conflict"
                        } else if !dir.join(".git").exists() {
                            "uninitialized"
                        } else {
                            match run_git(&dir.to_string_lossy(), ["rev-parse", "HEAD"]) {
                                Ok(head) if head.trim() != sha => "modified",
                                _ => "ok",
                            }
                        };
                        state
                    })
                })
                .collect();
            links
                .iter()
                .zip(handles)
                .map(|((sha, sub_path, _), h)| Submodule {
                    path: sub_path.clone(),
                    sha: sha.clone(),
                    state: h.join().unwrap_or("ok").to_string(),
                    describe: None,
                })
                .collect()
        });
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

pub const LOG_FORMAT: &str = "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%D%x1f%s%x1e";

#[derive(Serialize, Debug)]
pub struct Submodule {
    pub path: String,
    pub sha: String,
    /// `ok`, `uninitialized`, `modified` (checked-out commit differs) or `conflict`.
    pub state: String,
    pub describe: Option<String>,
}

/// Rejects a branch, tag, remote or commit name that git would read as an option (for example
/// `--upload-pack=...`) or that contains a NUL byte. Git itself forbids ref names that start with `-`,
/// so no legitimate name is refused.
pub fn ref_arg(s: &str) -> Result<(), String> {
    if s.starts_with('-') || s.contains('\0') {
        let shown: String = s.chars().filter(|c| !c.is_control()).take(60).collect();
        return Err(format!("'{shown}' is not a valid branch, tag, remote or commit name."));
    }
    Ok(())
}

/// `ref_arg` for an optional value (None and empty are fine).
pub fn opt_ref_arg(s: &Option<String>) -> Result<(), String> {
    s.as_deref().map_or(Ok(()), ref_arg)
}
