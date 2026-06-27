//! AI service configuration loading and precedence merging.
//!
//! The effective AI connection fields are resolved independently from the
//! settings UI overlay, the `.env` file next to the executable, Claude's
//! read-only settings file, and system environment variables. The precedence is:
//! app settings > exe `.env` > `~/.claude/settings.json` > system environment >
//! built-in defaults.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// AI provider configuration used by the agent client.
#[derive(Debug, Clone)]
pub struct AiConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl AiConfig {
    /// Resolve each field with this precedence:
    /// settings UI > exe `.env` > Claude settings > system environment > default.
    ///
    /// File-based sources are read-only. They are parsed directly instead of
    /// being loaded into the process environment, so system variables cannot
    /// accidentally outrank the configured app sources.
    pub fn load() -> Result<Self, String> {
        let overlay = crate::app_settings::AppSettings::load().ai;
        let exe_env = load_exe_env().unwrap_or_default();
        let claude = load_claude_env().unwrap_or_default();

        let api_key = overlay
            .api_key
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| exe_env.ANTHROPIC_AUTH_TOKEN.clone())
            .or_else(|| exe_env.ANTHROPIC_API_KEY.clone())
            .or_else(|| claude.ANTHROPIC_AUTH_TOKEN.clone())
            .or_else(|| claude.ANTHROPIC_API_KEY.clone())
            .or_else(|| non_empty_env("ANTHROPIC_API_KEY"))
            .or_else(|| non_empty_env("ANTHROPIC_AUTH_TOKEN"))
            .ok_or_else(|| {
                String::from(
                    "未找到 API key（设置页 / exe .env / ~/.claude/settings.json / 系统环境变量均为空）",
                )
            })?;

        let base_url = overlay
            .base_url
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| exe_env.ANTHROPIC_BASE_URL.clone())
            .or_else(|| claude.ANTHROPIC_BASE_URL.clone())
            .or_else(|| non_empty_env("ANTHROPIC_BASE_URL"))
            .unwrap_or_else(|| String::from("https://api.anthropic.com"));

        let model = overlay
            .model
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| exe_env.ANTHROPIC_MODEL.clone())
            .or_else(|| claude.ANTHROPIC_MODEL.clone())
            .or_else(|| non_empty_env("ANTHROPIC_MODEL"))
            .unwrap_or_else(|| String::from("claude-sonnet-4-20250514"));

        Ok(Self {
            api_key,
            base_url,
            model,
        })
    }

    /// Resolve the max output token setting with the same source precedence.
    pub fn max_tokens(&self) -> u64 {
        if let Some(n) = crate::app_settings::AppSettings::load().ai.max_tokens {
            return n;
        }
        if let Some(n) = load_exe_env()
            .and_then(|env| env.ANTHROPIC_MAX_TOKENS)
            .and_then(|v| v.parse().ok())
        {
            return n;
        }
        if let Some(n) = load_claude_env()
            .and_then(|env| env.ANTHROPIC_MAX_TOKENS)
            .and_then(|v| v.parse().ok())
        {
            return n;
        }
        if let Ok(v) = std::env::var("ANTHROPIC_MAX_TOKENS")
            && let Ok(n) = v.parse()
        {
            return n;
        }
        model_max_tokens(&self.model)
    }
}

/// Model max output token mapping.
///
/// Keep this as a provider-safe upper bound. Some Anthropic-compatible
/// providers reject requests before generation starts when `max_tokens` is
/// above their advertised range.
fn model_max_tokens(model: &str) -> u64 {
    let normalized = model.to_ascii_lowercase();
    if normalized.starts_with("glm-") {
        return 131_072;
    }
    256_000
}

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
    #[serde(default, alias = "anthropic_max_tokens")]
    ANTHROPIC_MAX_TOKENS: Option<String>,
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn non_empty_dotenv(env: &HashMap<String, String>, name: &str) -> Option<String> {
    env.get(name).cloned().filter(|v| !v.is_empty())
}

/// Read `.env` next to the executable without mutating process environment.
fn load_exe_env() -> Option<EnvSection> {
    let path = exe_env_path()?;
    let env: HashMap<String, String> = dotenvy::from_path_iter(path).ok()?.flatten().collect();
    Some(EnvSection {
        ANTHROPIC_AUTH_TOKEN: non_empty_dotenv(&env, "ANTHROPIC_AUTH_TOKEN"),
        ANTHROPIC_API_KEY: non_empty_dotenv(&env, "ANTHROPIC_API_KEY"),
        ANTHROPIC_BASE_URL: non_empty_dotenv(&env, "ANTHROPIC_BASE_URL"),
        ANTHROPIC_MODEL: non_empty_dotenv(&env, "ANTHROPIC_MODEL"),
        ANTHROPIC_MAX_TOKENS: non_empty_dotenv(&env, "ANTHROPIC_MAX_TOKENS"),
    })
}

/// Read the `env` section from `~/.claude/settings.json`.
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

fn exe_env_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|dir| dir.join(".env"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_from_real_settings_json() {
        match AiConfig::load() {
            Ok(cfg) => {
                assert!(!cfg.api_key.is_empty(), "API key should not be empty");
                assert!(!cfg.base_url.is_empty(), "base_url should not be empty");
            }
            Err(e) => {
                eprintln!("skip real config test: no API key configured ({e})");
            }
        }
    }

    #[test]
    fn test_parse_settings_with_all_fields() {
        let cfg: SettingsFile = serde_json::from_str(
            r#"{
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "sk-test",
                "ANTHROPIC_BASE_URL": "https://proxy.example.com",
                "ANTHROPIC_MODEL": "glm-5.1",
                "ANTHROPIC_MAX_TOKENS": "1234"
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
        assert_eq!(cfg.env.ANTHROPIC_MAX_TOKENS, Some("1234".into()));

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

    #[test]
    fn test_model_max_tokens_caps_glm_models() {
        assert_eq!(model_max_tokens("claude-sonnet-4-20250514"), 256_000);
        assert_eq!(model_max_tokens("glm-5v-turbo"), 131_072);
        assert_eq!(model_max_tokens("glm-5.1"), 131_072);
        assert_eq!(model_max_tokens("GLM-5V-TURBO"), 131_072);
        assert_eq!(model_max_tokens("deepseek-chat"), 256_000);
        assert_eq!(model_max_tokens("qwen-max"), 256_000);
        assert_eq!(model_max_tokens("some-unknown-model"), 256_000);
    }

    #[test]
    fn test_config_max_tokens_from_real_settings() {
        if let Ok(cfg) = AiConfig::load() {
            let mt = cfg.max_tokens();
            assert!(mt > 0);
            eprintln!("model: {}, max_tokens: {}", cfg.model, mt);
        }
    }
}
