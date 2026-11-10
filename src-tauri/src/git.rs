//! Git engine core: process helpers, repository status, submodules and cloning.
//! Every operation shells out to the system `git` CLI on a blocking worker thread,
//! so the UI never waits on Git.

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
pub use treeline_core::git::{
    GitInfo, LITERAL, OpResult, SPAWN_ERROR_PREFIX, git_info_now, run_git, run_git_ok_codes, run_git_stdin, run_op,
    set_custom_git, verify_git,
};
#[cfg(test)]
use std::process::Command;
#[cfg(test)]
pub(crate) use treeline_core::git::{git_cmd, git_program, merge_path, spawn_error};
#[cfg(all(test, windows))]
use treeline_core::git::windows_git_candidates;
pub use treeline_core::git::stream_git;
#[cfg(test)]
use treeline_core::git::percent_of;
use treeline_core::git::ProgressSink;
pub use treeline_core::trust::{
    allow_key, set_trust_file, read_trust_file, persist_trusted, restore_allowed_keys, is_filesystem_root, allow_repo, allow_clone_destination, clone_destination_allowed, ensure_allowed,
};
#[cfg(test)]
use treeline_core::trust::write_trust_file;
pub use treeline_core::status::{RepoStatus, parse_status, parse_gitlinks, redact_urls};

// ---------------------------------------------------------------------------
// Process helpers
// ---------------------------------------------------------------------------

/// Reports which git the app uses and whether it works.
#[tauri::command]
pub async fn git_info() -> Result<GitInfo, String> {
    blocking(|| Ok(git_info_now())).await
}

// The opened folders are remembered between runs in a file that only Rust writes, kept apart from the
// tabs file the page can write. It stores resolved locations, so a link created later at a remembered
// path cannot make that path lead somewhere else.
/// Shows the operating system's folder picker and trusts whatever the user picks. `purpose` is
/// `repo` (a repository to open) or `destination` (a folder to clone into).
#[tauri::command]
pub async fn pick_folder(app: AppHandle, title: String, purpose: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().set_title(title).blocking_pick_folder())
        .await
        .map_err(|e| e.to_string())?;
    let Some(picked) = picked else { return Ok(None) };
    let path = picked.into_path().map_err(|e| e.to_string())?.to_string_lossy().into_owned();
    if purpose != "destination" && is_filesystem_root(&allow_key(&path)) {
        return Err("Choose the repository's folder, not a whole drive.".into());
    }
    if purpose == "destination" {
        allow_clone_destination(&path);
    } else {
        allow_repo(&path);
    }
    Ok(Some(path))
}

/// Rejects a branch, tag, remote or commit name that git would read as an option (for example
/// `--upload-pack=...`) or that contains a NUL byte. Git itself forbids ref names that start with `-`,
/// so no legitimate name is refused.
pub(crate) fn ref_arg(s: &str) -> Result<(), String> {
    if s.starts_with('-') || s.contains('\0') {
        let shown: String = s.chars().filter(|c| !c.is_control()).take(60).collect();
        return Err(format!("'{shown}' is not a valid branch, tag, remote or commit name."));
    }
    Ok(())
}

/// `ref_arg` for an optional value (None and empty are fine).
pub(crate) fn opt_ref_arg(s: &Option<String>) -> Result<(), String> {
    s.as_deref().map_or(Ok(()), ref_arg)
}

pub(crate) async fn blocking<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Streaming (network) operations
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
struct OpProgress {
    id: String,
    line: String,
    percent: Option<u8>,
}

/// Shows git progress in the window as `op-progress` events (the payload the UI has always received).
pub(crate) struct WindowProgress<'a>(pub &'a AppHandle);

impl ProgressSink for WindowProgress<'_> {
    fn progress(&self, id: &str, line: &str, percent: Option<u8>) {
        let _ = self.0.emit("op-progress", OpProgress { id: id.to_string(), line: line.to_string(), percent });
    }
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug)]
pub struct Submodule {
    pub path: String,
    pub sha: String,
    /// `ok`, `uninitialized`, `modified` (checked-out commit differs) or `conflict`.
    pub state: String,
    pub describe: Option<String>,
}

/// Detects an in-progress merge / rebase / cherry-pick / revert from the git directory.
fn repo_state(path: &str) -> Option<String> {
    let dir = PathBuf::from(
        run_git(path, ["rev-parse", "--absolute-git-dir"])
            .ok()?
            .trim(),
    );
    if dir.join("rebase-merge").exists() || dir.join("rebase-apply").exists() {
        Some("rebase".into())
    } else if dir.join("MERGE_HEAD").exists() {
        Some("merge".into())
    } else if dir.join("CHERRY_PICK_HEAD").exists() {
        Some("cherry-pick".into())
    } else if dir.join("REVERT_HEAD").exists() {
        Some("revert".into())
    } else {
        None
    }
}

