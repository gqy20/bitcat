//! AI Agent 内置工具定义与执行逻辑。
//!
//! 定义了桌宠 AI 可调用的全部工具：launch_program、shell、read_file、
//! get_time、recent_screenshots、hotkey、clipboard、foreground、perform_dance、
//! play_dance。每个工具由参数结构体（`XxxArgs`）+ 纯函数执行器（`execute_xxx`）组成，
//! 结果统一为 `ToolResult { output, success }`。
//!
//! 工具不内置权限分级，由 `agent.rs` 在注册时决定哪些工具暴露给 AI。
//! 执行器均为纯函数（async 用 `tokio::process`），方便独立测试。
//! 与 `agent.rs`（注册工具到 rig Agent）和 `bridge.rs`（解析工具调用）交互。

use crate::action::launch_program;
use crate::dance::{DanceDef, DanceStep};
use crate::logging::log_preview;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info};

/// 工具执行错误类型：执行失败、权限不足、超时。
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("执行失败: {0}")]
    Execution(String),
    #[error("权限不足: {0}")]
    Permission(String),
    #[error("超时")]
    Timeout,
}

/// 按字符数截断字符串（中文/emoji 安全）
pub fn truncate_chars(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max).collect();
        format!("{truncated}...(截断，共 {char_count} 字符)")
    }
}

const SHELL_TIMEOUT_SECS: u64 = 30;
const MAX_OUTPUT_CHARS: usize = 8000;

// ---- Tool 参数定义 ----

/// `launch_program` 工具参数：程序名、参数、工作目录、是否在终端中启动。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct LaunchArgs {
    /// 要启动的程序名或路径。
    pub program: String,
    /// 命令行参数。
    #[serde(default)]
    pub args: String,
    /// 工作目录。
    #[serde(default)]
    pub workdir: String,
    /// 是否在新终端窗口中打开。
    #[serde(default = "default_terminal")]
    pub terminal: bool,
}

fn default_terminal() -> bool {
    true
}

/// `shell` 工具参数：要执行的 PowerShell 命令。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ShellArgs {
    /// 要执行的 PowerShell 命令。
    pub command: String,
}

/// `read_file` 工具参数：要读取的文件绝对路径。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ReadFileArgs {
    /// 文件路径。
    pub path: String,
}

/// `get_time` 工具参数：时间格式（`full` / `date` / `time`），默认 `full`。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetTimeArgs {
    /// 输出格式。
    #[serde(default)]
    pub format: GetTimeFormat,
}

/// `get_time` 输出格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GetTimeFormat {
    Full,
    Date,
    Time,
}

impl Default for GetTimeFormat {
    fn default() -> Self {
        Self::Full
    }
}

impl GetTimeFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Date => "date",
            Self::Time => "time",
        }
    }
}

/// `recent_screenshots` 工具参数：返回最近 N 条截图分析记录，默认 3。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RecentScreenshotsArgs {
    /// 返回的记录数量，默认 3。
    #[serde(default = "default_screenshot_count")]
    pub count: Option<u32>,
}

fn default_screenshot_count() -> Option<u32> {
    Some(3)
}

// ---- Tool 执行结果 ----

/// 工具执行结果：`output` 为可读文本，`success` 标记成功或失败。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolResult {
    pub output: String,
    pub success: bool,
}

impl ToolResult {
    /// 构造成功结果。
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            success: true,
        }
    }

    /// 构造失败结果。
    pub fn err(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            success: false,
        }
    }
}

// ---- Tool 执行逻辑（纯函数，方便测试） ----

/// 启动程序
pub fn execute_launch(args: &LaunchArgs) -> ToolResult {
    let terminal_name = std::env::var("TERMINAL").unwrap_or_else(|_| "powershell".into());
    debug!(program = %args.program, args = %args.args, terminal = args.terminal, "AI 启动程序");
    match launch_program(
        &args.program,
        &args.args,
        &args.workdir,
        args.terminal,
        &terminal_name,
    ) {
        Ok(()) => ToolResult::ok(format!("已启动: {} {}", args.program, args.args)),
        Err(e) => ToolResult::err(e),
    }
}

