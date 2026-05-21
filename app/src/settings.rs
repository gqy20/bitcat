//! 设置窗口 IPC 命令集：前端设置界面与后端配置之间的桥梁。
//!
//! 设计要点（见 plan `Settings_UI_Design_Plan`）：
//! - `~/.claude/settings.json` 仅读，**永不写入**；AI 覆盖写入 `app_settings.json`
//! - config/actions.yml / config/prompts.yml / config/user.yml 就地写回（注释会被覆盖，保存前自动备份 `.bak`）
//! - 保存后仅 set 原子 flag，由 gamepad_loop 下 tick 自动 reload（复用现有机制）
//!
//! 安全设计：API Key 在前后端之间不以明文传递；`AiView.has_effective_key` 仅返回布尔值，
//! 加载快照时用占位符代替真实 Key，防止 WebView2 DevTools 泄露凭证。
//!
//! 与以下模块交互：`ai_config`（AI 配置加载）、`action`（按键绑定）、
//! `prompts`（提示词）、`user_profile`（用户画像）、`app_settings`（持久化）、`token_tracker`（用量统计）。

use crate::commands::SharedWindowState;
use ai_pad_core::action::{ActionConfig, ActionDef, Defaults};
use ai_pad_core::app_settings::{AgentWatchSettings, AiOverride, AppSettings, AppearanceSettings};
use ai_pad_core::memory::{LongTermMemory, LongTermReviewEntry};
use ai_pad_core::prompts::PromptsConfig;
use ai_pad_core::reminder::{ListRemindersArgs, ReminderRecord, ReminderSchedule};
use ai_pad_core::token_tracker::{
    load_sessions, read_usage_records, token_sessions_path, token_usage_path, TokenRecord,
    TokenSession, TokenTotals,
};
use ai_pad_core::user_profile::UserProfile;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tracing::{info, warn};

const WINDOW_LABEL: &str = "settings";
const WINDOW_W: f64 = 1040.0;
const WINDOW_H: f64 = 720.0;

// ---- 窗口生命周期 ----

/// 切换设置窗口显示（托盘菜单 / 快捷键调用）
pub fn toggle_settings(app: &AppHandle) {
    match app.get_webview_window(WINDOW_LABEL) {
        Some(w) => match w.is_visible() {
            Ok(true) => {
                info!("[settings] 隐藏");
                let _ = w.hide();
            }
            Ok(false) => {
                info!("[settings] 显示");
                let _ = w.show();
                let _ = w.set_focus();
            }
            Err(e) => warn!(error = %e, "[settings] is_visible 错误"),
        },
        None => match create_settings_window(app) {
            Ok(w) => {
                let _ = w.set_focus();
                info!("[settings] 已创建并显示");
            }
            Err(e) => warn!(error = %e, "[settings] 创建失败"),
        },
    }
}

/// 按需创建设置窗口
fn create_settings_window(app: &AppHandle) -> Result<tauri::WebviewWindow, tauri::Error> {
    WebviewWindowBuilder::new(app, WINDOW_LABEL, WebviewUrl::App("settings.html".into()))
        .title("8Bit Cat 设置")
        .inner_size(WINDOW_W, WINDOW_H)
        .min_inner_size(860.0, 560.0)
        .decorations(false)
        .transparent(false)
        .shadow(true)
        .always_on_top(false)
        .skip_taskbar(false)
        .resizable(true)
        .center()
        .focused(true)
        .build()
}

// ---- 数据契约（前后端 JSON 交换结构）----

/// 初次打开设置界面时一次性返回的全量快照。
#[derive(Debug, Serialize)]
pub struct SettingsSnapshot {
    pub ai: AiView,
    pub user: UserProfile,
    pub actions: ActionsView,
    pub prompts: PromptsConfig,
    pub appearance: AppearanceSettings,
    pub agent_watch: AgentWatchSettings,
    pub about: AboutInfo,
    /// 按 config/buttons.yml 列出的所有可配置按键（有序，index 升序）
    pub button_catalog: Vec<ButtonCatalogItem>,
}

/// 按键元数据：用于设置界面展示"全部支持的按键 + 中文说明"。
#[derive(Debug, Serialize)]
pub struct ButtonCatalogItem {
    /// 按键主名，如 "Start"（同 config/actions.yml 里的 key）
    pub name: String,
    /// 中文别名，例如 "开始"；取 config/buttons.yml aliases 中第一个中文别名
    pub label: String,
    /// 硬件位置描述，例如 "中间偏右"
    pub position: String,
    /// 显示顺序（对应 config/buttons.yml 中的 button index）
    pub order: u32,
}

