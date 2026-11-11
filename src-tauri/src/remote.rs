//! Repositories on another computer, reached over ssh through `treeline-helper` (read-only for now).
//!
//! The app keeps, in Rust, which folders the user opened on which computer (`remote-trusted.json`); the page cannot
//! add to it: opening a folder asks for confirmation in a native dialog. Every call is checked against that list and
//! against the read-only methods before it goes to the helper, which checks again on its side.
//!
//! For now (first version): only keys from the ssh agent (no password prompt); the static Linux helpers are files in
//! the app's settings folder (see `helper_for`), or the path in `TREELINE_HELPER`. Connections: `treeline_core::hosts`
//! (one per computer, replaced once when the link drops, a failed start remembered for a while, `remote-host-state` events).

use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use treeline_core::hosts::{Ask, Config, Hosts};
pub use treeline_core::remote::READ_METHODS;
use treeline_core::remote::{Abort, Connection, Upload, helper_build_for, parse_ssh_hosts, ssh_helper_command, valid_host};

use crate::git::blocking;

const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// An absolute folder path on the other computer: starts with `/`, no `..`, no control characters. Trailing `/` dropped.
pub fn clean_remote_path(path: &str) -> Result<String, String> {
    if !path.starts_with('/') || path.chars().any(char::is_control) || path.split('/').any(|p| p == "..") {
        return Err(format!("“{path}” is not a full folder path on the other computer."));
    }
    let trimmed = path.trim_end_matches('/');
    Ok(if trimmed.is_empty() { "/".into() } else { trimmed.into() })
}

/// True when `path` is one of the opened folders or inside one (compared as text: the folders are on another computer).
pub fn is_opened(opened: &[String], path: &str) -> bool {
    let Ok(path) = clean_remote_path(path) else { return false };
    opened.iter().any(|root| root != "/" && (path == *root || path.starts_with(&format!("{root}/"))))
}

struct State {
    /// host -> folders opened there (as the user gave them, plus the helper's resolved form).
    opened: HashMap<String, Vec<String>>,
    file: Option<PathBuf>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State { opened: HashMap::new(), file: None }))
}

fn lock() -> std::sync::MutexGuard<'static, State> {
    state().lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn remember(host: &str, folder: String) {
    let mut s = lock();
    let list = s.opened.entry(host.to_string()).or_default();
    if !list.contains(&folder) {
        list.push(folder);
        if let Some(file) = s.file.clone() {
            if let Ok(text) = serde_json::to_string_pretty(&s.opened) {
                let tmp = file.with_extension("json.tmp");
                if std::fs::write(&tmp, text).is_ok() {
                    let _ = std::fs::rename(&tmp, &file);
                }
            }
        }
    }
}

/// True when `path` was opened on `host` (or is inside such a folder): what the saved tabs may keep.
pub(crate) fn is_remote_opened(host: &str, path: &str) -> bool {
    valid_host(host) && lock().opened.get(host).is_some_and(|l| is_opened(l, path))
}

/// Reads the list of opened remote folders at start-up (a file only Rust writes).
pub fn load(app: &AppHandle) {
    let Ok(dir) = app.path().app_config_dir() else { return };
    let file = dir.join("remote-trusted.json");
    let opened: HashMap<String, Vec<String>> =
        std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let mut s = lock();
    s.opened = opened.into_iter().filter(|(h, _)| valid_host(h)).collect();
    s.file = Some(file);
    drop(s);
    let (for_connect, for_events) = (app.clone(), app.clone());
    let _ = HOSTS.set(Hosts::with(
        move |host, abort| connect(&for_connect, host, abort),
        Config::default(),
        move |host, generation, state| {
            let _ = for_events.emit("remote-host-state", json!({ "host": host, "gen": generation, "state": state }));
        },
    ));
}

/// The connections to other computers (set up by `load`).
static HOSTS: OnceLock<Hosts> = OnceLock::new();

fn hosts() -> Result<&'static Hosts, String> {
    HOSTS.get().ok_or_else(|| "Treeline is still starting.".to_string())
}

