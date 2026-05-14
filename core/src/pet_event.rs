//! 宠物语义事件协议。
//!
//! 本模块定义 app 层发送给前端宠物窗口的稳定 IPC payload，把“发生了什么”
//! 与“播放哪一帧动画”拆开。Rig Agent、手柄、游戏等上游只发通知、反应、
//! 模式和明确动作，前端状态机再负责映射到具体视觉状态与生命周期。

use crate::agent::{ToolPhase, ToolRuntimeEvent};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 前端宠物窗口的语义事件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PetEvent {
    /// 设置或刷新一个短生命周期通知。
    Notify {
        kind: PetNotificationKind,
        body: Option<String>,
        ttl_ms: Option<u64>,
        refresh: bool,
    },
    /// 清理通知；`kind = None` 表示清理所有通知。
    ClearNotification { kind: Option<PetNotificationKind> },
    /// 设置一次最终情绪反应，可选同步显示气泡文本。
    React {
        mood: PetMood,
        speech: Option<String>,
    },
    /// 设置长生命周期模式。
    SetMode { mode: PetMode },
    /// 走到指定横向坐标。
    WalkTo { x: f32 },
    /// 显示气泡文本。
    ShowBubble { text: String },
    /// 播放舞蹈。
    PlayDance { name: String },
    /// 退出宠物窗口。
    Exit,
}

/// 短生命周期通知类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PetNotificationKind {
    AiThinking,
    ToolRunning,
    ToolBlocked,
    ToolFailed,
    Listening,
    ScreenshotObserving,
}

/// 对话或业务事件结束后的情绪反应。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PetMood {
    Idle,
    Happy,
    Confused,
    Focused,
    Caring,
    Excited,
    Sleepy,
}

/// 长生命周期模式。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PetMode {
    Idle,
    Sleep,
    GamePlay,
}

impl PetEvent {
    /// AI 正在思考或输出文本。
    pub fn ai_thinking() -> Self {
        Self::Notify {
            kind: PetNotificationKind::AiThinking,
            body: Some("思考中...".to_string()),
            ttl_ms: Some(30_000),
            refresh: true,
        }
    }

    /// 设置情绪反应。
    pub fn react(mood: PetMood) -> Self {
        Self::React { mood, speech: None }
    }

    /// 设置长期模式。
    pub fn set_mode(mode: PetMode) -> Self {
        Self::SetMode { mode }
    }

    /// 走到指定横向坐标。
    pub fn walk_to(x: f32) -> Self {
        Self::WalkTo { x }
    }

    /// 显示气泡文本。
    pub fn show_bubble(text: impl Into<String>) -> Self {
        Self::ShowBubble { text: text.into() }
    }

    /// 播放舞蹈。
    pub fn play_dance(name: impl Into<String>) -> Self {
        Self::PlayDance { name: name.into() }
    }

    /// 退出。
    pub fn exit() -> Self {
        Self::Exit
    }
}

/// 将 Rig 工具生命周期事件映射为宠物语义事件。
pub fn tool_event_to_pet_event(event: &ToolRuntimeEvent) -> Option<PetEvent> {
    match event.phase {
        ToolPhase::Planned => Some(PetEvent::Notify {
            kind: PetNotificationKind::ToolRunning,
            body: Some(event.label.clone()),
            ttl_ms: Some(30_000),
            refresh: true,
        }),
        ToolPhase::Blocked => Some(PetEvent::Notify {
            kind: PetNotificationKind::ToolBlocked,
            body: event.result_preview.clone(),
            ttl_ms: Some(15_000),
            refresh: true,
        }),
        ToolPhase::Failed => Some(PetEvent::Notify {
            kind: PetNotificationKind::ToolFailed,
            body: event.result_preview.clone(),
            ttl_ms: Some(15_000),
            refresh: true,
        }),
        ToolPhase::Finished => Some(PetEvent::ClearNotification {
            kind: Some(PetNotificationKind::ToolRunning),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{ToolKind, ToolPhase, ToolRuntimeEvent};
    use rstest::rstest;

    fn tool_event(phase: ToolPhase) -> ToolRuntimeEvent {
        ToolRuntimeEvent {
            tool_name: "shell".to_string(),
            label: "执行命令".to_string(),
            kind: ToolKind::System,
            phase,
            call_id: Some("call-1".to_string()),
            internal_call_id: "internal-1".to_string(),
            result_preview: Some("preview".to_string()),
            success: Some(false),
            elapsed_ms: Some(42),
        }
    }

    #[test]
    fn serializes_tagged_pet_event() {
        let event = PetEvent::Notify {
            kind: PetNotificationKind::AiThinking,
            body: Some("思考中...".to_string()),
            ttl_ms: Some(30_000),
            refresh: true,
        };

        insta::assert_yaml_snapshot!(event);
    }

    #[rstest]
    #[case(ToolPhase::Planned, PetNotificationKind::ToolRunning)]
    #[case(ToolPhase::Blocked, PetNotificationKind::ToolBlocked)]
    #[case(ToolPhase::Failed, PetNotificationKind::ToolFailed)]
    fn maps_tool_notification_phases(
        #[case] phase: ToolPhase,
        #[case] expected: PetNotificationKind,
    ) {
        let mapped = tool_event_to_pet_event(&tool_event(phase)).unwrap();

        match mapped {
            PetEvent::Notify { kind, .. } => assert_eq!(kind, expected),
            other => panic!("expected notify event, got {other:?}"),
        }
    }

    #[test]
    fn maps_finished_to_clear_running_notification() {
        let mapped = tool_event_to_pet_event(&tool_event(ToolPhase::Finished)).unwrap();

        assert_eq!(
            mapped,
            PetEvent::ClearNotification {
                kind: Some(PetNotificationKind::ToolRunning)
            }
        );
    }
}
