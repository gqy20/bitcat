//! 陪伴时长统计：屏幕亮灭事件的本地事实源与聚合视图。
//!
//! 只记录"显示器亮/灭"这一个电源状态比特，不采集任何屏幕内容、不联网。
//! 事件追加到 `~/.bitcat/logs/screen_time_events.jsonl`（与 reminder_events
//! 同模式），按日分钟数与 24 小时分布由事件流**纯函数推导**——不是 tick
//! 累积，读取端随时可以重算。A4 金币掉落联动"屏幕亮着"的在场事实也从
//! 这里读取，保持账本与在场数据都是确定性推导。
//!
//! 交互关系：app 层 `screen_time` 模块注册 Windows 电源通知并调用
//! [`record_event`]；设置页与未来的 earnings 联动消费 [`aggregate_day`]。

use chrono::{DateTime, Duration, Local, NaiveDate, Timelike};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// 屏幕电源状态事件。变暗（Dim）由 app 层归一为亮，不单独记录。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScreenPowerEvent {
    ScreenOn,
    ScreenOff,
}

impl ScreenPowerEvent {
    pub fn is_on(self) -> bool {
        matches!(self, Self::ScreenOn)
    }
}

/// JSONL 里一行事件记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenTimeEventRecord {
    pub at: DateTime<Local>,
    pub event: ScreenPowerEvent,
}

/// 一天的陪伴时长聚合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenTimeDaySummary {
    pub date: NaiveDate,
    /// 屏幕亮着的总秒数。
    pub on_seconds: u64,
    /// 每小时亮着的秒数（索引 0..24）。
    pub hourly_seconds: [u64; 24],
}

impl ScreenTimeDaySummary {
    pub fn empty(date: NaiveDate) -> Self {
        Self {
            date,
            on_seconds: 0,
            hourly_seconds: [0; 24],
        }
    }

    /// 人类可读的分钟数（向下取整）。
    pub fn on_minutes(&self) -> u64 {
        self.on_seconds / 60
    }
}

/// 陪伴时长事件文件路径（`~/.bitcat/logs/screen_time_events.jsonl`）。
pub fn screen_time_events_path() -> Result<PathBuf, String> {
    Ok(crate::logging::log_dir()?.join("screen_time_events.jsonl"))
}

/// 记录一次电源状态事件；连续同向事件会被去重（电源通知偶发重放）。
pub fn record_event(event: ScreenPowerEvent) -> Result<(), String> {
    let path = screen_time_events_path()?;
    record_event_at_path(&path, event, Local::now())
}

/// 记录事件的路径注入版本，测试用。
pub fn record_event_at_path(
    path: &Path,
    event: ScreenPowerEvent,
    at: DateTime<Local>,
) -> Result<(), String> {
    if let Some(last) = last_event_at_path(path)?
        && last.event == event
    {
        return Ok(());
    }
    let record = ScreenTimeEventRecord { at, event };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create screen_time dir failed: {e}"))?;
    }
    let line = serde_json::to_string(&record)
        .map_err(|e| format!("serialize screen_time event failed: {e}"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("open screen_time event log failed: {e}"))?;
    writeln!(file, "{line}").map_err(|e| format!("write screen_time event failed: {e}"))
}

/// 读取全部事件（已按时间排序）；单行损坏跳过并在返回值里计数。
pub fn load_events() -> Result<(Vec<ScreenTimeEventRecord>, usize), String> {
    let path = screen_time_events_path()?;
    load_events_at_path(&path)
}

/// 从事件流判断屏幕当前是否亮着：最后一个事件为 On 即亮。
pub fn screen_on_from_events(events: &[ScreenTimeEventRecord]) -> bool {
    events
        .last()
        .map(|record| record.event.is_on())
        .unwrap_or(true)
}

