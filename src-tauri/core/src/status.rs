use serde::Serialize;

#[derive(Serialize, Default, Debug)]
pub struct FileChange {
    pub path: String,
    /// Index (staged) state letter, `.` when unchanged.
    pub index: String,
    /// Working tree state letter, `.` when unchanged.
    pub worktree: String,
    pub orig_path: Option<String>,
    pub untracked: bool,
    pub conflicted: bool,
}

#[derive(Serialize, Default, Debug)]
pub struct RepoStatus {
    pub branch: Option<String>,
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<FileChange>,
    /// `merge`, `rebase`, `cherry-pick` or `revert` while one is in progress.
    pub state: Option<String>,
}

pub fn parse_status(raw: &str) -> RepoStatus {
    let mut status = RepoStatus::default();
    let mut tokens = raw.split('\0');
    while let Some(tok) = tokens.next() {
        if tok.is_empty() {
            continue;
        }
        if let Some(rest) = tok.strip_prefix("# ") {
            if let Some(v) = rest.strip_prefix("branch.head ") {
                // A detached HEAD is reported as "(detached)"; model it as no branch.
                status.branch = Some(v.to_string()).filter(|b| b != "(detached)");
            } else if let Some(v) = rest.strip_prefix("branch.oid ") {
                if v != "(initial)" {
                    status.head = Some(v.to_string());
                }
            } else if let Some(v) = rest.strip_prefix("branch.upstream ") {
                status.upstream = Some(v.to_string());
            } else if let Some(v) = rest.strip_prefix("branch.ab ") {
                let mut parts = v.split(' ');
                status.ahead = parts
                    .next()
                    .and_then(|p| p.trim_start_matches('+').parse().ok())
                    .unwrap_or(0);
                status.behind = parts
                    .next()
                    .and_then(|p| p.trim_start_matches('-').parse().ok())
                    .unwrap_or(0);
            }
            continue;
        }
        match tok.chars().next().unwrap_or(' ') {
            '1' => {
                let f: Vec<&str> = tok.splitn(9, ' ').collect();
                if f.len() == 9 {
                    status.files.push(change(f[1], f[8], None, false));
                }
            }
            '2' => {
                let f: Vec<&str> = tok.splitn(10, ' ').collect();
                let orig = tokens.next().map(str::to_string);
                if f.len() == 10 {
                    status.files.push(change(f[1], f[9], orig, false));
                }
            }
            'u' => {
                let f: Vec<&str> = tok.splitn(11, ' ').collect();
                if f.len() == 11 {
                    let mut c = change(f[1], f[10], None, false);
                    c.conflicted = true;
                    status.files.push(c);
                }
            }
            '?' => {
                if let Some(path) = tok.strip_prefix("? ") {
                    status.files.push(change("??", path, None, true));
                }
            }
            _ => {}
        }
    }
    status
}

pub fn change(xy: &str, path: &str, orig: Option<String>, untracked: bool) -> FileChange {
    let mut chars = xy.chars();
    FileChange {
        path: path.to_string(),
        index: chars.next().unwrap_or('.').to_string(),
        worktree: chars.next().unwrap_or('.').to_string(),
        orig_path: orig,
        untracked,
        conflicted: false,
    }
}

/// Extracts submodules (gitlinks, mode 160000) from `git ls-files --stage -z` output as
/// `(recorded sha, path, conflicted)`. A gitlink with a non-zero stage is mid-merge.
pub fn parse_gitlinks(raw: &str) -> Vec<(String, String, bool)> {
    let mut out: Vec<(String, String, bool)> = Vec::new();
    for entry in raw.split('\0') {
        let Some((meta, path)) = entry.split_once('\t') else { continue };
        let mut parts = meta.split(' ');
        let (Some(mode), Some(sha), Some(stage)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if mode != "160000" {
            continue;
        }
        match out.iter_mut().find(|(_, p, _)| p == path) {
            Some(existing) => existing.2 = true,
            None => out.push((sha.to_string(), path.to_string(), stage != "0")),
        }
    }
    out
}

/// Hides credentials embedded in URLs, for example `https://user:token@host/repo.git` or
/// `https://token@host/repo.git`, so that git's error and progress messages never put a secret on
/// screen, in a bug report or in a copied error. Only the user part is replaced; the host stays.
/// For web URLs any user part is hidden (it is often a token); for other schemes only `user:password`.
pub fn redact_urls(text: &str) -> String {
    let single_quote = char::from(39);
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let lower = head.to_ascii_lowercase();
        let web = lower.ends_with("http://") || lower.ends_with("https://");
        // The authority part ends at the first '/', whitespace or quote.
        let end = tail
            .find(|c: char| c == '/' || c.is_whitespace() || c == single_quote || c == '"')
            .unwrap_or(tail.len());
        let authority = &tail[..end];
        match authority.rfind('@') {
            Some(at) if web || authority[..at].contains(':') => {
                out.push_str("***");
                out.push_str(&authority[at..]);
            }
            _ => out.push_str(authority),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}