/// Returns the repository's top-level folder, or an error if `path` is not inside one.
/// The top level of the repository `path` is in, checked against what the user opened and then trusted.
fn resolve_toplevel(path: &str) -> Result<String, String> {
        ensure_allowed(path)?;
        let out = run_git(path, ["rev-parse", "--show-toplevel"])
            .map_err(|e| {
                // Only blame the folder when git actually ran; "git could not start" must stay visible.
                if e.starts_with(SPAWN_ERROR_PREFIX) { e } else { "This folder is not a Git repository.".to_string() }
            })?;
        let top = out.trim().to_string();
        // The folder the user picked may be inside a repository (or be a submodule of one they opened):
        // the repository's own top level is the folder every later command will use. Git computes it from
        // the folder's own files, so it is only trusted when it is the folder itself or one above it. A
        // crafted `.git` file or `core.worktree` can make git name a folder elsewhere; that must not widen
        // what the app may touch.
        let (here, there) = (allow_key(path), allow_key(&top));
        if here != there {
            if !here.starts_with(&format!("{there}/")) {
                return Err("That repository's working folder lies outside the folder you opened, so it was not opened.".into());
            }
            // Widening to a folder above the one that was opened is only done for the ordinary layout, where
            // the repository's .git sits directly inside that top folder. A `core.worktree` (or any other
            // redirect) can name an ancestor such as a home folder while the repository lives below it.
            let git_dir = run_git(path, ["rev-parse", "--absolute-git-dir"]).map(|d| allow_key(d.trim())).unwrap_or_default();
            if git_dir != allow_key(&format!("{top}/.git")) {
                return Err("That folder is inside a repository with an unusual layout. Open the repository's own top folder instead.".into());
            }
        }
        allow_repo(&top);
        Ok(top)
}

#[tauri::command]
pub async fn git_toplevel(path: String) -> Result<String, String> {
    blocking(move || resolve_toplevel(&path)).await
}

/// Describes a repository config entry that makes git run a program, or `None` when it is harmless
/// or something the app never triggers. Git LFS filters and plain credential helpers such as
/// `manager` are normal and ignored.
pub(crate) fn risky_config_entry(key: &str, value: &str) -> Option<String> {
    let k = key.to_ascii_lowercase();
    let backslash = char::from(92);
    let program = |why: &str| Some(format!("{key} = {value}: {why}"));
    match k.as_str() {
        "core.sshcommand" => return program("runs a program to connect over SSH"),
        "core.hookspath" => return program("git hooks from that folder run when you commit, merge or check out"),
        "core.worktree" => return program("points the repository's working folder somewhere else"),
        "core.editor" | "sequence.editor" | "core.pager" | "core.askpass" | "gpg.program" | "core.gitproxy" | "diff.external" => {
            return program("runs a program")
        }
        "credential.helper" => {
            return (value.starts_with('!') || value.contains('/') || value.contains(backslash))
                .then(|| format!("{key} = {value}: runs a program to supply credentials"));
        }
        "include.path" => return program("pulls in another config file, which is not checked here"),
        _ => {}
    }
    if k.starts_with("includeif.") && k.ends_with(".path") {
        return program("pulls in another config file, which is not checked here");
    }
    if k.starts_with("filter.") && (k.ends_with(".clean") || k.ends_with(".smudge") || k.ends_with(".process")) {
        return (!value.trim_start().starts_with("git-lfs"))
            .then(|| format!("{key} = {value}: runs a program on files when they are staged or checked out"));
    }
    if k.starts_with("merge.") && k.ends_with(".driver") {
        return program("runs a program to merge files");
    }
    if k.starts_with("diff.") && (k.ends_with(".textconv") || k.ends_with(".command")) {
        return program("runs a program to convert or show diffs");
    }
    if k.starts_with("credential.") && k.ends_with(".helper") {
        return (value.starts_with('!') || value.contains('/') || value.contains(backslash))
            .then(|| format!("{key} = {value}: runs a program to supply credentials"));
    }
    if k.starts_with("remote.") && (k.ends_with(".vcs") || k.ends_with(".uploadpack") || k.ends_with(".receivepack")) {
        return program("runs a program for this remote");
    }
    // Signing programs (gpg.program is above; also gpg.openpgp / gpg.x509 / gpg.ssh .program) run on every signed commit or tag.
    if k.starts_with("gpg.") && (k.ends_with(".program") || k == "gpg.ssh.defaultkeycommand") {
        return program("runs a program to sign commits and tags");
    }
    if (k.starts_with("difftool.") || k.starts_with("mergetool.") || k.starts_with("trailer."))
        && (k.ends_with(".cmd") || k.ends_with(".command"))
    {
        return program("runs a command");
    }
    // pager.<command> may be true/false or a program to page that command's output with. Git compares the
    // value exactly (ignoring only case), so a quoted "true " with a space is a program: do not trim.
    if k.starts_with("pager.") && !matches!(value.to_ascii_lowercase().as_str(), "true" | "false" | "yes" | "no" | "on" | "off" | "1" | "0" | "") {
        return program("runs a program to page output");
    }
    if k.starts_with("submodule.") && k.ends_with(".update") && value.starts_with('!') {
        return program("runs a command when updating this submodule");
    }
    None
}

