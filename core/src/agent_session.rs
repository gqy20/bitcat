//! Claude Code 会话状态模型。
//!
//! 本模块把外部编码 Agent 的原始事件压缩成稳定的 `AgentSession` 状态，
//! 让 app 和前端不需要理解 Claude Code hook 的字段细节。它只保存短 preview
//! 和可排序的状态元数据，避免把大工具输入或完整对话历史写进 UI 状态。
//!
//! 路径派生（项目名、文件名 preview）刻意不用 `std::path::Path`：Agent Watch
//! 会展示远程机器上报的会话，本机可能是 Linux/macOS 而远端是 Windows，
//! `Path::file_name()` 只认当前平台分隔符，会把 `D:\work\abc` 整串当成文件名。
//! 因此这里统一按 `\` 和 `/` 双分隔符切分。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const DONE_QUIET_AFTER_SEC: u64 = 60;
/// background 子会话（子代理/后台任务）完成后的停留窗口比主会话短：
/// 它们是主任务的内部细节，快速退场避免挤占浮窗空间。
const BACKGROUND_DONE_QUIET_AFTER_SEC: u64 = 15;
/// 生命周期短于该值且从未运行过工具的会话视为脚本式短命调用，结束后直接安静。
const EPHEMERAL_LIFETIME_SEC: u64 = 10;

/// 外部编码 Agent 来源。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentSource {
    ClaudeCode,
    Codex,
    Pi,
    OpenCode,
}

/// `Waiting` 状态的具体原因：等权限批准是阻塞型，比等下一次输入更紧急。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WaitingReason {
    Permission,
    Input,
}

impl WaitingReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Permission => "permission",
            Self::Input => "input",
        }
    }
}

/// 单条事件携带的用量快照。
///
/// `cumulative = true` 表示来源给的是会话级累计值（如 opencode `session.info`），
/// 直接覆盖；`false` 表示单条消息增量（如 pi `message_end`），需要累加。
/// 成本用微美元（1e-6 USD）整数存储，避免 f64 破坏 `Eq` 派生和 JSONL 稳定性。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentUsage {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost_usd_micros: u64,
    pub cumulative: bool,
}

