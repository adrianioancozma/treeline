//! Git engine core: process helpers, repository status, submodules and cloning.
//! Every operation shells out to the system `git` CLI on a blocking worker thread,
//! so the UI never waits on Git.

use serde::Serialize;
use std::ffi::OsStr;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::RwLock;
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------------------
// Process helpers
// ---------------------------------------------------------------------------

/// Global git option that makes file names literal (never globs or ":" magic). Pass it as the
/// first argument of commands that take user-supplied file names. It must NOT be set globally:
/// git's own porcelain (e.g. `stash -u`) relies on pathspec magic internally.
pub(crate) const LITERAL: &str = "--literal-pathspecs";

/// Directories where a user-installed `git` usually lives on macOS/Linux. A GUI app launched from
/// Finder/the Dock inherits only a minimal PATH (/usr/bin:/bin:…), which excludes Homebrew.
#[cfg_attr(not(unix), allow(dead_code))]
const EXTRA_BIN_DIRS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"];

/// Returns `current` with the `extra` directories put in front (those already present are skipped),
/// so a Homebrew git is found before Apple's /usr/bin/git shim (which pops up an installer when
/// the developer tools are missing).
pub(crate) fn merge_path(current: &str, extra: &[&str], sep: char) -> String {
    let present: Vec<&str> = current.split(sep).collect();
    let mut parts: Vec<&str> = extra.iter().copied().filter(|d| !present.contains(d)).collect();
    parts.extend(present.into_iter().filter(|p| !p.is_empty()));
    parts.join(&sep.to_string())
}

/// The git executable the user picked in the app (Settings / the "Git not found" dialog).
/// `None` means "find it automatically". Loaded from the settings file at start-up.
static CUSTOM_GIT: RwLock<Option<PathBuf>> = RwLock::new(None);

pub(crate) fn set_custom_git(path: Option<PathBuf>) {
    if let Ok(mut g) = CUSTOM_GIT.write() {
        *g = path;
    }
}

fn custom_git() -> Option<PathBuf> {
    CUSTOM_GIT.read().ok().and_then(|g| g.clone())
}

/// Where Git for Windows normally lives when it is installed but missing from PATH
/// (a GUI app launched from the Start menu does not see the PATH of your terminal).
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_git_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Ok(base) = std::env::var(var) {
            v.push(PathBuf::from(base).join("Git").join("cmd").join("git.exe"));
        }
    }
    if let Ok(base) = std::env::var("LOCALAPPDATA") {
        v.push(PathBuf::from(base).join("Programs").join("Git").join("cmd").join("git.exe"));
    }
    v
}

/// True when an executable called `name` exists in one of the PATH directories.
fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
        .unwrap_or(false)
}

/// The program to run for git: the user's chosen file, else `git` from PATH, else (Windows only)
/// a standard Git for Windows install folder.
fn git_program() -> PathBuf {
    if let Some(p) = custom_git().filter(|p| p.is_file()) {
        return p;
    }
    #[cfg(windows)]
    if !on_path("git.exe") {
        if let Some(found) = windows_git_candidates().into_iter().find(|p| p.is_file()) {
            return found;
        }
    }
    #[cfg(not(windows))]
    let _ = on_path;
    PathBuf::from("git")
}

pub(crate) fn git_cmd() -> Command {
    let mut cmd = Command::new(git_program());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: never flash a console window.
        cmd.creation_flags(0x0800_0000);
    }
    #[cfg(unix)]
    cmd.env(
        "PATH",
        merge_path(&std::env::var("PATH").unwrap_or_default(), &EXTRA_BIN_DIRS, ':'),
    );
    // Fail instead of hanging on a terminal prompt; credential helpers still work.
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    // Read-only commands (status, diff …) must not opportunistically rewrite `.git/index`:
    // that would trigger the file watcher and cause an endless refresh loop.
    cmd.env("GIT_OPTIONAL_LOCKS", "0");
    // The `ext::` transport runs an arbitrary command as a "remote". Git leaves it off by default, but a
    // repository's own config could switch it on; the command line wins over that.
    cmd.args(["-c", "protocol.ext.allow=never"]);
    // `core.fsmonitor` can name a program that git runs on every status. A repository's own config could
    // set it, so it is always switched off here (this also turns off git's built-in fsmonitor daemon).
    cmd.args(["-c", "core.fsmonitor=false"]);
    cmd
}

