//! 手柄按键配置加载与查询
//!
//! 解析 config/buttons.yml，将 SDL2 按钮 ID 映射为人类可读名称和别名，
//! 并提供方向键 (hat) 映射。gamepad_loop 通过此模块识别按钮语义。

use std::{collections::HashMap, fs, path::PathBuf};

use serde::Deserialize;

/// 按统一优先级读取配置文件：exe 同目录 → 传入路径（CWD）→ 嵌入默认值。
pub fn load_config_content(path: &str, default: &str) -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(path)))
        .filter(|p| p.exists())
        .and_then(|p| fs::read_to_string(p).ok())
        .or_else(|| fs::read_to_string(path).ok())
        .unwrap_or_else(|| default.to_string())
}

/// 解析保存路径：优先复用加载时的路径（exe 同目录 → CWD），都不存在则写到 CWD。
pub fn resolve_save_path(path: &str) -> PathBuf {
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

// ---- 数据结构 ----

/// 按钮信息：名称、别名列表、物理位置描述
#[derive(Debug, Clone, PartialEq)]
pub struct ButtonInfo {
    pub name: String,
    pub aliases: Vec<String>,
    pub position: String,
}

/// 方向键映射：箭头符号与中文名称
#[derive(Debug, Clone, PartialEq)]
pub struct HatDir {
    pub arrow: String,
    pub name: String,
}

/// 完整的手柄按键配置，包含按钮映射、方向键映射和 D-pad 激活方式
#[derive(Debug, Clone)]
pub struct ButtonConfig {
    pub buttons: HashMap<u32, ButtonInfo>,
    pub hat: HashMap<(i32, i32), HatDir>,
    pub dpad_hint: String,
}

// ---- YAML 中间结构 ----

#[derive(Deserialize)]
struct RawButtonConfig {
    buttons: HashMap<String, RawButton>,
    hat: HashMap<String, RawHatDir>,
    dpad_activation: String,
}

#[derive(Deserialize)]
struct RawButton {
    name: String,
    aliases: Vec<String>,
    position: String,
}

#[derive(Deserialize)]
struct RawHatDir {
    arrow: String,
    name: String,
}

fn parse_hat_key(s: &str) -> Option<(i32, i32)> {
    let s = s.trim_start_matches('(').trim_end_matches(')');
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 2 {
        return None;
    }
    Some((parts[0].trim().parse().ok()?, parts[1].trim().parse().ok()?))
}

impl ButtonConfig {
    /// 从 YAML 文件加载配置，文件不存在时回退到编译时嵌入的默认值
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        const DEFAULT_YML: &str = include_str!("../../config/buttons.yml");
        let content = load_config_content(path, DEFAULT_YML);
        let raw: RawButtonConfig = serde_yaml::from_str(&content)?;

        let buttons = raw
            .buttons
            .into_iter()
            .filter_map(|(k, v)| {
                let id: u32 = k.parse().ok()?;
                Some((
                    id,
                    ButtonInfo {
                        name: v.name,
                        aliases: v.aliases,
                        position: v.position,
                    },
                ))
            })
            .collect();

        let hat = raw
            .hat
            .into_iter()
            .filter_map(|(k, v)| {
                let key = parse_hat_key(&k)?;
                Some((
                    key,
                    HatDir {
                        arrow: v.arrow,
                        name: v.name,
                    },
                ))
            })
            .collect();

        Ok(ButtonConfig {
            buttons,
            hat,
            dpad_hint: raw.dpad_activation,
        })
    }

    /// 按名称或别名查找按钮 ID
    pub fn find_by_name(&self, name: &str) -> Option<u32> {
        self.buttons
            .iter()
            .find(|(_, info)| info.name == name || info.aliases.iter().any(|a| a == name))
            .map(|(&id, _)| id)
    }

    /// 按 SDL2 按钮 ID 查询按钮信息
    pub fn get(&self, id: u32) -> Option<&ButtonInfo> {
        self.buttons.get(&id)
    }
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_buttons_yml() {
        let config = ButtonConfig::load("config/buttons.yml").unwrap();
        assert!(config.buttons.contains_key(&0));
        assert!(config.buttons.contains_key(&11));
        assert!(config.hat.contains_key(&(0, 1)));
    }

    #[test]
    fn test_load_button_details() {
        let config = ButtonConfig::load("config/buttons.yml").unwrap();
        let a = config.get(0).unwrap();
        assert_eq!(a.name, "A");
        assert!(a.aliases.contains(&"确认".to_string()));
        assert_eq!(a.position, "面键-右下");
    }

    #[test]
    fn test_load_hat_directions() {
        let config = ButtonConfig::load("config/buttons.yml").unwrap();
        let up = config.hat.get(&(0, 1)).unwrap();
        assert_eq!(up.arrow, "↑");
        assert_eq!(up.name, "上");
    }

    #[test]
    fn test_load_dpad_hint() {
        let config = ButtonConfig::load("config/buttons.yml").unwrap();
        assert!(!config.dpad_hint.is_empty());
    }

    #[test]
    fn test_find_by_name() {
        let config = ButtonConfig::load("config/buttons.yml").unwrap();
        assert_eq!(config.find_by_name("A"), Some(0));
        assert_eq!(config.find_by_name("Start"), Some(11));
        assert_eq!(config.find_by_name("确认"), Some(0));
        assert_eq!(config.find_by_name("NotExist"), None);
    }

    #[test]
    fn test_load_missing_file_falls_back_to_default() {
        // 文件不存在时，load 会退化到内置 DEFAULT_YML（buttons.yml）。
        let config = ButtonConfig::load("config/definitely_does_not_exist_123abc.yml")
            .expect("load 不应失败，应回退到内置默认");
        assert!(!config.buttons.is_empty(), "默认 buttons 不应为空");
    }

    #[test]
    fn test_parse_hat_key_valid() {
        assert_eq!(parse_hat_key("(0, 1)"), Some((0, 1)));
        assert_eq!(parse_hat_key("(-1, 0)"), Some((-1, 0)));
        assert_eq!(parse_hat_key("(1, -1)"), Some((1, -1)));
    }

    #[test]
    fn test_parse_hat_key_invalid() {
        assert_eq!(parse_hat_key("invalid"), None);
        assert_eq!(parse_hat_key(""), None);
    }
}
