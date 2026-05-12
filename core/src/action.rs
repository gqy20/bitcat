use serde::Deserialize;
use std::fs;

// ---- 数据结构 ----

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Defaults {
    #[serde(default = "default_terminal")]
    pub terminal: String,
    #[serde(default = "default_window")]
    pub window: String,
}

fn default_terminal() -> String {
    "powershell".into()
}
fn default_window() -> String {
    "maximized".into()
}

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

fn default_delay() -> f64 {
    1.0
}

const DEFAULT_YML: &str = include_str!("../../actions.yml");

/// 配置文件查找：exe 同目录 → 传入路径（CWD）→ 嵌入默认值
fn load_config_content(path: &str, default: &str) -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(path)))
        .filter(|p| p.exists())
        .and_then(|p| fs::read_to_string(p).ok())
        .or_else(|| fs::read_to_string(path).ok())
        .unwrap_or_else(|| default.to_string())
}

impl ActionConfig {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = load_config_content(path, DEFAULT_YML);
        let config: ActionConfig = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}

/// 统一程序启动逻辑（tools.rs / panel.rs / lib.rs 共用）
pub fn launch_program(
    program: &str,
    args: &str,
    workdir: &str,
    terminal: bool,
    default_terminal: &str,
) -> Result<(), String> {
    if terminal {
        let term = if terminal && default_terminal.is_empty() {
            "powershell"
        } else {
            default_terminal
        };
        let is_shell = matches!(program, "powershell" | "pwsh" | "cmd");
        let ps_cmd = if is_shell && args.is_empty() {
            format!("Start-Process {program} -WindowStyle Maximized")
        } else {
            let cmd = if args.is_empty() {
                program.to_string()
            } else {
                format!("{program} {args}")
            };
            format!(
                "Start-Process {term} -ArgumentList '-NoExit','-Command','{cmd}' -WindowStyle Maximized"
            )
        };
        std::process::Command::new("powershell")
            .args(["-Command", &ps_cmd])
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("启动失败: {e}"))
    } else {
        let workdir = if workdir.is_empty() { "." } else { workdir };
        std::process::Command::new(program)
            .args(args.split_whitespace().filter(|s| !s.is_empty()))
            .current_dir(workdir)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("启动失败: {e}"))
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
