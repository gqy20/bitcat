//! 弹出面板动作配置。
//!
//! 本模块解析 `config/panel_action.yml`，描述面板布局、按钮展示和按钮动作。
//! 这样可以把弹出面板从前端硬编码中拆出来，让按钮数量与窗口尺寸随配置自动变化。
//! app crate 的 panel IPC 会读取本配置，生成前端 ViewModel 并执行对应动作。

use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs};

const DEFAULT_YML: &str = include_str!("../../config/panel_action.yml");

/// 面板快捷入口配置：全局默认值 + 面板按钮 id 到动作定义的映射。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PanelActionConfig {
    #[serde(default)]
    pub defaults: PanelDefaults,
    pub actions: HashMap<String, PanelActionDef>,
}

/// 面板动作的全局默认值。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PanelDefaults {
    #[serde(default = "default_terminal")]
    pub terminal: String,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default = "default_columns")]
    pub columns: u32,
    #[serde(default = "default_rows")]
    pub rows: u32,
}

impl Default for PanelDefaults {
    fn default() -> Self {
        Self {
            terminal: default_terminal(),
            width: default_width(),
            height: default_height(),
            columns: default_columns(),
            rows: default_rows(),
        }
    }
}

fn default_terminal() -> String {
    "powershell".into()
}
fn default_width() -> u32 {
    480
}
fn default_height() -> u32 {
    420
}
fn default_columns() -> u32 {
    3
}
fn default_rows() -> u32 {
    3
}

/// 单个面板快捷入口定义。
///
/// 面板支持外部程序启动、PowerShell 脚本和少量内置命令。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PanelActionDef {
    #[serde(rename = "type")]
    pub action_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub order: i32,
    #[serde(default = "default_enabled", skip_serializing_if = "is_true")]
    pub enabled: bool,
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
}

fn is_false(value: &bool) -> bool {
    !*value
}
fn is_true(value: &bool) -> bool {
    *value
}
fn is_zero(value: &i32) -> bool {
    *value == 0
}
fn default_enabled() -> bool {
    true
}

/// 前端渲染面板所需的完整视图模型。
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PanelViewModel {
    pub width: u32,
    pub height: u32,
    pub columns: u32,
    pub rows: u32,
    pub actions: Vec<PanelActionItem>,
}

/// 前端渲染单个面板按钮所需的数据。
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PanelActionItem {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub enabled: bool,
}

impl PanelActionConfig {
    /// 从 YAML 文件加载面板动作配置，文件不存在时回退到编译时嵌入的默认值。
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = crate::config::load_config_content(path, DEFAULT_YML);
        let config: PanelActionConfig = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// 序列化写回 `config/panel_action.yml`（会覆盖注释，保存前自动备份 `.bak`）。
    pub fn save(&self, path: &str) -> Result<(), String> {
        let target = crate::config::resolve_save_path(path);
        if let Ok(old) = fs::read_to_string(&target) {
            let _ = fs::write(target.with_extension("yml.bak"), old);
        }
        let header = "# 由 8Bit Cat 设置界面生成\n\
                      # 手动编辑仍然生效，但下次保存设置会覆盖注释\n\n";
        let body = serde_yaml::to_string(self).map_err(|e| e.to_string())?;
        fs::write(&target, format!("{header}{body}"))
            .map_err(|e| format!("写入 {:?} 失败: {e}", target))
    }

    /// 返回内置默认面板配置。
    pub fn default_builtin() -> Self {
        serde_yaml::from_str(DEFAULT_YML).expect("内置 config/panel_action.yml 损坏")
    }

    /// 转为前端 ViewModel。会按 `order` 再按 id 排序，并限制最多显示 `columns * rows` 项。
    pub fn to_view_model(&self) -> PanelViewModel {
        let columns = self.defaults.columns.clamp(1, 8);
        let rows = self.defaults.rows.clamp(1, 8);
        let limit = (columns * rows) as usize;
        let mut actions: Vec<(&String, &PanelActionDef)> = self.actions.iter().collect();
        actions.sort_by(|(left_id, left), (right_id, right)| {
            left.order
                .cmp(&right.order)
                .then_with(|| left_id.cmp(right_id))
        });

        PanelViewModel {
            width: self.defaults.width.clamp(240, 1200),
            height: self.defaults.height.clamp(180, 900),
            columns,
            rows,
            actions: actions
                .into_iter()
                .take(limit)
                .map(|(id, action)| PanelActionItem {
                    id: id.clone(),
                    label: if action.label.is_empty() {
                        id.clone()
                    } else {
                        action.label.clone()
                    },
                    icon: action.icon.clone(),
                    enabled: action.enabled,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_panel_action_yml() {
        let config = PanelActionConfig::load("config/panel_action.yml").unwrap();
        for id in ["game", "memory", "catch", "battle"] {
            assert!(config.actions.contains_key(id), "missing panel action {id}");
        }
    }

    #[test]
    fn test_panel_only_contains_minigames() {
        let config = PanelActionConfig::load("config/panel_action.yml").unwrap();
        assert_eq!(config.actions.len(), 4);
        for (id, action) in &config.actions {
            assert_eq!(action.action_type, "builtin", "{id} should be builtin");
            assert!(
                matches!(
                    action.command.as_deref(),
                    Some("game" | "memory" | "catch" | "battle")
                ),
                "{id} should launch a minigame"
            );
        }
    }

    #[test]
    fn test_load_missing_file_falls_back_to_default() {
        let config = PanelActionConfig::load("config/definitely_missing_panel_action.yml")
            .expect("missing file should fall back to embedded panel action config");
        assert!(config.actions.contains_key("game"));
    }

    #[test]
    fn test_view_model_uses_layout_and_order() {
        let config = PanelActionConfig::load("config/panel_action.yml").unwrap();
        let vm = config.to_view_model();
        assert_eq!((vm.width, vm.height, vm.columns, vm.rows), (480, 360, 2, 2));
        assert_eq!(vm.actions.len(), 4);
        let ids: Vec<&str> = vm.actions.iter().map(|action| action.id.as_str()).collect();
        assert_eq!(ids, vec!["game", "memory", "catch", "battle"]);
        let labels: Vec<&str> = vm
            .actions
            .iter()
            .map(|action| action.label.as_str())
            .collect();
        assert_eq!(
            labels,
            vec!["毛线球大作战", "翻牌配对", "接食物", "飞机守护战"]
        );
    }
}
