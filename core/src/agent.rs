use crate::ai_config::AiConfig;
use crate::tools::{self, LaunchArgs, ReadFileArgs, ShellArgs};
use futures::StreamExt;
use rig::agent::Agent;
use rig::agent::MultiTurnStreamItem;
use rig::client::CompletionClient;
use rig::completion::{Prompt, ToolDefinition};
use rig::providers::anthropic;
use rig::streaming::{StreamedAssistantContent, StreamingPrompt};
use rig::tool::Tool;
use serde_json::json;

/// 桌宠 AI Agent
pub struct PetAgent {
    pub agent: Agent<anthropic::completion::CompletionModel>,
    pub config: AiConfig,
}

const PREAMBLE: &str = r#"你是 8Bit，一只住在电脑屏幕上的像素风小猫助手。

性格特点：
- 活泼好奇，喜欢用 emoji
- 偶尔调皮，但做事靠谱
- 回答简洁，不说废话
- 用中文交流

你通过手柄和用户交互，可以帮用户：
- 启动程序、执行命令
- 查时间、读文件
- 闲聊、讲笑话、提醒事项

回答时保持角色感，像一只懂技术的猫。"#;

impl PetAgent {
    pub fn new() -> Result<Self, String> {
        let config = AiConfig::load()?;

        let client = anthropic::Client::builder()
            .api_key(&config.api_key)
            .base_url(&config.base_url)
            .build()
            .map_err(|e| format!("创建 Anthropic Client 失败: {e}"))?;

        let model = client.completion_model(config.model.as_str());

        let max_tokens = config.max_tokens();

        let agent = rig::agent::AgentBuilder::new(model)
            .preamble(PREAMBLE)
            .max_tokens(max_tokens)
            .tool(LaunchTool)
            .tool(ShellTool)
            .tool(ReadFileTool)
            .tool(GetTimeTool)
            .build();

        Ok(Self { agent, config })
    }

    pub async fn chat(&self, message: &str) -> Result<String, String> {
        self.agent
            .prompt(message)
            .await
            .map_err(|e| format!("AI 对话失败: {e}"))
    }

    /// 流式对话: 每收到文本块通过 on_chunk 回调发出, 返回累积的完整回复
    pub async fn chat_stream<F>(&self, message: &str, mut on_chunk: F) -> Result<String, String>
    where
        F: FnMut(&str),
    {
        let mut stream = self.agent.stream_prompt(message.to_string()).await;

        let mut accumulated = String::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(
                    text,
                ))) => {
                    accumulated.push_str(&text.text);
                    on_chunk(&text.text);
                }
                Ok(MultiTurnStreamItem::FinalResponse(_)) => {}
                Ok(_) => {}
                Err(e) => return Err(format!("AI 流错误: {e}")),
            }
        }
        Ok(accumulated)
    }
}

// ---- Tool: 启动程序 ----

struct LaunchTool;
impl Tool for LaunchTool {
    const NAME: &'static str = "launch_program";
    type Error = std::convert::Infallible;
    type Args = LaunchArgs;
    type Output = tools::ToolResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "launch_program".into(),
            description: "启动一个程序或应用".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "program": { "type": "string", "description": "要启动的程序名或路径" },
                    "args": { "type": "string", "description": "命令行参数" },
                    "terminal": { "type": "boolean", "description": "是否在新终端窗口中打开" },
                    "workdir": { "type": "string", "description": "工作目录" }
                },
                "required": ["program"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(tools::execute_launch(&args))
    }
}

// ---- Tool: 执行 Shell 命令 ----

struct ShellTool;
impl Tool for ShellTool {
    const NAME: &'static str = "shell";
    type Error = std::convert::Infallible;
    type Args = ShellArgs;
    type Output = tools::ToolResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "shell".into(),
            description: "执行 PowerShell 命令并返回输出".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "要执行的 PowerShell 命令" }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(tools::execute_shell(&args))
    }
}

// ---- Tool: 读取文件 ----

struct ReadFileTool;
impl Tool for ReadFileTool {
    const NAME: &'static str = "read_file";
    type Error = std::convert::Infallible;
    type Args = ReadFileArgs;
    type Output = tools::ToolResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".into(),
            description: "读取文件内容，支持文本文件".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "文件路径" }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(tools::execute_read_file(&args))
    }
}

// ---- Tool: 获取时间 ----

struct GetTimeTool;
use crate::tools::GetTimeArgs;

impl Tool for GetTimeTool {
    const NAME: &'static str = "get_time";
    type Error = std::convert::Infallible;
    type Args = GetTimeArgs;
    type Output = tools::ToolResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "get_time".into(),
            description: "获取当前日期和时间".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "format": { "type": "string", "enum": ["full", "date", "time"], "description": "输出格式" }
                }
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(tools::execute_get_time(&args))
    }
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preamble_is_non_empty() {
        assert!(!PREAMBLE.is_empty());
        assert!(PREAMBLE.contains("8Bit"));
        assert!(PREAMBLE.contains("猫"));
    }

    #[test]
    fn test_launch_tool_definition() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tool = LaunchTool;
        let def = rt.block_on(tool.definition(String::new()));
        assert_eq!(def.name, "launch_program");
        assert!(!def.description.is_empty());
        let params = def.parameters.as_object().unwrap();
        assert!(params.get("properties").is_some());
        assert!(params.get("required").is_some());
    }

    #[test]
    fn test_shell_tool_definition() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tool = ShellTool;
        let def = rt.block_on(tool.definition(String::new()));
        assert_eq!(def.name, "shell");
        assert!(def.description.contains("PowerShell"));
    }

    #[test]
    fn test_read_file_tool_definition() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tool = ReadFileTool;
        let def = rt.block_on(tool.definition(String::new()));
        assert_eq!(def.name, "read_file");
    }

    #[test]
    fn test_get_time_tool_definition() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tool = GetTimeTool;
        let def = rt.block_on(tool.definition(String::new()));
        assert_eq!(def.name, "get_time");
        let params = def.parameters.as_object().unwrap();
        let props = params.get("properties").unwrap().as_object().unwrap();
        let fmt = props.get("format").unwrap().as_object().unwrap();
        let enum_vals = fmt.get("enum").unwrap().as_array().unwrap();
        assert_eq!(enum_vals.len(), 3);
    }

    #[test]
    fn test_tool_call_launch() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tool = LaunchTool;
        let args = LaunchArgs { program: "echo".into(), args: "test".into(), workdir: String::new(), terminal: false };
        let result = rt.block_on(tool.call(args)).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_tool_call_shell() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tool = ShellTool;
        let args = ShellArgs { command: "echo hello_test".into() };
        let result = rt.block_on(tool.call(args)).unwrap();
        assert!(result.success);
        assert!(result.output.contains("hello_test"));
    }

    #[test]
    fn test_tool_call_read_file() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tool = ReadFileTool;
        let args = ReadFileArgs { path: "Cargo.toml".into() };
        let result = rt.block_on(tool.call(args)).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_tool_call_get_time() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tool = GetTimeTool;
        let args = GetTimeArgs { format: "date".into() };
        let result = rt.block_on(tool.call(args)).unwrap();
        assert!(result.success);
        assert!(!result.output.is_empty());
    }
}