/// Start of the message produced when git itself could not be started (as opposed to git failing).
const SPAWN_ERROR_PREFIX: &str = "Could not run git";

fn spawn_error(e: std::io::Error) -> String {
    format!("{SPAWN_ERROR_PREFIX} (is it installed and on PATH?): {e}")
}

/// What the app knows about the git it will use.
#[derive(Serialize)]
pub struct GitInfo {
    /// Output of `git --version`, or `None` when git cannot be started.
    pub version: Option<String>,
    /// The executable that will be run.
    pub path: String,
    /// The path the user chose, if any.
    pub custom: Option<String>,
}

fn git_info_now() -> GitInfo {
    let program = git_program();
    let version = Command::new(&program).arg("--version").output().ok().filter(|o| o.status.success()).map(
        |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
    );
    GitInfo {
        version,
        path: program.to_string_lossy().into_owned(),
        custom: custom_git().map(|p| p.to_string_lossy().into_owned()),
    }
}

/// Checks that `path` is a working git executable and returns its version line.
pub(crate) fn verify_git(path: &Path) -> Result<String, String> {
    if !path.is_file() {
        return Err("That file does not exist.".into());
    }
    let out = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|e| format!("That file could not be run: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && text.starts_with("git version") {
        Ok(text)
    } else {
        Err("That file is not Git (it did not answer to “git --version”).".into())
    }
}

/// Reports which git the app uses and whether it works.
#[tauri::command]
pub async fn git_info() -> Result<GitInfo, String> {
    blocking(|| Ok(git_info_now())).await
}

/// Hides credentials embedded in URLs, for example `https://user:token@host/repo.git` or
/// `https://token@host/repo.git`, so that git's error and progress messages never put a secret on
/// screen, in a bug report or in a copied error. Only the user part is replaced; the host stays.
/// For web URLs any user part is hidden (it is often a token); for other schemes only `user:password`.
pub(crate) fn redact_urls(text: &str) -> String {
    let single_quote = char::from(39);
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let lower = head.to_ascii_lowercase();
        let web = lower.ends_with("http://") || lower.ends_with("https://");
        // The authority part ends at the first '/', whitespace or quote.
        let end = tail
            .find(|c: char| c == '/' || c.is_whitespace() || c == single_quote || c == '"')
            .unwrap_or(tail.len());
        let authority = &tail[..end];
        match authority.rfind('@') {
            Some(at) if web || authority[..at].contains(':') => {
                out.push_str("***");
                out.push_str(&authority[at..]);
            }
            _ => out.push_str(authority),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

fn combined(out: &std::process::Output) -> String {
    let err = redact_urls(String::from_utf8_lossy(&out.stderr).trim());
    let std = redact_urls(String::from_utf8_lossy(&out.stdout).trim());
    match (std.is_empty(), err.is_empty()) {
        (_, true) => std,
        (true, false) => err,
        (false, false) => format!("{std}\n{err}"),
    }
}

/// Rejects a branch, tag, remote or commit name that git would read as an option (for example
/// `--upload-pack=...`) or that contains a NUL byte. Git itself forbids ref names that start with `-`,
/// so no legitimate name is refused.
pub(crate) fn ref_arg(s: &str) -> Result<(), String> {
    if s.starts_with('-') || s.contains('\0') {
        let shown: String = s.chars().filter(|c| !c.is_control()).take(60).collect();
        return Err(format!("'{shown}' is not a valid branch, tag, remote or commit name."));
    }
    Ok(())
}

/// `ref_arg` for an optional value (None and empty are fine).
pub(crate) fn opt_ref_arg(s: &Option<String>) -> Result<(), String> {
    s.as_deref().map_or(Ok(()), ref_arg)
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
                let line = redact_urls(String::from_utf8_lossy(&buf).trim());
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
    let mut output = redact_urls(std.trim());
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
            .map_err(|e| {
                // Only blame the folder when git actually ran; "git could not start" must stay visible.
                if e.starts_with(SPAWN_ERROR_PREFIX) { e } else { "This folder is not a Git repository.".to_string() }
            })?;
        Ok(out.trim().to_string())
    })
    .await
}

