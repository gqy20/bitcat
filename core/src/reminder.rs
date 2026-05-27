//! Programmatic reminders persisted as small JSON records.
//!
//! This module owns reminder validation, storage, and due-time calculation so
//! the agent tool and the app scheduler can share one deterministic contract.
//! The app crate is responsible for rendering fired reminders as UI.

use chrono::{
    DateTime, Duration as ChronoDuration, Local, LocalResult, NaiveDateTime, NaiveTime, TimeZone,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::info;

const MAX_TITLE_CHARS: usize = 80;
const MAX_MESSAGE_CHARS: usize = 280;
const MIN_INTERVAL_MINUTES: u32 = 1;
const MAX_INTERVAL_MINUTES: u32 = 60 * 24 * 365;

/// Reminder lifecycle persisted on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReminderStatus {
    Active,
    Done,
    Cancelled,
}

impl ReminderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Supported deterministic schedule forms for reminders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReminderSchedule {
    Once { at: String },
    Interval { every_minutes: u32 },
    Daily { time: String },
}

/// A reminder record stored in `~/.bitcat/reminders/reminders.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReminderRecord {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub schedule: ReminderSchedule,
    pub next_fire_at: String,
    pub status: ReminderStatus,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fired_at: Option<String>,
    #[serde(default)]
    pub fire_count: u32,
}

/// Reminder lifecycle event written to `~/.bitcat/logs/reminder_events.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderEventRecord {
    pub at: String,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reminder_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ReminderStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_fire_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_bytes: Option<String>,
}

/// Schedule kind accepted by the agent-facing create_reminder tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreateReminderKind {
    Once,
    Interval,
    Daily,
}

/// Agent-facing arguments for creating a reminder.
///
/// The schema is intentionally explicit because LLMs often confuse one-shot
/// clock-time reminders with daily recurring reminders. `daily_time` is only
/// for repeated daily schedules; a single "10 点提醒我" request must use
/// `schedule_kind = once` with `at = "YYYY-MM-DD 10:00"`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CreateReminderArgs {
    /// Short reminder title, for example "Drink water".
    pub title: String,
    /// Optional detail shown when the reminder fires.
    #[serde(default)]
    pub message: Option<String>,
    /// Schedule type: once, interval, or daily. Use once unless the user clearly asks for a repeating reminder.
    pub schedule_kind: CreateReminderKind,
    /// Absolute local time for one-shot clock-time reminders. Accepts RFC3339 or "YYYY-MM-DD HH:MM".
    #[serde(default)]
    pub at: Option<String>,
    /// Relative delay in minutes for one-shot reminders like "in 3 minutes". JSON number, not a string.
    #[serde(default)]
    pub delay_minutes: Option<u32>,
    /// Interval in minutes for repeated interval reminders like "every 30 minutes". JSON number, not a string.
    #[serde(default)]
    pub interval_minutes: Option<u32>,
    /// Local clock time for daily repeated reminders only, such as "09:00". Do not use for one-shot reminders.
    #[serde(default)]
    pub daily_time: Option<String>,
}

/// Agent-facing arguments for listing reminders.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct ListRemindersArgs {
    /// Include done/cancelled reminders as well as active reminders.
    #[serde(default)]
    pub include_inactive: bool,
}

/// Agent-facing arguments for cancelling a reminder.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CancelReminderArgs {
    /// Reminder id returned by create_reminder or list_reminders.
    pub id: String,
}

/// Return the reminder store path under the user's data directory.
pub fn reminder_store_path() -> Result<PathBuf, String> {
    let dir = crate::storage::app_data_dir()?.join("reminders");
    Ok(dir.join("reminders.json"))
}

/// Return the reminder event audit log path.
pub fn reminder_events_path() -> Result<PathBuf, String> {
    Ok(crate::logging::log_dir()?.join("reminder_events.jsonl"))
}

/// Load all reminders from the default store.
pub fn load_reminders() -> Result<Vec<ReminderRecord>, String> {
    load_reminders_from_path(&reminder_store_path()?)
}

