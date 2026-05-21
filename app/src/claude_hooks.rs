//! Claude Code hook 安装器。
//!
//! 本模块只写入 ai-pad 自己的 PowerShell hook 脚本，并在 `~/.claude/settings.json`
//! 中合并带有 ai-pad 标记的 hook 配置。写入前会备份 settings，失败时不覆盖，
//! 确保用户已有 Claude Code 配置仍可人工恢复。

use crate::agent_monitor::DEFAULT_AGENT_MONITOR_PORT;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tauri_plugin_opener::OpenerExt;

const AI_PAD_HOOK_MARKER: &str = "ai-pad-claude-code-watch";

#[derive(Debug, Clone, Copy)]
struct HookSpec {
    event_name: &'static str,
    matcher: Option<&'static str>,
}

#[derive(Debug, Default)]
struct HookRepairReport {
    removed: usize,
    installed: usize,
    script_updated: bool,
}

impl HookRepairReport {
    fn message(&self, target: &str, script_path: &Path) -> String {
        format!(
            "{target} hook ready: {} installed, {} repaired, script {} ({})",
            self.installed,
            self.removed,
            if self.script_updated {
                "updated"
            } else {
                "unchanged"
            },
            script_path.display()
        )
    }
}

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
    let mut report = HookRepairReport::default();
    let script_path = hook_script_path()?;
    if let Some(parent) = script_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建 hook 目录失败: {e}"))?;
    }
    let script = hook_script(DEFAULT_AGENT_MONITOR_PORT);
    report.script_updated = !file_content_matches(&script_path, &script);
    atomic_write(&script_path, &script)?;

    let settings_path = settings_path()?;
    let mut settings = read_settings_json(&settings_path)?;
    ensure_ai_pad_hooks(&mut settings, &script_path, &mut report)?;
    backup_if_exists(&settings_path)?;
    atomic_write(
        &settings_path,
        &serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?,
    )?;
    Ok(report.message("Claude Code", &script_path))
}
fn read_settings_json(path: &PathBuf) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let raw = std::fs::read_to_string(path).map_err(|e| format!("读取 settings.json 失败: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("解析 settings.json 失败，已停止写入: {e}"))
}

fn ensure_ai_pad_hooks(
    settings: &mut Value,
    script_path: &Path,
    report: &mut HookRepairReport,
) -> Result<(), String> {
    if !settings.is_object() {
        return Err("settings.json 根节点不是 object，已停止写入".into());
    }
    let root = settings.as_object_mut().unwrap();
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        return Err("settings.json hooks 字段不是 object，已停止写入".into());
    }
    let hooks_obj = hooks.as_object_mut().unwrap();

    remove_invalid_ai_pad_events(hooks_obj, report);
    let hook = ai_pad_hook(script_path);
    let mut cleaned_events = HashSet::new();
    for spec in hook_specs() {
        let entry = hooks_obj
            .entry(spec.event_name)
            .or_insert_with(|| json!([]));
        if !entry.is_array() {
            return Err(format!(
                "settings.json hooks.{} 不是 array，已停止写入",
                spec.event_name
            ));
        }
        let arr = entry.as_array_mut().unwrap();
        if cleaned_events.insert(spec.event_name) {
            report.removed += remove_ai_pad_hooks(arr);
        }
        let group = ensure_hook_group(arr, spec.matcher);
        let hooks = group
            .entry("hooks")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                format!(
                    "settings.json hooks.{} 分组 hooks 不是 array，已停止写入",
                    spec.event_name
                )
            })?;
        hooks.push(hook.clone());
        report.installed += 1;
    }
    Ok(())
}

fn hook_specs() -> Vec<HookSpec> {
    vec![
        HookSpec {
            event_name: "UserPromptSubmit",
            matcher: None,
        },
        HookSpec {
            event_name: "SessionStart",
            matcher: None,
        },
        HookSpec {
            event_name: "PreToolUse",
            matcher: Some("*"),
        },
        HookSpec {
            event_name: "PostToolUse",
            matcher: Some("*"),
        },
        HookSpec {
            event_name: "PostToolUseFailure",
            matcher: Some("*"),
        },
        HookSpec {
            event_name: "PostToolBatch",
            matcher: None,
        },
        HookSpec {
            event_name: "PermissionRequest",
            matcher: Some("*"),
        },
        HookSpec {
            event_name: "PermissionDenied",
            matcher: Some("*"),
        },
        HookSpec {
            event_name: "PreCompact",
            matcher: Some("auto"),
        },
        HookSpec {
            event_name: "PreCompact",
            matcher: Some("manual"),
        },
        HookSpec {
            event_name: "Stop",
            matcher: None,
        },
        HookSpec {
            event_name: "StopFailure",
            matcher: None,
        },
        HookSpec {
            event_name: "SubagentStop",
            matcher: None,
        },
        HookSpec {
            event_name: "SubagentStart",
            matcher: None,
        },
        HookSpec {
            event_name: "TaskCreated",
            matcher: None,
        },
        HookSpec {
            event_name: "TaskCompleted",
            matcher: None,
        },
        HookSpec {
            event_name: "SessionEnd",
            matcher: None,
        },
        HookSpec {
            event_name: "Notification",
            matcher: None,
        },
    ]
}