/// Describes a repository config entry that makes git run a program, or `None` when it is harmless
/// or something the app never triggers. Git LFS filters and plain credential helpers such as
/// `manager` are normal and ignored.
pub(crate) fn risky_config_entry(key: &str, value: &str) -> Option<String> {
    let k = key.to_ascii_lowercase();
    let backslash = char::from(92);
    let program = |why: &str| Some(format!("{key} = {value}: {why}"));
    match k.as_str() {
        "core.sshcommand" => return program("runs a program to connect over SSH"),
        "core.hookspath" => return program("git hooks from that folder run when you commit, merge or check out"),
        "core.editor" | "sequence.editor" | "core.pager" | "core.askpass" | "gpg.program" | "core.gitproxy" | "diff.external" => {
            return program("runs a program")
        }
        "credential.helper" => {
            return (value.starts_with('!') || value.contains('/') || value.contains(backslash))
                .then(|| format!("{key} = {value}: runs a program to supply credentials"));
        }
        "include.path" => return program("pulls in another config file, which is not checked here"),
        _ => {}
    }
    if k.starts_with("includeif.") && k.ends_with(".path") {
        return program("pulls in another config file, which is not checked here");
    }
    if k.starts_with("filter.") && (k.ends_with(".clean") || k.ends_with(".smudge") || k.ends_with(".process")) {
        return (!value.trim_start().starts_with("git-lfs"))
            .then(|| format!("{key} = {value}: runs a program on files when they are staged or checked out"));
    }
    if k.starts_with("merge.") && k.ends_with(".driver") {
        return program("runs a program to merge files");
    }
    if k.starts_with("diff.") && (k.ends_with(".textconv") || k.ends_with(".command")) {
        return program("runs a program to convert or show diffs");
    }
    if k.starts_with("credential.") && k.ends_with(".helper") {
        return (value.starts_with('!') || value.contains('/') || value.contains(backslash))
            .then(|| format!("{key} = {value}: runs a program to supply credentials"));
    }
    if k.starts_with("remote.") && (k.ends_with(".vcs") || k.ends_with(".uploadpack") || k.ends_with(".receivepack")) {
        return program("runs a program for this remote");
    }
    if k.starts_with("submodule.") && k.ends_with(".update") && value.starts_with('!') {
        return program("runs a command when updating this submodule");
    }
    None
}

/// What in this repository's own settings or hooks could make git run a program when the app (or
/// you) commits, merges, checks out or fetches. A repository you made or cloned from a source you
/// trust normally has none of this; a folder handed to you with its `.git` included can.
pub(crate) fn repo_risks_now(path: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let raw = run_git(path, ["config", "--local", "--list", "-z"])?;
    for entry in raw.split('\0').filter(|e| !e.is_empty()) {
        let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
        if let Some(line) = risky_config_entry(key, value) {
            out.push(line);
        }
    }
    // Hook scripts (git ships only *.sample files).
    if let Ok(dir) = run_git(path, ["rev-parse", "--git-path", "hooks"]) {
        let dir = PathBuf::from(dir.trim());
        let dir = if dir.is_absolute() { dir } else { Path::new(path).join(dir) };
        if let Ok(read) = std::fs::read_dir(&dir) {
            let mut names: Vec<String> = read
                .flatten()
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.ends_with(".sample"))
                .collect();
            names.sort();
            if !names.is_empty() {
                out.push(format!("Hook scripts ({}): these run when you commit, merge, check out or push", names.join(", ")));
            }
        }
    }
    Ok(out)
}