/// Save all reminders to the default store.
pub fn save_reminders(reminders: &[ReminderRecord]) -> Result<(), String> {
    save_reminders_to_path(&reminder_store_path()?, reminders)
}

/// Create a reminder from agent tool arguments.
pub fn create_reminder(args: &CreateReminderArgs) -> Result<ReminderRecord, String> {
    let path = reminder_store_path()?;
    match create_reminder_in_path(&path, args, Local::now()) {
        Ok(reminder) => Ok(reminder),
        Err(e) => {
            record_reminder_failure("create_failed", Some("agent"), Some(e.clone()), None);
            Err(e)
        }
    }
}

/// List reminders for agent tool output.
pub fn list_reminders(args: &ListRemindersArgs) -> Result<Vec<ReminderRecord>, String> {
    let mut reminders = load_reminders()?;
    if !args.include_inactive {
        reminders.retain(|r| r.status == ReminderStatus::Active);
    }
    reminders.sort_by(|a, b| a.next_fire_at.cmp(&b.next_fire_at));
    Ok(reminders)
}

/// Cancel a reminder by id.
pub fn cancel_reminder(id: &str) -> Result<ReminderRecord, String> {
    cancel_reminder_with_source(id, "api")
}

/// Cancel a reminder by id and record the operation source.
pub fn cancel_reminder_with_source(id: &str, ui_source: &str) -> Result<ReminderRecord, String> {
    let path = reminder_store_path()?;
    cancel_reminder_in_path(&path, id, ui_source)
}

/// Permanently delete a reminder record by id.
pub fn delete_reminder(id: &str) -> Result<ReminderRecord, String> {
    delete_reminder_with_source(id, "api")
}

/// Permanently delete a reminder record and record the operation source.
pub fn delete_reminder_with_source(id: &str, ui_source: &str) -> Result<ReminderRecord, String> {
    let path = reminder_store_path()?;
    delete_reminder_in_path(&path, id, ui_source)
}

/// Snooze a reminder by moving its next fire time forward.
pub fn snooze_reminder(id: &str, minutes: u32) -> Result<ReminderRecord, String> {
    snooze_reminder_with_source(id, minutes, "api")
}

/// Snooze a reminder and record the operation source.
pub fn snooze_reminder_with_source(
    id: &str,
    minutes: u32,
    ui_source: &str,
) -> Result<ReminderRecord, String> {
    let path = reminder_store_path()?;
    let mut reminders = load_reminders_from_path(&path)?;
    let now = Local::now();
    let Some(reminder) = reminders.iter_mut().find(|r| r.id == id) else {
        return Err(format!("reminder not found: {id}"));
    };
    reminder.status = ReminderStatus::Active;
    reminder.next_fire_at = to_rfc3339(now + ChronoDuration::minutes(minutes.max(1) as i64));
    reminder.updated_at = to_rfc3339(now);
    let updated = reminder.clone();
    save_reminders_to_path(&path, &reminders)?;
    record_reminder_event(
        "snoozed",
        &updated,
        Some(format!("{}m", minutes.max(1))),
        Some(ui_source),
    );
    Ok(updated)
}

/// Acknowledge a reminder occurrence.
///
/// One-shot reminders stay done after firing. Recurring reminders remain active
/// because the scheduler has already advanced `next_fire_at` before showing UI.
pub fn complete_reminder(id: &str) -> Result<ReminderRecord, String> {
    complete_reminder_with_source(id, "api")
}

/// Acknowledge a reminder and record the operation source.
pub fn complete_reminder_with_source(id: &str, ui_source: &str) -> Result<ReminderRecord, String> {
    let path = reminder_store_path()?;
    complete_reminder_in_path(&path, id, ui_source)
}