/// AI 视图：覆盖层原值 + 生效值（UI 可以在 input 显示 overlay，placeholder 显示 effective）。
#[derive(Debug, Serialize)]
pub struct AiView {
    /// 当前 overlay（app_settings.json 里存的值，可能为空）
    pub overlay: AiOverride,
    /// 实际生效值（合并 env / overlay / claude settings / 默认后的结果）
    pub effective: AiEffective,
    /// API Key 是否已配置（用于 UI 显示"已保存"占位符而不暴露明文）
    pub has_effective_key: bool,
}

#[derive(Debug, Serialize)]
pub struct AiEffective {
    pub base_url: String,
    pub model: String,
    pub max_tokens: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActionsView {
    pub defaults: Defaults,
    pub actions: HashMap<String, ActionDef>,
}

#[derive(Debug, Serialize)]
pub struct AboutInfo {
    pub version: String,
    pub app_settings_path: String,
    pub actions_yml_hint: String,
    pub prompts_yml_hint: String,
}

#[derive(Debug, Serialize)]
pub struct TokenStatsView {
    pub generated_at: String,
    pub today: TokenTotals,
    pub selected_model: Option<String>,
    pub models: Vec<TokenModelUsageView>,
    pub recent_sessions: Vec<TokenSessionView>,
    pub paths: TokenStatsPaths,
}

#[derive(Debug, Serialize)]
pub struct TokenModelUsageView {
    pub model: String,
    pub record_count: u32,
    pub total_tokens: u64,
}

#[derive(Debug, Serialize)]
pub struct MemoryReviewView {
    pub generated_at: String,
    pub total_entries: usize,
    pub entries: Vec<LongTermReviewEntry>,
    pub markdown: String,
}

#[derive(Debug, Serialize)]
pub struct ReminderReviewView {
    pub generated_at: String,
    pub total_entries: usize,
    pub active_count: usize,
    pub store_path: String,
    pub events_path: String,
    pub entries: Vec<ReminderView>,
}

#[derive(Debug, Serialize)]
pub struct ReminderView {
    pub id: String,
    pub title: String,
    pub message: Option<String>,
    pub status: String,
    pub schedule_label: String,
    pub next_fire_at: String,
    pub last_fired_at: Option<String>,
    pub fire_count: u32,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct ResourceUsageView {
    pub generated_at: String,
    pub process_cpu_percent: f64,
    pub process_memory_mb: f64,
    pub system_memory_used_mb: f64,
    pub system_memory_total_mb: f64,
    pub system_memory_percent: f64,
}

#[derive(Debug, Serialize)]
pub struct TokenStatsPaths {
    pub usage_jsonl: String,
    pub sessions_json: String,
}

#[derive(Debug, Serialize)]
pub struct TokenSessionView {
    pub session_id: String,
    pub started_at: String,
    pub ended_at: String,
    pub models: Vec<String>,
    pub record_count: u32,
    pub elapsed_ms_total: u64,
    pub total_tokens: u64,
    pub chat_total_tokens: u64,
    pub vision_total_tokens: u64,
    pub screen_summary_total_tokens: u64,
    pub memory_aggregation_total_tokens: u64,
}

#[derive(Debug, Deserialize)]
pub struct AppearanceInput {
    pub always_on_top: bool,
    pub default_collapsed: bool,
    pub tts_enabled: bool,
    #[serde(default = "default_true")]
    pub notification_sound_enabled: bool,
    #[serde(default = "default_true")]
    pub notification_sound_reminder: bool,
    #[serde(default = "default_true")]
    pub notification_sound_agent_watch: bool,
    #[serde(default = "default_true")]
    pub notification_sound_skip_agent_tts: bool,
    #[serde(default)]
    pub reminder_ai_personalization_enabled: bool,
    #[serde(default = "default_reminder_ai_timeout_ms")]
    pub reminder_ai_timeout_ms: u64,
    pub global_shortcut: String,
    #[serde(default = "default_screenshot_interval_sec")]
    pub screenshot_interval_sec: u64,
    #[serde(default)]
    pub pet_asset_url: Option<String>,
    #[serde(default = "default_true")]
    pub screenshot_show_bubble: bool,
}

#[derive(Debug, Deserialize)]
pub struct AgentWatchInput {
    pub enabled: bool,
    pub away_nudge_enabled: bool,
    #[serde(default = "default_first_nudge_after_sec")]
    pub first_nudge_after_sec: u64,
    #[serde(default = "default_repeat_nudge_after_min")]
    pub repeat_nudge_after_min: u64,
    pub waiting_alert: bool,
    pub done_alert: bool,
    pub use_tts: bool,
    #[serde(default = "default_true")]
    pub remote_view_enabled: bool,
    #[serde(default = "default_true")]
    pub remote_install_enabled: bool,
}

fn default_screenshot_interval_sec() -> u64 {
    30
}
fn default_reminder_ai_timeout_ms() -> u64 {
    3_000
}
fn default_first_nudge_after_sec() -> u64 {
    30
}
fn default_repeat_nudge_after_min() -> u64 {
    8
}
fn default_true() -> bool {
    true
}

fn token_session_view(session: &TokenSession) -> TokenSessionView {
    let total_tokens = session
        .chat_total_tokens
        .saturating_add(session.vision_total_tokens)
        .saturating_add(session.screen_summary_total_tokens)
        .saturating_add(session.memory_aggregation_total_tokens);

    TokenSessionView {
        session_id: session.session_id.clone(),
        started_at: session.started_at.clone(),
        ended_at: session.ended_at.clone(),
        models: session.models.clone(),
        record_count: session.record_count,
        elapsed_ms_total: session.elapsed_ms_total,
        total_tokens,
        chat_total_tokens: session.chat_total_tokens,
        vision_total_tokens: session.vision_total_tokens,
        screen_summary_total_tokens: session.screen_summary_total_tokens,
        memory_aggregation_total_tokens: session.memory_aggregation_total_tokens,
    }
}

// ---- 命令 ----

/// 托盘菜单 / 快捷键触发的设置窗口显示命令。
#[tauri::command]
pub async fn cmd_settings_show(app: AppHandle) -> Result<(), String> {
    toggle_settings(&app);
    Ok(())
}

/// 隐藏设置窗口（不销毁，保留 WebView 状态）。
#[tauri::command]
pub async fn cmd_settings_hide(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 关闭（隐藏）设置窗口，行为同 `cmd_settings_hide`。
#[tauri::command]
pub async fn cmd_settings_close(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 前端调试日志桥
#[tauri::command]
pub async fn cmd_settings_log(msg: String) -> Result<(), String> {
    if !ai_pad_core::logging::frontend_log_allowed(
        "settings",
        std::time::Duration::from_millis(120),
    ) {
        return Ok(());
    }
    let preview = ai_pad_core::logging::log_preview(&msg, 80);
    info!(
        msg_chars = msg.chars().count(),
        msg_preview = %preview,
        "settings frontend log"
    );
    Ok(())
}

/// 读取全量配置快照
#[tauri::command]
pub async fn cmd_settings_load() -> Result<SettingsSnapshot, String> {
    let overlay = AppSettings::load();

    // AI effective：尝试加载，失败则用默认占位
    let (effective, has_key) = match ai_pad_core::ai_config::AiConfig::load() {
        Ok(cfg) => {
            let mt = cfg.max_tokens();
            let has_key = !cfg.api_key.is_empty();
            (
                AiEffective {
                    base_url: cfg.base_url,
                    model: cfg.model,
                    max_tokens: mt,
                },
                has_key,
            )
        }
        Err(_) => (
            AiEffective {
                base_url: "https://api.anthropic.com".into(),
                model: "claude-sonnet-4-20250514".into(),
                max_tokens: 256_000,
            },
            false,
        ),
    };

    // actions / prompts：失败回退到内置默认
    let action_cfg = ActionConfig::load("config/actions.yml").unwrap_or_else(|e| {
        warn!(error = %e, "加载 config/actions.yml 失败，使用内置默认");
        ActionConfig::default_builtin()
    });
    let prompts_cfg = PromptsConfig::load();

    let app_settings_path = ai_pad_core::app_settings::settings_path()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "<unknown>".into());

    // 读取 buttons.yml 提供可配置按键全集
    let button_catalog = match ai_pad_core::config::ButtonConfig::load("config/buttons.yml") {
        Ok(btn_cfg) => {
            let mut items: Vec<ButtonCatalogItem> = btn_cfg
                .buttons
                .iter()
                .map(|(idx, info)| ButtonCatalogItem {
                    name: info.name.clone(),
                    label: pick_cn_label(&info.aliases),
                    position: info.position.clone(),
                    order: *idx,
                })
                .collect();
            items.sort_by_key(|it| it.order);
            items
        }
        Err(e) => {
            warn!(error = %e, "加载 config/buttons.yml 失败，按键列表为空");
            Vec::new()
        }
    };

    Ok(SettingsSnapshot {
        ai: AiView {
            overlay: overlay.ai,
            effective,
            has_effective_key: has_key,
        },
        user: UserProfile::load(),
        actions: ActionsView {
            defaults: action_cfg.defaults,
            actions: action_cfg.actions,
        },
        prompts: prompts_cfg,
        appearance: overlay.appearance,
        agent_watch: overlay.agent_watch,
        about: AboutInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            app_settings_path,
            actions_yml_hint: "config/actions.yml（exe 同目录/config/ 或 CWD/config/）".into(),
            prompts_yml_hint: "config/prompts.yml（exe 同目录/config/ 或 CWD/config/）".into(),
        },
        button_catalog,
    })
}

/// 返回 Token 用量统计：今日汇总 + 模型列表 + 最近 10 个 session 明细。
#[tauri::command]
pub async fn cmd_get_token_stats(model: Option<String>) -> Result<TokenStatsView, String> {
    let usage_path = token_usage_path()?;
    let sessions_path = token_sessions_path()?;
    let today = chrono::Local::now().date_naive();
    let selected_model = model.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed == "__all" {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    let records = read_usage_records(&usage_path)?;
    let today_records = records_for_date(&records, today);
    let today_totals = totals_for_records(today_records.iter().copied().filter(|record| {
        selected_model
            .as_ref()
            .map(|model| record.model == *model)
            .unwrap_or(true)
    }));
    let models = model_usage_options(today_records.iter().copied());
    let sessions = load_sessions(&sessions_path)?;
    let recent_sessions = sessions
        .sessions
        .iter()
        .filter(|session| {
            selected_model
                .as_ref()
                .map(|model| session.models.iter().any(|m| m == model))
                .unwrap_or(true)
        })
        .take(10)
        .map(token_session_view)
        .collect();

    Ok(TokenStatsView {
        generated_at: chrono::Local::now().to_rfc3339(),
        today: today_totals,
        selected_model,
        models,
        recent_sessions,
        paths: TokenStatsPaths {
            usage_jsonl: usage_path.to_string_lossy().into_owned(),
            sessions_json: sessions_path.to_string_lossy().into_owned(),
        },
    })
}

fn records_for_date(records: &[TokenRecord], date: chrono::NaiveDate) -> Vec<&TokenRecord> {
    let date_prefix = date.format("%Y-%m-%d").to_string();
    records
        .iter()
        .filter(|record| record.timestamp.starts_with(&date_prefix))
        .collect()
}

fn totals_for_records<'a>(records: impl Iterator<Item = &'a TokenRecord>) -> TokenTotals {
    let mut totals = TokenTotals::default();
    for record in records {
        totals.add_record(record);
    }
    totals
}

fn model_usage_options<'a>(
    records: impl Iterator<Item = &'a TokenRecord>,
) -> Vec<TokenModelUsageView> {
    let mut by_model: HashMap<String, TokenModelUsageView> = HashMap::new();
    for record in records {
        let entry = by_model
            .entry(record.model.clone())
            .or_insert_with(|| TokenModelUsageView {
                model: record.model.clone(),
                record_count: 0,
                total_tokens: 0,
            });
        entry.record_count = entry.record_count.saturating_add(1);
        entry.total_tokens = entry.total_tokens.saturating_add(record.total_tokens);
    }
    let mut models: Vec<_> = by_model.into_values().collect();
    models.sort_by(|a, b| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then_with(|| a.model.cmp(&b.model))
    });
    models
}

