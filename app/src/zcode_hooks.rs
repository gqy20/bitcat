//! ZCode 原生 hook 安装器。
//!
//! ZCode 通过 `~/.zcode/cli/config.json` 的用户级 hooks 转发会话事件；本模块
//! 只合并带 BitCat 标记的只读转发器，不修改项目配置，也不绕过 ZCode 的权限模型。
//! 写入前备份配置，保留用户已有 hook，并把宿主不可用时的时间限制在 2 秒内。

use crate::agent_monitor::DEFAULT_AGENT_MONITOR_PORT;
use crate::hook_install_common::{
    atomic_write, backup_if_exists, cleanup_stale_scripts, file_content_matches,
    powershell_hook_script,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri_plugin_opener::OpenerExt;

const BITCAT_HOOK_MARKER: &str = "bitcat-zcode-watch";
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";
const HOOK_TIMEOUT_MS: u64 = 2_000;
const DEFAULT_HOOK_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_MAX_OUTPUT_BYTES: u64 = 32_768;
const HOOK_EVENTS: [&str; 7] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PostToolUseFailure",
    "Stop",
];

#[derive(Debug, Default)]
struct HookRepairReport {
    removed: usize,
    installed: usize,
    script_updated: bool,
}

impl HookRepairReport {
    fn message(&self, script_path: &Path) -> String {
        format!(
            "ZCode 连接就绪：{} 个事件已接入，{} 个旧连接已清理，脚本{}（{}）",
            self.installed,
            self.removed,
            if self.script_updated {
                "已更新"
            } else {
                "无变化"
            },
            script_path.display()
        )
    }
}

/// ZCode 用户数据目录。
pub fn zcode_dir() -> Result<PathBuf, String> {
    home_dir()
        .map(|dir| dir.join(".zcode"))
        .ok_or_else(|| "无法解析 home 目录".to_string())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}

/// ZCode 用户级配置文件路径。
pub fn config_path() -> Result<PathBuf, String> {
    Ok(zcode_dir()?.join("cli").join("config.json"))
}

/// BitCat 转发脚本路径。
pub fn hook_script_path() -> Result<PathBuf, String> {
    Ok(zcode_dir()?.join("hooks").join("bitcat-zcode-hook.ps1"))
}

/// 安装或修复 ZCode 用户级连接。
///
/// 若用户已在 ZCode 中显式关闭全部 Hook，则停止并提示用户手动开启，不覆盖
/// 用户的全局选择。安装后需要新开会话或发送 `/reload` 才会加载配置。
pub fn install_zcode_hooks() -> Result<String, String> {
    let mut report = HookRepairReport::default();
    let config_path = config_path()?;
    let script_path = hook_script_path()?;
    let mut config = read_config_json(&config_path)?;
    ensure_zcode_hooks(&mut config, &script_path, &mut report)?;

    let script = hook_script(DEFAULT_AGENT_MONITOR_PORT);
    report.script_updated = !file_content_matches(&script_path, &script);
    if let Some(parent) = script_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建 ZCode hook 目录失败: {e}"))?;
    }
    atomic_write(&script_path, &script)?;
    if let Some(hooks_dir) = script_path.parent() {
        cleanup_stale_scripts(hooks_dir);
    }

    backup_if_exists(&config_path, "config.bitcat-backup.json")?;
    atomic_write(
        &config_path,
        &serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?,
    )?;
    Ok(report.message(&script_path))
}

/// 探测 ZCode 配置中是否仍有 BitCat 连接标记。
pub fn is_installed() -> bool {
    config_path()
        .and_then(|path| std::fs::read_to_string(path).map_err(|e| e.to_string()))
        .map(|content| content.contains(BITCAT_HOOK_MARKER))
        .unwrap_or(false)
}

/// 探测已安装脚本是否与当前代码生成的版本一致。
pub fn is_script_current() -> bool {
    hook_script_path()
        .map(|path| file_content_matches(&path, &hook_script(DEFAULT_AGENT_MONITOR_PORT)))
        .unwrap_or(false)
}

fn hook_script(port: u16) -> String {
    powershell_hook_script(BITCAT_HOOK_MARKER, "zcode", "ZCode", port)
}

fn powershell_executable() -> String {
    let windir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(windir)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe")
        .to_string_lossy()
        .replace('\\', "/")
}