fn complete_reminder_in_path(
    path: &Path,
    id: &str,
    ui_source: &str,
) -> Result<ReminderRecord, String> {
    let mut reminders = load_reminders_from_path(path)?;
    let now = Local::now();
    let Some(reminder) = reminders.iter_mut().find(|r| r.id == id) else {
        return Err(format!("reminder not found: {id}"));
    };
    if matches!(reminder.schedule, ReminderSchedule::Once { .. }) {
        reminder.status = ReminderStatus::Done;
    }
    reminder.updated_at = to_rfc3339(now);
    let updated = reminder.clone();
    save_reminders_to_path(path, &reminders)?;
    record_reminder_event("completed", &updated, None, Some(ui_source));
    Ok(updated)
}

/// Fire due active reminders and update the store.
pub fn fire_due_reminders(now: DateTime<Local>) -> Result<Vec<ReminderRecord>, String> {
    let path = reminder_store_path()?;
    fire_due_reminders_in_path(&path, now)
}

fn load_reminders_from_path(path: &Path) -> Result<Vec<ReminderRecord>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            let error = format!("read reminders failed: {e}");
            record_store_failure("store_read_failed", path, &error);
            return Err(error);
        }
    };
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&content).map_err(|e| {
        let error = format!("parse reminders failed: {e}");
        record_store_failure("store_read_failed", path, &error);
        error
    })
}

fn save_reminders_to_path(path: &Path, reminders: &[ReminderRecord]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create reminder dir failed: {e}"))?;
    }
    let json = serde_json::to_string_pretty(reminders)
        .map_err(|e| format!("serialize reminders failed: {e}"))?;
    write_reminders_atomically(path, json.as_bytes()).map_err(|e| {
        let error = format!("write reminders failed: {e}");
        record_store_failure("store_write_failed", path, &error);
        error
    })
}

fn write_reminders_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("reminder path has no parent: {}", path.display()))?;
    let temp_path = parent.join(format!(
        ".{}.tmp-{}-{:08x}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("reminders.json"),
        std::process::id(),
        rand::random::<u32>()
    ));
    {
        let mut file = fs::File::create(&temp_path)
            .map_err(|e| format!("create temp reminder file failed: {e}"))?;
        file.write_all(bytes)
            .map_err(|e| format!("write temp reminder file failed: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("sync temp reminder file failed: {e}"))?;
    }
    replace_file(&temp_path, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp_path);
    })
}