impl AgentSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::Codex => "codex",
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
        }
    }

    /// hook envelope 的 source 字段别名，统一小写后匹配。
    pub fn from_envelope(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "claude" | "claude-code" | "claude_code" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            "pi" => Some(Self::Pi),
            "opencode" | "open-code" | "open_code" => Some(Self::OpenCode),
            _ => None,
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
            Self::Waiting | Self::Error => 0,
            Self::ToolRunning | Self::Working | Self::Compacting => 1,
            Self::Done => 2,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    pub status: AgentStatus,
    pub tool_name: Option<String>,
    pub tool_input_preview: Option<String>,
    pub user_prompt_preview: Option<String>,
    pub last_response_preview: Option<String>,
    #[serde(default)]
    pub background: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_file: Option<String>,
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    #[serde(default)]
    pub first_seen_at_ms: u64,
    #[serde(default)]
    pub has_run_tools: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_in: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_out: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_reason: Option<WaitingReason>,
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

    /// 应用事件并返回会话是否有可见变化。
    ///
    /// `false` 表示事件与当前状态完全同质（opencode `session.updated` 这类
    /// 高频心跳），调用方应跳过 UI 推送和 nudge 评估，只做审计日志。
    pub fn apply_event(&mut self, event: AgentSessionEvent) -> bool {
        let before = self.visible_fingerprint();
        if self.status != event.status {
            self.status_changed_at_ms = event.at_ms;
        }
        self.status = event.status;
        self.updated_at_ms = event.at_ms;
        self.needs_user = event.status.needs_user() || event.needs_user;
        if !event.workspace.is_empty() {
            self.workspace = event.workspace;
        }
        if event.parent_session_id.is_some() {
            self.parent_session_id = event.parent_session_id;
        }
        let should_clear_tool =
            event.status != AgentStatus::ToolRunning && event.tool_name.is_none();
        if event.tool_name.is_some() {
            self.tool_name = event.tool_name;
        }
        if event.tool_input_preview.is_some() {
            self.tool_input_preview = event.tool_input_preview;
        }
        if should_clear_tool {
            self.tool_name = None;
            self.tool_input_preview = None;
        }
        if event.user_prompt_preview.is_some() {
            self.user_prompt_preview = event.user_prompt_preview;
        }
        if event.last_response_preview.is_some() {
            self.last_response_preview = event.last_response_preview;
        }
        if event.background {
            self.background = true;
        }
        if event.agent_id.is_some() {
            self.agent_id = event.agent_id;
        }
        if event.agent_type.is_some() {
            self.agent_type = event.agent_type;
        }
        if event.task_id.is_some() {
            self.task_id = event.task_id;
        }
        if event.output_file.is_some() {
            self.output_file = event.output_file;
        }
        if event.pid.is_some() {
            self.pid = event.pid;
        }
        if event.machine.is_some() {
            self.machine = event.machine;
        }
        if event.status == AgentStatus::ToolRunning {
            self.has_run_tools = true;
        }
        if let Some(usage) = event.usage {
            let (tokens_in, tokens_out, cost) = if usage.cumulative {
                (usage.tokens_in, usage.tokens_out, usage.cost_usd_micros)
            } else {
                (
                    self.tokens_in.unwrap_or(0).saturating_add(usage.tokens_in),
                    self.tokens_out
                        .unwrap_or(0)
                        .saturating_add(usage.tokens_out),
                    self.cost_usd_micros
                        .unwrap_or(0)
                        .saturating_add(usage.cost_usd_micros),
                )
            };
            self.tokens_in = Some(tokens_in);
            self.tokens_out = Some(tokens_out);
            self.cost_usd_micros = Some(cost);
        }
        self.waiting_reason = match event.status {
            AgentStatus::Waiting => event.waiting_reason.or(Some(WaitingReason::Input)),
            _ => None,
        };
        before != self.visible_fingerprint()
    }

    /// 参与变化检测的字段指纹。不含 `updated_at_ms`/`status_changed_at_ms`：
    /// 时间戳推进本身不算可见变化。
    fn visible_fingerprint(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(self.status.as_str().as_bytes());
        out.push(self.needs_user as u8);
        out.extend_from_slice(self.workspace.as_bytes());
        out.extend(self.tool_name.iter().flat_map(|v| v.as_bytes()));
        out.extend(self.tool_input_preview.iter().flat_map(|v| v.as_bytes()));
        out.extend(self.user_prompt_preview.iter().flat_map(|v| v.as_bytes()));
        out.extend(self.last_response_preview.iter().flat_map(|v| v.as_bytes()));
        out.extend(self.parent_session_id.iter().flat_map(|v| v.as_bytes()));
        out.extend(self.agent_id.iter().flat_map(|v| v.as_bytes()));
        out.extend(self.task_id.iter().flat_map(|v| v.as_bytes()));
        out.extend(self.output_file.iter().flat_map(|v| v.as_bytes()));
        out.extend(self.machine.iter().flat_map(|v| v.as_bytes()));
        out.extend(self.tokens_in.iter().flat_map(|v| v.to_le_bytes()));
        out.extend(self.tokens_out.iter().flat_map(|v| v.to_le_bytes()));
        out.extend(self.cost_usd_micros.iter().flat_map(|v| v.to_le_bytes()));
        out.extend(
            self.waiting_reason
                .iter()
                .flat_map(|reason| reason.as_str().as_bytes()),
        );
        out.extend_from_slice(&[self.background as u8, self.has_run_tools as u8]);
        out.extend(self.pid.iter().flat_map(|v| v.to_le_bytes()));
        out
    }
}

