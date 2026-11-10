//! Treeline's git layer without Tauri: parsing git output, and later running git and the commands themselves.
//! The desktop app uses it in process; a helper on another computer can use the same code.

pub mod ops_parse;
pub mod status;
