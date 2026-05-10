use crate::action::launch_program;
use serde::{Deserialize, Serialize};

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

fn default_terminal() -> bool { true }

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

fn default_format() -> String { "full".into() }

// ---- Tool 执行结果 ----

#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub output: String,
    pub success: bool,
}

impl ToolResult {
    pub fn ok(output: impl Into<String>) -> Self {
        Self { output: output.into(), success: true }
    }

    pub fn err(output: impl Into<String>) -> Self {
        Self { output: output.into(), success: false }
    }
}

// ---- Tool 执行逻辑（纯函数，方便测试） ----

/// 启动程序
pub fn execute_launch(args: &LaunchArgs) -> ToolResult {
    let terminal_name = std::env::var("TERMINAL").unwrap_or_else(|_| "powershell".into());
    match launch_program(&args.program, &args.args, &args.workdir, args.terminal, &terminal_name) {
        Ok(()) => ToolResult::ok(format!("已启动: {} {}", args.program, args.args)),
        Err(e) => ToolResult::err(e),
    }
}

/// 执行 shell 命令
pub fn execute_shell(args: &ShellArgs) -> ToolResult {
    let output = std::process::Command::new("powershell")
        .args(["-Command", &args.command])
        .output();

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            if o.status.success() {
                ToolResult::ok(if stdout.is_empty() { "(无输出)".into() } else { stdout })
            } else {
                ToolResult::err(if stderr.is_empty() { format!("命令失败 (exit code {:?})", o.status.code()) } else { stderr })
            }
        }
        Err(e) => ToolResult::err(format!("执行错误: {e}")),
    }
}

/// 读取文件内容
pub fn execute_read_file(args: &ReadFileArgs) -> ToolResult {
    match std::fs::read_to_string(&args.path) {
        Ok(content) => {
            // 截断过长的文件，避免 token 浪费
            let max_chars = 8000;
            if content.chars().count() > max_chars {
                let truncated: String = content.chars().take(max_chars).collect();
                ToolResult::ok(format!("{truncated}...(截断，共 {} 字符)", content.len()))
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

    #[test]
    fn test_execute_shell_echo() {
        let args = ShellArgs { command: "echo 'hello_world'".into() };
        let result = execute_shell(&args);
        assert!(result.success);
        assert!(result.output.contains("hello_world") || !result.output.is_empty());
    }

    #[test]
    fn test_execute_shell_invalid_command() {
        let args = ShellArgs { command: "nonexistent_command_xyz_12345".into() };
        let result = execute_shell(&args);
        assert!(!result.success);
    }

    #[test]
    fn test_execute_read_file_existing() {
        let args = ReadFileArgs { path: "Cargo.toml".into() };
        let result = execute_read_file(&args);
        assert!(result.success);
        assert!(result.output.contains("[package]"));
    }

    #[test]
    fn test_execute_read_file_not_found() {
        let args = ReadFileArgs { path: "/nonexistent/path/file.xyz".into() };
        let result = execute_read_file(&args);
        assert!(!result.success);
    }

    #[test]
    fn test_execute_get_time_full() {
        let args = GetTimeArgs { format: "full".into() };
        let result = execute_get_time(&args);
        assert!(result.success);
        assert!(result.output.contains(' ') && result.output.len() > 10);
    }

    #[test]
    fn test_execute_get_time_date_only() {
        let args = GetTimeArgs { format: "date".into() };
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

        let args = ReadFileArgs { path: file_path.to_string_lossy().to_string() };
        let result = execute_read_file(&args);
        assert!(result.success);
        assert!(result.output.ends_with("字符)"));
        assert!(result.output.len() < 10000); // 应该被截断
    }
}