/// 单条归一后的会话更新事件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionEvent {
    pub session_id: String,
    pub source: AgentSource,
    pub workspace: String,
    pub parent_session_id: Option<String>,
    pub status: AgentStatus,
    pub tool_name: Option<String>,
    pub tool_input_preview: Option<String>,
    pub user_prompt_preview: Option<String>,
    pub last_response_preview: Option<String>,
    pub background: bool,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub task_id: Option<String>,
    pub output_file: Option<String>,
    pub pid: Option<u32>,
    pub machine: Option<String>,
    pub at_ms: u64,
    pub needs_user: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<AgentUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_reason: Option<WaitingReason>,
}

impl AgentSessionEvent {
    pub fn into_session(self) -> AgentSession {
        AgentSession {
            session_id: self.session_id,
            source: self.source,
            workspace: self.workspace,
            parent_session_id: self.parent_session_id,
            status: self.status,
            tool_name: self.tool_name,
            tool_input_preview: self.tool_input_preview,
            user_prompt_preview: self.user_prompt_preview,
            last_response_preview: self.last_response_preview,
            background: self.background,
            agent_id: self.agent_id,
            agent_type: self.agent_type,
            task_id: self.task_id,
            output_file: self.output_file,
            pid: self.pid,
            machine: self.machine,
            first_seen_at_ms: self.at_ms,
            has_run_tools: self.status == AgentStatus::ToolRunning,
            tokens_in: self.usage.map(|usage| usage.tokens_in),
            tokens_out: self.usage.map(|usage| usage.tokens_out),
            cost_usd_micros: self.usage.map(|usage| usage.cost_usd_micros),
            waiting_reason: self.waiting_reason,
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
    pub machine: Option<String>,
    pub parent_session_id: Option<String>,
    pub status: String,
    pub status_label: String,
    pub tool_name: Option<String>,
    pub tool_input_preview: Option<String>,
    pub user_prompt_preview: Option<String>,
    pub last_response_preview: Option<String>,
    pub background: bool,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub task_id: Option<String>,
    pub output_file: Option<String>,
    pub needs_user: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_in: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_out: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_reason: Option<WaitingReason>,
    /// 旗下仍在展示的 background 子会话统计（子代理/后台任务），
    /// 由 `attach_subtask_summaries` 在快照组装时填充。
    #[serde(default)]
    pub subtasks_active: u32,
    #[serde(default)]
    pub subtasks_recent_done: u32,
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
            machine: session.machine.clone(),
            parent_session_id: session.parent_session_id.clone(),
            status: session.status.as_str().to_string(),
            status_label: session.status.label().to_string(),
            tool_name: session.tool_name.clone(),
            tool_input_preview: session.tool_input_preview.clone(),
            user_prompt_preview: session.user_prompt_preview.clone(),
            last_response_preview: session.last_response_preview.clone(),
            background: session.background,
            agent_id: session.agent_id.clone(),
            agent_type: session.agent_type.clone(),
            task_id: session.task_id.clone(),
            output_file: session.output_file.clone(),
            needs_user: session.needs_user,
            tokens_in: session.tokens_in,
            tokens_out: session.tokens_out,
            cost_usd_micros: session.cost_usd_micros,
            waiting_reason: session.waiting_reason,
            subtasks_active: 0,
            subtasks_recent_done: 0,
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
    /// 简短用量标签：金额优先（如 `$0.05`），无金额时退回 token 数（如 `12.3k`）。
    /// 空串表示没有可用数据。完整数值在 View 的 tokens/cost 字段里。
    pub usage_label: String,
    /// 子任务摘要（如 "3 个子任务运行中"），由 `attach_subtask_summaries`
    /// 在快照组装时填充；空串表示没有在展示的子会话。
    pub subtask_label: String,
    pub quiet: bool,
}

impl AgentSessionDisplay {
    pub fn from_session(session: &AgentSession, age_sec: u64) -> Self {
        let action = action_summary(session);
        let project = session.workspace_name();
        let source_label = session.source.display_name().to_string();
        let action_label = action.label.clone();
        let tone = tone_for(session.status).to_string();
        let quiet = is_quiet(session, age_sec);
        let headline = match session.status {
            AgentStatus::Waiting => match session.waiting_reason {
                Some(WaitingReason::Permission) => "等你批准操作".to_string(),
                _ => "需要你处理".to_string(),
            },
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
            usage_label: usage_label(session),
            subtask_label: String::new(),
            quiet,
        }
    }
}

/// 会话是否应从 UI 安静移除：
/// - 完成后超过停留窗口（主会话 60s，background 子会话 15s）；
/// - 或生命周期极短且从未运行过工具（`pi -p` / `opencode run` 这类脚本式
///   单句调用），避免浮窗反复冒出秒级卡片。
fn is_quiet(session: &AgentSession, age_sec: u64) -> bool {
    if session.background && session.status == AgentStatus::Done {
        return age_sec >= BACKGROUND_DONE_QUIET_AFTER_SEC;
    }
    if session.status == AgentStatus::Done && age_sec >= DONE_QUIET_AFTER_SEC {
        return true;
    }
    if matches!(
        session.status,
        AgentStatus::Done | AgentStatus::Interrupted | AgentStatus::Idle
    ) && !session.has_run_tools
    {
        let lifetime_sec = session
            .updated_at_ms
            .saturating_sub(session.first_seen_at_ms)
            / 1000;
        if lifetime_sec <= EPHEMERAL_LIFETIME_SEC {
            return true;
        }
    }
    false
}

/// 生成简短用量标签：金额优先，无金额退回 token 数。
/// 成本单位沿用上游事件（USD），不引入汇率换算。
fn usage_label(session: &AgentSession) -> String {
    if let Some(cost) = session.cost_usd_micros.filter(|cost| *cost > 0) {
        let usd = cost as f64 / 1_000_000.0;
        if usd < 0.01 {
            return "<$0.01".to_string();
        }
        if usd < 10.0 {
            return format!("${usd:.2}");
        }
        return format!("${usd:.0}");
    }
    let tokens = session
        .tokens_in
        .unwrap_or(0)
        .saturating_add(session.tokens_out.unwrap_or(0));
    if tokens == 0 {
        return String::new();
    }
    if tokens >= 1000 {
        format!("{:.1}k", tokens as f64 / 1000.0)
    } else {
        format!("{tokens}")
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
    if session.background {
        if session.agent_id.is_some() {
            let detail = session
                .user_prompt_preview
                .clone()
                .or_else(|| session.last_response_preview.clone())
                .or_else(|| session.agent_type.clone());
            return ActionSummary {
                label: "Agent".to_string(),
                headline: Some("Background agent running".to_string()),
                detail,
            };
        }
        if session.task_id.is_some() {
            let detail = session
                .user_prompt_preview
                .clone()
                .or_else(|| session.last_response_preview.clone());
            return ActionSummary {
                label: "Task".to_string(),
                headline: Some("Background task running".to_string()),
                detail,
            };
        }
    }

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
            headline: Some("正在运行".to_string()),
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

/// 取路径最后一段，同时识别 Windows（`\`）和 POSIX（`/`）分隔符。
///
/// 返回 `None` 表示路径为空或只由分隔符组成，调用方自行决定回退值。
/// 尾部分隔符会被忽略，因此 `D:\work\abc\` 和 `/home/x/abc/` 都能得到 `abc`。
fn path_last_segment(path: &str) -> Option<String> {
    let is_sep = |c: char| c == '\\' || c == '/';
    let trimmed = path.trim().trim_end_matches(is_sep);
    trimmed
        .rsplit(is_sep)
        .next()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

/// 从工具输入的文件路径里取出文件名，用于 Agent Watch 的短 preview。
fn basename(path: &str) -> String {
    path_last_segment(path).unwrap_or_else(|| path.to_string())
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

/// 用归一事件更新会话表，返回该事件是否造成可见变化。
/// 新会话（首次出现）始终视为有变化。
pub fn apply_session_event(
    sessions: &mut HashMap<String, AgentSession>,
    event: AgentSessionEvent,
) -> bool {
    let id = event.session_id.clone();
    match sessions.get_mut(&id) {
        Some(session) => session.apply_event(event),
        None => {
            sessions.insert(id, event.into_session());
            true
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

/// 把 background 子会话（子代理/后台任务）聚合计数到主会话 View 上。
///
/// 子会话是主任务的内部细节，不应在浮窗里与主会话平级占位（一个复杂任务
/// 并发 3-5 个子代理时，主任务反而被挤出视野）。此函数只填充计数与摘要
/// 文案；明细行的展开由前端按 `parent_session_id` 现场分组渲染。
/// 父会话不在快照中（或已安静）时子会话计数自然丢弃，前端会兜底独立展示。
pub fn attach_subtask_summaries(views: &mut [AgentSessionView]) {
    use std::collections::HashMap as Map;

    let mut totals: Map<String, (u32, u32)> = Map::new();
    for view in views.iter() {
        if !view.background {
            continue;
        }
        if view.display.quiet {
            continue;
        }
        let Some(parent_id) = view.parent_session_id.as_deref() else {
            continue;
        };
        let entry = totals.entry(parent_id.to_string()).or_insert((0, 0));
        match view.status.as_str() {
            "working" | "tool_running" | "waiting" | "compacting" => entry.0 += 1,
            "done" => entry.1 += 1,
            _ => {}
        }
    }

    for view in views.iter_mut() {
        let Some((active, recent_done)) = totals.get(&view.session_id).copied() else {
            continue;
        };
        view.subtasks_active = active;
        view.subtasks_recent_done = recent_done;
        view.display.subtask_label = if active > 0 {
            format!("{active} 个子任务运行中")
        } else if recent_done > 0 {
            format!("{recent_done} 个子任务刚完成")
        } else {
            String::new()
        };
    }
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

/// 从会话 workspace 路径派生可读项目名，供浮窗和 nudge 文案使用。
fn workspace_name(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "未知项目".to_string();
    }
    path_last_segment(trimmed).unwrap_or_else(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn event(id: &str, status: AgentStatus, at_ms: u64) -> AgentSessionEvent {
        AgentSessionEvent {
            session_id: id.into(),
            source: AgentSource::ClaudeCode,
            workspace: format!("D:\\work\\{id}"),
            parent_session_id: None,
            status,
            tool_name: None,
            tool_input_preview: None,
            user_prompt_preview: None,
            last_response_preview: None,
            background: false,
            agent_id: None,
            agent_type: None,
            task_id: None,
            output_file: None,
            pid: None,
            machine: None,
            at_ms,
            needs_user: false,
            usage: None,
            waiting_reason: None,
        }
    }

    #[test]
    fn preview_truncates_on_char_boundary() {
        assert_eq!(preview_text("你好世界abc", 3).as_deref(), Some("你好世…"));
    }

    #[test]
    fn sort_prioritizes_waiting_then_active_then_done_then_idle() {
        let sessions = vec![
            event("idle", AgentStatus::Idle, 400).into_session(),
            event("work", AgentStatus::Working, 500).into_session(),
            event("done", AgentStatus::Done, 300).into_session(),
            event("wait", AgentStatus::Waiting, 100).into_session(),
        ];
        let sorted = sort_sessions(sessions);
        let ids: Vec<_> = sorted.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(ids, vec!["wait", "work", "done", "idle"]);
    }

    #[test]
    fn apply_event_detects_noop_and_changed() {
        let mut sessions = HashMap::new();
        // 新会话始终算变化。
        assert!(apply_session_event(
            &mut sessions,
            event("a", AgentStatus::Working, 1000)
        ));
        // opencode session.updated 式心跳：状态/preview 全部相同 → 无变化。
        assert!(!apply_session_event(
            &mut sessions,
            event("a", AgentStatus::Working, 2000)
        ));
        // usage 累计推进是可见变化。
        let mut usage_event = event("a", AgentStatus::Working, 3000);
        usage_event.usage = Some(AgentUsage {
            tokens_in: 10,
            tokens_out: 1,
            cost_usd_micros: 0,
            cumulative: false,
        });
        assert!(apply_session_event(&mut sessions, usage_event));
        // 相同 usage 再来一次（累计后值不同仍算变化）；换回纯心跳则无变化。
        assert!(!apply_session_event(
            &mut sessions,
            event("a", AgentStatus::Working, 4000)
        ));
        // 状态迁移必然是变化。
        assert!(apply_session_event(
            &mut sessions,
            event("a", AgentStatus::Done, 5000)
        ));
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
    fn apply_event_clears_stale_tool_when_tool_finishes() {
        let mut sessions = HashMap::new();
        let mut running = event("a", AgentStatus::ToolRunning, 1000);
        running.tool_name = Some("Bash".into());
        running.tool_input_preview = Some(r#"{"command":"cargo test"}"#.into());
        apply_session_event(&mut sessions, running);
        apply_session_event(&mut sessions, event("a", AgentStatus::Working, 2000));
        let session = sessions.get("a").unwrap();
        assert_eq!(session.status, AgentStatus::Working);
        assert!(session.tool_name.is_none());
        assert!(session.tool_input_preview.is_none());
    }

    #[test]
    fn view_derives_workspace_name_and_age() {
        let mut session = event("abc", AgentStatus::Done, 1000).into_session();
        session.machine = Some("macbook-pro".into());
        let view = AgentSessionView::from_session(&session, 6100);
        assert_eq!(view.workspace_name, "abc");
        assert_eq!(view.machine.as_deref(), Some("macbook-pro"));
        assert_eq!(view.status, "done");
        assert_eq!(view.age_sec, 5);
        assert_eq!(view.display.headline, "已完成");
    }

    // ---- 跨平台路径派生 ----
    //
    // Agent Watch 会展示远程机器上报的会话：本机是 Linux/macOS 时，
    // 远端 Windows 路径必须同样能切出最后一段。这组用例锁死该行为，
    // 防止改回 `std::path::Path`（只认当前平台分隔符）。

    #[rstest]
    #[case("D:\\work\\abc", "abc")]
    #[case("D:\\C\\Desktop\\ai\\bitcat", "bitcat")]
    #[case("D:\\work\\abc\\", "abc")]
    #[case("/home/qy113/workspace/bitcat", "bitcat")]
    #[case("/home/qy113/workspace/bitcat/", "bitcat")]
    #[case("C:\\repo", "repo")]
    #[case("relative/path/proj", "proj")]
    #[case("single", "single")]
    #[case("  D:\\work\\abc  ", "abc")]
    fn workspace_name_splits_windows_and_posix_paths(#[case] path: &str, #[case] expected: &str) {
        assert_eq!(workspace_name(path), expected);
    }

    #[rstest]
    #[case("", "未知项目")]
    #[case("   ", "未知项目")]
    #[case("/", "/")]
    #[case("\\", "\\")]
    fn workspace_name_falls_back_on_degenerate_paths(#[case] path: &str, #[case] expected: &str) {
        assert_eq!(workspace_name(path), expected);
    }

    #[rstest]
    #[case("D:\\work\\abc\\src\\main.rs", "main.rs")]
    #[case("/home/x/bitcat/core/src/tools.rs", "tools.rs")]
    #[case("main.rs", "main.rs")]
    fn basename_takes_last_segment_on_any_platform(#[case] path: &str, #[case] expected: &str) {
        assert_eq!(basename(path), expected);
    }

    #[test]
    fn display_summarizes_shell_command() {
        let mut session = event("abc", AgentStatus::ToolRunning, 1000).into_session();
        session.tool_name = Some("Bash".into());
        session.tool_input_preview =
            Some(r#"{"command":"cargo nextest run -p bitcat-core"}"#.into());
        let view = AgentSessionView::from_session(&session, 6100);
        assert_eq!(view.display.tone, "active");
        assert_eq!(view.display.action_label, "Shell");
        assert_eq!(view.display.headline, "正在运行");
        assert_eq!(view.display.detail, "cargo nextest run -p bitcat-core");
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

    #[test]
    fn background_done_quiets_faster_than_main_session() {
        // 子代理是主任务的内部细节：Done 后 15s 即退场，不占 60s。
        let mut session = event("sub", AgentStatus::Done, 1000).into_session();
        session.background = true;
        session.has_run_tools = true;
        assert!(
            !AgentSessionView::from_session(&session, 10_000)
                .display
                .quiet
        );
        assert!(
            AgentSessionView::from_session(&session, 17_000)
                .display
                .quiet
        );
        // 主会话（跑过工具的长会话）仍按 60s。
        let mut main = event("main", AgentStatus::Done, 1000).into_session();
        main.has_run_tools = true;
        assert!(!AgentSessionView::from_session(&main, 17_000).display.quiet);
        assert!(AgentSessionView::from_session(&main, 62_000).display.quiet);
    }

    #[test]
    fn attach_subtask_summaries_aggregates_children_into_parent() {
        let parent = event("parent", AgentStatus::Working, 1000).into_session();
        let mut running_agent =
            event("claude:parent:agent:a", AgentStatus::ToolRunning, 2000).into_session();
        running_agent.background = true;
        running_agent.parent_session_id = Some("parent".into());
        running_agent.tool_name = Some("grep".into());
        let mut waiting_agent =
            event("claude:parent:agent:b", AgentStatus::Waiting, 2000).into_session();
        waiting_agent.background = true;
        waiting_agent.parent_session_id = Some("parent".into());
        let mut done_agent = event("claude:parent:agent:c", AgentStatus::Done, 2000).into_session();
        done_agent.background = true;
        done_agent.parent_session_id = Some("parent".into());
        // 安静的子会话不计数。
        let mut quiet_agent =
            event("claude:parent:agent:d", AgentStatus::Done, 100_000).into_session();
        quiet_agent.background = true;
        quiet_agent.parent_session_id = Some("parent".into());

        let mut views: Vec<AgentSessionView> = [
            &parent,
            &running_agent,
            &waiting_agent,
            &done_agent,
            &quiet_agent,
        ]
        .iter()
        .map(|session| AgentSessionView::from_session(session, 105_000))
        .collect();
        attach_subtask_summaries(&mut views);

        let parent_view = views.iter().find(|v| v.session_id == "parent").unwrap();
        assert_eq!(parent_view.subtasks_active, 2);
        assert_eq!(parent_view.subtasks_recent_done, 1);
        assert_eq!(parent_view.display.subtask_label, "2 个子任务运行中");
        // 子会话自身不带聚合（父不在其 parent 链上）。
        let child_view = views
            .iter()
            .find(|v| v.session_id == "claude:parent:agent:a")
            .unwrap();
        assert_eq!(child_view.subtasks_active, 0);
    }

    #[test]
    fn attach_subtask_summaries_reports_recent_done_when_none_active() {
        let parent = event("parent", AgentStatus::Working, 1000).into_session();
        let mut done = event("claude:parent:task:t1", AgentStatus::Done, 2000).into_session();
        done.background = true;
        done.parent_session_id = Some("parent".into());

        let mut views: Vec<AgentSessionView> = [&parent, &done]
            .iter()
            .map(|session| AgentSessionView::from_session(session, 3000))
            .collect();
        attach_subtask_summaries(&mut views);

        let parent_view = views.iter().find(|v| v.session_id == "parent").unwrap();
        assert_eq!(parent_view.subtasks_active, 0);
        assert_eq!(parent_view.display.subtask_label, "1 个子任务刚完成");
    }

    #[test]
    fn display_quiets_ephemeral_sessions_without_tools() {
        // pi -p / opencode run 式短命调用：无工具、生命周期 < 10s，结束时直接安静。
        let mut sessions = std::collections::HashMap::new();
        apply_session_event(&mut sessions, event("s", AgentStatus::Working, 1000));
        apply_session_event(&mut sessions, event("s", AgentStatus::Done, 6_000));
        let session = sessions.get("s").unwrap();
        assert!(!session.has_run_tools);
        let view = AgentSessionView::from_session(session, 6_500);
        assert!(view.display.quiet);
    }

    #[test]
    fn display_keeps_ephemeral_session_that_ran_tools() {
        // 同样短的生命周期，但运行过工具 → 是真实任务，不安静。
        let mut sessions = std::collections::HashMap::new();
        apply_session_event(&mut sessions, event("s", AgentStatus::ToolRunning, 1000));
        apply_session_event(&mut sessions, event("s", AgentStatus::Done, 6_000));
        let session = sessions.get("s").unwrap();
        assert!(session.has_run_tools);
        let view = AgentSessionView::from_session(session, 6_500);
        assert!(!view.display.quiet);
    }

    #[test]
    fn usage_accumulates_incremental_and_overrides_cumulative() {
        let mut sessions = std::collections::HashMap::new();
        // pi 两条 assistant 消息的增量 usage 累加。
        let mut first = event("s", AgentStatus::Working, 1000);
        first.usage = Some(AgentUsage {
            tokens_in: 1000,
            tokens_out: 20,
            cost_usd_micros: 1_500,
            cumulative: false,
        });
        apply_session_event(&mut sessions, first);
        let mut second = event("s", AgentStatus::Done, 2000);
        second.usage = Some(AgentUsage {
            tokens_in: 500,
            tokens_out: 10,
            cost_usd_micros: 500,
            cumulative: false,
        });
        apply_session_event(&mut sessions, second);
        let session = sessions.get("s").unwrap();
        assert_eq!(session.tokens_in, Some(1500));
        assert_eq!(session.tokens_out, Some(30));
        assert_eq!(session.cost_usd_micros, Some(2_000));
        // opencode 会话级累计值直接覆盖。
        let mut third = event("s", AgentStatus::Done, 3000);
        third.usage = Some(AgentUsage {
            tokens_in: 900,
            tokens_out: 9,
            cost_usd_micros: 900,
            cumulative: true,
        });
        apply_session_event(&mut sessions, third);
        let session = sessions.get("s").unwrap();
        assert_eq!(session.tokens_in, Some(900));
        assert_eq!(session.cost_usd_micros, Some(900));
    }

    #[rstest]
    #[case(Some(2_000_000), "$2.00")]
    #[case(Some(5_000), "<$0.01")]
    #[case(Some(120_000_000), "$120")]
    #[case(None, "12.3k")]
    fn usage_label_prefers_cost_over_tokens(
        #[case] cost_usd_micros: Option<u64>,
        #[case] expected: &str,
    ) {
        let mut session = event("s", AgentStatus::Done, 1000).into_session();
        session.tokens_in = Some(12_000);
        session.tokens_out = Some(345);
        session.cost_usd_micros = cost_usd_micros;
        let view = AgentSessionView::from_session(&session, 1100);
        assert_eq!(view.display.usage_label, expected);
    }

    #[test]
    fn usage_label_empty_without_data() {
        let session = event("s", AgentStatus::Working, 1000).into_session();
        let view = AgentSessionView::from_session(&session, 1100);
        assert_eq!(view.display.usage_label, "");
    }

    #[test]
    fn waiting_reason_distinguishes_permission_from_input() {
        let mut sessions = std::collections::HashMap::new();
        let mut waiting = event("s", AgentStatus::Waiting, 1000);
        waiting.waiting_reason = Some(WaitingReason::Permission);
        apply_session_event(&mut sessions, waiting);
        let view = AgentSessionView::from_session(sessions.get("s").unwrap(), 1100);
        assert_eq!(view.display.headline, "等你批准操作");

        // 恢复工作后 waiting_reason 清空。
        apply_session_event(&mut sessions, event("s", AgentStatus::Working, 2000));
        let session = sessions.get("s").unwrap();
        assert!(session.waiting_reason.is_none());
    }
}
