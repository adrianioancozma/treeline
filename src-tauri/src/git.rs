//! Git engine core: process helpers, repository status, submodules and cloning.
//! Every operation shells out to the system `git` CLI on a blocking worker thread,
//! so the UI never waits on Git.

use serde::Serialize;
use std::ffi::OsStr;
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------------------
// Process helpers
// ---------------------------------------------------------------------------

/// Global git option that makes file names literal (never globs or ":" magic). Pass it as the
/// first argument of commands that take user-supplied file names. It must NOT be set globally:
/// git's own porcelain (e.g. `stash -u`) relies on pathspec magic internally.
pub(crate) const LITERAL: &str = "--literal-pathspecs";

pub(crate) fn git_cmd() -> Command {
    let mut cmd = Command::new("git");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: never flash a console window.
        cmd.creation_flags(0x0800_0000);
    }
    // Fail instead of hanging on a terminal prompt; credential helpers still work.
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    // Read-only commands (status, diff …) must not opportunistically rewrite `.git/index`:
    // that would trigger the file watcher and cause an endless refresh loop.
    cmd.env("GIT_OPTIONAL_LOCKS", "0");
    cmd
}

fn spawn_error(e: std::io::Error) -> String {
    format!("Could not run git (is it installed and on PATH?): {e}")
}

fn combined(out: &std::process::Output) -> String {
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let std = String::from_utf8_lossy(&out.stdout).trim().to_string();
    match (std.is_empty(), err.is_empty()) {
        (_, true) => std,
        (true, false) => err,
        (false, false) => format!("{std}\n{err}"),
    }
}

/// Runs git and returns stdout; a non-zero exit becomes an `Err` with git's message.
pub(crate) fn run_git<I, S>(repo: &str, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let out = git_cmd()
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(spawn_error)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(combined(&out))
    }
}

/// Like `run_git` but also accepts the listed non-zero exit codes (e.g. `diff --no-index` exits 1).
pub(crate) fn run_git_ok_codes<I, S>(repo: &str, args: I, ok: &[i32]) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let out = git_cmd()
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(spawn_error)?;
    let code = out.status.code().unwrap_or(-1);
    if out.status.success() || ok.contains(&code) {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(combined(&out))
    }
}

/// Runs git with `input` piped to stdin (patches, commit messages, path lists).
pub(crate) fn run_git_stdin<I, S>(repo: &str, args: I, input: &[u8]) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = git_cmd()
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(spawn_error)?;
    let mut stdin = child.stdin.take().ok_or("Could not open git's stdin")?;
    let data = input.to_vec();
    // Write on a thread so a large input can never deadlock against git's output.
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&data);
    });
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let _ = writer.join();
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(combined(&out))
    }
}

/// Result of an operation that can legitimately "fail" (conflicts, dirty tree …) without
/// being an application error. The UI shows `output` and refreshes the repository state.
#[derive(Serialize, Clone, Debug)]
pub struct OpResult {
    pub ok: bool,
    pub output: String,
}

pub(crate) fn run_op<I, S>(repo: &str, args: I, envs: &[(&str, String)]) -> Result<OpResult, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = git_cmd();
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(spawn_error)?;
    Ok(OpResult {
        ok: out.status.success(),
        output: combined(&out),
    })
}

pub(crate) async fn blocking<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Streaming (network) operations
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
struct OpProgress {
    id: String,
    line: String,
    percent: Option<u8>,
}

fn percent_of(line: &str) -> Option<u8> {
    let idx = line.find('%')?;
    let digits: String = line[..idx]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse().ok()
}

/// Runs git, streaming each progress line as an `op-progress` event tagged with `id`.
pub(crate) fn stream_git(
    app: &AppHandle,
    id: &str,
    cwd: Option<&str>,
    args: &[String],
) -> Result<OpResult, String> {
    let mut cmd = git_cmd();
    if let Some(dir) = cwd {
        cmd.arg("-C").arg(dir);
    }
    let mut child = cmd
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(spawn_error)?;

    let mut stdout = child.stdout.take().ok_or("Could not read git output")?;
    let out_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });

    let stderr = child.stderr.take().ok_or("Could not read git output")?;
    let mut reader = BufReader::new(stderr);
    let (mut buf, mut lines) = (Vec::new(), Vec::<String>::new());
    let mut byte = [0u8; 1];
    // git redraws progress with '\r', so split on both '\r' and '\n'.
    while reader.read(&mut byte).map_err(|e| e.to_string())? == 1 {
        if byte[0] == b'\r' || byte[0] == b'\n' {
            if !buf.is_empty() {
                let line = String::from_utf8_lossy(&buf).trim().to_string();
                let _ = app.emit(
                    "op-progress",
                    OpProgress {
                        id: id.to_string(),
                        percent: percent_of(&line),
                        line: line.clone(),
                    },
                );
                // Keep only the last redraw of a progress line; keep everything else.
                if line.contains('%') && lines.last().is_some_and(|l| l.contains('%')) {
                    lines.pop();
                }
                lines.push(line);
                if lines.len() > 200 {
                    lines.remove(0);
                }
                buf.clear();
            }
        } else {
            buf.push(byte[0]);
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let std = out_thread.join().unwrap_or_default();
    let mut output = std.trim().to_string();
    let err = lines.join("\n");
    if !err.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&err);
    }
    Ok(OpResult {
        ok: status.success(),
        output,
    })
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