/// Return a reviewable view of grep-first long-term memory.
#[tauri::command]
pub async fn cmd_get_memory_review(limit: Option<usize>) -> Result<MemoryReviewView, String> {
    let store = LongTermMemory::load();
    Ok(memory_review_view(
        &store,
        limit.unwrap_or(20).clamp(1, 100),
    ))
}

#[tauri::command]
pub async fn cmd_get_reminders(
    include_inactive: Option<bool>,
) -> Result<ReminderReviewView, String> {
    let reminders = ai_pad_core::reminder::list_reminders(&ListRemindersArgs {
        include_inactive: include_inactive.unwrap_or(true),
    })?;
    Ok(reminder_review_view(reminders))
}

#[tauri::command]
pub async fn cmd_cancel_reminder(id: String) -> Result<ReminderReviewView, String> {
    ai_pad_core::reminder::cancel_reminder_with_source(&id, "settings")?;
    cmd_get_reminders(Some(true)).await
}

#[tauri::command]
pub async fn cmd_delete_reminder(id: String) -> Result<ReminderReviewView, String> {
    ai_pad_core::reminder::delete_reminder_with_source(&id, "settings")?;
    cmd_get_reminders(Some(true)).await
}

#[tauri::command]
pub async fn cmd_complete_reminder(id: String) -> Result<ReminderReviewView, String> {
    ai_pad_core::reminder::complete_reminder_with_source(&id, "settings")?;
    cmd_get_reminders(Some(true)).await
}

