//! Claude Code hook 安装器。
//!
//! 本模块只写入 ai-pad 自己的 PowerShell hook 脚本，并在 `~/.claude/settings.json`
//! 中合并带有 ai-pad 标记的 hook 配置。写入前会备份 settings，失败时不覆盖，
//! 确保用户已有 Claude Code 配置仍可人工恢复。

use crate::agent_monitor::DEFAULT_AGENT_MONITOR_PORT;
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri_plugin_opener::OpenerExt;

const AI_PAD_HOOK_MARKER: &str = "ai-pad-claude-code-watch";

pub fn claude_dir() -> Result<PathBuf, String> {
    home_dir()
        .map(|dir| dir.join(".claude"))
        .ok_or_else(|| "无法解析 home 目录".to_string())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}

pub fn hook_script_path() -> Result<PathBuf, String> {
    Ok(claude_dir()?.join("hooks").join("ai-pad-hook.ps1"))
}

pub fn settings_path() -> Result<PathBuf, String> {
    Ok(claude_dir()?.join("settings.json"))
}

pub fn install_claude_code_hooks() -> Result<String, String> {
    let script_path = hook_script_path()?;
    if let Some(parent) = script_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建 hook 目录失败: {e}"))?;
    }
    atomic_write(&script_path, &hook_script(DEFAULT_AGENT_MONITOR_PORT))?;

    let settings_path = settings_path()?;
    let mut settings = read_settings_json(&settings_path)?;
    ensure_ai_pad_hooks(&mut settings, &script_path)?;
    backup_if_exists(&settings_path)?;
    atomic_write(
        &settings_path,
        &serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?,
    )?;
    Ok(format!(
        "已安装 Claude Code 看管 hook: {}",
        script_path.display()
    ))
}

fn read_settings_json(path: &PathBuf) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let raw = std::fs::read_to_string(path).map_err(|e| format!("读取 settings.json 失败: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("解析 settings.json 失败，已停止写入: {e}"))
}

fn ensure_ai_pad_hooks(settings: &mut Value, script_path: &PathBuf) -> Result<(), String> {
    if !settings.is_object() {
        return Err("settings.json 根节点不是 object，已停止写入".into());
    }
    let root = settings.as_object_mut().unwrap();
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        return Err("settings.json hooks 字段不是 object，已停止写入".into());
    }
    let hooks_obj = hooks.as_object_mut().unwrap();

    for event_name in [
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
        "PreCompact",
        "Stop",
        "SessionEnd",
        "Notification",
    ] {
        let entry = hooks_obj.entry(event_name).or_insert_with(|| json!([]));
        if !entry.is_array() {
            return Err(format!(
                "settings.json hooks.{event_name} 不是 array，已停止写入"
            ));
        }
        let arr = entry.as_array_mut().unwrap();
        arr.retain(|item| {
            item.get("ai_pad_marker").and_then(Value::as_str) != Some(AI_PAD_HOOK_MARKER)
        });
        arr.push(json!({
            "type": "command",
            "command": format!("powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\"", script_path.display()),
            "ai_pad_marker": AI_PAD_HOOK_MARKER
        }));
    }
    Ok(())
}

fn hook_script(port: u16) -> String {
    format!(
        r#"# {AI_PAD_HOOK_MARKER}
# Installed by 8Bit Cat. Read-only: sends Claude Code hook JSON to local ai-pad monitor.
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$ErrorActionPreference = "Stop"

$raw = [Console]::In.ReadToEnd()
if ([string]::IsNullOrWhiteSpace($raw)) {{ exit 0 }}

try {{
  $client = [System.Net.Sockets.TcpClient]::new()
  $client.Connect("127.0.0.1", {port})
  $stream = $client.GetStream()
  $bytes = [System.Text.Encoding]::UTF8.GetBytes($raw)
  $stream.Write($bytes, 0, $bytes.Length)
  $stream.Flush()
  $client.Client.Shutdown([System.Net.Sockets.SocketShutdown]::Send)
  $stream.Dispose()
  $client.Dispose()
}} catch {{
  # Keep Claude Code hooks non-blocking for the user.
  exit 0
}}
"#
    )
}

fn backup_if_exists(path: &PathBuf) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let backup = path.with_file_name(format!("settings.ai-pad-backup-{stamp}.json"));
    std::fs::copy(path, &backup).map_err(|e| format!("备份 settings.json 失败: {e}"))?;
    Ok(())
}

fn atomic_write(path: &PathBuf, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败: {e}"))?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| format!("替换文件失败: {e}"))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("保存文件失败: {e}"))
}

#[tauri::command]
pub async fn cmd_install_claude_code_hooks() -> Result<String, String> {
    install_claude_code_hooks()
}

#[tauri::command]
pub async fn cmd_open_claude_settings(app: tauri::AppHandle) -> Result<(), String> {
    let path = settings_path()?;
    app.opener()
        .open_path(path.to_string_lossy().to_string(), None::<String>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_script_contains_port_and_utf8() {
        let script = hook_script(19283);
        assert!(script.contains("127.0.0.1"));
        assert!(script.contains("19283"));
        assert!(script.contains("InputEncoding"));
    }

    #[test]
    fn ensure_hooks_preserves_other_entries_and_dedupes() {
        let mut settings = json!({
            "hooks": {
                "Stop": [
                    {"type": "command", "command": "echo keep"},
                    {"type": "command", "command": "old", "ai_pad_marker": AI_PAD_HOOK_MARKER}
                ]
            }
        });
        ensure_ai_pad_hooks(&mut settings, &PathBuf::from("C:\\x\\ai-pad-hook.ps1")).unwrap();
        let stop = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["command"], "echo keep");
        assert_eq!(stop[1]["ai_pad_marker"], AI_PAD_HOOK_MARKER);
        assert!(settings["hooks"]["UserPromptSubmit"].is_array());
    }
}
