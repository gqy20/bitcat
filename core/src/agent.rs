//! AI Agent 模块：基于 rig-core 的多轮流式对话与工具调用。
//!
//! 本模块是桌宠的"大脑"，负责与 Anthropic Claude 模型进行流式对话，并注册
//! 一组内置工具（启动程序、执行命令、读取文件等），让模型可以自主调用以完成
//! 用户指令。
//!
//! 设计上采用 rig-core 的 Agent + StreamingPrompt 模式：文本 chunk 通过
//! [`AgentStreamEvent::Text`] 实时传递给 app 层的 bubble 窗口渲染，工具调用通过
//! [`AgentStreamEvent::Tool`] 形成独立状态事件，避免混进正文流。
//!
//! 工具注册使用 `define_tool_sync!` / `define_tool_async!` 两个宏消除样板代码，
//! 每个工具只需提供名称、描述、参数 schema 和执行函数即可自动实现 `Tool` trait。
//!
//! 交互关系：`bridge::handle_button_press` 触发对话 → `PetAgent::chat_stream`
//! 执行流式推理 → 回调把文本 chunk 送往 bubble，并把工具生命周期转换为宠物
//! 语义事件。最终情绪与长期记忆候选由结构化 `AgentReaction` 接管。

use crate::ai_config::AiConfig;
use crate::permission_hook;
use crate::permission_hook::PermissionHook;
use crate::prompts::PromptsConfig;
use crate::token_tracker::{
    TokenCategory, TokenRecord, TokenUsage, new_session_id, record_token_usage,
};
use crate::tool_events::{ToolEventRecord, record_tool_event};
use crate::tools::{
    self, CancelReminderArgs, ClipboardArgs, CreateReminderArgs, ForegroundArgs, GetTimeArgs,
    HotkeyArgs, LaunchArgs, ListRemindersArgs, PerformDanceArgs, PlayDanceArgs, ReadFileArgs,
    RecentScreenshotsArgs, RememberArgs, SearchMemoryArgs, ShellArgs, StartGameArgs, ToolError,
};
use futures::StreamExt;
use rig::agent::Agent;
use rig::agent::MultiTurnStreamItem;
use rig::client::CompletionClient;
use rig::completion::{Prompt, ToolDefinition};
use rig::message::{ToolResult as RigToolResult, ToolResultContent};
use rig::providers::anthropic;
use rig::streaming::{StreamedAssistantContent, StreamedUserContent, StreamingPrompt};
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, instrument, trace, warn};

/// AI Agent 多轮工具调用的最大回合数。
///
/// 每次"模型输出 → 工具执行 → 模型再读结果"算一个 turn。设 0 时 rig 会立刻
/// 抛 `MaxTurnError`（这也是 rig 默认值触发过的坑）。给个宽裕的上限覆盖：
/// perform_dance → 再总结 ≈ 4 turn，带搜索记忆/读文件的链路可达 10+。
/// 16 留足余量又不会让异常循环无限跑（每轮至少数秒，满轮约等于几分钟超时兜底）。
const MAX_AGENT_TURNS: usize = 16;
const MAX_CONSECUTIVE_TOOL_FAILURES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPhase {
    Planned,
    Blocked,
    Finished,
    Failed,
}

impl ToolPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Blocked => "blocked",
            Self::Finished => "finished",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Utility,
    System,
    Performance,
}

impl ToolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Utility => "utility",
            Self::System => "system",
            Self::Performance => "performance",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRuntimeEvent {
    pub tool_name: String,
    pub label: String,
    pub kind: ToolKind,
    pub phase: ToolPhase,
    pub call_id: Option<String>,
    pub internal_call_id: String,
    pub result_preview: Option<String>,
    pub success: Option<bool>,
    pub elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentStreamEvent {
    Status { status: AgentStreamStatus },
    Text { text: String },
    Tool { event: ToolRuntimeEvent },
}

/// Rig 流式对话中可直接观察到的高层状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStreamStatus {
    AiWriting,
    ToolPreparing,
}

/// 对话流错误分类：区分可恢复（已有部分回复）和致命错误。
///
/// 设计参考 gomoku_ai.rs 的 retry-with-feedback 模式，但适配流式对话场景——
/// 对话有副作用工具（shell、create_reminder），不能像五子棋那样盲目 retry，
/// 因此重点放在**错误分类 + 可恢复错误的 fallback + 诊断日志**。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatError {
    /// 可恢复的流错误：模型已输出部分内容后异常终止（如 SSE [DONE]）。
    /// 调用方可选择使用已累积的文本作为部分回复。
    RecoverableStream {
        reason: String,
        original: String,
        accumulated_chars: usize,
        chunk_count: u32,
        tool_call_count: u32,
    },
    /// 致命错误：网络、认证、限流、max_turns 等，无法从已有内容恢复。
    Fatal {
        reason: String,
        original: String,
        accumulated_chars: usize,
        tool_call_count: u32,
    },
}

impl ChatError {
    /// 短错误类别标识，用于结构化日志字段。
    pub fn short_kind(&self) -> &'static str {
        match self {
            Self::RecoverableStream { .. } => "recoverable",
            Self::Fatal { .. } => "fatal",
        }
    }

    /// 是否为可恢复错误（有累积文本时可 fallback 到 Ok）。
    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::RecoverableStream { .. })
    }

    /// 原始 rig 错误文本，用于诊断日志（需用 log_preview 截断后再输出）。
    pub fn original_message(&self) -> &str {
        match self {
            Self::RecoverableStream { original, .. } | Self::Fatal { original, .. } => original,
        }
    }

    /// 已累积的字符数（用于判断是否有足够内容 fallback）。
    pub fn accumulated_chars(&self) -> usize {
        match self {
            Self::RecoverableStream {
                accumulated_chars, ..
            }
            | Self::Fatal {
                accumulated_chars, ..
            } => *accumulated_chars,
        }
    }
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatError::RecoverableStream {
                reason,
                accumulated_chars,
                ..
            } => write!(f, "AI 流错误(可恢复,{reason},{accumulated_chars}字符)"),
            ChatError::Fatal { reason, .. } => write!(f, "AI 流错误({reason})"),
        }
    }
}

