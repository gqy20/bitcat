//! Claude Code 会话状态模型。
//!
//! 本模块把外部编码 Agent 的原始事件压缩成稳定的 `AgentSession` 状态，
//! 让 app 和前端不需要理解 Claude Code hook 的字段细节。它只保存短 preview
//! 和可排序的状态元数据，避免把大工具输入或完整对话历史写进 UI 状态。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// 外部编码 Agent 来源。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentSource {
    ClaudeCode,
}

impl AgentSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
        }
    }
}

/// 归一后的 Agent 会话状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Working,
    ToolRunning,
    Waiting,
    Compacting,
    Done,
    Interrupted,
    Error,
}

impl AgentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::ToolRunning => "tool_running",
            Self::Waiting => "waiting",
            Self::Compacting => "compacting",
            Self::Done => "done",
            Self::Interrupted => "interrupted",
            Self::Error => "error",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "空闲",
            Self::Working => "工作中",
            Self::ToolRunning => "运行工具",
            Self::Waiting => "等待处理",
            Self::Compacting => "压缩上下文",
            Self::Done => "已完成",
            Self::Interrupted => "已中断",
            Self::Error => "异常",
        }
    }

    pub fn needs_user(self) -> bool {
        matches!(self, Self::Waiting | Self::Error)
    }

    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Working | Self::ToolRunning | Self::Waiting | Self::Compacting
        )
    }

    fn sort_rank(self) -> u8 {
        match self {
            Self::Done => 0,
            Self::Waiting | Self::Error => 1,
            Self::ToolRunning | Self::Working | Self::Compacting => 2,
            Self::Interrupted => 3,
            Self::Idle => 4,
        }
    }
}

/// 当前外部 Agent 会话快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSession {
    pub session_id: String,
    pub source: AgentSource,
    pub workspace: String,
    pub status: AgentStatus,
    pub tool_name: Option<String>,
    pub tool_input_preview: Option<String>,
    pub user_prompt_preview: Option<String>,
    pub last_response_preview: Option<String>,
    pub pid: Option<u32>,
    pub updated_at_ms: u64,
    pub status_changed_at_ms: u64,
    pub needs_user: bool,
}

impl AgentSession {
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }

    pub fn workspace_name(&self) -> String {
        workspace_name(&self.workspace)
    }

    pub fn apply_event(&mut self, event: AgentSessionEvent) {
        if self.status != event.status {
            self.status_changed_at_ms = event.at_ms;
        }
        self.status = event.status;
        self.updated_at_ms = event.at_ms;
        self.needs_user = event.status.needs_user() || event.needs_user;
        if !event.workspace.is_empty() {
            self.workspace = event.workspace;
        }
        if event.tool_name.is_some() {
            self.tool_name = event.tool_name;
        }
        if event.tool_input_preview.is_some() {
            self.tool_input_preview = event.tool_input_preview;
        }
        if event.user_prompt_preview.is_some() {
            self.user_prompt_preview = event.user_prompt_preview;
        }
        if event.last_response_preview.is_some() {
            self.last_response_preview = event.last_response_preview;
        }
        if event.pid.is_some() {
            self.pid = event.pid;
        }
    }
}

/// 单条归一后的会话更新事件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionEvent {
    pub session_id: String,
    pub source: AgentSource,
    pub workspace: String,
    pub status: AgentStatus,
    pub tool_name: Option<String>,
    pub tool_input_preview: Option<String>,
    pub user_prompt_preview: Option<String>,
    pub last_response_preview: Option<String>,
    pub pid: Option<u32>,
    pub at_ms: u64,
    pub needs_user: bool,
}

impl AgentSessionEvent {
    pub fn into_session(self) -> AgentSession {
        AgentSession {
            session_id: self.session_id,
            source: self.source,
            workspace: self.workspace,
            status: self.status,
            tool_name: self.tool_name,
            tool_input_preview: self.tool_input_preview,
            user_prompt_preview: self.user_prompt_preview,
            last_response_preview: self.last_response_preview,
            pid: self.pid,
            updated_at_ms: self.at_ms,
            status_changed_at_ms: self.at_ms,
            needs_user: self.status.needs_user() || self.needs_user,
        }
    }
}

/// 前端可直接消费的会话视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionView {
    pub session_id: String,
    pub source: String,
    pub workspace: String,
    pub workspace_name: String,
    pub status: String,
    pub status_label: String,
    pub tool_name: Option<String>,
    pub tool_input_preview: Option<String>,
    pub user_prompt_preview: Option<String>,
    pub last_response_preview: Option<String>,
    pub needs_user: bool,
    pub updated_at_ms: u64,
    pub age_sec: u64,
}

