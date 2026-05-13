use crate::ai_config::AiConfig;
use crate::permission_hook::PermissionHook;
use crate::prompts::PromptsConfig;
use crate::tools::{
    self, ClipboardArgs, CreateDanceArgs, ForegroundArgs, GetTimeArgs, HotkeyArgs, LaunchArgs,
    PerformDanceArgs, PlayDanceArgs, ReadFileArgs, RecentScreenshotsArgs, ShellArgs, ToolError,
};
use futures::StreamExt;
use rig::agent::Agent;
use rig::agent::MultiTurnStreamItem;
use rig::client::CompletionClient;
use rig::completion::{Prompt, ToolDefinition};
use rig::providers::anthropic;
use rig::streaming::{StreamedAssistantContent, StreamingPrompt};
use rig::tool::Tool;
use serde_json::json;
use tracing::{debug, info, instrument};

/// AI Agent 多轮工具调用的最大回合数。
///
/// 每次"模型输出 → 工具执行 → 模型再读结果"算一个 turn。设 0 时 rig 会立刻
/// 抛 `MaxTurnError`（这也是 rig 默认值触发过的坑）。给个宽裕的上限覆盖：
/// create_dance → play_dance → 再总结 ≈ 6 turn，带搜索记忆/读文件的链路可达 10+。
/// 16 留足余量又不会让异常循环无限跑（每轮至少数秒，满轮约等于几分钟超时兜底）。
const MAX_AGENT_TURNS: usize = 16;

/// 桌宠 AI Agent
pub struct PetAgent {
    pub agent: Agent<anthropic::completion::CompletionModel, PermissionHook>,
    pub config: AiConfig,
}

impl PetAgent {
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
            .tool(CreateDanceTool)
            .tool(PlayDanceTool)
            .build();

        Ok(Self { agent, config })
    }

    pub async fn chat(&self, message: &str) -> Result<String, String> {
        self.agent
            .prompt(message)
            .max_turns(MAX_AGENT_TURNS)
            .await
            .map_err(|e| format!("AI 对话失败: {e}"))
    }

    /// 流式对话: 每收到文本块通过 on_chunk 回调发出, 返回累积的完整回复
    #[instrument(skip(self, on_chunk), fields(msg_len = message.chars().count()))]
    pub async fn chat_stream<F>(&self, message: &str, mut on_chunk: F) -> Result<String, String>
    where
        F: FnMut(&str),
    {
        let mut stream = self
            .agent
            .stream_prompt(message.to_string())
            .multi_turn(MAX_AGENT_TURNS)
            .await;

        let mut accumulated = String::new();
        let mut chunk_count = 0u32;
        let mut tool_call_count = 0u32;
        while let Some(item) = stream.next().await {
            match item {
                Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(
                    text,
                ))) => {
                    accumulated.push_str(&text.text);
                    on_chunk(&text.text);
                    chunk_count += 1;
                    debug!(len = text.text.len(), "text chunk");
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
                    info!(
                        chars = res.response().len(),
                        tokens = res.usage().total_tokens,
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

/// 定义一个同步执行的 Tool（execute 函数返回 `ToolResult`）
macro_rules! define_tool_sync {
    ($name:ident, $tool_name:literal, $desc:literal, $args_ty:ty, $params:expr, $exec_fn:expr) => {
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
                    parameters: $params.clone(),
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
    ($name:ident, $tool_name:literal, $desc:literal, $args_ty:ty, $params:expr, $exec_fn:expr) => {
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
                    parameters: $params.clone(),
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
    json!({
        "type": "object",
        "properties": {
            "program": { "type": "string", "description": "要启动的程序名或路径" },
            "args": { "type": "string", "description": "命令行参数" },
            "terminal": { "type": "boolean", "description": "是否在新终端窗口中打开" },
            "workdir": { "type": "string", "description": "工作目录" }
        },
        "required": ["program"]
    }),
    tools::execute_launch
);

define_tool_async!(
    ShellTool,
    "shell",
    "执行 PowerShell 命令并返回输出（30s 超时，输出截断至 8000 字符）",
    ShellArgs,
    json!({
        "type": "object",
        "properties": {
            "command": { "type": "string", "description": "要执行的 PowerShell 命令" }
        },
        "required": ["command"]
    }),
    tools::execute_shell
);

define_tool_sync!(
    ReadFileTool,
    "read_file",
    "读取文件内容，支持文本文件（超过 8000 字符自动截断）",
    ReadFileArgs,
    json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "文件路径" }
        },
        "required": ["path"]
    }),
    tools::execute_read_file
);

define_tool_sync!(
    GetTimeTool,
    "get_time",
    "获取当前日期和时间",
    GetTimeArgs,
    json!({
        "type": "object",
        "properties": {
            "format": { "type": "string", "enum": ["full", "date", "time"], "description": "输出格式" }
        }
    }),
    tools::execute_get_time
);

define_tool_sync!(
    RecentScreenshotsTool,
    "recent_screenshots",
    "查询最近的截图视觉分析记录，了解用户最近在屏幕上做什么",
    RecentScreenshotsArgs,
    json!({
        "type": "object",
        "properties": {
            "count": { "type": "integer", "description": "返回的记录数量，默认 3" }
        }
    }),
    |args| tools::execute_recent_screenshots(args, None)
);

define_tool_sync!(
    HotkeyTool,
    "send_hotkey",
    "模拟键盘快捷键组合（如 Alt+Tab 切窗口、Ctrl+C 复制）",
    HotkeyArgs,
    json!({
        "type": "object",
        "properties": {
            "keys": { "type": "array", "items": { "type": "string" }, "description": "按键列表，如 [\"ctrl\", \"alt\", \"tab\"]" },
            "hold": { "type": "number", "description": "按键保持时间（秒），默认 0.02" }
        },
        "required": ["keys"]
    }),
    tools::execute_hotkey
);

define_tool_sync!(
    ClipboardTool,
    "read_clipboard",
    "读取系统剪贴板中的文本内容",
    ClipboardArgs,
    json!({
        "type": "object",
        "properties": {},
        "description": "无参数，直接读取剪贴板"
    }),
    tools::execute_clipboard
);

define_tool_sync!(
    ForegroundTool,
    "force_foreground",
    "将指定窗口强制提到前台（需要窗口句柄 hwnd）",
    ForegroundArgs,
    json!({
        "type": "object",
        "properties": {
            "hwnd": { "type": "number", "description": "目标窗口的句柄（整数）" }
        },
        "required": ["hwnd"]
    }),
    tools::execute_foreground
);

define_tool_sync!(
    PerformDanceTool,
    "perform_dance",
    "直接编排并立即播放一段完整舞蹈。适合用户让桌宠跳舞、表演、扭动、庆祝、安慰或表达情绪时调用。你需要给出完整 steps，不要只给 mood。动作只允许 jump/spin/wave/shake/idle；建议 3-8 步，每步 150-900ms。",
    PerformDanceArgs,
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "description": "舞蹈名称/文件名，只能使用英文、数字、下划线或短横线，例如 happy_twist" },
            "loop_": { "type": "boolean", "description": "保存到 YAML 的默认循环设置，通常为 true" },
            "steps": {
                "type": "array",
                "description": "完整舞蹈步骤，按时间顺序执行",
                "items": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["jump", "spin", "wave", "shake", "idle"], "description": "舞蹈动作" },
                        "duration_ms": { "type": "integer", "description": "该动作持续毫秒数，建议 150-900，允许 80-5000" },
                        "repeat": { "type": "integer", "description": "重复次数，通常省略或设为 1，允许 1-8" }
                    },
                    "required": ["action", "duration_ms"]
                },
                "minItems": 1,
                "maxItems": 24
            },
            "loops": { "type": "integer", "description": "本次播放轮数：不填或 1=单次，0=无限循环，>=2=固定轮数" },
            "duration_ms": { "type": "integer", "description": "本次播放硬上限毫秒数，到时强制停止" }
        },
        "required": ["name", "steps"]
    }),
    tools::execute_perform_dance
);