/// 执行 shell 命令（async，带超时和输出截断）
pub async fn execute_shell(args: &ShellArgs) -> Result<ToolResult, ToolError> {
    let command_preview = log_preview(&args.command, 120);
    debug!(
        command_chars = args.command.chars().count(),
        command_preview = %command_preview,
        "AI executes shell command"
    );
    let result = tokio::time::timeout(
        Duration::from_secs(SHELL_TIMEOUT_SECS),
        tokio::process::Command::new("powershell")
            .args(["-Command", &args.command])
            .output(),
    )
    .await;

    match result {
        Ok(Ok(o)) => {
            let stdout = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            if o.status.success() {
                let output = if stdout.is_empty() {
                    "(无输出)".into()
                } else {
                    truncate_chars(&stdout, MAX_OUTPUT_CHARS)
                };
                Ok(ToolResult::ok(output))
            } else {
                let err_msg = if stderr.is_empty() {
                    format!("命令失败 (exit code {:?})", o.status.code())
                } else {
                    truncate_chars(&stderr, MAX_OUTPUT_CHARS)
                };
                Ok(ToolResult::err(err_msg))
            }
        }
        Ok(Err(e)) => Ok(ToolResult::err(format!("执行错误: {e}"))),
        Err(_) => Err(ToolError::Timeout),
    }
}

/// 读取文件内容
pub fn execute_read_file(args: &ReadFileArgs) -> ToolResult {
    info!(path = %args.path, "tool: read_file");
    match std::fs::read_to_string(&args.path) {
        Ok(content) => {
            // 截断过长的文件，避免 token 浪费
            if content.chars().count() > MAX_OUTPUT_CHARS {
                ToolResult::ok(truncate_chars(&content, MAX_OUTPUT_CHARS))
            } else {
                ToolResult::ok(content)
            }
        }
        Err(e) => ToolResult::err(format!("读取文件失败: {e}")),
    }
}

/// 获取当前时间
pub fn execute_get_time(args: &GetTimeArgs) -> ToolResult {
    info!(format = %args.format.as_str(), "tool: get_time");
    let now = chrono::Local::now();
    match args.format {
        GetTimeFormat::Date => ToolResult::ok(now.format("%Y-%m-%d").to_string()),
        GetTimeFormat::Time => ToolResult::ok(now.format("%H:%M:%S").to_string()),
        GetTimeFormat::Full => ToolResult::ok(now.format("%Y-%m-%d %H:%M:%S %Z").to_string()),
    }
}

/// 查询最近的截图分析记录。
///
/// `override_dir` 仅用于测试，生产环境传 None 则读取真实截图目录。
pub fn execute_recent_screenshots(
    args: &RecentScreenshotsArgs,
    override_dir: Option<&std::path::Path>,
) -> ToolResult {
    use crate::screenshot::{ensure_today_dir, list_recent_analyses};

    let count = args.count.unwrap_or(3);
    info!(count, "tool: recent_screenshots");

    let dir = match override_dir {
        Some(d) => d.to_path_buf(),
        None => match ensure_today_dir() {
            Ok(d) => d,
            Err(e) => return ToolResult::err(format!("获取截图目录失败: {e}")),
        },
    };

    let records = list_recent_analyses(&dir, count);
    if records.is_empty() {
        return ToolResult::ok("暂无截图分析记录");
    }

    let mut lines = Vec::with_capacity(records.len() + 1);
    lines.push(format!("最近 {} 条截图分析:", records.len()));
    for (i, r) in records.iter().enumerate() {
        lines.push(format!(
            "  {}. {} ({}x{}, {} bytes)",
            i + 1,
            r.description(),
            r.width,
            r.height,
            r.jpeg_size
        ));
    }
    ToolResult::ok(lines.join("\n"))
}

