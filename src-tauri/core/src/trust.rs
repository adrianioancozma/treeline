//! Which folders the app may act on.
//!
//! Every command takes a folder path from the web page. If script ever ran in the page it could point
//! git at any folder, so the commands only accept folders the user has actually opened. The list is
//! filled only from Rust: the native folder picker (`pick_folder`), the saved tabs and bookmarks read at
//! start-up, a repository derived from an allowed one (a submodule, the top level of a picked
//! sub-folder) and a finished clone.

use std::path::{Path, PathBuf};

pub const NOT_OPENED: &str = "That folder has not been opened in Treeline. Use Open repository to choose it.";

pub fn opened_repos() -> &'static std::sync::Mutex<Vec<String>> {
    static REPOS: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> = std::sync::OnceLock::new();
    REPOS.get_or_init(Default::default)
}

pub fn clone_destinations() -> &'static std::sync::Mutex<Vec<String>> {
    static DESTS: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> = std::sync::OnceLock::new();
    DESTS.get_or_init(Default::default)
}

/// A comparable form of a folder path: symlinks and junctions resolved when the folder exists,
/// forward slashes, no Windows `\\?\` prefix, no trailing slash, and lower case on Windows.
pub fn allow_key(p: &str) -> String {
    let path = Path::new(p);
    // Resolve links through the deepest part that exists, then add the rest back unchanged: a path that
    // does not exist yet below a junction must still be judged by where the junction leads.
    let mut existing = path;
    let mut rest: Vec<&std::ffi::OsStr> = Vec::new();
    let mut real = loop {
        if let Ok(resolved) = std::fs::canonicalize(existing) {
            break resolved;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
                rest.push(name);
                existing = parent;
            }
            _ => break path.to_path_buf(), // nothing resolves (or a relative path): compared as written
        }
    };
    if real != path {
        for part in rest.iter().rev() {
            real.push(part);
        }
    }
    let mut s = real.to_string_lossy().replace(char::from(92), "/");
    if let Some(rest) = s.strip_prefix("//?/UNC/") {
        s = format!("//{rest}");
    } else if let Some(rest) = s.strip_prefix("//?/") {
        s = rest.to_string();
    }
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    if cfg!(windows) {
        s = s.to_lowercase();
    }
    s
}

/// Adds `key` unless it is already there; true when it was added.
pub fn push_unique(list: &std::sync::Mutex<Vec<String>>, key: String) -> bool {
    if let Ok(mut g) = list.lock() {
        if !g.contains(&key) {
            g.push(key);
            return true;
        }
    }
    false
}

pub static TRUST_FILE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub const TRUST_FILE_MAX: usize = 500;

pub fn set_trust_file(path: PathBuf) {
    let _ = TRUST_FILE.set(path);
}

pub fn write_trust_file(path: &Path, keys: &[String]) -> std::io::Result<()> {
    let start = keys.len().saturating_sub(TRUST_FILE_MAX);
    let body = serde_json::json!({ "repos": &keys[start..] }).to_string();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, path)
}

pub fn read_trust_file(path: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(value.get("repos")?.as_array()?.iter().filter_map(|v| v.as_str().map(String::from)).collect())
}

/// Writes the current list of opened folders to the trust file (if one is set).
pub fn persist_trusted() {
    if let (Some(file), Ok(list)) = (TRUST_FILE.get(), opened_repos().lock()) {
        let _ = write_trust_file(file, &list);
    }
}

/// Loads folders that were trusted in an earlier run (already in resolved form).
pub fn restore_allowed_keys(keys: &[String]) {
    for key in keys {
        if !is_filesystem_root(key) {
            push_unique(opened_repos(), key.clone());
        }
    }
}

/// A whole drive or the filesystem root ("/", "C:", "C:/"): trusting one would trust everything below it.
pub fn is_filesystem_root(key: &str) -> bool {
    key == "/" || key.is_empty() || (key.len() <= 3 && key.as_bytes().get(1) == Some(&b':'))
}

/// Marks a folder (and everything inside it) as one the user has opened. A filesystem root is never
/// trusted this way.
pub fn allow_repo(path: &str) {
    let key = allow_key(path);
    if !is_filesystem_root(&key) && push_unique(opened_repos(), key) {
        persist_trusted();
    }
}

/// Marks a folder as one the user has chosen to clone into.
pub fn allow_clone_destination(path: &str) {
    push_unique(clone_destinations(), allow_key(path));
}

pub fn clone_destination_allowed(path: &str) -> bool {
    let key = allow_key(path);
    clone_destinations().lock().map(|g| g.contains(&key)).unwrap_or(false)
}

/// Fails unless `path` is (inside) a folder the user opened. A path that tries to climb out with `..`
/// is refused outright, and a link or junction is judged by where it really leads.
pub fn ensure_allowed(path: &str) -> Result<(), String> {
    let key = allow_key(path);
    if key.split('/').any(|part| part == "..") {
        return Err(NOT_OPENED.into());
    }
    let ok = opened_repos()
        .lock()
        .map(|g| g.iter().any(|root| key == *root || key.starts_with(&format!("{root}/"))))
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(NOT_OPENED.into())
    }
}
