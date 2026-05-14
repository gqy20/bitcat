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

/// 应用级配置覆盖层，包含 AI 服务覆盖和外观/行为设置
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AppSettings {
    #[serde(default)]
    pub ai: AiOverride,
    #[serde(default)]
    pub appearance: AppearanceSettings,
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

/// 外观与行为设置：置顶、折叠、TTS、全局快捷键、截图间隔等
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AppearanceSettings {
    #[serde(default = "default_true")]
    pub always_on_top: bool,
    #[serde(default)]
    pub default_collapsed: bool,
    #[serde(default)]
    pub tts_enabled: bool,
    #[serde(default = "default_shortcut")]
    pub global_shortcut: String,
    /// 截屏 Vision 分析间隔（秒），默认 30。
    /// 值越大越省 API，值越小响应越即时。最小值 5 秒，避免刷屏。
    #[serde(default = "default_screenshot_interval_sec")]
    pub screenshot_interval_sec: u64,
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

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            always_on_top: true,
            default_collapsed: false,
            tts_enabled: false,
            global_shortcut: default_shortcut(),
            screenshot_interval_sec: default_screenshot_interval_sec(),
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
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| format!("写入临时文件失败: {e}"))?;
        fs::rename(&tmp, &path).map_err(|e| format!("保存 app_settings.json 失败: {e}"))
    }
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
        assert_eq!(a.global_shortcut, "CommandOrControl+Alt+Space");
        assert_eq!(a.screenshot_interval_sec, 30);
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
                global_shortcut: "F12".into(),
                screenshot_interval_sec: 45,
                pet_position: Some(WindowPosition { x: 123, y: 456 }),
            },
        };
        let json = serde_json::to_string_pretty(&s).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.ai.api_key.as_deref(), Some("sk-xxx"));
        assert_eq!(restored.ai.model.as_deref(), Some("glm-5.1"));
        assert_eq!(restored.appearance, s.appearance);
        assert_eq!(
            restored.appearance.pet_position,
            Some(WindowPosition { x: 123, y: 456 })
        );
    }

    #[test]
    fn test_empty_ai_section_none_fields() {
        let s: AppSettings = serde_json::from_str("{}").unwrap();
        assert!(s.ai.api_key.is_none());
        assert!(s.ai.base_url.is_none());
        assert!(s.ai.model.is_none());
        assert_eq!(s.appearance, AppearanceSettings::default());
    }

    #[test]
    fn test_partial_appearance_gets_defaults() {
        let json = r#"{"appearance": {"tts_enabled": false}}"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert!(!s.appearance.tts_enabled);
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
}
