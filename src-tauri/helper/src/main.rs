//! treeline-helper: runs next to repositories on another computer and answers Treeline over stdin/stdout.
//!
//! The app starts it through `ssh host <path> --stdio` and talks JSON-RPC, one message per line. It lives exactly as
//! long as that connection: when stdin closes it exits; nothing listens on a port. With `--self-delete` it removes
//! its own file at start-up (the app sends a fresh copy on every connection, so nothing stays on the computer).
//!
//! Messages:
//! - first line out: `{"treeline-helper": {"proto": 1, "version": …, "git": …, "self_deleted": …}}`; the app skips
//!   anything a login shell printed before it.
//! - `{"id": 1, "method": "init", "params": {"trusted": ["/home/me/repo", …]}}` — the folders the user opened in the
//!   app for this computer. Nothing else is answered before it. Folders added later on this side (a submodule, the
//!   top level of a sub-folder) are reported as `{"method": "trusted", "params": {"key": …}}`.
//! - read requests (`repo_status`, `git_log`, …) with the same parameter names as the functions in
//!   `treeline_core::read`; each runs on its own thread, so a slow one does not hold up the others.
//! - answers: `{"id": …, "result": …}` or `{"id": …, "error": "…"}`.

use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use treeline_core::read;

pub const PROTOCOL: u32 = 1;

type Out = Arc<Mutex<std::io::Stdout>>;

fn send(out: &Out, msg: &Value) {
    let text = msg.to_string();
    let mut o = out.lock().unwrap_or_else(|e| e.into_inner());
    // The app is gone if stdout is closed: stop quietly instead of panicking.
    if writeln!(o, "{text}").and_then(|_| o.flush()).is_err() {
        std::process::exit(0);
    }
}

/// Deserialises `params` into the named fields and calls `f` with them, in order.
macro_rules! call {
    ($params:expr, $f:path, $($arg:ident: $t:ty),* $(,)?) => {{
        #[derive(Deserialize)]
        struct Args { $($arg: $t),* }
        let a: Args = serde_json::from_value($params).map_err(|e| format!("bad parameters: {e}"))?;
        serde_json::to_value($f($(a.$arg),*)?).map_err(|e| e.to_string())
    }};
}

fn dispatch(method: &str, p: Value) -> Result<Value, String> {
    match method {
        "ping" => Ok(json!({})),
        "repo_status" => call!(p, read::repo_status, path: String),
        "list_submodules" => call!(p, read::list_submodules, path: String),
        "list_refs" => call!(p, read::list_refs, path: String),
        "stash_list" => call!(p, read::stash_list, path: String),
        "list_remotes" => call!(p, read::list_remotes, path: String),
        "head_message" => call!(p, read::head_message, path: String),
        "git_log" => call!(p, read::git_log, path: String, skip: u32, limit: u32, all: bool, query: Option<String>, file: Option<String>),
        "commit_files" => call!(p, read::commit_files, path: String, sha: String, parent: Option<String>),
        "commit_diff" => call!(p, read::commit_diff, path: String, sha: String, parent: Option<String>, file: String, old_file: Option<String>, context: u32),
        "file_diff" => call!(p, read::file_diff, path: String, file: String, staged: bool, untracked: bool, context: u32),
        _ => Err(format!("unknown method: {method}")),
    }
}

fn main() {
    let self_deleted = std::env::args()
        .any(|a| a == "--self-delete")
        .then(|| std::env::current_exe().and_then(std::fs::remove_file).is_ok());
    let out: Out = Arc::new(Mutex::new(std::io::stdout()));
    let git = treeline_core::git::git_info_now();
    send(&out, &json!({ "treeline-helper": {
        "proto": PROTOCOL, "version": env!("CARGO_PKG_VERSION"), "git": git.version, "self_deleted": self_deleted,
    }}));

    let ready = Arc::new(AtomicBool::new(false));
    let mut workers = Vec::new();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                send(&out, &json!({ "id": null, "error": format!("not JSON: {e}") }));
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or_default().to_string();
        let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
        if method == "init" {
            let trusted: Vec<String> = params.get("trusted").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
            for path in &trusted {
                treeline_core::trust::allow_repo(path);
            }
            // Only folders added after this point are news to the app: it sent these.
            let report = Arc::clone(&out);
            treeline_core::trust::set_grant_observer(move |key| {
                send(&report, &json!({ "method": "trusted", "params": { "key": key } }));
            });
            ready.store(true, Ordering::SeqCst);
            send(&out, &json!({ "id": id, "result": { "trusted": trusted.len() } }));
            continue;
        }
        if !ready.load(Ordering::SeqCst) {
            send(&out, &json!({ "id": id, "error": "init first" }));
            continue;
        }
        let out = Arc::clone(&out);
        workers.push(std::thread::spawn(move || {
            let msg = match dispatch(&method, params) {
                Ok(result) => json!({ "id": id, "result": result }),
                Err(error) => json!({ "id": id, "error": error }),
            };
            send(&out, &msg);
        }));
    }
    // stdin closed: the connection is gone. Let running reads finish (their answers go nowhere), then exit.
    for w in workers {
        let _ = w.join();
    }
}
