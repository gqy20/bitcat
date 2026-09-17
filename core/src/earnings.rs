//! 上班金币掉落的账本：从墙钟确定性推导已赚金额。
//!
//! 一切金额从"月薪 ÷ (月工作日 × 每日工时)"的秒汇率与两个时间点推导，
//! **不做 tick 累积**——电脑睡眠、程序重启、中途关机都不会让数字漂移，
//! 同一时刻永远得到同一答案。第一版每天都是工作日（不做节假日历与午休
//! 扣除），稳定压倒精确（A4 设计决策）。
//!
//! 交互关系：app 层 `earnings_scheduler` 定时轮询 [`coins_due`] 的增量并发
//! `coin-drop` 事件；设置页 ④ 区展示 [`earned_today_cents`]。

use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};

/// 一枚金币代表的金额（分）。固定真实面额：1 角 = 1 枚。
pub const COIN_VALUE_CENTS: u64 = 10;

/// 金币账本配置。金额用分存储，避免浮点累计误差。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EarningsConfig {
    /// 月薪（分），大于 0。
    pub monthly_salary_cents: u64,
    /// 上班时间，当天分钟数（如 9:00 = 540）。
    pub work_start_minutes: u32,
    /// 下班时间，当天分钟数（如 18:00 = 1080），必须大于上班时间。
    pub work_end_minutes: u32,
    /// 每月工作日数，默认 21.75。
    pub workdays_per_month: f64,
}

impl Default for EarningsConfig {
    fn default() -> Self {
        Self {
            monthly_salary_cents: 0,
            work_start_minutes: 9 * 60,
            work_end_minutes: 18 * 60,
            workdays_per_month: 21.75,
        }
    }
}

impl EarningsConfig {
    /// 校验配置；salary 为 0 表示未启用，单独放行由调用方判断。
    pub fn validate(&self) -> Result<(), String> {
        if self.work_start_minutes >= self.work_end_minutes {
            return Err("上班时间必须早于下班时间".into());
        }
        if self.work_end_minutes > 24 * 60 {
            return Err("下班时间不能超过 24:00".into());
        }
        if !(1.0..=31.0).contains(&self.workdays_per_month) {
            return Err("每月工作日数必须在 1 到 31 之间".into());
        }
        Ok(())
    }

    /// 每秒挣多少分（f64，仅用于推导，不持久化）。
    fn cents_per_second(&self) -> f64 {
        let work_seconds_per_month = self.workdays_per_month
            * f64::from(self.work_end_minutes - self.work_start_minutes)
            * 60.0;
        self.monthly_salary_cents as f64 / work_seconds_per_month
    }

    /// 当天工作窗口 [start, end) 的本地时间。
    pub fn work_window(&self, date: NaiveDate) -> (DateTime<Local>, DateTime<Local>) {
        // DST 空洞的兜底：以本地墙上时钟为准，分钟偏移从零点起算
        let to_local = |minutes: u32| -> DateTime<Local> {
            let naive = date.and_hms_opt(0, 0, 0).unwrap() + Duration::minutes(i64::from(minutes));
            naive
                .and_local_timezone(Local)
                .single()
                .unwrap_or_else(|| Local.from_utc_datetime(&naive))
        };
        (
            to_local(self.work_start_minutes),
            to_local(self.work_end_minutes),
        )
    }
}

/// 推导 `[from, now)` 期间累计挣了多少分。
///
/// 纯函数：逐天取 `[max(当天上班, from), min(当天下班, now))` 的重叠秒数
/// 乘秒汇率，最后一次性取整。每天都是工作日（第一版设计）。
pub fn earned_cents_between(
    config: &EarningsConfig,
    from: DateTime<Local>,
    now: DateTime<Local>,
) -> u64 {
    if config.monthly_salary_cents == 0 || now <= from {
        return 0;
    }
    let rate = config.cents_per_second();
    let mut total_seconds = 0.0f64;
    let mut date = from.date_naive();
    let last_date = now.date_naive();
    while date <= last_date {
        let (window_start, window_end) = config.work_window(date);
        let start = window_start.max(from);
        let end = window_end.min(now);
        if end > start {
            total_seconds += (end - start).num_seconds() as f64;
        }
        date = match date.succ_opt() {
            Some(next) => next,
            None => break,
        };
    }
    (total_seconds * rate) as u64
}

/// 今天 0 点（或上班时刻，取较晚者）到现在挣了多少分。
pub fn earned_today_cents(config: &EarningsConfig, now: DateTime<Local>) -> u64 {
    let today = now.date_naive();
    let (window_start, _) = config.work_window(today);
    let from = window_start.min(now);
    earned_cents_between(config, from, now)
}

