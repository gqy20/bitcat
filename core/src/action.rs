use std::fs;
use serde::Deserialize;

// ---- 数据结构 ----

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Defaults {
    #[serde(default = "default_terminal")]
    pub terminal: String,
    #[serde(default = "default_window")]
    pub window: String,
}

fn default_terminal() -> String { "powershell".into() }
fn default_window() -> String { "maximized".into() }

#[derive(Debug, Clone, Deserialize)]
pub struct ActionConfig {
    #[serde(default)]
    pub defaults: Defaults,
    pub actions: std::collections::HashMap<String, ActionDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActionDef {
    #[serde(rename = "type")]
    pub action_type: String,
    pub program: Option<String>,
    pub args: Option<String>,
    #[serde(default)]
    pub workdir: String,
    #[serde(default)]
    pub terminal: bool,
    #[serde(default)]
    pub command: Option<String>,
    pub voice: Option<VoiceConfig>,
    pub trigger: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VoiceConfig {
    #[serde(default)]
    pub trigger: Vec<String>,
    #[serde(default = "default_delay")]
    pub delay: f64,
}

fn default_delay() -> f64 { 1.0 }

impl ActionConfig {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: ActionConfig = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_actions_yml() {
        let config = ActionConfig::load("actions.yml").unwrap();
        assert!(config.actions.contains_key("Start"));
        assert!(config.actions.contains_key("Y"));
    }

    #[test]
    fn test_load_launch_action() {
        let config = ActionConfig::load("actions.yml").unwrap();
        let start = config.actions.get("Start").unwrap();
        assert_eq!(start.action_type, "launch");
        assert_eq!(start.program.as_deref(), Some("claude"));
        assert!(start.terminal);
    }

    #[test]
    fn test_load_voice_action() {
        let config = ActionConfig::load("actions.yml").unwrap();
        let y = config.actions.get("Y").unwrap();
        assert_eq!(y.action_type, "voice");
        let voice = y.voice.as_ref().unwrap();
        assert_eq!(voice.trigger, vec!["ctrl", "win"]);
    }

    #[test]
    fn test_load_defaults() {
        let config = ActionConfig::load("actions.yml").unwrap();
        assert_eq!(config.defaults.terminal, "powershell");
        assert_eq!(config.defaults.window, "maximized");
    }

    #[test]
    fn test_load_missing_file() {
        assert!(ActionConfig::load("nonexistent.yml").is_err());
    }
}
