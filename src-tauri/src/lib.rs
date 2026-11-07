use tauri::menu::{Menu, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Wry};

mod git;
mod ops;
mod store;
mod watcher;

/// The macOS menu bar. Tauri's default menu binds Cmd+W to "Close Window", which would close the
/// whole app instead of the current tab (the app handles Cmd+W itself), so it is left out here.
/// The Edit menu is required for Cmd+C / Cmd+V / Cmd+A to work in text fields.
/// (Built on every platform so it is compile-checked everywhere; only applied on macOS.)
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn build_app_menu(app: &AppHandle<Wry>) -> tauri::Result<Menu<Wry>> {
    let app_menu = Submenu::with_items(
        app,
        "APMGIT CLIENT",
        true,
        &[
            &PredefinedMenuItem::about(app, None, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let view_menu = Submenu::with_items(app, "View", true, &[&PredefinedMenuItem::fullscreen(app, None)?])?;
    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[&PredefinedMenuItem::minimize(app, None)?, &PredefinedMenuItem::maximize(app, None)?],
    )?;
    Menu::with_items(app, &[&app_menu, &edit_menu, &view_menu, &window_menu])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(watcher::WatchState::default())
        .setup(|app| {
            store::apply_saved_settings(app.handle());
            #[cfg(target_os = "macos")]
            app.set_menu(build_app_menu(app.handle())?)?;
            #[cfg(not(target_os = "macos"))]
            let _ = app;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            git::git_toplevel,
            git::git_info,
            store::set_git_path,
            git::repo_status,
            git::list_submodules,
            git::init_submodule,
            git::update_submodules,
            git::clone_repo,
            ops::stage_files,
            ops::unstage_files,
            ops::stage_all,
            ops::unstage_all,
            ops::discard_files,
            ops::apply_patch,
            ops::commit,
            ops::head_message,
            ops::file_diff,
            ops::commit_files,
            ops::commit_diff,
            ops::commit_detail,
            ops::git_log,
            ops::list_refs,
            ops::checkout,
            ops::checkout_remote,
            ops::create_branch,
            ops::rename_branch,
            ops::delete_branch,
            ops::create_tag,
            ops::delete_tag,
            ops::fetch,
            ops::fetch_ref,
            ops::pull,
            ops::push,
            ops::list_remotes,
            ops::delete_remote_branch,
            ops::merge,
            ops::rebase,
            ops::cherry_pick,
            ops::revert,
            ops::reset,
            ops::sequencer_action,
            ops::rebase_todo,
            ops::rebase_interactive,
            ops::stash_list,
            ops::stash_push,
            ops::stash_action,
            ops::conflict_file,
            ops::resolve_side,
            ops::save_resolution,
            ops::remove_lock_file,
            ops::reveal_in_file_manager,
            ops::open_terminal,
            store::load_state,
            store::save_state,
            watcher::watch_repo,
            watcher::unwatch_repo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running APMGIT CLIENT");
}
