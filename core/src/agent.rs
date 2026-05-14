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
    self, ClipboardArgs, ForegroundArgs, GetTimeArgs, HotkeyArgs, LaunchArgs, PerformDanceArgs,
    PlayDanceArgs, ReadFileArgs, RecentScreenshotsArgs, ShellArgs, ToolError,
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

fn tool_label(tool_name: &str) -> String {
    match tool_name {
        "launch_program" => "启动程序",
        "shell" => "执行命令",
        "read_file" => "读取文件",
        "get_time" => "查看时间",
        "recent_screenshots" => "查看屏幕记录",
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

    planned.phase = if blocked {
        ToolPhase::Blocked
    } else if success == Some(false) {
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
    planned.success = if blocked { Some(false) } else { success };
    planned.elapsed_ms = elapsed_ms;
    planned
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

        let client = anthropic::Client::builder()
            .api_key(&config.api_key)
            .base_url(&config.base_url)
            .build()
            .map_err(|e| format!("创建 Anthropic Client 失败: {e}"))?;

        let model = client.completion_model(config.model.as_str());

        let max_tokens = config.max_tokens();

        let agent = rig::agent::AgentBuilder::new(model)
            .preamble(&prompts.agent.preamble)
            .max_tokens(max_tokens)
            .hook(PermissionHook)
            .tool(LaunchTool)
            .tool(ShellTool)
            .tool(ReadFileTool)
            .tool(GetTimeTool)
            .tool(RecentScreenshotsTool)
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
                        on_event(AgentStreamEvent::Tool { event });
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

/// 定义一个同步执行的 Tool（execute 函数返回 `ToolResult`）
macro_rules! define_tool_sync {
    ($name:ident, $tool_name:literal, $desc:literal, $args_ty:ty, $exec_fn:expr) => {
        struct $name;
        impl Tool for $name {
            const NAME: &'static str = $tool_name;
            type Error = ToolError;
            type Args = $args_ty;
            type Output = tools::ToolResult;

            async fn definition(&self, _prompt: String) -> ToolDefinition {
                ToolDefinition {
                    name: $tool_name.into(),
                    description: $desc.into(),
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
    ($name:ident, $tool_name:literal, $desc:literal, $args_ty:ty, $exec_fn:expr) => {
        struct $name;
        impl Tool for $name {
            const NAME: &'static str = $tool_name;
            type Error = ToolError;
            type Args = $args_ty;
            type Output = tools::ToolResult;

            async fn definition(&self, _prompt: String) -> ToolDefinition {
                ToolDefinition {
                    name: $tool_name.into(),
                    description: $desc.into(),
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

define_tool_sync!(
    LaunchTool,
    "launch_program",
    "启动一个程序或应用",
    LaunchArgs,
    tools::execute_launch
);

define_tool_async!(
    ShellTool,
    "shell",
    "执行 PowerShell 命令并返回输出（30s 超时，输出截断至 8000 字符）",
    ShellArgs,
    tools::execute_shell
);

define_tool_sync!(
    ReadFileTool,
    "read_file",
    "读取文件内容，支持文本文件（超过 8000 字符自动截断）",
    ReadFileArgs,
    tools::execute_read_file
);

define_tool_sync!(
    GetTimeTool,
    "get_time",
    "获取当前日期和时间",
    GetTimeArgs,
    tools::execute_get_time
);

define_tool_sync!(
    RecentScreenshotsTool,
    "recent_screenshots",
    "查询最近的截图视觉分析记录，了解用户最近在屏幕上做什么",
    RecentScreenshotsArgs,
    |args| tools::execute_recent_screenshots(args, None)
);

define_tool_sync!(
    HotkeyTool,
    "send_hotkey",
    "模拟键盘快捷键组合（如 Alt+Tab 切窗口、Ctrl+C 复制）",
    HotkeyArgs,
    tools::execute_hotkey
);

define_tool_sync!(
    ClipboardTool,
    "read_clipboard",
    "读取系统剪贴板中的文本内容",
    ClipboardArgs,
    tools::execute_clipboard
);

define_tool_sync!(
    ForegroundTool,
    "force_foreground",
    "将指定窗口强制提到前台（需要窗口句柄 hwnd）",
    ForegroundArgs,
    tools::execute_foreground
);

define_tool_sync!(
    PerformDanceTool,
    "perform_dance",
    "编排并立即播放舞蹈。用户要跳舞/表演/庆祝时用。给完整 steps；动作限 jump/spin/wave/shake/idle，建议 3-8 步、每步 150-900ms。",
    PerformDanceArgs,
    tools::execute_perform_dance
);

define_tool_sync!(
    PlayDanceTool,
    "play_dance",
    "播放已保存舞蹈。即兴新舞用 perform_dance。",
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