#[tauri::command]
pub async fn cmd_snooze_reminder(
    id: String,
    minutes: Option<u32>,
) -> Result<ReminderReviewView, String> {
    ai_pad_core::reminder::snooze_reminder_with_source(&id, minutes.unwrap_or(10), "settings")?;
    cmd_get_reminders(Some(true)).await
}

#[tauri::command]
pub async fn cmd_get_resource_usage() -> Result<ResourceUsageView, String> {
    resource_usage_snapshot()
}

/// Delete one long-term memory entry by its stable id.
#[tauri::command]
pub async fn cmd_delete_memory_entry(
    id: String,
    limit: Option<usize>,
) -> Result<MemoryReviewView, String> {
    let mut store = LongTermMemory::load();
    if !store.delete_entry_by_id(&id) {
        return Err(format!("memory entry id {id} does not exist"));
    }
    store.save()?;
    Ok(memory_review_view(
        &store,
        limit.unwrap_or(20).clamp(1, 100),
    ))
}

fn memory_review_view(store: &LongTermMemory, limit: usize) -> MemoryReviewView {
    MemoryReviewView {
        generated_at: chrono::Local::now().to_rfc3339(),
        total_entries: store.entries.iter().filter(|entry| !entry.deleted).count(),
        entries: store.review_entries(limit),
        markdown: store.review_markdown(limit),
    }
}

