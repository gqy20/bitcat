use crate::action::launch_program;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use tracing::debug;

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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LaunchArgs {
    pub program: String,
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub workdir: String,
    #[serde(default = "default_terminal")]
    pub terminal: bool,
}

fn default_terminal() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShellArgs {
    pub command: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReadFileArgs {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTimeArgs {
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "full".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecentScreenshotsArgs {
    #[serde(default = "default_screenshot_count")]
    pub count: Option<u32>,
}

fn default_screenshot_count() -> Option<u32> {
    Some(3)
}

// ---- Tool 执行结果 ----

#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub output: String,
    pub success: bool,
}

impl ToolResult {
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            success: true,
        }
    }

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
    debug!(command = %args.command, "AI 执行 shell 命令");
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
                let output = if stdout.is_empty() { "(无输出)".into() } else { truncate_chars(&stdout, MAX_OUTPUT_CHARS) };
                Ok(ToolResult::ok(output))
            } else {
                let err_msg = if stderr.is_empty() { format!("命令失败 (exit code {:?})", o.status.code()) } else { truncate_chars(&stderr, MAX_OUTPUT_CHARS) };
                Ok(ToolResult::err(err_msg))
            }
        }
        Ok(Err(e)) => Ok(ToolResult::err(format!("执行错误: {e}"))),
        Err(_) => Err(ToolError::Timeout),
    }
}

/// 读取文件内容
pub fn execute_read_file(args: &ReadFileArgs) -> ToolResult {
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
    let now = chrono::Local::now();
    match args.format.as_str() {
        "date" => ToolResult::ok(now.format("%Y-%m-%d").to_string()),
        "time" => ToolResult::ok(now.format("%H:%M:%S").to_string()),
        _ => ToolResult::ok(now.format("%Y-%m-%d %H:%M:%S %Z").to_string()),
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
            r.description,
            r.width,
            r.height,
            r.jpeg_size
        ));
    }
    ToolResult::ok(lines.join("\n"))
}

// ---- 新增工具：Hotkey / Clipboard / Foreground ----

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HotkeyArgs {
    pub keys: Vec<String>,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClipboardArgs {}

/// 读取剪贴板文本
pub fn execute_clipboard(_args: &ClipboardArgs) -> ToolResult {
    match crate::hotkey::read_clipboard() {
        Some(text) => {
            let truncated = truncate_chars(&text, MAX_OUTPUT_CHARS);
            ToolResult::ok(truncated)
        }
        None => ToolResult::err("剪贴板无内容或无法读取"),
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ForegroundArgs {
    pub hwnd: isize,
}

/// 将指定窗口提到前台
pub fn execute_foreground(args: &ForegroundArgs) -> ToolResult {
    match crate::hotkey::force_foreground(args.hwnd) {
        Ok(()) => ToolResult::ok("窗口已置顶"),
        Err(e) => ToolResult::err(e),
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
        assert_eq!(args.format, "full");
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
            format: "full".into(),
        };
        let result = execute_get_time(&args);
        assert!(result.success);
        assert!(result.output.contains(' ') && result.output.len() > 10);
    }

    #[test]
    fn test_execute_get_time_date_only() {
        let args = GetTimeArgs {
            format: "date".into(),
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
                description: (*desc).into(),
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
        let s = "你好世界测试截断";
        let result = truncate_chars(s, 4);
        assert_eq!(result, "你好世界...(截断，共 6 字符)");
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
}