// ---- 新增工具：Hotkey / Clipboard / Foreground ----

/// `hotkey` 工具参数：要模拟的按键序列和可选的按住时长（秒）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct HotkeyArgs {
    /// 按键列表，如 ["ctrl", "alt", "tab"]。
    pub keys: Vec<String>,
    /// 按键保持时间（秒），默认 0。
    #[serde(default)]
    pub hold: f64,
}

/// 模拟键盘快捷键
pub fn execute_hotkey(args: &HotkeyArgs) -> ToolResult {
    debug!(keys = ?args.keys, hold = args.hold, "AI 模拟快捷键");
    let key_refs: Vec<&str> = args.keys.iter().map(|s| s.as_str()).collect();
    match crate::hotkey::trigger_hotkey(&key_refs, args.hold) {
        Ok(()) => ToolResult::ok(format!("已发送快捷键: {}", args.keys.join("+"))),
        Err(e) => ToolResult::err(e),
    }
}

/// `clipboard` 工具参数（当前无额外字段）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ClipboardArgs {}

/// 读取剪贴板文本
pub fn execute_clipboard(_args: &ClipboardArgs) -> ToolResult {
    info!("tool: read_clipboard");
    match crate::hotkey::read_clipboard() {
        Some(text) => {
            let truncated = truncate_chars(&text, MAX_OUTPUT_CHARS);
            ToolResult::ok(truncated)
        }
        None => ToolResult::err("剪贴板无内容或无法读取"),
    }
}

/// `foreground` 工具参数：目标窗口句柄（HWND）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ForegroundArgs {
    /// 目标窗口的句柄（整数）。
    pub hwnd: isize,
}

/// 将指定窗口提到前台
pub fn execute_foreground(args: &ForegroundArgs) -> ToolResult {
    info!(hwnd = args.hwnd, "tool: force_foreground");
    match crate::hotkey::force_foreground(args.hwnd) {
        Ok(()) => ToolResult::ok("窗口已置顶"),
        Err(e) => ToolResult::err(e),
    }
}

// ---- 舞蹈工具 ----

/// 编排并播放舞蹈。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PerformDanceArgs {
    /// 文件名：英文/数字/_/-。
    pub name: String,
    /// 保存时循环。
    #[serde(default = "default_dance_loop")]
    pub loop_: bool,
    /// 步骤，建议 3-8 步。
    pub steps: Vec<DanceStep>,
    /// 轮数：空/1 单次，0 无限，>=2 固定轮。
    #[serde(default)]
    pub loops: Option<u32>,
    /// 播放上限毫秒。
    #[serde(default)]
    pub duration_ms: Option<u32>,
}

fn default_dance_loop() -> bool {
    true
}

/// AI 直接编排并立即播放完整舞蹈定义。
pub fn execute_perform_dance(args: &PerformDanceArgs) -> ToolResult {
    debug!(
        name = %args.name,
        steps = args.steps.len(),
        loops = ?args.loops,
        duration_ms = ?args.duration_ms,
        "AI 编排并播放舞蹈"
    );

    let def = DanceDef {
        name: args.name.clone(),
        loop_: args.loop_,
        steps: args.steps.clone(),
    };

    if let Err(e) = crate::dance::validate_dance_def(&def) {
        return ToolResult::err(format!("舞蹈定义无效: {e}"));
    }

    let path = match crate::dance::save_dance(&def) {
        Ok(path) => path,
        Err(e) => return ToolResult::err(format!("保存舞蹈失败: {e}")),
    };

    let req = crate::dance::PlayDanceRequest {
        name: def.name.clone(),
        loops: args.loops,
        duration_ms: args.duration_ms,
    };

    match crate::dance::request_play_dance(req) {
        Ok(()) => ToolResult::ok(format!(
            "已编排并播放舞蹈「{}」({} 步, {}ms)，保存在 {}",
            def.name,
            def.steps.len(),
            def.total_duration_ms(),
            path.display()
        )),
        Err(e) => ToolResult::err(format!(
            "舞蹈已保存但触发播放失败: {e}。文件位置: {}",
            path.display()
        )),
    }
}