fn reminder_review_view(reminders: Vec<ReminderRecord>) -> ReminderReviewView {
    let total_entries = reminders.len();
    let active_count = reminders
        .iter()
        .filter(|r| r.status == ai_pad_core::reminder::ReminderStatus::Active)
        .count();
    let events_path = ai_pad_core::reminder::reminder_events_path()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|e| format!("<unavailable: {e}>"));
    let store_path = ai_pad_core::reminder::reminder_store_path()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|e| format!("<unavailable: {e}>"));
    ReminderReviewView {
        generated_at: chrono::Local::now().to_rfc3339(),
        total_entries,
        active_count,
        store_path,
        events_path,
        entries: reminders.into_iter().map(reminder_view).collect(),
    }
}

fn reminder_view(reminder: ReminderRecord) -> ReminderView {
    ReminderView {
        id: reminder.id,
        title: reminder.title,
        message: reminder.message,
        status: reminder.status.as_str().to_string(),
        schedule_label: schedule_label(&reminder.schedule),
        next_fire_at: reminder.next_fire_at,
        last_fired_at: reminder.last_fired_at,
        fire_count: reminder.fire_count,
        source: reminder.source,
    }
}

fn schedule_label(schedule: &ReminderSchedule) -> String {
    match schedule {
        ReminderSchedule::Once { at } => format!("一次 · {at}"),
        ReminderSchedule::Interval { every_minutes } => {
            if *every_minutes % 60 == 0 {
                format!("每 {} 小时", every_minutes / 60)
            } else {
                format!("每 {every_minutes} 分钟")
            }
        }
        ReminderSchedule::Daily { time } => format!("每天 · {time}"),
    }
}

#[derive(Debug, Clone, Copy)]
struct ResourceSample {
    at: Instant,
    process_kernel_100ns: u64,
    process_user_100ns: u64,
}

fn resource_sample_slot() -> &'static Mutex<Option<ResourceSample>> {
    static SLOT: OnceLock<Mutex<Option<ResourceSample>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[cfg(windows)]
fn filetime_to_u64(ft: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

#[cfg(windows)]
fn current_process_times_100ns() -> Result<(u64, u64), String> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let mut creation = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    // Safety: GetCurrentProcess returns a pseudo handle owned by the process; all FILETIME pointers are valid out params.
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };
    if ok == 0 {
        return Err("GetProcessTimes failed".into());
    }
    Ok((filetime_to_u64(kernel), filetime_to_u64(user)))
}

#[cfg(windows)]
fn current_process_memory_mb() -> Result<f64, String> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        PageFaultCount: 0,
        PeakWorkingSetSize: 0,
        WorkingSetSize: 0,
        QuotaPeakPagedPoolUsage: 0,
        QuotaPagedPoolUsage: 0,
        QuotaPeakNonPagedPoolUsage: 0,
        QuotaNonPagedPoolUsage: 0,
        PagefileUsage: 0,
        PeakPagefileUsage: 0,
    };
    // Safety: pseudo process handle is valid; counters points to an initialized buffer with correct cb size.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    };
    if ok == 0 {
        return Err("GetProcessMemoryInfo failed".into());
    }
    Ok(counters.WorkingSetSize as f64 / 1024.0 / 1024.0)
}