/// 将 rig 原始流错误字符串分类为 [`ChatError`]。
///
/// 分类策略基于错误文本的特征匹配（不依赖 rig 内部错误类型枚举，
/// 因为 rig 的 `StreamingError` 通过 `format!` 转成字符串后丢失了类型信息）。
///
/// # 分类规则
///
/// | 特征 | 分类 | 说明 |
/// |------|------|------|
/// | `[DONE]` / `failed to parse json` + `data:` | `RecoverableStream(sse_parse)` | glm-5v-turbo 多轮 round 2 常见 |
/// | `MaxTurn` / `max_turn` | `Fatal(max_turns)` | rig 多轮超限 |
/// | timeout / connection / socket / network / dns / eof | `Fatal(network)` | 网络/连接类 |
/// | unauthorized / 401 / 403 / authentication / api_key | `Fatal(auth)` | 认证/权限 |
/// | rate_limit / 429 / too many requests | `Fatal(rate_limit)` | 限流 |
/// | 未知 + accumulated > 20 字符 | `RecoverableStream(unknown_with_content)` | 倾向视为可恢复 |
/// | 其他 | `Fatal(unknown)` | 默认致命 |
fn classify_stream_error(
    raw: &str,
    accumulated_chars: usize,
    chunk_count: u32,
    tool_call_count: u32,
) -> ChatError {
    let lower = raw.to_lowercase();

    // [DONE] / SSE parse — glm-5v-turbo 在多轮 tool result 后直接发 [DONE] 关闭流
    if lower.contains("[done]")
        || (lower.contains("failed to parse json") && lower.contains("data:"))
    {
        return ChatError::RecoverableStream {
            reason: "sse_parse".into(),
            original: raw.to_string(),
            accumulated_chars,
            chunk_count,
            tool_call_count,
        };
    }

    // MaxTurns — rig 内部多轮超限
    if lower.contains("maxturn") || lower.contains("max_turn") {
        return ChatError::Fatal {
            reason: "max_turns".into(),
            original: raw.to_string(),
            accumulated_chars,
            tool_call_count,
        };
    }

    // 网络 / 连接
    if lower.contains("timeout")
        || lower.contains("connection")
        || lower.contains("socket")
        || lower.contains("network")
        || lower.contains("dns")
        || lower.contains("eof")
    {
        return ChatError::Fatal {
            reason: "network".into(),
            original: raw.to_string(),
            accumulated_chars,
            tool_call_count,
        };
    }

    // 认证 / 权限
    if lower.contains("unauthorized")
        || lower.contains("401")
        || lower.contains("403")
        || lower.contains("authentication")
        || lower.contains("api_key")
    {
        return ChatError::Fatal {
            reason: "auth".into(),
            original: raw.to_string(),
            accumulated_chars,
            tool_call_count,
        };
    }

    // 限流
    if lower.contains("rate_limit") || lower.contains("429") || lower.contains("too many requests")
    {
        return ChatError::Fatal {
            reason: "rate_limit".into(),
            original: raw.to_string(),
            accumulated_chars,
            tool_call_count,
        };
    }

    // 默认：如果已有大量文本累积，倾向视为可恢复（模型说了些话但没正常结束）
    if accumulated_chars > 20 {
        return ChatError::RecoverableStream {
            reason: "unknown_with_content".into(),
            original: raw.to_string(),
            accumulated_chars,
            chunk_count,
            tool_call_count,
        };
    }

    ChatError::Fatal {
        reason: "unknown".into(),
        original: raw.to_string(),
        accumulated_chars,
        tool_call_count,
    }
}

const TOOL_FAILURE_STOP_PREFIX: &str = "tool_failure_stop:";

fn tool_failure_stop_message(tool_name: &str, body: &str) -> String {
    format!("{TOOL_FAILURE_STOP_PREFIX}{tool_name}:{body}")
}

pub fn parse_tool_failure_stop(error: &str) -> Option<(&str, &str)> {
    let rest = error.strip_prefix(TOOL_FAILURE_STOP_PREFIX)?;
    rest.split_once(':')
}

fn tool_label(tool_name: &str) -> String {
    match tool_name {
        "launch_program" => "启动程序",
        "shell" => "执行命令",
        "read_file" => "读取文件",
        "get_time" => "查看时间",
        "recent_screenshots" => "查看屏幕记录",
        "search_memory" => "检索记忆",
        "remember" => "保存记忆",
        "send_hotkey" => "发送快捷键",
        "read_clipboard" => "读取剪贴板",
        "force_foreground" => "切换窗口",
        "perform_dance" => "编排舞蹈",
        "play_dance" => "播放舞蹈",
        "start_game" => "启动游戏",
        other => other,
    }
    .to_string()
}

fn tool_kind(tool_name: &str) -> ToolKind {
    match tool_name {
        "perform_dance" | "play_dance" | "start_game" => ToolKind::Performance,
        "launch_program" | "shell" | "send_hotkey" | "force_foreground" => ToolKind::System,
        _ => ToolKind::Utility,
    }
}

fn planned_tool_event(
    tool_name: String,
    call_id: Option<String>,
    internal_call_id: String,
) -> ToolRuntimeEvent {
    ToolRuntimeEvent {
        label: tool_label(&tool_name),
        kind: tool_kind(&tool_name),
        tool_name,
        phase: ToolPhase::Planned,
        call_id,
        internal_call_id,
        result_preview: None,
        success: None,
        elapsed_ms: None,
    }
}

fn truncate_event_preview(text: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 120;
    let count = text.chars().count();
    if count <= MAX_PREVIEW_CHARS {
        text.to_string()
    } else {
        format!(
            "{}...",
            text.chars().take(MAX_PREVIEW_CHARS).collect::<String>()
        )
    }
}

fn tool_result_preview(result: &RigToolResult) -> Option<String> {
    result
        .content
        .iter()
        .map(|content| match content {
            ToolResultContent::Text(text) => truncate_event_preview(&text.text),
            ToolResultContent::Image(_) => "[image]".to_string(),
        })
        .next()
}

fn tool_result_is_failure(preview: Option<&str>, success: Option<bool>) -> bool {
    success == Some(false)
        || preview.is_some_and(|text| {
            text.contains("Toolset error")
                || text.contains("ToolCallError")
                || text.contains("JsonError")
                || text.contains("\"success\":false")
                || text.contains("\"success\": false")
        })
}

