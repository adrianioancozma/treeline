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