/// 屏幕当前是否亮着；无事件或读取失败时乐观视为亮（不影响掉币主链路）。
pub fn screen_currently_on() -> bool {
    load_events()
        .map(|(events, _)| screen_on_from_events(&events))
        .unwrap_or(true)
}

pub fn load_events_at_path(path: &Path) -> Result<(Vec<ScreenTimeEventRecord>, usize), String> {
    if !path.exists() {
        return Ok((Vec::new(), 0));
    }
    let file = File::open(path).map_err(|e| format!("open screen_time events failed: {e}"))?;
    let mut events = Vec::new();
    let mut skipped = 0usize;
    for line in BufReader::new(file).lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<ScreenTimeEventRecord>(&line) {
            Ok(record) => events.push(record),
            Err(_) => skipped += 1,
        }
    }
    events.sort_by_key(|record| record.at);
    Ok((events, skipped))
}

/// 清理保留窗口之外的事件（原子替换），返回被删除的事件数。
pub fn cleanup_old_events(keep_days: u32) -> Result<usize, String> {
    let path = screen_time_events_path()?;
    cleanup_old_events_at_path(&path, keep_days, Local::now())
}

pub fn cleanup_old_events_at_path(
    path: &Path,
    keep_days: u32,
    now: DateTime<Local>,
) -> Result<usize, String> {
    let (events, _) = load_events_at_path(path)?;
    let cutoff = now - Duration::days(i64::from(keep_days));
    let kept: Vec<&ScreenTimeEventRecord> =
        events.iter().filter(|record| record.at >= cutoff).collect();
    let removed = events.len() - kept.len();
    if removed == 0 {
        return Ok(0);
    }
    let mut buf = String::new();
    for record in kept {
        let line = serde_json::to_string(record)
            .map_err(|e| format!("serialize screen_time event failed: {e}"))?;
        buf.push_str(&line);
        buf.push('\n');
    }
    crate::storage::write_file_atomically(path, buf.as_bytes())?;
    Ok(removed)
}

fn last_event_at_path(path: &Path) -> Result<Option<ScreenTimeEventRecord>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let file = File::open(path).map_err(|e| format!("open screen_time events failed: {e}"))?;
    let mut last = None;
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(record) = serde_json::from_str::<ScreenTimeEventRecord>(&line) {
            last = Some(record);
        }
    }
    Ok(last)
}

/// 聚合某一天屏幕亮着的时长（纯函数，不读文件）。
///
/// `until` 是统计上界——过去的一天取当天 24:00，今天取当前时刻。首个事件
/// 之前的状态视为灭；最后一次亮屏若未关，视为持续亮到 `until`；亮屏区间
/// 跨零点时按天切分。事件需按时间升序（[`load_events_at_path`] 已排序）。
pub fn aggregate_day(
    events: &[ScreenTimeEventRecord],
    date: NaiveDate,
    until: DateTime<Local>,
) -> ScreenTimeDaySummary {
    let day_start = date
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| naive.and_local_timezone(Local).single())
        .unwrap_or(until);
    let day_end = day_start + Duration::days(1);
    let window_end = if until < day_end { until } else { day_end };
    let mut summary = ScreenTimeDaySummary::empty(date);
    if window_end <= day_start {
        return summary;
    }

    let mut state_on = false;
    let mut seg_start = events.first().map(|record| record.at).unwrap_or(day_start);

    for record in events {
        if state_on {
            add_interval(&mut summary, day_start, window_end, seg_start, record.at);
        }
        state_on = record.event.is_on();
        seg_start = record.at;
    }
    if state_on {
        add_interval(&mut summary, day_start, window_end, seg_start, window_end);
    }
    summary
}

