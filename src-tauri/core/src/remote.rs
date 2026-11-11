//! The app's side of a connection to `treeline-helper`, over any child process (in practice `ssh host …`).
//!
//! It waits for the helper's hello line (skipping anything a login shell printed before it), then sends requests with
//! ids and matches the answers, so several requests can be in flight at once. Messages without an id are notifications
//! (for example a folder the helper started trusting) and go to `on_note`. When the process ends, every request still
//! waiting gets an error instead of hanging.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

type Pending = Arc<Mutex<HashMap<u64, Sender<Result<Value, CallError>>>>>;
type OnClose = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

/// Why a call did not get an answer. The kind decides what the app may do next (retry a read after `Closed`, never
/// after `Cancelled`); the text is what the user reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// The process ended on its own: the link dropped, ssh gave up, the helper exited.
    Closed,
    /// Treeline closed the connection itself (`close`).
    Cancelled,
    /// No answer within the call's time limit; the connection may still be alive.
    Timeout,
    /// The helper answered with an error (git's message, a refused folder…).
    Remote(String),
}

/// Lets another thread end a connection while it is still starting (a time limit, the user closing it). Once the
/// connection exists it ends that connection too.
#[derive(Clone, Default)]
pub struct Abort(Arc<Mutex<AbortState>>);

#[derive(Default)]
struct AbortState {
    child: Option<Weak<Mutex<Child>>>,
    why: Option<String>,
}

impl Abort {
    pub fn new() -> Abort {
        Abort::default()
    }

    /// Ends the process now (or as soon as it is started); the first `why` is kept for the error message.
    pub fn abort(&self, why: &str) {
        let child = {
            let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
            s.why.get_or_insert_with(|| why.to_string());
            s.child.clone()
        };
        // std does not signal a process it already waited for, so a reused pid is never hit.
        if let Some(child) = child.and_then(|w| w.upgrade()) {
            let _ = child.lock().unwrap_or_else(|e| e.into_inner()).kill();
        }
    }

    pub fn reason(&self) -> Option<String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).why.clone()
    }

    fn watch(&self, child: &Arc<Mutex<Child>>) {
        let aborted = {
            let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
            s.child = Some(Arc::downgrade(child));
            s.why.is_some()
        };
        if aborted {
            let _ = child.lock().unwrap_or_else(|e| e.into_inner()).kill();
        }
    }
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Closed => f.write_str(CLOSED),
            CallError::Cancelled => f.write_str("Treeline closed the connection to the other computer."),
            CallError::Timeout => f.write_str("The other computer did not answer in time."),
            CallError::Remote(e) => f.write_str(e),
        }
    }
}

impl From<CallError> for String {
    fn from(e: CallError) -> String {
        e.to_string()
    }
}

/// What to send before the helper starts.
pub enum Upload<'a> {
    /// The helper is already on the other side (tests, or a helper started some other way).
    Nothing,
    /// The launch script first says which computer it runs on (`treeline-arch <system> <machine>`); this picks the
    /// helper built for it (or refuses), and it is sent with its length.
    ForComputer(&'a dyn Fn(&str) -> Result<Vec<u8>, String>),
}

pub struct Connection {
    stdin: Mutex<ChildStdin>,
    child: Arc<Mutex<Child>>,
    pending: Pending,
    next: AtomicU64,
    alive: Arc<AtomicBool>,
    /// Set by `close` before the process is killed, so the requests it cuts off get `Cancelled`, not `Closed`.
    closing: Arc<AtomicBool>,
    /// Run once when the process ends (guarded by its own lock together with `alive`).
    on_close: OnClose,
    stderr: Arc<Mutex<String>>,
    /// The helper's hello: protocol, version, git version.
    pub hello: Value,
    /// Lines printed before the hello (a login shell's banner), kept for diagnostics.
    pub noise: Vec<String>,
}

const CLOSED: &str = "The connection to the other computer was closed.";

impl Connection {
    /// Starts `cmd`, sends the helper if asked for (see `Upload`), and waits up to `hello_timeout` for the hello line.
    pub fn start(
        cmd: Command,
        upload: Upload<'_>,
        hello_timeout: Duration,
        on_note: impl Fn(&Value) + Send + 'static,
    ) -> Result<Connection, String> {
        Connection::start_with(cmd, upload, hello_timeout, &Abort::new(), on_note)
    }

