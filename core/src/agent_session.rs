//! Claude Code 会话状态模型。
//!
//! 本模块把外部编码 Agent 的原始事件压缩成稳定的 `AgentSession` 状态，
//! 让 app 和前端不需要理解 Claude Code hook 的字段细节。它只保存短 preview
//! 和可排序的状态元数据，避免把大工具输入或完整对话历史写进 UI 状态。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const DONE_QUIET_AFTER_SEC: u64 = 60;

/// 外部编码 Agent 来源。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentSource {
    ClaudeCode,
    Codex,
}

impl AgentSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::Codex => "codex",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
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
        if event.machine.is_some() {
            self.machine = event.machine;
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
    pub display: AgentSessionDisplay,
}

impl AgentSessionView {
    pub fn from_session(session: &AgentSession, now_ms: u64) -> Self {
        let age_sec = now_ms.saturating_sub(session.updated_at_ms) / 1000;
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
            age_sec,
            display: AgentSessionDisplay::from_session(session, age_sec),
        }
    }
}

/// 前端浮窗可直接展示的任务摘要，避免 UI 解析 hook JSON。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionDisplay {
    pub tone: String,
    pub headline: String,
    pub detail: String,
    pub project: String,
    pub source_label: String,
    pub action_label: String,
    pub age_label: String,
    pub quiet: bool,
}

impl AgentSessionDisplay {
    pub fn from_session(session: &AgentSession, age_sec: u64) -> Self {
        let action = action_summary(session);
        let project = session.workspace_name();
        let source_label = session.source.display_name().to_string();
        let action_label = action.label.clone();
        let tone = tone_for(session.status).to_string();
        let quiet = session.status == AgentStatus::Done && age_sec >= DONE_QUIET_AFTER_SEC;
        let headline = match session.status {
            AgentStatus::Waiting => "需要你处理".to_string(),
            AgentStatus::Error => "任务遇到异常".to_string(),
            AgentStatus::Compacting => "正在压缩上下文".to_string(),
            AgentStatus::ToolRunning => action
                .headline
                .unwrap_or_else(|| "正在运行工具".to_string()),
            AgentStatus::Working => action
                .headline
                .unwrap_or_else(|| "正在思考下一步".to_string()),
            AgentStatus::Done => "已完成".to_string(),
            AgentStatus::Interrupted => "已中断".to_string(),
            AgentStatus::Idle => "空闲".to_string(),
        };
        let detail = match session.status {
            AgentStatus::Waiting => action
                .detail
                .or_else(|| session.user_prompt_preview.clone())
                .unwrap_or_else(|| format!("{project} 等待确认")),
            AgentStatus::Error => session
                .last_response_preview
                .clone()
                .or(action.detail)
                .unwrap_or_else(|| format!("{project} 返回了错误")),
            AgentStatus::Done => session
                .last_response_preview
                .as_deref()
                .and_then(|text| compact_preview(text, 72))
                .or_else(|| action.detail.clone())
                .unwrap_or_else(|| format!("{project} 的任务已结束")),
            _ => action
                .detail
                .or_else(|| session.user_prompt_preview.clone())
                .unwrap_or_else(|| project.clone()),
        };

        Self {
            tone,
            headline,
            detail,
            project,
            source_label,
            action_label,
            age_label: age_label(age_sec),
            quiet,
        }
    }
}

#[derive(Debug, Clone)]
struct ActionSummary {
    label: String,
    headline: Option<String>,
    detail: Option<String>,
}

fn tone_for(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Waiting => "needs_user",
        AgentStatus::Error => "error",
        AgentStatus::Done => "done",
        AgentStatus::Interrupted => "muted",
        AgentStatus::ToolRunning | AgentStatus::Working | AgentStatus::Compacting => "active",
        AgentStatus::Idle => "muted",
    }
}

