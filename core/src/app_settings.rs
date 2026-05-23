//! ai-pad 专属应用配置覆盖层。
//!
//! 位置：`dirs::config_dir()/ai-pad/app_settings.json`
//!
//! - `ai` 段：作为 `~/.claude/settings.json` 的覆盖层（ai-pad 层，不回写 claude settings）
//! - `appearance` 段：ai-pad 独有的外观/行为配置
//!
//! 设计原则：`~/.claude/settings.json` 只读不写；ai-pad 自己的修改全部落在此文件。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static SETTINGS_IO_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// 应用级配置覆盖层，包含 AI 服务覆盖和外观/行为设置
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AppSettings {
    #[serde(default)]
    pub ai: AiOverride,
    #[serde(default)]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub agent_watch: AgentWatchSettings,
}

/// AI 服务配置覆盖字段（api_key / base_url / model / max_tokens），均为可选
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AiOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
}

/// Claude Code 看管设置：是否启用 hook 观察、离屏提醒和完成/等待提示。
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AgentWatchSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub away_nudge_enabled: bool,
    #[serde(default = "default_first_nudge_after_sec")]
    pub first_nudge_after_sec: u64,
    #[serde(default = "default_repeat_nudge_after_min")]
    pub repeat_nudge_after_min: u64,
    #[serde(default = "default_true")]
    pub waiting_alert: bool,
    #[serde(default = "default_true")]
    pub done_alert: bool,
    #[serde(default)]
    pub use_tts: bool,
    #[serde(default = "default_true")]
    pub remote_view_enabled: bool,
    #[serde(default = "default_true")]
    pub remote_install_enabled: bool,
}

/// 外观与行为设置：置顶、折叠、TTS、全局快捷键、截图间隔等
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AppearanceSettings {
    #[serde(default = "default_true")]
    pub always_on_top: bool,
    #[serde(default)]
    pub default_collapsed: bool,
    #[serde(default)]
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
    #[serde(default = "default_shortcut")]
    pub global_shortcut: String,
    /// 截屏 Vision 分析间隔（秒），默认 30。
    /// 值越大越省 API，值越小响应越即时。最小值 5 秒，避免刷屏。
    #[serde(default = "default_screenshot_interval_sec")]
    pub screenshot_interval_sec: u64,
    /// 截屏分析完成后是否弹出气泡显示结果，默认 true。
    #[serde(default = "default_true")]
    pub screenshot_show_bubble: bool,
    /// 摄像头观察是否启用，默认关闭。该功能只在用户显式开启后采样。
    #[serde(default)]
    pub camera_observation_enabled: bool,
    /// 摄像头观察间隔（秒），默认跟随截屏间隔。保存设置时会与截屏分析间隔保持一致。
    #[serde(default = "default_camera_observation_interval_sec")]
    pub camera_observation_interval_sec: u64,
    /// 是否保存摄像头原始 JPEG。默认 false，只保存结构化分析记录。
    #[serde(default)]
    pub camera_save_frames: bool,
    /// 外部宠物资产根 URL。为空时使用内置 sprite。
    /// 开发期可填 `/__fixtures__/pets/default-cat` 或 file/server URL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pet_asset_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pet_position: Option<WindowPosition>,
}

/// Persisted physical desktop coordinates for the main pet window.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct WindowPosition {
    pub x: i32,
    pub y: i32,
}

