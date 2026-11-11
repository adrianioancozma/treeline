//! The app's side of the connection (treeline_core::remote) against the real helper and against stand-ins written in sh.

use serde_json::{Value, json};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use treeline_core::remote::{CallError, Connection, Upload, launch_script};

const HELPER: &str = env!("CARGO_BIN_EXE_treeline-helper");
const SEC: Duration = Duration::from_secs(10);

fn sh(script: &str) -> Command {
    let mut c = Command::new("/bin/sh");
    c.args(["-c", script]);
    c
}

fn temp_repo(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("treeline-client-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let d = std::fs::canonicalize(&d).unwrap();
    let git = |args: &[&str]| {
        assert!(Command::new("git").args(["-c", "user.name=T", "-c", "user.email=t@example.com", "-C"]).arg(&d).args(args).status().unwrap().success());
    };
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(d.join("a.txt"), "one\n").unwrap();
    git(&["add", "a.txt"]);
    git(&["commit", "-q", "-m", "first"]);
    d.to_string_lossy().to_string()
}

#[test]
fn talks_to_the_helper_from_several_threads() {
    let repo = temp_repo("threads");
    let conn = Arc::new(Connection::start(Command::new(HELPER), Upload::Nothing, SEC, |_| {}).unwrap());
    assert_eq!(conn.hello["treeline-helper"]["proto"], json!(1));
    assert_eq!(conn.call("init", json!({ "trusted": [repo] }), SEC).unwrap()["trusted"], json!(1));
    let threads: Vec<_> = (0..8)
        .map(|i| {
            let conn = Arc::clone(&conn);
            let repo = repo.clone();
            std::thread::spawn(move || {
                let method = ["repo_status", "list_refs", "git_log"][i % 3];
                let params = json!({ "path": repo, "skip": 0, "limit": 10, "all": true });
                conn.call(method, params, SEC).unwrap()
            })
        })
        .collect();
    for t in threads {
        assert!(!t.join().unwrap().is_null());
    }
    let err = conn.call("repo_status", json!({ "path": "/" }), SEC).unwrap_err().to_string();
    assert!(err.contains("has not been opened"), "{err}");
    conn.close();
    assert!(conn.call("ping", json!({}), SEC).is_err(), "no calls after close");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn skips_what_a_login_shell_prints_first() {
    let script = format!(r#"echo 'Welcome to linux-box'; echo '{{"id":1,"result":"fake"}}'; exec '{HELPER}'"#);
    let conn = Connection::start(sh(&script), Upload::Nothing, SEC, |_| {}).unwrap();
    assert_eq!(conn.noise, vec!["Welcome to linux-box".to_string(), r#"{"id":1,"result":"fake"}"#.to_string()]);
    conn.call("init", json!({ "trusted": [] }), SEC).unwrap();
    assert_eq!(conn.call("ping", json!({}), SEC).unwrap(), json!({}));
}

fn private_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("treeline-client-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(unix)]
#[test]
fn sends_the_helper_along_and_it_leaves_nothing_behind() {
    let bytes = std::fs::read(HELPER).unwrap();
    let dir = private_dir("upload");
    // The script the app runs on the other computer, with a private folder instead of /tmp.
    let script = launch_script(&[&dir.to_string_lossy()]);
    let pick = |_: &str| Ok(bytes.clone());
    let conn = Connection::start(sh(&script), Upload::ForComputer(&pick), SEC, |_| {}).unwrap();
    assert_eq!(conn.hello["treeline-helper"]["self_deleted"], json!(true));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "the sent helper deleted its file");
    conn.call("init", json!({ "trusted": [] }), SEC).unwrap();
    assert_eq!(conn.call("ping", json!({}), SEC).unwrap(), json!({}));
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn uses_the_next_folder_when_the_first_does_not_work() {
    use std::os::unix::fs::PermissionsExt;
    let bytes = std::fs::read(HELPER).unwrap();
    let blocked = private_dir("blocked");
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o500)).unwrap(); // nothing can be created there
    let fallback = private_dir("fallback");
    let script = launch_script(&[&blocked.to_string_lossy(), &fallback.to_string_lossy()]);
    let pick = |_: &str| Ok(bytes.clone());
    let conn = Connection::start(sh(&script), Upload::ForComputer(&pick), SEC, |_| {}).unwrap();
    assert_eq!(conn.hello["treeline-helper"]["self_deleted"], json!(true));
    assert_eq!(std::fs::read_dir(&fallback).unwrap().count(), 0, "ran from the fallback folder and left nothing");
    let mode = std::fs::metadata(&blocked).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o500, "an existing folder's permissions are left alone, so the helper could only run from the fallback");
    conn.call("init", json!({ "trusted": [] }), SEC).unwrap();
    assert_eq!(conn.call("ping", json!({}), SEC).unwrap(), json!({}));
    drop(conn);
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o700)).unwrap();
    let _ = std::fs::remove_dir_all(&blocked);
    let _ = std::fs::remove_dir_all(&fallback);
}

#[cfg(unix)]
#[test]
fn says_clearly_when_no_folder_allows_running_the_helper() {
    use std::os::unix::fs::PermissionsExt;
    let bytes = std::fs::read(HELPER).unwrap();
    let a = private_dir("none-a");
    let b = private_dir("none-b");
    for d in [&a, &b] {
        std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o500)).unwrap();
    }
    let script = launch_script(&[&a.to_string_lossy(), &b.to_string_lossy()]);
    let pick = |_: &str| Ok(bytes.clone());
    let err = Connection::start(sh(&script), Upload::ForComputer(&pick), SEC, |_| {}).err().expect("no connection");
    assert!(err.contains("no folder on this computer allows running the helper"), "{err}");
    for d in [&a, &b] {
        std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700)).unwrap();
        let _ = std::fs::remove_dir_all(d);
    }
}