/// What in this repository's own settings or hooks could make git run a program when the app (or
/// you) commits, merges, checks out or fetches. A repository you made or cloned from a source you
/// trust normally has none of this; a folder handed to you with its `.git` included can.
pub(crate) fn repo_risks_now(path: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    // Every entry git would use, with its origin, after following include.path. Only what the repository
    // itself supplies counts: its .git/config, its per-worktree config and the files those include. With -z
    // and --show-scope the output alternates "scope", "key<LF>value".
    let raw = run_git(path, ["config", "--list", "--show-scope", "--includes", "-z"])?;
    let mut tokens = raw.split('\0');
    while let Some(scope) = tokens.next() {
        let Some(entry) = tokens.next() else { break };
        if !matches!(scope, "local" | "worktree") {
            continue;
        }
        let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
        if let Some(line) = risky_config_entry(key, value) {
            out.push(line);
        }
    }
    // Hook scripts (git ships only *.sample files).
    if let Ok(dir) = run_git(path, ["rev-parse", "--git-path", "hooks"]) {
        let dir = PathBuf::from(dir.trim());
        let dir = if dir.is_absolute() { dir } else { Path::new(path).join(dir) };
        if let Ok(read) = std::fs::read_dir(&dir) {
            let mut names: Vec<String> = read
                .flatten()
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.ends_with(".sample"))
                .collect();
            names.sort();
            if !names.is_empty() {
                out.push(format!("Hook scripts ({}): these run when you commit, merge, check out or push", names.join(", ")));
            }
        }
    }
    Ok(out)
}

/// Lists the ways this repository could make git run a program (see `repo_risks_now`).
#[tauri::command]
pub async fn repo_risks(path: String) -> Result<Vec<String>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        repo_risks_now(&path)
    }).await
}

#[tauri::command]
pub async fn repo_status(path: String) -> Result<RepoStatus, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let raw = run_git(
            &path,
            [
                "status",
                "--porcelain=v2",
                "--branch",
                "--untracked-files=normal",
                "-z",
            ],
        )?;
        let mut status = parse_status(&raw);
        status.state = repo_state(&path);
        Ok(status)
    })
    .await
}

#[tauri::command]
pub async fn list_submodules(path: String) -> Result<Vec<Submodule>, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        // A repo without a .gitmodules has none; skip the process entirely.
        if !PathBuf::from(&path).join(".gitmodules").exists() {
            return Ok(Vec::new());
        }
        // `git submodule status` costs 0.6–0.9 s on real repos. One `ls-files` for the recorded
        // commits plus a parallel `rev-parse HEAD` per checked-out submodule is several times faster.
        let raw = run_git(&path, ["ls-files", "--stage", "-z"])?;
        let links = parse_gitlinks(&raw);
        let root = PathBuf::from(&path);
        let mut out: Vec<Submodule> = std::thread::scope(|scope| {
            let handles: Vec<_> = links
                .iter()
                .map(|(sha, sub_path, conflicted)| {
                    let dir = root.join(sub_path);
                    scope.spawn(move || {
                        let state = if *conflicted {
                            "conflict"
                        } else if !dir.join(".git").exists() {
                            "uninitialized"
                        } else {
                            match run_git(&dir.to_string_lossy(), ["rev-parse", "HEAD"]) {
                                Ok(head) if head.trim() != sha => "modified",
                                _ => "ok",
                            }
                        };
                        state
                    })
                })
                .collect();
            links
                .iter()
                .zip(handles)
                .map(|((sha, sub_path, _), h)| Submodule {
                    path: sub_path.clone(),
                    sha: sha.clone(),
                    state: h.join().unwrap_or("ok").to_string(),
                    describe: None,
                })
                .collect()
        });
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    })
    .await
}

/// Initialises and checks out one submodule (and any nested ones) so it can be opened.
#[tauri::command]
pub async fn init_submodule(path: String, submodule: String) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        run_op(
            &path,
            [LITERAL, "submodule", "update", "--init", "--recursive", "--", &submodule],
            &[],
        )
    })
    .await
}

/// Picks the ref a submodule should check out for `branch`: the local branch if there is one,
/// otherwise a remote-tracking branch of that name (`origin` preferred). `None` = no such branch.
pub(crate) fn pick_branch_source(refs: &str, branch: &str) -> Option<(bool, String)> {
    let mut remote: Option<String> = None;
    for line in refs.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line == branch {
            return Some((true, line.to_string()));
        }
        // Remote-tracking refs look like `origin/feature/x`; the remote name has no slash.
        if let Some((r, rest)) = line.split_once('/') {
            if rest == branch && !line.ends_with("/HEAD") && (remote.is_none() || r == "origin") {
                remote = Some(line.to_string());
            }
        }
    }
    remote.map(|r| (false, r))
}