fn default_true() -> bool {
    true
}
fn default_shortcut() -> String {
    "CommandOrControl+Alt+Space".to_string()
}
fn default_screenshot_interval_sec() -> u64 {
    30
}
fn default_camera_observation_interval_sec() -> u64 {
    default_screenshot_interval_sec()
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

impl Default for AgentWatchSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            away_nudge_enabled: true,
            first_nudge_after_sec: default_first_nudge_after_sec(),
            repeat_nudge_after_min: default_repeat_nudge_after_min(),
            waiting_alert: true,
            done_alert: true,
            use_tts: false,
            remote_view_enabled: true,
            remote_install_enabled: true,
        }
    }
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            always_on_top: true,
            default_collapsed: false,
            tts_enabled: false,
            notification_sound_enabled: true,
            notification_sound_reminder: true,
            notification_sound_agent_watch: true,
            notification_sound_skip_agent_tts: true,
            reminder_ai_personalization_enabled: false,
            reminder_ai_timeout_ms: default_reminder_ai_timeout_ms(),
            global_shortcut: default_shortcut(),
            screenshot_interval_sec: default_screenshot_interval_sec(),
            screenshot_show_bubble: true,
            camera_observation_enabled: false,
            camera_observation_interval_sec: default_camera_observation_interval_sec(),
            camera_save_frames: false,
            pet_asset_url: None,
            pet_position: None,
        }
    }
}

/// `app_settings.json` 的实际路径。None 表示无法解析 config_dir（罕见）。
pub fn settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ai-pad").join("app_settings.json"))
}

impl AppSettings {
    /// 从磁盘读取；文件不存在或解析失败均回退到默认。
    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Self::default();
        };
        let _guard = settings_io_lock().lock().unwrap_or_else(|e| e.into_inner());
        if !path.exists() {
            return Self::default();
        }
        match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
                tracing::warn!(error = %e, path = ?path, "app_settings.json 解析失败，使用默认");
                Self::default()
            }),
            Err(e) => {
                tracing::warn!(error = %e, path = ?path, "读取 app_settings.json 失败");
                Self::default()
            }
        }
    }

    /// 原子地写入（先写临时文件再 rename，防止崩溃造成损坏）。
    pub fn save(&self) -> Result<(), String> {
        let path = settings_path().ok_or_else(|| "无法解析配置目录".to_string())?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let _guard = settings_io_lock().lock().unwrap_or_else(|e| e.into_inner());
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let tmp = path.with_extension(format!(
            "json.{}.{}.{unique}.tmp",
            std::process::id(),
            thread_id_suffix()
        ));
        fs::write(&tmp, json).map_err(|e| format!("写入临时文件失败: {e}"))?;
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("替换 app_settings.json 失败: {e}"))?;
        }
        fs::rename(&tmp, &path).map_err(|e| format!("保存 app_settings.json 失败: {e}"))
    }
}

fn settings_io_lock() -> &'static Mutex<()> {
    SETTINGS_IO_LOCK.get_or_init(|| Mutex::new(()))
}