fn result_tool_event(
    mut planned: ToolRuntimeEvent,
    result: &RigToolResult,
    internal_call_id: String,
    elapsed_ms: Option<u64>,
) -> ToolRuntimeEvent {
    let preview = tool_result_preview(result);
    let blocked = preview
        .as_deref()
        .is_some_and(permission_hook::is_policy_block_reason);
    let success = preview
        .as_deref()
        .and_then(|text| serde_json::from_str::<tools::ToolResult>(text).ok())
        .map(|result| result.success);
    let failed = tool_result_is_failure(preview.as_deref(), success);

    planned.phase = if blocked {
        ToolPhase::Blocked
    } else if failed {
        ToolPhase::Failed
    } else {
        ToolPhase::Finished
    };
    planned.call_id = result
        .call_id
        .clone()
        .or(planned.call_id)
        .or(Some(result.id.clone()));
    planned.internal_call_id = internal_call_id;
    planned.result_preview = preview;
    planned.success = if blocked || failed {
        Some(false)
    } else {
        success
    };
    planned.elapsed_ms = elapsed_ms;
    planned
}

fn repeated_tool_failure_message(tool_name: &str, failures: usize) -> String {
    match tool_name {
        "create_reminder" => format!(
            "提醒没有创建成功：create_reminder 连续失败 {failures} 次。请不要继续重试工具，告诉主人提醒没有创建成功，并说明最后一次工具错误；如果时间不明确，请主人换成更明确的时间。"
        ),
        _ => format!(
            "{tool_name} 连续失败 {failures} 次。请不要继续重试工具，告诉主人这次操作没有完成，并说明最后一次错误。"
        ),
    }
}

fn should_stop_after_tool_failure(
    event: &ToolRuntimeEvent,
    last_failed_tool: &mut Option<String>,
    consecutive_failures: &mut usize,
) -> Option<String> {
    if event.phase != ToolPhase::Failed {
        *last_failed_tool = None;
        *consecutive_failures = 0;
        return None;
    }

    if last_failed_tool.as_deref() == Some(event.tool_name.as_str()) {
        *consecutive_failures += 1;
    } else {
        *last_failed_tool = Some(event.tool_name.clone());
        *consecutive_failures = 1;
    }

    (*consecutive_failures >= MAX_CONSECUTIVE_TOOL_FAILURES).then(|| {
        tool_failure_stop_message(
            &event.tool_name,
            &repeated_tool_failure_message(&event.tool_name, *consecutive_failures),
        )
    })
}

/// 桌宠 AI Agent，封装 rig-core Agent 和运行时配置。
///
/// 通过 `new()` 从 `AiConfig` + `PromptsConfig` 构建，内部注册了全部内置工具，
/// 对外暴露 `chat`（一次性）和 `chat_stream`（流式）两个对话入口。
pub struct PetAgent {
    pub agent: Agent<anthropic::completion::CompletionModel, PermissionHook>,
    pub config: AiConfig,
}

impl PetAgent {
    /// 从配置文件构建 Agent：加载 AI 密钥、模型、提示词，注册全部工具。
    pub fn new() -> Result<Self, String> {
        let config = AiConfig::load()?;
        let prompts = PromptsConfig::load();
        let preamble = build_agent_preamble(&prompts.agent.preamble);

        let client = anthropic::Client::builder()
            .api_key(&config.api_key)
            .base_url(&config.base_url)
            .build()
            .map_err(|e| format!("创建 Anthropic Client 失败: {e}"))?;

        let model = client.completion_model(config.model.as_str());

        let max_tokens = config.max_tokens();

        let agent = rig::agent::AgentBuilder::new(model)
            .preamble(&preamble)
            .max_tokens(max_tokens)
            .hook(PermissionHook)
            .tool(LaunchTool)
            .tool(ShellTool)
            .tool(ReadFileTool)
            .tool(GetTimeTool)
            .tool(RecentScreenshotsTool)
            .tool(SearchMemoryTool)
            .tool(RememberTool)
            .tool(CreateReminderTool)
            .tool(ListRemindersTool)
            .tool(CancelReminderTool)
            .tool(HotkeyTool)
            .tool(ClipboardTool)
            .tool(ForegroundTool)
            .tool(PerformDanceTool)
            .tool(PlayDanceTool)
            .tool(StartGameTool)
            .build();

        Ok(Self { agent, config })
    }

    /// 一次性对话（非流式），等待模型返回完整回复。
    /// 适用于不需要实时显示中间结果的场景。
    pub async fn chat(&self, message: &str) -> Result<String, ChatError> {
        self.agent
            .prompt(message)
            .max_turns(MAX_AGENT_TURNS)
            .await
            .map_err(|e| {
                let raw = e.to_string();
                classify_stream_error(&raw, 0, 0, 0)
            })
    }

