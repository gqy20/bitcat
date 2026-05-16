//! Claude Code 会话看管线程。
//!
//! 本模块在本机 TCP 端口接收 Claude Code hook 发来的 JSON，转换成 core
//! 的 `AgentSession` 快照，并通过宠物事件总线发出低频提醒。它不直接处理
//! hook 安装，也不向 Claude Code 回写权限决策，第一版保持只读观察。

use ai_pad_core::agent_nudge::{AgentNudge, AgentNudgeDecision, AgentNudgePolicy};
use ai_pad_core::agent_session::{
    apply_session_event, sort_sessions, AgentSession, AgentSessionEvent, AgentSessionView,
    AgentSource,
};
use ai_pad_core::app_settings::AppSettings;
use ai_pad_core::claude_code::ClaudeHookEvent;
use ai_pad_core::pet_event::PetEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tracing::{debug, info, warn};

pub const DEFAULT_AGENT_MONITOR_PORT: u16 = 5342;
const MAX_HOOK_PAYLOAD_BYTES: u64 = 512 * 1024;

/// Claude Code 看管共享状态。
pub struct SharedAgentMonitor {
    sessions: Mutex<HashMap<String, AgentSession>>,
    nudge_policy: Mutex<AgentNudgePolicy>,
    event_count: Mutex<u64>,
    last_event_at_ms: Mutex<Option<u64>>,
}

impl Default for SharedAgentMonitor {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            nudge_policy: Mutex::new(AgentNudgePolicy::new()),
            event_count: Mutex::new(0),
            last_event_at_ms: Mutex::new(None),
        }
    }
}

/// 前端会话列表快照。
#[derive(Debug, Clone, Serialize)]
pub struct AgentSessionsSnapshot {
    pub sessions: Vec<AgentSessionView>,
    pub primary: Option<AgentSessionView>,
    pub generated_at_ms: u64,
    pub monitor_port: u16,
    pub event_count: u64,
    pub last_event_at_ms: Option<u64>,
    pub log_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AgentNudgeLogRecord {
    seq: u64,
    at_ms: u64,
    session_id: String,
    kind: String,
    decision: String,
    status: String,
    reason: Option<String>,
    message: Option<String>,
}

impl SharedAgentMonitor {
    pub fn snapshot(&self, now_ms: u64) -> Result<AgentSessionsSnapshot, String> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|e| format!("agent sessions lock poisoned: {e}"))?;
        let event_count = *self
            .event_count
            .lock()
            .map_err(|e| format!("agent event count lock poisoned: {e}"))?;
        let last_event_at_ms = *self
            .last_event_at_ms
            .lock()
            .map_err(|e| format!("agent last event lock poisoned: {e}"))?;
        Ok(snapshot_from_sessions(
            sessions.values().cloned().collect(),
            now_ms,
            event_count,
            last_event_at_ms,
        ))
    }

    fn next_event_seq(&self, now_ms: u64) -> Result<u64, String> {
        let mut count = self
            .event_count
            .lock()
            .map_err(|e| format!("agent event count lock poisoned: {e}"))?;
        *count = count.saturating_add(1);
        *self
            .last_event_at_ms
            .lock()
            .map_err(|e| format!("agent last event lock poisoned: {e}"))? = Some(now_ms);
        Ok(*count)
    }

    fn remove_session(&self, session_id: &str) -> Result<(), String> {
        self.sessions
            .lock()
            .map_err(|e| format!("agent sessions lock poisoned: {e}"))?
            .remove(session_id);
        Ok(())
    }
}

/// 启动 Claude Code hook TCP 接收线程。
pub fn spawn_agent_monitor(app: AppHandle) {
    std::thread::spawn(move || {
        let addr = format!("127.0.0.1:{DEFAULT_AGENT_MONITOR_PORT}");
        let listener = match TcpListener::bind(&addr) {
            Ok(listener) => listener,
            Err(e) => {
                warn!(error = %e, addr, "Claude Code monitor bind failed");
                return;
            }
        };
        if let Err(e) = listener.set_nonblocking(true) {
            warn!(error = %e, "Claude Code monitor set_nonblocking failed");
        }
        info!(addr, "Claude Code monitor listening");

        while !crate::shutdown::is_requested() {
            match listener.accept() {
                Ok((stream, _)) => handle_hook_stream(&app, stream),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    warn!(error = %e, "Claude Code monitor accept failed");
                    std::thread::sleep(Duration::from_millis(250));
                }
            }
        }
        info!("Claude Code monitor stopped");
    });
}