    /// `start`, which `abort` can end from another thread. Sending the helper is limited to `hello_timeout` too.
    pub fn start_with(
        mut cmd: Command,
        upload: Upload<'_>,
        hello_timeout: Duration,
        abort: &Abort,
        on_note: impl Fn(&Value) + Send + 'static,
    ) -> Result<Connection, String> {
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Could not start the connection: {e}"))?;
        let mut stdin = child.stdin.take().ok_or("no input")?;
        let stdout = child.stdout.take().ok_or("no output")?;
        let mut err_pipe = child.stderr.take().ok_or("no error output")?;
        let child = Arc::new(Mutex::new(child));
        abort.watch(&child);
        let stderr = Arc::new(Mutex::new(String::new()));
        {
            let stderr = Arc::clone(&stderr);
            std::thread::spawn(move || {
                let mut s = String::new();
                let _ = err_pipe.read_to_string(&mut s);
                *stderr.lock().unwrap_or_else(|e| e.into_inner()) = s;
            });
        }
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let closing = Arc::new(AtomicBool::new(false));
        let on_close: OnClose = Arc::new(Mutex::new(None));
        let (hello_tx, hello_rx) = channel::<(Value, Vec<String>)>();
        let (arch_tx, arch_rx) = channel::<String>();
        {
            let pending = Arc::clone(&pending);
            let alive = Arc::clone(&alive);
            let closing = Arc::clone(&closing);
            let on_close = Arc::clone(&on_close);
            std::thread::spawn(move || {
                let mut noise = Vec::new();
                let mut hello = Some(hello_tx);
                let mut arch = Some(arch_tx);
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    let msg = serde_json::from_str::<Value>(&line).ok();
                    if let Some(rest) = line.strip_prefix("treeline-arch ").filter(|_| hello.is_some()) {
                        if let Some(tx) = arch.take() {
                            let _ = tx.send(rest.trim().to_string());
                            continue;
                        }
                    }
                    if let Some(tx) = hello.take() {
                        match msg {
                            Some(v) if v.get("treeline-helper").is_some() => {
                                let _ = tx.send((v, std::mem::take(&mut noise)));
                            }
                            _ => {
                                noise.push(line);
                                hello = Some(tx);
                            }
                        }
                        continue;
                    }
                    let Some(v) = msg else { continue };
                    match v.get("id").and_then(Value::as_u64) {
                        Some(id) => {
                            if let Some(tx) = pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id) {
                                let answer = match v.get("error") {
                                    Some(e) => Err(CallError::Remote(e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string()))),
                                    None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
                                };
                                let _ = tx.send(answer);
                            }
                        }
                        None => on_note(&v),
                    }
                }
                // The process ended: nothing more will arrive.
                let why = if closing.load(Ordering::SeqCst) { CallError::Cancelled } else { CallError::Closed };
                let callback = {
                    let mut slot = on_close.lock().unwrap_or_else(|e| e.into_inner());
                    alive.store(false, Ordering::SeqCst);
                    slot.take()
                };
                for (_, tx) in pending.lock().unwrap_or_else(|e| e.into_inner()).drain() {
                    let _ = tx.send(Err(why.clone()));
                }
                if let Some(f) = callback {
                    f();
                }
            });
        }

        // Ends the process and says why: the caller's own reason, then an abort's, then what ssh printed.
        let fail = |own: Option<String>, default: &str| -> String {
            {
                let mut c = child.lock().unwrap_or_else(|e| e.into_inner());
                let _ = c.kill();
                let _ = c.wait();
            }
            std::thread::sleep(Duration::from_millis(50)); // let the error output be collected
            let err = stderr.lock().unwrap_or_else(|e| e.into_inner()).trim().to_string();
            own.or_else(|| abort.reason()).unwrap_or(if err.is_empty() { default.into() } else { err })
        };
        if let Upload::ForComputer(pick) = upload {
            let Ok(computer) = arch_rx.recv_timeout(hello_timeout) else {
                return Err(fail(None, "The other computer did not answer."));
            };
            let bytes = match pick(&computer) {
                Ok(b) => b,
                Err(e) => {
                    let _ = writeln!(stdin, "none");
                    return Err(fail(Some(e), ""));
                }
            };
            // A side that stops reading would block the write for ever: give it `hello_timeout`.
            let (sent_tx, sent_rx) = channel::<()>();
            {
                let abort = abort.clone();
                std::thread::spawn(move || {
                    if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = sent_rx.recv_timeout(hello_timeout) {
                        abort.abort("The other computer stopped taking the helper.");
                    }
                });
            }
            let sent = writeln!(stdin, "{}", bytes.len()).and_then(|_| stdin.write_all(&bytes)).and_then(|_| stdin.flush());
            drop(sent_tx);
            if sent.is_err() {
                // The other side stopped reading (no usable folder…): its own message says why.
                let _ = child.lock().unwrap_or_else(|e| e.into_inner()).wait();
                std::thread::sleep(Duration::from_millis(50));
                let err = stderr.lock().unwrap_or_else(|e| e.into_inner()).trim().to_string();
                let err = if err.is_empty() { None } else { Some(err) };
                return Err(abort.reason().or(err).unwrap_or_else(|| "Could not send the helper.".into()));
            }
        }
        match hello_rx.recv_timeout(hello_timeout) {
            Ok((hello, noise)) => Ok(Connection {
                stdin: Mutex::new(stdin),
                child,
                pending,
                next: AtomicU64::new(0),
                alive,
                closing,
                on_close,
                stderr,
                hello,
                noise,
            }),
            Err(_) => Err(fail(None, "The other computer did not answer.")),
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Runs `f` once when the process ends (at once if it already has), whoever ended it.
    pub fn on_close(&self, f: impl FnOnce() + Send + 'static) {
        let mut slot = self.on_close.lock().unwrap_or_else(|e| e.into_inner());
        if self.is_alive() {
            *slot = Some(Box::new(f));
        } else {
            drop(slot);
            f();
        }
    }

    fn gone(&self) -> CallError {
        if self.closing.load(Ordering::SeqCst) { CallError::Cancelled } else { CallError::Closed }
    }

    /// What the process wrote on its error output (ssh's messages), for an error shown to the user.
    pub fn error_output(&self) -> String {
        self.stderr.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Sends one request and waits for its answer (other requests may be answered meanwhile).
    pub fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, CallError> {
        if !self.is_alive() {
            return Err(self.gone());
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = channel();
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(id, tx);
        let line = json!({ "id": id, "method": method, "params": params }).to_string();
        let sent = {
            let mut stdin = self.stdin.lock().unwrap_or_else(|e| e.into_inner());
            writeln!(stdin, "{line}").and_then(|_| stdin.flush())
        };
        if sent.is_err() {
            self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
            return Err(self.gone());
        }
        match rx.recv_timeout(timeout) {
            Ok(answer) => answer,
            Err(_) => {
                self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                Err(CallError::Timeout)
            }
        }
    }

    /// Ends the connection: the helper sees its input close and exits.
    pub fn close(&self) {
        self.closing.store(true, Ordering::SeqCst);
        self.abandon();
    }

    /// Ends the process as if the link had dropped: the requests in flight get `Closed`, so a read may be sent again on
    /// a new connection (used when the connection stopped answering).
    pub fn abandon(&self) {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.close();
    }
}

/// A computer name as ssh and the user's `~/.ssh/config` know it: letters, digits, `.`, `-`, `_`, optionally `user@` in
/// front. Never starts with `-` (ssh would read it as an option, e.g. `-oProxyCommand=…`) and has nothing a shell would
/// interpret.
pub fn valid_host(host: &str) -> bool {
    let (user, name) = match host.split_once('@') {
        Some(("", _)) => return false, // "@host": an empty user name
        Some(parts) => parts,
        None => ("", host),
    };
    let ok = |s: &str| s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    !name.is_empty() && !name.starts_with('-') && !user.starts_with('-') && ok(name) && ok(user) && host.len() <= 255
}

/// The protocol between the app and the helper; both sides must speak the same one.
pub const PROTOCOL: u64 = 1;

/// What a remote tab may ask for in this version: reading only.
pub const READ_METHODS: &[&str] = &[
    "repo_status", "list_submodules", "list_refs", "stash_list", "list_remotes", "head_message", "git_log",
    "commit_files", "commit_detail", "commit_diff", "file_diff", "file_log", "git_blame", "git_log_matches",
];

/// Everything the app sends a helper: the reads, plus opening folders, browsing, the start and Reconnect's ping.
pub fn required_methods() -> Vec<&'static str> {
    let mut all = READ_METHODS.to_vec();
    all.extend(["access", "list_dir", "init", "allow", "ping"]);
    all
}

/// Whether a helper's hello (`{"treeline-helper": {…}}`) can serve this app: the same protocol and every command in
/// `required`. Its version is not compared (it can lag behind the app's without missing anything). The error is the
/// end of a sentence about the helper ("The helper on … ").
pub fn check_hello(hello: &Value, required: &[&str]) -> Result<(), String> {
    let h = &hello["treeline-helper"];
    let proto = h["proto"].as_u64().unwrap_or(0);
    if proto != PROTOCOL {
        return Err(format!("speaks protocol {proto}; this Treeline speaks {PROTOCOL}."));
    }
    let Some(listed) = h["methods"].as_array() else {
        return Err("is older than this Treeline: it does not list its commands.".into());
    };
    let missing: Vec<&str> = required.iter().copied().filter(|m| !listed.iter().any(|l| l == m)).collect();
    if missing.is_empty() { Ok(()) } else { Err(format!("is older than this Treeline: it lacks {}.", missing.join(", "))) }
}

/// Where the sent helper may be put, in order: /tmp, then a private folder in the user's home (for computers whose
/// /tmp does not allow running programs).
pub const HELPER_DIRS: [&str; 2] = ["/tmp", "$HOME/.cache/treeline"];

/// The shell script run on the other computer (always by `/bin/sh`, whatever the user's login shell). It says which
/// computer it is on, reads the length of the helper built for it (digits only), reads exactly that many bytes once
/// into a private file in the first folder where one can be created, and moves it on if it cannot run there
/// (`--probe`). The helper deletes its file as soon as it starts. No value from the app is put into the script.
/// A folder is used only if it has the sticky bit (like /tmp: no one can replace another user's file) or belongs to the
/// user and no one else can write to it. Files of its own left there more than a day ago (an upload cut off) are
/// removed first. Only POSIX `find` options, so it runs with busybox and dash too.
pub fn launch_script(dirs: &[&str]) -> String {
    let dirs = dirs.iter().map(|d| format!("\"{d}\"")).collect::<Vec<_>>().join(" ");
    format!(
        r#"echo "treeline-arch $(uname -s) $(uname -m)"; read n; case "$n" in ""|*[!0-9]*) exit 96;; esac; u=$(id -u); f=; for d in {dirs}; do [ -d "$d" ] || {{ mkdir -p "$d" 2>/dev/null && chmod 700 "$d"; }}; [ -n "$(find "$d" -prune \( -perm -1000 -o \( -user "$u" ! -perm -0020 ! -perm -0002 \) \) 2>/dev/null)" ] || continue; find "$d"/. ! -name . -prune -type f -name "treeline-helper.??????" -user "$u" -mtime +0 -exec rm -f {{}} \; >/dev/null 2>&1; g=$(mktemp "$d/treeline-helper.XXXXXX" 2>/dev/null) || continue; chmod 700 "$g"; if [ -z "$f" ]; then head -c "$n" > "$g" || exit 97; else cat "$f" > "$g"; rm -f "$f"; fi; f=$g; if "$f" --probe >/dev/null 2>&1; then exec "$f" --stdio --self-delete; fi; done; [ -n "$f" ] && rm -f "$f"; echo "treeline: no folder on this computer allows running the helper (tried /tmp and ~/.cache/treeline)" >&2; exit 99"#
    )
}

/// Which helper build runs on a computer, from the launch script's `<system> <machine>` (as `uname -s`, `uname -m`).
pub fn helper_build_for(computer: &str) -> Option<&'static str> {
    match computer.split_whitespace().collect::<Vec<_>>().as_slice() {
        ["Linux", "x86_64" | "amd64"] => Some("linux-x86_64"),
        ["Linux", "aarch64" | "arm64"] => Some("linux-aarch64"),
        _ => None,
    }
}

