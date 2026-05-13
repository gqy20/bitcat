//! 用户动作配置与程序启动逻辑
//!
//! 解析 config/actions.yml，定义手柄按键触发的动作（启动程序、执行命令、语音输入等）。
//! launch_program 是跨模块共用的统一程序启动函数，处理终端包裹和参数传递。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// ---- 数据结构 ----

/// 全局默认配置（终端程序、窗口模式）
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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

/// 动作配置根结构：全局默认值 + 按钮名称到动作定义的映射
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActionConfig {
    #[serde(default)]
    pub defaults: Defaults,
    pub actions: std::collections::HashMap<String, ActionDef>,
}

/// 单个动作定义：可启动程序、执行命令或触发语音输入
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActionDef {
    #[serde(rename = "type")]
    pub action_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub workdir: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub terminal: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<VoiceConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<Vec<String>>,
    /// 可选：为该动作绑定一个键盘全局热键（形如 `"Ctrl+Alt+D"`）。
    ///
    /// 若指定，启动时会注册全局热键 → 触发时通过 ActionBus 以
    /// `ActionSource::Keyboard` 分发对应的 Action（与手柄按键等价）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard_shortcut: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// 语音输入触发配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VoiceConfig {
    #[serde(default)]
    pub trigger: Vec<String>,
    #[serde(default = "default_delay")]
    pub delay: f64,
}

fn default_delay() -> f64 {
    1.0
}

const DEFAULT_YML: &str = include_str!("../../config/actions.yml");

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

/// 解析保存路径：优先复用加载时的路径（exe 同目录 → CWD），都不存在则写到 CWD。
fn resolve_save_path(path: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(path)))
        .filter(|p| p.exists())
        .or_else(|| {
            let p = PathBuf::from(path);
            if p.exists() { Some(p) } else { None }
        })
        .unwrap_or_else(|| PathBuf::from(path))
}

impl ActionConfig {
    /// 从 YAML 文件加载动作配置，文件不存在时回退到编译时嵌入的默认值
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = load_config_content(path, DEFAULT_YML);
        let config: ActionConfig = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// 序列化写回 config/actions.yml（会覆盖注释，保存前自动备份 `.bak`）。
    pub fn save(&self, path: &str) -> Result<(), String> {
        let target = resolve_save_path(path);
        if let Ok(old) = fs::read_to_string(&target) {
            let _ = fs::write(target.with_extension("yml.bak"), old);
        }
        let header = "# 由 8Bit Cat 设置界面生成\n\
                      # 手动编辑仍然生效，但下次保存设置会覆盖注释\n\n";
        let body = serde_yaml::to_string(self).map_err(|e| e.to_string())?;
        fs::write(&target, format!("{header}{body}"))
            .map_err(|e| format!("写入 {:?} 失败: {e}", target))
    }

    /// 返回内置默认 yml 解析后的 `ActionConfig`（用于"重置为默认"）。
    pub fn default_builtin() -> Self {
        serde_yaml::from_str(DEFAULT_YML).expect("内置 config/actions.yml 损坏")
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
        let term = if default_terminal.is_empty() {
            "powershell"
        } else {
            default_terminal
        };
        let is_shell = matches!(program, "powershell" | "pwsh" | "cmd");
        // 用数组式参数传递，避免命令注入
        let (ps_args, cmd) = if is_shell && args.is_empty() {
            let shell_cmd = format!("Start-Process {program} -WindowStyle Maximized");
            (
                vec!["-NoExit".to_string(), "-Command".to_string(), shell_cmd],
                None,
            )
        } else {
            let cmd_str = if args.is_empty() {
                program.to_string()
            } else {
                format!("{program} {args}")
            };
            (
                vec!["-NoExit".to_string(), "-Command".to_string()],
                Some(cmd_str),
            )
        };

        let mut cmd_builder = std::process::Command::new(term);
        cmd_builder.args(&ps_args);
        if let Some(c) = &cmd {
            cmd_builder.arg(c);
        }
        cmd_builder.arg("-WindowStyle").arg("Maximized");
        cmd_builder
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
        let config = ActionConfig::load("config/actions.yml").unwrap();
        assert!(config.actions.contains_key("Start"));
        assert!(config.actions.contains_key("Y"));
    }

    #[test]
    fn test_load_launch_action() {
        let config = ActionConfig::load("config/actions.yml").unwrap();
        let start = config.actions.get("Start").unwrap();
        assert_eq!(start.action_type, "launch");
        assert_eq!(start.program.as_deref(), Some("claude"));
        assert!(start.terminal);
    }

    #[test]
    fn test_load_voice_action() {
        let config = ActionConfig::load("config/actions.yml").unwrap();
        let y = config.actions.get("Y").unwrap();
        assert_eq!(y.action_type, "voice");
        let voice = y.voice.as_ref().unwrap();
        assert_eq!(voice.trigger, vec!["ctrl", "win"]);
    }

    #[test]
    fn test_load_defaults() {
        let config = ActionConfig::load("config/actions.yml").unwrap();
        assert_eq!(config.defaults.terminal, "powershell");
        assert_eq!(config.defaults.window, "maximized");
    }

    #[test]
    fn test_load_missing_file_falls_back_to_default() {
        // 文件不存在时，load_config_content 会退化到内置 DEFAULT_YML。
        // 因此 load 不报错，且能拿到默认配置（terminal=powershell）。
        let config = ActionConfig::load("config/definitely_does_not_exist_123abc.yml")
            .expect("load 不应失败，应回退到内置默认");
        assert_eq!(config.defaults.terminal, "powershell");
    }
}
