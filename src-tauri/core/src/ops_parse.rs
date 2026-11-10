use serde::Serialize;

#[derive(Serialize, Debug, PartialEq)]
pub struct FileEntry {
    pub status: String,
    pub path: String,
    pub old_path: Option<String>,
}

pub fn parse_name_status(raw: &str) -> Vec<FileEntry> {
    let mut out = Vec::new();
    let mut tokens = raw.split('\0').filter(|t| !t.is_empty());
    while let Some(status) = tokens.next() {
        let first = status.chars().next().unwrap_or('M');
        if first == 'R' || first == 'C' {
            let old = tokens.next().map(str::to_string);
            if let Some(new) = tokens.next() {
                out.push(FileEntry { status: first.to_string(), path: new.to_string(), old_path: old });
            }
        } else if let Some(p) = tokens.next() {
            out.push(FileEntry { status: first.to_string(), path: p.to_string(), old_path: None });
        }
    }
    out
}

#[derive(Serialize, Debug)]
pub struct Commit {
    pub sha: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub refs: Vec<String>,
    pub subject: String,
}

pub fn parse_log(raw: &str) -> Vec<Commit> {
    raw.split('\x1e')
        .filter_map(|rec| {
            let rec = rec.trim_matches(|c| c == '\n' || c == '\r');
            if rec.is_empty() {
                return None;
            }
            let f: Vec<&str> = rec.split('\x1f').collect();
            if f.len() < 7 {
                return None;
            }
            Some(Commit {
                sha: f[0].into(),
                parents: f[1].split_whitespace().map(String::from).collect(),
                author: f[2].into(),
                email: f[3].into(),
                time: f[4].parse().unwrap_or(0),
                refs: f[5]
                    .split(", ")
                    .filter(|r| !r.is_empty())
                    .map(String::from)
                    .collect(),
                subject: f[6].trim().into(),
            })
        })
        .collect()
}

#[derive(Serialize, Debug)]
pub struct RefInfo {
    /// `branch`, `remote` or `tag`.
    pub kind: String,
    pub name: String,
    pub sha: String,
    pub upstream: Option<String>,
    pub track: Option<String>,
    pub head: bool,
}

pub fn parse_refs(raw: &str) -> Vec<RefInfo> {
    raw.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\x1f').collect();
            if f.len() < 7 {
                return None;
            }
            let (full, short) = (f[0], f[1]);
            let (kind, name) = if let Some(n) = full.strip_prefix("refs/heads/") {
                ("branch", n.to_string())
            } else if let Some(n) = full.strip_prefix("refs/remotes/") {
                if n.ends_with("/HEAD") {
                    return None;
                }
                ("remote", n.to_string())
            } else if let Some(n) = full.strip_prefix("refs/tags/") {
                ("tag", n.to_string())
            } else {
                return None;
            };
            let _ = short;
            // Annotated tags point at a tag object; `*objectname` is the commit underneath.
            let sha = if !f[6].is_empty() { f[6] } else { f[2] };
            Some(RefInfo {
                kind: kind.into(),
                name,
                sha: sha.into(),
                upstream: Some(f[3].to_string()).filter(|s| !s.is_empty()),
                track: Some(f[4].to_string()).filter(|s| !s.is_empty()),
                head: f[5] == "*",
            })
        })
        .collect()
}

#[derive(Serialize, Debug)]
pub struct StashEntry {
    #[serde(rename = "ref")]
    pub reference: String,
    pub message: String,
    pub time: i64,
}

pub fn parse_stashes(raw: &str) -> Vec<StashEntry> {
    raw.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.splitn(3, '\x1f').collect();
            (f.len() == 3).then(|| StashEntry {
                reference: f[0].into(),
                message: f[1].into(),
                time: f[2].parse().unwrap_or(0),
            })
        })
        .collect()
}

/// Parses `git merge-tree --write-tree --name-only --no-messages` output: the result tree id,
/// then (only on conflict) the conflicted paths, one per line.
pub fn parse_merge_tree(out: &str) -> (String, Vec<String>) {
    let mut lines = out.lines();
    let tree = lines.next().unwrap_or("").trim().to_string();
    let mut files: Vec<String> = Vec::new();
    for l in lines.map(str::trim).filter(|l| !l.is_empty()) {
        if !files.iter().any(|f| f == l) {
            files.push(l.to_string());
        }
    }
    (tree, files)
}

/// The branch named in a merge message such as "Merge branch 'bugfix' into release-4".
pub fn merge_msg_branch(line: &str) -> Option<String> {
    let q = char::from(39);
    let rest = ["Merge branch ", "Merge remote-tracking branch ", "Merge tag "]
        .iter()
        .find_map(|p| line.strip_prefix(p))?;
    let rest = rest.strip_prefix(q)?;
    let end = rest.find(q)?;
    Some(rest[..end].to_string())
}

/// One commit that touched a file, with the file's name at that commit (it changes across renames).
#[derive(Serialize, Debug)]
pub struct FileCommit {
    pub sha: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub refs: Vec<String>,
    pub subject: String,
    pub file: String,
}

pub fn parse_file_log(raw: &str) -> Vec<FileCommit> {
    raw.split('\x1e')
        .filter_map(|rec| {
            let f: Vec<&str> = rec.split('\x1f').collect();
            // seven commit fields, then the file name `--name-only` prints after the format line
            if f.len() < 8 {
                return None;
            }
            let file = f[7].trim().lines().next().unwrap_or("").trim().to_string();
            Some(FileCommit {
                sha: f[0].trim().into(),
                parents: f[1].split_whitespace().map(String::from).collect(),
                author: f[2].into(),
                email: f[3].into(),
                time: f[4].parse().unwrap_or(0),
                refs: f[5].split(", ").filter(|r| !r.is_empty()).map(String::from).collect(),
                subject: f[6].into(),
                file,
            })
        })
        .collect()
}

/// One line of a file together with the commit that last changed it.
#[derive(Serialize, Debug)]
pub struct BlameLine {
    pub sha: String,
    pub author: String,
    pub time: i64,
    pub summary: String,
    pub line: u32,
    pub text: String,
}

pub fn parse_blame(raw: &str) -> Vec<BlameLine> {
    let mut out = Vec::new();
    let (mut sha, mut author, mut summary) = (String::new(), String::new(), String::new());
    let (mut time, mut line) = (0i64, 0u32);
    for l in raw.lines() {
        if let Some(text) = l.strip_prefix('\t') {
            out.push(BlameLine {
                sha: sha.clone(),
                author: author.clone(),
                time,
                summary: summary.clone(),
                line,
                text: text.to_string(),
            });
        } else if let Some(v) = l.strip_prefix("author ") {
            author = v.into();
        } else if let Some(v) = l.strip_prefix("author-time ") {
            time = v.parse().unwrap_or(0);
        } else if let Some(v) = l.strip_prefix("summary ") {
            summary = v.into();
        } else {
            // "<40-hex sha> <original line> <final line> [<group size>]" starts each line's block
            let mut it = l.split(' ');
            if let (Some(h), Some(_orig), Some(fin)) = (it.next(), it.next(), it.next()) {
                if h.len() == 40 && h.chars().all(|c| c.is_ascii_hexdigit()) {
                    sha = h.into();
                    line = fin.parse().unwrap_or(0);
                }
            }
        }
    }
    out
}
