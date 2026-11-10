//! The read-only commands, without Tauri: what one reload of a repository and the history and diff views ask for.
//! The desktop app calls them through thin Tauri commands; a helper next to a remote repository calls them directly.

use crate::git::{LITERAL, run_git, run_git_ok_codes};
use serde::{Deserialize, Serialize};
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn temp_repo(name: &str) -> String {
        let d = std::env::temp_dir().join(format!("treeline-core-read-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let d = std::fs::canonicalize(&d).unwrap().to_string_lossy().to_string();
        git_at(&d, &["init", "-q", "-b", "main"], 0);
        crate::trust::allow_repo(&d);
        d
    }

    /// Runs git with a fixed author and the given commit time (seconds), so commit order by date is controlled.
    fn git_at(repo: &str, args: &[&str], time: i64) {
        let date = format!("{} +0000", 1_700_000_000 + time); // a real timestamp (git rejects tiny ones)
        let ok = Command::new("git")
            .args(["-c", "user.name=T", "-c", "user.email=t@example.com", "-C", repo])
            .args(args)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn history_keeps_each_branch_together() {
        let _serial = crate::serial_git();
        // main: A(1) - M1(2) - M2(4); feature from A: F1(3) - F2(5); merge X(6). By date the branches interleave
        // (F2 M2 F1 M1); topological order keeps F1 next to F2 and M1 next to M2, which the graph lanes rely on.
        let r = temp_repo("topo");
        git_at(&r, &["commit", "-q", "--allow-empty", "-m", "A"], 1);
        git_at(&r, &["checkout", "-q", "-b", "feature"], 1);
        git_at(&r, &["commit", "-q", "--allow-empty", "-m", "F1"], 3);
        git_at(&r, &["commit", "-q", "--allow-empty", "-m", "F2"], 5);
        git_at(&r, &["checkout", "-q", "main"], 1);
        git_at(&r, &["commit", "-q", "--allow-empty", "-m", "M1"], 2);
        git_at(&r, &["commit", "-q", "--allow-empty", "-m", "M2"], 4);
        git_at(&r, &["merge", "-q", "--no-ff", "--no-edit", "-m", "X", "feature"], 6);
        let log: Vec<String> = git_log(r.clone(), 0, 50, false, None, None).unwrap().into_iter().map(|c| c.subject).collect();
        let at = |s: &str| log.iter().position(|x| x == s).unwrap();
        assert_eq!(log[0], "X");
        assert_eq!(at("F1").abs_diff(at("F2")), 1, "feature commits stay together: {log:?}");
        assert_eq!(at("M1").abs_diff(at("M2")), 1, "main commits stay together: {log:?}");
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn detects_an_operation_in_progress() {
        let _serial = crate::serial_git();
        let r = temp_repo("state");
        git_at(&r, &["commit", "-q", "--allow-empty", "-m", "A"], 1);
        assert_eq!(repo_state(&r), None);
        let git_dir = format!("{r}/.git");
        std::fs::write(format!("{git_dir}/MERGE_HEAD"), "0000000000000000000000000000000000000000\n").unwrap();
        assert_eq!(repo_state(&r).as_deref(), Some("merge"));
        std::fs::remove_file(format!("{git_dir}/MERGE_HEAD")).unwrap();
        std::fs::create_dir(format!("{git_dir}/rebase-merge")).unwrap();
        assert_eq!(repo_state(&r).as_deref(), Some("rebase"), "a rebase wins over everything else");
        std::fs::write(format!("{git_dir}/CHERRY_PICK_HEAD"), "x\n").unwrap();
        assert_eq!(repo_state(&r).as_deref(), Some("rebase"));
        std::fs::remove_dir(format!("{git_dir}/rebase-merge")).unwrap();
        assert_eq!(repo_state(&r).as_deref(), Some("cherry-pick"));
        let _ = std::fs::remove_dir_all(&r);
    }
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

pub fn commit_detail(path: String, sha: String) -> Result<CommitDetail, String> {
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

pub fn non_empty(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

/// Dates reach Git as one `--since=` argument; only the characters of an ISO 8601 time are allowed.
pub fn iso_time(v: &str) -> Result<&str, String> {
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
pub fn log_filter_args(all: bool, f: &LogFilter, with_text: bool) -> Result<Vec<String>, String> {
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

pub const FILE_LOG_FORMAT: &str = "--format=%x1e%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%D%x1f%s%x1f";

pub fn file_log(path: String, file: String, skip: u32, limit: u32) -> Result<Vec<FileCommit>, String> {
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
    }

pub fn git_blame(path: String, file: String, rev: Option<String>) -> Result<Vec<BlameLine>, String> {
        ensure_allowed(&path)?;
        opt_ref_arg(&rev)?;
        let mut args: Vec<String> = vec![LITERAL.into(), "blame".into(), "--line-porcelain".into()];
        if let Some(r) = rev.filter(|r| !r.is_empty()) {
            args.push(r);
        }
        args.push("--".into());
        args.push(file);
        run_git(&path, args).map(|raw| parse_blame(&raw))
    }

pub fn git_log_matches(path: String, all: bool, filter: LogFilter) -> Result<Vec<String>, String> {
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
    }

/// A commit id as it appears in a rebase todo line: only hex digits, so it cannot carry a newline
/// (which would add a line of its own, such as an `exec`) or a space.
pub fn is_hex_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}