#[cfg(windows)]
fn system_memory_stats() -> Result<(f64, f64, f64), String> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        dwMemoryLoad: 0,
        ullTotalPhys: 0,
        ullAvailPhys: 0,
        ullTotalPageFile: 0,
        ullAvailPageFile: 0,
        ullTotalVirtual: 0,
        ullAvailVirtual: 0,
        ullAvailExtendedVirtual: 0,
    };
    // Safety: status is an initialized MEMORYSTATUSEX with dwLength set as required by the Win32 API.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    if ok == 0 {
        return Err("GlobalMemoryStatusEx failed".into());
    }
    let total = status.ullTotalPhys as f64 / 1024.0 / 1024.0;
    let avail = status.ullAvailPhys as f64 / 1024.0 / 1024.0;
    let used = (total - avail).max(0.0);
    Ok((used, total, status.dwMemoryLoad as f64))
}

#[cfg(windows)]
fn process_cpu_percent() -> Result<f64, String> {
    let (kernel, user) = current_process_times_100ns()?;
    let now = Instant::now();
    let mut guard = resource_sample_slot()
        .lock()
        .map_err(|_| "resource sample lock poisoned".to_string())?;
    let Some(prev) = *guard else {
        *guard = Some(ResourceSample {
            at: now,
            process_kernel_100ns: kernel,
            process_user_100ns: user,
        });
        return Ok(0.0);
    };
    *guard = Some(ResourceSample {
        at: now,
        process_kernel_100ns: kernel,
        process_user_100ns: user,
    });
    let elapsed = now.duration_since(prev.at).as_secs_f64();
    if elapsed <= 0.0 {
        return Ok(0.0);
    }
    let process_delta_100ns = kernel.saturating_add(user).saturating_sub(
        prev.process_kernel_100ns
            .saturating_add(prev.process_user_100ns),
    );
    let cpu_seconds = process_delta_100ns as f64 / 10_000_000.0;
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as f64;
    Ok(((cpu_seconds / elapsed) * 100.0 / cores).clamp(0.0, 100.0))
}

fn resource_usage_snapshot() -> Result<ResourceUsageView, String> {
    #[cfg(windows)]
    {
        let (system_memory_used_mb, system_memory_total_mb, system_memory_percent) =
            system_memory_stats()?;
        Ok(ResourceUsageView {
            generated_at: chrono::Local::now().to_rfc3339(),
            process_cpu_percent: process_cpu_percent()?,
            process_memory_mb: current_process_memory_mb()?,
            system_memory_used_mb,
            system_memory_total_mb,
            system_memory_percent,
        })
    }
    #[cfg(not(windows))]
    {
        Ok(ResourceUsageView {
            generated_at: chrono::Local::now().to_rfc3339(),
            process_cpu_percent: 0.0,
            process_memory_mb: 0.0,
            system_memory_used_mb: 0.0,
            system_memory_total_mb: 0.0,
            system_memory_percent: 0.0,
        })
    }
}

/// 从 aliases 中挑选最合适的中文标签（优先含中文的条目；退化为第一个；空则用 name 当 fallback 由调用方决定）
fn pick_cn_label(aliases: &[String]) -> String {
    for a in aliases {
        if a.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)) {
            return a.clone();
        }
    }
    aliases.first().cloned().unwrap_or_default()
}

/// 保存 AI 覆盖层（仅非空字段写入 overlay；空串视为"清除覆盖，回退下一层"）。
///
/// API Key 仅做非空校验，不限制格式/长度。
#[tauri::command]
pub async fn cmd_settings_save_ai(payload: AiOverride) -> Result<(), String> {
    // 非空校验：如果用户填了 api_key，不能是纯空白；否则视为"清除覆盖"
    if let Some(ref k) = payload.api_key {
        if k.trim().is_empty() {
            // 当成清空：写 None 到 overlay
        }
    }
    let mut s = AppSettings::load();
    s.ai = AiOverride {
        api_key: payload
            .api_key
            .and_then(|k| if k.trim().is_empty() { None } else { Some(k) }),
        base_url: payload
            .base_url
            .and_then(|v| if v.trim().is_empty() { None } else { Some(v) }),
        model: payload
            .model
            .and_then(|v| if v.trim().is_empty() { None } else { Some(v) }),
        max_tokens: payload.max_tokens,
    };
    s.save()?;
    info!("[settings] AI 覆盖层已保存");
    Ok(())
}

