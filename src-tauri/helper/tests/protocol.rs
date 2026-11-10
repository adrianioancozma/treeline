//! Runs the real helper binary as a child process and talks to it the way the app will (over its stdin/stdout).

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

struct Helper {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Value>,
    hello: Value,
}

impl Helper {
    fn start_with(exe: &Path, args: &[&str]) -> Helper {
        // A process another test starts at the same moment can hold a just-written executable open for a moment
        // ("Text file busy" on Linux); trying again shortly after is the usual way around it.
        let mut tries = 0;
        let mut child = loop {
            match Command::new(exe).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn() {
                Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 50 => {
                    tries += 1;
                    std::thread::sleep(Duration::from_millis(20));
                }
                other => break other.unwrap(),
            }
        };
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = tx.send(serde_json::from_str::<Value>(&line).unwrap());
            }
        });
        let hello = lines.recv_timeout(Duration::from_secs(10)).expect("hello");
        Helper { child, stdin, lines, hello }
    }
    fn start() -> Helper {
        Helper::start_with(Path::new(env!("CARGO_BIN_EXE_treeline-helper")), &["--stdio"])
    }
    fn send(&mut self, msg: Value) {
        let s = self.stdin.as_mut().unwrap();
        writeln!(s, "{msg}").unwrap();
        s.flush().unwrap();
    }
    fn next(&self) -> Value {
        self.lines.recv_timeout(Duration::from_secs(20)).expect("an answer")
    }
    fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({ "id": id, "method": method, "params": params }));
        let v = self.next();
        assert_eq!(v["id"], json!(id), "answer to {method}: {v}");
        v
    }
}

/// Tests that write an executable and then run it take turns: on Linux a process started meanwhile by another test can
/// briefly hold the file open for writing, and running it then fails with "Text file busy".
fn exe_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("treeline-helper-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::canonicalize(&d).unwrap()
}