define_tool_sync!(
    CreateDanceTool,
    "create_dance",
    "旧版兜底工具：根据心情(mood)用内置模板创建舞蹈 YAML。优先使用 perform_dance 直接编排完整 steps；仅当你只需要按 mood 快速生成模板时使用本工具。",
    CreateDanceArgs,
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "description": "舞蹈名称（用于文件名，如 happy_twist）" },
            "mood": { "type": "string", "description": "心情关键词：happy/excited/sleepy/angry/cute/开心/兴奋/困/生气/萌" },
            "duration_ms": { "type": "integer", "description": "可选，总时长限制（毫秒），不填则使用默认编排" }
        },
        "required": ["name", "mood"]
    }),
    tools::execute_create_dance
);

define_tool_sync!(
    PlayDanceTool,
    "play_dance",
    "立即播放一段已保存的舞蹈，桌宠会根据 YAML 中的 steps 序列表演。若用户要求你即兴编一段新舞，优先调用 perform_dance。",
    PlayDanceArgs,
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "description": "舞蹈名称，必须已存在于 ~/.ai-pad/dances/" },
            "loops": { "type": "integer", "description": "播放轮数：不填或 1=单次，0=无限循环，>=2=固定轮数" },
            "duration_ms": { "type": "integer", "description": "硬上限毫秒数，到时强制停止（优先级高于 loops）" }
        },
        "required": ["name"]
    }),
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
        let params = def.parameters.as_object().unwrap();
        let props = params.get("properties").unwrap().as_object().unwrap();
        let fmt = props.get("format").unwrap().as_object().unwrap();
        let enum_vals = fmt.get("enum").unwrap().as_array().unwrap();
        assert_eq!(enum_vals.len(), 3);
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
    async fn test_create_dance_tool_definition() {
        let def = CreateDanceTool.definition(String::new()).await;
        assert_eq!(def.name, "create_dance");
        assert!(def.description.contains("舞蹈"));
        let params = def.parameters.as_object().unwrap();
        let props = params.get("properties").unwrap().as_object().unwrap();
        assert!(props.get("name").is_some());
        assert!(props.get("mood").is_some());
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
        let args = LaunchArgs {
            program: "echo".into(),
            args: "test".into(),
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
            format: "date".into(),
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