/// The helper built for `computer` (`<system> <machine>`, as the launch script reports it): a static build per
/// architecture, so it runs on any Linux. For now the files sit in the app's settings folder
/// (`treeline-helper-linux-x86_64`, `treeline-helper-linux-aarch64`), or `TREELINE_HELPER` names one.
fn helper_for(app: &AppHandle, computer: &str) -> Result<Vec<u8>, String> {
    let build = helper_build_for(computer).ok_or_else(|| format!("Treeline has no helper for {computer} yet."))?;
    let path = match std::env::var_os("TREELINE_HELPER") {
        Some(p) => PathBuf::from(p),
        None => app.path().app_config_dir().map_err(|e| e.to_string())?.join(format!("treeline-helper-{build}")),
    };
    std::fs::read(&path).map_err(|_| format!("The {build} helper is not in this build yet. Put it at {}.", path.display()))
}

/// A new, ready connection to `host`: ssh, the helper built for that computer, and the opened folders handed to it.
fn connect(app: &AppHandle, host: &str, abort: &Abort) -> Result<Connection, String> {
    let cmd = ssh_helper_command(host)?;
    let note_host = host.to_string();
    let pick = |computer: &str| helper_for(app, computer);
    let conn = Connection::start_with(cmd, Upload::ForComputer(&pick), Duration::from_secs(20), abort, move |note| {
        if note["method"] == "trusted" {
            if let Some(key) = note["params"]["key"].as_str() {
                remember(&note_host, key.to_string());
            }
        }
    })?;
    let opened = lock().opened.get(host).cloned().unwrap_or_default();
    conn.call("init", json!({ "trusted": opened }), Duration::from_secs(20))?;
    Ok(conn)
}

#[tauri::command]
pub async fn remote_connect(host: String) -> Result<Value, String> {
    blocking(move || {
        if !valid_host(&host) {
            return Err(format!("“{host}” is not a computer name ssh can use."));
        }
        Ok(hosts()?.get(&host, Ask::EXPLICIT)?.1.hello.clone())
    })
    .await
}

/// Opens a folder on `host` after the user confirms it in a native dialog (the page cannot widen the list itself).
/// The folder is checked on that computer first, so a wrong path, an unreadable folder or one git refuses is reported
/// before anything is confirmed. Returns the path and whether the user may write to the repository there.
#[tauri::command]
pub async fn remote_open(app: AppHandle, host: String, path: String) -> Result<Value, String> {
    blocking(move || {
        if !valid_host(&host) {
            return Err(format!("“{host}” is not a computer name ssh can use."));
        }
        let path = clean_remote_path(&path)?;
        let hosts = hosts()?;
        let access = hosts.call(&host, "access", json!({ "path": path }), CALL_TIMEOUT, Ask::EXPLICIT)?;
        if let Some(problem) = access["problem"].as_str() {
            return Err(problem.to_string());
        }
        let writable = access["tree_writable"] != json!(false) && access["git_writable"] != json!(false);
        let already = lock().opened.get(&host).is_some_and(|l| is_opened(l, &path));
        if !already {
            use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
            let note = if writable { "" } else { "\n\nYou cannot write to this repository there; it opens read-only." };
            let sure = app
                .dialog()
                .message(format!("Open\n{path}\n\non {host}?\n\nTreeline reads this repository over ssh. Nothing is installed there.{note}"))
                .title("Open remote repository")
                .kind(MessageDialogKind::Info)
                .buttons(MessageDialogButtons::OkCancelCustom("Open".into(), "Cancel".into()))
                .blocking_show();
            if !sure {
                return Err("Cancelled.".into());
            }
            remember(&host, path.clone());
        }
        hosts.call(&host, "allow", json!({ "path": path }), CALL_TIMEOUT, Ask::EXPLICIT)?;
        Ok(json!({ "path": path, "writable": writable, "toplevel": access["toplevel"] }))
    })
    .await
}

