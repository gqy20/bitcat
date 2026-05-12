use std::collections::HashMap;
use std::fs;

use serde::Deserialize;

// ---- 数据结构 ----

#[derive(Debug, Clone, PartialEq)]
pub struct ButtonInfo {
    pub name: String,
    pub aliases: Vec<String>,
    pub position: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HatDir {
    pub arrow: String,
    pub name: String,
}

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
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        const DEFAULT_YML: &str = include_str!("../../buttons.yml");
        let content = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join(path)))
            .filter(|p| p.exists())
            .and_then(|p| fs::read_to_string(p).ok())
            .or_else(|| fs::read_to_string(path).ok())
            .unwrap_or_else(|| DEFAULT_YML.to_string());
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

    pub fn find_by_name(&self, name: &str) -> Option<u32> {
        self.buttons
            .iter()
            .find(|(_, info)| info.name == name || info.aliases.iter().any(|a| a == name))
            .map(|(&id, _)| id)
    }

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
        let config = ButtonConfig::load("buttons.yml").unwrap();
        assert!(config.buttons.contains_key(&0));
        assert!(config.buttons.contains_key(&11));
        assert!(config.hat.contains_key(&(0, 1)));
    }

    #[test]
    fn test_load_button_details() {
        let config = ButtonConfig::load("buttons.yml").unwrap();
        let a = config.get(0).unwrap();
        assert_eq!(a.name, "A");
        assert!(a.aliases.contains(&"确认".to_string()));
        assert_eq!(a.position, "面键-右下");
    }

    #[test]
    fn test_load_hat_directions() {
        let config = ButtonConfig::load("buttons.yml").unwrap();
        let up = config.hat.get(&(0, 1)).unwrap();
        assert_eq!(up.arrow, "↑");
        assert_eq!(up.name, "上");
    }

    #[test]
    fn test_load_dpad_hint() {
        let config = ButtonConfig::load("buttons.yml").unwrap();
        assert!(!config.dpad_hint.is_empty());
    }

    #[test]
    fn test_find_by_name() {
        let config = ButtonConfig::load("buttons.yml").unwrap();
        assert_eq!(config.find_by_name("A"), Some(0));
        assert_eq!(config.find_by_name("Start"), Some(11));
        assert_eq!(config.find_by_name("确认"), Some(0));
        assert_eq!(config.find_by_name("NotExist"), None);
    }

    #[test]
    fn test_load_missing_file() {
        assert!(ButtonConfig::load("nonexistent.yml").is_err());
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
