//! 工具运行时事件的轻量审计日志。
//!
//! 这里记录 Agent 工具调用的稳定字段到 append-only JSONL，便于之后统计成功率、
//! 拦截次数和耗时，而不把完整参数、文件内容或命令输出写入普通日志。
//! `agent.rs` 在工具生命周期事件落定时调用本模块，设置页或诊断工具可后续读取。

use crate::agent::{ToolKind, ToolPhase, ToolRuntimeEvent};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tracing::warn;

static TOOL_EVENT_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// 写入 `~/.ai-pad/logs/tool_events.jsonl` 的工具事件记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolEventRecord {
    pub timestamp: String,
    pub session_id: String,
    pub tool_name: String,
    pub label: String,
    pub kind: ToolKind,
    pub phase: ToolPhase,
    pub success: Option<bool>,
    pub blocked: bool,
    pub elapsed_ms: Option<u64>,
    pub call_id: Option<String>,
    pub internal_call_id: String,
    pub result_preview: Option<String>,
}

impl ToolEventRecord {
    /// 从 UI 同源的运行时事件生成审计记录。
    pub fn from_event(session_id: impl Into<String>, event: &ToolRuntimeEvent) -> Self {
        Self {
            timestamp: chrono::Local::now().to_rfc3339(),
            session_id: session_id.into(),
            tool_name: event.tool_name.clone(),
            label: event.label.clone(),
            kind: event.kind.clone(),
            phase: event.phase.clone(),
            success: event.success,
            blocked: event.phase == ToolPhase::Blocked,
            elapsed_ms: event.elapsed_ms,
            call_id: event.call_id.clone(),
            internal_call_id: event.internal_call_id.clone(),
            result_preview: event.result_preview.clone(),
        }
    }
}

/// 返回工具事件 JSONL 路径 `~/.ai-pad/logs/tool_events.jsonl`。
pub fn tool_events_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取 HOME 目录".to_string())?;
    Ok(home.join(".ai-pad").join("logs").join("tool_events.jsonl"))
}

/// 记录一条工具事件。失败只写 warn，不影响主对话。
pub fn record_tool_event(record: &ToolEventRecord) {
    let path = match tool_events_path() {
        Ok(path) => path,
        Err(e) => {
            warn!(error = %e, "tool event path unavailable");
            return;
        }
    };

    if let Err(e) = append_tool_event(&path, record) {
        warn!(error = %e, path = ?path, "tool event write failed");
    }
}

/// 向 JSONL 文件追加一条工具事件记录。
pub fn append_tool_event(path: &Path, record: &ToolEventRecord) -> Result<(), String> {
    let _guard = TOOL_EVENT_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|e| format!("tool event 写入锁中毒: {e}"))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建工具事件日志目录失败: {e}"))?;
    }

    let line = serde_json::to_string(record).map_err(|e| format!("序列化工具事件失败: {e}"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("打开工具事件日志失败: {e}"))?;
    writeln!(file, "{line}").map_err(|e| format!("写入工具事件日志失败: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> ToolRuntimeEvent {
        ToolRuntimeEvent {
            tool_name: "shell".into(),
            label: "执行命令".into(),
            kind: ToolKind::System,
            phase: ToolPhase::Blocked,
            call_id: Some("provider-call".into()),
            internal_call_id: "rig-call".into(),
            result_preview: Some("blocked".into()),
            success: Some(false),
            elapsed_ms: Some(12),
        }
    }

    #[test]
    fn tool_event_record_snapshot() {
        let mut record = ToolEventRecord::from_event("session-1", &sample_event());
        record.timestamp = "2026-05-13T12:00:00+08:00".into();
        insta::assert_yaml_snapshot!(record);
    }

    #[test]
    fn append_tool_event_writes_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tool_events.jsonl");
        let mut record = ToolEventRecord::from_event("session-1", &sample_event());
        record.timestamp = "2026-05-13T12:00:00+08:00".into();

        append_tool_event(&path, &record).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        let line: ToolEventRecord = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(line.session_id, "session-1");
        assert_eq!(line.phase, ToolPhase::Blocked);
        assert!(line.blocked);
    }
}