/// 把亮屏区间 `[start, end)` 与目标窗口取交集后按小时累计。
fn add_interval(
    summary: &mut ScreenTimeDaySummary,
    day_start: DateTime<Local>,
    window_end: DateTime<Local>,
    start: DateTime<Local>,
    end: DateTime<Local>,
) {
    let mut cursor = start.max(day_start);
    let end = end.min(window_end);
    while cursor < end {
        // ponytail: 小时边界用 truncate 到整点 +1h；DST 不存在的整点退化为 +1h，误差分钟级
        let next_hour = cursor
            .with_minute(0)
            .and_then(|c| c.with_second(0))
            .and_then(|c| c.with_nanosecond(0))
            .map(|h| h + Duration::hours(1))
            .unwrap_or(cursor + Duration::hours(1));
        let seg_end = next_hour.min(end);
        let seconds = (seg_end - cursor).num_seconds().max(0) as u64;
        summary.hourly_seconds[cursor.hour() as usize] += seconds;
        summary.on_seconds += seconds;
        cursor = seg_end;
    }
}

/// 聚合 `[from, to]` 闭区间内每天的陪伴时长。
pub fn aggregate_range(
    events: &[ScreenTimeEventRecord],
    from: NaiveDate,
    to: NaiveDate,
    now: DateTime<Local>,
) -> Vec<ScreenTimeDaySummary> {
    let mut days = Vec::new();
    let mut date = from;
    while date <= to {
        days.push(aggregate_day(events, date, now));
        date = match date.succ_opt() {
            Some(next) => next,
            None => break,
        };
    }
    days
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(date: &str, time: &str) -> DateTime<Local> {
        // 字面时间即本地墙上时钟：CI runner 在 UTC 也能得到与开发机相同的语义
        let naive =
            chrono::NaiveDateTime::parse_from_str(&format!("{date} {time}"), "%Y-%m-%d %H:%M")
                .expect("valid naive datetime");
        naive.and_local_timezone(Local).single().unwrap()
    }

    fn event(date: &str, time: &str, on: bool) -> ScreenTimeEventRecord {
        ScreenTimeEventRecord {
            at: at(date, time),
            event: if on {
                ScreenPowerEvent::ScreenOn
            } else {
                ScreenPowerEvent::ScreenOff
            },
        }
    }

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn aggregates_simple_on_off_pair() {
        let events = vec![
            event("2026-09-17", "10:00", true),
            event("2026-09-17", "11:30", false),
        ];
        let summary = aggregate_day(&events, date("2026-09-17"), at("2026-09-17", "23:00"));
        assert_eq!(summary.on_seconds, 90 * 60);
        assert_eq!(summary.hourly_seconds[10], 3600);
        assert_eq!(summary.hourly_seconds[11], 1800);
        assert_eq!(summary.on_minutes(), 90);
    }

    #[test]
    fn trailing_on_counts_until_now() {
        let events = vec![event("2026-09-17", "10:00", true)];
        let summary = aggregate_day(&events, date("2026-09-17"), at("2026-09-17", "11:00"));
        assert_eq!(summary.on_seconds, 3600);
    }

    #[test]
    fn interval_crossing_midnight_splits_across_days() {
        let events = vec![
            event("2026-09-17", "23:30", true),
            event("2026-09-18", "00:30", false),
        ];
        let day1 = aggregate_day(&events, date("2026-09-17"), at("2026-09-18", "12:00"));
        let day2 = aggregate_day(&events, date("2026-09-18"), at("2026-09-18", "12:00"));
        assert_eq!(day1.on_seconds, 30 * 60);
        assert_eq!(day1.hourly_seconds[23], 1800);
        assert_eq!(day2.on_seconds, 30 * 60);
        assert_eq!(day2.hourly_seconds[0], 1800);
    }

    #[test]
    fn on_before_window_counts_from_window_start() {
        // 前一天 23:00 亮屏且未关，窗口开始时应从 0:00 起算
        let events = vec![
            event("2026-09-16", "23:00", true),
            event("2026-09-17", "01:00", false),
        ];
        let summary = aggregate_day(&events, date("2026-09-17"), at("2026-09-17", "12:00"));
        assert_eq!(summary.on_seconds, 3600);
        assert_eq!(summary.hourly_seconds[0], 3600);
    }

    #[test]
    fn off_history_counts_nothing_before_first_on() {
        let events = vec![
            event("2026-09-17", "10:00", false),
            event("2026-09-17", "11:00", true),
        ];
        let summary = aggregate_day(&events, date("2026-09-17"), at("2026-09-17", "12:00"));
        assert_eq!(summary.on_seconds, 3600);
    }

    #[test]
    fn events_after_until_are_ignored() {
        let events = vec![
            event("2026-09-17", "10:00", true),
            event("2026-09-17", "11:00", false),
        ];
        let summary = aggregate_day(&events, date("2026-09-17"), at("2026-09-17", "10:30"));
        assert_eq!(summary.on_seconds, 1800);
    }

    #[test]
    fn interleaved_events_accumulate() {
        let events = vec![
            event("2026-09-17", "09:00", true),
            event("2026-09-17", "09:30", false),
            event("2026-09-17", "10:00", true),
            event("2026-09-17", "10:15", false),
        ];
        let summary = aggregate_day(&events, date("2026-09-17"), at("2026-09-17", "12:00"));
        assert_eq!(summary.on_seconds, 45 * 60);
    }

    #[test]
    fn store_dedups_consecutive_same_events() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screen_time_events.jsonl");
        record_event_at_path(&path, ScreenPowerEvent::ScreenOn, at("2026-09-17", "09:00")).unwrap();
        record_event_at_path(&path, ScreenPowerEvent::ScreenOn, at("2026-09-17", "09:01")).unwrap();
        record_event_at_path(
            &path,
            ScreenPowerEvent::ScreenOff,
            at("2026-09-17", "10:00"),
        )
        .unwrap();
        let (events, skipped) = load_events_at_path(&path).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn store_skips_malformed_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screen_time_events.jsonl");
        std::fs::write(&path, "not json\n").unwrap();
        record_event_at_path(&path, ScreenPowerEvent::ScreenOn, at("2026-09-17", "09:00")).unwrap();
        let (events, skipped) = load_events_at_path(&path).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(skipped, 1);
    }

    #[test]
    fn cleanup_drops_events_beyond_keep_days() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screen_time_events.jsonl");
        record_event_at_path(&path, ScreenPowerEvent::ScreenOn, at("2026-08-01", "09:00")).unwrap();
        record_event_at_path(
            &path,
            ScreenPowerEvent::ScreenOff,
            at("2026-09-16", "09:30"),
        )
        .unwrap();
        record_event_at_path(&path, ScreenPowerEvent::ScreenOn, at("2026-09-17", "10:00")).unwrap();
        let removed = cleanup_old_events_at_path(&path, 30, at("2026-09-17", "12:00")).unwrap();
        assert_eq!(removed, 1);
        let (events, _) = load_events_at_path(&path).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn aggregate_range_returns_each_day() {
        let events = vec![
            event("2026-09-16", "09:00", true),
            event("2026-09-16", "10:00", false),
        ];
        let days = aggregate_range(
            &events,
            date("2026-09-16"),
            date("2026-09-18"),
            at("2026-09-18", "12:00"),
        );
        assert_eq!(days.len(), 3);
        assert_eq!(days[0].on_seconds, 3600);
        assert_eq!(days[1].on_seconds, 0);
        assert_eq!(days[2].on_seconds, 0);
    }

    #[test]
    fn screen_on_from_events_reads_last_state() {
        let on = vec![
            event("2026-09-17", "09:00", false),
            event("2026-09-17", "09:01", true),
        ];
        let off = vec![
            event("2026-09-17", "09:00", true),
            event("2026-09-17", "12:00", false),
        ];
        assert!(screen_on_from_events(&on));
        assert!(!screen_on_from_events(&off));
        // 无事件：乐观视为亮
        assert!(screen_on_from_events(&[]));
    }
}
