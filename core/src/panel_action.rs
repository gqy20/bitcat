//! 弹出面板动作配置。
//!
//! 本模块解析 `config/panel_action.yml`，描述面板布局、按钮展示和按钮动作。
//! 这样可以把弹出面板从前端硬编码中拆出来，让按钮数量与窗口尺寸随配置自动变化。
//! app crate 的 panel IPC 会读取本配置，生成前端 ViewModel 并执行对应动作。
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs};

const DEFAULT_YML: &str = include_str!("../../config/panel_action.yml");

/// Panel shortcut configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PanelActionConfig {
    #[serde(default)]
    pub defaults: PanelDefaults,
    pub actions: HashMap<String, PanelActionDef>,
}

/// Global defaults for panel layout and command execution.
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

/// One panel shortcut entry.
///
/// The panel supports launching programs, PowerShell scripts, and builtin commands.
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

/// View model consumed by the frontend panel.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PanelViewModel {
    pub width: u32,
    pub height: u32,
    pub columns: u32,
    pub rows: u32,
    pub actions: Vec<PanelActionItem>,
}

/// Frontend data for one panel button.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PanelActionItem {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub enabled: bool,
}

impl PanelActionConfig {
    /// Load panel actions from YAML, falling back to the embedded default.
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = crate::config::load_config_content(path, DEFAULT_YML);
        let config: PanelActionConfig = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Save panel actions to YAML and keep a `.bak` copy of the previous file.
    pub fn save(&self, path: &str) -> Result<(), String> {
        let target = crate::config::resolve_save_path(path);
        if let Ok(old) = fs::read_to_string(&target) {
            let _ = fs::write(target.with_extension("yml.bak"), old);
        }
        let header = "# 由 BitCat 设置界面生成\n\
                      # 手动编辑仍然生效，但下次保存设置会覆盖注释\n\n";
        let body = serde_yaml::to_string(self).map_err(|e| e.to_string())?;
        fs::write(&target, format!("{header}{body}"))
            .map_err(|e| format!("写入 {:?} 失败: {e}", target))
    }

    /// Return the embedded default panel configuration.
    pub fn default_builtin() -> Self {
        serde_yaml::from_str(DEFAULT_YML).expect("内置 config/panel_action.yml 损坏")
    }

    /// Convert to a sorted frontend view model limited by `columns * rows`.
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
        for id in [
            "game", "memory", "catch", "battle", "gomoku", "arena", "beads", "invasion",
        ] {
            assert!(config.actions.contains_key(id), "missing panel action {id}");
        }
    }

    #[test]
    fn test_panel_only_contains_minigames() {
        let config = PanelActionConfig::load("config/panel_action.yml").unwrap();
        assert_eq!(config.actions.len(), 8);
        for (id, action) in &config.actions {
            assert_eq!(action.action_type, "builtin", "{id} should be builtin");
            assert!(
                matches!(
                    action.command.as_deref(),
                    Some(
                        "game"
                            | "memory"
                            | "catch"
                            | "battle"
                            | "gomoku"
                            | "arena"
                            | "beads"
                            | "invasion"
                    )
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
        assert_eq!((vm.width, vm.height, vm.columns, vm.rows), (480, 420, 3, 3));
        assert_eq!(vm.actions.len(), 8);
        let ids: Vec<&str> = vm.actions.iter().map(|action| action.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "game", "memory", "catch", "battle", "gomoku", "arena", "beads", "invasion"
            ]
        );
    }
}