fn action_summary(session: &AgentSession) -> ActionSummary {
    let tool = session.tool_name.as_deref().unwrap_or_default();
    let lower = tool.to_ascii_lowercase();
    let parsed = session
        .tool_input_preview
        .as_deref()
        .and_then(parse_preview_object);

    if lower.contains("applypatch") || lower.contains("apply_patch") || lower == "patch" {
        let patch = parsed
            .as_ref()
            .and_then(|input| input.get("command"))
            .map(String::as_str)
            .or(session.tool_input_preview.as_deref())
            .unwrap_or_default();
        let target = patch_target(patch);
        return ActionSummary {
            label: "Patch".to_string(),
            headline: target
                .as_deref()
                .map(|value| format!("正在修改 {value}"))
                .or_else(|| Some("正在应用补丁".to_string())),
            detail: target.or_else(|| patch_summary(patch)),
        };
    }

    if lower.contains("bash") || lower.contains("powershell") {
        let command = parsed
            .as_ref()
            .and_then(|input| input.get("command"))
            .map(|value| command_summary(value))
            .or_else(|| session.tool_input_preview.as_deref().map(command_summary));
        let description = parsed
            .as_ref()
            .and_then(|input| input.get("description"))
            .and_then(|value| compact_preview(value, 56));
        return ActionSummary {
            label: "Shell".to_string(),
            headline: Some(
                command
                    .as_deref()
                    .map(|value| format!("正在运行 {value}"))
                    .unwrap_or_else(|| "正在运行命令".to_string()),
            ),
            detail: description.or(command),
        };
    }

    if lower.contains("read") {
        let target = parsed
            .as_ref()
            .and_then(|input| input.get("file_path"))
            .map(|value| basename(value));
        return ActionSummary {
            label: "Read".to_string(),
            headline: target
                .as_deref()
                .map(|value| format!("正在读取 {value}"))
                .or_else(|| Some("正在读取文件".to_string())),
            detail: target,
        };
    }

    if lower.contains("edit") || lower.contains("write") {
        let target = parsed
            .as_ref()
            .and_then(|input| input.get("file_path"))
            .map(|value| basename(value));
        return ActionSummary {
            label: if lower.contains("write") {
                "Write"
            } else {
                "Edit"
            }
            .to_string(),
            headline: target
                .as_deref()
                .map(|value| format!("正在修改 {value}"))
                .or_else(|| Some("正在修改文件".to_string())),
            detail: target,
        };
    }

    if lower.contains("grep") || lower.contains("glob") {
        let pattern = parsed
            .as_ref()
            .and_then(|input| input.get("pattern"))
            .and_then(|value| compact_preview(value, 40));
        return ActionSummary {
            label: "Search".to_string(),
            headline: pattern
                .as_deref()
                .map(|value| format!("正在搜索 {value}"))
                .or_else(|| Some("正在搜索代码".to_string())),
            detail: pattern,
        };
    }

    if lower.contains("agent") || lower.contains("task") {
        let description = parsed
            .as_ref()
            .and_then(|input| input.get("description"))
            .and_then(|value| compact_preview(value, 64));
        return ActionSummary {
            label: "Agent".to_string(),
            headline: Some("正在分派子任务".to_string()),
            detail: description,
        };
    }

    ActionSummary {
        label: tool_label(tool),
        headline: Some("正在处理任务".to_string()),
        detail: session
            .tool_input_preview
            .as_deref()
            .and_then(generic_detail),
    }
}

fn patch_target(value: &str) -> Option<String> {
    for marker in [
        "*** Update File:",
        "*** Add File:",
        "*** Delete File:",
        "*** Move to:",
    ] {
        if let Some((_, rest)) = value.split_once(marker) {
            let target = rest.lines().next().unwrap_or_default().trim();
            if !target.is_empty() {
                return Some(basename(target));
            }
        }
    }
    None
}

fn patch_summary(value: &str) -> Option<String> {
    if value.contains("*** Begin Patch") {
        return Some("补丁正在应用".to_string());
    }
    compact_preview(value, 48)
}

fn parse_preview_object(value: &str) -> Option<HashMap<String, String>> {
    let parsed = serde_json::from_str::<serde_json::Value>(value).ok()?;
    let object = parsed.as_object()?;
    Some(
        object
            .iter()
            .filter_map(|(key, value)| preview_value_to_string(value).map(|v| (key.clone(), v)))
            .collect(),
    )
}

