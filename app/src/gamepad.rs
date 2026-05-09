use ai_pad_core::bridge::{handle_button_press, resolve_agent_response, PetCommand};
use serde::{Deserialize, Serialize};

/// 前端事件 payload
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PetEvent {
    /// 状态变化（如果有）
    pub state: Option<String>,
    /// 气泡文本（如果有）
    pub bubble: Option<String>,
    /// 走到目标位置（如果有）
    pub walk_to: Option<f32>,
}

impl PetEvent {
    pub fn set_state(state: &str) -> Self {
        Self { state: Some(state.to_string()), bubble: None, walk_to: None }
    }

    pub fn bubble(text: &str) -> Self {
        Self { state: None, bubble: Some(text.to_string()), walk_to: None }
    }

    pub fn walk_to(x: f32) -> Self {
        Self { state: None, bubble: None, walk_to: Some(x) }
    }

    pub fn empty() -> Self {
        Self { state: None, bubble: None, walk_to: None }
    }
}

/// 从 PetCommand 生成前端事件列表
pub fn commands_to_events(cmds: &[PetCommand]) -> Vec<PetEvent> {
    cmds.iter().map(|cmd| match cmd {
        PetCommand::SetState { state } => {
            PetEvent::set_state(&format!("{:?}", state).to_lowercase())
        }
        PetCommand::WalkTo { x } => PetEvent::walk_to(*x),
        PetCommand::ShowBubble { text } => PetEvent::bubble(text),
        PetCommand::Exit => PetEvent::set_state("exit"),
    }).collect()
}

/// 处理手柄按键，返回要发送给前端的事件
pub fn process_button(button_index: u32) -> Vec<PetEvent> {
    let (_agent_msg, pet_cmd) = handle_button_press(button_index, "");
    let mut events = Vec::new();
    if let Some(cmd) = pet_cmd {
        events.extend(commands_to_events(&[cmd]));
    }
    // agent_msg 在集成阶段处理（需要 async Agent 调用）
    events
}

/// 处理 AI 回复，返回要发送给前端的事件
pub fn process_agent_response(reply: &str) -> Vec<PetEvent> {
    commands_to_events(&resolve_agent_response(reply))
}

// ---- 测试（TDD：先写测试） ----

#[cfg(test)]
mod tests {
    use super::*;
    use ai_pad_core::bridge::PetStateName;

    #[test]
    fn test_event_set_state() {
        let e = PetEvent::set_state("talk");
        assert_eq!(e.state, Some("talk".to_string()));
        assert_eq!(e.bubble, None);
        assert_eq!(e.walk_to, None);
    }

    #[test]
    fn test_event_bubble() {
        let e = PetEvent::bubble("喵~");
        assert_eq!(e.bubble, Some("喵~".to_string()));
        assert_eq!(e.state, None);
    }

    #[test]
    fn test_event_walk_to() {
        let e = PetEvent::walk_to(150.0);
        assert_eq!(e.walk_to, Some(150.0));
    }

    #[test]
    fn test_event_serialization() {
        let e = PetEvent::set_state("happy");
        let json = serde_json::to_string(&e).unwrap();
        let parsed: PetEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, e);
    }

    #[test]
    fn test_event_deserialization_full() {
        let json = r#"{"state":"talk","bubble":"hello","walk_to":100.0}"#;
        let e: PetEvent = serde_json::from_str(json).unwrap();
        assert_eq!(e.state, Some("talk".to_string()));
        assert_eq!(e.bubble, Some("hello".to_string()));
        assert_eq!(e.walk_to, Some(100.0));
    }

    #[test]
    fn test_commands_to_events_set_state() {
        let cmds = vec![PetCommand::SetState { state: PetStateName::Talk }];
        let events = commands_to_events(&cmds);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].state, Some("talk".to_string()));
    }

    #[test]
    fn test_commands_to_events_walk() {
        let cmds = vec![PetCommand::WalkTo { x: 200.0 }];
        let events = commands_to_events(&cmds);
        assert_eq!(events[0].walk_to, Some(200.0));
    }

    #[test]
    fn test_commands_to_events_bubble() {
        let cmds = vec![PetCommand::ShowBubble { text: "hi".into() }];
        let events = commands_to_events(&cmds);
        assert_eq!(events[0].bubble, Some("hi".to_string()));
    }

    #[test]
    fn test_process_button_start() {
        let events = process_button(11); // Start
        assert!(!events.is_empty());
        assert_eq!(events[0].state, Some("talk".to_string()));
    }

    #[test]
    fn test_process_button_select() {
        let events = process_button(10); // Select
        assert!(!events.is_empty());
        assert_eq!(events[0].state, Some("sleep".to_string()));
    }

    #[test]
    fn test_process_button_unknown() {
        let events = process_button(99); // 不存在的按钮
        assert!(events.is_empty());
    }

    #[test]
    fn test_process_button_a_is_praise() {
        let events = process_button(0); // A → Praise
        assert!(!events.is_empty());
        assert_eq!(events[0].state, Some("happy".to_string()));
    }

    #[test]
    fn test_process_agent_response_happy() {
        let events = process_agent_response("哈哈哈太有趣了！");
        assert!(events.iter().any(|e| e.state == Some("happy".to_string())));
        assert!(events.iter().any(|e| e.bubble.is_some()));
        // 最后一个事件应该是回 idle
        assert_eq!(events.last().unwrap().state, Some("idle".to_string()));
    }

    #[test]
    fn test_process_agent_response_error() {
        let events = process_agent_response("抱歉，操作失败了");
        assert!(events.iter().any(|e| e.state == Some("confused".to_string())));
    }

    #[test]
    fn test_process_agent_response_normal() {
        let events = process_agent_response("今天的天气不错");
        // 应该有 bubble + idle
        assert!(events.iter().any(|e| e.bubble.is_some()));
        assert!(events.iter().any(|e| e.state == Some("idle".to_string())));
    }
}
