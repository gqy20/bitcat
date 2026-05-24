//! Codex hook 安装器。
//!
//! 本模块写入 ai-pad 自己的 PowerShell hook 脚本，并把只读 command hook 合并到
//! `~/.codex/config.toml`。Codex hook payload 与 Claude Code hook payload 高度同构，
//! 因此脚本只负责加一层 source envelope 后转发给现有 Agent Watch TCP monitor。

use crate::agent_monitor::DEFAULT_AGENT_MONITOR_PORT;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tauri_plugin_opener::OpenerExt;
use toml_edit::{value, ArrayOfTables, DocumentMut, Item, Table};

const AI_PAD_HOOK_MARKER: &str = "bitcat-codex-watch";

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

pub fn codex_dir() -> Result<PathBuf, String> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
                .map(|dir| dir.join(".codex"))
        })
        .ok_or_else(|| "无法解析 CODEX_HOME 或 home 目录".to_string())
}

pub fn hook_script_path() -> Result<PathBuf, String> {
    Ok(codex_dir()?.join("hooks").join("bitcat-codex-hook.ps1"))
}

pub fn config_path() -> Result<PathBuf, String> {
    Ok(codex_dir()?.join("config.toml"))
}

pub fn install_codex_hooks() -> Result<String, String> {
    let mut report = HookRepairReport::default();
    let script_path = hook_script_path()?;
    if let Some(parent) = script_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建 Codex hook 目录失败: {e}"))?;
    }
    let script = hook_script(DEFAULT_AGENT_MONITOR_PORT);
    report.script_updated = !file_content_matches(&script_path, &script);
    atomic_write(&script_path, &script)?;

    let config_path = config_path()?;
    let mut doc = read_config_toml(&config_path)?;
    ensure_ai_pad_hooks(&mut doc, &script_path, &mut report)?;
    backup_if_exists(&config_path)?;
    atomic_write(&config_path, &doc.to_string())?;

    Ok(report.message("Codex", &script_path))
}
fn read_config_toml(path: &PathBuf) -> Result<DocumentMut, String> {
    if !path.exists() {
        return Ok(DocumentMut::new());
    }
    let raw =
        std::fs::read_to_string(path).map_err(|e| format!("读取 Codex config.toml 失败: {e}"))?;
    raw.parse::<DocumentMut>()
        .map_err(|e| format!("解析 Codex config.toml 失败，已停止写入: {e}"))
}

fn ensure_ai_pad_hooks(
    doc: &mut DocumentMut,
    script_path: &Path,
    report: &mut HookRepairReport,
) -> Result<(), String> {
    let hooks = ensure_table(&mut doc["hooks"])?;
    remove_invalid_ai_pad_events(hooks, report);
    let hook = ai_pad_hook(script_path);
    let mut cleaned_events = HashSet::new();

    for spec in hook_specs() {
        let groups = ensure_array_of_tables(&mut hooks[spec.event_name])?;
        if cleaned_events.insert(spec.event_name) {
            report.removed += remove_ai_pad_hooks(groups);
        }
        let group = ensure_hook_group(groups, spec.matcher);
        let hooks = ensure_array_of_tables(&mut group["hooks"])?;
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
            event_name: "PermissionRequest",
            matcher: Some("*"),
        },
        HookSpec {
            event_name: "PostToolUse",
            matcher: Some("*"),
        },
        HookSpec {
            event_name: "PreCompact",
            matcher: Some("*"),
        },
        HookSpec {
            event_name: "PostCompact",
            matcher: Some("*"),
        },
        HookSpec {
            event_name: "Stop",
            matcher: None,
        },
    ]
}

fn valid_hook_event(event_name: &str) -> bool {
    hook_specs()
        .iter()
        .any(|spec| spec.event_name == event_name)
}

fn remove_invalid_ai_pad_events(hooks: &mut Table, report: &mut HookRepairReport) {
    let invalid_events = hooks
        .iter()
        .filter_map(|(event_name, _)| {
            if valid_hook_event(event_name) {
                None
            } else {
                Some(event_name.to_string())
            }
        })
        .collect::<Vec<_>>();
    for event_name in invalid_events {
        let Some(groups) = hooks[&event_name].as_array_of_tables_mut() else {
            continue;
        };
        let removed = remove_ai_pad_hooks(groups);
        report.removed += removed;
        if removed > 0 && groups.is_empty() {
            hooks.remove(&event_name);
        }
    }
}

fn ai_pad_hook(script_path: &Path) -> Table {
    let script = script_path.to_string_lossy().replace('\\', "\\\\");
    let mut hook = Table::new();
    hook["type"] = value("command");
    hook["command"] = value(format!(
        "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{script}\""
    ));
    hook["commandWindows"] = value(format!(
        "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{script}\""
    ));
    hook["timeout"] = value(5);
    hook["ai_pad_marker"] = value(AI_PAD_HOOK_MARKER);
    hook
}

fn ensure_table(item: &mut Item) -> Result<&mut Table, String> {
    if item.is_none() {
        *item = Item::Table(Table::new());
    }
    item.as_table_mut()
        .ok_or_else(|| "Codex config.toml hooks 字段不是 table，已停止写入".to_string())
}

