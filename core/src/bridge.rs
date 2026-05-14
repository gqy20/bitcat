//! 手柄 → AI → 宠物动画 桥接层。
//!
//! 本模块负责将 SDL2 手柄按键翻译为 AI 对话请求或明确宠物动作命令。
//! Agent 回复的语义反应由 Rig 生命周期事件和后续结构化输出接管，不再在这里
//! 通过关键词推断情绪。
//!
//! `PetStateName` 独立于 `pet::PetState` 存在，是因为 IPC 序列化需要零依赖的
//! 纯数据枚举——不携带帧索引、计时器等运行时状态，确保 JSON 线路上传输的内容
//! 稳定且安全。两端通过 `From` trait 互转。
//!
//! 按键映射流程：`handle_button_press` 根据按钮索引查 `default_button_mapping`，
//! 返回 (可选 AI 消息, 可选宠物命令) 二元组；app 层的 `gamepad_loop` 消费这对
//! 结果，分别决定是否发起对话、移动或播放舞蹈。

use serde::{Deserialize, Serialize};
use tracing::debug;

// ---- IPC 命令协议 ----

/// app 层发送给 pet 窗口的命令，通过 JSON 行协议序列化。
///
/// 每个变体对应一种明确宠物动作：移动、显示气泡文本、播放舞蹈或退出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum PetCommand {
    /// 走到指定位置
    WalkTo { x: f32 },
    /// 显示对话气泡文本
    ShowBubble { text: String },
    /// 退出
    Exit,
    /// 播放舞蹈
    PlayDance { name: String },
}

/// 宠物状态名称（IPC 安全，不含帧数据）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PetStateName {
    Idle,
    Walk,
    Sleep,
    Talk,
    Happy,
    Confused,
    GamePlay,
    GameWin,
    GameLose,
}

impl From<PetStateName> for crate::pet::PetState {
    fn from(name: PetStateName) -> Self {
        match name {
            PetStateName::Idle => crate::pet::PetState::Idle,
            PetStateName::Walk => crate::pet::PetState::Walk,
            PetStateName::Sleep => crate::pet::PetState::Sleep,
            PetStateName::Talk => crate::pet::PetState::Talk,
            PetStateName::Happy => crate::pet::PetState::Happy,
            PetStateName::Confused => crate::pet::PetState::Confused,
            PetStateName::GamePlay => crate::pet::PetState::GamePlay,
            PetStateName::GameWin => crate::pet::PetState::GameWin,
            PetStateName::GameLose => crate::pet::PetState::GameLose,
        }
    }
}

impl From<crate::pet::PetState> for PetStateName {
    fn from(state: crate::pet::PetState) -> Self {
        match state {
            crate::pet::PetState::Idle => PetStateName::Idle,
            crate::pet::PetState::Walk => PetStateName::Walk,
            crate::pet::PetState::Sleep => PetStateName::Sleep,
            crate::pet::PetState::Talk => PetStateName::Talk,
            crate::pet::PetState::Happy => PetStateName::Happy,
            crate::pet::PetState::Confused => PetStateName::Confused,
            crate::pet::PetState::GamePlay => PetStateName::GamePlay,
            crate::pet::PetState::GameWin => PetStateName::GameWin,
            crate::pet::PetState::GameLose => PetStateName::GameLose,
        }
    }
}

// ---- 按键映射 ----

/// 特殊按键功能定义
#[derive(Debug, Clone, Copy)]
pub enum SpecialAction {
    /// 触发 AI 对话
    AiChat,
    /// 让宠物睡觉/唤醒
    ToggleSleep,
    /// 让宠物开心（夸奖）
    Praise,
    /// 随机移动
    Wander,
    /// 播放舞蹈
    PlayDance,
}

/// 默认按键映射：按钮索引 → 功能
fn default_button_mapping(button_index: u32) -> Option<SpecialAction> {
    match button_index {
        11 => Some(SpecialAction::AiChat),      // Start → AI 对话
        10 => Some(SpecialAction::ToggleSleep), // Select → 睡觉
        0 => Some(SpecialAction::Praise),       // A → 开心
        1 => Some(SpecialAction::Wander),       // B → 随机走动
        4 => Some(SpecialAction::PlayDance),    // Y → 跳舞
        _ => None,
    }
}

// ---- 命令序列化 ----

