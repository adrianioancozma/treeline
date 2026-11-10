//! Treeline's git layer without Tauri: parsing git output, and later running git and the commands themselves.
//! The desktop app uses it in process; a helper on another computer can use the same code.

pub mod git;
pub mod ops_parse;
pub mod read;
pub mod status;
pub mod trust;

/// Tests that run git share the process-wide git program setting, and one of them swaps in a stand-in git:
/// every such test holds this lock.
#[cfg(test)]
pub(crate) fn serial_git() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}