/// Switches every checked-out submodule to `branch` when a branch of that name exists there
/// (locally, or as a remote-tracking branch, which gets a local tracking branch). Submodules that
/// are not initialised or lack the branch are left alone. A submodule whose checkout fails (for
/// example uncommitted changes) is reported but does not stop the others.
#[tauri::command]
pub async fn switch_submodules_branch(path: String, branch: String, pull: Option<bool>) -> Result<OpResult, String> {
    let pull = pull.unwrap_or(false);
    blocking(move || {
        ensure_allowed(&path)?;
        ref_arg(&branch)?;
        let raw = run_git(&path, ["ls-files", "--stage", "-z"])?;
        let root = PathBuf::from(&path);
        let (mut ok, mut lines) = (true, Vec::new());
        for (_, sub, _) in parse_gitlinks(&raw) {
            let dir = root.join(&sub);
            if !dir.join(".git").exists() {
                lines.push(format!("{sub}: skipped (not initialised)"));
                continue;
            }
            let dir_s = dir.to_string_lossy().into_owned();
            let refs = run_git(
                &dir_s,
                ["for-each-ref", "--format=%(refname:short)", "refs/heads", "refs/remotes"],
            )?;
            let Some((local, source)) = pick_branch_source(&refs, &branch) else {
                lines.push(format!("{sub}: skipped (no branch {branch})"));
                continue;
            };
            let res = if local {
                run_op(&dir_s, ["checkout", &branch], &[])?
            } else {
                run_op(&dir_s, ["checkout", "--track", "-b", &branch, &source], &[])?
            };
            if res.ok && pull {
                // Only a fast-forward: a submodule is never left half-merged. A branch without an
                // upstream simply has nothing to pull.
                let has_upstream = run_op(&dir_s, ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"], &[])?.ok;
                if !has_upstream {
                    lines.push(format!("{sub}: switched to {branch} (no upstream, not pulled)"));
                } else {
                    let p = run_op(&dir_s, ["pull", "--ff-only"], &[])?;
                    if p.ok {
                        lines.push(format!("{sub}: switched to {branch} and pulled"));
                    } else {
                        ok = false;
                        lines.push(format!("{sub}: switched to {branch}, but the pull FAILED
{}", p.output.trim()));
                    }
                }
            } else if res.ok {
                lines.push(format!("{sub}: switched to {branch}"));
            } else {
                ok = false;
                lines.push(format!("{sub}: FAILED\n{}", res.output.trim()));
            }
        }
        if lines.is_empty() {
            lines.push("No submodules.".into());
        }
        Ok(OpResult { ok, output: lines.join("\n") })
    })
    .await
}

/// Updates every submodule to the commit recorded by the superproject.
#[tauri::command]
pub async fn update_submodules(path: String, remote: bool) -> Result<OpResult, String> {
    blocking(move || {
        ensure_allowed(&path)?;
        let mut args = vec!["submodule", "update", "--init", "--recursive"];
        if remote {
            args.push("--remote");
        }
        run_op(&path, args, &[])
    })
    .await
}

#[derive(Serialize)]
pub struct CloneOutcome {
    pub path: String,
    /// Set when the repository was cloned but something secondary (usually a submodule) failed.
    pub warning: Option<String>,
}

/// Clones `url` into `dest`, streaming progress as `op-progress` events tagged with `id`.
/// A clone whose only failure is a submodule still counts: the repository is usable.
#[tauri::command]
pub async fn clone_repo(
    app: AppHandle,
    id: String,
    url: String,
    dest: String,
    recursive: bool,
) -> Result<CloneOutcome, String> {
    blocking(move || {
        // A clone creates files, so the destination must come from the user: a folder chosen with the
        // picker, or (when the path was typed) one the user confirms in a window of the operating system.
        let parent = Path::new(&dest).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        if !clone_destination_allowed(&parent) {
            use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
            let sure = app
                .dialog()
                .message(format!("Clone\n{}\n\ninto\n{}", redact_urls(&url), dest))
                .title("Clone repository")
                .kind(MessageDialogKind::Warning)
                .buttons(MessageDialogButtons::OkCancelCustom("Clone".into(), "Cancel".into()))
                .blocking_show();
            if !sure {
                return Err("Clone cancelled.".into());
            }
            allow_clone_destination(&parent);
        }
        let mut args = vec!["clone".to_string(), "--progress".to_string()];
        if recursive {
            args.push("--recurse-submodules".into());
        }
        args.extend(["--".to_string(), url, dest.clone()]);
        let res = stream_git(&WindowProgress(&app), &id, None, &args)?;
        if res.ok {
            allow_repo(&dest);
            return Ok(CloneOutcome { path: dest, warning: None });
        }
        if is_submodule_failure(&res.output) && run_git(&dest, ["rev-parse", "--git-dir"]).is_ok() {
            allow_repo(&dest);
            return Ok(CloneOutcome {
                path: dest,
                warning: Some(format!(
                    "Cloned, but some submodules could not be fetched. Open the repository and use Update submodules to retry.\n\n{}",
                    last_lines(&res.output, 6)
                )),
            });
        }
        Err(res.output)
    })
    .await
}

fn is_submodule_failure(output: &str) -> bool {
    let o = output.to_lowercase();
    o.contains("submodule") || o.contains("failed to clone")
}

fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_and_changes() {
        let raw = "# branch.oid abc\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
                   1 .M N... 100644 100644 100644 aaa bbb src/a b.ts\0\
                   2 R. N... 100644 100644 100644 aaa bbb R100 new.ts\0old.ts\0\
                   u UU N... 100644 100644 100644 100644 a b c conflicted.ts\0\
                   ? notes.txt\0";
        let s = parse_status(raw);
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.head.as_deref(), Some("abc"));
        assert_eq!((s.ahead, s.behind), (2, 1));
        assert_eq!(s.files.len(), 4);
        assert_eq!(s.files[0].path, "src/a b.ts");
        assert_eq!(s.files[0].worktree, "M");
        assert_eq!(s.files[1].orig_path.as_deref(), Some("old.ts"));
        assert!(s.files[2].conflicted);
        assert!(s.files[3].untracked);
    }

    #[test]
    fn merge_path_puts_missing_dirs_first_without_duplicates() {
        let p = merge_path("/usr/bin:/bin:/usr/local/bin", &["/opt/homebrew/bin", "/usr/local/bin"], ':');
        assert_eq!(p, "/opt/homebrew/bin:/usr/bin:/bin:/usr/local/bin");
        assert_eq!(merge_path("", &["/a"], ':'), "/a");
        assert_eq!(merge_path("/x:", &["/a"], ':'), "/a:/x");
    }

    #[test]
    fn detached_head_is_not_a_branch() {
        let s = parse_status("# branch.oid abc\0# branch.head (detached)\0");
        assert_eq!(s.branch, None);
        assert_eq!(s.head.as_deref(), Some("abc"));
    }

    #[test]
    fn initial_commit_has_no_head() {
        let s = parse_status("# branch.oid (initial)\0# branch.head main\0");
        assert_eq!(s.head, None);
    }

    #[test]
    fn parses_gitlinks_only() {
        let raw = "100644 aaa 0\tREADME.md\0160000 bbb 0\tsrc/lib\0160000 ccc 1\tsrc/conflicted\0160000 ddd 2\tsrc/conflicted\0";
        let g = parse_gitlinks(raw);
        assert_eq!(
            g,
            vec![
                ("bbb".to_string(), "src/lib".to_string(), false),
                ("ccc".to_string(), "src/conflicted".to_string(), true),
            ]
        );
    }

    #[test]
    fn a_repository_cannot_run_a_program_through_core_fsmonitor() {
        let dir = std::env::temp_dir().join(format!("treeline-fsm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = dir.to_string_lossy().to_string();
        let git = |args: &[&str]| {
            run_git(&repo, ["-c", "user.name=t", "-c", "user.email=t@t"].iter().chain(args.iter()).copied()).unwrap()
        };
        git(&["init", "-q"]);
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "one"]);

        let marker = dir.join("marker.txt");
        let hook = dir.join("hook.sh");
        let fwd = |p: &Path| p.to_string_lossy().replace('\\', "/");
        std::fs::write(&hook, format!("#!/bin/sh\necho ran > \"{}\"\n", fwd(&marker))).unwrap();
        git(&["config", "core.fsmonitor", &fwd(&hook)]);

        // Control: plain git does run the program. If it does not on this machine the check proves nothing.
        let _ = Command::new("git").arg("-C").arg(&repo).args(["status", "-s"]).output();
        let control_ran = marker.exists();
        let _ = std::fs::remove_file(&marker);

        git(&["status", "-s"]);
        assert!(!marker.exists(), "the hook must not run through the app's git calls");
        if !control_ran {
            eprintln!("note: plain git did not run the hook here, so this run could not show the difference");
        }

        // The same setting arriving through an included file (which can even be a tracked file that a
        // later checkout changes) is covered too, because the override does not depend on where it came from.
        git(&["config", "--unset", "core.fsmonitor"]);
        let extra = dir.join("extra.cfg");
        std::fs::write(&extra, format!("[core]\n\tfsmonitor = {}\n", fwd(&hook))).unwrap();
        git(&["config", "include.path", &fwd(&extra)]);
        git(&["status", "-s"]);
        assert!(!marker.exists(), "the hook must not run when it comes from an included file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn classifies_risky_repository_settings() {
        let risky = [
            ("core.hooksPath", ".husky"),
            ("core.worktree", ".."),
            ("core.sshCommand", "ssh -i key"),
            ("filter.evil.smudge", "calc.exe"),
            ("merge.evil.driver", "calc %O %A %B"),
            ("diff.evil.textconv", "calc"),
            ("credential.helper", "!echo pw"),
            ("credential.https://x.y.helper", "/opt/helper"),
            ("include.path", "../x.cfg"),
            ("includeIf.gitdir:/a/.path", "x"),
            ("remote.origin.vcs", "ext"),
            ("submodule.a.update", "!calc"),
            ("core.editor", "vim"),
            ("gpg.program", "x"),
            ("gpg.openpgp.program", "x"),
            ("gpg.x509.program", "x"),
            ("gpg.ssh.program", "x"),
            ("gpg.ssh.defaultKeyCommand", "x"),
            ("mergetool.x.cmd", "x"),
            ("trailer.t.command", "x"),
            ("pager.log", "less -R"),
            ("pager.log", "true "),
            ("pager.log", " yes"),
        ];
        for (k, v) in risky {
            assert!(risky_config_entry(k, v).is_some(), "{k}");
        }
        let fine = [
            ("filter.lfs.clean", "git-lfs clean -- %f"),
            ("filter.lfs.smudge", "git-lfs smudge -- %f"),
            ("filter.lfs.process", "git-lfs filter-process"),
            ("credential.helper", "manager"),
            ("credential.helper", "osxkeychain"),
            ("core.autocrlf", "true"),
            ("user.name", "Me"),
            ("alias.st", "!echo status"),
            ("remote.origin.url", "https://x"),
            ("branch.main.remote", "origin"),
            ("pager.log", "true"),
            ("gpg.format", "ssh"),
            ("commit.gpgsign", "true"),
        ];
        for (k, v) in fine {
            assert!(risky_config_entry(k, v).is_none(), "{k}");
        }
    }

    #[test]
    fn a_repository_with_hooks_and_drivers_is_reported() {
        let dir = std::env::temp_dir().join(format!("treeline-risk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = dir.to_string_lossy().to_string();
        run_git(&repo, ["init", "-q"]).unwrap();
        assert!(repo_risks_now(&repo).unwrap().is_empty(), "a fresh repository is clean (sample hooks are ignored)");

        std::fs::write(dir.join(".git").join("hooks").join("pre-commit"), "#!/bin/sh\n").unwrap();
        run_git(&repo, ["config", "merge.evil.driver", "calc"]).unwrap();
        run_git(&repo, ["config", "filter.lfs.clean", "git-lfs clean -- %f"]).unwrap();
        let risks = repo_risks_now(&repo).unwrap();
        assert!(risks.iter().any(|r| r.contains("Hook scripts") && r.contains("pre-commit")), "{risks:?}");
        assert!(risks.iter().any(|r| r.contains("merge.evil.driver")), "{risks:?}");
        assert!(!risks.iter().any(|r| r.contains("git-lfs")), "LFS is normal: {risks:?}");

        // Per-worktree config is not part of --local, and an included file is not in the repository config
        // itself: both must still be seen.
        run_git(&repo, ["config", "extensions.worktreeConfig", "true"]).unwrap();
        run_git(&repo, ["config", "--worktree", "core.hooksPath", ".evil-hooks"]).unwrap();
        let extra = dir.join("extra.cfg");
        std::fs::write(&extra, "[core]\n\tsshCommand = evil-ssh\n").unwrap();
        run_git(&repo, ["config", "include.path", &extra.to_string_lossy().replace('\\', "/")]).unwrap();
        let risks = repo_risks_now(&repo).unwrap();
        assert!(risks.iter().any(|r| r.contains("core.hookspath") && r.contains(".evil-hooks")), "worktree config: {risks:?}");
        assert!(risks.iter().any(|r| r.to_ascii_lowercase().contains("core.sshcommand") && r.contains("evil-ssh")), "included file: {risks:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn credentials_in_urls_are_hidden() {
        let cases = [
            ("fatal: unable to access 'https://user:ghp_secret@github.com/a/b.git/': 403", "fatal: unable to access 'https://***@github.com/a/b.git/': 403"),
            ("remote: https://ghp_tokenonly@github.com/a/b.git", "remote: https://***@github.com/a/b.git"),
            ("cloning ssh://me:pw@host.example/repo", "cloning ssh://***@host.example/repo"),
            ("pushing ssh://git@github.com/a/b.git", "pushing ssh://git@github.com/a/b.git"),
            ("plain https://github.com/a/b.git here", "plain https://github.com/a/b.git here"),
            ("two https://a:b@h1/x and https://c@h2/y", "two https://***@h1/x and https://***@h2/y"),
            ("no urls at all", "no urls at all"),
            ("trailing https://u:p@host", "trailing https://***@host"),
            ("an email me@example.com stays", "an email me@example.com stays"),
        ];
        for (input, want) in cases {
            assert_eq!(redact_urls(input), want, "{input}");
        }
    }

    #[test]
    fn git_cannot_widen_what_is_trusted_by_reporting_a_folder_elsewhere() {
        let base = std::env::temp_dir().join(format!("treeline-top-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let opened = base.join("opened");
        let outside = base.join("outside");
        std::fs::create_dir_all(opened.join("sub")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        run_git(&s(&opened), ["init", "-q"]).unwrap();

        // The ordinary case: a folder inside a repository resolves to the repository above it, which
        // then becomes trusted too.
        allow_repo(&s(&opened.join("sub")));
        let top = resolve_toplevel(&s(&opened.join("sub"))).unwrap();
        assert_eq!(allow_key(&top), allow_key(&s(&opened)));
        assert!(ensure_allowed(&s(&opened)).is_ok(), "the repository's own top level is trusted");

        // A repository whose config sends the working tree somewhere else must not make that place trusted.
        allow_repo(&s(&opened));
        run_git(&s(&opened), ["config", "core.worktree", &s(&outside)]).unwrap();
        let result = resolve_toplevel(&s(&opened));
        assert!(ensure_allowed(&s(&outside)).is_err(), "a folder git merely named is not trusted: {result:?}");
        if let Ok(t) = &result {
            assert_eq!(allow_key(t), allow_key(&s(&opened)), "if it answers at all it must be the folder itself");
        }

        // core.worktree naming a folder ABOVE the opened one is an ancestor, but not the ordinary layout, so
        // it must not widen trust to it either (think of a home folder).
        run_git(&s(&opened), ["config", "core.worktree", &s(&base)]).unwrap();
        let widened = resolve_toplevel(&s(&opened.join("sub")));
        assert!(widened.is_err(), "an ancestor reached through core.worktree is refused: {widened:?}");
        assert!(ensure_allowed(&s(&base)).is_err(), "the folder above is still not trusted");
        assert!(ensure_allowed(&s(&outside)).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn trusted_folders_are_remembered_as_resolved_locations() {
        let base = std::env::temp_dir().join(format!("treeline-trust-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("trusted-folders.json");
        assert!(read_trust_file(&file).is_none(), "no file yet");

        let keys: Vec<String> = vec!["c:/work/a".into(), "c:/work/b".into()];
        write_trust_file(&file, &keys).unwrap();
        assert_eq!(read_trust_file(&file).unwrap(), keys);
        std::fs::write(&file, "not json").unwrap();
        assert!(read_trust_file(&file).is_none(), "a damaged file is treated as absent, not trusted");

        let many: Vec<String> = (0..600).map(|i| format!("c:/work/{i}")).collect();
        write_trust_file(&file, &many).unwrap();
        let back = read_trust_file(&file).unwrap();
        assert_eq!(back.len(), 500);
        assert_eq!(back.last().unwrap(), "c:/work/599", "the most recent are kept");

        // What is remembered is the real location, so a link made later at the old path cannot redirect it.
        let real = base.join("real");
        let link = base.join("link");
        std::fs::create_dir_all(&real).unwrap();
        #[cfg(windows)]
        let made = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&link).arg(&real).output().map(|o| o.status.success()).unwrap_or(false);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&real, &link).is_ok();
        if made {
            assert_eq!(allow_key(&link.to_string_lossy()), allow_key(&real.to_string_lossy()), "stored as the real folder");
        } else {
            eprintln!("skipped link check: cannot create links here");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn only_opened_folders_are_accepted() {
        let base = std::env::temp_dir().join(format!("treeline-allow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let opened = base.join("opened");
        let inside = opened.join("libs").join("sub");
        let sibling = base.join("sibling");
        let outside = base.join("outside");
        for d in [&inside, &sibling, &outside] {
            std::fs::create_dir_all(d).unwrap();
        }
        let s = |p: &Path| p.to_string_lossy().into_owned();

        assert!(ensure_allowed(&s(&opened)).is_err(), "nothing is allowed until the user opens it");
        allow_repo(&s(&opened));
        assert!(ensure_allowed(&s(&opened)).is_ok());
        assert!(ensure_allowed(&s(&inside)).is_ok(), "a submodule folder inside an opened repository");
        assert!(ensure_allowed(&s(&opened.join("not-created-yet"))).is_ok(), "a path below it that does not exist yet");
        assert!(ensure_allowed(&s(&sibling)).is_err(), "a sibling folder");
        assert!(ensure_allowed(&s(&base)).is_err(), "the parent of an opened folder");
        assert!(ensure_allowed(&format!("{}-evil", s(&opened))).is_err(), "a folder whose name merely starts the same");
        assert!(ensure_allowed(&s(&opened.join("..").join("outside"))).is_err(), "climbing out with ..");
        assert!(ensure_allowed(&format!("{}/../outside", s(&opened))).is_err());
        assert!(ensure_allowed("").is_err());
        assert!(ensure_allowed("relative/path").is_err());
        if cfg!(windows) {
            assert!(ensure_allowed(&s(&opened).to_uppercase()).is_ok(), "Windows paths ignore case");
            assert!(ensure_allowed(&s(&opened).replace('\\', "/")).is_ok(), "and either slash");
        }

        // A link or junction inside the opened folder that leads elsewhere is judged by where it leads.
        let link = opened.join("escape");
        #[cfg(windows)]
        let made = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&link).arg(&outside).output().map(|o| o.status.success()).unwrap_or(false);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&outside, &link).is_ok();
        if made {
            assert!(ensure_allowed(&s(&link)).is_err(), "a link out of the opened folder");
            assert!(ensure_allowed(&s(&link.join("anything"))).is_err());
        } else {
            eprintln!("skipped link check: cannot create links here");
        }

        // A whole drive or the filesystem root is never trusted: it would trust everything below it.
        for root in ["/", "C:/", "C:", "D:\\"] {
            allow_repo(root);
        }
        assert!(ensure_allowed(&s(&sibling)).is_err(), "roots were not registered");
        assert!(is_filesystem_root(&allow_key(if cfg!(windows) { "C:\\" } else { "/" })));
        assert!(!is_filesystem_root(&allow_key(&s(&opened))));

        // Clone destinations are a separate, exact-match list.
        assert!(!clone_destination_allowed(&s(&sibling)));
        allow_clone_destination(&s(&sibling));
        assert!(clone_destination_allowed(&s(&sibling)));
        assert!(!clone_destination_allowed(&s(&sibling.join("deeper"))), "only the chosen folder itself");
        assert!(ensure_allowed(&s(&sibling)).is_err(), "choosing a destination does not open it");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn option_like_names_are_refused() {
        for bad in ["--upload-pack=calc.exe", "-D", "--orphan=x", "a\0b"] {
            assert!(ref_arg(bad).is_err(), "{bad:?}");
        }
        for ok in ["main", "feature/x-1", "origin/develop", "v1.2.3", "HEAD~2", "abc123", "refs/heads/x", "stash@{0}", ""] {
            assert!(ref_arg(ok).is_ok(), "{ok:?}");
        }
        assert!(opt_ref_arg(&None).is_ok());
        assert!(opt_ref_arg(&Some("--x".into())).is_err());
        assert!(opt_ref_arg(&Some("main".into())).is_ok());
    }

    #[test]
    fn picks_submodule_branch_source() {
        let refs = "main\nfeature/x\norigin/HEAD\norigin/main\nfork/dev\norigin/dev\norigin/feature/y\n";
        assert_eq!(pick_branch_source(refs, "feature/x"), Some((true, "feature/x".into())));
        assert_eq!(pick_branch_source(refs, "dev"), Some((false, "origin/dev".into())));
        assert_eq!(pick_branch_source(refs, "feature/y"), Some((false, "origin/feature/y".into())));
        assert_eq!(pick_branch_source(refs, "HEAD"), None);
        assert_eq!(pick_branch_source(refs, "nope"), None);
    }

    #[test]
    fn recognises_submodule_clone_failures() {
        assert!(is_submodule_failure("fatal: clone of 'x' into submodule path 'a' failed"));
        assert!(!is_submodule_failure("fatal: destination path 'x' already exists and is not an empty directory."));
        assert_eq!(last_lines("a\nb\nc", 2), "b\nc");
    }

    #[test]
    fn extracts_percent() {
        assert_eq!(percent_of("Receiving objects:  42% (1/2)"), Some(42));
        assert_eq!(percent_of("Cloning into 'x'..."), None);
    }
}

#[cfg(test)]
mod git_path_tests {
    use super::*;

    #[test]
    fn verify_git_rejects_missing_and_non_git_files() {
        assert!(verify_git(Path::new("/definitely/not/here/git")).is_err());
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("notgit.txt");
        std::fs::write(&fake, "x").unwrap();
        assert!(verify_git(&fake).is_err());
    }

    /// A chosen file that no longer exists must be ignored, never break every git call.
    #[test]
    fn missing_custom_git_falls_back_to_detection() {
        set_custom_git(Some(PathBuf::from("/definitely/not/here/git")));
        let program = git_program();
        set_custom_git(None);
        assert_ne!(program, PathBuf::from("/definitely/not/here/git"));
    }

    /// The installed-app situation: PATH has no git, yet the standard Git for Windows folder is
    /// found. Runs the probe in a child process with an empty PATH so other tests are unaffected.
    #[cfg(windows)]
    #[test]
    fn windows_fallback_finds_git_when_path_is_empty() {
        const NAME: &str = "git::git_path_tests::windows_fallback_finds_git_when_path_is_empty";
        if std::env::var_os("TREELINE_PROBE").is_some() {
            let info = git_info_now();
            assert!(info.version.is_some(), "git not found with empty PATH: {}", info.path);
            return;
        }
        if windows_git_candidates().iter().all(|p| !p.is_file()) {
            return; // Git is installed somewhere unusual on this machine; nothing to probe.
        }
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env("TREELINE_PROBE", "1")
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    }

    #[test]
    fn toplevel_keeps_spawn_errors_visible() {
        let e = spawn_error(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert!(e.starts_with(SPAWN_ERROR_PREFIX));
    }
}