    /// 流式对话：文本和工具调用都通过结构化事件发出，返回累积的完整回复。
    ///
    /// 错误处理策略：
    /// - 可恢复错误（如 SSE `[DONE]`）+ 已有累积文本 → 返回 `Ok(accumulated)` 而非报错
    /// - 致命错误（网络、认证等）→ 返回 `Err(ChatError::Fatal)` 供调用方生成友好消息
    /// - 每条错误都输出结构化诊断日志（session_id / error_kind / accumulated_chars 等）
    #[instrument(skip(self, message, on_event), fields(msg_chars = message.chars().count()))]
    pub async fn chat_stream<F>(&self, message: &str, mut on_event: F) -> Result<String, ChatError>
    where
        F: FnMut(AgentStreamEvent),
    {
        let session_id = new_session_id();
        let mut stream = self
            .agent
            .stream_prompt(message.to_string())
            .multi_turn(MAX_AGENT_TURNS)
            .await;

        let mut accumulated = String::new();
        let mut chunk_count = 0u32;
        let mut tool_call_count = 0u32;
        let mut final_response_count = 0u32;
        let mut writing_active = false;
        let mut last_failed_tool: Option<String> = None;
        let mut consecutive_tool_failures = 0usize;
        let mut tool_events =
            std::collections::HashMap::<String, (ToolRuntimeEvent, std::time::Instant)>::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(
                    text,
                ))) => {
                    if !writing_active {
                        writing_active = true;
                        on_event(AgentStreamEvent::Status {
                            status: AgentStreamStatus::AiWriting,
                        });
                    }
                    accumulated.push_str(&text.text);
                    on_event(AgentStreamEvent::Text {
                        text: text.text.clone(),
                    });
                    chunk_count += 1;
                    trace!(chunk_chars = text.text.chars().count(), "text chunk");
                }
                Ok(MultiTurnStreamItem::StreamAssistantItem(
                    StreamedAssistantContent::ToolCall {
                        tool_call,
                        internal_call_id,
                        ..
                    },
                )) => {
                    writing_active = false;
                    on_event(AgentStreamEvent::Status {
                        status: AgentStreamStatus::ToolPreparing,
                    });
                    tool_call_count += 1;
                    let event = planned_tool_event(
                        tool_call.function.name.clone(),
                        tool_call.call_id.or(Some(tool_call.id)),
                        internal_call_id,
                    );
                    info!(tool = %event.tool_name, phase = ?event.phase, "tool call planned");
                    tool_events.insert(
                        event.internal_call_id.clone(),
                        (event.clone(), std::time::Instant::now()),
                    );
                    on_event(AgentStreamEvent::Tool { event });
                }
                Ok(MultiTurnStreamItem::FinalResponse(res)) => {
                    final_response_count += 1;
                    let usage = TokenUsage::from(res.usage());
                    record_token_usage(
                        &TokenRecord::new(
                            session_id.clone(),
                            TokenCategory::Chat,
                            self.config.model.clone(),
                            usage.clone(),
                        )
                        .with_extra(format!("turn={final_response_count}")),
                    );

                    // 兜底：某些 provider 可能在 FinalResponse 中才暴露完整文本，
                    // 或 multi-turn 末轮的文本仅出现在此处。安全追加（去重）。
                    let final_text = res.response();
                    if !final_text.is_empty() && !accumulated.ends_with(final_text) {
                        info!(
                            final_text_chars = final_text.len(),
                            "appending final response text"
                        );
                        accumulated.push_str(final_text);
                    }

                    info!(
                        chars = res.response().len(),
                        input_tokens = usage.input_tokens,
                        output_tokens = usage.output_tokens,
                        total_tokens = usage.total_tokens,
                        cache_read_tokens = usage.cache_read_tokens,
                        cache_write_tokens = usage.cache_write_tokens,
                        "final response"
                    );
                }
                Ok(MultiTurnStreamItem::StreamUserItem(StreamedUserContent::ToolResult {
                    tool_result,
                    internal_call_id,
                })) => {
                    writing_active = false;
                    if let Some((planned, started_at)) = tool_events.remove(&internal_call_id) {
                        let elapsed_ms = started_at.elapsed().as_millis().try_into().ok();
                        let event =
                            result_tool_event(planned, &tool_result, internal_call_id, elapsed_ms);
                        info!(
                            tool = %event.tool_name,
                            phase = ?event.phase,
                            success = ?event.success,
                            elapsed_ms = ?event.elapsed_ms,
                            "tool call result"
                        );
                        record_tool_event(&ToolEventRecord::from_event(session_id.clone(), &event));
                        let stop_message = should_stop_after_tool_failure(
                            &event,
                            &mut last_failed_tool,
                            &mut consecutive_tool_failures,
                        );
                        on_event(AgentStreamEvent::Tool { event });
                        if let Some(message) = stop_message {
                            return Err(ChatError::Fatal {
                                reason: "tool_failure_stop".into(),
                                original: message,
                                accumulated_chars: accumulated.chars().count(),
                                tool_call_count,
                            });
                        }
                    } else {
                        debug!(internal_call_id, "tool result without planned event");
                    }
                }
                Ok(other) => {
                    debug!(item = ?other, "其他 stream item");
                }
                Err(e) => {
                    let error_str = e.to_string();
                    let acc_chars = accumulated.chars().count();

                    // 分类错误
                    let classified =
                        classify_stream_error(&error_str, acc_chars, chunk_count, tool_call_count);

                    // 结构化诊断日志（每条错误都记录，方便后续复盘）
                    warn!(
                        session_id = %session_id,
                        error_kind = %classified.short_kind(),
                        error_reason = %match &classified {
                            ChatError::RecoverableStream { reason, .. } | ChatError::Fatal { reason, .. } => reason.as_str(),
                        },
                        error_original = %crate::logging::log_preview(&error_str, 200),
                        accumulated_chars = acc_chars,
                        chunk_count,
                        tool_call_count,
                        final_response_count,
                        "chat stream error"
                    );

                    // 可恢复错误 + 有累积文本 → 返回已有内容而非报错
                    if classified.is_recoverable() && !accumulated.is_empty() {
                        info!(
                            recovered_chars = acc_chars,
                            reason = %match &classified {
                                ChatError::RecoverableStream { reason, .. } => reason.as_str(),
                                _ => unreachable!(),
                            },
                            "recovering from stream error, returning accumulated text"
                        );
                        return Ok(accumulated);
                    }

                    // 致命错误或无累积文本 → 返回分类后的结构化错误
                    return Err(classified);
                }
            }
        }
        info!(
            chunk_count,
            tool_call_count,
            chars = accumulated.chars().count(),
            "stream complete"
        );
        Ok(accumulated)
    }
}

// ---- 工具定义宏：消除样板代码 ----

fn tool_schema<T: JsonSchema>() -> Value {
    let schema = schemars::schema_for!(T);
    serde_json::to_value(schema).expect("tool args schema should serialize")
}

#[derive(Debug, Clone, Copy)]
struct ToolSpec {
    name: &'static str,
    description: &'static str,
    guidance: &'static [&'static str],
}

impl ToolSpec {
    fn to_policy_line(self) -> Option<String> {
        if self.guidance.is_empty() {
            None
        } else {
            Some(format!("- {}：{}", self.name, self.guidance.join("；")))
        }
    }
}