fn valid_hook_event(event_name: &str) -> bool {
    hook_specs()
        .iter()
        .any(|spec| spec.event_name == event_name)
}

fn remove_invalid_ai_pad_events(
    hooks_obj: &mut serde_json::Map<String, Value>,
    report: &mut HookRepairReport,
) {
    let invalid_events = hooks_obj
        .keys()
        .filter(|event_name| !valid_hook_event(event_name))
        .cloned()
        .collect::<Vec<_>>();
    for event_name in invalid_events {
        let Some(groups) = hooks_obj.get_mut(&event_name).and_then(Value::as_array_mut) else {
            continue;
        };
        let removed = remove_ai_pad_hooks(groups);
        report.removed += removed;
        if removed > 0 && groups.is_empty() {
            hooks_obj.remove(&event_name);
        }
    }
}

fn ai_pad_hook(script_path: &Path) -> Value {
    let script = script_path.to_string_lossy().replace('\\', "/");
    json!({
        "type": "command",
        "command": format!("powershell.exe -NoProfile -ExecutionPolicy Bypass -File '{script}'"),
        "ai_pad_marker": AI_PAD_HOOK_MARKER
    })
}

fn remove_ai_pad_hooks(groups: &mut Vec<Value>) -> usize {
    let before_groups = groups.len();
    groups.retain(|item| {
        item.get("ai_pad_marker").and_then(Value::as_str) != Some(AI_PAD_HOOK_MARKER)
    });
    let mut removed = before_groups.saturating_sub(groups.len());
    for group in groups.iter_mut() {
        if let Some(hooks) = group.get_mut("hooks").and_then(Value::as_array_mut) {
            let before_hooks = hooks.len();
            hooks.retain(|item| {
                item.get("ai_pad_marker").and_then(Value::as_str) != Some(AI_PAD_HOOK_MARKER)
            });
            removed += before_hooks.saturating_sub(hooks.len());
        }
    }
    groups.retain(|item| {
        let Some(object) = item.as_object() else {
            return true;
        };
        object
            .get("hooks")
            .and_then(Value::as_array)
            .map(|hooks| !hooks.is_empty())
            .unwrap_or(true)
            || object.keys().any(|key| key != "hooks" && key != "matcher")
    });
    removed
}

fn ensure_hook_group<'a>(
    groups: &'a mut Vec<Value>,
    matcher: Option<&str>,
) -> &'a mut serde_json::Map<String, Value> {
    if let Some(index) = groups.iter().position(|item| {
        item.as_object()
            .map(|object| {
                matcher_matches(object.get("matcher"), matcher) && object.get("hooks").is_some()
            })
            .unwrap_or(false)
    }) {
        return groups[index].as_object_mut().unwrap();
    }

    let mut group = serde_json::Map::new();
    if let Some(matcher) = matcher {
        group.insert("matcher".into(), Value::String(matcher.into()));
    }
    group.insert("hooks".into(), Value::Array(Vec::new()));
    groups.push(Value::Object(group));
    groups.last_mut().unwrap().as_object_mut().unwrap()
}

fn matcher_matches(value: Option<&Value>, expected: Option<&str>) -> bool {
    match (value.and_then(Value::as_str), expected) {
        (None, None) => true,
        (Some(actual), Some(expected)) => actual == expected,
        _ => false,
    }
}

