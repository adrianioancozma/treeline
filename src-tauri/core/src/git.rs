use crate::status::redact_urls;
use serde::Serialize;
use std::ffi::OsStr;
use std::io::{BufReader, Read, Write};
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

/// Receives the progress lines of a streaming git command (fetch, pull, push, clone): the app shows them in its
/// window; a helper on another computer sends them over its connection.
pub trait ProgressSink {
    fn progress(&self, id: &str, line: &str, percent: Option<u8>);
}

pub fn percent_of(line: &str) -> Option<u8> {
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

/// Runs git, handing each progress line (already redacted) to `sink`, tagged with `id`.
pub fn stream_git(
    sink: &dyn ProgressSink,
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
                sink.progress(id, &line, percent_of(&line));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<(String, String, Option<u8>)>>);
    impl ProgressSink for Recorder {
        fn progress(&self, id: &str, line: &str, percent: Option<u8>) {
            self.0.lock().unwrap().push((id.to_string(), line.to_string(), percent));
        }
    }

    #[cfg(unix)]
    #[test]
    fn progress_lines_reach_the_sink_tagged_and_without_secrets() {
        use std::os::unix::fs::PermissionsExt;
        let _serial = crate::serial_git();
        // A stand-in for git that prints what a real fetch can print: a progress line redrawn with '\r', then a URL
        // with a password (real git hides it in some messages, not in all; this layer must not rely on that).
        let dir = std::env::temp_dir().join(format!("treeline-core-stream-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("git");
        std::fs::write(&fake, "#!/bin/sh\nprintf 'Receiving objects:  42%% (1/2)\\rReceiving objects: 100%% (2/2)\\nremote: https://user:s3cr3t@example.com/x.git\\n' >&2\necho 'From https://user:s3cr3t@example.com/x.git'\nexit 1\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        set_custom_git(Some(fake));

        let rec = Recorder::default();
        let res = stream_git(&rec, "op-7", None, &["fetch".to_string()]).unwrap();
        set_custom_git(None);
        let _ = std::fs::remove_dir_all(&dir);

        let lines = rec.0.lock().unwrap();
        assert!(!res.ok, "the command failed");
        assert_eq!(lines.len(), 3, "the redrawn progress line arrives twice, then the remote line: {lines:?}");
        assert!(lines.iter().all(|(id, _, _)| id == "op-7"), "every line carries the operation id");
        assert_eq!(lines[0].2, Some(42));
        assert_eq!(lines[1].2, Some(100));
        assert!(lines.iter().all(|(_, l, _)| !l.contains("s3cr3t")), "no secret in what the user sees: {lines:?}");
        assert!(lines[2].1.contains("https://***@example.com/x.git"), "the URL is shown with the secret hidden: {lines:?}");
        assert!(!res.output.contains("s3cr3t"), "nor in the final output (stdout and stderr): {}", res.output);
        assert!(res.output.contains("From https://***@example.com/x.git"), "stdout is kept, with the secret hidden");
        assert!(!res.output.contains("42%"), "only the last redraw of a progress line is kept in the output");
    }

    #[test]
    fn every_git_command_carries_the_safety_settings() {
        let cmd = git_cmd();
        let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        for setting in ["protocol.ext.allow=never", "core.fsmonitor=false"] {
            let at = args.iter().position(|a| a == setting).unwrap_or_else(|| panic!("missing -c {setting}: {args:?}"));
            assert!(at > 0 && args[at - 1] == "-c", "{setting} must be passed with -c");
        }
        let envs: Vec<(String, Option<String>)> = cmd
            .get_envs()
            .map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned())))
            .collect();
        let value = |k: &str| envs.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        assert_eq!(value("GIT_TERMINAL_PROMPT"), Some(Some("0".into())), "never wait for a password on a terminal");
        assert_eq!(value("GIT_OPTIONAL_LOCKS"), Some(Some("0".into())), "read-only commands must not rewrite the index");
        for var in ["GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR", "GIT_INDEX_FILE"] {
            assert_eq!(value(var), Some(None), "{var} inherited from the environment must be removed");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_repository_cannot_make_git_run_a_program() {
        use std::os::unix::fs::PermissionsExt;
        let _serial = crate::serial_git();
        let dir = std::env::temp_dir().join(format!("treeline-core-guard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let repo_s = repo.to_string_lossy().to_string();
        run_git(&repo_s, ["init", "-q"]).unwrap();
        // The repository's own config asks git to run a program on every status (fsmonitor) and allows the
        // ext:: transport, which runs a command as a "remote". Both would create a marker file.
        let fsmon_marker = dir.join("fsmonitor-ran");
        let hook = dir.join("fsmonitor.sh");
        std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", fsmon_marker.display())).unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        run_git(&repo_s, ["config", "core.fsmonitor", &hook.to_string_lossy()]).unwrap();
        run_git(&repo_s, ["config", "protocol.ext.allow", "always"]).unwrap();

        run_git(&repo_s, ["status", "--porcelain"]).unwrap();
        assert!(!fsmon_marker.exists(), "core.fsmonitor from the repository ran a program");

        let ext_marker = dir.join("ext-ran");
        let url = format!("ext::sh -c touch% {}", ext_marker.display());
        let _ = run_git(&repo_s, ["ls-remote", &url]);
        assert!(!ext_marker.exists(), "an ext:: remote ran a program");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
