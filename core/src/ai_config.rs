use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// 从 ~/.claude/settings.json 读取 AI 配置
#[derive(Debug, Clone)]
pub struct AiConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl AiConfig {
    /// 逐字段优先级：环境变量 > `app_settings.json` 覆盖层 > `~/.claude/settings.json` > 默认值。
    ///
    /// 任一来源提供该字段即采用；`~/.claude/settings.json` 全程只读。
    pub fn load() -> Result<Self, String> {
        let overlay = crate::app_settings::AppSettings::load().ai;
        let claude = load_claude_env().unwrap_or_default();

        let api_key = non_empty_env("ANTHROPIC_API_KEY")
            .or_else(|| non_empty_env("ANTHROPIC_AUTH_TOKEN"))
            .or_else(|| overlay.api_key.clone().filter(|s| !s.is_empty()))
            .or_else(|| claude.ANTHROPIC_AUTH_TOKEN.clone())
            .or_else(|| claude.ANTHROPIC_API_KEY.clone())
            .ok_or_else(|| {
                String::from("未找到 API key（环境变量 / app_settings.json / ~/.claude/settings.json 均为空）")
            })?;

        let base_url = non_empty_env("ANTHROPIC_BASE_URL")
            .or_else(|| overlay.base_url.clone().filter(|s| !s.is_empty()))
            .or_else(|| claude.ANTHROPIC_BASE_URL.clone())
            .unwrap_or_else(|| String::from("https://api.anthropic.com"));

        let model = non_empty_env("ANTHROPIC_MODEL")
            .or_else(|| overlay.model.clone().filter(|s| !s.is_empty()))
            .or_else(|| claude.ANTHROPIC_MODEL.clone())
            .unwrap_or_else(|| String::from("claude-sonnet-4-20250514"));

        Ok(Self {
            api_key,
            base_url,
            model,
        })
    }

    /// 根据模型名推断合适的 max_tokens
    ///
    /// 基于 Anthropic 官方发布的各模型最大输出 token 数。
    /// 对于桌面宠物场景，取模型最大值的 1/8 作为上限足够，
    /// 既保证回复质量又不浪费。
    pub fn max_tokens(&self) -> u64 {
        // 环境变量覆盖
        if let Ok(v) = std::env::var("ANTHROPIC_MAX_TOKENS")
            && let Ok(n) = v.parse()
        {
            return n;
        }
        // app_settings 覆盖层
        if let Some(n) = crate::app_settings::AppSettings::load().ai.max_tokens {
            return n;
        }
        model_max_tokens(&self.model)
    }
}

/// 模型最大输出 token 映射
///
/// 国产模型统一 256K；其他模型默认 256K。
/// API 不会因为 max_tokens 设大了就多输出——模型生成完自动停止。
fn model_max_tokens(model: &str) -> u64 {
    let _ = model;
    256_000
}

// ---- 内部解析结构 ----

#[derive(Deserialize, Default, Clone)]
struct SettingsFile {
    #[serde(default)]
    env: EnvSection,
}

#[derive(Deserialize, Default, Clone)]
#[allow(non_snake_case)]
struct EnvSection {
    #[serde(default, alias = "anthropic_auth_token")]
    ANTHROPIC_AUTH_TOKEN: Option<String>,
    #[serde(default, alias = "anthropic_api_key")]
    ANTHROPIC_API_KEY: Option<String>,
    #[serde(default, alias = "anthropic_base_url")]
    ANTHROPIC_BASE_URL: Option<String>,
    #[serde(default, alias = "anthropic_model")]
    ANTHROPIC_MODEL: Option<String>,
}

/// 非空环境变量读取。空串视为未设置。
fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// 读取 `~/.claude/settings.json` 的 env 段（只读）。返回 `None` 表示文件不存在或损坏。
fn load_claude_env() -> Option<EnvSection> {
    let raw = fs::read_to_string(settings_path()).ok()?;
    let cfg: SettingsFile = serde_json::from_str(&raw).ok()?;
    Some(cfg.env)
}