fn handle_hook_stream(app: &AppHandle, stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut raw = String::new();
    match stream.take(MAX_HOOK_PAYLOAD_BYTES).read_to_string(&mut raw) {
        Ok(_) => {
            if raw.trim().is_empty() {
                debug!("Claude hook payload empty");
                return;
            }
            if let Err(e) = handle_hook_payload(app, &raw) {
                warn!(error = %e, "Claude hook payload handling failed");
            }
        }
        Err(e) => warn!(error = %e, "Claude hook read failed"),
    }
}

pub fn handle_hook_payload(app: &AppHandle, raw: &str) -> Result<(), String> {
    let now_ms = now_ms();
    let event = parse_agent_hook_payload(raw, now_ms)?;
    let monitor: tauri::State<SharedAgentMonitor> = app.state();
    let seq = monitor.next_event_seq(now_ms)?;
    if let Err(e) = append_jsonl(
        "agent_watch_events.jsonl",
        &AgentWatchEventLogRecord::from_event(seq, &event),
    ) {
        warn!(error = %e, "write agent watch event log failed");
    }

    let (snapshot, updated_session) = {
        let updated_session_id = event.session_id.clone();
        let mut sessions = monitor
            .sessions
            .lock()
            .map_err(|e| format!("agent sessions lock poisoned: {e}"))?;
        apply_session_event(&mut sessions, event);
        let updated = sessions.get(&updated_session_id).cloned();
        let sorted = sort_sessions(sessions.values().cloned().collect());
        (
            snapshot_from_sessions(sorted, now_ms, seq, Some(now_ms)),
            updated,
        )
    };

    let _ = app.emit("agent-session-update", &snapshot);
    crate::agent_watch_window::show_snapshot(app, &snapshot);
    if let Err(e) = append_jsonl("agent_watch_sessions.jsonl", &snapshot) {
        warn!(error = %e, "write agent watch session snapshot failed");
    }

    if let Some(session) = updated_session {
        evaluate_nudge(app, &monitor, &session, now_ms)?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct AgentHookEnvelope {
    source: String,
    payload: Value,
}

fn parse_agent_hook_payload(raw: &str, now_ms: u64) -> Result<AgentSessionEvent, String> {
    if let Ok(envelope) = serde_json::from_str::<AgentHookEnvelope>(raw) {
        let source = match envelope.source.trim().to_ascii_lowercase().as_str() {
            "codex" => AgentSource::Codex,
            "claude" | "claude-code" | "claude_code" => AgentSource::ClaudeCode,
            other => return Err(format!("unknown agent hook source: {other}")),
        };
        let payload = serde_json::to_string(&envelope.payload)
            .map_err(|e| format!("agent hook envelope payload serialize failed: {e}"))?;
        return ClaudeHookEvent::from_json(&payload)?.into_session_event_from(source, now_ms);
    }

    ClaudeHookEvent::from_json(raw)?.into_session_event(now_ms)
}

fn evaluate_nudge(
    app: &AppHandle,
    monitor: &SharedAgentMonitor,
    session: &AgentSession,
    now_ms: u64,
) -> Result<(), String> {
    let settings = AppSettings::load().agent_watch;
    let decision = {
        let mut policy = monitor
            .nudge_policy
            .lock()
            .map_err(|e| format!("agent nudge lock poisoned: {e}"))?;
        policy.evaluate(session, &settings, now_ms)
    };

    match decision {
        AgentNudgeDecision::Send(nudge) => {
            if low_priority_nudge_is_gated(app, &nudge) {
                write_nudge_log(AgentNudgeLogRecord {
                    seq: now_ms,
                    at_ms: now_ms,
                    session_id: session.session_id.clone(),
                    kind: nudge.kind.as_str().to_string(),
                    decision: "gated".to_string(),
                    status: session.status.as_str().to_string(),
                    reason: Some("ui_gate".to_string()),
                    message: Some(nudge.message),
                });
                return Ok(());
            }
            emit_nudge(app, &nudge);
            monitor
                .nudge_policy
                .lock()
                .map_err(|e| format!("agent nudge lock poisoned: {e}"))?
                .mark_sent(&nudge, now_ms);
            write_nudge_log(AgentNudgeLogRecord {
                seq: now_ms,
                at_ms: now_ms,
                session_id: session.session_id.clone(),
                kind: nudge.kind.as_str().to_string(),
                decision: "sent".to_string(),
                status: session.status.as_str().to_string(),
                reason: None,
                message: Some(nudge.message),
            });
        }
        AgentNudgeDecision::Skip { reason, status } => {
            write_nudge_log(AgentNudgeLogRecord {
                seq: now_ms,
                at_ms: now_ms,
                session_id: session.session_id.clone(),
                kind: "none".to_string(),
                decision: "skipped".to_string(),
                status: status.as_str().to_string(),
                reason: Some(reason.as_str().to_string()),
                message: None,
            });
        }
    }
    Ok(())
}

fn low_priority_nudge_is_gated(app: &AppHandle, nudge: &AgentNudge) -> bool {
    if nudge.kind != ai_pad_core::agent_nudge::AgentNudgeKind::AwayWhileWorking {
        return false;
    }
    if ai_pad_core::performance::blocks_screenshot_observation() {
        return true;
    }
    if crate::game::is_game_busy(app) {
        return true;
    }
    let bubble: tauri::State<crate::bubble::SharedBubble> = app.state();
    if bubble.is_chat_active() {
        return true;
    }
    let gate: tauri::State<crate::observation_gate::SharedObservationGate> = app.state();
    gate.skip_reason().is_some()
}

fn emit_nudge(app: &AppHandle, nudge: &AgentNudge) {
    let bus: tauri::State<crate::pet_event_bus::SharedPetEventBus> = app.state();
    bus.emit(
        app,
        PetEvent::React {
            mood: nudge.mood,
            speech: Some(nudge.message.clone()),
            ttl_ms: Some(nudge.ttl_ms),
        },
    );
    if let Err(e) = crate::bubble::show_bubble(app, &nudge.message) {
        warn!(error = %e, "agent nudge bubble failed");
    }
    if nudge.use_tts {
        let message = nudge.message.clone();
        std::thread::spawn(move || crate::tts::speak(&message));
    }
}

#[derive(Debug, Clone, Serialize)]
struct AgentWatchEventLogRecord {
    seq: u64,
    at_ms: u64,
    session_id: String,
    source: String,
    workspace: String,
    status: String,
    tool_name: Option<String>,
    needs_user: bool,
    user_prompt_preview: Option<String>,
    tool_input_preview: Option<String>,
}

impl AgentWatchEventLogRecord {
    fn from_event(seq: u64, event: &ai_pad_core::agent_session::AgentSessionEvent) -> Self {
        Self {
            seq,
            at_ms: event.at_ms,
            session_id: event.session_id.clone(),
            source: event.source.as_str().to_string(),
            workspace: event.workspace.clone(),
            status: event.status.as_str().to_string(),
            tool_name: event.tool_name.clone(),
            needs_user: event.needs_user,
            user_prompt_preview: event.user_prompt_preview.clone(),
            tool_input_preview: event.tool_input_preview.clone(),
        }
    }
}

fn snapshot_from_sessions(
    sessions: Vec<AgentSession>,
    now_ms: u64,
    event_count: u64,
    last_event_at_ms: Option<u64>,
) -> AgentSessionsSnapshot {
    let views: Vec<AgentSessionView> = sessions
        .iter()
        .map(|session| AgentSessionView::from_session(session, now_ms))
        .collect();
    AgentSessionsSnapshot {
        primary: views.first().cloned(),
        sessions: views,
        generated_at_ms: now_ms,
        monitor_port: DEFAULT_AGENT_MONITOR_PORT,
        event_count,
        last_event_at_ms,
        log_dir: log_dir().map(|path| path.to_string_lossy().to_string()),
    }
}

fn write_nudge_log(record: AgentNudgeLogRecord) {
    if let Err(e) = append_jsonl("agent_watch_nudges.jsonl", &record) {
        warn!(error = %e, "write agent nudge log failed");
    }
}

pub fn append_jsonl<T: Serialize>(file_name: &str, value: &T) -> Result<(), String> {
    let dir = log_dir().ok_or_else(|| "无法解析 home 目录".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建日志目录失败: {e}"))?;
    let path = dir.join(file_name);
    let line = serde_json::to_string(value).map_err(|e| e.to_string())?;
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("打开日志失败: {e}"))?;
    writeln!(file, "{line}").map_err(|e| format!("写入日志失败: {e}"))
}

pub fn log_dir() -> Option<PathBuf> {
    home_dir().map(|dir| dir.join(".ai-pad").join("logs"))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[tauri::command]
pub async fn cmd_get_agent_sessions(
    monitor: tauri::State<'_, SharedAgentMonitor>,
) -> Result<AgentSessionsSnapshot, String> {
    monitor.snapshot(now_ms())
}

#[tauri::command]
pub async fn cmd_dismiss_agent_session(
    app: AppHandle,
    monitor: tauri::State<'_, SharedAgentMonitor>,
    session_id: String,
) -> Result<AgentSessionsSnapshot, String> {
    monitor.remove_session(&session_id)?;
    let snapshot = monitor.snapshot(now_ms())?;
    let _ = app.emit("agent-session-update", &snapshot);
    crate::agent_watch_window::show_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn cmd_open_agent_workspace(
    app: AppHandle,
    monitor: tauri::State<'_, SharedAgentMonitor>,
    session_id: String,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let sessions = monitor
        .sessions
        .lock()
        .map_err(|e| format!("agent sessions lock poisoned: {e}"))?;
    let Some(session) = sessions.get(&session_id) else {
        return Err("会话不存在".into());
    };
    if session.workspace.trim().is_empty() {
        return Err("会话没有工作目录".into());
    }
    app.opener()
        .open_path(session.workspace.clone(), None::<String>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_pad_core::agent_session::{AgentSource, AgentStatus};

    #[test]
    fn snapshot_sorts_and_marks_primary() {
        let sessions = vec![
            AgentSession {
                session_id: "work".into(),
                source: AgentSource::ClaudeCode,
                workspace: "D:\\repo\\work".into(),
                status: AgentStatus::Working,
                tool_name: None,
                tool_input_preview: None,
                user_prompt_preview: None,
                last_response_preview: None,
                pid: None,
                updated_at_ms: 10,
                status_changed_at_ms: 10,
                needs_user: false,
            },
            AgentSession {
                session_id: "wait".into(),
                source: AgentSource::ClaudeCode,
                workspace: "D:\\repo\\wait".into(),
                status: AgentStatus::Waiting,
                tool_name: None,
                tool_input_preview: None,
                user_prompt_preview: None,
                last_response_preview: None,
                pid: None,
                updated_at_ms: 5,
                status_changed_at_ms: 5,
                needs_user: true,
            },
        ];
        let snapshot = snapshot_from_sessions(sort_sessions(sessions), 1000, 2, Some(10));
        assert_eq!(snapshot.primary.unwrap().session_id, "wait");
        assert_eq!(snapshot.sessions[0].status, "waiting");
        assert_eq!(snapshot.monitor_port, DEFAULT_AGENT_MONITOR_PORT);
        assert_eq!(snapshot.event_count, 2);
    }

    #[test]
    fn parses_codex_hook_envelope() {
        let raw = r#"{
            "source": "codex",
            "payload": {
                "session_id": "codex-session",
                "hook_event_name": "PreToolUse",
                "cwd": "D:\\repo",
                "tool_name": "Bash",
                "tool_input": {"command": "cargo test"}
            }
        }"#;
        let event = parse_agent_hook_payload(raw, 42).unwrap();
        assert_eq!(event.session_id, "codex-session");
        assert_eq!(event.source, AgentSource::Codex);
        assert_eq!(event.status, AgentStatus::ToolRunning);
        assert_eq!(event.tool_name.as_deref(), Some("Bash"));
        assert!(event.tool_input_preview.unwrap().contains("cargo test"));
    }
}
