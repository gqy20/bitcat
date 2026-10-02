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
use tauri::{AppHandle, Emitter, Manager};
use tracing::{debug, info, warn};

use bitcat_core::app_settings::AppSettings;
use bitcat_core::earnings::{coins_due, earned_today_cents};

const TICK_MS: u64 = 5_000;
/// 单次 tick 最多补发的金币数（基线对齐保护）。
const MAX_BURST: u64 = 5;
/// 批量掉落阈值：累积到这个数才发一次，把恒流细雨变成一阵一阵的节奏。
const BATCH_MIN: u64 = 6;
/// 单次掉落上限（与前端喷泉上限对齐）。
const MAX_DROP: u64 = 16;

/// 结算仪式补播：用户不在场（息屏）时暂存，回场当天补说一次。
static PENDING_SETTLEMENT: std::sync::Mutex<Option<(chrono::NaiveDate, String)>> =
    std::sync::Mutex::new(None);

/// 今日实际掉落的金币数（息屏跳过不计），设置页"已落"以此为准，
/// 与 `coins_due` 的理论值区分开，避免账本和视觉对不上。
static EMITTED_TODAY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static EMITTED_DAY: std::sync::Mutex<Option<chrono::NaiveDate>> = std::sync::Mutex::new(None);

fn note_emitted(day: chrono::NaiveDate, count: u64) {
    let mut last = EMITTED_DAY.lock().unwrap_or_else(|e| e.into_inner());
    if *last != Some(day) {
        *last = Some(day);
        EMITTED_TODAY.store(0, std::sync::atomic::Ordering::SeqCst);
    }
    EMITTED_TODAY.fetch_add(count, std::sync::atomic::Ordering::SeqCst);
}

fn emitted_today(day: chrono::NaiveDate) -> u64 {
    let last = EMITTED_DAY.lock().unwrap_or_else(|e| e.into_inner());
    if *last == Some(day) {
        EMITTED_TODAY.load(std::sync::atomic::Ordering::SeqCst)
    } else {
        0
    }
}

/// ④ 区卡片消费的账本视图。
#[derive(Debug, Clone, Serialize)]
pub struct EarningsSummary {
    /// 月薪已填且配置有效时为 true。
    pub enabled: bool,
    /// 今天已挣多少分（从上班时刻墙钟推导）。
    pub today_cents: u64,
    /// 今天应累计掉落的金币数。
    pub coins: u64,
    /// 今天实际掉落的金币数（息屏跳过不计），设置页展示以此为准。
    pub coins_emitted: u64,
}

/// 启动金币调度线程。
pub fn spawn_earnings_scheduler(app: AppHandle) {
    std::thread::Builder::new()
        .name("bitcat-earnings-scheduler".to_string())
        .spawn(move || {
            let mut last_due: Option<u64> = None;
            // 批量掉落的累积器：攒够 BATCH_MIN 才发一次事件
            let mut pending_coins: u64 = 0;
            // 结算补播：息屏（用户不在场）时武装，回场亮屏补说一次
            let mut settle_replay_armed = false;
            // 下班结算按天一次；None 表示今天还没结算
            let mut settled_for: Option<chrono::NaiveDate> = None;
            loop {
                if crate::shutdown::is_requested() {
                    debug!("earnings scheduler shutdown requested");
                    break;
                }
                let config = AppSettings::load().appearance.earnings;
                if config.monthly_salary_cents > 0 && config.validate().is_ok() {
                    let now = chrono::Local::now();
                    let due = coins_due(&config, now);
                    // A4.2 联动：掉币条件 = 工作时段 ∩ 屏幕亮着。息屏时基线
                    // 跟随（息屏时段的金币直接跳过，不补发）
                    let screen_on = bitcat_core::screen_time::screen_currently_on();
                    match last_due {
                        Some(prev) if due > prev => {
                            let gained = (due - prev).min(MAX_BURST);
                            last_due = Some(prev + gained);
                            // 息屏时段的增量直接跳过（不进 pending），语义与之前一致
                            if screen_on {
                                pending_coins += gained;
                            }
                            // 批量掉落：攒够 BATCH_MIN 才发一次，掉币是"时刻"不是"细雨"
                            if pending_coins >= BATCH_MIN {
                                let burst = pending_coins.min(MAX_DROP);
                                if let Err(e) = app.emit_to(
                                    "pet",
                                    "coin-drop",
                                    serde_json::json!({ "count": burst, "fountain": true }),
                                ) {
                                    warn!(error = %e, "coin drop emit failed");
                                } else {
                                    note_emitted(now.date_naive(), burst);
                                    pending_coins -= burst;
                                }
                            }
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

                    // A4.2 下班结算：过下班时刻当天一次——金币喷泉 + Happy 情绪
                    let today = now.date_naive();
                    let (_, work_end) = config.work_window(today);
                    if now >= work_end && settled_for != Some(today) {
                        settled_for = Some(today);
                        let today_cents = earned_today_cents(&config, now);
                        let yuan = format!("{:.2}", today_cents as f64 / 100.0);
                        let _ = app.emit_to(
                            "pet",
                            "coin-drop",
                            // fountain 让前端放宽单次上限，结算仪式不被夹半。
                            serde_json::json!({ "count": 15, "fountain": true }),
                        );
                        note_emitted(today, 15);
                        pending_coins = 0;
                        let speech = format!("今天赚了 ¥{yuan}，辛苦啦");
                        let bus = app.state::<crate::pet_event_bus::SharedPetEventBus>();
                        bus.emit(
                            &app,
                            bitcat_core::pet_event::PetEvent::React {
                                mood: bitcat_core::pet_event::PetMood::Happy,
                                speech: Some(speech.clone()),
                                ttl_ms: Some(10_000),
                            },
                        );
                        // 用户可能已经离席：暂存仪式，回场当天补播一次
                        *PENDING_SETTLEMENT.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some((today, speech));
                        settle_replay_armed = false;
                        info!(yuan = %yuan, "earnings day settled");
                    }

                    // 结算补播：结算后屏幕灭过（人不在）又亮（人回来）→ 补说一次；跨天作废
                    if let Some((day, text)) = PENDING_SETTLEMENT
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .clone()
                    {
                        if day != now.date_naive() {
                            *PENDING_SETTLEMENT.lock().unwrap_or_else(|e| e.into_inner()) = None;
                        } else if screen_on {
                            if settle_replay_armed {
                                let bus = app.state::<crate::pet_event_bus::SharedPetEventBus>();
                                bus.emit(
                                    &app,
                                    bitcat_core::pet_event::PetEvent::React {
                                        mood: bitcat_core::pet_event::PetMood::Happy,
                                        speech: Some(text),
                                        ttl_ms: Some(10_000),
                                    },
                                );
                                *PENDING_SETTLEMENT.lock().unwrap_or_else(|e| e.into_inner()) =
                                    None;
                                settle_replay_armed = false;
                                info!("earnings settlement replayed on return");
                            }
                        } else {
                            settle_replay_armed = true;
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
            coins_emitted: 0,
        };
    }
    EarningsSummary {
        enabled,
        today_cents: earned_today_cents(&config, now),
        coins: coins_due(&config, now),
        coins_emitted: emitted_today(now.date_naive()),
    }
}