#[tauri::command]
pub async fn remote_call(host: String, method: String, params: Value) -> Result<Value, String> {
    blocking(move || {
        if !READ_METHODS.contains(&method.as_str()) {
            return Err(format!("{method} is not available for repositories on another computer yet."));
        }
        let path = params.get("path").and_then(Value::as_str).unwrap_or_default().to_string();
        let opened = lock().opened.get(&host).cloned().unwrap_or_default();
        if !is_opened(&opened, &path) {
            return Err(format!("{path} has not been opened on {host} in Treeline."));
        }
        hosts()?.call(&host, &method, params, CALL_TIMEOUT, Ask::READ)
    })
    .await
}

/// Folders on `host`, to pick one without typing its path (folder names only; see `treeline_core::browse`).
#[tauri::command]
pub async fn remote_list(host: String, path: String, show_hidden: bool) -> Result<Value, String> {
    blocking(move || {
        if !valid_host(&host) {
            return Err(format!("“{host}” is not a computer name ssh can use."));
        }
        hosts()?.call(&host, "list_dir", json!({ "path": path, "show_hidden": show_hidden }), CALL_TIMEOUT, Ask::EXPLICIT)
    })
    .await
}

/// The computers named in `~/.ssh/config`, offered when opening a remote repository.
#[tauri::command]
pub fn ssh_hosts() -> Vec<String> {
    std::env::var_os("HOME")
        .and_then(|h| std::fs::read_to_string(PathBuf::from(h).join(".ssh/config")).ok())
        .map(|c| parse_ssh_hosts(&c))
        .unwrap_or_default()
}

/// Folders already opened on `host` (shown as "Recent on <host>").
#[tauri::command]
pub fn remote_recent(host: String) -> Vec<String> {
    lock().opened.get(&host).cloned().unwrap_or_default()
}

#[tauri::command]
pub fn remote_disconnect(host: String) {
    if let Ok(h) = hosts() {
        h.disconnect(&host);
    }
}

/// Keeps a connection that answers, replaces one that does not, or starts one now (even right after a failure);
/// returns when that is done, so the page can reload afterwards.
#[tauri::command]
pub async fn remote_reconnect(host: String) -> Result<(), String> {
    blocking(move || {
        if !valid_host(&host) {
            return Err(format!("“{host}” is not a computer name ssh can use."));
        }
        hosts()?.reconnect(&host)
    })
    .await
}

/// Every computer's connection state, for a page that starts listening to `remote-host-state` after it changed.
#[tauri::command]
pub fn remote_host_states() -> Vec<Value> {
    let states = hosts().map(|h| h.states()).unwrap_or_default();
    states.into_iter().map(|(host, generation, state)| json!({ "host": host, "gen": generation, "state": state })).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_paths_must_be_full_and_cannot_climb() {
        assert_eq!(clean_remote_path("/home/a/repo/").unwrap(), "/home/a/repo");
        for bad in ["home/a", "~/repo", "/home/a/../b", "/home/a/..", "/x\n/y", ""] {
            assert!(clean_remote_path(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn only_opened_folders_and_their_contents_pass() {
        let opened = vec!["/home/a/repo".to_string()];
        assert!(is_opened(&opened, "/home/a/repo"));
        assert!(is_opened(&opened, "/home/a/repo/sub"));
        assert!(!is_opened(&opened, "/home/a/repo2"), "a sibling with the same prefix");
        assert!(!is_opened(&opened, "/home/a"));
        assert!(!is_opened(&opened, "/home/a/repo/../other"));
        assert!(!is_opened(&["/".to_string()], "/etc"), "the root is never an opened folder");
        assert!(!is_opened(&["/".to_string()], "/"), "not even the root itself");
    }

    #[test]
    fn remote_tabs_can_only_read() {
        for m in ["commit", "push", "checkout", "discard", "save_resolution", "remove_lock_file", "allow", "init"] {
            assert!(!READ_METHODS.contains(&m), "{m}");
        }
        assert!(READ_METHODS.contains(&"repo_status") && READ_METHODS.contains(&"git_log"));
        for m in ["file_log", "git_blame", "git_log_matches"] {
            assert!(READ_METHODS.contains(&m), "File history, Blame and the History filters read only: {m}");
        }
    }
}