fn hook_script(port: u16) -> String {
    format!(
        r#"# {AI_PAD_HOOK_MARKER}
# Installed by 8Bit Cat. Read-only: sends Claude Code hook JSON to local ai-pad monitor.
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$ErrorActionPreference = "Stop"

$payload = $null
$logDir = Join-Path $HOME ".ai-pad\logs"
$logFile = Join-Path $logDir "agent_hook_bridge.jsonl"

function Get-Field($obj, [string[]]$names) {{
  foreach ($name in $names) {{
    if ($null -ne $obj -and $obj.PSObject.Properties.Name -contains $name) {{
      return [string]$obj.$name
    }}
  }}
  return $null
}}

function Write-AiPadHookLog([string]$status, [string]$detail) {{
  try {{
    New-Item -ItemType Directory -Force -Path $logDir | Out-Null
    if ((Test-Path $logFile) -and ((Get-Item $logFile).Length -gt 1048576)) {{
      Move-Item -Force $logFile "$logFile.1"
    }}
    $entry = [ordered]@{{
      ts = (Get-Date).ToUniversalTime().ToString("o")
      source = "claude"
      status = $status
      detail = $detail
      hook = Get-Field $payload @("hook_event_name", "hookEventName", "event")
      session_id = Get-Field $payload @("session_id", "sessionId")
      tool = Get-Field $payload @("tool_name", "toolName")
      bytes = if ($null -eq $raw) {{ 0 }} else {{ $raw.Length }}
    }} | ConvertTo-Json -Depth 8 -Compress
    Add-Content -LiteralPath $logFile -Value $entry -Encoding UTF8
  }} catch {{}}
}}

$raw = [Console]::In.ReadToEnd()
if ([string]::IsNullOrWhiteSpace($raw)) {{ exit 0 }}

try {{
  $payload = $raw | ConvertFrom-Json
  $envelope = [ordered]@{{
    schema = "ai-pad.agent-hook.v1"
    source = "claude"
    machine = $env:COMPUTERNAME
    payload = $payload
  }} | ConvertTo-Json -Depth 100 -Compress

  $client = [System.Net.Sockets.TcpClient]::new()
  $client.Connect("127.0.0.1", {port})
  $stream = $client.GetStream()
  $bytes = [System.Text.Encoding]::UTF8.GetBytes($envelope)
  $stream.Write($bytes, 0, $bytes.Length)
  $stream.Flush()
  $client.Client.Shutdown([System.Net.Sockets.SocketShutdown]::Send)
  $stream.Dispose()
  $client.Dispose()
  Write-AiPadHookLog "sent" "127.0.0.1:{port}"
}} catch {{
  Write-AiPadHookLog "failed" $_.Exception.GetType().Name
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

fn file_content_matches(path: &PathBuf, expected: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|actual| actual == expected)
        .unwrap_or(false)
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
        let script = hook_script(5342);
        assert!(script.contains("127.0.0.1"));
        assert!(script.contains("5342"));
        assert!(script.contains("InputEncoding"));
    }

    #[test]
    fn ensure_hooks_preserves_other_entries_and_dedupes() {
        let mut settings = json!({
            "hooks": {
                "Stop": [
                    {
                        "hooks": [
                            {"type": "command", "command": "echo keep"},
                            {"type": "command", "command": "old", "ai_pad_marker": AI_PAD_HOOK_MARKER}
                        ]
                    },
                    {"type": "command", "command": "old-flat", "ai_pad_marker": AI_PAD_HOOK_MARKER}
                ],
                "PreToolUse": [
                    {
                        "matcher": "*",
                        "hooks": [
                            {"type": "command", "command": "echo tool"}
                        ]
                    }
                ]
            }
        });
        let mut report = HookRepairReport::default();
        ensure_ai_pad_hooks(
            &mut settings,
            &PathBuf::from("C:\\x\\ai-pad-hook.ps1"),
            &mut report,
        )
        .unwrap();
        let stop = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        let stop_hooks = stop[0]["hooks"].as_array().unwrap();
        assert_eq!(stop_hooks.len(), 2);
        assert_eq!(stop_hooks[0]["command"], "echo keep");
        assert_eq!(stop_hooks[1]["ai_pad_marker"], AI_PAD_HOOK_MARKER);
        let pre_tool = settings["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre_tool.len(), 1);
        assert_eq!(pre_tool[0]["matcher"], "*");
        assert_eq!(pre_tool[0]["hooks"].as_array().unwrap().len(), 2);
        assert!(settings["hooks"]["UserPromptSubmit"].is_array());
        assert!(settings["hooks"]["SessionStart"].is_array());
        assert!(settings["hooks"]["SubagentStart"].is_array());
        assert!(settings["hooks"]["SubagentStop"].is_array());
        assert!(settings["hooks"]["TaskCreated"].is_array());
        assert!(settings["hooks"]["TaskCompleted"].is_array());
        assert!(settings["hooks"]["PostToolBatch"].is_array());
        assert!(settings["hooks"].get("SubagentStopFailure").is_none());
        assert!(settings["hooks"]["StopFailure"].is_array());
        assert!(settings["hooks"]["PermissionDenied"].is_array());
        assert!(settings["hooks"]["PostToolUseFailure"].is_array());
        assert_eq!(settings["hooks"]["PreCompact"].as_array().unwrap().len(), 2);
        assert_eq!(report.installed, hook_specs().len());
        assert_eq!(report.removed, 2);
    }

    #[test]
    fn removes_only_ai_pad_hooks_from_invalid_events() {
        let mut settings = json!({
            "hooks": {
                "SubagentStopFailure": [
                    {
                        "hooks": [
                            {"type": "command", "command": "echo keep"},
                            {"type": "command", "command": "old", "ai_pad_marker": AI_PAD_HOOK_MARKER}
                        ]
                    }
                ],
                "TotallyUnknown": [
                    {"type": "command", "command": "old-flat", "ai_pad_marker": AI_PAD_HOOK_MARKER}
                ]
            }
        });
        let mut report = HookRepairReport::default();
        ensure_ai_pad_hooks(
            &mut settings,
            &PathBuf::from("C:\\x\\ai-pad-hook.ps1"),
            &mut report,
        )
        .unwrap();
        assert_eq!(
            settings["hooks"]["SubagentStopFailure"][0]["hooks"][0]["command"],
            "echo keep"
        );
        assert!(settings["hooks"].get("TotallyUnknown").is_none());
        assert_eq!(report.removed, 2);
    }
}