/// 现在应累计掉落的金币数（每 [`COIN_VALUE_CENTS`] 分一枚）。
pub fn coins_due(config: &EarningsConfig, now: DateTime<Local>) -> u64 {
    earned_today_cents(config, now) / COIN_VALUE_CENTS
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

    fn config(salary_cents: u64) -> EarningsConfig {
        EarningsConfig {
            monthly_salary_cents: salary_cents,
            ..Default::default()
        }
    }

    /// 月薪 3000 元（300000 分）、21.75 天 × 每天 9 小时（默认 9:00-18:00
    /// 不扣午休）→ 每秒约 0.426 分
    #[test]
    fn salary_3000_earns_expected_cents_per_second() {
        let cfg = config(300_000);
        let earned =
            earned_cents_between(&cfg, at("2026-09-17", "09:00"), at("2026-09-17", "09:01"));
        // 60s × 300000 / (21.75×540×60) ≈ 25.5 分 → 25
        assert_eq!(earned, 25);
    }

    #[test]
    fn full_workday_earns_daily_salary() {
        let cfg = config(300_000);
        let earned =
            earned_cents_between(&cfg, at("2026-09-17", "09:00"), at("2026-09-17", "18:00"));
        // 300000 / 21.75 ≈ 13793.1 分/天 → 13793
        assert_eq!(earned, 13_793);
    }

    #[test]
    fn interval_clamped_to_work_window() {
        let cfg = config(300_000);
        // 从 0 点算到 24 点，也只有工作窗口 8 小时计薪
        let earned =
            earned_cents_between(&cfg, at("2026-09-17", "00:00"), at("2026-09-18", "00:00"));
        assert_eq!(earned, 13_793);
    }

    #[test]
    fn partial_overlap_counts_pro_rata() {
        let cfg = config(300_000);
        // 8:00-10:30 → 只算 9:00-10:30 的 90 分钟（540 分钟工作窗的 1/6）
        let earned =
            earned_cents_between(&cfg, at("2026-09-17", "08:00"), at("2026-09-17", "10:30"));
        assert_eq!(earned, 2_298); // 13793 × 90/540 ≈ 2298.8
    }

    #[test]
    fn range_crossing_days_accumulates() {
        let cfg = config(300_000);
        // 17 日 12:00 → 18 日 12:00 = 半天 + 半天 = 一天薪
        let earned =
            earned_cents_between(&cfg, at("2026-09-17", "12:00"), at("2026-09-18", "12:00"));
        assert_eq!(earned, 13_793);
    }

    #[test]
    fn after_workday_is_capped() {
        let cfg = config(300_000);
        let at_6pm = at("2026-09-17", "18:00");
        let at_11pm = at("2026-09-17", "23:00");
        assert_eq!(
            earned_cents_between(&cfg, at("2026-09-17", "09:00"), at_11pm),
            earned_cents_between(&cfg, at("2026-09-17", "09:00"), at_6pm)
        );
    }

    #[test]
    fn zero_salary_earns_nothing() {
        let cfg = config(0);
        assert_eq!(
            earned_cents_between(&cfg, at("2026-09-17", "09:00"), at("2026-09-17", "18:00")),
            0
        );
        assert_eq!(coins_due(&cfg, at("2026-09-17", "12:00")), 0);
    }

    #[test]
    fn earned_today_from_work_start_when_earlier() {
        let cfg = config(300_000);
        // 中午启动：等价于从 9:00 起算
        let noon = at("2026-09-17", "12:00");
        assert_eq!(
            earned_today_cents(&cfg, noon),
            earned_cents_between(&cfg, at("2026-09-17", "09:00"), noon)
        );
    }

    #[test]
    fn coins_due_uses_dime_denomination() {
        let cfg = config(300_000);
        // 09:00-09:01 挣 25 分 → 2 枚（每 10 分一枚）
        assert_eq!(coins_due(&cfg, at("2026-09-17", "09:01")), 2);
    }

    #[test]
    fn validate_rejects_bad_windows() {
        let mut cfg = config(300_000);
        cfg.work_start_minutes = 18 * 60;
        cfg.work_end_minutes = 9 * 60;
        assert!(cfg.validate().is_err());
        cfg.work_start_minutes = 9 * 60;
        cfg.work_end_minutes = 25 * 60;
        assert!(cfg.validate().is_err());
        cfg.work_end_minutes = 18 * 60;
        cfg.workdays_per_month = 0.5;
        assert!(cfg.validate().is_err());
        cfg.workdays_per_month = 21.75;
        assert!(cfg.validate().is_ok());
    }
}