impl AgentSessionView {
    pub fn from_session(session: &AgentSession, now_ms: u64) -> Self {
        Self {
            session_id: session.session_id.clone(),
            source: session.source.as_str().to_string(),
            workspace: session.workspace.clone(),
            workspace_name: session.workspace_name(),
            status: session.status.as_str().to_string(),
            status_label: session.status.label().to_string(),
            tool_name: session.tool_name.clone(),
            tool_input_preview: session.tool_input_preview.clone(),
            user_prompt_preview: session.user_prompt_preview.clone(),
            last_response_preview: session.last_response_preview.clone(),
            needs_user: session.needs_user,
            updated_at_ms: session.updated_at_ms,
            age_sec: now_ms.saturating_sub(session.updated_at_ms) / 1000,
        }
    }
}

/// 用归一事件更新会话表。
pub fn apply_session_event(sessions: &mut HashMap<String, AgentSession>, event: AgentSessionEvent) {
    let id = event.session_id.clone();
    match sessions.get_mut(&id) {
        Some(session) => session.apply_event(event),
        None => {
            sessions.insert(id, event.into_session());
        }
    }
}

/// 返回按 UI 优先级排序后的会话。
pub fn sort_sessions(mut sessions: Vec<AgentSession>) -> Vec<AgentSession> {
    sessions.sort_by(|left, right| {
        left.status
            .sort_rank()
            .cmp(&right.status.sort_rank())
            .then_with(|| right.updated_at_ms.cmp(&left.updated_at_ms))
            .then_with(|| left.workspace.cmp(&right.workspace))
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    sessions
}

/// 按字符截断 preview，避免中文落在非法字节边界。
pub fn preview_text(value: impl AsRef<str>, max_chars: usize) -> Option<String> {
    let text = value.as_ref().trim();
    if text.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut truncated = false;
    for (idx, ch) in text.chars().enumerate() {
        if idx >= max_chars {
            truncated = true;
            break;
        }
        out.push(ch);
    }
    if truncated {
        out.push('…');
    }
    Some(out)
}

fn workspace_name(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "未知项目".to_string();
    }
    Path::new(trimmed)
        .file_name()
        .and_then(|v| v.to_str())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: &str, status: AgentStatus, at_ms: u64) -> AgentSessionEvent {
        AgentSessionEvent {
            session_id: id.into(),
            source: AgentSource::ClaudeCode,
            workspace: format!("D:\\work\\{id}"),
            status,
            tool_name: None,
            tool_input_preview: None,
            user_prompt_preview: None,
            last_response_preview: None,
            pid: None,
            at_ms,
            needs_user: false,
        }
    }

    #[test]
    fn preview_truncates_on_char_boundary() {
        assert_eq!(preview_text("你好世界abc", 3).as_deref(), Some("你好世…"));
    }

    #[test]
    fn sort_prioritizes_done_waiting_active_idle() {
        let sessions = vec![
            event("idle", AgentStatus::Idle, 400).into_session(),
            event("work", AgentStatus::Working, 500).into_session(),
            event("done", AgentStatus::Done, 300).into_session(),
            event("wait", AgentStatus::Waiting, 100).into_session(),
        ];
        let sorted = sort_sessions(sessions);
        let ids: Vec<_> = sorted.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(ids, vec!["done", "wait", "work", "idle"]);
    }

    #[test]
    fn apply_event_preserves_status_changed_time_when_status_same() {
        let mut sessions = HashMap::new();
        apply_session_event(&mut sessions, event("a", AgentStatus::Working, 1000));
        apply_session_event(&mut sessions, event("a", AgentStatus::Working, 2000));
        let session = sessions.get("a").unwrap();
        assert_eq!(session.updated_at_ms, 2000);
        assert_eq!(session.status_changed_at_ms, 1000);
    }

    #[test]
    fn apply_event_updates_status_changed_time_on_transition() {
        let mut sessions = HashMap::new();
        apply_session_event(&mut sessions, event("a", AgentStatus::Working, 1000));
        apply_session_event(&mut sessions, event("a", AgentStatus::Waiting, 2000));
        let session = sessions.get("a").unwrap();
        assert_eq!(session.status_changed_at_ms, 2000);
        assert!(session.needs_user);
    }

    #[test]
    fn view_derives_workspace_name_and_age() {
        let session = event("abc", AgentStatus::Done, 1000).into_session();
        let view = AgentSessionView::from_session(&session, 6100);
        assert_eq!(view.workspace_name, "abc");
        assert_eq!(view.status, "done");
        assert_eq!(view.age_sec, 5);
    }
}
