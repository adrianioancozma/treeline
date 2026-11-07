mod git;
mod ops;
mod store;
mod watcher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(watcher::WatchState::default())
        .invoke_handler(tauri::generate_handler![
            git::git_toplevel,
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