#[allow(dead_code)]
fn env_fallback(a: &str, b: &str) -> Result<String, ()> {
    let a_val = std::env::var(a);
    let b_val = std::env::var(b);
    if let Ok(v) = a_val
        && !v.is_empty()
    {
        return Ok(v);
    }
    if let Ok(v) = b_val
        && !v.is_empty()
    {
        return Ok(v);
    }
    Err(())
}

fn settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude/settings.json")
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_from_real_settings_json() {
        let result = AiConfig::load();
        if let Err(e) = &result {
            let path = settings_path();
            if let Ok(raw) = fs::read_to_string(&path) {
                eprintln!("原始文件前 300 字符: {}", &raw[..raw.len().min(300)]);
            }
            panic!("读取 settings.json 失败: {e}");
        }
        let cfg = result.unwrap();
        assert!(!cfg.api_key.is_empty(), "API key 不应为空");
        assert!(!cfg.base_url.is_empty(), "base_url 不应为空");
    }

    #[test]
    fn test_parse_settings_with_all_fields() {
        let cfg: SettingsFile = serde_json::from_str(
            r#"{
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "sk-test",
                "ANTHROPIC_BASE_URL": "https://proxy.example.com",
                "ANTHROPIC_MODEL": "glm-5.1"
            }
        }"#,
        )
        .unwrap();

        assert_eq!(cfg.env.ANTHROPIC_AUTH_TOKEN, Some("sk-test".into()));
        assert_eq!(
            cfg.env.ANTHROPIC_BASE_URL,
            Some("https://proxy.example.com".into())
        );
        assert_eq!(cfg.env.ANTHROPIC_MODEL, Some("glm-5.1".into()));

        let cfg2: SettingsFile = serde_json::from_str(
            r#"{
            "env": {
                "anthropic_auth_token": "sk-test2",
                "anthropic_base_url": "https://proxy2.example.com"
            }
        }"#,
        )
        .unwrap();
        assert_eq!(cfg2.env.ANTHROPIC_AUTH_TOKEN, Some("sk-test2".into()));
    }

    #[test]
    fn test_parse_settings_missing_optional_fields() {
        let cfg: SettingsFile = serde_json::from_str(
            r#"{
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "sk-only-key"
            }
        }"#,
        )
        .unwrap();

        assert_eq!(cfg.env.ANTHROPIC_AUTH_TOKEN, Some("sk-only-key".into()));
        assert_eq!(cfg.env.ANTHROPIC_BASE_URL, None);
        assert_eq!(cfg.env.ANTHROPIC_MODEL, None);
    }

    #[test]
    fn test_env_fallback_both_missing() {
        let r = env_fallback("NONEXISTENT_VAR_XYZ", "ALSO_NONEXISTENT");
        assert!(r.is_err());
    }

    #[test]
    fn test_default_values_when_missing() {
        let cfg: SettingsFile = serde_json::from_str(
            r#"{
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "sk-key"
            }
        }"#,
        )
        .unwrap();

        let url = cfg
            .env
            .ANTHROPIC_BASE_URL
            .unwrap_or_else(|| String::from("https://api.anthropic.com"));
        let model = cfg
            .env
            .ANTHROPIC_MODEL
            .unwrap_or_else(|| String::from("claude-sonnet-4-20250514"));

        assert_eq!(url, "https://api.anthropic.com");
        assert_eq!(model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_settings_path_returns_claude_dir() {
        let path = settings_path();
        assert!(path.to_str().unwrap().ends_with(".claude/settings.json"));
    }

    // ---- max_tokens 测试 ----

    #[test]
    fn test_max_tokens_always_256k() {
        assert_eq!(model_max_tokens("claude-sonnet-4-20250514"), 256_000);
        assert_eq!(model_max_tokens("glm-5.1"), 256_000);
        assert_eq!(model_max_tokens("deepseek-chat"), 256_000);
        assert_eq!(model_max_tokens("qwen-max"), 256_000);
        assert_eq!(model_max_tokens("some-unknown-model"), 256_000);
    }

    #[test]
    fn test_config_max_tokens_from_real_settings() {
        if let Ok(cfg) = AiConfig::load() {
            let mt = cfg.max_tokens();
            assert_eq!(mt, 256_000);
            eprintln!("模型: {}, max_tokens: {}", cfg.model, mt);
        }
    }
}