const LAUNCH_TOOL: ToolSpec = ToolSpec {
    name: "launch_program",
    description: "启动一个程序或应用",
    guidance: &[],
};
const SHELL_TOOL: ToolSpec = ToolSpec {
    name: "shell",
    description: "执行 PowerShell 命令并返回输出（30s 超时，输出截断至 8000 字符）",
    guidance: &["只在用户明确需要系统命令或文件/进程检查时使用"],
};
const READ_FILE_TOOL: ToolSpec = ToolSpec {
    name: "read_file",
    description: "读取文件内容，支持文本文件（超过 8000 字符自动截断）",
    guidance: &[],
};
const GET_TIME_TOOL: ToolSpec = ToolSpec {
    name: "get_time",
    description: "获取当前日期和时间",
    guidance: &[],
};
const RECENT_SCREENSHOTS_TOOL: ToolSpec = ToolSpec {
    name: "recent_screenshots",
    description: "查询最近的截图视觉分析记录，了解用户最近在屏幕上做什么",
    guidance: &[],
};
const SEARCH_MEMORY_TOOL: ToolSpec = ToolSpec {
    name: "search_memory",
    description: "Search grep-first long-term memory with text, tags, source, and importance filters.",
    guidance: &[],
};
const REMEMBER_TOOL: ToolSpec = ToolSpec {
    name: "remember",
    description: "Store an explicit durable long-term memory note with optional importance and tags.",
    guidance: &["只保存用户明确要求记住或确有长期价值的信息"],
};
const CREATE_REMINDER_DESC: &str = "Create a deterministic local reminder that will pop up later. Field rules are strict: \
for one-shot relative requests like 'in 3 minutes' or '3 minutes later', use schedule_kind='once' and delay_minutes as a number; \
for one-shot clock-time requests like 'today at 10:00', '10点提醒我', or 'tonight 22:10', use schedule_kind='once' and at as local 'YYYY-MM-DD HH:MM'; \
for repeated requests like 'every 30 minutes', use schedule_kind='interval' and interval_minutes as a number; \
for daily repeated requests like 'every day at 09:00', use schedule_kind='daily' and daily_time as 'HH:MM'. \
Do not use daily for a single clock-time reminder. Do not put numbers in string fields except inside formatted time strings. If creation fails, tell the user it was not created instead of claiming success.";
const CREATE_REMINDER_TOOL: ToolSpec = ToolSpec {
    name: "create_reminder",
    description: CREATE_REMINDER_DESC,
    guidance: &[
        "几分钟/几小时后提醒我：schedule_kind=once，delay_minutes 用数字",
        "今天/明天/十点/晚上十点提醒我：schedule_kind=once，at=\"YYYY-MM-DD HH:MM\"",
        "每天十点提醒我：schedule_kind=daily，daily_time=\"HH:MM\"",
        "每隔 N 分钟提醒我：schedule_kind=interval，interval_minutes 用数字",
        "创建失败必须告诉用户没有创建成功，不要口头承诺",
    ],
};
const LIST_REMINDERS_TOOL: ToolSpec = ToolSpec {
    name: "list_reminders",
    description: "List active reminders, or include inactive reminders when requested.",
    guidance: &[],
};
const CANCEL_REMINDER_TOOL: ToolSpec = ToolSpec {
    name: "cancel_reminder",
    description: "Cancel a reminder by id.",
    guidance: &[],
};
const HOTKEY_TOOL: ToolSpec = ToolSpec {
    name: "send_hotkey",
    description: "模拟键盘快捷键组合（如 Alt+Tab 切窗口、Ctrl+C 复制）",
    guidance: &[],
};
const CLIPBOARD_TOOL: ToolSpec = ToolSpec {
    name: "read_clipboard",
    description: "读取系统剪贴板中的文本内容",
    guidance: &[],
};
const FOREGROUND_TOOL: ToolSpec = ToolSpec {
    name: "force_foreground",
    description: "将指定窗口强制提到前台（需要窗口句柄 hwnd）",
    guidance: &[],
};
const PERFORM_DANCE_TOOL: ToolSpec = ToolSpec {
    name: "perform_dance",
    description: "编排并立即播放舞蹈。用户要跳舞/表演/庆祝时用。给完整 steps；动作限 jump/spin/wave/shake/idle，建议 3-8 步、每步 150-900ms。",
    guidance: &[],
};
const PLAY_DANCE_TOOL: ToolSpec = ToolSpec {
    name: "play_dance",
    description: "播放已保存舞蹈。即兴新舞用 perform_dance。",
    guidance: &[],
};
const START_GAME_TOOL: ToolSpec = ToolSpec {
    name: "start_game",
    description: "启动内置小游戏。kind 只能是 snake、memory、catch、battle、gomoku、arena；用户说玩游戏、来一局、下五子棋、猫猫擂台或玩贪吃蛇时使用。用户要求背单词、练词汇、指定主题/难度词汇时，kind=snake 并提供 vocab_pack。",
    guidance: &[
        "snake 背单词组卷：vocab_pack.mode=\"meaning_choice\"，answer_count 默认 4，target_correct 建议 8-12",
        "每个 vocab_pack.entries 项必须有 id、term、meaning；distractors 给 3 个中文干扰项，不能重复正确释义",
        "词表按用户主题生成：如编程、四级、英语面试、日常；不要生成过长词表，建议 8-16 个 entries",
    ],
};

const TOOL_SPECS: &[ToolSpec] = &[
    LAUNCH_TOOL,
    SHELL_TOOL,
    READ_FILE_TOOL,
    GET_TIME_TOOL,
    RECENT_SCREENSHOTS_TOOL,
    SEARCH_MEMORY_TOOL,
    REMEMBER_TOOL,
    CREATE_REMINDER_TOOL,
    LIST_REMINDERS_TOOL,
    CANCEL_REMINDER_TOOL,
    HOTKEY_TOOL,
    CLIPBOARD_TOOL,
    FOREGROUND_TOOL,
    PERFORM_DANCE_TOOL,
    PLAY_DANCE_TOOL,
    START_GAME_TOOL,
];

fn build_tool_guide_prompt() -> String {
    let mut prompt = String::from("[工具使用政策]\n");
    prompt.push_str(
        "工具的名称、用途和参数以原生 tool definition 为准，不要在系统提示词里另造参数。需要真实执行动作时使用工具；工具失败时必须如实说明，不要假装成功。系统操作类工具（shell、launch_program、send_hotkey、force_foreground）只在用户明确需要时使用。用户提到过往偏好、项目背景或曾经说过的事，而当前上下文不足时，优先用 search_memory 检索，不要猜测。\n",
    );
    for spec in TOOL_SPECS {
        if let Some(line) = spec.to_policy_line() {
            prompt.push_str(&line);
            prompt.push('\n');
        }
    }
    prompt.push_str("[/工具使用政策]");
    prompt
}

