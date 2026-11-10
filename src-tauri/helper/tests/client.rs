//! The app's side of the connection (treeline_core::remote) against the real helper and against stand-ins written in sh.

use serde_json::{Value, json};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use treeline_core::remote::Connection;

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
    let conn = Arc::new(Connection::start(Command::new(HELPER), None, SEC, |_| {}).unwrap());
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
    let err = conn.call("repo_status", json!({ "path": "/" }), SEC).unwrap_err();
    assert!(err.contains("has not been opened"), "{err}");
    conn.close();
    assert!(conn.call("ping", json!({}), SEC).is_err(), "no calls after close");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn skips_what_a_login_shell_prints_first() {
    let script = format!(r#"echo 'Welcome to linux-box'; echo '{{"id":1,"result":"fake"}}'; exec '{HELPER}'"#);
    let conn = Connection::start(sh(&script), None, SEC, |_| {}).unwrap();
    assert_eq!(conn.noise, vec!["Welcome to linux-box".to_string(), r#"{"id":1,"result":"fake"}"#.to_string()]);
    conn.call("init", json!({ "trusted": [] }), SEC).unwrap();
    assert_eq!(conn.call("ping", json!({}), SEC).unwrap(), json!({}));
}

#[cfg(unix)]
#[test]
fn sends_the_helper_along_and_it_leaves_nothing_behind() {
    let bytes = std::fs::read(HELPER).unwrap();
    let dir = std::env::temp_dir().join(format!("treeline-client-upload-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // What the app runs on the other computer, with a private folder instead of /tmp.
    let script = format!(
        r#"f=$(mktemp '{}/treeline-helper.XXXXXX') && head -c {} > "$f" && chmod 700 "$f" && exec "$f" --stdio --self-delete"#,
        dir.display(),
        bytes.len()
    );
    let conn = Connection::start(sh(&script), Some(&bytes), SEC, |_| {}).unwrap();
    assert_eq!(conn.hello["treeline-helper"]["self_deleted"], json!(true));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "the sent helper deleted its file");
    conn.call("init", json!({ "trusted": [] }), SEC).unwrap();
    assert_eq!(conn.call("ping", json!({}), SEC).unwrap(), json!({}));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_request_in_flight_fails_at_once_when_the_other_side_goes_away() {
    // Says hello, reads one request and exits without answering.
    let conn = Connection::start(sh(r#"echo '{"treeline-helper":{"proto":1}}'; read line; exit 0"#), None, SEC, |_| {}).unwrap();
    let started = Instant::now();
    let err = conn.call("repo_status", json!({ "path": "/x" }), Duration::from_secs(30)).unwrap_err();
    assert!(err.contains("closed"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(5), "did not wait for the timeout");
    assert!(!conn.is_alive());
}

#[test]
fn an_ssh_failure_before_hello_becomes_its_message() {
    let started = Instant::now();
    let err = Connection::start(
        sh("echo 'ssh: connect to host linux-box.local port 22: Connection refused' >&2; exit 255"),
        None,
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
        None,
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
