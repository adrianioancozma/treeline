//! One connection per computer, shared by every caller: started once even when many calls arrive together, replaced
//! once when it drops, and never started while holding the lock (a slow computer does not hold up another).
//!
//! Each connection has a generation. Anything that reacts to a connection (a call that saw it close, the process
//! ending, a ping that got no answer) names the generation it saw, so a late reaction cannot touch a newer connection.
//! A failed start is remembered for `Config::hold`: a tab's calls get the same error at once instead of each starting
//! ssh again, while what the user just asked for (connect, browse, open, Reconnect) always tries.

use crate::remote::{Abort, CallError, Connection};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Starts a ready connection to a computer (in the app: ssh, the helper, `init`); the error is the reason, for the user.
/// The `Abort` ends that start from another thread (the time limit, a disconnect): pass it to `Connection::start_with`.
pub type Connect = dyn Fn(&str, &Abort) -> Result<Connection, String> + Send + Sync;
/// Told every change of a computer's state, in order (called with the lock held: it must not call `Hosts`).
pub type Sink = dyn Fn(&str, u64, State) + Send + Sync;

/// A computer's state as the page shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Connecting,
    Alive,
    Failed,
    Idle,
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// How long a failed start is answered from memory for `Ask::READ` calls.
    pub hold: Duration,
    /// How long Reconnect waits for a live connection to answer before replacing it.
    pub ping: Duration,
    /// The longest a start may take in all (ssh, sending the helper, its hello, `init`).
    pub deadline: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Config { hold: Duration::from_secs(10), ping: Duration::from_secs(3), deadline: Duration::from_secs(25) }
    }
}

/// Who is asking, which decides what a call may do.
#[derive(Debug, Clone, Copy)]
pub struct Ask {
    /// Sent once more on a new connection when the link drops under it (only calls that change nothing).
    pub retry: bool,
    /// Answered from a remembered failed start instead of starting ssh again.
    pub respect_hold: bool,
}

impl Ask {
    /// A tab reading a repository.
    pub const READ: Ask = Ask { retry: true, respect_hold: true };
    /// Something the user just asked for and that changes nothing (connect, browse, open a folder).
    pub const EXPLICIT: Ask = Ask { retry: true, respect_hold: false };
    /// A call that may change something: never sent twice.
    pub const WRITE: Ask = Ask { retry: false, respect_hold: true };
}

enum Entry {
    Connecting(u64, Abort),
    Alive(u64, Arc<Connection>),
    /// The start of this generation failed; the callers that waited for it get the error.
    Failed(u64, String, Instant),
    /// This generation dropped; the next caller starts the next one.
    Gone(u64),
    /// Treeline closed this generation (disconnect); the callers that waited for it are told so.
    Cancelled(u64),
}

impl Entry {
    fn generation(&self) -> u64 {
        match self {
            Entry::Connecting(g, _) | Entry::Alive(g, _) | Entry::Failed(g, _, _) | Entry::Gone(g) | Entry::Cancelled(g) => *g,
        }
    }

    fn state(&self) -> State {
        match self {
            Entry::Connecting(..) => State::Connecting,
            Entry::Alive(..) => State::Alive,
            Entry::Failed(..) => State::Failed,
            Entry::Gone(_) | Entry::Cancelled(_) => State::Idle,
        }
    }
}

type Entries = HashMap<String, Entry>;

struct Inner {
    connect: Box<Connect>,
    cfg: Config,
    sink: Box<Sink>,
    entries: Mutex<Entries>,
    changed: Condvar,
}

