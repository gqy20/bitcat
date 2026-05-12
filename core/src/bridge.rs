//! 手柄 → AI → 宠物动画 桥接层
//!
//! 职责：
//! 1. 将手柄按键映射到 Agent 动作
//! 2. 格式化 IPC 命令（ctl→pet）
//! 3. 解析 Agent 回复，决定宠物状态变化

use serde::{Deserialize, Serialize};
use tracing::debug;

// ---- IPC 命令协议 ----

/// ctl 发给 pet 的命令（JSON 行协议）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum PetCommand {
    /// 切换状态
    SetState { state: PetStateName },
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
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).expect("PetCommand 序列化失败")
    }

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
                (
                    Some(msg),
                    Some(PetCommand::SetState {
                        state: PetStateName::Talk,
                    }),
                )
            }
            SpecialAction::ToggleSleep => (
                None,
                Some(PetCommand::SetState {
                    state: PetStateName::Sleep,
                }),
            ),
            SpecialAction::Praise => (
                Some("喵~ 谢谢夸奖！".into()),
                Some(PetCommand::SetState {
                    state: PetStateName::Happy,
                }),
            ),
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

/// 根据 Agent 回复结果决定宠物状态
pub fn resolve_agent_response(reply: &str) -> Vec<PetCommand> {
    debug!(chars = reply.chars().count(), "resolve_agent_response");
    let mut cmds = Vec::new();

    // 简单关键词检测
    if reply.contains("错误") || reply.contains("失败") || reply.contains("抱歉") {
        cmds.push(PetCommand::SetState {
            state: PetStateName::Confused,
        });
    } else if reply.contains("哈哈") || reply.contains("😄") || reply.contains("喵") {
        cmds.push(PetCommand::SetState {
            state: PetStateName::Happy,
        });
    }

    // 始终显示对话内容（按字符切片，防止 UTF-8 边界 panic）
    let char_count = reply.chars().count();
    let short = if char_count > 200 {
        let truncated: String = reply.chars().take(197).collect();
        format!("{truncated}...")
    } else {
        reply.to_string()
    };
    cmds.push(PetCommand::ShowBubble { text: short });

    // 对话结束后回 Idle
    cmds.push(PetCommand::SetState {
        state: PetStateName::Idle,
    });

    debug!(cmd_count = cmds.len(), "resolve_agent_response done");
    cmds
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
    #[case(11, "", true, true, "Start → AI chat")]
    #[case(11, "现在几点了", true, true, "Start with message")]
    #[case(10, "", false, true, "Select → sleep")]
    #[case(0, "", true, true, "A → praise")]
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
    fn test_set_state_json_snapshot() {
        insta::assert_snapshot!(
            PetCommand::SetState {
                state: PetStateName::Talk
            }
            .to_json_line()
        );
    }

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

    // ---- 其他测试 ----

    #[test]
    fn test_invalid_json_returns_none() {
        assert!(PetCommand::from_json_line("invalid").is_none());
        assert!(PetCommand::from_json_line("").is_none());
    }

    #[test]
    fn test_resolve_happy_response() {
        let cmds = resolve_agent_response("哈哈哈太有趣了！");
        assert!(cmds.len() >= 2);
        let has_happy = cmds.iter().any(|c| {
            matches!(
                c,
                PetCommand::SetState {
                    state: PetStateName::Happy
                }
            )
        });
        let has_bubble = cmds
            .iter()
            .any(|c| matches!(c, PetCommand::ShowBubble { .. }));
        assert!(has_happy);
        assert!(has_bubble);
    }

    #[test]
    fn test_resolve_error_response() {
        let cmds = resolve_agent_response("抱歉，操作失败了");
        let has_confused = cmds.iter().any(|c| {
            matches!(
                c,
                PetCommand::SetState {
                    state: PetStateName::Confused
                }
            )
        });
        assert!(has_confused);
    }

    #[test]
    fn test_resolve_long_text_truncated() {
        let long = "这是一个非常长的回复".repeat(25);
        let cmds = resolve_agent_response(&long);
        let bubble = cmds.iter().find_map(|c| match c {
            PetCommand::ShowBubble { text } => Some(text.clone()),
            _ => None,
        });
        assert!(bubble.is_some());
        let text = bubble.unwrap();
        assert_eq!(text.chars().count(), 200);
        assert!(text.ends_with("..."));
    }

    #[test]
    fn test_resolve_short_text_not_truncated() {
        let short = "短回复，不该截断";
        let cmds = resolve_agent_response(short);
        let bubble = cmds.iter().find_map(|c| match c {
            PetCommand::ShowBubble { text } => Some(text.clone()),
            _ => None,
        });
        assert_eq!(bubble.unwrap(), short);
    }

    #[test]
    fn test_resolve_truncate_no_panic_on_utf8_boundary() {
        let mixed = format!("{}{}", "a".repeat(50), "中文测试");
        let _ = resolve_agent_response(&mixed);
        let with_emoji = "喵呜~ 🐱✨ ".repeat(20);
        let cmds = resolve_agent_response(&with_emoji);
        assert!(!cmds.is_empty());
    }

    #[test]
    fn test_resolve_ends_with_idle() {
        let cmds = resolve_agent_response("普通回复");
        let last = cmds.last().unwrap();
        matches!(
            last,
            PetCommand::SetState {
                state: PetStateName::Idle
            }
        );
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
        ] {
            let name = PetStateName::from(s);
            let back: PetState = name.into();
            assert_eq!(back as usize, s as usize, "{:?} roundtrip mismatch", s);
        }
    }
}
