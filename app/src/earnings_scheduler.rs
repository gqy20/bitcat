//! 金币掉落调度：轮询墙钟推导的应得金币数，增量定向发给宠物窗口。
//!
//! 与 reminder_scheduler 同一哲学：无状态轮询 + 确定性推导。每 5 秒用
//! core 的 `coins_due` 算出"此刻应累计掉落的金币数"，与上次值取增量；
//! 重启/配置突变导致基线跳变时单次最多补发 [`MAX_BURST`] 枚，避免金币
//! 海爆发。跨零点后 due 自然回落，基线只跟不发。
//!
//! 交互关系：`coin-drop` 事件定向发给 pet 窗口的粒子系统；设置页 ④ 区
//! 通过 `cmd_earnings_summary` 展示今日已赚。

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::{debug, info, warn};

use bitcat_core::app_settings::AppSettings;
use bitcat_core::earnings::{coins_due, earned_today_cents};

const TICK_MS: u64 = 5_000;
/// 单次 tick 最多补发的金币数（基线对齐保护）。
const MAX_BURST: u64 = 5;

/// ④ 区卡片消费的账本视图。
#[derive(Debug, Clone, Serialize)]
pub struct EarningsSummary {
    /// 月薪已填且配置有效时为 true。
    pub enabled: bool,
    /// 今天已挣多少分（从上班时刻墙钟推导）。
    pub today_cents: u64,
    /// 今天应累计掉落的金币数。
    pub coins: u64,
}

/// 启动金币调度线程。
pub fn spawn_earnings_scheduler(app: AppHandle) {
    std::thread::Builder::new()
        .name("bitcat-earnings-scheduler".to_string())
        .spawn(move || {
            let mut last_due: Option<u64> = None;
            loop {
                if crate::shutdown::is_requested() {
                    debug!("earnings scheduler shutdown requested");
                    break;
                }
                let config = AppSettings::load().appearance.earnings;
                if config.monthly_salary_cents > 0 && config.validate().is_ok() {
                    let now = chrono::Local::now();
                    let due = coins_due(&config, now);
                    match last_due {
                        Some(prev) if due > prev => {
                            let burst = (due - prev).min(MAX_BURST);
                            if let Err(e) = app.emit_to(
                                "pet",
                                "coin-drop",
                                serde_json::json!({ "count": burst }),
                            ) {
                                warn!(error = %e, "coin drop emit failed");
                            }
                            last_due = Some(prev + burst);
                        }
                        Some(prev) if due < prev => {
                            // 跨零点或配置调小：基线回落，只跟不发
                            last_due = Some(due);
                        }
                        _ => {
                            // 首个 tick 建立基线，或无增量
                            if last_due.is_none() {
                                info!(due, "earnings baseline established");
                            }
                            last_due = Some(due);
                        }
                    }
                } else {
                    // 未启用/配置无效：保持基线为 None，重新启用时重建
                    last_due = None;
                }
                std::thread::sleep(std::time::Duration::from_millis(TICK_MS));
            }
        })
        .expect("failed to spawn earnings scheduler");
}

#[tauri::command]
pub fn cmd_earnings_summary() -> EarningsSummary {
    let config = AppSettings::load().appearance.earnings;
    let enabled = config.monthly_salary_cents > 0 && config.validate().is_ok();
    let now = chrono::Local::now();
    if !enabled {
        return EarningsSummary {
            enabled: false,
            today_cents: 0,
            coins: 0,
        };
    }
    EarningsSummary {
        enabled,
        today_cents: earned_today_cents(&config, now),
        coins: coins_due(&config, now),
    }
}
