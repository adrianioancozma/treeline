//! Repositories on another computer, reached over ssh through `treeline-helper` (read-only for now).
//!
//! The app keeps, in Rust, which folders the user opened on which computer (`remote-trusted.json`); the page cannot
//! add to it: opening a folder asks for confirmation in a native dialog. Every call is checked against that list and
//! against the read-only methods before it goes to the helper, which checks again on its side.
//!
//! For now (first version): only keys from the ssh agent (no password prompt); the Linux helper is a file next to
//! the app's settings (`treeline-helper-linux-x86_64`), or the path in `TREELINE_HELPER`; no automatic reconnect.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use treeline_core::remote::{Connection, ssh_helper_command, valid_host};

use crate::git::blocking;

/// What a remote tab may ask for in this version: reading only.
pub const READ_METHODS: &[&str] = &[
    "repo_status", "list_submodules", "list_refs", "stash_list", "list_remotes", "head_message", "git_log",
    "commit_files", "commit_diff", "file_diff",
];

const HELPER_FILE: &str = "treeline-helper-linux-x86_64";
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
    conns: HashMap<String, Arc<Connection>>,
    /// host -> folders opened there (as the user gave them, plus the helper's resolved form).
    opened: HashMap<String, Vec<String>>,
    file: Option<PathBuf>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State { conns: HashMap::new(), opened: HashMap::new(), file: None }))
}

fn lock() -> std::sync::MutexGuard<'static, State> {
    state().lock().unwrap_or_else(|e| e.into_inner())
}

fn remember(host: &str, folder: String) {
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

/// Reads the list of opened remote folders at start-up (a file only Rust writes).
pub fn load(app: &AppHandle) {
    let Ok(dir) = app.path().app_config_dir() else { return };
    let file = dir.join("remote-trusted.json");
    let opened: HashMap<String, Vec<String>> =
        std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let mut s = lock();
    s.opened = opened.into_iter().filter(|(h, _)| valid_host(h)).collect();
    s.file = Some(file);
}

fn helper_binary(app: &AppHandle) -> Result<Vec<u8>, String> {
    let path = match std::env::var_os("TREELINE_HELPER") {
        Some(p) => PathBuf::from(p),
        None => app.path().app_config_dir().map_err(|e| e.to_string())?.join(HELPER_FILE),
    };
    std::fs::read(&path).map_err(|_| format!("The Linux helper is not in this build yet. Put it at {}.", path.display()))
}

/// The live connection to `host`, starting one (and handing it the opened folders) if there is none.
fn connection(app: &AppHandle, host: &str) -> Result<Arc<Connection>, String> {
    if let Some(c) = lock().conns.get(host).filter(|c| c.is_alive()).cloned() {
        return Ok(c);
    }
    let helper = helper_binary(app)?;
    let cmd = ssh_helper_command(host, helper.len())?;
    let note_host = host.to_string();
    let conn = Connection::start(cmd, Some(&helper), Duration::from_secs(20), move |note| {
        if note["method"] == "trusted" {
            if let Some(key) = note["params"]["key"].as_str() {
                remember(&note_host, key.to_string());
            }
        }
    })?;
    let opened = lock().opened.get(host).cloned().unwrap_or_default();
    conn.call("init", json!({ "trusted": opened }), Duration::from_secs(20))?;
    let conn = Arc::new(conn);
    lock().conns.insert(host.to_string(), Arc::clone(&conn));
    Ok(conn)
}

#[tauri::command]
pub async fn remote_connect(app: AppHandle, host: String) -> Result<Value, String> {
    blocking(move || {
        if !valid_host(&host) {
            return Err(format!("“{host}” is not a computer name ssh can use."));
        }
        Ok(connection(&app, &host)?.hello.clone())
    })
    .await
}

/// Opens a folder on `host` after the user confirms it in a native dialog (the page cannot widen the list itself).
#[tauri::command]
pub async fn remote_open(app: AppHandle, host: String, path: String) -> Result<String, String> {
    blocking(move || {
        if !valid_host(&host) {
            return Err(format!("“{host}” is not a computer name ssh can use."));
        }
        let path = clean_remote_path(&path)?;
        let already = lock().opened.get(&host).is_some_and(|l| is_opened(l, &path));
        if !already {
            use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
            let sure = app
                .dialog()
                .message(format!("Open\n{path}\n\non {host}?\n\nTreeline reads this repository over ssh. Nothing is installed there."))
                .title("Open remote repository")
                .kind(MessageDialogKind::Info)
                .buttons(MessageDialogButtons::OkCancelCustom("Open".into(), "Cancel".into()))
                .blocking_show();
            if !sure {
                return Err("Cancelled.".into());
            }
            remember(&host, path.clone());
        }
        let conn = connection(&app, &host)?;
        conn.call("allow", json!({ "path": path }), CALL_TIMEOUT)?;
        Ok(path)
    })
    .await
}

#[tauri::command]
pub async fn remote_call(app: AppHandle, host: String, method: String, params: Value) -> Result<Value, String> {
    blocking(move || {
        if !READ_METHODS.contains(&method.as_str()) {
            return Err(format!("{method} is not available for repositories on another computer yet."));
        }
        let path = params.get("path").and_then(Value::as_str).unwrap_or_default().to_string();
        let opened = lock().opened.get(&host).cloned().unwrap_or_default();
        if !is_opened(&opened, &path) {
            return Err(format!("{path} has not been opened on {host} in Treeline."));
        }
        connection(&app, &host)?.call(&method, params, CALL_TIMEOUT)
    })
    .await
}

#[tauri::command]
pub fn remote_disconnect(host: String) {
    if let Some(conn) = lock().conns.remove(&host) {
        conn.close();
    }
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
    }
}