pub struct Hosts(Arc<Inner>);

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
    pub fn new(connect: impl Fn(&str, &Abort) -> Result<Connection, String> + Send + Sync + 'static) -> Hosts {
        Hosts::with(connect, Config::default(), |_, _, _| {})
    }

    pub fn with(
        connect: impl Fn(&str, &Abort) -> Result<Connection, String> + Send + Sync + 'static,
        cfg: Config,
        sink: impl Fn(&str, u64, State) + Send + Sync + 'static,
    ) -> Hosts {
        Hosts(Arc::new(Inner {
            connect: Box::new(connect),
            cfg,
            sink: Box::new(sink),
            entries: Mutex::new(HashMap::new()),
            changed: Condvar::new(),
        }))
    }

    fn lock(&self) -> MutexGuard<'_, Entries> {
        self.0.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set(&self, entries: &mut Entries, host: &str, entry: Entry) {
        (self.0.sink)(host, entry.generation(), entry.state());
        entries.insert(host.to_string(), entry);
    }

    /// Every computer Treeline knows about, with its generation and state (for a page that starts listening late).
    pub fn states(&self) -> Vec<(String, u64, State)> {
        self.lock().iter().map(|(h, e)| (h.clone(), e.generation(), e.state())).collect()
    }

    /// The live connection to `host` and its generation, starting one if needed (or waiting for the one being started).
    pub fn get(&self, host: &str, ask: Ask) -> Result<(u64, Arc<Connection>), String> {
        let mut entries = self.lock();
        let mut waited: Option<u64> = None;
        loop {
            match entries.get(host) {
                Some(Entry::Alive(g, c)) if c.is_alive() => return Ok((*g, Arc::clone(c))),
                Some(Entry::Connecting(g, _)) => {
                    waited = Some(*g);
                    entries = self.0.changed.wait(entries).unwrap_or_else(|e| e.into_inner());
                }
                Some(Entry::Failed(g, err, _)) if waited == Some(*g) => return Err(err.clone()),
                Some(Entry::Cancelled(g)) if waited == Some(*g) => return Err(describe(host, CallError::Cancelled)),
                Some(Entry::Failed(_, err, at)) if ask.respect_hold && at.elapsed() < self.0.cfg.hold => return Err(err.clone()),
                other => {
                    // Nothing usable (none yet, a dead one, an old failure): start the next generation.
                    let g = other.map_or(0, Entry::generation) + 1;
                    let abort = Abort::new();
                    self.set(&mut entries, host, Entry::Connecting(g, abort.clone()));
                    drop(entries);
                    self.watch_deadline(host, g, &abort);
                    return self.start(host, g, &abort);
                }
            }
        }
    }

    /// Ends the start of generation `g` if it is still under way when `Config::deadline` passes.
    fn watch_deadline(&self, host: &str, g: u64, abort: &Abort) {
        let (inner, host, abort) = (Arc::clone(&self.0), host.to_string(), abort.clone());
        std::thread::spawn(move || {
            let until = Instant::now() + inner.cfg.deadline;
            let mut entries = inner.entries.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if !matches!(entries.get(&host), Some(Entry::Connecting(n, _)) if *n == g) {
                    return;
                }
                let now = Instant::now();
                if now >= until {
                    break;
                }
                entries = inner.changed.wait_timeout(entries, until - now).unwrap_or_else(|e| e.into_inner()).0;
            }
            drop(entries);
            abort.abort(&format!("it did not answer within {} seconds.", inner.cfg.deadline.as_secs_f64().ceil()));
        });
    }

    fn start(&self, host: &str, g: u64, abort: &Abort) -> Result<(u64, Arc<Connection>), String> {
        let started = match (self.0.connect)(host, abort) {
            // Aborted after all (the time limit passed as it finished): not used.
            Ok(_) if abort.reason().is_some() => Err(start_failed(host, &abort.reason().unwrap_or_default())),
            Ok(c) => Ok(Arc::new(c)),
            Err(reason) => Err(start_failed(host, &abort.reason().unwrap_or(reason))),
        };
        let mut entries = self.lock();
        let current = matches!(entries.get(host), Some(Entry::Connecting(n, _)) if *n == g);
        let result = match (&started, current) {
            (Ok(c), true) => {
                self.set(&mut entries, host, Entry::Alive(g, Arc::clone(c)));
                Ok((g, Arc::clone(c)))
            }
            (Err(e), true) => {
                self.set(&mut entries, host, Entry::Failed(g, e.clone(), Instant::now()));
                Err(e.clone())
            }
            // Disconnected while starting: the new connection is not used (it closes when dropped, below).
            (_, false) => Err(describe(host, CallError::Cancelled)),
        };
        self.0.changed.notify_all();
        drop(entries);
        if let (Ok((_, c)), true) = (&result, current) {
            // Outside the lock: if the process already ended, this runs at once.
            let inner = Arc::downgrade(&self.0);
            let host = host.to_string();
            c.on_close(move || {
                if let Some(inner) = inner.upgrade() {
                    Hosts(inner).invalidate(&host, g);
                }
            });
        }
        drop(started);
        result
    }

    /// Marks generation `g` of `host` as gone, if it is still the current one (a later one is left alone).
    pub fn invalidate(&self, host: &str, g: u64) {
        let mut entries = self.lock();
        // Not closed here: the requests still in flight on it get `Closed` from the process ending, and retry too.
        if matches!(entries.get(host), Some(Entry::Alive(n, _)) if *n == g) {
            self.set(&mut entries, host, Entry::Gone(g));
        }
    }

    /// What Reconnect does: keeps a connection that answers, replaces one that does not, starts one otherwise
    /// (even right after a failed start), and waits for the outcome.
    pub fn reconnect(&self, host: &str) -> Result<(), String> {
        let alive = match self.lock().get(host) {
            Some(Entry::Alive(g, c)) if c.is_alive() => Some((*g, Arc::clone(c))),
            _ => None,
        };
        if let Some((g, c)) = alive {
            if c.call("ping", json!({}), self.0.cfg.ping).is_ok() {
                return Ok(());
            }
            self.invalidate(host, g);
            c.abandon(); // its requests in flight get `Closed` and move to the new connection
        }
        self.get(host, Ask::EXPLICIT).map(|_| ())
    }

    /// Closes the connection to `host`: calls in flight are told Treeline closed it (and are not sent again), a start
    /// under way is not used, and a remembered failure is forgotten.
    pub fn disconnect(&self, host: &str) {
        let (conn, starting) = {
            let mut entries = self.lock();
            let (g, conn, starting) = match entries.get(host) {
                Some(Entry::Alive(g, c)) => (*g, Some(Arc::clone(c)), None),
                Some(Entry::Connecting(g, a)) => (*g, None, Some(a.clone())),
                Some(Entry::Failed(g, _, _)) => (*g, None, None),
                _ => return,
            };
            self.set(&mut entries, host, Entry::Cancelled(g));
            self.0.changed.notify_all();
            (conn, starting)
        };
        if let Some(c) = conn {
            c.close();
        }
        if let Some(a) = starting {
            a.abort("Treeline closed the connection.");
        }
    }

    /// One request to `host`. A request cut off by a dropped link is sent once more on a new connection when
    /// `ask.retry` (only requests that change nothing, so sending one twice is harmless).
    pub fn call(&self, host: &str, method: &str, params: Value, timeout: Duration, ask: Ask) -> Result<Value, String> {
        let (g, conn) = self.get(host, ask)?;
        match conn.call(method, params.clone(), timeout) {
            Err(CallError::Closed) if ask.retry => {
                self.invalidate(host, g);
                let (_, conn) = self.get(host, ask)?;
                conn.call(method, params, timeout).map_err(|e| describe(host, e))
            }
            other => other.map_err(|e| describe(host, e)),
        }
    }
}
