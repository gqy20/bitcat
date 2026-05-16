//! Claude Code 会话看管线程。
//!
//! 本模块在本机 TCP 端口接收 Claude Code hook 发来的 JSON，转换成 core
//! 的 `AgentSession` 快照，并通过宠物事件总线发出低频提醒。它不直接处理
//! hook 安装，也不向 Claude Code 回写权限决策，第一版保持只读观察。

use ai_pad_core::agent_nudge::{AgentNudge, AgentNudgeDecision, AgentNudgePolicy};
use ai_pad_core::agent_session::{
    apply_session_event, sort_sessions, AgentSession, AgentSessionView,
};
use ai_pad_core::app_settings::AppSettings;
use ai_pad_core::claude_code::ClaudeHookEvent;
use ai_pad_core::pet_event::PetEvent;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tracing::{debug, info, warn};

pub const DEFAULT_AGENT_MONITOR_PORT: u16 = 19283;
const MAX_HOOK_PAYLOAD_BYTES: u64 = 512 * 1024;

/// Claude Code 看管共享状态。
pub struct SharedAgentMonitor {
    sessions: Mutex<HashMap<String, AgentSession>>,
    nudge_policy: Mutex<AgentNudgePolicy>,
}

impl Default for SharedAgentMonitor {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            nudge_policy: Mutex::new(AgentNudgePolicy::new()),
        }
    }
}

/// 前端会话列表快照。
#[derive(Debug, Clone, Serialize)]
pub struct AgentSessionsSnapshot {
    pub sessions: Vec<AgentSessionView>,
    pub primary: Option<AgentSessionView>,
    pub generated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct AgentNudgeLogRecord {
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
        Ok(snapshot_from_sessions(
            sessions.values().cloned().collect(),
            now_ms,
        ))
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
    let event = ClaudeHookEvent::from_json(raw)?.into_session_event(now_ms)?;
    if let Err(e) = append_jsonl("agent_sessions.jsonl", &event) {
        warn!(error = %e, "write agent session event log failed");
    }
    let monitor: tauri::State<SharedAgentMonitor> = app.state();

    let (snapshot, primary_session) = {
        let mut sessions = monitor
            .sessions
            .lock()
            .map_err(|e| format!("agent sessions lock poisoned: {e}"))?;
        apply_session_event(&mut sessions, event);
        let sorted = sort_sessions(sessions.values().cloned().collect());
        let primary = sorted.first().cloned();
        (snapshot_from_sessions(sorted, now_ms), primary)
    };

    let _ = app.emit("agent-session-update", &snapshot);

    if let Some(session) = primary_session {
        evaluate_nudge(app, &monitor, &session, now_ms)?;
    }
    Ok(())
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
            write_nudge_log(AgentNudgeLogRecord {
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

fn snapshot_from_sessions(sessions: Vec<AgentSession>, now_ms: u64) -> AgentSessionsSnapshot {
    let views: Vec<AgentSessionView> = sessions
        .iter()
        .map(|session| AgentSessionView::from_session(session, now_ms))
        .collect();
    AgentSessionsSnapshot {
        primary: views.first().cloned(),
        sessions: views,
        generated_at_ms: now_ms,
    }
}

fn write_nudge_log(record: AgentNudgeLogRecord) {
    if let Err(e) = append_jsonl("agent_nudges.jsonl", &record) {
        warn!(error = %e, "write agent nudge log failed");
    }
}

pub fn append_jsonl<T: Serialize>(file_name: &str, value: &T) -> Result<(), String> {
    let mut dir = home_dir().ok_or_else(|| "无法解析 home 目录".to_string())?;
    dir.push(".ai-pad");
    dir.push("logs");
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
        let snapshot = snapshot_from_sessions(sort_sessions(sessions), 1000);
        assert_eq!(snapshot.primary.unwrap().session_id, "wait");
        assert_eq!(snapshot.sessions[0].status, "waiting");
    }
}