/// 保存按键绑定（写回 actions.yml + 触发 reload flag）
#[tauri::command]
pub async fn cmd_settings_save_actions(app: AppHandle, payload: ActionsView) -> Result<(), String> {
    let cfg = ActionConfig {
        defaults: payload.defaults,
        actions: payload.actions,
    };
    cfg.save("config/actions.yml")?;
    // 触发 gamepad_loop 热重载
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    ws.config_reload.store(true, Ordering::SeqCst);
    info!(
        actions = cfg.actions.len(),
        "[settings] config/actions.yml 已保存并触发 reload"
    );
    Ok(())
}

/// 保存 Prompt 配置（写回 prompts.yml）
#[tauri::command]
pub async fn cmd_settings_save_prompts(payload: PromptsConfig) -> Result<(), String> {
    payload.save()?;
    info!("[settings] config/prompts.yml 已保存");
    Ok(())
}

/// 保存用户配置（写回 config/user.yml）
#[tauri::command]
pub async fn cmd_settings_save_user(payload: UserProfile) -> Result<(), String> {
    payload.save()?;
    info!("[settings] config/user.yml 已保存");
    Ok(())
}

/// 保存外观设置（写入 app_settings.json 的 appearance 段）
#[tauri::command]
pub async fn cmd_settings_save_appearance(
    app: AppHandle,
    payload: AppearanceInput,
) -> Result<(), String> {
    let mut s = AppSettings::load();
    // 夹紧范围：不允许 < 5s（防止刷屏 vision API）
    let interval = payload.screenshot_interval_sec.clamp(5, 3600);
    s.appearance = AppearanceSettings {
        always_on_top: payload.always_on_top,
        default_collapsed: payload.default_collapsed,
        tts_enabled: payload.tts_enabled,
        notification_sound_enabled: payload.notification_sound_enabled,
        notification_sound_reminder: payload.notification_sound_reminder,
        notification_sound_agent_watch: payload.notification_sound_agent_watch,
        notification_sound_skip_agent_tts: payload.notification_sound_skip_agent_tts,
        reminder_ai_personalization_enabled: payload.reminder_ai_personalization_enabled,
        reminder_ai_timeout_ms: payload.reminder_ai_timeout_ms.clamp(500, 10_000),
        global_shortcut: payload.global_shortcut,
        screenshot_interval_sec: interval,
        screenshot_show_bubble: payload.screenshot_show_bubble,
        pet_asset_url: payload.pet_asset_url.and_then(|value| {
            let trimmed = value.trim().trim_end_matches('/').to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }),
        pet_position: s.appearance.pet_position,
    };
    s.save()?;

    // 立即生效：同步 SharedWindowState.always_on_top + 真实窗口属性
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    ws.always_on_top
        .store(s.appearance.always_on_top, Ordering::SeqCst);
    for label in ["pet", "pet-mini", "pet-snap"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.set_always_on_top(s.appearance.always_on_top);
        }
    }
    if let Some(w) = app.get_webview_window("pet") {
        let _ = w.emit(
            "pet-asset-config-changed",
            s.appearance.pet_asset_url.clone(),
        );
    }
    info!(appearance = ?s.appearance, "[settings] 外观设置已保存");
    Ok(())
}

/// 保存 Claude Code Agent 看管设置。
#[tauri::command]
pub async fn cmd_settings_save_agent_watch(payload: AgentWatchInput) -> Result<(), String> {
    let mut s = AppSettings::load();
    s.agent_watch = AgentWatchSettings {
        enabled: payload.enabled,
        away_nudge_enabled: payload.away_nudge_enabled,
        first_nudge_after_sec: payload.first_nudge_after_sec.clamp(10, 3600),
        repeat_nudge_after_min: payload.repeat_nudge_after_min.clamp(1, 240),
        waiting_alert: payload.waiting_alert,
        done_alert: payload.done_alert,
        use_tts: payload.use_tts,
        remote_view_enabled: payload.remote_view_enabled,
        remote_install_enabled: payload.remote_install_enabled,
    };
    s.save()?;
    info!(agent_watch = ?s.agent_watch, "[settings] Agent 看管设置已保存");
    Ok(())
}

/// 重置某一分类为内置默认（actions / prompts / appearance / ai / user）
#[tauri::command]
pub async fn cmd_settings_reset(category: String) -> Result<(), String> {
    match category.as_str() {
        "actions" => {
            ActionConfig::default_builtin().save("config/actions.yml")?;
        }
        "prompts" => {
            PromptsConfig::default_builtin().save()?;
        }
        "appearance" => {
            let mut s = AppSettings::load();
            s.appearance = AppearanceSettings::default();
            s.save()?;
        }
        "agent_watch" => {
            let mut s = AppSettings::load();
            s.agent_watch = AgentWatchSettings::default();
            s.save()?;
        }
        "ai" => {
            let mut s = AppSettings::load();
            s.ai = AiOverride::default();
            s.save()?;
        }
        "user" => {
            UserProfile::default_builtin().save()?;
        }
        other => return Err(format!("未知重置分类: {other}")),
    }
    info!(category = %category, "[settings] 已重置为默认");
    Ok(())
}