fn thread_id_suffix() -> String {
    format!("{:?}", std::thread::current().id())
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_appearance_values() {
        let a = AppearanceSettings::default();
        assert!(a.always_on_top);
        assert!(!a.default_collapsed);
        assert!(!a.tts_enabled);
        assert!(a.notification_sound_enabled);
        assert!(a.notification_sound_reminder);
        assert!(a.notification_sound_agent_watch);
        assert!(a.notification_sound_skip_agent_tts);
        assert!(!a.reminder_ai_personalization_enabled);
        assert_eq!(a.reminder_ai_timeout_ms, 3_000);
        assert_eq!(a.global_shortcut, "CommandOrControl+Alt+Space");
        assert_eq!(a.screenshot_interval_sec, 30);
    }

    #[test]
    fn test_default_agent_watch_values() {
        let s = AgentWatchSettings::default();
        assert!(!s.enabled);
        assert!(s.away_nudge_enabled);
        assert_eq!(s.first_nudge_after_sec, 30);
        assert_eq!(s.repeat_nudge_after_min, 8);
        assert!(s.waiting_alert);
        assert!(s.done_alert);
        assert!(!s.use_tts);
        assert!(s.remote_view_enabled);
        assert!(s.remote_install_enabled);
    }

    #[test]
    fn test_serde_roundtrip_full() {
        let s = AppSettings {
            ai: AiOverride {
                api_key: Some("sk-xxx".into()),
                base_url: Some("https://proxy.example.com".into()),
                model: Some("glm-5.1".into()),
                max_tokens: Some(4096),
            },
            appearance: AppearanceSettings {
                always_on_top: false,
                default_collapsed: true,
                tts_enabled: false,
                notification_sound_enabled: true,
                notification_sound_reminder: false,
                notification_sound_agent_watch: true,
                notification_sound_skip_agent_tts: false,
                reminder_ai_personalization_enabled: true,
                reminder_ai_timeout_ms: 2_500,
                global_shortcut: "F12".into(),
                screenshot_interval_sec: 45,
                screenshot_show_bubble: false,
                camera_observation_enabled: true,
                camera_observation_interval_sec: 600,
                camera_save_frames: true,
                pet_asset_url: Some("/__fixtures__/pets/default-cat".into()),
                pet_position: Some(WindowPosition { x: 123, y: 456 }),
            },
            agent_watch: AgentWatchSettings {
                enabled: true,
                away_nudge_enabled: false,
                first_nudge_after_sec: 120,
                repeat_nudge_after_min: 15,
                waiting_alert: true,
                done_alert: false,
                use_tts: true,
                remote_view_enabled: false,
                remote_install_enabled: false,
            },
        };
        let json = serde_json::to_string_pretty(&s).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.ai.api_key.as_deref(), Some("sk-xxx"));
        assert_eq!(restored.ai.model.as_deref(), Some("glm-5.1"));
        assert_eq!(restored.appearance, s.appearance);
        assert_eq!(
            restored.appearance.pet_asset_url.as_deref(),
            Some("/__fixtures__/pets/default-cat")
        );
        assert_eq!(
            restored.appearance.pet_position,
            Some(WindowPosition { x: 123, y: 456 })
        );
        assert_eq!(restored.agent_watch.first_nudge_after_sec, 120);
        assert!(restored.agent_watch.use_tts);
        assert!(!restored.agent_watch.remote_view_enabled);
        assert!(!restored.agent_watch.remote_install_enabled);
    }

    #[test]
    fn test_empty_ai_section_none_fields() {
        let s: AppSettings = serde_json::from_str("{}").unwrap();
        assert!(s.ai.api_key.is_none());
        assert!(s.ai.base_url.is_none());
        assert!(s.ai.model.is_none());
        assert_eq!(s.appearance, AppearanceSettings::default());
        assert_eq!(s.agent_watch, AgentWatchSettings::default());
    }

    #[test]
    fn test_partial_appearance_gets_defaults() {
        let json = r#"{"appearance": {"tts_enabled": false}}"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert!(!s.appearance.tts_enabled);
        assert!(s.appearance.notification_sound_enabled);
        assert!(s.appearance.notification_sound_reminder);
        assert!(s.appearance.notification_sound_agent_watch);
        assert!(s.appearance.notification_sound_skip_agent_tts);
        assert!(!s.appearance.reminder_ai_personalization_enabled);
        assert_eq!(s.appearance.reminder_ai_timeout_ms, 3_000);
        assert!(s.appearance.always_on_top); // 默认值
        assert_eq!(s.appearance.global_shortcut, "CommandOrControl+Alt+Space");
    }

    #[test]
    fn test_ai_override_omits_none_when_serialize() {
        let o = AiOverride {
            api_key: Some("sk-1".into()),
            base_url: None,
            model: None,
            max_tokens: None,
        };
        let json = serde_json::to_string(&o).unwrap();
        assert!(json.contains("api_key"));
        assert!(!json.contains("base_url"));
        assert!(!json.contains("model"));
        assert!(!json.contains("max_tokens"));
    }

    #[test]
    fn test_thread_id_suffix_is_file_name_safe() {
        let suffix = thread_id_suffix();
        assert!(!suffix.is_empty());
        assert!(suffix.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