fn build_agent_preamble(base: &str) -> String {
    format!("{}\n\n{}", base.trim_end(), build_tool_guide_prompt())
}

/// 定义一个同步执行的 Tool（execute 函数返回 `ToolResult`）
macro_rules! define_tool_sync {
    ($name:ident, $spec:expr, $args_ty:ty, $exec_fn:expr) => {
        struct $name;
        impl Tool for $name {
            const NAME: &'static str = $spec.name;
            type Error = ToolError;
            type Args = $args_ty;
            type Output = tools::ToolResult;

            async fn definition(&self, _prompt: String) -> ToolDefinition {
                ToolDefinition {
                    name: Self::NAME.into(),
                    description: $spec.description.into(),
                    parameters: tool_schema::<$args_ty>(),
                }
            }

            async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
                Ok($exec_fn(&args))
            }
        }
    };
}

/// 定义一个异步执行的 Tool（execute 函数返回 `Result<ToolResult, ToolError>`）
macro_rules! define_tool_async {
    ($name:ident, $spec:expr, $args_ty:ty, $exec_fn:expr) => {
        struct $name;
        impl Tool for $name {
            const NAME: &'static str = $spec.name;
            type Error = ToolError;
            type Args = $args_ty;
            type Output = tools::ToolResult;

            async fn definition(&self, _prompt: String) -> ToolDefinition {
                ToolDefinition {
                    name: Self::NAME.into(),
                    description: $spec.description.into(),
                    parameters: tool_schema::<$args_ty>(),
                }
            }

            async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
                $exec_fn(&args).await
            }
        }
    };
}

// ---- 8 个工具定义 ----

define_tool_sync!(LaunchTool, LAUNCH_TOOL, LaunchArgs, tools::execute_launch);

define_tool_async!(ShellTool, SHELL_TOOL, ShellArgs, tools::execute_shell);

define_tool_sync!(
    ReadFileTool,
    READ_FILE_TOOL,
    ReadFileArgs,
    tools::execute_read_file
);

define_tool_sync!(
    GetTimeTool,
    GET_TIME_TOOL,
    GetTimeArgs,
    tools::execute_get_time
);

define_tool_sync!(
    RecentScreenshotsTool,
    RECENT_SCREENSHOTS_TOOL,
    RecentScreenshotsArgs,
    |args| tools::execute_recent_screenshots(args, None)
);

define_tool_sync!(
    SearchMemoryTool,
    SEARCH_MEMORY_TOOL,
    SearchMemoryArgs,
    tools::execute_search_memory_live
);

define_tool_sync!(
    RememberTool,
    REMEMBER_TOOL,
    RememberArgs,
    tools::execute_remember
);

define_tool_sync!(
    CreateReminderTool,
    CREATE_REMINDER_TOOL,
    CreateReminderArgs,
    tools::execute_create_reminder
);

define_tool_sync!(
    ListRemindersTool,
    LIST_REMINDERS_TOOL,
    ListRemindersArgs,
    tools::execute_list_reminders
);

define_tool_sync!(
    CancelReminderTool,
    CANCEL_REMINDER_TOOL,
    CancelReminderArgs,
    tools::execute_cancel_reminder
);

define_tool_sync!(HotkeyTool, HOTKEY_TOOL, HotkeyArgs, tools::execute_hotkey);

define_tool_sync!(
    ClipboardTool,
    CLIPBOARD_TOOL,
    ClipboardArgs,
    tools::execute_clipboard
);

define_tool_sync!(
    ForegroundTool,
    FOREGROUND_TOOL,
    ForegroundArgs,
    tools::execute_foreground
);

define_tool_sync!(
    PerformDanceTool,
    PERFORM_DANCE_TOOL,
    PerformDanceArgs,
    tools::execute_perform_dance
);

define_tool_sync!(
    PlayDanceTool,
    PLAY_DANCE_TOOL,
    PlayDanceArgs,
    tools::execute_play_dance
);

