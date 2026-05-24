//! Logging utilities shared by app and core modules.
//!
//! This module centralizes log directory resolution, safe JSONL appends, atomic
//! JSON writes, and short single-line previews. Keeping these helpers in core
//! prevents portable builds from scattering diagnostics across multiple paths.

use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

const DEFAULT_JSONL_MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ROTATED_FILES: usize = 5;

static LOG_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static FRONTEND_LOG_GATE: OnceLock<Mutex<HashMap<String, SystemTime>>> = OnceLock::new();

/// Return the canonical BitCat log directory.
///
/// Resolution order is explicit Windows profile, generic HOME, then the
/// platform-specific home directory reported by `dirs`.
pub fn log_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("USERPROFILE")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .or_else(dirs::home_dir)
        .ok_or_else(|| "unable to resolve home directory".to_string())?;
    Ok(home.join(".bitcat").join("logs"))
}

/// Append a serializable record to a JSONL file under the canonical log dir.
pub fn append_jsonl(file_name: &str, value: &impl Serialize) -> Result<PathBuf, String> {
    let dir = log_dir()?;
    let path = dir.join(file_name);
    append_jsonl_path(&path, value)?;
    Ok(path)
}

/// Append a serializable record to a JSONL file with size-based rotation.
pub fn append_jsonl_path(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let _guard = LOG_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|e| format!("log write lock poisoned: {e}"))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create log directory failed: {e}"))?;
    }

    rotate_if_needed(path, DEFAULT_JSONL_MAX_BYTES)?;
    let line = serde_json::to_string(value).map_err(|e| format!("serialize JSONL failed: {e}"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("open JSONL log failed: {e}"))?;
    writeln!(file, "{line}").map_err(|e| format!("write JSONL log failed: {e}"))
}

/// Write JSON atomically by creating a temporary sibling file then renaming it.
pub fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let _guard = LOG_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|e| format!("log write lock poisoned: {e}"))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create log directory failed: {e}"))?;
    }

    let json =
        serde_json::to_string_pretty(value).map_err(|e| format!("serialize JSON failed: {e}"))?;
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|s| s.to_str()).unwrap_or("json")
    ));
    fs::write(&tmp, json).map_err(|e| format!("write temp JSON failed: {e}"))?;
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("remove old JSON failed: {e}"))?;
    }
    fs::rename(&tmp, path).map_err(|e| format!("replace JSON failed: {e}"))
}

/// Remove old managed log files by modification time.
pub fn cleanup_old_logs(dir: &Path, retention: Duration) -> Result<usize, String> {
    if !dir.is_dir() {
        return Ok(0);
    }

    let cutoff = SystemTime::now()
        .checked_sub(retention)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut removed = 0usize;
    for entry in fs::read_dir(dir).map_err(|e| format!("read log dir failed: {e}"))? {
        let entry = entry.map_err(|e| format!("read log entry failed: {e}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !is_managed_log_file(name) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if modified < cutoff && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

fn is_managed_log_file(name: &str) -> bool {
    name.starts_with("app.log.")
        || name.ends_with(".jsonl")
        || name.contains(".jsonl.")
        || name == "panic.log"
        || name == "native-crash.log"
        || (name.starts_with("crash-") && name.ends_with(".dmp"))
}

fn rotate_if_needed(path: &Path, max_bytes: u64) -> Result<(), String> {
    let Ok(meta) = fs::metadata(path) else {
        return Ok(());
    };
    if meta.len() < max_bytes {
        return Ok(());
    }

    for index in (1..=MAX_ROTATED_FILES).rev() {
        let from = rotated_path(path, index);
        let to = rotated_path(path, index + 1);
        if from.exists() {
            if index == MAX_ROTATED_FILES {
                let _ = fs::remove_file(&from);
            } else {
                let _ = fs::rename(&from, &to);
            }
        }
    }
    fs::rename(path, rotated_path(path, 1)).map_err(|e| format!("rotate log failed: {e}"))
}

fn rotated_path(path: &Path, index: usize) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("log.jsonl");
    path.with_file_name(format!("{file_name}.{index}"))
}

/// Build a short, single-line preview for logs.
///
/// The returned string is character-safe and normalizes line breaks so log
/// records stay one event per line.
pub fn log_preview(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    let mut out: String = s.chars().take(max_chars).collect();
    if char_count > max_chars {
        out.push('…');
    }
    out.replace('\r', "\\r").replace('\n', "\\n")
}

/// Return true when a frontend log source is allowed to emit.
///
/// This intentionally drops bursts from chatty windows such as resize loops
/// while still preserving periodic context in the backend log.
pub fn frontend_log_allowed(source: &str, min_interval: Duration) -> bool {
    let now = SystemTime::now();
    let mut gate = match FRONTEND_LOG_GATE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        Ok(gate) => gate,
        Err(_) => return true,
    };
    let Some(last) = gate.get(source) else {
        gate.insert(source.to_string(), now);
        return true;
    };
    if now.duration_since(*last).unwrap_or_default() >= min_interval {
        gate.insert(source.to_string(), now);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_keeps_short_text() {
        assert_eq!(log_preview("hello", 10), "hello");
    }

    #[test]
    fn preview_truncates_by_chars() {
        assert_eq!(log_preview("你好世界", 2), "你好…");
    }

    #[test]
    fn preview_escapes_newlines() {
        assert_eq!(log_preview("a\nb\rc", 10), "a\\nb\\rc");
    }

    #[test]
    fn append_jsonl_path_writes_one_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        append_jsonl_path(&path, &serde_json::json!({"ok": true})).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert_eq!(content.lines().count(), 1);
    }

    #[test]
    fn write_json_atomic_replaces_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        write_json_atomic(&path, &serde_json::json!({"n": 1})).unwrap();
        write_json_atomic(&path, &serde_json::json!({"n": 2})).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("2"));
    }

    #[test]
    fn managed_log_files_include_native_crash_artifacts() {
        assert!(is_managed_log_file("native-crash.log"));
        assert!(is_managed_log_file("crash-1710000000-1234.dmp"));
        assert!(!is_managed_log_file("notes.dmp"));
    }
}