fn ensure_array_of_tables(item: &mut Item) -> Result<&mut ArrayOfTables, String> {
    if item.is_none() {
        *item = Item::ArrayOfTables(ArrayOfTables::new());
    }
    item.as_array_of_tables_mut()
        .ok_or_else(|| "Codex hook 配置节点不是 array of tables，已停止写入".to_string())
}

fn remove_ai_pad_hooks(groups: &mut ArrayOfTables) -> usize {
    let mut removed = 0;
    for group in groups.iter_mut() {
        if let Some(hooks) = group["hooks"].as_array_of_tables_mut() {
            let mut kept = ArrayOfTables::new();
            for hook in hooks.iter() {
                if hook
                    .get("ai_pad_marker")
                    .and_then(Item::as_value)
                    .and_then(|v| v.as_str())
                    != Some(AI_PAD_HOOK_MARKER)
                {
                    kept.push(hook.clone());
                } else {
                    removed += 1;
                }
            }
            group["hooks"] = Item::ArrayOfTables(kept);
        }
    }
    removed
}

fn ensure_hook_group<'a>(groups: &'a mut ArrayOfTables, matcher: Option<&str>) -> &'a mut Table {
    let existing_index = groups
        .iter()
        .position(|group| matcher_matches(group.get("matcher"), matcher));
    if let Some(index) = existing_index {
        return groups
            .get_mut(index)
            .expect("group index from position exists");
    }

    let mut group = Table::new();
    if let Some(matcher) = matcher {
        group["matcher"] = value(matcher);
    }
    group["hooks"] = Item::ArrayOfTables(ArrayOfTables::new());
    groups.push(group);
    let last = groups.len().saturating_sub(1);
    groups.get_mut(last).expect("group was just pushed")
}

fn matcher_matches(value: Option<&Item>, expected: Option<&str>) -> bool {
    match (
        value
            .and_then(Item::as_value)
            .and_then(|value| value.as_str()),
        expected,
    ) {
        (None, None) => true,
        (Some(actual), Some(expected)) => actual == expected,
        _ => false,
    }
}

fn hook_script(port: u16) -> String {
    format!(
        r#"# {AI_PAD_HOOK_MARKER}
# Installed by BitCat. Read-only: sends Codex hook JSON to local BitCat monitor.
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$ErrorActionPreference = "Stop"

$payload = $null
$logDir = Join-Path $HOME ".bitcat\logs"
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
      source = "codex"
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
    schema = "bitcat.agent-hook.v1"
    source = "codex"
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
  # Keep Codex hooks non-blocking for the user.
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
    let backup = path.with_file_name(format!("config.bitcat-backup-{stamp}.toml"));
    std::fs::copy(path, &backup).map_err(|e| format!("备份 Codex config.toml 失败: {e}"))?;
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
pub async fn cmd_install_codex_hooks() -> Result<String, String> {
    install_codex_hooks()
}

#[tauri::command]
pub async fn cmd_open_codex_config(app: tauri::AppHandle) -> Result<(), String> {
    let path = config_path()?;
    app.opener()
        .open_path(path.to_string_lossy().to_string(), None::<String>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_script_wraps_codex_source() {
        let script = hook_script(5342);
        assert!(script.contains("127.0.0.1"));
        assert!(script.contains("source = \"codex\""));
        assert!(script.contains("ConvertTo-Json"));
    }

    #[test]
    fn ensure_hooks_preserves_other_entries_and_dedupes() {
        let mut doc = r#"
[hooks]

[[hooks.PreToolUse]]
matcher = "*"

[[hooks.PreToolUse.hooks]]
type = "command"
command = "echo keep"

[[hooks.PreToolUse.hooks]]
type = "command"
command = "old"
ai_pad_marker = "bitcat-codex-watch"
"#
        .parse::<DocumentMut>()
        .unwrap();

        let mut report = HookRepairReport::default();
        ensure_ai_pad_hooks(
            &mut doc,
            &PathBuf::from("C:\\x\\bitcat-codex-hook.ps1"),
            &mut report,
        )
        .unwrap();
        let rendered = doc.to_string();
        assert_eq!(rendered.matches("echo keep").count(), 1);
        assert_eq!(
            rendered.matches(AI_PAD_HOOK_MARKER).count(),
            hook_specs().len()
        );
        assert!(rendered.contains("[[hooks.UserPromptSubmit]]"));
        assert!(rendered.contains("[[hooks.PermissionRequest]]"));
        assert!(rendered.contains("commandWindows"));
        assert_eq!(report.installed, hook_specs().len());
        assert_eq!(report.removed, 1);
    }

    #[test]
    fn removes_only_ai_pad_hooks_from_invalid_events() {
        let mut doc = r#"
[hooks]

[[hooks.OldEvent]]

[[hooks.OldEvent.hooks]]
type = "command"
command = "echo keep"

[[hooks.OldEvent.hooks]]
type = "command"
command = "old"
ai_pad_marker = "bitcat-codex-watch"
"#
        .parse::<DocumentMut>()
        .unwrap();
        let mut report = HookRepairReport::default();
        ensure_ai_pad_hooks(
            &mut doc,
            &PathBuf::from("C:\\x\\bitcat-codex-hook.ps1"),
            &mut report,
        )
        .unwrap();
        let rendered = doc.to_string();
        assert!(rendered.contains("echo keep"));
        assert!(!rendered.contains("command = \"old\""));
        assert_eq!(report.removed, 1);
    }
}
