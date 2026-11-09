//! Watches the active repository and tells the UI when something changed, replacing polling.

use notify_debouncer_mini::{
    new_debouncer,
    notify::{RecommendedWatcher, RecursiveMode},
    DebounceEventResult, Debouncer,
};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

#[derive(Default)]
pub struct WatchState(Mutex<Option<Debouncer<RecommendedWatcher>>>);

/// Paths whose changes can never affect what the UI shows.
fn is_noise(root: &Path, path: &Path) -> bool {
    // Judge only the part below the repository root, so a repo that merely lives under a folder
    // called "dist" or "target" is not ignored wholesale.
    let rel = path.strip_prefix(root).unwrap_or(path);
    let s = format!("/{}", rel.to_string_lossy().replace('\\', "/"));
    // Dependency folders are almost always git-ignored and churn constantly. Build-output folders
    // (dist, target) are not filtered: they may be tracked, and ignoring them could hide real changes.
    if s.contains("/node_modules/") {
        return true;
    }
    if let Some(idx) = s.find("/.git/") {
        let inside = &s[idx + 6..];
        // Object / log / module churn happens on every git command and changes nothing visible.
        return inside.starts_with("objects")
            || inside.starts_with("logs")
            || inside.starts_with("modules")
            || inside.ends_with(".lock");
    }
    false
}

/// Starts watching `path` (replacing any previous watch). Emits `repo-changed` with the path.
#[tauri::command]
pub fn watch_repo(app: AppHandle, state: State<'_, WatchState>, path: String) -> Result<(), String> {
    crate::git::ensure_allowed(&path)?;
    let emit_path = path.clone();
    let root = std::path::PathBuf::from(&path);
    let mut debouncer = new_debouncer(
        Duration::from_millis(400),
        move |res: DebounceEventResult| {
            if let Ok(events) = res {
                if events.iter().any(|e| !is_noise(&root, &e.path)) {
                    let _ = app.emit("repo-changed", emit_path.clone());
                }
            }
        },
    )
    .map_err(|e| e.to_string())?;
    debouncer
        .watcher()
        .watch(Path::new(&path), RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;
    *state.0.lock().map_err(|e| e.to_string())? = Some(debouncer);
    Ok(())
}

#[tauri::command]
pub fn unwatch_repo(state: State<'_, WatchState>) {
    if let Ok(mut g) = state.0.lock() {
        *g = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_noise() {
        let root = Path::new("C:/r");
        assert!(is_noise(root, Path::new("C:/r/node_modules/x/y.js")));
        assert!(is_noise(root, Path::new(r"C:\r\.git\objects\ab\cd")));
        assert!(is_noise(root, Path::new("C:/r/.git/index.lock")));
        assert!(!is_noise(root, Path::new("C:/r/.git/index")));
        assert!(!is_noise(root, Path::new("C:/r/.git/HEAD")));
        assert!(!is_noise(root, Path::new("C:/r/src/a.ts")));
        // A repo that lives under a folder named like build output must still be watched.
        let nested = Path::new("C:/work/dist/myrepo");
        assert!(!is_noise(nested, Path::new("C:/work/dist/myrepo/src/a.ts")));
        assert!(!is_noise(nested, Path::new("C:/work/dist/myrepo/.git/index")));
    }
}