/// Lists the ways this repository could make git run a program (see `repo_risks_now`).
#[tauri::command]
pub async fn repo_risks(path: String) -> Result<Vec<String>, String> {
    blocking(move || repo_risks_now(&path)).await
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

/// Picks the ref a submodule should check out for `branch`: the local branch if there is one,
/// otherwise a remote-tracking branch of that name (`origin` preferred). `None` = no such branch.
pub(crate) fn pick_branch_source(refs: &str, branch: &str) -> Option<(bool, String)> {
    let mut remote: Option<String> = None;
    for line in refs.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line == branch {
            return Some((true, line.to_string()));
        }
        // Remote-tracking refs look like `origin/feature/x`; the remote name has no slash.
        if let Some((r, rest)) = line.split_once('/') {
            if rest == branch && !line.ends_with("/HEAD") && (remote.is_none() || r == "origin") {
                remote = Some(line.to_string());
            }
        }
    }
    remote.map(|r| (false, r))
}

/// Switches every checked-out submodule to `branch` when a branch of that name exists there
/// (locally, or as a remote-tracking branch, which gets a local tracking branch). Submodules that
/// are not initialised or lack the branch are left alone. A submodule whose checkout fails (for
/// example uncommitted changes) is reported but does not stop the others.
#[tauri::command]
pub async fn switch_submodules_branch(path: String, branch: String) -> Result<OpResult, String> {
    blocking(move || {
        ref_arg(&branch)?;
        let raw = run_git(&path, ["ls-files", "--stage", "-z"])?;
        let root = PathBuf::from(&path);
        let (mut ok, mut lines) = (true, Vec::new());
        for (_, sub, _) in parse_gitlinks(&raw) {
            let dir = root.join(&sub);
            if !dir.join(".git").exists() {
                lines.push(format!("{sub}: skipped (not initialised)"));
                continue;
            }
            let dir_s = dir.to_string_lossy().into_owned();
            let refs = run_git(
                &dir_s,
                ["for-each-ref", "--format=%(refname:short)", "refs/heads", "refs/remotes"],
            )?;
            let Some((local, source)) = pick_branch_source(&refs, &branch) else {
                lines.push(format!("{sub}: skipped (no branch {branch})"));
                continue;
            };
            let res = if local {
                run_op(&dir_s, ["checkout", &branch], &[])?
            } else {
                run_op(&dir_s, ["checkout", "--track", "-b", &branch, &source], &[])?
            };
            if res.ok {
                lines.push(format!("{sub}: switched to {branch}"));
            } else {
                ok = false;
                lines.push(format!("{sub}: FAILED\n{}", res.output.trim()));
            }
        }
        if lines.is_empty() {
            lines.push("No submodules.".into());
        }
        Ok(OpResult { ok, output: lines.join("\n") })
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
    fn merge_path_puts_missing_dirs_first_without_duplicates() {
        let p = merge_path("/usr/bin:/bin:/usr/local/bin", &["/opt/homebrew/bin", "/usr/local/bin"], ':');
        assert_eq!(p, "/opt/homebrew/bin:/usr/bin:/bin:/usr/local/bin");
        assert_eq!(merge_path("", &["/a"], ':'), "/a");
        assert_eq!(merge_path("/x:", &["/a"], ':'), "/a:/x");
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
    fn a_repository_cannot_run_a_program_through_core_fsmonitor() {
        let dir = std::env::temp_dir().join(format!("treeline-fsm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = dir.to_string_lossy().to_string();
        let git = |args: &[&str]| {
            run_git(&repo, ["-c", "user.name=t", "-c", "user.email=t@t"].iter().chain(args.iter()).copied()).unwrap()
        };
        git(&["init", "-q"]);
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "one"]);

        let marker = dir.join("marker.txt");
        let hook = dir.join("hook.sh");
        let fwd = |p: &Path| p.to_string_lossy().replace('\\', "/");
        std::fs::write(&hook, format!("#!/bin/sh\necho ran > \"{}\"\n", fwd(&marker))).unwrap();
        git(&["config", "core.fsmonitor", &fwd(&hook)]);

        // Control: plain git does run the program. If it does not on this machine the check proves nothing.
        let _ = Command::new("git").arg("-C").arg(&repo).args(["status", "-s"]).output();
        let control_ran = marker.exists();
        let _ = std::fs::remove_file(&marker);

        git(&["status", "-s"]);
        assert!(!marker.exists(), "the hook must not run through the app's git calls");
        if !control_ran {
            eprintln!("note: plain git did not run the hook here, so this run could not show the difference");
        }

        // The same setting arriving through an included file (which can even be a tracked file that a
        // later checkout changes) is covered too, because the override does not depend on where it came from.
        git(&["config", "--unset", "core.fsmonitor"]);
        let extra = dir.join("extra.cfg");
        std::fs::write(&extra, format!("[core]\n\tfsmonitor = {}\n", fwd(&hook))).unwrap();
        git(&["config", "include.path", &fwd(&extra)]);
        git(&["status", "-s"]);
        assert!(!marker.exists(), "the hook must not run when it comes from an included file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn classifies_risky_repository_settings() {
        let risky = [
            ("core.hooksPath", ".husky"),
            ("core.sshCommand", "ssh -i key"),
            ("filter.evil.smudge", "calc.exe"),
            ("merge.evil.driver", "calc %O %A %B"),
            ("diff.evil.textconv", "calc"),
            ("credential.helper", "!echo pw"),
            ("credential.https://x.y.helper", "/opt/helper"),
            ("include.path", "../x.cfg"),
            ("includeIf.gitdir:/a/.path", "x"),
            ("remote.origin.vcs", "ext"),
            ("submodule.a.update", "!calc"),
            ("core.editor", "vim"),
        ];
        for (k, v) in risky {
            assert!(risky_config_entry(k, v).is_some(), "{k}");
        }
        let fine = [
            ("filter.lfs.clean", "git-lfs clean -- %f"),
            ("filter.lfs.smudge", "git-lfs smudge -- %f"),
            ("filter.lfs.process", "git-lfs filter-process"),
            ("credential.helper", "manager"),
            ("credential.helper", "osxkeychain"),
            ("core.autocrlf", "true"),
            ("user.name", "Me"),
            ("alias.st", "!echo status"),
            ("remote.origin.url", "https://x"),
            ("branch.main.remote", "origin"),
        ];
        for (k, v) in fine {
            assert!(risky_config_entry(k, v).is_none(), "{k}");
        }
    }

    #[test]
    fn a_repository_with_hooks_and_drivers_is_reported() {
        let dir = std::env::temp_dir().join(format!("treeline-risk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = dir.to_string_lossy().to_string();
        run_git(&repo, ["init", "-q"]).unwrap();
        assert!(repo_risks_now(&repo).unwrap().is_empty(), "a fresh repository is clean (sample hooks are ignored)");

        std::fs::write(dir.join(".git").join("hooks").join("pre-commit"), "#!/bin/sh\n").unwrap();
        run_git(&repo, ["config", "merge.evil.driver", "calc"]).unwrap();
        run_git(&repo, ["config", "filter.lfs.clean", "git-lfs clean -- %f"]).unwrap();
        let risks = repo_risks_now(&repo).unwrap();
        assert!(risks.iter().any(|r| r.contains("Hook scripts") && r.contains("pre-commit")), "{risks:?}");
        assert!(risks.iter().any(|r| r.contains("merge.evil.driver")), "{risks:?}");
        assert!(!risks.iter().any(|r| r.contains("git-lfs")), "LFS is normal: {risks:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn credentials_in_urls_are_hidden() {
        let cases = [
            ("fatal: unable to access 'https://user:ghp_secret@github.com/a/b.git/': 403", "fatal: unable to access 'https://***@github.com/a/b.git/': 403"),
            ("remote: https://ghp_tokenonly@github.com/a/b.git", "remote: https://***@github.com/a/b.git"),
            ("cloning ssh://me:pw@host.example/repo", "cloning ssh://***@host.example/repo"),
            ("pushing ssh://git@github.com/a/b.git", "pushing ssh://git@github.com/a/b.git"),
            ("plain https://github.com/a/b.git here", "plain https://github.com/a/b.git here"),
            ("two https://a:b@h1/x and https://c@h2/y", "two https://***@h1/x and https://***@h2/y"),
            ("no urls at all", "no urls at all"),
            ("trailing https://u:p@host", "trailing https://***@host"),
            ("an email me@example.com stays", "an email me@example.com stays"),
        ];
        for (input, want) in cases {
            assert_eq!(redact_urls(input), want, "{input}");
        }
    }

    #[test]
    fn option_like_names_are_refused() {
        for bad in ["--upload-pack=calc.exe", "-D", "--orphan=x", "a\0b"] {
            assert!(ref_arg(bad).is_err(), "{bad:?}");
        }
        for ok in ["main", "feature/x-1", "origin/develop", "v1.2.3", "HEAD~2", "abc123", "refs/heads/x", "stash@{0}", ""] {
            assert!(ref_arg(ok).is_ok(), "{ok:?}");
        }
        assert!(opt_ref_arg(&None).is_ok());
        assert!(opt_ref_arg(&Some("--x".into())).is_err());
        assert!(opt_ref_arg(&Some("main".into())).is_ok());
    }

    #[test]
    fn picks_submodule_branch_source() {
        let refs = "main\nfeature/x\norigin/HEAD\norigin/main\nfork/dev\norigin/dev\norigin/feature/y\n";
        assert_eq!(pick_branch_source(refs, "feature/x"), Some((true, "feature/x".into())));
        assert_eq!(pick_branch_source(refs, "dev"), Some((false, "origin/dev".into())));
        assert_eq!(pick_branch_source(refs, "feature/y"), Some((false, "origin/feature/y".into())));
        assert_eq!(pick_branch_source(refs, "HEAD"), None);
        assert_eq!(pick_branch_source(refs, "nope"), None);
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

#[cfg(test)]
mod git_path_tests {
    use super::*;

    #[test]
    fn verify_git_rejects_missing_and_non_git_files() {
        assert!(verify_git(Path::new("/definitely/not/here/git")).is_err());
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("notgit.txt");
        std::fs::write(&fake, "x").unwrap();
        assert!(verify_git(&fake).is_err());
    }

    /// A chosen file that no longer exists must be ignored, never break every git call.
    #[test]
    fn missing_custom_git_falls_back_to_detection() {
        set_custom_git(Some(PathBuf::from("/definitely/not/here/git")));
        let program = git_program();
        set_custom_git(None);
        assert_ne!(program, PathBuf::from("/definitely/not/here/git"));
    }

    /// The installed-app situation: PATH has no git, yet the standard Git for Windows folder is
    /// found. Runs the probe in a child process with an empty PATH so other tests are unaffected.
    #[cfg(windows)]
    #[test]
    fn windows_fallback_finds_git_when_path_is_empty() {
        const NAME: &str = "git::git_path_tests::windows_fallback_finds_git_when_path_is_empty";
        if std::env::var_os("TREELINE_PROBE").is_some() {
            let info = git_info_now();
            assert!(info.version.is_some(), "git not found with empty PATH: {}", info.path);
            return;
        }
        if windows_git_candidates().iter().all(|p| !p.is_file()) {
            return; // Git is installed somewhere unusual on this machine; nothing to probe.
        }
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env("TREELINE_PROBE", "1")
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    }

    #[test]
    fn toplevel_keeps_spawn_errors_visible() {
        let e = spawn_error(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert!(e.starts_with(SPAWN_ERROR_PREFIX));
    }
}
