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
    /// 优先级: 环境变量 > ~/.claude/settings.json > 默认值
    pub fn load() -> Result<Self, String> {
        // 1. 尝试环境变量
        if let Ok(key) = env_fallback("ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN") {
            if let Ok(url) = std::env::var("ANTHROPIC_BASE_URL") {
                let model = std::env::var("ANTHROPIC_MODEL")
                    .unwrap_or_else(|_| "claude-sonnet-4-20250514".into());
                return Ok(Self { api_key: key, base_url: url, model });
            }
        }

        // 2. 回退到 settings.json
        let path = settings_path();
        let raw = fs::read_to_string(&path)
            .map_err(|e| format!("读取 {:?} 失败: {e}", path))?;
        let cfg: SettingsFile = serde_json::from_str(&raw)
            .map_err(|e| format!("解析 settings.json 失败: {e}"))?;

        let api_key = cfg.env.ANTHROPIC_AUTH_TOKEN
            .or(cfg.env.ANTHROPIC_API_KEY)
            .ok_or_else(|| String::from("settings.json 中未找到 API key"))?;

        let base_url = cfg.env.ANTHROPIC_BASE_URL
            .unwrap_or_else(|| String::from("https://api.anthropic.com"));

        let model = cfg.env.ANTHROPIC_MODEL
            .unwrap_or_else(|| String::from("claude-sonnet-4-20250514"));

        Ok(Self { api_key, base_url, model })
    }

    /// 根据模型名推断合适的 max_tokens
    ///
    /// 基于 Anthropic 官方发布的各模型最大输出 token 数。
    /// 对于桌面宠物场景，取模型最大值的 1/8 作为上限足够，
    /// 既保证回复质量又不浪费。
    pub fn max_tokens(&self) -> u64 {
        // 环境变量覆盖
        if let Ok(v) = std::env::var("ANTHROPIC_MAX_TOKENS") {
            if let Ok(n) = v.parse() {
                return n;
            }
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

#[derive(Deserialize)]
struct SettingsFile {
    env: EnvSection,
}

#[derive(Deserialize)]
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

fn env_fallback(a: &str, b: &str) -> Result<String, ()> {
    let a_val = std::env::var(a);
    let b_val = std::env::var(b);
    if let Ok(v) = a_val {
        if !v.is_empty() { return Ok(v); }
    }
    if let Ok(v) = b_val {
        if !v.is_empty() { return Ok(v); }
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
        let cfg: SettingsFile = serde_json::from_str(r#"{
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "sk-test",
                "ANTHROPIC_BASE_URL": "https://proxy.example.com",
                "ANTHROPIC_MODEL": "glm-5.1"
            }
        }"#).unwrap();

        assert_eq!(cfg.env.ANTHROPIC_AUTH_TOKEN, Some("sk-test".into()));
        assert_eq!(cfg.env.ANTHROPIC_BASE_URL, Some("https://proxy.example.com".into()));
        assert_eq!(cfg.env.ANTHROPIC_MODEL, Some("glm-5.1".into()));

        let cfg2: SettingsFile = serde_json::from_str(r#"{
            "env": {
                "anthropic_auth_token": "sk-test2",
                "anthropic_base_url": "https://proxy2.example.com"
            }
        }"#).unwrap();
        assert_eq!(cfg2.env.ANTHROPIC_AUTH_TOKEN, Some("sk-test2".into()));
    }

    #[test]
    fn test_parse_settings_missing_optional_fields() {
        let cfg: SettingsFile = serde_json::from_str(r#"{
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "sk-only-key"
            }
        }"#).unwrap();

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
        let cfg: SettingsFile = serde_json::from_str(r#"{
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "sk-key"
            }
        }"#).unwrap();

        let url = cfg.env.ANTHROPIC_BASE_URL
            .unwrap_or_else(|| String::from("https://api.anthropic.com"));
        let model = cfg.env.ANTHROPIC_MODEL
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