fn git(repo: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@example.com", "-C"])
        .arg(repo)
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// A repository with two commits, a branch, a staged and an untracked file.
fn sample_repo(name: &str) -> PathBuf {
    let r = temp(name);
    git(&r, &["init", "-q", "-b", "main"]);
    std::fs::write(r.join("a.txt"), "one\n").unwrap();
    git(&r, &["add", "a.txt"]);
    git(&r, &["commit", "-q", "-m", "first"]);
    std::fs::write(r.join("a.txt"), "one\ntwo\n").unwrap();
    git(&r, &["commit", "-q", "-am", "second"]);
    git(&r, &["branch", "feature"]);
    std::fs::write(r.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    git(&r, &["add", "a.txt"]);
    std::fs::write(r.join("new.txt"), "x\n").unwrap();
    r
}

#[test]
fn says_hello_first_and_answers_nothing_before_init() {
    let mut h = Helper::start();
    assert_eq!(h.hello["treeline-helper"]["proto"], json!(1));
    assert!(h.hello["treeline-helper"]["git"].as_str().unwrap_or("").starts_with("git version"));
    let v = h.call(1, "repo_status", json!({ "path": "/" }));
    assert_eq!(v["error"], json!("init first"));
}

#[test]
fn answers_like_the_library_for_opened_folders_only() {
    let repo = sample_repo("same");
    let other = sample_repo("other");
    let p = repo.to_string_lossy().to_string();
    let mut h = Helper::start();
    assert_eq!(h.call(1, "init", json!({ "trusted": [p] }))["result"]["trusted"], json!(1));

    // The same functions called in this process (after trusting the folder here too) give the reference answers.
    treeline_core::trust::allow_repo(&p);
    let status = serde_json::to_value(treeline_core::read::repo_status(p.clone()).unwrap()).unwrap();
    let log = serde_json::to_value(treeline_core::read::git_log(p.clone(), 0, 400, true, None, None).unwrap()).unwrap();
    let refs = serde_json::to_value(treeline_core::read::list_refs(p.clone()).unwrap()).unwrap();
    let diff = treeline_core::read::file_diff(p.clone(), "a.txt".into(), true, false, 3).unwrap();
    assert_eq!(h.call(2, "repo_status", json!({ "path": p }))["result"], status);
    assert_eq!(h.call(3, "git_log", json!({ "path": p, "skip": 0, "limit": 400, "all": true }))["result"], log);
    assert_eq!(h.call(4, "list_refs", json!({ "path": p }))["result"], refs);
    assert_eq!(
        h.call(5, "file_diff", json!({ "path": p, "file": "a.txt", "staged": true, "untracked": false, "context": 3 }))["result"],
        json!(diff)
    );
    assert!(status["files"].as_array().unwrap().len() >= 2, "the sample has a staged and an untracked file: {status}");

    // A folder the user did not open for this computer is refused, like in the app.
    let refused = h.call(6, "repo_status", json!({ "path": other.to_string_lossy() }));
    assert!(refused["error"].as_str().unwrap().contains("has not been opened"), "{refused}");
    // Unknown methods and bad parameters are errors, not crashes.
    assert!(h.call(7, "rm_rf", json!({}))["error"].as_str().unwrap().starts_with("unknown method"));
    assert!(h.call(8, "git_log", json!({ "path": p }))["error"].as_str().unwrap().starts_with("bad parameters"));
    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_dir_all(&other);
}

#[test]
fn answers_file_history_blame_and_history_filters_like_the_library() {
    let repo = sample_repo("file-history");
    let p = repo.to_string_lossy().to_string();
    let mut h = Helper::start();
    h.call(1, "init", json!({ "trusted": [p] }));
    treeline_core::trust::allow_repo(&p);
    let log = treeline_core::read::file_log(p.clone(), "a.txt".into(), 0, 50).unwrap();
    let blame = treeline_core::read::git_blame(p.clone(), "a.txt".into(), None).unwrap();
    let filter = treeline_core::read::LogFilter { text: Some("second".into()), ..Default::default() };
    let matches = treeline_core::read::git_log_matches(p.clone(), true, filter).unwrap();
    assert_eq!(log.len(), 2, "two commits touched a.txt");
    assert_eq!(matches.len(), 1, "one commit says “second”");
    assert_eq!(h.call(2, "file_log", json!({ "path": p, "file": "a.txt", "skip": 0, "limit": 50 }))["result"], json!(log));
    assert_eq!(h.call(3, "git_blame", json!({ "path": p, "file": "a.txt", "rev": null }))["result"], json!(blame));
    let found = h.call(4, "git_log_matches", json!({ "path": p, "all": true, "filter": { "text": "second" } }));
    assert_eq!(found["result"], json!(matches));
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn answers_requests_sent_together() {
    let repo = sample_repo("together");
    let p = repo.to_string_lossy().to_string();
    let mut h = Helper::start();
    h.call(1, "init", json!({ "trusted": [p] }));
    let methods = ["repo_status", "list_submodules", "list_refs", "stash_list", "list_remotes"];
    for i in 0..20u64 {
        h.send(json!({ "id": 100 + i, "method": methods[i as usize % methods.len()], "params": { "path": p } }));
    }
    let mut ids: Vec<u64> = (0..20).map(|_| h.next()).map(|v| {
        assert!(v.get("result").is_some(), "{v}");
        v["id"].as_u64().unwrap()
    }).collect();
    ids.sort();
    assert_eq!(ids, (100..120).collect::<Vec<_>>());
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn exits_when_the_connection_closes() {
    let mut h = Helper::start();
    drop(h.stdin.take()); // what ssh does when the app goes away
    for _ in 0..50 {
        if h.child.try_wait().unwrap().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = h.child.kill();
    panic!("the helper kept running after its input closed");
}

#[cfg(unix)]
#[test]
fn deletes_its_own_file_and_keeps_working() {
    let _turn = exe_lock();
    let dir = temp("selfdelete");
    let copy = dir.join("treeline-helper");
    std::fs::copy(env!("CARGO_BIN_EXE_treeline-helper"), &copy).unwrap();
    let mut h = Helper::start_with(&copy, &["--stdio", "--self-delete"]);
    assert_eq!(h.hello["treeline-helper"]["self_deleted"], json!(true));
    assert!(!copy.exists(), "the helper's file is gone while it runs");
    h.call(1, "init", json!({ "trusted": [] }));
    assert_eq!(h.call(2, "ping", json!({}))["result"], json!({}));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_folder_allowed_later_is_answered_and_reported_back() {
    let repo = sample_repo("allow");
    let p = repo.to_string_lossy().to_string();
    let mut h = Helper::start();
    h.call(1, "init", json!({ "trusted": [] }));
    assert!(h.call(2, "repo_status", json!({ "path": p }))["error"].as_str().unwrap().contains("has not been opened"));
    h.send(json!({ "id": 3, "method": "allow", "params": { "path": p } }));
    // The notification and the answer may come in either order.
    let (a, b) = (h.next(), h.next());
    let (note, answer) = if a.get("id").is_some() { (b, a) } else { (a, b) };
    assert_eq!(note["method"], json!("trusted"));
    assert_eq!(answer["result"]["key"], note["params"]["key"]);
    assert!(h.call(4, "repo_status", json!({ "path": p }))["result"].is_object());
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn checks_a_folder_before_it_is_opened() {
    let repo = sample_repo("access");
    let p = repo.to_string_lossy().to_string();
    let mut h = Helper::start();
    h.call(1, "init", json!({ "trusted": [] }));
    let a = h.call(2, "access", json!({ "path": p }))["result"].clone();
    assert_eq!((a["exists"].clone(), a["repo"].clone(), a["readable"].clone()), (json!(true), json!(true), json!(true)), "{a}");
    assert!(a["problem"].is_null());
    let missing = h.call(3, "access", json!({ "path": format!("{p}/nope") }))["result"].clone();
    assert!(missing["problem"].as_str().unwrap().contains("does not exist"));
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn lists_folders_starting_from_home_on_that_computer() {
    let _turn = exe_lock();
    let home = temp("home");
    for sub in ["dev/project/.git", "dev/notes", ".config"] {
        std::fs::create_dir_all(home.join(sub)).unwrap();
    }
    let exe = std::env::temp_dir().join(format!("treeline-helper-launcher-{}", std::process::id()));
    let _ = std::fs::remove_file(&exe);
    // A launcher that starts the helper with HOME pointing at the test folder.
    let script = format!("HOME='{}' exec '{}' --stdio", home.display(), env!("CARGO_BIN_EXE_treeline-helper"));
    std::fs::write(&exe, format!("#!/bin/sh\n{script}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut h = Helper::start_with(&exe, &[]);
    h.call(1, "init", json!({ "trusted": [] }));
    let l = h.call(2, "list_dir", json!({ "path": "~" }))["result"].clone();
    assert_eq!(l["path"], json!(home.to_string_lossy()), "{l}");
    assert_eq!(l["entries"], json!([{ "name": "dev", "repo": false }]), "hidden folders left out");
    let dev = h.call(3, "list_dir", json!({ "path": "~/dev" }))["result"].clone();
    assert_eq!(dev["entries"], json!([{ "name": "notes", "repo": false }, { "name": "project", "repo": true }]));
    let hidden = h.call(4, "list_dir", json!({ "path": "~", "show_hidden": true }))["result"].clone();
    assert!(hidden["entries"].as_array().unwrap().iter().any(|e| e["name"] == ".config"));
    assert!(h.call(5, "list_dir", json!({ "path": "~/nope" }))["error"].as_str().unwrap().contains("does not exist"));
    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_dir_all(&home);
}
