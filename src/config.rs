//! 配置模块测试
//!
//! TDD: 先定义期望的 API 行为，再实现

use std::collections::HashMap;

// ---- 期望的数据结构（先定义接口） ----

/// 单个按键的映射信息
#[derive(Debug, Clone, PartialEq)]
pub struct ButtonInfo {
    pub name: String,
    pub aliases: Vec<String>,
    pub position: String,
}

/// Hat 方向映射
#[derive(Debug, Clone, PartialEq)]
pub struct HatDir {
    pub arrow: String,
    pub name: String,
}

/// 完整的按键配置
#[derive(Debug, Clone)]
pub struct ButtonConfig {
    pub buttons: HashMap<u32, ButtonInfo>,
    pub hat: HashMap<(i32, i32), HatDir>,
    pub dpad_hint: String,
}

impl ButtonConfig {
    /// 从 buttons.yml 加载
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        todo!()
    }

    /// 按名称反查按钮编号
    pub fn find_by_name(&self, name: &str) -> Option<u32> {
        self.buttons
            .iter()
            .find(|(_, info)| info.name == name || info.aliases.iter().any(|a| a == name))
            .map(|(&id, _)| id)
    }

    /// 获取按钮信息
    pub fn get(&self, id: u32) -> Option<&ButtonInfo> {
        self.buttons.get(&id)
    }
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_by_name_known_buttons() {
        let config = sample_config();
        assert_eq!(config.find_by_name("A"), Some(0));
        assert_eq!(config.find_by_name("B"), Some(1));
        assert_eq!(config.find_by_name("Start"), Some(11));
        assert_eq!(config.find_by_name("Home"), Some(12));
    }

    #[test]
    fn test_find_by_name_alias() {
        let config = sample_config();
        assert_eq!(config.find_by_name("确认"), Some(0));  // A 的别名
        assert_eq!(config.find_by_name("LB"), Some(6));    // L1 的别名
    }

    #[test]
    fn test_find_by_name_unknown() {
        let config = sample_config();
        assert_eq!(config.find_by_name("NotExist"), None);
    }

    #[test]
    fn test_get_button_info() {
        let config = sample_config();
        let a = config.get(0).unwrap();
        assert_eq!(a.name, "A");
        assert_eq!(a.aliases, vec!["确认"]);
        assert_eq!(a.position, "面键-右下");
    }

    #[test]
    fn test_get_unknown_button() {
        let config = sample_config();
        assert!(config.get(99).is_none());
    }

    #[test]
    fn test_hat_directions() {
        let config = sample_config();
        let up = config.hat.get(&(0, 1)).unwrap();
        assert_eq!(up.arrow, "↑");
        assert_eq!(up.name, "上");

        let down = config.hat.get(&(0, -1)).unwrap();
        assert_eq!(down.arrow, "↓");
        assert_eq!(down.name, "下");
    }

    // ---- 辅助函数 ----

    fn sample_config() -> ButtonConfig {
        let mut buttons = HashMap::new();
        buttons.insert(0, ButtonInfo {
            name: "A".into(),
            aliases: vec!["确认".into()],
            position: "面键-右下".into(),
        });
        buttons.insert(1, ButtonInfo {
            name: "B".into(),
            aliases: vec!["取消".into()],
            position: "面键-右中".into(),
        });
        buttons.insert(6, ButtonInfo {
            name: "L1".into(),
            aliases: vec!["LB".into(), "左肩键".into()],
            position: "左上边缘".into(),
        });
        buttons.insert(11, ButtonInfo {
            name: "Start".into(),
            aliases: vec!["开始".into()],
            position: "中间偏右".into(),
        });
        buttons.insert(12, ButtonInfo {
            name: "Home".into(),
            aliases: vec!["Home".into(), "心形".into()],
            position: "中间正上".into(),
        });

        let mut hat = HashMap::new();
        hat.insert((0, 1), HatDir { arrow: "↑".into(), name: "上".into() });
        hat.insert((0, -1), HatDir { arrow: "↓".into(), name: "下".into() });
        hat.insert((-1, 0), HatDir { arrow: "←".into(), name: "左".into() });
        hat.insert((1, 0), HatDir { arrow: "→".into(), name: "右".into() });

        ButtonConfig {
            buttons,
            hat,
            dpad_hint: "按住 Select + ↑ 五秒激活方向键".into(),
        }
    }
}