#[cfg(target_os = "windows")]
fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    let from_w = wide(from);
    let to_w = wide(to);
    let ok = unsafe {
        MoveFileExW(
            from_w.as_ptr(),
            to_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(format!(
            "replace reminder file failed: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    fs::rename(from, to).map_err(|e| format!("replace reminder file failed: {e}"))
}

fn create_reminder_in_path(
    path: &Path,
    args: &CreateReminderArgs,
    now: DateTime<Local>,
) -> Result<ReminderRecord, String> {
    let title = clean_required(&args.title, MAX_TITLE_CHARS, "title")?;
    let message = args
        .message
        .as_ref()
        .map(|m| clean_optional(m, MAX_MESSAGE_CHARS))
        .filter(|m| !m.is_empty());
    let schedule = build_schedule(args, now)?;
    let next_fire_at = compute_next_fire_at(&schedule, now)?;
    let timestamp = to_rfc3339(now);
    let mut reminders = load_reminders_from_path(path)?;
    let reminder = ReminderRecord {
        id: new_reminder_id(now),
        title,
        message,
        schedule,
        next_fire_at: to_rfc3339(next_fire_at),
        status: ReminderStatus::Active,
        source: "agent".to_string(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
        last_fired_at: None,
        fire_count: 0,
    };
    reminders.push(reminder.clone());
    save_reminders_to_path(path, &reminders)?;
    record_reminder_event("created", &reminder, None, None);
    info!(id = %reminder.id, title = %reminder.title, next_fire_at = %reminder.next_fire_at, "reminder created");
    Ok(reminder)
}

fn cancel_reminder_in_path(
    path: &Path,
    id: &str,
    ui_source: &str,
) -> Result<ReminderRecord, String> {
    let mut reminders = load_reminders_from_path(path)?;
    let now = Local::now();
    let Some(reminder) = reminders.iter_mut().find(|r| r.id == id) else {
        return Err(format!("reminder not found: {id}"));
    };
    reminder.status = ReminderStatus::Cancelled;
    reminder.updated_at = to_rfc3339(now);
    let updated = reminder.clone();
    save_reminders_to_path(path, &reminders)?;
    record_reminder_event("cancelled", &updated, None, Some(ui_source));
    Ok(updated)
}

fn delete_reminder_in_path(
    path: &Path,
    id: &str,
    ui_source: &str,
) -> Result<ReminderRecord, String> {
    let mut reminders = load_reminders_from_path(path)?;
    let Some(index) = reminders.iter().position(|r| r.id == id) else {
        return Err(format!("reminder not found: {id}"));
    };
    let mut deleted = reminders.remove(index);
    deleted.updated_at = to_rfc3339(Local::now());
    save_reminders_to_path(path, &reminders)?;
    record_reminder_event("deleted", &deleted, None, Some(ui_source));
    Ok(deleted)
}

fn fire_due_reminders_in_path(
    path: &Path,
    now: DateTime<Local>,
) -> Result<Vec<ReminderRecord>, String> {
    let mut reminders = load_reminders_from_path(path)?;
    let mut fired = Vec::new();
    for reminder in reminders.iter_mut() {
        if reminder.status != ReminderStatus::Active {
            continue;
        }
        let Ok(next) = DateTime::parse_from_rfc3339(&reminder.next_fire_at) else {
            continue;
        };
        if next.with_timezone(&Local) > now {
            continue;
        }
        reminder.last_fired_at = Some(to_rfc3339(now));
        reminder.fire_count = reminder.fire_count.saturating_add(1);
        match reminder.schedule.clone() {
            ReminderSchedule::Once { .. } => {
                reminder.status = ReminderStatus::Done;
            }
            ReminderSchedule::Interval { every_minutes } => {
                reminder.next_fire_at =
                    to_rfc3339(now + ChronoDuration::minutes(every_minutes as i64));
            }
            ReminderSchedule::Daily { .. } => {
                reminder.next_fire_at = to_rfc3339(compute_next_fire_at(&reminder.schedule, now)?);
            }
        }
        reminder.updated_at = to_rfc3339(now);
        fired.push(reminder.clone());
    }
    if !fired.is_empty() {
        save_reminders_to_path(path, &reminders)?;
        for reminder in &fired {
            record_reminder_event("fired", reminder, None, Some("scheduler"));
        }
    }
    Ok(fired)
}

fn build_schedule(
    args: &CreateReminderArgs,
    now: DateTime<Local>,
) -> Result<ReminderSchedule, String> {
    match args.schedule_kind {
        CreateReminderKind::Once => {
            let parsed = if let Some(at) = args.at.as_deref() {
                parse_local_datetime(at)?
            } else if let Some(minutes) = args.delay_minutes {
                if !(MIN_INTERVAL_MINUTES..=MAX_INTERVAL_MINUTES).contains(&minutes) {
                    return Err(format!(
                        "delay_minutes must be between {MIN_INTERVAL_MINUTES} and {MAX_INTERVAL_MINUTES}"
                    ));
                }
                now + ChronoDuration::minutes(minutes as i64)
            } else {
                return Err("once reminder requires at or delay_minutes".to_string());
            };
            Ok(ReminderSchedule::Once {
                at: to_rfc3339(parsed),
            })
        }
        CreateReminderKind::Interval => {
            let every_minutes = args
                .interval_minutes
                .ok_or_else(|| "interval reminder requires interval_minutes".to_string())?;
            if !(MIN_INTERVAL_MINUTES..=MAX_INTERVAL_MINUTES).contains(&every_minutes) {
                return Err(format!(
                    "interval_minutes must be between {MIN_INTERVAL_MINUTES} and {MAX_INTERVAL_MINUTES}"
                ));
            }
            Ok(ReminderSchedule::Interval { every_minutes })
        }
        CreateReminderKind::Daily => {
            let daily_time = args
                .daily_time
                .as_deref()
                .ok_or_else(|| "daily reminder requires daily_time".to_string())?;
            let time = parse_daily_time(daily_time)?;
            Ok(ReminderSchedule::Daily {
                time: time.format("%H:%M").to_string(),
            })
        }
    }
}

fn compute_next_fire_at(
    schedule: &ReminderSchedule,
    now: DateTime<Local>,
) -> Result<DateTime<Local>, String> {
    match schedule {
        ReminderSchedule::Once { at } => parse_local_datetime(at),
        ReminderSchedule::Interval { every_minutes } => {
            Ok(now + ChronoDuration::minutes(*every_minutes as i64))
        }
        ReminderSchedule::Daily { time } => {
            let time = parse_daily_time(time)?;
            let today = now.date_naive();
            let today_at = localize(NaiveDateTime::new(today, time))?;
            if today_at > now {
                Ok(today_at)
            } else {
                localize(NaiveDateTime::new(today + ChronoDuration::days(1), time))
            }
        }
    }
}

fn parse_local_datetime(input: &str) -> Result<DateTime<Local>, String> {
    let trimmed = input.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(dt.with_timezone(&Local));
    }
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            return localize(naive);
        }
    }
    Err("time must be RFC3339 or YYYY-MM-DD HH:MM".to_string())
}

fn parse_daily_time(input: &str) -> Result<NaiveTime, String> {
    let trimmed = input.trim();
    for fmt in ["%H:%M:%S", "%H:%M"] {
        if let Ok(time) = NaiveTime::parse_from_str(trimmed, fmt) {
            return Ok(time);
        }
    }
    Err("daily_time must be HH:MM".to_string())
}

fn localize(naive: NaiveDateTime) -> Result<DateTime<Local>, String> {
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Ok(dt),
        LocalResult::Ambiguous(earliest, _) => Ok(earliest),
        LocalResult::None => Err("local time does not exist in this timezone".to_string()),
    }
}

