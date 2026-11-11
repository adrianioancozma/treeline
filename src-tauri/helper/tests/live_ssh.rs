//! End to end over real ssh, against a real computer. Not run by default; run it with:
//! TREELINE_LIVE_HOST=linux-box.local TREELINE_LIVE_HELPER=/path/to/treeline-helper-linux-x86_64 \
//! TREELINE_LIVE_REPO=/home/me/repo cargo test -p treeline-helper --test live_ssh -- --ignored --nocapture

use serde_json::json;
use std::time::{Duration, Instant};
use treeline_core::remote::{Connection, Upload, ssh_helper_command};

#[test]
#[ignore]
fn opens_a_repository_on_another_computer() {
    let host = std::env::var("TREELINE_LIVE_HOST").expect("TREELINE_LIVE_HOST");
    let helper = std::fs::read(std::env::var("TREELINE_LIVE_HELPER").expect("TREELINE_LIVE_HELPER")).unwrap();
    let repo = std::env::var("TREELINE_LIVE_REPO").expect("TREELINE_LIVE_REPO");
    let t = Instant::now();
    let pick = |computer: &str| { println!("computer: {computer}"); Ok(helper.clone()) };
    let conn = Connection::start(ssh_helper_command(&host).unwrap(), Upload::ForComputer(&pick), Duration::from_secs(20), |_| {}).unwrap();
    let connect = t.elapsed();
    assert_eq!(conn.hello["treeline-helper"]["self_deleted"], json!(true), "{}", conn.hello);
    conn.call("init", json!({ "trusted": [repo] }), Duration::from_secs(10)).unwrap();
    let t = Instant::now();
    let status = conn.call("repo_status", json!({ "path": repo }), Duration::from_secs(30)).unwrap();
    let log = conn.call("git_log", json!({ "path": repo, "skip": 0, "limit": 400, "all": true }), Duration::from_secs(30)).unwrap();
    let reads = t.elapsed();
    println!(
        "connect {connect:?} · status + log400 {reads:?} · branch {} · {} commits · hello {}",
        status["branch"], log.as_array().unwrap().len(), conn.hello
    );
    assert_eq!(log.as_array().unwrap().len(), 400);
    assert!(conn.call("repo_status", json!({ "path": "/etc" }), Duration::from_secs(10)).unwrap_err().to_string().contains("has not been opened"));
}
