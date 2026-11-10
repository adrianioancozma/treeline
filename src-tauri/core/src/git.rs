use crate::status::redact_urls;
use serde::Serialize;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::RwLock;

/// Global git option that makes file names literal (never globs or ":" magic). Pass it as the
/// first argument of commands that take user-supplied file names. It must NOT be set globally:
/// git's own porcelain (e.g. `stash -u`) relies on pathspec magic internally.
pub const LITERAL: &str = "--literal-pathspecs";

/// Directories where a user-installed `git` usually lives on macOS/Linux. A GUI app launched from
/// Finder/the Dock inherits only a minimal PATH (/usr/bin:/bin:…), which excludes Homebrew.
#[cfg_attr(not(unix), allow(dead_code))]
pub const EXTRA_BIN_DIRS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"];

/// Returns `current` with the `extra` directories put in front (those already present are skipped),
/// so a Homebrew git is found before Apple's /usr/bin/git shim (which pops up an installer when
/// the developer tools are missing).
pub fn merge_path(current: &str, extra: &[&str], sep: char) -> String {
    let present: Vec<&str> = current.split(sep).collect();
    let mut parts: Vec<&str> = extra.iter().copied().filter(|d| !present.contains(d)).collect();
    parts.extend(present.into_iter().filter(|p| !p.is_empty()));
    parts.join(&sep.to_string())
}

/// The git executable the user picked in the app (Settings / the "Git not found" dialog).
/// `None` means "find it automatically". Loaded from the settings file at start-up.
pub static CUSTOM_GIT: RwLock<Option<PathBuf>> = RwLock::new(None);

pub fn set_custom_git(path: Option<PathBuf>) {
    if let Ok(mut g) = CUSTOM_GIT.write() {
        *g = path;
    }
}

pub fn custom_git() -> Option<PathBuf> {
    CUSTOM_GIT.read().ok().and_then(|g| g.clone())
}

/// Where Git for Windows normally lives when it is installed but missing from PATH
/// (a GUI app launched from the Start menu does not see the PATH of your terminal).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn windows_git_candidates() -> Vec<PathBuf> {
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
pub fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
        .unwrap_or(false)
}

/// The program to run for git: the user's chosen file, else `git` from PATH, else (Windows only)
/// a standard Git for Windows install folder.
pub fn git_program() -> PathBuf {
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

pub fn git_cmd() -> Command {
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
    // The app names the repository with -C. A GIT_DIR or GIT_WORK_TREE inherited from the environment would
    // silently point git somewhere else.
    for var in ["GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR", "GIT_INDEX_FILE"] {
        cmd.env_remove(var);
    }
    cmd
}

/// Start of the message produced when git itself could not be started (as opposed to git failing).
pub const SPAWN_ERROR_PREFIX: &str = "Could not run git";

pub fn spawn_error(e: std::io::Error) -> String {
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

pub fn git_info_now() -> GitInfo {
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
pub fn verify_git(path: &Path) -> Result<String, String> {
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

pub fn combined(out: &std::process::Output) -> String {
    let err = redact_urls(String::from_utf8_lossy(&out.stderr).trim());
    let std = redact_urls(String::from_utf8_lossy(&out.stdout).trim());
    match (std.is_empty(), err.is_empty()) {
        (_, true) => std,
        (true, false) => err,
        (false, false) => format!("{std}\n{err}"),
    }
}

/// Runs git and returns stdout; a non-zero exit becomes an `Err` with git's message.
pub fn run_git<I, S>(repo: &str, args: I) -> Result<String, String>
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
pub fn run_git_ok_codes<I, S>(repo: &str, args: I, ok: &[i32]) -> Result<String, String>
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
pub fn run_git_stdin<I, S>(repo: &str, args: I, input: &[u8]) -> Result<String, String>
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

pub fn run_op<I, S>(repo: &str, args: I, envs: &[(&str, String)]) -> Result<OpResult, String>
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