// ---- play_dance 工具 ----

/// 播放已保存舞蹈。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PlayDanceArgs {
    /// 舞蹈名。
    pub name: String,
    /// 轮数：空/1 单次，0 无限，>=2 固定轮。
    #[serde(default)]
    pub loops: Option<u32>,
    /// 播放上限毫秒。
    #[serde(default)]
    pub duration_ms: Option<u32>,
}

/// AI 播放已保存的舞蹈：校验舞蹈存在 → 通过事件通道通知 app 层 emit 到前端
pub fn execute_play_dance(args: &PlayDanceArgs) -> ToolResult {
    debug!(
        name = %args.name,
        loops = ?args.loops,
        duration_ms = ?args.duration_ms,
        "AI 播放舞蹈"
    );

    // 先确认舞蹈文件存在且能反序列化，避免发了事件前端却拿不到定义
    if let Err(e) = crate::dance::load_dance(&args.name) {
        return ToolResult::err(format!("舞蹈「{}」不存在或无法加载: {}", args.name, e));
    }

    let req = crate::dance::PlayDanceRequest {
        name: args.name.clone(),
        loops: args.loops,
        duration_ms: args.duration_ms,
    };

    match crate::dance::request_play_dance(req) {
        Ok(()) => {
            let hint = match (args.loops, args.duration_ms) {
                (Some(0), _) => "无限循环".to_string(),
                (Some(n), _) if n >= 2 => format!("{n} 轮"),
                (_, Some(ms)) => format!("{ms}ms 上限"),
                _ => "单次".to_string(),
            };
            ToolResult::ok(format!("已触发播放舞蹈「{}」（{}）", args.name, hint))
        }
        Err(e) => ToolResult::err(format!("触发播放失败: {e}")),
    }
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launch_args_deserialize() {
        let json = r#"{"program":"notepad","args":"test.txt","terminal":true}"#;
        let args: LaunchArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.program, "notepad");
        assert_eq!(args.args, "test.txt");
        assert!(args.terminal);
    }

    #[test]
    fn test_launch_args_defaults() {
        let json = r#"{"program":"calc"}"#;
        let args: LaunchArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.program, "calc");
        assert!(args.args.is_empty());
        assert!(args.workdir.is_empty());
        assert!(args.terminal); // default is true
    }

    #[test]
    fn test_shell_args_deserialize() {
        let json = r#"{"command":"echo hello"}"#;
        let args: ShellArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.command, "echo hello");
    }

    #[test]
    fn test_read_file_args_deserialize() {
        let json = r#"{"path":"C:\\test\\file.txt"}"#;
        let args: ReadFileArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.path, r"C:\test\file.txt");
    }

    #[test]
    fn test_get_time_args_default_format() {
        let json = r#"{}"#;
        let args: GetTimeArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.format, GetTimeFormat::Full);
    }

    #[tokio::test]
    async fn test_execute_shell_echo() {
        let args = ShellArgs {
            command: "echo 'hello_world'".into(),
        };
        let result = execute_shell(&args).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("hello_world") || !result.output.is_empty());
    }

    #[tokio::test]
    async fn test_execute_shell_invalid_command() {
        let args = ShellArgs {
            command: "nonexistent_command_xyz_12345".into(),
        };
        let result = execute_shell(&args).await.unwrap();
        assert!(!result.success);
    }

    #[test]
    fn test_execute_read_file_existing() {
        let args = ReadFileArgs {
            path: "Cargo.toml".into(),
        };
        let result = execute_read_file(&args);
        assert!(result.success);
        assert!(result.output.contains("[package]"));
    }

    #[test]
    fn test_execute_read_file_not_found() {
        let args = ReadFileArgs {
            path: "/nonexistent/path/file.xyz".into(),
        };
        let result = execute_read_file(&args);
        assert!(!result.success);
    }

    #[test]
    fn test_execute_get_time_full() {
        let args = GetTimeArgs {
            format: GetTimeFormat::Full,
        };
        let result = execute_get_time(&args);
        assert!(result.success);
        assert!(result.output.contains(' ') && result.output.len() > 10);
    }

    #[test]
    fn test_execute_get_time_date_only() {
        let args = GetTimeArgs {
            format: GetTimeFormat::Date,
        };
        let result = execute_get_time(&args);
        assert!(result.success);
        assert!(!result.output.contains(' ')); // date only, no time part with space
    }

    #[test]
    fn test_tool_result_ok_and_err() {
        let ok = ToolResult::ok("good");
        assert!(ok.success);
        assert_eq!(ok.output, "good");

        let err = ToolResult::err("bad");
        assert!(!err.success);
        assert_eq!(err.output, "bad");
    }

    #[test]
    fn test_execute_read_file_truncation() {
        // 创建一个临时大文件测试截断
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("large.txt");
        let long_content = "x".repeat(10000);
        std::fs::write(&file_path, &long_content).unwrap();

        let args = ReadFileArgs {
            path: file_path.to_string_lossy().to_string(),
        };
        let result = execute_read_file(&args);
        assert!(result.success);
        assert!(result.output.ends_with("字符)"));
        assert!(result.output.len() < 10000); // 应该被截断
    }

    // ---- recent_screenshots TDD 测试 ----

    #[test]
    fn test_recent_screenshots_args_default_count() {
        let json = r#"{}"#;
        let args: RecentScreenshotsArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.count, Some(3)); // default
    }

    #[test]
    fn test_recent_screenshots_args_custom_count() {
        let json = r#"{"count": 5}"#;
        let args: RecentScreenshotsArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.count, Some(5));
    }

    #[test]
    fn test_execute_recent_screenshots_with_mock_data() {
        let dir = tempfile::tempdir().unwrap();
        let today = dir.path().join("2026-05-11");
        std::fs::create_dir_all(&today).unwrap();

        // 写入 2 条模拟截图分析
        for (i, desc) in ["用户在用浏览器", "用户在写代码"].iter().enumerate() {
            use crate::screenshot::{ScreenshotRecord, save_analysis_json};
            let prefix = format!("{:06}", 100000 + i * 10);
            let record = ScreenshotRecord {
                analysis: crate::vision::VisionAnalysis {
                    description: (*desc).into(),
                    ..Default::default()
                },
                hash: i as u64,
                skipped: false,
                width: 1280,
                height: 800,
                jpeg_size: 5000,
            };
            save_analysis_json(&today, &prefix, "", &record).unwrap();
        }

        // 用临时目录替换 screenshot_base_dir 的行为
        // 这里测试的是格式化输出，不依赖真实 HOME 目录
        let args = RecentScreenshotsArgs { count: Some(5) };
        let result = execute_recent_screenshots(&args, Some(today.as_path()));
        assert!(result.success);
        assert!(result.output.contains("浏览器"));
        assert!(result.output.contains("代码"));
        // 最新的在前
        let code_pos = result.output.find("代码").unwrap();
        let browser_pos = result.output.find("浏览器").unwrap();
        assert!(code_pos < browser_pos, "最新记录应在前面");
    }

    #[test]
    fn test_execute_recent_screenshots_empty_returns_message() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();

        let args = RecentScreenshotsArgs { count: Some(3) };
        let result = execute_recent_screenshots(&args, Some(empty.as_path()));
        assert!(result.success);
        assert!(result.output.contains("暂无"));
    }

    // ---- truncate_chars 测试 ----

    #[test]
    fn test_truncate_chars_short_string_unchanged() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_chars_exact_length_unchanged() {
        assert_eq!(truncate_chars("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_chars_long_string_truncated() {
        let result = truncate_chars("x".repeat(100).as_str(), 10);
        assert!(result.contains("截断"));
        assert!(result.contains("100 字符"));
    }

    #[test]
    fn test_truncate_chars_chinese_safe() {
        // 中文字符每个占 3 字节，但 chars().count() 正确计数
        // 输入共 8 个字符，截断到 4 个字符 + 后缀提示原始长度
        let s = "你好世界测试截断";
        let result = truncate_chars(s, 4);
        assert_eq!(result, "你好世界...(截断，共 8 字符)");
    }

    // ---- Hotkey / Clipboard / Foreground 参数测试 ----

    #[test]
    fn test_hotkey_args_deserialize() {
        let json = r#"{"keys":["ctrl","alt","delete"]}"#;
        let args: HotkeyArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.keys, vec!["ctrl", "alt", "delete"]);
        assert!((args.hold - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_hotkey_args_with_hold() {
        let json = r#"{"keys":["alt","tab"],"hold":0.05}"#;
        let args: HotkeyArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.keys.len(), 2);
        assert!((args.hold - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn test_clipboard_args_deserialize_empty() {
        let json = r#"{}"#;
        let _args: ClipboardArgs = serde_json::from_str(json).unwrap();
    }

    #[test]
    fn test_foreground_args_deserialize() {
        let json = r#"{"hwnd":12345}"#;
        let args: ForegroundArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.hwnd, 12345);
    }

    #[test]
    fn test_tool_error_display() {
        assert_eq!(
            ToolError::Execution("fail".to_string()).to_string(),
            "执行失败: fail"
        );
        assert_eq!(
            ToolError::Permission("denied".to_string()).to_string(),
            "权限不足: denied"
        );
        assert_eq!(ToolError::Timeout.to_string(), "超时");
    }

    // ---- perform_dance 工具测试 ----

    #[test]
    fn perform_dance_args_deserialize() {
        let json = r#"{
            "name":"ai_bounce",
            "steps":[
                {"action":"jump","duration_ms":300},
                {"action":"spin","duration_ms":450,"repeat":2}
            ],
            "loops":1
        }"#;
        let args: PerformDanceArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.name, "ai_bounce");
        assert!(args.loop_);
        assert_eq!(args.steps.len(), 2);
        assert_eq!(args.steps[0].duration_ms, 300);
        assert_eq!(args.steps[1].repeat, 2);
        assert_eq!(args.loops, Some(1));
    }

    #[test]
    fn execute_perform_dance_rejects_invalid_def() {
        let args = PerformDanceArgs {
            name: "../bad".into(),
            loop_: true,
            steps: vec![],
            loops: None,
            duration_ms: None,
        };
        let result = execute_perform_dance(&args);
        assert!(!result.success);
        assert!(result.output.contains("无效"));
    }

    // ---- play_dance 工具测试 ----

    #[test]
    fn play_dance_args_deserialize() {
        let json = r#"{"name":"happy_twist"}"#;
        let args: PlayDanceArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.name, "happy_twist");
        assert!(args.loops.is_none());
        assert!(args.duration_ms.is_none());
    }

    #[test]
    fn play_dance_args_with_loops_and_duration() {
        let json = r#"{"name":"spin","loops":3,"duration_ms":5000}"#;
        let args: PlayDanceArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.loops, Some(3));
        assert_eq!(args.duration_ms, Some(5000));
    }

    #[test]
    fn execute_play_dance_missing_file_returns_err() {
        let args = PlayDanceArgs {
            name: "definitely_not_exist_dance_xyz_987".into(),
            loops: None,
            duration_ms: None,
        };
        let result = execute_play_dance(&args);
        assert!(!result.success);
        assert!(result.output.contains("不存在"));
    }
}
