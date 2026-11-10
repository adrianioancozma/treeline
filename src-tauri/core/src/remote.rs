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
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Pending = Arc<Mutex<HashMap<u64, Sender<Result<Value, String>>>>>;

pub struct Connection {
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    pending: Pending,
    next: AtomicU64,
    alive: Arc<AtomicBool>,
    stderr: Arc<Mutex<String>>,
    /// The helper's hello: protocol, version, git version.
    pub hello: Value,
    /// Lines printed before the hello (a login shell's banner), kept for diagnostics.
    pub noise: Vec<String>,
}

const CLOSED: &str = "The connection to the other computer was closed.";

impl Connection {
    /// Starts `cmd`, writes `first` to its input before anything else (the helper binary, when it is sent along), and
    /// waits up to `hello_timeout` for the hello line.
    pub fn start(
        mut cmd: Command,
        first: Option<&[u8]>,
        hello_timeout: Duration,
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
        let stderr = Arc::new(Mutex::new(String::new()));
        {
            let stderr = Arc::clone(&stderr);
            std::thread::spawn(move || {
                let mut s = String::new();
                let _ = err_pipe.read_to_string(&mut s);
                *stderr.lock().unwrap_or_else(|e| e.into_inner()) = s;
            });
        }
        if let Some(bytes) = first {
            stdin.write_all(bytes).and_then(|_| stdin.flush()).map_err(|e| format!("Could not send the helper: {e}"))?;
        }

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let (hello_tx, hello_rx) = channel::<(Value, Vec<String>)>();
        {
            let pending = Arc::clone(&pending);
            let alive = Arc::clone(&alive);
            std::thread::spawn(move || {
                let mut noise = Vec::new();
                let mut hello = Some(hello_tx);
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    let msg = serde_json::from_str::<Value>(&line).ok();
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
                                    Some(e) => Err(e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string())),
                                    None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
                                };
                                let _ = tx.send(answer);
                            }
                        }
                        None => on_note(&v),
                    }
                }
                // The process ended: nothing more will arrive.
                alive.store(false, Ordering::SeqCst);
                for (_, tx) in pending.lock().unwrap_or_else(|e| e.into_inner()).drain() {
                    let _ = tx.send(Err(CLOSED.into()));
                }
            });
        }

        match hello_rx.recv_timeout(hello_timeout) {
            Ok((hello, noise)) => Ok(Connection {
                stdin: Mutex::new(stdin),
                child: Mutex::new(child),
                pending,
                next: AtomicU64::new(0),
                alive,
                stderr,
                hello,
                noise,
            }),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                std::thread::sleep(Duration::from_millis(50)); // let the error output be collected
                let err = stderr.lock().unwrap_or_else(|e| e.into_inner()).trim().to_string();
                Err(if err.is_empty() { "The other computer did not answer.".into() } else { err })
            }
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// What the process wrote on its error output (ssh's messages), for an error shown to the user.
    pub fn error_output(&self) -> String {
        self.stderr.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Sends one request and waits for its answer (other requests may be answered meanwhile).
    pub fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        if !self.is_alive() {
            return Err(CLOSED.into());
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
            return Err(CLOSED.into());
        }
        match rx.recv_timeout(timeout) {
            Ok(answer) => answer,
            Err(_) => {
                self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                Err("The other computer did not answer in time.".into())
            }
        }
    }

    /// Ends the connection: the helper sees its input close and exits.
    pub fn close(&self) {
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

/// The command that starts a fresh helper on `host` with the system ssh. The helper binary (`helper_len` bytes) must be
/// written to its input first: the other side copies exactly that many bytes into a private file in /tmp, runs it, and
/// the helper deletes the file at once. Only keys from the ssh agent are used for now (no password prompt).
pub fn ssh_helper_command(host: &str, helper_len: usize) -> Result<Command, String> {
    if !valid_host(host) {
        return Err(format!("“{host}” is not a computer name ssh can use."));
    }
    let ssh = if cfg!(target_os = "macos") { "/usr/bin/ssh" } else { "ssh" };
    // The only value put into the remote command is a number this code computed.
    let script = format!(
        r#"f=$(mktemp /tmp/treeline-helper.XXXXXX) && head -c {helper_len} > "$f" && chmod 700 "$f" && exec "$f" --stdio --self-delete"#
    );
    let mut cmd = Command::new(ssh);
    cmd.args(["-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=3"])
        .arg("--")
        .arg(host)
        .arg(script);
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(ssh_helper_command("-oProxyCommand=touch /tmp/x", 10).is_err());
        let cmd = ssh_helper_command("adrian@linux-box.local", 994296).unwrap();
        let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let dd = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[dd + 1], "adrian@linux-box.local", "the host comes right after `--`");
        assert_eq!(args.len(), dd + 3, "then only the remote command");
        assert!(args[..dd].windows(2).any(|w| w[0] == "-o" && w[1] == "BatchMode=yes"));
        assert!(args[dd + 2].contains("head -c 994296 "), "{}", args[dd + 2]);
        assert!(args[dd + 2].ends_with("--stdio --self-delete"));
    }
}
