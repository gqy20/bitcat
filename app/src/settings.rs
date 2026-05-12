//! 设置窗口：独立 Tauri 窗口，由托盘菜单"设置…"触发。
//!
//! 设计要点（见 plan `Settings_UI_Design_Plan`）：
//! - `~/.claude/settings.json` 仅读，**永不写入**；AI 覆盖写入 `app_settings.json`
//! - actions.yml / prompts.yml 就地写回（注释会被覆盖，保存前自动备份 `.bak`）
//! - 保存后仅 set 原子 flag，由 gamepad_loop 下 tick 自动 reload（复用现有机制）

use crate::commands::SharedWindowState;
use ai_pad_core::action::{ActionConfig, ActionDef, Defaults};
use ai_pad_core::app_settings::{AiOverride, AppSettings, AppearanceSettings};
use ai_pad_core::prompts::PromptsConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tracing::{info, warn};

const WINDOW_LABEL: &str = "settings";
const WINDOW_W: f64 = 720.0;
const WINDOW_H: f64 = 520.0;

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
        .min_inner_size(540.0, 400.0)
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
    pub actions: ActionsView,
    pub prompts: PromptsConfig,
    pub appearance: AppearanceSettings,
    pub about: AboutInfo,
    /// 按 buttons.yml 列出的所有可配置按键（有序，index 升序）
    pub button_catalog: Vec<ButtonCatalogItem>,
}

/// 按键元数据：用于设置界面展示"全部支持的按键 + 中文说明"。
#[derive(Debug, Serialize)]
pub struct ButtonCatalogItem {
    /// 按键主名，如 "Start"（同 actions.yml 里的 key）
    pub name: String,
    /// 中文别名，例如 "开始"；取 buttons.yml aliases 中第一个中文别名
    pub label: String,
    /// 硬件位置描述，例如 "中间偏右"
    pub position: String,
    /// 显示顺序（对应 buttons.yml 中的 button index）
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

#[derive(Debug, Deserialize)]
pub struct AppearanceInput {
    pub always_on_top: bool,
    pub default_collapsed: bool,
    pub tts_enabled: bool,
    pub global_shortcut: String,
}

// ---- 命令 ----

#[tauri::command]
pub async fn cmd_settings_show(app: AppHandle) -> Result<(), String> {
    toggle_settings(&app);
    Ok(())
}

#[tauri::command]
pub async fn cmd_settings_hide(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

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
    info!("[settings-js] {msg}");
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
    let action_cfg = ActionConfig::load("actions.yml").unwrap_or_else(|e| {
        warn!(error = %e, "加载 actions.yml 失败，使用内置默认");
        ActionConfig::default_builtin()
    });
    let prompts_cfg = PromptsConfig::load();

    let app_settings_path = ai_pad_core::app_settings::settings_path()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "<unknown>".into());

    // 读取 buttons.yml 提供可配置按键全集
    let button_catalog = match ai_pad_core::config::ButtonConfig::load("buttons.yml") {
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
            warn!(error = %e, "加载 buttons.yml 失败，按键列表为空");
            Vec::new()
        }
    };

    Ok(SettingsSnapshot {
        ai: AiView {
            overlay: overlay.ai,
            effective,
            has_effective_key: has_key,
        },
        actions: ActionsView {
            defaults: action_cfg.defaults,
            actions: action_cfg.actions,
        },
        prompts: prompts_cfg,
        appearance: overlay.appearance,
        about: AboutInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            app_settings_path,
            actions_yml_hint: "actions.yml（exe 同目录 或 CWD）".into(),
            prompts_yml_hint: "prompts.yml（exe 同目录 或 CWD）".into(),
        },
        button_catalog,
    })
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
    cfg.save("actions.yml")?;
    // 触发 gamepad_loop 热重载
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    ws.config_reload.store(true, Ordering::SeqCst);
    info!(
        actions = cfg.actions.len(),
        "[settings] actions.yml 已保存并触发 reload"
    );
    Ok(())
}

/// 保存 Prompt 配置（写回 prompts.yml）
#[tauri::command]
pub async fn cmd_settings_save_prompts(payload: PromptsConfig) -> Result<(), String> {
    payload.save()?;
    info!("[settings] prompts.yml 已保存");
    Ok(())
}

/// 保存外观设置（写入 app_settings.json 的 appearance 段）
#[tauri::command]
pub async fn cmd_settings_save_appearance(
    app: AppHandle,
    payload: AppearanceInput,
) -> Result<(), String> {
    let mut s = AppSettings::load();
    s.appearance = AppearanceSettings {
        always_on_top: payload.always_on_top,
        default_collapsed: payload.default_collapsed,
        tts_enabled: payload.tts_enabled,
        global_shortcut: payload.global_shortcut,
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
    info!(appearance = ?s.appearance, "[settings] 外观设置已保存");
    Ok(())
}

/// 重置某一分类为内置默认（actions / prompts / appearance / ai）
#[tauri::command]
pub async fn cmd_settings_reset(category: String) -> Result<(), String> {
    match category.as_str() {
        "actions" => {
            ActionConfig::default_builtin().save("actions.yml")?;
        }
        "prompts" => {
            PromptsConfig::default_builtin().save()?;
        }
        "appearance" => {
            let mut s = AppSettings::load();
            s.appearance = AppearanceSettings::default();
            s.save()?;
        }
        "ai" => {
            let mut s = AppSettings::load();
            s.ai = AiOverride::default();
            s.save()?;
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
            actions: ActionsView {
                defaults: Defaults::default(),
                actions: HashMap::new(),
            },
            prompts: PromptsConfig::default(),
            appearance: AppearanceSettings::default(),
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
}
