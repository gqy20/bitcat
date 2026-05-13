//! AI Agent 模块：基于 rig-core 的多轮流式对话与工具调用。
//!
//! 本模块是桌宠的"大脑"，负责与 Anthropic Claude 模型进行流式对话，并注册
//! 一组内置工具（启动程序、执行命令、读取文件等），让模型可以自主调用以完成
//! 用户指令。
//!
//! 设计上采用 rig-core 的 Agent + StreamingPrompt 模式：文本 chunk 通过
//! `on_chunk` 回调实时传递给 app 层的 bubble 窗口渲染，工具调用和最终响应
//! 统计通过 `MultiTurnStreamItem` 枚举分别处理，避免阻塞流式输出。
//!
//! 工具注册使用 `define_tool_sync!` / `define_tool_async!` 两个宏消除样板代码，
//! 每个工具只需提供名称、描述、参数 schema 和执行函数即可自动实现 `Tool` trait。
//!
//! 交互关系：`bridge::handle_button_press` 触发对话 → `PetAgent::chat_stream`
//! 执行流式推理 → `on_chunk` 回调驱动 bubble 窗口更新 → 返回完整回复后由
//! `bridge::resolve_agent_response` 决定宠物状态变化。

use crate::ai_config::AiConfig;
use crate::permission_hook::PermissionHook;
use crate::prompts::PromptsConfig;
use crate::token_tracker::{
    TokenCategory, TokenRecord, TokenUsage, new_session_id, record_token_usage,
};
use crate::tools::{
    self, ClipboardArgs, ForegroundArgs, GetTimeArgs, HotkeyArgs, LaunchArgs, PerformDanceArgs,
    PlayDanceArgs, ReadFileArgs, RecentScreenshotsArgs, ShellArgs, ToolError,
};
use futures::StreamExt;
use rig::agent::Agent;
use rig::agent::MultiTurnStreamItem;
use rig::client::CompletionClient;
use rig::completion::{Prompt, ToolDefinition};
use rig::providers::anthropic;
use rig::streaming::{StreamedAssistantContent, StreamingPrompt};
use rig::tool::Tool;
use schemars::JsonSchema;
use serde_json::Value;
use tracing::{debug, info, instrument, trace};

/// AI Agent 多轮工具调用的最大回合数。
///
/// 每次"模型输出 → 工具执行 → 模型再读结果"算一个 turn。设 0 时 rig 会立刻
/// 抛 `MaxTurnError`（这也是 rig 默认值触发过的坑）。给个宽裕的上限覆盖：
/// perform_dance → 再总结 ≈ 4 turn，带搜索记忆/读文件的链路可达 10+。
/// 16 留足余量又不会让异常循环无限跑（每轮至少数秒，满轮约等于几分钟超时兜底）。
const MAX_AGENT_TURNS: usize = 16;

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

    /// 流式对话: 每收到文本块通过 on_chunk 回调发出, 返回累积的完整回复
    #[instrument(skip(self, message, on_chunk), fields(msg_chars = message.chars().count()))]
    pub async fn chat_stream<F>(&self, message: &str, mut on_chunk: F) -> Result<String, String>
    where
        F: FnMut(&str),
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
        while let Some(item) = stream.next().await {
            match item {
                Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(
                    text,
                ))) => {
                    accumulated.push_str(&text.text);
                    on_chunk(&text.text);
                    chunk_count += 1;
                    trace!(chunk_chars = text.text.chars().count(), "text chunk");
                }
                Ok(MultiTurnStreamItem::StreamAssistantItem(
                    StreamedAssistantContent::ToolCall { tool_call, .. },
                )) => {
                    tool_call_count += 1;
                    info!(tool = %tool_call.function.name, "tool call");
                    // 通知用户 AI 正在调用工具
                    on_chunk(&format!("[正在执行: {}...]", tool_call.function.name));
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
                Ok(MultiTurnStreamItem::StreamUserItem(_)) => {
                    debug!("user item (工具结果)");
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
    "直接编排并立即播放一段完整舞蹈。适合用户让桌宠跳舞、表演、扭动、庆祝、安慰或表达情绪时调用。你需要给出完整 steps，不要只给 mood。动作只允许 jump/spin/wave/shake/idle；建议 3-8 步，每步 150-900ms。",
    PerformDanceArgs,
    tools::execute_perform_dance
);

define_tool_sync!(
    PlayDanceTool,
    "play_dance",
    "立即播放一段已保存的舞蹈，桌宠会根据 YAML 中的 steps 序列表演。若用户要求你即兴编一段新舞，优先调用 perform_dance。",
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
