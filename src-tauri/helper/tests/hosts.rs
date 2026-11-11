//! treeline_core::hosts: one connection per computer, against the real helper and stand-ins written in sh.

use serde_json::{Value, json};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, Instant};
use treeline_core::hosts::{Ask, Config, Hosts, State, describe, start_failed};
use treeline_core::remote::{CallError, Connection, Upload};

const HELPER: &str = env!("CARGO_BIN_EXE_treeline-helper");
const SEC: Duration = Duration::from_secs(10);

fn sh(script: &str) -> Command {
    let mut c = Command::new("/bin/sh");
    c.args(["-c", script]);
    c
}

fn helper() -> Result<Connection, String> {
    let c = Connection::start(Command::new(HELPER), Upload::Nothing, SEC, |_| {})?;
    c.call("init", json!({ "trusted": [] }), SEC)?;
    Ok(c)
}

/// A `Hosts` whose n-th start (from 1) is made by `make(n)`, after `delay`; returns it and the start counter.
fn hosts(delay: Duration, make: impl Fn(usize) -> Result<Connection, String> + Send + Sync + 'static) -> (Arc<Hosts>, Arc<AtomicUsize>) {
    let starts = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&starts);
    let h = Hosts::new(move |_host| {
        let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
        std::thread::sleep(delay);
        make(n)
    });
    (Arc::new(h), starts)
}

/// `n` calls to `host` at the same moment; their results in order.
fn together(h: &Arc<Hosts>, host: &str, n: usize, method: &str, ask: Ask) -> Vec<Result<Value, String>> {
    let gate = Arc::new(Barrier::new(n));
    let threads: Vec<_> = (0..n)
        .map(|_| {
            let (h, gate, host, method) = (Arc::clone(h), Arc::clone(&gate), host.to_string(), method.to_string());
            std::thread::spawn(move || {
                gate.wait();
                h.call(&host, &method, json!({}), SEC, ask)
            })
        })
        .collect();
    threads.into_iter().map(|t| t.join().unwrap()).collect()
}