#[test]
fn a_request_in_flight_fails_at_once_when_the_other_side_goes_away() {
    // Says hello, reads one request and exits without answering.
    let conn = Connection::start(sh(r#"echo '{"treeline-helper":{"proto":1}}'; read line; exit 0"#), Upload::Nothing, SEC, |_| {}).unwrap();
    let started = Instant::now();
    let err = conn.call("repo_status", json!({ "path": "/x" }), Duration::from_secs(30)).unwrap_err().to_string();
    assert!(err.contains("closed"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(5), "did not wait for the timeout");
    assert!(!conn.is_alive());
}

#[test]
fn an_ssh_failure_before_hello_becomes_its_message() {
    let started = Instant::now();
    let err = Connection::start(
        sh("echo 'ssh: connect to host linux-box.local port 22: Connection refused' >&2; exit 255"),
        Upload::Nothing,
        SEC,
        |_| {},
    )
    .err()
    .expect("no connection");
    assert!(err.contains("Connection refused"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(5), "did not wait for the timeout");
}

#[test]
fn notifications_go_to_the_listener() {
    let heard = Arc::new(Mutex::new(Vec::<Value>::new()));
    let sink = Arc::clone(&heard);
    let conn = Connection::start(
        sh(r#"echo '{"treeline-helper":{"proto":1}}'; echo '{"method":"trusted","params":{"key":"/home/a/sub"}}'; cat > /dev/null"#),
        Upload::Nothing,
        SEC,
        move |v| sink.lock().unwrap().push(v.clone()),
    )
    .unwrap();
    for _ in 0..50 {
        if !heard.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(heard.lock().unwrap()[0]["params"]["key"], json!("/home/a/sub"));
    conn.close();
}

#[test]
fn an_ssh_failure_while_sending_the_helper_becomes_its_message() {
    // ssh gives up (e.g. a refused key) before reading what the app sends.
    let bytes = vec![0u8; 4 * 1024 * 1024];
    let pick = |_: &str| Ok(bytes.clone());
    let err = Connection::start(
        sh("echo 'adrian@linux-box.local: Permission denied (publickey).' >&2; exit 255"),
        Upload::ForComputer(&pick),
        SEC,
        |_| {},
    )
    .err()
    .expect("no connection");
    assert!(err.contains("Permission denied (publickey)"), "{err}");
}

#[cfg(unix)]
#[test]
fn sends_the_build_for_the_computer_and_refuses_an_unknown_one() {
    let bytes = std::fs::read(HELPER).unwrap();
    let dir = private_dir("arch");
    let script = launch_script(&[&dir.to_string_lossy()]);
    // A login banner first, then the script's own line.
    let seen = Arc::new(Mutex::new(String::new()));
    let seen2 = Arc::clone(&seen);
    let pick = move |computer: &str| {
        *seen2.lock().unwrap() = computer.to_string();
        Ok(bytes.clone())
    };
    let conn = Connection::start(sh(&format!("echo 'Last login: today'; {script}")), Upload::ForComputer(&pick), SEC, |_| {}).unwrap();
    let computer = seen.lock().unwrap().clone();
    let expected = format!(
        "{} {}",
        String::from_utf8(Command::new("uname").arg("-s").output().unwrap().stdout).unwrap().trim(),
        String::from_utf8(Command::new("uname").arg("-m").output().unwrap().stdout).unwrap().trim()
    );
    assert_eq!(computer, expected, "the script said which computer it runs on");
    assert_eq!(conn.noise, vec!["Last login: today".to_string()]);
    drop(conn);

    let refuse = |computer: &str| Err(format!("Treeline has no helper for {computer} yet."));
    let started = Instant::now();
    let err = Connection::start(sh(&script), Upload::ForComputer(&refuse), SEC, |_| {}).err().expect("refused");
    assert!(err.starts_with("Treeline has no helper for "), "{err}");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "nothing was written for an unknown computer");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn close_by_treeline_is_not_a_lost_connection() {
    // Answers nothing; `exec` so that killing the shell ends the process holding the output.
    let script = r#"echo '{"treeline-helper":{"proto":1}}'; read line; exec sleep 30"#;
    let conn = Arc::new(Connection::start(sh(script), Upload::Nothing, SEC, |_| {}).unwrap());
    let caller = {
        let conn = Arc::clone(&conn);
        std::thread::spawn(move || conn.call("repo_status", json!({ "path": "/x" }), SEC).unwrap_err().to_string())
    };
    std::thread::sleep(Duration::from_millis(300));
    conn.close();
    let in_flight = caller.join().unwrap();
    assert!(in_flight.contains("Treeline closed the connection"), "{in_flight}");
    let after = conn.call("ping", json!({}), SEC).unwrap_err().to_string();
    assert!(after.contains("Treeline closed the connection"), "{after}");
}

#[test]
fn each_failure_has_its_kind() {
    // Answers the first request with an error, ignores the second, exits after the third line.
    let script = r#"echo '{"treeline-helper":{"proto":1}}'; read a; echo '{"id":1,"error":"fatal: bad revision"}'; read b; read c; exit 0"#;
    let conn = Connection::start(sh(script), Upload::Nothing, SEC, |_| {}).unwrap();
    assert_eq!(conn.call("git_log", json!({}), SEC), Err(CallError::Remote("fatal: bad revision".into())));
    assert_eq!(conn.call("git_log", json!({}), Duration::from_millis(300)), Err(CallError::Timeout));
    assert_eq!(conn.call("git_log", json!({}), SEC), Err(CallError::Closed), "the other side went away on its own");
    assert!(CallError::Closed.to_string().contains("was closed"));
}

#[test]
fn says_when_the_process_ends_whoever_ends_it() {
    let ended = |script: &str, close: bool| {
        let conn = Connection::start(sh(script), Upload::Nothing, SEC, |_| {}).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        conn.on_close(move || tx.send(()).unwrap());
        if close {
            conn.close();
        }
        rx.recv_timeout(Duration::from_secs(5)).is_ok()
    };
    assert!(ended(r#"echo '{"treeline-helper":{"proto":1}}'; exit 0"#, false), "the other side exits");
    assert!(ended(r#"echo '{"treeline-helper":{"proto":1}}'; exec sleep 30"#, true), "Treeline closes it");
    // Registered after the end: runs at once.
    let conn = Connection::start(sh(r#"echo '{"treeline-helper":{"proto":1}}'; exit 0"#), Upload::Nothing, SEC, |_| {}).unwrap();
    while conn.is_alive() {
        std::thread::sleep(Duration::from_millis(10));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    conn.on_close(move || tx.send(()).unwrap());
    assert!(rx.try_recv().is_ok());
}

#[test]
fn a_side_that_stops_reading_does_not_hang_the_start() {
    // Reads the length, then stops reading: a helper bigger than the pipe's buffer can never be written in full.
    let pidfile = std::env::temp_dir().join(format!("treeline-client-stalled-{}", std::process::id()));
    let script = format!(r#"echo $$ > '{}'; echo "treeline-arch Linux x86_64"; read n; exec sleep 30"#, pidfile.display());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let pick = |_: &str| Ok(vec![0u8; 4 << 20]);
        let _ = tx.send(Connection::start(sh(&script), Upload::ForComputer(&pick), Duration::from_secs(1), |_| {}).map(|_| ()));
    });
    let outcome = rx.recv_timeout(Duration::from_secs(5));
    let pid = std::fs::read_to_string(&pidfile).unwrap_or_default();
    let _ = std::fs::remove_file(&pidfile);
    let still_running = !pid.trim().is_empty() && Command::new("kill").args(["-0", pid.trim()]).status().unwrap().success();
    if still_running {
        let _ = Command::new("kill").args(["-9", pid.trim()]).status(); // do not leave it behind when the test fails
    }
    assert!(outcome.is_ok(), "the start was still waiting to send the helper after 5 s");
    assert!(outcome.unwrap().is_err());
    assert!(!still_running, "the process was left running");
}
