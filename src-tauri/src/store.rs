//! Persists the UI state (open tabs and bookmarks) as one JSON file in the app config folder.

use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn state_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("state.json"))
}

#[tauri::command]
pub fn load_state(app: AppHandle) -> Result<Option<String>, String> {
    let path = state_path(&app)?;
    match fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Removes tabs and bookmarks that point at a folder the user has not opened. The saved file is what
/// the next start trusts (`register_saved_repos`), so the page must not be able to plant a folder in
/// it: only folders that were already allowed can be written back.
pub(crate) fn without_unopened_folders(json: &str) -> Result<String, String> {
    let mut state: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    for list in ["tabs", "bookmarks"] {
        if let Some(items) = state.get_mut(list).and_then(|v| v.as_array_mut()) {
            items.retain(|item| match item.get("path").and_then(|v| v.as_str()) {
                Some(p) => crate::git::ensure_allowed(p).is_ok(),
                None => true, // a tab that is still the home screen has no folder
            });
        }
    }
    serde_json::to_string(&state).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_state(app: AppHandle, json: String) -> Result<(), String> {
    let json = without_unopened_folders(&json)?;
    let path = state_path(&app)?;
    // Write then rename so a crash can never leave a half-written state file.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_cannot_plant_folders_in_the_saved_state() {
        let base = std::env::temp_dir().join(format!("treeline-state-{}", std::process::id()));
        let kept = base.join("kept");
        let planted = base.join("planted");
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::create_dir_all(&planted).unwrap();
        crate::git::allow_repo(&kept.to_string_lossy());

        let state = serde_json::json!({
            "tabs": [
                { "id": "1", "path": kept.to_string_lossy(), "name": "kept" },
                { "id": "2", "path": planted.to_string_lossy(), "name": "planted" },
                { "id": "3", "path": null, "name": "home" }
            ],
            "bookmarks": [
                { "id": "b1", "path": kept.to_string_lossy() },
                { "id": "b2", "path": "C:/Windows" }
            ],
            "activeId": "2",
            "theme": "dark"
        });
        let cleaned: serde_json::Value = serde_json::from_str(&without_unopened_folders(&state.to_string()).unwrap()).unwrap();
        let ids = |list: &str| -> Vec<String> {
            cleaned[list].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap().to_string()).collect()
        };
        assert_eq!(ids("tabs"), ["1", "3"], "the planted folder is dropped, the home tab stays");
        assert_eq!(ids("bookmarks"), ["b1"]);
        assert_eq!(cleaned["theme"], "dark", "everything else is kept");
        assert!(without_unopened_folders("not json").is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}

/// Trusts the repositories saved in the tabs and bookmarks, so a restart reopens them. It runs once at
/// start-up, from Rust, before any page script exists; the page cannot call it.
pub fn register_saved_repos(app: &AppHandle) {
    let Ok(path) = state_path(app) else { return };
    let Ok(text) = fs::read_to_string(path) else { return };
    let Ok(state) = serde_json::from_str::<serde_json::Value>(&text) else { return };
    for list in ["tabs", "bookmarks"] {
        for item in state.get(list).and_then(|v| v.as_array()).into_iter().flatten() {
            if let Some(p) = item.get("path").and_then(|v| v.as_str()) {
                crate::git::allow_repo(p);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Settings (the git executable the user chose)
// ---------------------------------------------------------------------------

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

/// Reads the saved git path (if any) and applies it. A missing or corrupt file is ignored:
/// settings must never block start-up.
pub fn apply_saved_settings(app: &AppHandle) {
    let Ok(path) = settings_path(app) else { return };
    let Ok(text) = fs::read_to_string(path) else { return };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')) else { return };
    let custom = v.get("gitPath").and_then(|p| p.as_str()).filter(|p| !p.is_empty()).map(PathBuf::from);
    crate::git::set_custom_git(custom);
}

/// Saves the git executable to use (`None` / empty = detect automatically) after checking that it
/// really is git, then returns the resulting git info.
#[tauri::command]
pub async fn set_git_path(app: AppHandle, path: Option<String>) -> Result<crate::git::GitInfo, String> {
    let chosen = path.filter(|p| !p.trim().is_empty()).map(PathBuf::from);
    if let Some(p) = &chosen {
        crate::git::verify_git(p)?;
    }
    let json = serde_json::json!({ "gitPath": chosen.as_ref().map(|p| p.to_string_lossy().into_owned()) });
    let file = settings_path(&app)?;
    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, json.to_string()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
    crate::git::set_custom_git(chosen);
    crate::git::git_info().await
}
