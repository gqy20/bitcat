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
    RecentScreenshotsArgs, RememberArgs, SearchMemoryArgs, ShellArgs, ToolError,
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
use tracing::{debug, info, instrument, trace};

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
        other => other,
    }
    .to_string()
}

fn tool_kind(tool_name: &str) -> ToolKind {
    match tool_name {
        "perform_dance" | "play_dance" => ToolKind::Performance,
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
            .build();

        Ok(Self { agent, config })
    }

    /// 一次性对话（非流式），等待模型返回完整回复。
    /// 适用于不需要实时显示中间结果的场景。
    pub async fn chat(&self, message: &str) -> Result<String, String> {
        self.agent
            .prompt(message)
            .max_turns(MAX_AGENT_TURNS)
            .await
            .map_err(|e| format!("AI 对话失败: {e}"))
    }

    /// 流式对话：文本和工具调用都通过结构化事件发出，返回累积的完整回复。
    #[instrument(skip(self, message, on_event), fields(msg_chars = message.chars().count()))]
    pub async fn chat_stream<F>(&self, message: &str, mut on_event: F) -> Result<String, String>
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
                            return Err(message);
                        }
                    } else {
                        debug!(internal_call_id, "tool result without planned event");
                    }
                }
                Ok(other) => {
                    debug!(item = ?other, "其他 stream item");
                }
                Err(e) => return Err(format!("AI 流错误: {e}")),
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
    summary: &'static str,
    guidance: &'static [&'static str],
}

impl ToolSpec {
    fn to_prompt_line(self) -> String {
        if self.guidance.is_empty() {
            format!("- {}：{}", self.name, self.summary)
        } else {
            format!(
                "- {}：{}；{}",
                self.name,
                self.summary,
                self.guidance.join("；")
            )
        }
    }
}

const LAUNCH_TOOL: ToolSpec = ToolSpec {
    name: "launch_program",
    description: "启动一个程序或应用",
    summary: "启动程序或应用",
    guidance: &[],
};
const SHELL_TOOL: ToolSpec = ToolSpec {
    name: "shell",
    description: "执行 PowerShell 命令并返回输出（30s 超时，输出截断至 8000 字符）",
    summary: "执行 PowerShell 命令并返回输出",
    guidance: &["只在用户明确需要系统命令或文件/进程检查时使用"],
};
const READ_FILE_TOOL: ToolSpec = ToolSpec {
    name: "read_file",
    description: "读取文件内容，支持文本文件（超过 8000 字符自动截断）",
    summary: "读取文本文件内容",
    guidance: &[],
};
const GET_TIME_TOOL: ToolSpec = ToolSpec {
    name: "get_time",
    description: "获取当前日期和时间",
    summary: "获取当前日期和时间",
    guidance: &["处理相对时间、今天/明天/整点提醒前先确认当前时间"],
};
const RECENT_SCREENSHOTS_TOOL: ToolSpec = ToolSpec {
    name: "recent_screenshots",
    description: "查询最近的截图视觉分析记录，了解用户最近在屏幕上做什么",
    summary: "查询最近截图视觉分析记录",
    guidance: &["用户问最近屏幕、刚才在做什么、观察到什么时使用"],
};
const SEARCH_MEMORY_TOOL: ToolSpec = ToolSpec {
    name: "search_memory",
    description: "Search grep-first long-term memory with text, tags, source, and importance filters.",
    summary: "检索长期记忆",
    guidance: &["用户问偏好、历史承诺、以前聊过什么时使用"],
};
const REMEMBER_TOOL: ToolSpec = ToolSpec {
    name: "remember",
    description: "Store an explicit durable long-term memory note with optional importance and tags.",
    summary: "保存明确的长期记忆",
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
    summary: "创建真实本地提醒",
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
    summary: "列出提醒",
    guidance: &["用户问提醒是否设好、有哪些提醒时使用"],
};
const CANCEL_REMINDER_TOOL: ToolSpec = ToolSpec {
    name: "cancel_reminder",
    description: "Cancel a reminder by id.",
    summary: "取消提醒",
    guidance: &["需要提醒 id，必要时先 list_reminders"],
};
const HOTKEY_TOOL: ToolSpec = ToolSpec {
    name: "send_hotkey",
    description: "模拟键盘快捷键组合（如 Alt+Tab 切窗口、Ctrl+C 复制）",
    summary: "发送系统快捷键",
    guidance: &[],
};
const CLIPBOARD_TOOL: ToolSpec = ToolSpec {
    name: "read_clipboard",
    description: "读取系统剪贴板中的文本内容",
    summary: "读取系统剪贴板文本",
    guidance: &[],
};
const FOREGROUND_TOOL: ToolSpec = ToolSpec {
    name: "force_foreground",
    description: "将指定窗口强制提到前台（需要窗口句柄 hwnd）",
    summary: "将指定窗口提到前台",
    guidance: &["需要窗口 hwnd"],
};
const PERFORM_DANCE_TOOL: ToolSpec = ToolSpec {
    name: "perform_dance",
    description: "编排并立即播放舞蹈。用户要跳舞/表演/庆祝时用。给完整 steps；动作限 jump/spin/wave/shake/idle，建议 3-8 步、每步 150-900ms。",
    summary: "编排并立即播放新舞蹈",
    guidance: &["用户要跳舞/表演/庆祝时优先使用，给完整 steps"],
};
const PLAY_DANCE_TOOL: ToolSpec = ToolSpec {
    name: "play_dance",
    description: "播放已保存舞蹈。即兴新舞用 perform_dance。",
    summary: "播放已保存舞蹈",
    guidance: &["即兴新舞用 perform_dance"],
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
];

fn build_tool_guide_prompt() -> String {
    let mut prompt = String::from("[工具使用指南]\n");
    prompt.push_str(
        "你可以调用以下工具。需要执行动作时优先使用工具；工具失败时必须如实说明，不要假装成功。\n",
    );
    for spec in TOOL_SPECS {
        prompt.push_str(&spec.to_prompt_line());
        prompt.push('\n');
    }
    prompt.push_str("[/工具使用指南]");
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

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preamble_is_non_empty() {
        let cfg = PromptsConfig::default();
        assert!(!cfg.agent.preamble.is_empty());
        assert!(cfg.agent.preamble.contains("8Bit"));
        assert!(cfg.agent.preamble.contains("猫"));
    }

    #[test]
    fn test_tool_guide_prompt_includes_reminder_rules() {
        let guide = build_tool_guide_prompt();
        assert!(guide.contains("[工具使用指南]"));
        assert!(guide.contains("create_reminder"));
        assert!(guide.contains("delay_minutes 用数字"));
        assert!(guide.contains("at=\"YYYY-MM-DD HH:MM\""));
        assert!(guide.contains("daily_time=\"HH:MM\""));
        assert!(guide.contains("创建失败必须告诉用户没有创建成功"));
    }

    #[test]
    fn test_agent_preamble_appends_tool_guide() {
        let preamble = build_agent_preamble("base prompt");
        assert!(preamble.starts_with("base prompt"));
        assert!(preamble.contains("[工具使用指南]"));
        assert!(preamble.contains("perform_dance"));
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
}