fn hook_runner(script_path: &Path) -> Value {
    json!({
        "type": "process",
        "command": powershell_executable(),
        "args": [
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            script_path.to_string_lossy().replace('\\', "/")
        ],
        "timeoutMs": HOOK_TIMEOUT_MS,
        "onError": "allow",
        "bitcat_marker": BITCAT_HOOK_MARKER
    })
}

fn ensure_zcode_hooks(
    config: &mut Value,
    script_path: &Path,
    report: &mut HookRepairReport,
) -> Result<(), String> {
    if !config.is_object() {
        return Err("ZCode config.json 根节点不是 object，已停止写入".into());
    }
    let root = config.as_object_mut().unwrap();
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        return Err("ZCode config.json hooks 字段不是 object，已停止写入".into());
    }
    let hooks_obj = hooks.as_object_mut().unwrap();

    match hooks_obj.get("enabled") {
        Some(Value::Bool(false)) => {
            return Err(
                "ZCode 的连接功能当前已关闭。请先在 ZCode 设置 → 连接中开启，再点击修复。".into(),
            )
        }
        Some(Value::Bool(true)) | None => {}
        Some(_) => return Err("ZCode config.json hooks.enabled 不是布尔值，已停止写入".into()),
    }
    hooks_obj.entry("enabled").or_insert(json!(true));
    hooks_obj
        .entry("timeoutMs")
        .or_insert(json!(DEFAULT_HOOK_TIMEOUT_MS));
    hooks_obj
        .entry("maxOutputBytes")
        .or_insert(json!(DEFAULT_MAX_OUTPUT_BYTES));

    let events = hooks_obj.entry("events").or_insert_with(|| json!({}));
    if !events.is_object() {
        return Err("ZCode config.json hooks.events 不是 object，已停止写入".into());
    }
    let events_obj = events.as_object_mut().unwrap();

    // 先清理所有事件里的旧 BitCat ZCode 连接，包括未来版本已移除的事件。
    // 只在本次确实删掉了标记 Hook 且数组变空时移除事件键，避免动用户自己的空配置。
    let mut empty_after_cleanup = Vec::new();
    for (event, groups) in events_obj.iter_mut() {
        if let Some(groups) = groups.as_array_mut() {
            let removed_before = report.removed;
            remove_marked_hooks(groups, report);
            if report.removed > removed_before && groups.is_empty() {
                empty_after_cleanup.push(event.clone());
            }
        }
    }
    for event in empty_after_cleanup {
        events_obj.remove(&event);
    }

    let runner = hook_runner(script_path);
    for event in HOOK_EVENTS {
        let entry = events_obj.entry(event).or_insert_with(|| json!([]));
        if !entry.is_array() {
            return Err(format!(
                "ZCode config.json hooks.events.{event} 不是 array，已停止写入"
            ));
        }
        entry
            .as_array_mut()
            .unwrap()
            .push(json!({ "hooks": [runner.clone()] }));
        report.installed += 1;
    }
    Ok(())
}

fn remove_marked_hooks(groups: &mut Vec<Value>, report: &mut HookRepairReport) {
    let before_groups = groups.len();
    groups.retain(|group| !is_marked_hook(group));
    report.removed += before_groups.saturating_sub(groups.len());

    for group in groups.iter_mut() {
        if let Some(hooks) = group.get_mut("hooks").and_then(Value::as_array_mut) {
            let before_hooks = hooks.len();
            hooks.retain(|hook| !is_marked_hook(hook));
            report.removed += before_hooks.saturating_sub(hooks.len());
        }
    }

    groups.retain(|group| {
        let Some(object) = group.as_object() else {
            return true;
        };
        let empty_hooks = object
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| hooks.is_empty());
        !(empty_hooks && object.keys().all(|key| key == "hooks" || key == "matcher"))
    });
}

fn is_marked_hook(value: &Value) -> bool {
    value.get("bitcat_marker").and_then(Value::as_str) == Some(BITCAT_HOOK_MARKER)
}

fn read_config_json(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("读取 ZCode config.json 失败: {e}"))?;
    let json_bytes = if bytes.starts_with(UTF8_BOM) {
        tracing::warn!(
            path = %path.display(),
            "ZCode config.json 包含 UTF-8 BOM，将在本次连接修复写回时清理"
        );
        &bytes[UTF8_BOM.len()..]
    } else {
        bytes.as_slice()
    };
    let raw = std::str::from_utf8(json_bytes)
        .map_err(|e| format!("ZCode config.json 不是有效 UTF-8，已停止写入: {e}"))?;
    serde_json::from_str(raw).map_err(|e| {
        format!(
            "解析 ZCode config.json 失败，已停止写入: {e} (path={}, head_bytes={})",
            path.display(),
            head_bytes_hex(&bytes)
        )
    })
}