/// 通知后端应用配置（set config_reload flag，gamepad_loop 下 tick 会自动读取）
#[tauri::command]
pub async fn cmd_settings_apply(app: AppHandle) -> Result<(), String> {
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    ws.config_reload.store(true, Ordering::SeqCst);
    info!("[settings] config_reload 已触发");
    Ok(())
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_serializable() {
        let snap = SettingsSnapshot {
            ai: AiView {
                overlay: AiOverride::default(),
                effective: AiEffective {
                    base_url: "u".into(),
                    model: "m".into(),
                    max_tokens: 1,
                },
                has_effective_key: false,
            },
            user: UserProfile::default(),
            actions: ActionsView {
                defaults: Defaults::default(),
                actions: HashMap::new(),
            },
            prompts: PromptsConfig::default(),
            appearance: AppearanceSettings::default(),
            agent_watch: AgentWatchSettings::default(),
            about: AboutInfo {
                version: "0.0.0".into(),
                app_settings_path: "".into(),
                actions_yml_hint: "".into(),
                prompts_yml_hint: "".into(),
            },
            button_catalog: vec![ButtonCatalogItem {
                name: "Start".into(),
                label: "开始".into(),
                position: "中间偏右".into(),
                order: 11,
            }],
        };
        let json = serde_json::to_string(&snap).expect("snapshot serialize");
        assert!(json.contains("effective"));
        assert!(json.contains("appearance"));
        assert!(json.contains("agent_watch"));
        assert!(json.contains("button_catalog"));
    }

    #[test]
    fn test_pick_cn_label() {
        assert_eq!(pick_cn_label(&["Back".into(), "选择".into()]), "选择");
        assert_eq!(pick_cn_label(&["确认".into()]), "确认");
        assert_eq!(pick_cn_label(&["LB".into(), "左肩键".into()]), "左肩键");
        assert_eq!(pick_cn_label(&["OnlyEn".into()]), "OnlyEn");
        assert_eq!(pick_cn_label(&[]), "");
    }

    #[test]
    fn test_appearance_input_deserialize() {
        let json = r#"{
            "always_on_top": false,
            "default_collapsed": true,
            "tts_enabled": false,
            "global_shortcut": "F12"
        }"#;
        let input: AppearanceInput = serde_json::from_str(json).unwrap();
        assert!(!input.always_on_top);
        assert_eq!(input.global_shortcut, "F12");
    }

    #[test]
    fn test_agent_watch_input_deserialize() {
        let json = r#"{
            "enabled": true,
            "away_nudge_enabled": true,
            "waiting_alert": true,
            "done_alert": true,
            "use_tts": false
        }"#;
        let input: AgentWatchInput = serde_json::from_str(json).unwrap();
        assert!(input.enabled);
        assert_eq!(input.first_nudge_after_sec, 30);
        assert_eq!(input.repeat_nudge_after_min, 8);
        assert!(input.remote_view_enabled);
        assert!(input.remote_install_enabled);
    }

    #[test]
    fn test_token_session_view_sums_categories() {
        let session = TokenSession {
            session_id: "s1".into(),
            started_at: "2026-05-13T10:00:00+08:00".into(),
            ended_at: "2026-05-13T10:00:02+08:00".into(),
            models: vec!["model-a".into()],
            record_count: 3,
            elapsed_ms_total: 1200,
            chat_total_tokens: 10,
            vision_total_tokens: 20,
            screen_summary_total_tokens: 30,
            memory_aggregation_total_tokens: 40,
            ..Default::default()
        };

        let view = token_session_view(&session);
        assert_eq!(view.total_tokens, 100);
        assert_eq!(view.session_id, "s1");
        assert_eq!(view.models, vec!["model-a"]);
        assert_eq!(view.record_count, 3);
    }

    #[test]
    fn test_memory_review_view_serializable() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        store.record("remember me", "ok", 10);

        let view = memory_review_view(&store, 5);
        let json = serde_json::to_string(&view).expect("memory review serialize");

        assert_eq!(view.total_entries, 1);
        assert!(json.contains("entries"));
        assert!(json.contains("markdown"));
    }
}