fn clean_required(input: &str, max_chars: usize, field: &str) -> Result<String, String> {
    let cleaned = clean_optional(input, max_chars);
    if cleaned.is_empty() {
        Err(format!("{field} cannot be empty"))
    } else {
        Ok(cleaned)
    }
}

fn clean_optional(input: &str, max_chars: usize) -> String {
    let trimmed = input.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        trimmed.chars().take(max_chars).collect()
    }
}

fn new_reminder_id(now: DateTime<Local>) -> String {
    format!(
        "rem_{}_{:08x}",
        now.timestamp_millis(),
        rand::random::<u32>()
    )
}

fn to_rfc3339(dt: DateTime<Local>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn record_reminder_event(
    event: &str,
    reminder: &ReminderRecord,
    detail: Option<String>,
    ui_source: Option<&str>,
) {
    if cfg!(test) {
        return;
    }
    let path = match reminder_events_path() {
        Ok(path) => path,
        Err(e) => {
            tracing::warn!(error = %e, "reminder event path unavailable");
            return;
        }
    };
    let record = ReminderEventRecord::for_reminder(event, reminder, detail, ui_source);
    if let Err(e) = append_reminder_event(&path, &record) {
        tracing::warn!(error = %e, path = ?path, "reminder event write failed");
    }
}

fn record_reminder_failure(
    event: &str,
    source: Option<&str>,
    error: Option<String>,
    detail: Option<String>,
) {
    if cfg!(test) {
        return;
    }
    let path = match reminder_events_path() {
        Ok(path) => path,
        Err(e) => {
            tracing::warn!(error = %e, "reminder event path unavailable");
            return;
        }
    };
    let record = ReminderEventRecord::failure(event, source, error, detail);
    if let Err(e) = append_reminder_event(&path, &record) {
        tracing::warn!(error = %e, path = ?path, "reminder event write failed");
    }
}

fn record_store_failure(event: &str, path: &Path, error: &str) {
    if cfg!(test) {
        return;
    }
    let event_path = match reminder_events_path() {
        Ok(path) => path,
        Err(e) => {
            tracing::warn!(error = %e, "reminder event path unavailable");
            return;
        }
    };
    let record = ReminderEventRecord::store_failure(event, path, error);
    if let Err(e) = append_reminder_event(&event_path, &record) {
        tracing::warn!(error = %e, path = ?event_path, "reminder event write failed");
    }
}

impl ReminderEventRecord {
    fn for_reminder(
        event: &str,
        reminder: &ReminderRecord,
        detail: Option<String>,
        ui_source: Option<&str>,
    ) -> Self {
        Self {
            at: Local::now().to_rfc3339(),
            event: event.to_string(),
            reminder_id: Some(reminder.id.clone()),
            title: Some(reminder.title.clone()),
            status: Some(reminder.status),
            next_fire_at: Some(reminder.next_fire_at.clone()),
            source: Some(reminder.source.clone()),
            ui_source: ui_source.map(str::to_string),
            detail,
            error: None,
            store_path: None,
            file_size: None,
            head_bytes: None,
        }
    }

    fn failure(
        event: &str,
        source: Option<&str>,
        error: Option<String>,
        detail: Option<String>,
    ) -> Self {
        Self {
            at: Local::now().to_rfc3339(),
            event: event.to_string(),
            reminder_id: None,
            title: None,
            status: None,
            next_fire_at: None,
            source: source.map(str::to_string),
            ui_source: None,
            detail,
            error,
            store_path: None,
            file_size: None,
            head_bytes: None,
        }
    }

    fn store_failure(event: &str, path: &Path, error: &str) -> Self {
        let metadata = fs::metadata(path).ok();
        let head_bytes = fs::read(path).ok().map(|bytes| {
            bytes
                .iter()
                .take(8)
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join(" ")
        });
        Self {
            at: Local::now().to_rfc3339(),
            event: event.to_string(),
            reminder_id: None,
            title: None,
            status: None,
            next_fire_at: None,
            source: Some("store".to_string()),
            ui_source: None,
            detail: None,
            error: Some(error.to_string()),
            store_path: Some(path.to_string_lossy().into_owned()),
            file_size: metadata.map(|m| m.len()),
            head_bytes,
        }
    }
}

fn append_reminder_event(path: &Path, record: &ReminderEventRecord) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create reminder log dir failed: {e}"))?;
    }
    let line = serde_json::to_string(&record)
        .map_err(|e| format!("serialize reminder event failed: {e}"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("open reminder event log failed: {e}"))?;
    writeln!(file, "{line}").map_err(|e| format!("write reminder event log failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fixed_now() -> DateTime<Local> {
        parse_local_datetime("2026-05-21 10:00").unwrap()
    }

    #[test]
    fn create_interval_reminder_sets_next_fire_time() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("reminders.json");
        let reminder = create_reminder_in_path(
            &path,
            &CreateReminderArgs {
                title: "Drink water".into(),
                message: Some("Stand up and drink water".into()),
                schedule_kind: CreateReminderKind::Interval,
                at: None,
                delay_minutes: None,
                interval_minutes: Some(60),
                daily_time: None,
            },
            fixed_now(),
        )
        .unwrap();

        assert_eq!(reminder.title, "Drink water");
        assert_eq!(reminder.status, ReminderStatus::Active);
        assert!(reminder.next_fire_at.contains("11:00:00"));
        assert_eq!(load_reminders_from_path(&path).unwrap().len(), 1);
    }

    #[test]
    fn create_once_reminder_accepts_relative_delay() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("reminders.json");
        let reminder = create_reminder_in_path(
            &path,
            &CreateReminderArgs {
                title: "Drink water".into(),
                message: None,
                schedule_kind: CreateReminderKind::Once,
                at: None,
                delay_minutes: Some(3),
                interval_minutes: None,
                daily_time: None,
            },
            fixed_now(),
        )
        .unwrap();

        assert!(matches!(reminder.schedule, ReminderSchedule::Once { .. }));
        assert!(reminder.next_fire_at.contains("10:03:00"));
    }

    #[test]
    fn fire_due_interval_reminder_reschedules() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("reminders.json");
        create_reminder_in_path(
            &path,
            &CreateReminderArgs {
                title: "Drink water".into(),
                message: None,
                schedule_kind: CreateReminderKind::Interval,
                at: None,
                delay_minutes: None,
                interval_minutes: Some(60),
                daily_time: None,
            },
            fixed_now(),
        )
        .unwrap();

        let fired =
            fire_due_reminders_in_path(&path, parse_local_datetime("2026-05-21 11:00").unwrap())
                .unwrap();
        assert_eq!(fired.len(), 1);
        let stored = load_reminders_from_path(&path).unwrap();
        assert_eq!(stored[0].status, ReminderStatus::Active);
        assert_eq!(stored[0].fire_count, 1);
        assert!(stored[0].next_fire_at.contains("12:00:00"));
    }

    #[test]
    fn cancel_marks_reminder_cancelled() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("reminders.json");
        let reminder = create_reminder_in_path(
            &path,
            &CreateReminderArgs {
                title: "One shot".into(),
                message: None,
                schedule_kind: CreateReminderKind::Once,
                at: Some("2026-05-21 12:00".into()),
                delay_minutes: None,
                interval_minutes: None,
                daily_time: None,
            },
            fixed_now(),
        )
        .unwrap();

        let cancelled = cancel_reminder_in_path(&path, &reminder.id, "test").unwrap();
        assert_eq!(cancelled.status, ReminderStatus::Cancelled);
    }

    #[test]
    fn delete_reminder_removes_record() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("reminders.json");
        let reminder = create_reminder_in_path(
            &path,
            &CreateReminderArgs {
                title: "Coffee".into(),
                message: None,
                schedule_kind: CreateReminderKind::Once,
                at: Some("2026-05-21 12:00".into()),
                delay_minutes: None,
                interval_minutes: None,
                daily_time: None,
            },
            fixed_now(),
        )
        .unwrap();

        let deleted = delete_reminder_in_path(&path, &reminder.id, "test").unwrap();

        assert_eq!(deleted.id, reminder.id);
        assert!(load_reminders_from_path(&path).unwrap().is_empty());
    }

    #[test]
    fn complete_recurring_reminder_keeps_it_active() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("reminders.json");
        let reminder = create_reminder_in_path(
            &path,
            &CreateReminderArgs {
                title: "Drink water".into(),
                message: None,
                schedule_kind: CreateReminderKind::Interval,
                at: None,
                delay_minutes: None,
                interval_minutes: Some(60),
                daily_time: None,
            },
            fixed_now(),
        )
        .unwrap();

        let completed = complete_reminder_in_path(&path, &reminder.id, "test").unwrap();
        assert_eq!(completed.status, ReminderStatus::Active);
    }

    #[test]
    fn append_reminder_event_writes_jsonl() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("reminder_events.jsonl");
        let reminder = ReminderRecord {
            id: "rem_test".into(),
            title: "Drink water".into(),
            message: None,
            schedule: ReminderSchedule::Interval { every_minutes: 60 },
            next_fire_at: "2026-05-21T11:00:00+08:00".into(),
            status: ReminderStatus::Active,
            source: "agent".into(),
            created_at: "2026-05-21T10:00:00+08:00".into(),
            updated_at: "2026-05-21T10:00:00+08:00".into(),
            last_fired_at: None,
            fire_count: 0,
        };

        let event = ReminderEventRecord::for_reminder("created", &reminder, None, Some("test"));
        append_reminder_event(&path, &event).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        let record: ReminderEventRecord = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(record.event, "created");
        assert_eq!(record.reminder_id.as_deref(), Some("rem_test"));
        assert_eq!(record.ui_source.as_deref(), Some("test"));
    }
}
