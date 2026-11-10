//! Listing folders on the computer where the repositories are, to pick one without typing its path.
//!
//! Only folder names leave this function (and whether each is a git repository): no file names, no contents. It does
//! not require the folder to be on the allow-list: choosing a folder is how one gets there, and the final choice is
//! still confirmed by the user before anything is opened.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Most entries returned for one folder; a bigger folder is cut (and says so).
pub const MAX_ENTRIES: usize = 2000;

#[derive(Serialize, Debug, PartialEq)]
pub struct DirEntry {
    pub name: String,
    pub repo: bool,
}

#[derive(Serialize, Debug)]
pub struct DirListing {
    /// The folder listed, as a full path with links resolved.
    pub path: String,
    pub parent: Option<String>,
    /// The user's home on this computer, so the app can show `~/…`.
    pub home: Option<String>,
    /// Whether the listed folder itself is a repository.
    pub repo: bool,
    pub entries: Vec<DirEntry>,
    pub truncated: bool,
}

fn is_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// `~` and `~/…` mean the home folder of the user on this computer.
fn expand_home(path: &str, home: Option<&str>) -> Result<PathBuf, String> {
    match (path.strip_prefix('~'), home) {
        (Some(rest), Some(h)) if rest.is_empty() || rest.starts_with('/') => Ok(PathBuf::from(format!("{h}{rest}"))),
        (Some(_), _) => Err(format!("“{path}” cannot be read here.")),
        (None, _) if path.starts_with('/') => Ok(PathBuf::from(path)),
        _ => Err(format!("“{path}” is not a full folder path.")),
    }
}

pub fn list_dir(path: &str, show_hidden: bool) -> Result<DirListing, String> {
    let home = std::env::var("HOME").ok().filter(|h| h.starts_with('/'));
    let wanted = expand_home(if path.is_empty() { "~" } else { path }, home.as_deref())?;
    let dir = std::fs::canonicalize(&wanted).map_err(|_| format!("{} does not exist.", wanted.display()))?;
    if !dir.is_dir() {
        return Err(format!("{} is not a folder.", dir.display()));
    }
    let read = std::fs::read_dir(&dir).map_err(|_| format!("You are not allowed to read {}.", dir.display()))?;
    let mut entries: Vec<DirEntry> = read
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !show_hidden && name.starts_with('.') {
                return None;
            }
            let full = e.path();
            // Folders, including links to folders.
            full.is_dir().then(|| DirEntry { repo: is_repo(&full), name })
        })
        .collect();
    entries.sort_by_key(|e| e.name.to_lowercase());
    let truncated = entries.len() > MAX_ENTRIES;
    entries.truncate(MAX_ENTRIES);
    Ok(DirListing {
        parent: dir.parent().map(|p| p.to_string_lossy().into_owned()),
        repo: is_repo(&dir),
        path: dir.to_string_lossy().into_owned(),
        home: home.and_then(|h| std::fs::canonicalize(h).ok()).map(|h| h.to_string_lossy().into_owned()),
        entries,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("treeline-core-browse-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::canonicalize(&d).unwrap()
    }

    #[test]
    fn lists_folders_marks_repositories_and_hides_hidden_ones() {
        let d = temp("list");
        for sub in ["beta", "Alpha", "Zeta", ".cache", "repo/.git"] {
            std::fs::create_dir_all(d.join(sub)).unwrap();
        }
        std::fs::write(d.join("notes.txt"), "x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(d.join("beta"), d.join("link-to-beta")).unwrap();

        let l = list_dir(&d.to_string_lossy(), false).unwrap();
        let names: Vec<&str> = l.entries.iter().map(|e| e.name.as_str()).collect();
        let mut expected = vec!["Alpha", "beta", "repo", "Zeta"];
        if cfg!(unix) {
            expected.insert(2, "link-to-beta");
        }
        assert_eq!(names, expected, "folders only, sorted ignoring case, hidden ones left out, files never");
        assert!(l.entries.iter().find(|e| e.name == "repo").unwrap().repo);
        assert!(!l.entries.iter().find(|e| e.name == "beta").unwrap().repo);
        assert_eq!(l.path, d.to_string_lossy());
        assert_eq!(l.parent.as_deref(), d.parent().map(|p| p.to_string_lossy()).as_deref());
        assert!(!l.repo && !l.truncated);

        let all = list_dir(&d.to_string_lossy(), true).unwrap();
        assert!(all.entries.iter().any(|e| e.name == ".cache"), "shown on request");
        assert!(list_dir(&d.join("repo").to_string_lossy(), true).unwrap().repo, "the folder itself is a repository");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn refuses_what_is_not_a_readable_folder() {
        let d = temp("refuse");
        std::fs::write(d.join("file"), "x").unwrap();
        assert!(list_dir(&d.join("missing").to_string_lossy(), false).unwrap_err().contains("does not exist"));
        assert!(list_dir(&d.join("file").to_string_lossy(), false).unwrap_err().contains("is not a folder"));
        assert!(list_dir("relative/path", false).unwrap_err().contains("not a full folder path"));
        assert!(list_dir("~someone", false).is_err(), "only the user's own home");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cuts_a_huge_folder_and_says_so() {
        let d = temp("huge");
        for i in 0..MAX_ENTRIES + 5 {
            std::fs::create_dir(d.join(format!("d{i:05}"))).unwrap();
        }
        let l = list_dir(&d.to_string_lossy(), false).unwrap();
        assert_eq!(l.entries.len(), MAX_ENTRIES);
        assert!(l.truncated);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn tilde_means_the_users_home() {
        assert_eq!(expand_home("~", Some("/home/a")).unwrap(), PathBuf::from("/home/a"));
        assert_eq!(expand_home("~/dev", Some("/home/a")).unwrap(), PathBuf::from("/home/a/dev"));
        assert!(expand_home("~/dev", None).is_err());
        assert!(expand_home("~someone", Some("/home/a")).is_err(), "another user's home is not expanded");
        assert_eq!(expand_home("/srv", None).unwrap(), PathBuf::from("/srv"));
    }
}
