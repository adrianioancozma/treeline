//! Which folders the app may act on.
//!
//! Every command takes a folder path from the web page. If script ever ran in the page it could point
//! git at any folder, so the commands only accept folders the user has actually opened. The list is
//! filled only from Rust: the native folder picker (`pick_folder`), the saved tabs and bookmarks read at
//! start-up, a repository derived from an allowed one (a submodule, the top level of a picked
//! sub-folder) and a finished clone.
//!
//! The list belongs to the process: the desktop app keeps one for the repositories on this computer, and a helper
//! started on another computer keeps its own for the repositories there, filled from what the user opened in the
//! app. Folders added on the helper's side (see `set_grant_observer`) are reported back, so the app can remember
//! them for the next connection.

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
    if !is_filesystem_root(&key) && push_unique(opened_repos(), key.clone()) {
        persist_trusted();
        if let Some(observe) = GRANT_OBSERVER.get() {
            observe(&key);
        }
    }
}

/// Told about every folder newly added to the list, including the ones added as a side effect (the top level of a
/// picked sub-folder, a submodule, a finished clone). The desktop app does not need it (it saves the list itself);
/// a helper running next to remote repositories uses it to report each new folder to the app that started it.
static GRANT_OBSERVER: std::sync::OnceLock<Box<dyn Fn(&str) + Send + Sync>> = std::sync::OnceLock::new();

/// Sets the observer of new folders, once per process; false if one was already set.
pub fn set_grant_observer(observe: impl Fn(&str) + Send + Sync + 'static) -> bool {
    GRANT_OBSERVER.set(Box::new(observe)).is_ok()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("treeline-core-trust-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn dot_dot_cannot_climb_out_of_an_opened_folder() {
        // A missing folder stops link resolution, so the rest of the path is kept as written; without the `..`
        // rule, "<opened>/missing/../../x" would still start with the opened folder and pass.
        let base = temp("dotdot");
        let opened = base.join("opened");
        std::fs::create_dir_all(&opened).unwrap();
        // The real location (on macOS the temp folder is behind a link), so only the `..` rule can refuse.
        let opened = std::fs::canonicalize(&opened).unwrap();
        allow_repo(&opened.to_string_lossy());
        assert!(ensure_allowed(&opened.to_string_lossy()).is_ok());
        let escape = format!("{}/missing/../../outside", opened.to_string_lossy());
        assert!(ensure_allowed(&escape).is_err(), "{escape} climbs out of the opened folder");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_observer_hears_each_new_folder_once() {
        let heard = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&heard);
        assert!(set_grant_observer(move |key| sink.lock().unwrap().push(key.to_string())));
        assert!(!set_grant_observer(|_| {}), "only one observer per process");

        let base = temp("observer");
        let repo = base.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        allow_repo(&repo.to_string_lossy());
        allow_repo(&repo.to_string_lossy()); // already there: not reported again
        allow_repo("/"); // a filesystem root is never trusted, so never reported
        let key = allow_key(&repo.to_string_lossy());
        let mine: Vec<String> = heard.lock().unwrap().iter().filter(|k| k.starts_with(&allow_key(&base.to_string_lossy()))).cloned().collect();
        assert_eq!(mine, vec![key]);
        assert!(!heard.lock().unwrap().iter().any(|k| k == "/"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
