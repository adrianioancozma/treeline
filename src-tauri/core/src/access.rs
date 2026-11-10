//! What the user may do with a folder, checked on the computer where it is, before it is opened.
//!
//! Used when a folder on another computer is about to be opened: it lets the app refuse a wrong path with a clear
//! reason, explain git's "dubious ownership" refusal, and mark a repository the user cannot write to as read-only.
//! It deliberately does not require the folder to be on the allow-list (that is what is being decided), and it
//! never writes: write permission is asked of the system (`access(2)`), not tried.

use crate::git::run_git;
use serde::Serialize;

#[derive(Serialize, Debug, Default, PartialEq)]
pub struct Access {
    pub exists: bool,
    pub readable: bool,
    pub repo: bool,
    pub toplevel: Option<String>,
    /// None where the system does not say (Windows).
    pub owned_by_me: Option<bool>,
    pub tree_writable: Option<bool>,
    pub git_writable: Option<bool>,
    /// Why the folder cannot be opened, in words for the user.
    pub problem: Option<String>,
}

#[cfg(unix)]
fn writable(path: &str) -> Option<bool> {
    let c = std::ffi::CString::new(path).ok()?;
    // SAFETY: a valid NUL-terminated path; access() only reads it.
    Some(unsafe { libc::access(c.as_ptr(), libc::W_OK) } == 0)
}
#[cfg(not(unix))]
fn writable(_: &str) -> Option<bool> {
    None
}

#[cfg(unix)]
fn owned_by_me(meta: &std::fs::Metadata) -> Option<bool> {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: geteuid() has no preconditions.
    Some(meta.uid() == unsafe { libc::geteuid() })
}
#[cfg(not(unix))]
fn owned_by_me(_: &std::fs::Metadata) -> Option<bool> {
    None
}

pub fn access(path: &str) -> Access {
    let mut a = Access::default();
    let Ok(meta) = std::fs::metadata(path) else {
        a.problem = Some(format!("{path} does not exist."));
        return a;
    };
    a.exists = true;
    if !meta.is_dir() {
        a.problem = Some(format!("{path} is a file, not a folder."));
        return a;
    }
    a.owned_by_me = owned_by_me(&meta);
    a.tree_writable = writable(path);
    a.readable = std::fs::read_dir(path).is_ok();
    if !a.readable {
        a.problem = Some(format!("You are not allowed to read {path}."));
        return a;
    }
    match run_git(path, ["rev-parse", "--show-toplevel"]) {
        Ok(top) => {
            a.repo = true;
            a.toplevel = Some(top.trim().to_string());
            if let Ok(dir) = run_git(path, ["rev-parse", "--absolute-git-dir"]) {
                a.git_writable = writable(dir.trim());
            }
        }
        Err(e) if e.contains("dubious ownership") => {
            a.problem = Some(format!(
                "Git refuses {path} because it belongs to another user. If you trust it, run on that computer:\n\
                 git config --global --add safe.directory {path}"
            ));
        }
        Err(_) => a.problem = Some(format!("{path} is not a Git repository.")),
    }
    a
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp(name: &str) -> String {
        let d = std::env::temp_dir().join(format!("treeline-core-access-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::canonicalize(&d).unwrap().to_string_lossy().to_string()
    }

    #[test]
    fn reports_what_the_user_may_do() {
        let _serial = crate::serial_git();
        let repo = temp("repo");
        run_git(&repo, ["init", "-q"]).unwrap();
        let a = access(&repo);
        assert!(a.exists && a.readable && a.repo && a.problem.is_none(), "{a:?}");
        assert_eq!(a.toplevel.as_deref(), Some(repo.as_str()));
        assert_eq!((a.owned_by_me, a.tree_writable, a.git_writable), (Some(true), Some(true), Some(true)));

        let plain = temp("plain");
        let p = access(&plain);
        assert!(p.exists && p.readable && !p.repo, "{p:?}");
        assert!(p.problem.unwrap().contains("is not a Git repository"));

        let missing = format!("{plain}/missing");
        assert!(access(&missing).problem.unwrap().contains("does not exist"));
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&plain);
    }

    #[test]
    fn a_repository_without_write_permission_is_read_only() {
        let _serial = crate::serial_git();
        if unsafe { libc::geteuid() } == 0 {
            return; // root may write anywhere; nothing to check
        }
        let repo = temp("readonly");
        run_git(&repo, ["init", "-q"]).unwrap();
        let git_dir = format!("{repo}/.git");
        for d in [&git_dir, &repo] {
            std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o555)).unwrap();
        }
        let a = access(&repo);
        for d in [&repo, &git_dir] {
            std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(a.repo && a.readable && a.problem.is_none(), "{a:?}");
        assert_eq!((a.tree_writable, a.git_writable), (Some(false), Some(false)));
        let _ = std::fs::remove_dir_all(&repo);
    }
}