#[derive(Serialize, Default, Debug)]
pub struct FileChange {
    pub path: String,
    /// Index (staged) state letter, `.` when unchanged.
    pub index: String,
    /// Working tree state letter, `.` when unchanged.
    pub worktree: String,
    pub orig_path: Option<String>,
    pub untracked: bool,
    pub conflicted: bool,
}

#[derive(Serialize, Default, Debug)]
pub struct RepoStatus {
    pub branch: Option<String>,
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<FileChange>,
    /// `merge`, `rebase`, `cherry-pick` or `revert` while one is in progress.
    pub state: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct Submodule {
    pub path: String,
    pub sha: String,
    /// `ok`, `uninitialized`, `modified` (checked-out commit differs) or `conflict`.
    pub state: String,
    pub describe: Option<String>,
}

pub(crate) fn parse_status(raw: &str) -> RepoStatus {
    let mut status = RepoStatus::default();
    let mut tokens = raw.split('\0');
    while let Some(tok) = tokens.next() {
        if tok.is_empty() {
            continue;
        }
        if let Some(rest) = tok.strip_prefix("# ") {
            if let Some(v) = rest.strip_prefix("branch.head ") {
                // A detached HEAD is reported as "(detached)"; model it as no branch.
                status.branch = Some(v.to_string()).filter(|b| b != "(detached)");
            } else if let Some(v) = rest.strip_prefix("branch.oid ") {
                if v != "(initial)" {
                    status.head = Some(v.to_string());
                }
            } else if let Some(v) = rest.strip_prefix("branch.upstream ") {
                status.upstream = Some(v.to_string());
            } else if let Some(v) = rest.strip_prefix("branch.ab ") {
                let mut parts = v.split(' ');
                status.ahead = parts
                    .next()
                    .and_then(|p| p.trim_start_matches('+').parse().ok())
                    .unwrap_or(0);
                status.behind = parts
                    .next()
                    .and_then(|p| p.trim_start_matches('-').parse().ok())
                    .unwrap_or(0);
            }
            continue;
        }
        match tok.chars().next().unwrap_or(' ') {
            '1' => {
                let f: Vec<&str> = tok.splitn(9, ' ').collect();
                if f.len() == 9 {
                    status.files.push(change(f[1], f[8], None, false));
                }
            }
            '2' => {
                let f: Vec<&str> = tok.splitn(10, ' ').collect();
                let orig = tokens.next().map(str::to_string);
                if f.len() == 10 {
                    status.files.push(change(f[1], f[9], orig, false));
                }
            }
            'u' => {
                let f: Vec<&str> = tok.splitn(11, ' ').collect();
                if f.len() == 11 {
                    let mut c = change(f[1], f[10], None, false);
                    c.conflicted = true;
                    status.files.push(c);
                }
            }
            '?' => {
                if let Some(path) = tok.strip_prefix("? ") {
                    status.files.push(change("??", path, None, true));
                }
            }
            _ => {}
        }
    }
    status
}

fn change(xy: &str, path: &str, orig: Option<String>, untracked: bool) -> FileChange {
    let mut chars = xy.chars();
    FileChange {
        path: path.to_string(),
        index: chars.next().unwrap_or('.').to_string(),
        worktree: chars.next().unwrap_or('.').to_string(),
        orig_path: orig,
        untracked,
        conflicted: false,
    }
}

/// Detects an in-progress merge / rebase / cherry-pick / revert from the git directory.
fn repo_state(path: &str) -> Option<String> {
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

/// Returns the repository's top-level folder, or an error if `path` is not inside one.
#[tauri::command]
pub async fn git_toplevel(path: String) -> Result<String, String> {
    blocking(move || {
        let out = run_git(&path, ["rev-parse", "--show-toplevel"])
            .map_err(|_| "This folder is not a Git repository.".to_string())?;
        Ok(out.trim().to_string())
    })
    .await
}

#[tauri::command]
pub async fn repo_status(path: String) -> Result<RepoStatus, String> {
    blocking(move || {
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
    })
    .await
}

#[tauri::command]
pub async fn list_submodules(path: String) -> Result<Vec<Submodule>, String> {
    blocking(move || {
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
    })
    .await
}

/// Extracts submodules (gitlinks, mode 160000) from `git ls-files --stage -z` output as
/// `(recorded sha, path, conflicted)`. A gitlink with a non-zero stage is mid-merge.
pub(crate) fn parse_gitlinks(raw: &str) -> Vec<(String, String, bool)> {
    let mut out: Vec<(String, String, bool)> = Vec::new();
    for entry in raw.split('\0') {
        let Some((meta, path)) = entry.split_once('\t') else { continue };
        let mut parts = meta.split(' ');
        let (Some(mode), Some(sha), Some(stage)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if mode != "160000" {
            continue;
        }
        match out.iter_mut().find(|(_, p, _)| p == path) {
            Some(existing) => existing.2 = true,
            None => out.push((sha.to_string(), path.to_string(), stage != "0")),
        }
    }
    out
}

/// Initialises and checks out one submodule (and any nested ones) so it can be opened.
#[tauri::command]
pub async fn init_submodule(path: String, submodule: String) -> Result<OpResult, String> {
    blocking(move || {
        run_op(
            &path,
            [LITERAL, "submodule", "update", "--init", "--recursive", "--", &submodule],
            &[],
        )
    })
    .await
}

/// Updates every submodule to the commit recorded by the superproject.
#[tauri::command]
pub async fn update_submodules(path: String, remote: bool) -> Result<OpResult, String> {
    blocking(move || {
        let mut args = vec!["submodule", "update", "--init", "--recursive"];
        if remote {
            args.push("--remote");
        }
        run_op(&path, args, &[])
    })
    .await
}

#[derive(Serialize)]
pub struct CloneOutcome {
    pub path: String,
    /// Set when the repository was cloned but something secondary (usually a submodule) failed.
    pub warning: Option<String>,
}

/// Clones `url` into `dest`, streaming progress as `op-progress` events tagged with `id`.
/// A clone whose only failure is a submodule still counts: the repository is usable.
#[tauri::command]
pub async fn clone_repo(
    app: AppHandle,
    id: String,
    url: String,
    dest: String,
    recursive: bool,
) -> Result<CloneOutcome, String> {
    blocking(move || {
        let mut args = vec!["clone".to_string(), "--progress".to_string()];
        if recursive {
            args.push("--recurse-submodules".into());
        }
        args.extend(["--".to_string(), url, dest.clone()]);
        let res = stream_git(&app, &id, None, &args)?;
        if res.ok {
            return Ok(CloneOutcome { path: dest, warning: None });
        }
        if is_submodule_failure(&res.output) && run_git(&dest, ["rev-parse", "--git-dir"]).is_ok() {
            return Ok(CloneOutcome {
                path: dest,
                warning: Some(format!(
                    "Cloned, but some submodules could not be fetched. Open the repository and use Update submodules to retry.\n\n{}",
                    last_lines(&res.output, 6)
                )),
            });
        }
        Err(res.output)
    })
    .await
}

fn is_submodule_failure(output: &str) -> bool {
    let o = output.to_lowercase();
    o.contains("submodule") || o.contains("failed to clone")
}

fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_and_changes() {
        let raw = "# branch.oid abc\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
                   1 .M N... 100644 100644 100644 aaa bbb src/a b.ts\0\
                   2 R. N... 100644 100644 100644 aaa bbb R100 new.ts\0old.ts\0\
                   u UU N... 100644 100644 100644 100644 a b c conflicted.ts\0\
                   ? notes.txt\0";
        let s = parse_status(raw);
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.head.as_deref(), Some("abc"));
        assert_eq!((s.ahead, s.behind), (2, 1));
        assert_eq!(s.files.len(), 4);
        assert_eq!(s.files[0].path, "src/a b.ts");
        assert_eq!(s.files[0].worktree, "M");
        assert_eq!(s.files[1].orig_path.as_deref(), Some("old.ts"));
        assert!(s.files[2].conflicted);
        assert!(s.files[3].untracked);
    }

    #[test]
    fn detached_head_is_not_a_branch() {
        let s = parse_status("# branch.oid abc\0# branch.head (detached)\0");
        assert_eq!(s.branch, None);
        assert_eq!(s.head.as_deref(), Some("abc"));
    }

    #[test]
    fn initial_commit_has_no_head() {
        let s = parse_status("# branch.oid (initial)\0# branch.head main\0");
        assert_eq!(s.head, None);
    }

    #[test]
    fn parses_gitlinks_only() {
        let raw = "100644 aaa 0\tREADME.md\0160000 bbb 0\tsrc/lib\0160000 ccc 1\tsrc/conflicted\0160000 ddd 2\tsrc/conflicted\0";
        let g = parse_gitlinks(raw);
        assert_eq!(
            g,
            vec![
                ("bbb".to_string(), "src/lib".to_string(), false),
                ("ccc".to_string(), "src/conflicted".to_string(), true),
            ]
        );
    }

    #[test]
    fn recognises_submodule_clone_failures() {
        assert!(is_submodule_failure("fatal: clone of 'x' into submodule path 'a' failed"));
        assert!(!is_submodule_failure("fatal: destination path 'x' already exists and is not an empty directory."));
        assert_eq!(last_lines("a\nb\nc", 2), "b\nc");
    }

    #[test]
    fn extracts_percent() {
        assert_eq!(percent_of("Receiving objects:  42% (1/2)"), Some(42));
        assert_eq!(percent_of("Cloning into 'x'..."), None);
    }
}
