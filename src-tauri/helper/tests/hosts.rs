//! treeline_core::hosts: one connection per computer, against the real helper and stand-ins written in sh.

use serde_json::{Value, json};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
use treeline_core::hosts::{Hosts, describe, start_failed};
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
fn together(h: &Arc<Hosts>, host: &str, n: usize, method: &str, retry: bool) -> Vec<Result<Value, String>> {
    let gate = Arc::new(Barrier::new(n));
    let threads: Vec<_> = (0..n)
        .map(|_| {
            let (h, gate, host, method) = (Arc::clone(h), Arc::clone(&gate), host.to_string(), method.to_string());
            std::thread::spawn(move || {
                gate.wait();
                h.call(&host, &method, json!({}), SEC, retry)
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
    for r in together(&h, "linux-box", 8, "ping", true) {
        assert_eq!(r.unwrap(), json!({}));
    }
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    h.call("linux-box", "ping", json!({}), SEC, true).unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 1, "a live connection is reused");
}

#[test]
fn a_dropped_link_costs_one_new_connection_and_no_call() {
    // The first connection ends once all 8 requests reached it; every later one is the real helper.
    let (h, starts) = hosts(Duration::from_millis(100), |n| if n == 1 { drops_after(8) } else { helper() });
    for r in together(&h, "linux-box", 8, "ping", true) {
        assert_eq!(r.unwrap(), json!({}), "each call cut off is sent once more");
    }
    assert_eq!(starts.load(Ordering::SeqCst), 2, "the 8 retries share one new connection");
}

#[test]
fn calls_that_may_change_something_are_not_sent_twice() {
    let (h, starts) = hosts(Duration::ZERO, |n| if n == 1 { drops_after(1) } else { helper() });
    let err = h.call("linux-box", "ping", json!({}), SEC, false).unwrap_err();
    assert_eq!(err, "Treeline lost the connection to “linux-box”.");
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[test]
fn a_retry_that_drops_too_is_reported_not_repeated() {
    let (h, starts) = hosts(Duration::ZERO, |_| drops_after(1));
    let err = h.call("linux-box", "ping", json!({}), SEC, true).unwrap_err();
    assert_eq!(err, "Treeline lost the connection to “linux-box”.");
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}

#[test]
fn a_failed_start_reaches_every_waiting_call() {
    let (h, starts) = hosts(Duration::from_millis(200), |_| Err("ssh: connect to host linux-box port 22: No route to host".into()));
    for r in together(&h, "linux-box", 5, "ping", true) {
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
        std::thread::spawn(move || h.call("slow", "ping", json!({}), SEC, true))
    };
    std::thread::sleep(Duration::from_millis(100)); // the slow start is under way
    let t = Instant::now();
    h.call("fast", "ping", json!({}), SEC, true).unwrap();
    assert!(t.elapsed() < Duration::from_millis(800), "took {:?}", t.elapsed());
    slow.join().unwrap().unwrap();
}

#[test]
fn a_reaction_to_an_old_connection_leaves_the_new_one_alone() {
    let (h, starts) = hosts(Duration::ZERO, |_| helper());
    let (g1, _) = h.get("linux-box").unwrap();
    h.invalidate("linux-box", g1);
    let (g2, _) = h.get("linux-box").unwrap();
    assert_eq!((g2, starts.load(Ordering::SeqCst)), (g1 + 1, 2));
    h.invalidate("linux-box", g1); // late, about the first one
    assert_eq!(h.get("linux-box").unwrap().0, g2);
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