impl PetCommand {
    /// 将命令序列化为单行 JSON，用于 IPC 线路传输。
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).expect("PetCommand 序列化失败")
    }

    /// 从 JSON 行反序列化命令，格式非法时返回 `None`。
    pub fn from_json_line(line: &str) -> Option<Self> {
        serde_json::from_str(line).ok()
    }
}

// ---- Bridge 逻辑 ----

/// 根据按键决定下一步动作
///
/// 返回 (发给 Agent 的消息, 发给 pet 的命令)
pub fn handle_button_press(
    button_index: u32,
    user_message: &str,
) -> (Option<String>, Option<PetCommand>) {
    debug!(
        button_index,
        has_msg = !user_message.is_empty(),
        "handle_button_press"
    );
    if let Some(action) = default_button_mapping(button_index) {
        match action {
            SpecialAction::AiChat => {
                let msg = if user_message.is_empty() {
                    "你好！有什么可以帮你的？".into()
                } else {
                    user_message.to_string()
                };
                (Some(msg), None)
            }
            SpecialAction::ToggleSleep => (None, None),
            SpecialAction::Praise => (Some("喵~ 谢谢夸奖！".into()), None),
            SpecialAction::Wander => {
                let x = rand_range(50.0, 200.0);
                (None, Some(PetCommand::WalkTo { x }))
            }
            SpecialAction::PlayDance => (
                None,
                Some(PetCommand::PlayDance {
                    name: "happy_twist".into(),
                }),
            ),
        }
    } else {
        (None, None)
    }
}

fn rand_range(lo: f32, hi: f32) -> f32 {
    use rand::Rng;
    rand::rng().random_range(lo..hi)
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    // ---- rstest 参数化：按钮映射 ----

    #[rstest]
    #[case(11, "", true, false, "Start → AI chat")]
    #[case(11, "现在几点了", true, false, "Start with message")]
    #[case(10, "", false, false, "Select → sleep handled by app semantic layer")]
    #[case(0, "", true, false, "A → praise message")]
    #[case(1, "", false, true, "B → wander")]
    #[case(4, "", false, true, "Y → dance")]
    #[case(99, "", false, false, "Unknown → no action")]
    fn test_handle_button_press(
        #[case] button: u32,
        #[case] msg: &str,
        #[case] expect_msg: bool,
        #[case] expect_cmd: bool,
        #[case] desc: &str,
    ) {
        let (m, c) = handle_button_press(button, msg);
        assert_eq!(m.is_some(), expect_msg, "{desc}");
        assert_eq!(c.is_some(), expect_cmd, "{desc}");
    }

    #[test]
    fn test_start_with_custom_message() {
        let (msg, _) = handle_button_press(11, "现在几点了");
        assert_eq!(msg.unwrap(), "现在几点了");
    }

    // ---- insta 快照：序列化 ----

    #[test]
    fn test_walk_to_json_snapshot() {
        insta::assert_snapshot!(PetCommand::WalkTo { x: 123.45 }.to_json_line());
    }

    #[test]
    fn test_show_bubble_json_snapshot() {
        insta::assert_snapshot!(
            PetCommand::ShowBubble {
                text: "你好世界".into()
            }
            .to_json_line()
        );
    }

    #[test]
    fn test_exit_json_snapshot() {
        insta::assert_snapshot!(PetCommand::Exit.to_json_line());
    }

    #[test]
    fn test_play_dance_json_snapshot() {
        insta::assert_snapshot!(
            PetCommand::PlayDance {
                name: "happy_twist".into()
            }
            .to_json_line()
        );
    }

    // ---- 其他测试 ----

    #[test]
    fn test_invalid_json_returns_none() {
        assert!(PetCommand::from_json_line("invalid").is_none());
        assert!(PetCommand::from_json_line("").is_none());
    }

    #[test]
    fn test_state_name_roundtrip() {
        use crate::pet::PetState;
        for s in [
            PetState::Idle,
            PetState::Walk,
            PetState::Sleep,
            PetState::Talk,
            PetState::Happy,
            PetState::Confused,
            PetState::GamePlay,
            PetState::GameWin,
            PetState::GameLose,
        ] {
            let name = PetStateName::from(s);
            let back: PetState = name.into();
            assert_eq!(back as usize, s as usize, "{:?} roundtrip mismatch", s);
        }
    }
}