define_tool_sync!(
    StartGameTool,
    START_GAME_TOOL,
    StartGameArgs,
    tools::execute_start_game
);

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    #[test]
    fn test_preamble_is_non_empty() {
        let cfg = PromptsConfig::default();
        assert!(!cfg.agent.preamble.is_empty());
        assert!(cfg.agent.preamble.contains("BitCat"));
        assert!(cfg.agent.preamble.contains("桌面 AI 伙伴"));
    }

    #[test]
    fn test_tool_guide_prompt_includes_reminder_rules() {
        let guide = build_tool_guide_prompt();
        assert!(guide.contains("[工具使用政策]"));
        assert!(guide.contains("tool definition"));
        assert!(guide.contains("create_reminder"));
        assert!(guide.contains("delay_minutes 用数字"));
        assert!(guide.contains("at=\"YYYY-MM-DD HH:MM\""));
        assert!(guide.contains("daily_time=\"HH:MM\""));
        assert!(guide.contains("创建失败必须告诉用户没有创建成功"));
        assert!(guide.contains("start_game"));
        assert!(guide.contains("vocab_pack"));
        assert!(guide.contains("meaning_choice"));
        assert!(!guide.contains("perform_dance"));
    }

    #[test]
    fn test_agent_preamble_appends_tool_guide() {
        let preamble = build_agent_preamble("base prompt");
        assert!(preamble.starts_with("base prompt"));
        assert!(preamble.contains("[工具使用政策]"));
        assert!(preamble.contains("工具失败时必须如实说明"));
    }

    #[test]
    fn test_planned_tool_event_metadata() {
        let event = planned_tool_event(
            "perform_dance".to_string(),
            Some("provider-call".to_string()),
            "rig-call".to_string(),
        );
        assert_eq!(event.tool_name, "perform_dance");
        assert_eq!(event.label, "编排舞蹈");
        assert_eq!(event.kind, ToolKind::Performance);
        assert_eq!(event.kind.as_str(), "performance");
        assert_eq!(event.phase, ToolPhase::Planned);
        assert_eq!(event.phase.as_str(), "planned");
        assert_eq!(event.call_id.as_deref(), Some("provider-call"));
        assert_eq!(event.internal_call_id, "rig-call");
        assert_eq!(event.result_preview, None);
        assert_eq!(event.success, None);
        assert_eq!(event.elapsed_ms, None);
    }

    #[test]
    fn test_toolset_error_counts_as_failure() {
        assert!(tool_result_is_failure(
            Some("Toolset error: ToolCallError: JsonError: invalid type"),
            None
        ));
        assert!(tool_result_is_failure(
            Some("{\"success\":false}"),
            Some(false)
        ));
        assert!(tool_result_is_failure(
            Some("{\"output\":\"Reminder was NOT created\",\"success\":false}"),
            None
        ));
        assert!(!tool_result_is_failure(
            Some("{\"success\":true}"),
            Some(true)
        ));
    }

    #[tokio::test]
    async fn test_launch_tool_definition() {
        let def = LaunchTool.definition(String::new()).await;
        assert_eq!(def.name, "launch_program");
        assert!(!def.description.is_empty());
        let params = def.parameters.as_object().unwrap();
        assert!(params.get("properties").is_some());
        assert!(params.get("required").is_some());
    }

    #[tokio::test]
    async fn test_shell_tool_definition() {
        let def = ShellTool.definition(String::new()).await;
        assert_eq!(def.name, "shell");
        assert!(def.description.contains("PowerShell"));
    }

    #[tokio::test]
    async fn test_read_file_tool_definition() {
        let def = ReadFileTool.definition(String::new()).await;
        assert_eq!(def.name, "read_file");
    }

    #[tokio::test]
    async fn test_get_time_tool_definition() {
        let def = GetTimeTool.definition(String::new()).await;
        assert_eq!(def.name, "get_time");
        let schema = serde_json::to_string(&def.parameters).unwrap();
        assert!(schema.contains("\"full\""));
        assert!(schema.contains("\"date\""));
        assert!(schema.contains("\"time\""));
    }

    #[tokio::test]
    async fn test_memory_tool_definitions() {
        let search = SearchMemoryTool.definition(String::new()).await;
        assert_eq!(search.name, "search_memory");
        let search_schema = serde_json::to_string(&search.parameters).unwrap();
        assert!(search_schema.contains("min_importance"));

        let remember = RememberTool.definition(String::new()).await;
        assert_eq!(remember.name, "remember");
        let remember_schema = serde_json::to_string(&remember.parameters).unwrap();
        assert!(remember_schema.contains("importance"));
    }

    #[tokio::test]
    async fn test_reminder_tool_definitions() {
        let create = CreateReminderTool.definition(String::new()).await;
        assert_eq!(create.name, "create_reminder");
        let create_schema = serde_json::to_string(&create.parameters).unwrap();
        assert!(create.description.contains("YYYY-MM-DD HH:MM"));
        assert!(
            create
                .description
                .contains("Do not use daily for a single clock-time reminder")
        );
        assert!(create_schema.contains("schedule_kind"));
        assert!(create_schema.contains("delay_minutes"));
        assert!(create_schema.contains("interval_minutes"));

        let list = ListRemindersTool.definition(String::new()).await;
        assert_eq!(list.name, "list_reminders");

        let cancel = CancelReminderTool.definition(String::new()).await;
        assert_eq!(cancel.name, "cancel_reminder");
    }

    #[tokio::test]
    async fn test_hotkey_tool_definition() {
        let def = HotkeyTool.definition(String::new()).await;
        assert_eq!(def.name, "send_hotkey");
        assert!(def.description.contains("快捷键"));
    }

    #[tokio::test]
    async fn test_clipboard_tool_definition() {
        let def = ClipboardTool.definition(String::new()).await;
        assert_eq!(def.name, "read_clipboard");
    }

    #[tokio::test]
    async fn test_foreground_tool_definition() {
        let def = ForegroundTool.definition(String::new()).await;
        assert_eq!(def.name, "force_foreground");
    }

    #[tokio::test]
    async fn test_perform_dance_tool_definition() {
        let def = PerformDanceTool.definition(String::new()).await;
        assert_eq!(def.name, "perform_dance");
        assert!(def.description.contains("完整"));
        let params = def.parameters.as_object().unwrap();
        let props = params.get("properties").unwrap().as_object().unwrap();
        assert!(props.get("name").is_some());
        assert!(props.get("steps").is_some());
        let required = params.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|v| v == "name"));
        assert!(required.iter().any(|v| v == "steps"));
    }

    #[tokio::test]
    async fn test_play_dance_tool_definition() {
        let def = PlayDanceTool.definition(String::new()).await;
        assert_eq!(def.name, "play_dance");
        assert!(def.description.contains("播放"));
        let params = def.parameters.as_object().unwrap();
        let props = params.get("properties").unwrap().as_object().unwrap();
        assert!(props.get("name").is_some());
        let required = params.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|v| v == "name"));
    }

    #[tokio::test]
    async fn test_start_game_tool_definition() {
        let def = StartGameTool.definition(String::new()).await;
        assert_eq!(def.name, "start_game");
        assert!(def.description.contains("小游戏"));
        let params = def.parameters.as_object().unwrap();
        let props = params.get("properties").unwrap().as_object().unwrap();
        assert!(props.get("kind").is_some());
        let required = params.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|v| v == "kind"));
    }

    #[tokio::test]
    async fn test_tool_call_launch() {
        let (program, args) = if cfg!(windows) {
            ("cmd", "/C echo test")
        } else {
            ("echo", "test")
        };
        let args = LaunchArgs {
            program: program.into(),
            args: args.into(),
            workdir: String::new(),
            terminal: false,
        };
        let result = LaunchTool.call(args).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_tool_call_shell() {
        let args = ShellArgs {
            command: "echo hello_test".into(),
        };
        let result = ShellTool.call(args).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("hello_test"));
    }

    #[tokio::test]
    async fn test_tool_call_read_file() {
        let args = ReadFileArgs {
            path: "Cargo.toml".into(),
        };
        let result = ReadFileTool.call(args).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_tool_call_get_time() {
        let args = GetTimeArgs {
            format: tools::GetTimeFormat::Date,
        };
        let result = GetTimeTool.call(args).await.unwrap();
        assert!(result.success);
        assert!(!result.output.is_empty());
    }

    #[tokio::test]
    async fn test_recent_screenshots_tool_call_returns_result() {
        let args = RecentScreenshotsArgs { count: Some(3) };
        let result = RecentScreenshotsTool.call(args).await.unwrap();
        assert!(result.success);
        assert!(!result.output.is_empty());
    }

    // ---- classify_stream_error 测试 ----

    #[test]
    fn test_classify_done_error_is_recoverable() {
        let err = "CompletionError: ResponseError: Failed to parse JSON: expected value at line 1 column 2 (Data: [DONE])";
        let classified = classify_stream_error(err, 100, 15, 2);
        assert!(classified.is_recoverable());
        match classified {
            ChatError::RecoverableStream {
                reason,
                accumulated_chars,
                chunk_count,
                tool_call_count,
                ..
            } => {
                assert_eq!(reason, "sse_parse");
                assert_eq!(accumulated_chars, 100);
                assert_eq!(chunk_count, 15);
                assert_eq!(tool_call_count, 2);
            }
            other => panic!("expected RecoverableStream, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_max_turns_is_fatal() {
        let err = "PromptError: MaxTurnError: (reached max turn limit: 16)";
        let classified = classify_stream_error(err, 0, 0, 5);
        assert!(!classified.is_recoverable());
        match classified {
            ChatError::Fatal {
                reason,
                tool_call_count,
                ..
            } => {
                assert_eq!(reason, "max_turns");
                assert_eq!(tool_call_count, 5);
            }
            other => panic!("expected Fatal(max_turns), got {other:?}"),
        }
    }

    #[test]
    fn test_classify_network_error_is_fatal() {
        let err = "CompletionError: ProviderError: SSE Error: connection refused";
        let classified = classify_stream_error(err, 10, 3, 1);
        assert!(!classified.is_recoverable());
        match classified {
            ChatError::Fatal { reason, .. } => assert_eq!(reason, "network"),
            other => panic!("expected Fatal(network), got {other:?}"),
        }
    }

    #[test]
    fn test_classify_timeout_is_fatal() {
        let err = "CompletionError: RequestError: timeout while waiting for response";
        let classified = classify_stream_error(err, 0, 0, 0);
        assert!(!classified.is_recoverable());
        match classified {
            ChatError::Fatal { reason, .. } => assert_eq!(reason, "network"),
            other => panic!("expected Fatal(network) for timeout, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_auth_error_is_fatal() {
        let err = "CompletionError: RequestError: unauthorized (401)";
        let classified = classify_stream_error(err, 0, 0, 0);
        assert!(!classified.is_recoverable());
        match classified {
            ChatError::Fatal { reason, .. } => assert_eq!(reason, "auth"),
            other => panic!("expected Fatal(auth), got {other:?}"),
        }
    }

    #[test]
    fn test_classify_rate_limit_is_fatal() {
        let err = "CompletionError: RequestError: rate limit exceeded (429)";
        let classified = classify_stream_error(err, 50, 8, 3);
        assert!(!classified.is_recoverable());
        match classified {
            ChatError::Fatal {
                reason,
                accumulated_chars,
                ..
            } => {
                assert_eq!(reason, "rate_limit");
                assert_eq!(accumulated_chars, 50);
            }
            other => panic!("expected Fatal(rate_limit), got {other:?}"),
        }
    }

    #[test]
    fn test_classify_unknown_with_content_is_recoverable() {
        let err = "some weird error we haven't seen before";
        // 有 >20 字符累积 → 倾向可恢复
        let classified = classify_stream_error(err, 100, 10, 0);
        assert!(classified.is_recoverable());
        match classified {
            ChatError::RecoverableStream { reason, .. } => {
                assert_eq!(reason, "unknown_with_content");
            }
            other => panic!("expected RecoverableStream(unknown_with_content), got {other:?}"),
        }
    }

    #[test]
    fn test_classify_unknown_no_content_is_fatal() {
        let err = "some weird error";
        // 无累积文本 → 默认致命
        let classified = classify_stream_error(err, 5, 1, 0);
        assert!(!classified.is_recoverable());
        match classified {
            ChatError::Fatal { reason, .. } => assert_eq!(reason, "unknown"),
            other => panic!("expected Fatal(unknown), got {other:?}"),
        }
    }

    #[test]
    fn test_chat_error_display_format() {
        let err = ChatError::RecoverableStream {
            reason: "sse_parse".into(),
            original: "[DONE] error".into(),
            accumulated_chars: 42,
            chunk_count: 5,
            tool_call_count: 1,
        };
        let display = format!("{err}");
        assert!(display.contains("可恢复"));
        assert!(display.contains("sse_parse"));
        assert!(display.contains("42"));

        let fatal = ChatError::Fatal {
            reason: "network".into(),
            original: "connection refused".into(),
            accumulated_chars: 0,
            tool_call_count: 0,
        };
        let display = format!("{fatal}");
        assert!(display.contains("network"));
        assert!(!display.contains("可恢复"));
    }

    #[test]
    fn test_chat_error_short_kind() {
        let rec = ChatError::RecoverableStream {
            reason: "test".into(),
            original: String::new(),
            accumulated_chars: 0,
            chunk_count: 0,
            tool_call_count: 0,
        };
        assert_eq!(rec.short_kind(), "recoverable");

        let fat = ChatError::Fatal {
            reason: "test".into(),
            original: String::new(),
            accumulated_chars: 0,
            tool_call_count: 0,
        };
        assert_eq!(fat.short_kind(), "fatal");
    }

    #[rstest] // 需要 rstest crate（项目已有依赖）
    #[case("[DONE]", 50, true)]
    #[case("failed to parse json: expected value (Data: [DONE])", 30, true)]
    #[case("MaxTurnError: reached max turn limit", 0, false)]
    #[case("connection refused", 0, false)]
    #[case("timeout after 30s", 0, false)]
    #[case("unauthorized: 401", 0, false)]
    #[case("rate_limit: 429 too many requests", 0, false)]
    #[case("totally unknown error", 100, true)] // >20 chars → recoverable
    #[case("totally unknown error", 0, false)] // ≤20 chars → fatal
    fn test_classify_roundtrip(
        #[case] error_msg: &str,
        #[case] acc_chars: usize,
        #[case] expect_recoverable: bool,
    ) {
        let classified = classify_stream_error(error_msg, acc_chars, 0, 0);
        assert_eq!(
            classified.is_recoverable(),
            expect_recoverable,
            "error={error_msg:?}, acc={acc_chars}"
        );
    }
}