fn head_bytes_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

#[tauri::command]
pub async fn cmd_install_zcode_hooks() -> Result<String, String> {
    install_zcode_hooks()
}

#[tauri::command]
pub async fn cmd_open_zcode_config(app: tauri::AppHandle) -> Result<(), String> {
    let path = config_path()?;
    app.opener()
        .open_path(path.to_string_lossy().to_string(), None::<String>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script_path() -> PathBuf {
        PathBuf::from("C:/Users/test/.zcode/hooks/bitcat-zcode-hook.ps1")
    }

    #[test]
    fn hook_script_targets_zcode_and_monitor() {
        let script = hook_script(5342);
        assert!(script.starts_with("# bitcat-zcode-watch\n"));
        assert!(script.contains("source = \"zcode\""));
        assert!(script.contains("127.0.0.1"));
        assert!(script.contains("monitor-state"));
        assert!(script.contains("connect timeout after 250ms"));
    }

    #[test]
    fn ensure_hooks_preserves_other_entries_and_dedupes() {
        let mut config = json!({
            "mcp": { "servers": { "keep": { "type": "stdio", "command": "keep" } } },
            "hooks": {
                "enabled": true,
                "events": {
                    "PreToolUse": [{
                        "matcher": "Bash",
                        "hooks": [
                            { "type": "process", "command": "echo keep" },
                            {
                                "type": "process",
                                "command": "old",
                                "bitcat_marker": BITCAT_HOOK_MARKER
                            }
                        ]
                    }],
                    "LegacyEvent": [{
                        "hooks": [{
                            "type": "process",
                            "command": "old",
                            "bitcat_marker": BITCAT_HOOK_MARKER
                        }]
                    }]
                }
            }
        });
        let mut report = HookRepairReport::default();

        ensure_zcode_hooks(&mut config, &script_path(), &mut report).unwrap();

        assert_eq!(config["mcp"]["servers"]["keep"]["command"], "keep");
        assert_eq!(config["hooks"]["enabled"], true);
        let pre_tool = config["hooks"]["events"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre_tool.len(), 2);
        assert_eq!(pre_tool[0]["hooks"][0]["command"], "echo keep");
        assert_eq!(pre_tool[1]["hooks"][0]["bitcat_marker"], BITCAT_HOOK_MARKER);
        assert!(config["hooks"]["events"].get("LegacyEvent").is_none());
        for event in HOOK_EVENTS {
            assert!(config["hooks"]["events"].get(event).is_some(), "{event}");
        }
        assert_eq!(report.installed, HOOK_EVENTS.len());
        assert_eq!(report.removed, 2);
    }

    #[test]
    fn repair_is_idempotent() {
        let mut config = json!({});
        let mut first = HookRepairReport::default();
        ensure_zcode_hooks(&mut config, &script_path(), &mut first).unwrap();
        let snapshot = config.clone();

        let mut second = HookRepairReport::default();
        ensure_zcode_hooks(&mut config, &script_path(), &mut second).unwrap();

        assert_eq!(config, snapshot);
        assert_eq!(second.installed, HOOK_EVENTS.len());
        assert_eq!(second.removed, HOOK_EVENTS.len());
    }

    #[test]
    fn respects_explicitly_disabled_hooks() {
        let mut config = json!({ "hooks": { "enabled": false, "events": {} } });
        let before = config.clone();
        let mut report = HookRepairReport::default();

        let error = ensure_zcode_hooks(&mut config, &script_path(), &mut report).unwrap_err();

        assert!(error.contains("已关闭"));
        assert_eq!(config, before);
    }

    #[test]
    fn rejects_malformed_config_shapes() {
        for raw in ["[]", r#"{"hooks":[]}"#, r#"{"hooks":{"events":[]}}"#] {
            let mut config: Value = serde_json::from_str(raw).unwrap();
            let mut report = HookRepairReport::default();
            assert!(
                ensure_zcode_hooks(&mut config, &script_path(), &mut report).is_err(),
                "{raw}"
            );
        }
    }
}
