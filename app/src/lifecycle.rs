//! Structured lifecycle diagnostics for the desktop process.
//!
//! Plain tracing lines are good for humans, but crash triage also needs a small
//! append-only record of process milestones. This module writes startup and
//! shutdown markers to `lifecycle.jsonl` so abrupt exits are visible as missing
//! terminal events.

use serde::Serialize;
use tracing::warn;

/// A stable process lifecycle marker written to the log directory.
#[derive(Debug, Serialize)]
pub struct LifecycleRecord<'a> {
    pub timestamp: String,
    pub event: &'a str,
    pub reason: Option<&'a str>,
    pub pid: u32,
    pub version: &'static str,
    pub exe: Option<String>,
    pub cwd: Option<String>,
}

/// Write a lifecycle event and log a warning if the file write fails.
pub fn record_lifecycle_event(event: &'static str, reason: Option<&'static str>) {
    if let Err(e) = write_lifecycle_event(event, reason) {
        warn!(error = %e, event, "lifecycle event write failed");
    }
}

/// Write a lifecycle event to `lifecycle.jsonl`.
pub fn write_lifecycle_event(
    event: &'static str,
    reason: Option<&'static str>,
) -> Result<(), String> {
    let record = LifecycleRecord {
        timestamp: chrono::Local::now().to_rfc3339(),
        event,
        reason,
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION"),
        exe: std::env::current_exe()
            .ok()
            .map(|path| path.display().to_string()),
        cwd: std::env::current_dir()
            .ok()
            .map(|path| path.display().to_string()),
    };
    ai_pad_core::logging::append_jsonl("lifecycle.jsonl", &record).map(|_| ())
}