/// A connection that takes `n` requests, answers none, then ends (the link dropping under calls in flight).
fn drops_after(n: usize) -> Result<Connection, String> {
    let reads = "read l; ".repeat(n);
    Connection::start(sh(&format!(r#"echo '{{"treeline-helper":{{"proto":1}}}}'; {reads}exit 0"#)), Upload::Nothing, SEC, |_| {})
}

#[test]
fn simultaneous_calls_share_one_start() {
    let (h, starts) = hosts(Duration::from_millis(200), |_| helper());
    for r in together(&h, "linux-box", 8, "ping", Ask::READ) {
        assert_eq!(r.unwrap(), json!({}));
    }
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    h.call("linux-box", "ping", json!({}), SEC, Ask::READ).unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 1, "a live connection is reused");
}

#[test]
fn a_dropped_link_costs_one_new_connection_and_no_call() {
    // The first connection ends once all 8 requests reached it; every later one is the real helper.
    let (h, starts) = hosts(Duration::from_millis(100), |n| if n == 1 { drops_after(8) } else { helper() });
    for r in together(&h, "linux-box", 8, "ping", Ask::READ) {
        assert_eq!(r.unwrap(), json!({}), "each call cut off is sent once more");
    }
    assert_eq!(starts.load(Ordering::SeqCst), 2, "the 8 retries share one new connection");
}

#[test]
fn calls_that_may_change_something_are_not_sent_twice() {
    let (h, starts) = hosts(Duration::ZERO, |n| if n == 1 { drops_after(1) } else { helper() });
    let err = h.call("linux-box", "ping", json!({}), SEC, Ask::WRITE).unwrap_err();
    assert_eq!(err, "Treeline lost the connection to “linux-box”.");
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[test]
fn a_retry_that_drops_too_is_reported_not_repeated() {
    let (h, starts) = hosts(Duration::ZERO, |_| drops_after(1));
    let err = h.call("linux-box", "ping", json!({}), SEC, Ask::READ).unwrap_err();
    assert_eq!(err, "Treeline lost the connection to “linux-box”.");
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}

#[test]
fn a_failed_start_reaches_every_waiting_call() {
    let (h, starts) = hosts(Duration::from_millis(200), |_| Err("ssh: connect to host linux-box port 22: No route to host".into()));
    for r in together(&h, "linux-box", 5, "ping", Ask::READ) {
        assert_eq!(r.unwrap_err(), "Treeline could not connect to “linux-box”: ssh: connect to host linux-box port 22: No route to host");
    }
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[test]
fn a_slow_computer_does_not_hold_up_another() {
    let h = Arc::new(Hosts::new(|host| {
        if host == "slow" {
            std::thread::sleep(Duration::from_millis(1500));
        }
        helper()
    }));
    let slow = {
        let h = Arc::clone(&h);
        std::thread::spawn(move || h.call("slow", "ping", json!({}), SEC, Ask::READ))
    };
    std::thread::sleep(Duration::from_millis(100)); // the slow start is under way
    let t = Instant::now();
    h.call("fast", "ping", json!({}), SEC, Ask::READ).unwrap();
    assert!(t.elapsed() < Duration::from_millis(800), "took {:?}", t.elapsed());
    slow.join().unwrap().unwrap();
}

#[test]
fn a_reaction_to_an_old_connection_leaves_the_new_one_alone() {
    let (h, starts) = hosts(Duration::ZERO, |_| helper());
    let (g1, _) = h.get("linux-box", Ask::READ).unwrap();
    h.invalidate("linux-box", g1);
    let (g2, _) = h.get("linux-box", Ask::READ).unwrap();
    assert_eq!((g2, starts.load(Ordering::SeqCst)), (g1 + 1, 2));
    h.invalidate("linux-box", g1); // late, about the first one
    assert_eq!(h.get("linux-box", Ask::READ).unwrap().0, g2);
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}

#[test]
fn the_texts_are_the_ones_the_page_knows() {
    let golden: Value = serde_json::from_str(include_str!("../../core/remote-errors.json")).unwrap();
    let host = golden["host"].as_str().unwrap();
    assert_eq!(start_failed(host, "ssh: connect to host linux-box.local port 22: No route to host"), golden["start"]);
    assert_eq!(describe(host, CallError::Closed), golden["lost"]);
    assert_eq!(describe(host, CallError::Cancelled), golden["cancelled"]);
    assert_eq!(describe(host, CallError::Timeout), golden["timeout"]);
    assert_eq!(describe(host, CallError::Remote("fatal: bad revision".into())), "fatal: bad revision", "git's own words pass through");
}

const FAST: Config = Config { hold: Duration::from_millis(400), ping: Duration::from_millis(300) };
const NO_ROUTE: &str = "ssh: connect to host linux-box port 22: No route to host";

/// Like `hosts`, with `FAST` timings and every state change recorded.
#[allow(clippy::type_complexity)]
fn watched(delay: Duration, make: impl Fn(usize) -> Result<Connection, String> + Send + Sync + 'static) -> (Arc<Hosts>, Arc<AtomicUsize>, Arc<Mutex<Vec<(u64, State)>>>) {
    let starts = Arc::new(AtomicUsize::new(0));
    let seen: Arc<Mutex<Vec<(u64, State)>>> = Arc::default();
    let (counter, log) = (Arc::clone(&starts), Arc::clone(&seen));
    let h = Hosts::with(
        move |_host| {
            let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
            std::thread::sleep(delay);
            make(n)
        },
        FAST,
        move |_host, g, state| log.lock().unwrap().push((g, state)),
    );
    (Arc::new(h), starts, seen)
}

/// Takes requests and never answers them (a connection that stopped responding).
fn silent() -> Result<Connection, String> {
    Connection::start(sh(r#"echo '{"treeline-helper":{"proto":1}}'; while read l; do :; done"#), Upload::Nothing, SEC, |_| {})
}

fn starts_of(s: &AtomicUsize) -> usize {
    s.load(Ordering::SeqCst)
}

#[test]
fn an_unreachable_computer_is_remembered_for_a_while() {
    let (h, starts, _) = watched(Duration::ZERO, |_| Err(NO_ROUTE.into()));
    let first = h.call("linux-box", "ping", json!({}), SEC, Ask::READ).unwrap_err();
    let second = h.call("linux-box", "ping", json!({}), SEC, Ask::READ).unwrap_err();
    assert_eq!(first, second);
    assert_eq!(starts_of(&starts), 1, "the second call is answered from memory, without ssh");
    std::thread::sleep(FAST.hold);
    let _ = h.call("linux-box", "ping", json!({}), SEC, Ask::READ);
    assert_eq!(starts_of(&starts), 2, "after the hold, the next call tries again");
}

#[test]
fn what_the_user_asks_for_always_tries() {
    let (h, starts, _) = watched(Duration::ZERO, |n| if n == 1 { Err(NO_ROUTE.into()) } else { helper() });
    let _ = h.call("linux-box", "ping", json!({}), SEC, Ask::READ);
    assert_eq!(h.call("linux-box", "ping", json!({}), SEC, Ask::EXPLICIT).unwrap(), json!({}));
    assert_eq!(starts_of(&starts), 2);
}

#[test]
fn reconnect_after_a_failure_tries_at_once() {
    let (h, starts, _) = watched(Duration::ZERO, |n| if n == 1 { Err(NO_ROUTE.into()) } else { helper() });
    let _ = h.call("linux-box", "ping", json!({}), SEC, Ask::READ);
    h.reconnect("linux-box").unwrap();
    assert_eq!(starts_of(&starts), 2);
    assert_eq!(h.call("linux-box", "ping", json!({}), SEC, Ask::READ).unwrap(), json!({}), "the remembered failure is gone");
}

#[test]
fn reconnect_keeps_a_connection_that_answers() {
    let (h, starts, _) = watched(Duration::ZERO, |_| helper());
    h.call("linux-box", "ping", json!({}), SEC, Ask::READ).unwrap();
    h.reconnect("linux-box").unwrap();
    assert_eq!(starts_of(&starts), 1);
}

#[test]
fn reconnect_replaces_a_connection_that_stopped_answering() {
    let (h, starts, _) = watched(Duration::ZERO, |n| if n == 1 { silent() } else { helper() });
    let (g1, _) = h.get("linux-box", Ask::READ).unwrap();
    // A read stuck on the silent connection moves to the new one.
    let stuck = {
        let h = Arc::clone(&h);
        std::thread::spawn(move || h.call("linux-box", "ping", json!({}), SEC, Ask::READ))
    };
    std::thread::sleep(Duration::from_millis(100));
    let t = Instant::now();
    h.reconnect("linux-box").unwrap();
    assert!(t.elapsed() < Duration::from_secs(2), "took {:?}", t.elapsed());
    assert_eq!(starts_of(&starts), 2);
    assert_eq!(stuck.join().unwrap().unwrap(), json!({}));
    assert_eq!(h.get("linux-box", Ask::READ).unwrap().0, g1 + 1);
}

#[test]
fn two_reconnects_at_once_replace_a_silent_connection_once() {
    let (h, starts, _) = watched(Duration::from_millis(100), |n| if n == 1 { silent() } else { helper() });
    h.get("linux-box", Ask::READ).unwrap();
    let gate = Arc::new(Barrier::new(2));
    let both: Vec<_> = (0..2)
        .map(|_| {
            let (h, gate) = (Arc::clone(&h), Arc::clone(&gate));
            std::thread::spawn(move || {
                gate.wait();
                h.reconnect("linux-box")
            })
        })
        .collect();
    for r in both {
        r.join().unwrap().unwrap();
    }
    assert_eq!(starts_of(&starts), 2);
}

#[test]
fn reconnect_while_starting_joins_the_start() {
    let (h, starts, _) = watched(Duration::from_millis(300), |_| helper());
    let first = {
        let h = Arc::clone(&h);
        std::thread::spawn(move || h.get("linux-box", Ask::READ).map(|_| ()))
    };
    std::thread::sleep(Duration::from_millis(100));
    h.reconnect("linux-box").unwrap();
    first.join().unwrap().unwrap();
    assert_eq!(starts_of(&starts), 1);
}

#[test]
fn disconnect_tells_calls_in_flight_and_starts_nothing() {
    // The connection never answers, so all 8 reads are in flight when the user closes the last tab.
    let (h, starts, _) = watched(Duration::ZERO, |n| if n == 1 { silent() } else { helper() });
    h.get("linux-box", Ask::READ).unwrap();
    let calls: Vec<_> = (0..8)
        .map(|_| {
            let h = Arc::clone(&h);
            std::thread::spawn(move || h.call("linux-box", "ping", json!({}), SEC, Ask::READ))
        })
        .collect();
    std::thread::sleep(Duration::from_millis(200));
    h.disconnect("linux-box");
    for c in calls {
        assert_eq!(c.join().unwrap().unwrap_err(), "Treeline closed the connection to “linux-box”.");
    }
    assert_eq!(starts_of(&starts), 1, "nothing reconnects a computer the user closed");
}

#[test]
fn disconnect_while_starting_keeps_nothing() {
    let marker = std::env::temp_dir().join(format!("treeline-hosts-pid-{}", std::process::id()));
    let pidfile = marker.to_string_lossy().to_string();
    let (h, starts, _) = watched(Duration::from_millis(300), move |_| {
        Connection::start(sh(&format!(r#"echo $$ > '{pidfile}'; echo '{{"treeline-helper":{{"proto":1}}}}'; while read l; do :; done"#)), Upload::Nothing, SEC, |_| {})
    });
    let waiting = {
        let h = Arc::clone(&h);
        std::thread::spawn(move || h.call("linux-box", "ping", json!({}), SEC, Ask::READ))
    };
    std::thread::sleep(Duration::from_millis(100));
    h.disconnect("linux-box");
    assert_eq!(waiting.join().unwrap().unwrap_err(), "Treeline closed the connection to “linux-box”.");
    assert!(!h.states().iter().any(|(_, _, s)| *s == State::Alive), "{:?}", h.states());
    let pid = std::fs::read_to_string(&marker).unwrap();
    let _ = std::fs::remove_file(&marker);
    std::thread::sleep(Duration::from_millis(100));
    let alive = Command::new("kill").args(["-0", pid.trim()]).status().unwrap().success();
    assert!(!alive, "the connection started meanwhile was ended");
    assert_eq!(starts_of(&starts), 1);
}

#[test]
fn disconnect_forgets_a_failure() {
    let (h, starts, _) = watched(Duration::ZERO, |_| Err(NO_ROUTE.into()));
    let _ = h.call("linux-box", "ping", json!({}), SEC, Ask::READ);
    h.disconnect("linux-box");
    let _ = h.call("linux-box", "ping", json!({}), SEC, Ask::READ);
    assert_eq!(starts_of(&starts), 2);
}

#[test]
fn a_slow_answer_leaves_the_connection_in_place() {
    let (h, starts, _) = watched(Duration::ZERO, |_| silent());
    let err = h.call("linux-box", "ping", json!({}), Duration::from_millis(200), Ask::READ).unwrap_err();
    assert_eq!(err, "Treeline got no answer from “linux-box” in time.");
    assert_eq!(h.states(), vec![("linux-box".to_string(), 1, State::Alive)]);
    assert_eq!(starts_of(&starts), 1);
}

#[test]
fn every_change_is_told_once_in_order() {
    // 1: connects, then the process ends by itself; 2: fails; 3 (after Reconnect): connects; then closed by the user.
    let (h, _, seen) = watched(Duration::ZERO, |n| match n {
        1 => Connection::start(sh(r#"echo '{"treeline-helper":{"proto":1}}'; sleep 0.3; exit 0"#), Upload::Nothing, SEC, |_| {}),
        2 => Err(NO_ROUTE.into()),
        _ => helper(),
    });
    h.get("linux-box", Ask::READ).unwrap();
    std::thread::sleep(Duration::from_millis(600)); // the link drops with no call in flight
    let _ = h.get("linux-box", Ask::READ);
    h.reconnect("linux-box").unwrap();
    h.disconnect("linux-box");
    use State::*;
    assert_eq!(
        *seen.lock().unwrap(),
        vec![(1, Connecting), (1, Alive), (1, Idle), (2, Connecting), (2, Failed), (3, Connecting), (3, Alive), (3, Idle)]
    );
}
