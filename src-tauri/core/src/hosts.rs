//! One connection per computer, shared by every caller: started once even when many calls arrive together, replaced
//! once when it drops, and never started while holding the lock (a slow computer does not hold up another).
//!
//! Each connection has a generation. Anything that reacts to a connection (a call that saw it close) names the
//! generation it saw, so a late reaction cannot replace a newer connection.

use crate::remote::{CallError, Connection};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

/// Starts a ready connection to a computer (in the app: ssh, the helper, `init`); the error is the reason, for the user.
pub type Connect = dyn Fn(&str) -> Result<Connection, String> + Send + Sync;

enum Entry {
    Connecting(u64),
    Alive(u64, Arc<Connection>),
    /// The start of this generation failed; the callers that waited for it get the error.
    Failed(u64, String),
    /// This generation dropped; the next caller starts the next one.
    Gone(u64),
}

impl Entry {
    fn generation(&self) -> u64 {
        match self {
            Entry::Connecting(g) | Entry::Alive(g, _) | Entry::Failed(g, _) | Entry::Gone(g) => *g,
        }
    }
}

pub struct Hosts {
    connect: Box<Connect>,
    entries: Mutex<HashMap<String, Entry>>,
    changed: Condvar,
}

/// The user-facing texts, with the computer's name in quotes so the page can find it again (see `remote-errors.json`).
pub fn start_failed(host: &str, reason: &str) -> String {
    format!("Treeline could not connect to “{host}”: {reason}")
}

pub fn describe(host: &str, e: CallError) -> String {
    match e {
        CallError::Closed => format!("Treeline lost the connection to “{host}”."),
        CallError::Cancelled => format!("Treeline closed the connection to “{host}”."),
        CallError::Timeout => format!("Treeline got no answer from “{host}” in time."),
        CallError::Remote(e) => e,
    }
}

impl Hosts {
    pub fn new(connect: impl Fn(&str) -> Result<Connection, String> + Send + Sync + 'static) -> Hosts {
        Hosts { connect: Box::new(connect), entries: Mutex::new(HashMap::new()), changed: Condvar::new() }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The live connection to `host` and its generation, starting one if needed (or waiting for the one being started).
    pub fn get(&self, host: &str) -> Result<(u64, Arc<Connection>), String> {
        let mut entries = self.lock();
        let mut waited: Option<u64> = None;
        loop {
            match entries.get(host) {
                Some(Entry::Alive(g, c)) if c.is_alive() => return Ok((*g, Arc::clone(c))),
                Some(Entry::Connecting(g)) => {
                    waited = Some(*g);
                    entries = self.changed.wait(entries).unwrap_or_else(|e| e.into_inner());
                }
                Some(Entry::Failed(g, err)) if waited == Some(*g) => return Err(err.clone()),
                other => {
                    // Nothing usable (none yet, a dead one, or a failure from before this call): start the next one.
                    let g = other.map_or(0, Entry::generation) + 1;
                    entries.insert(host.to_string(), Entry::Connecting(g));
                    drop(entries);
                    let started = (self.connect)(host).map(Arc::new).map_err(|reason| start_failed(host, &reason));
                    let mut entries = self.lock();
                    if matches!(entries.get(host), Some(Entry::Connecting(n)) if *n == g) {
                        let entry = match &started {
                            Ok(c) => Entry::Alive(g, Arc::clone(c)),
                            Err(e) => Entry::Failed(g, e.clone()),
                        };
                        entries.insert(host.to_string(), entry);
                    }
                    self.changed.notify_all();
                    return started.map(|c| (g, c));
                }
            }
        }
    }

    /// Marks generation `g` of `host` as gone, if it is still the current one (a later one is left alone).
    pub fn invalidate(&self, host: &str, g: u64) {
        let mut entries = self.lock();
        // Not closed here: the requests still in flight on it get `Closed` from the process ending, and retry too.
        if matches!(entries.get(host), Some(Entry::Alive(n, _)) if *n == g) {
            entries.insert(host.to_string(), Entry::Gone(g));
        }
    }

    /// One request to `host`. With `retry`, a request cut off by a dropped link is sent once more on a new connection
    /// (only for requests that change nothing, so sending one twice is harmless).
    pub fn call(&self, host: &str, method: &str, params: Value, timeout: Duration, retry: bool) -> Result<Value, String> {
        let (g, conn) = self.get(host)?;
        match conn.call(method, params.clone(), timeout) {
            Err(CallError::Closed) if retry => {
                self.invalidate(host, g);
                let (_, conn) = self.get(host)?;
                conn.call(method, params, timeout).map_err(|e| describe(host, e))
            }
            other => other.map_err(|e| describe(host, e)),
        }
    }
}