fn preview_value_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Bool(v) => Some(v.to_string()),
        serde_json::Value::Number(v) => Some(v.to_string()),
        other => serde_json::to_string(other).ok(),
    }
}

fn tool_label(tool: &str) -> String {
    let trimmed = tool.trim();
    if trimmed.is_empty() {
        "Task".to_string()
    } else {
        trimmed
            .strip_prefix("mcp__")
            .unwrap_or(trimmed)
            .replace("__", " / ")
    }
}

fn command_summary(value: &str) -> String {
    let text = value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches('"')
        .to_string();
    let lower = text.to_ascii_lowercase();
    if lower.starts_with('{') || lower.starts_with('[') {
        return "命令".to_string();
    }
    for prefix in ["cargo ", "npm ", "pnpm ", "yarn ", "python ", "pip "] {
        if let Some(pos) = lower.find(prefix) {
            return compact_middle(&text[pos..], 42);
        }
    }
    compact_middle(&text, 42)
}

fn generic_detail(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Some("查看任务详情".to_string());
    }
    compact_preview(trimmed, 72)
}

fn compact_preview(value: &str, max_chars: usize) -> Option<String> {
    let text = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    Some(compact_middle(&text, max_chars))
}

fn compact_middle(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }
    let head = (max_chars.saturating_mul(2) / 3).max(8);
    let tail = max_chars.saturating_sub(head).saturating_sub(1).max(4);
    let start: String = value.chars().take(head).collect();
    let end: String = value
        .chars()
        .rev()
        .take(tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{start}…{end}")
}

fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|v| v.to_str())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(path)
        .to_string()
}

fn age_label(age_sec: u64) -> String {
    if age_sec < 60 {
        format!("{age_sec}s")
    } else if age_sec < 3600 {
        format!("{}m", age_sec / 60)
    } else {
        format!("{}h", age_sec / 3600)
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
    fn sort_prioritizes_recent_done_waiting_active_idle() {
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
        assert_eq!(view.display.headline, "已完成");
    }

    #[test]
    fn display_summarizes_shell_command() {
        let mut session = event("abc", AgentStatus::ToolRunning, 1000).into_session();
        session.tool_name = Some("Bash".into());
        session.tool_input_preview =
            Some(r#"{"command":"cargo nextest run -p ai-pad-core"}"#.into());
        let view = AgentSessionView::from_session(&session, 6100);
        assert_eq!(view.display.tone, "active");
        assert_eq!(view.display.action_label, "Shell");
        assert!(view.display.headline.contains("cargo nextest"));
    }

    #[test]
    fn display_summarizes_apply_patch_without_json_headline() {
        let mut session = event("abc", AgentStatus::ToolRunning, 1000).into_session();
        session.tool_name = Some("apply_patch".into());
        session.tool_input_preview = Some(
            r#"{"command":"*** Begin Patch\n*** Update File: app/frontend/js/agent_watch.js\n@@\n-old\n+new\n*** End Patch"}"#.into(),
        );
        let view = AgentSessionView::from_session(&session, 6100);
        assert_eq!(view.display.action_label, "Patch");
        assert_eq!(view.display.headline, "正在修改 agent_watch.js");
        assert!(!view.display.headline.contains("{"));
        assert!(!view.display.detail.contains("Begin Patch"));
    }

    #[test]
    fn display_unknown_json_uses_generic_headline() {
        let mut session = event("abc", AgentStatus::ToolRunning, 1000).into_session();
        session.tool_name = Some("custom_tool".into());
        session.tool_input_preview = Some(r#"{"command":"large raw payload"}"#.into());
        let view = AgentSessionView::from_session(&session, 6100);
        assert_eq!(view.display.headline, "正在处理任务");
        assert!(!view.display.headline.contains("{"));
    }

    #[test]
    fn display_marks_old_done_as_quiet() {
        let session = event("abc", AgentStatus::Done, 1000).into_session();
        let view = AgentSessionView::from_session(&session, 62_000);
        assert!(view.display.quiet);
    }
}
