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

#[tauri::command]
pub fn save_state(app: AppHandle, json: String) -> Result<(), String> {
    let path = state_path(&app)?;
    // Write then rename so a crash can never leave a half-written state file.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
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