/// The command that starts a fresh helper on `host` with the system ssh (see `launch_script`); pair it with
/// `Upload::ForComputer`. Only keys from the ssh agent are used for now (no password prompt).
pub fn ssh_helper_command(host: &str) -> Result<Command, String> {
    if !valid_host(host) {
        return Err(format!("“{host}” is not a computer name ssh can use."));
    }
    let ssh = if cfg!(target_os = "macos") { "/usr/bin/ssh" } else { "ssh" };
    let script = launch_script(&HELPER_DIRS);
    debug_assert!(!script.contains('\''), "the script is wrapped in single quotes");
    let mut cmd = Command::new(ssh);
    cmd.args(["-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=3"])
        .arg("--")
        .arg(host)
        .arg(format!("exec /bin/sh -c '{script}'"));
    Ok(cmd)
}

/// The computer names an ssh config offers: the names after `Host`, in order, without patterns (`*`, `?`, `!`) and
/// without anything ssh could not take as a computer name. (`Include`d files are not followed.)
pub fn parse_ssh_hosts(config: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in config.lines() {
        let line = line.trim(); // a comment line never starts with `Host`, so it is skipped below
        let (key, rest) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((k, r)) => (k, r.trim_start_matches(|c: char| c.is_whitespace() || c == '=')),
            None => continue,
        };
        if !key.eq_ignore_ascii_case("host") {
            continue;
        }
        for name in rest.split_whitespace().take_while(|n| !n.starts_with('#')) {
            let name = name.trim_matches('"');
            // valid_host also leaves out patterns (`*`, `?`, `!`) and option-like names.
            if valid_host(name) && !out.iter().any(|n| n == name) {
                out.push(name.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod hello_tests {
    use super::*;

    fn hello(proto: u64, methods: Option<&[&str]>) -> Value {
        match methods {
            Some(m) => json!({ "treeline-helper": { "proto": proto, "version": "0.0.1", "methods": m } }),
            None => json!({ "treeline-helper": { "proto": proto, "version": "0.3.2" } }),
        }
    }

    #[test]
    fn a_helper_with_every_command_is_accepted_whatever_its_version() {
        let all = required_methods();
        assert_eq!(check_hello(&hello(PROTOCOL, Some(&all)), &all), Ok(()));
    }

    #[test]
    fn says_what_is_wrong_with_a_helper() {
        let all = required_methods();
        assert_eq!(check_hello(&hello(PROTOCOL, None), &all), Err("is older than this Treeline: it does not list its commands.".into()));
        let without: Vec<&str> = all.iter().copied().filter(|m| *m != "file_log" && *m != "list_dir").collect();
        assert_eq!(check_hello(&hello(PROTOCOL, Some(&without)), &all), Err("is older than this Treeline: it lacks file_log, list_dir.".into()));
        assert_eq!(check_hello(&hello(2, Some(&all)), &all), Err("speaks protocol 2; this Treeline speaks 1.".into()));
        assert!(check_hello(&json!({}), &all).is_err(), "no hello at all");
    }

    #[test]
    fn the_app_asks_for_reads_only_plus_what_it_needs_to_connect() {
        let all = required_methods();
        for m in READ_METHODS {
            assert!(all.contains(m));
        }
        for m in ["commit", "push", "checkout", "stage_files"] {
            assert!(!all.contains(&m), "{m}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_the_computers_named_in_the_ssh_config() {
        let config = "# my machines\nHost linux-box.local\n  User adrian\nHost build-server build\nhost=lowercase-key\n\
                      Host *.internal !secret gateway # the jump host\nHost *\n  ServerAliveInterval 30\nMatch host x\n\
                      Host linux-box.local\nHost \"quoted\"\nHost -oProxyCommand=evil\n";
        assert_eq!(
            parse_ssh_hosts(config),
            ["linux-box.local", "build-server", "build", "lowercase-key", "gateway", "quoted"],
            "patterns, comments, duplicates, Match blocks and option-like names are left out"
        );
        assert!(parse_ssh_hosts("").is_empty());
    }

    #[test]
    fn accepts_computer_names_and_refuses_anything_else() {
        for ok in ["linux-box.local", "adrian@linux-box.local", "10.0.0.5", "build_server-2", "a"] {
            assert!(valid_host(ok), "{ok}");
        }
        for bad in ["", "-oProxyCommand=touch x", "user@-oProxyCommand=x", "-l", "a b", "a;b", "$(id)", "a/b", "`id`", "@host", "a\nb"] {
            assert!(!valid_host(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_ssh_command_ends_options_before_the_host_and_uses_only_agent_keys() {
        assert!(ssh_helper_command("-oProxyCommand=touch /tmp/x").is_err());
        let cmd = ssh_helper_command("adrian@linux-box.local").unwrap();
        let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let dd = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[dd + 1], "adrian@linux-box.local", "the host comes right after `--`");
        assert_eq!(args.len(), dd + 3, "then only the remote command");
        assert!(args[..dd].windows(2).any(|w| w[0] == "-o" && w[1] == "BatchMode=yes"));
        assert!(args[dd + 2].starts_with("exec /bin/sh -c '") && args[dd + 2].ends_with('\''), "{}", args[dd + 2]);
        assert!(args[dd + 2].contains("exec \"$f\" --stdio --self-delete"));
    }

    #[test]
    fn picks_the_helper_built_for_the_computer() {
        assert_eq!(helper_build_for("Linux x86_64"), Some("linux-x86_64"));
        assert_eq!(helper_build_for("Linux aarch64"), Some("linux-aarch64"));
        assert_eq!(helper_build_for("Linux armv7l"), None);
        assert_eq!(helper_build_for("Darwin arm64"), None);
        assert_eq!(helper_build_for(""), None);
    }
}
