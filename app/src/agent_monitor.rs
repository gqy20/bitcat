//! Claude Code 会话看管线程。
//!
//! 本模块在本机 TCP 端口接收 Claude Code hook 发来的 JSON，转换成 core
//! 的 `AgentSession` 快照，并通过宠物事件总线发出低频提醒。它不直接处理
//! hook 安装，也不向 Claude Code 回写权限决策，第一版保持只读观察。

use ai_pad_core::agent_nudge::{AgentNudge, AgentNudgeDecision, AgentNudgeKind, AgentNudgePolicy};
use ai_pad_core::agent_session::{
    apply_session_event, sort_sessions, AgentSession, AgentSessionEvent, AgentSessionView,
    AgentSource,
};
use ai_pad_core::app_settings::{AgentWatchSettings, AppSettings};
use ai_pad_core::claude_code::ClaudeHookEvent;
use ai_pad_core::pet_event::PetEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tracing::{debug, info, warn};

use crate::remote_endpoint::RemoteInstallInfo;

pub const DEFAULT_AGENT_MONITOR_PORT: u16 = 5342;
pub const DEFAULT_AGENT_VIEW_PORT: u16 = 5344;
const MAX_HOOK_PAYLOAD_BYTES: u64 = 512 * 1024;

/// Claude Code 看管共享状态。
pub struct SharedAgentMonitor {
    sessions: Mutex<HashMap<String, AgentSession>>,
    nudge_policy: Mutex<AgentNudgePolicy>,
    event_count: Mutex<u64>,
    last_event_at_ms: Mutex<Option<u64>>,
    recent_events: Mutex<RecentAgentEvents>,
}

impl Default for SharedAgentMonitor {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            nudge_policy: Mutex::new(AgentNudgePolicy::new()),
            event_count: Mutex::new(0),
            last_event_at_ms: Mutex::new(None),
            recent_events: Mutex::new(RecentAgentEvents::default()),
        }
    }
}

#[derive(Default)]
struct RecentAgentEvents {
    order: VecDeque<(String, u64)>,
    seen: HashSet<String>,
}

impl RecentAgentEvents {
    fn should_accept(&mut self, fingerprint: String, now_ms: u64) -> bool {
        const WINDOW_MS: u64 = 1_500;
        const MAX_RECENT: usize = 256;

        while let Some((old, at_ms)) = self.order.front() {
            if now_ms.saturating_sub(*at_ms) <= WINDOW_MS && self.order.len() <= MAX_RECENT {
                break;
            }
            let old = old.clone();
            self.order.pop_front();
            self.seen.remove(&old);
        }

        if self.seen.contains(&fingerprint) {
            return false;
        }
        self.seen.insert(fingerprint.clone());
        self.order.push_back((fingerprint, now_ms));
        true
    }
}

/// 前端会话列表快照。
#[derive(Debug, Clone, Serialize)]
pub struct AgentSessionsSnapshot {
    pub sessions: Vec<AgentSessionView>,
    pub primary: Option<AgentSessionView>,
    pub generated_at_ms: u64,
    pub monitor_port: u16,
    pub view_port: u16,
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

#[derive(Debug, Clone, Serialize)]
pub struct DeviceSummary {
    pub machine: String,
    pub session_count: usize,
    pub active_count: usize,
    pub last_updated_at_ms: u64,
    pub stale: bool,
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
        let addr = format!("0.0.0.0:{DEFAULT_AGENT_MONITOR_PORT}");
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

pub fn spawn_agent_view_server(app: AppHandle) {
    std::thread::spawn(move || {
        let addr = format!("0.0.0.0:{DEFAULT_AGENT_VIEW_PORT}");
        let listener = match TcpListener::bind(&addr) {
            Ok(listener) => listener,
            Err(e) => {
                warn!(error = %e, addr, "Agent Watch view server bind failed");
                return;
            }
        };
        if let Err(e) = listener.set_nonblocking(true) {
            warn!(error = %e, "Agent Watch view server set_nonblocking failed");
        }
        info!(addr, "Agent Watch view server listening");

        while !crate::shutdown::is_requested() {
            match listener.accept() {
                Ok((stream, _)) => handle_view_stream(&app, stream),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    warn!(error = %e, "Agent Watch view server accept failed");
                    std::thread::sleep(Duration::from_millis(250));
                }
            }
        }
        info!("Agent Watch view server stopped");
    });
}

fn handle_hook_stream(app: &AppHandle, stream: TcpStream) {
    if let Err(e) = stream.set_nonblocking(false) {
        debug!(error = %e, "Claude hook stream set blocking failed");
    }
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
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            debug!(error = %e, "Claude hook stream had no payload ready");
        }
        Err(e) => warn!(error = %e, "Claude hook read failed"),
    }
}

fn handle_view_stream(app: &AppHandle, mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buffer = [0u8; 2048];
    let read = match stream.read(&mut buffer) {
        Ok(read) => read,
        Err(e) => {
            debug!(error = %e, "Agent Watch view request read failed");
            return;
        }
    };
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let (status, content_type, body) = view_response(app, path);
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}; charset=utf-8\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    if let Err(e) = stream.write_all(response.as_bytes()) {
        debug!(error = %e, "Agent Watch view response write failed");
    }
}

fn view_response(app: &AppHandle, path: &str) -> (&'static str, &'static str, String) {
    let clean_path = path.split('?').next().unwrap_or(path);
    let settings = AppSettings::load().agent_watch;
    if let Some(response) = remote_access_forbidden(clean_path, &settings) {
        return response;
    }
    match clean_path {
        "/" | "/watch" => ("200 OK", "text/html", watch_page_html()),
        "/agent-sessions" => {
            let monitor: tauri::State<SharedAgentMonitor> = app.state();
            match monitor
                .snapshot(now_ms())
                .and_then(|snapshot| serde_json::to_string(&snapshot).map_err(|e| e.to_string()))
            {
                Ok(body) => ("200 OK", "application/json", body),
                Err(e) => (
                    "500 Internal Server Error",
                    "application/json",
                    json_error(&e),
                ),
            }
        }
        "/devices" => {
            let monitor: tauri::State<SharedAgentMonitor> = app.state();
            match remote_devices(&monitor, now_ms())
                .and_then(|devices| serde_json::to_string(&devices).map_err(|e| e.to_string()))
            {
                Ok(body) => ("200 OK", "application/json", body),
                Err(e) => (
                    "500 Internal Server Error",
                    "application/json",
                    json_error(&e),
                ),
            }
        }
        "/remote-install.sh" => (
            "200 OK",
            "text/x-shellscript",
            include_str!("../../scripts/remote-install.sh").to_string(),
        ),
        "/health" => ("200 OK", "application/json", "{\"ok\":true}".to_string()),
        _ => (
            "404 Not Found",
            "application/json",
            "{\"error\":\"not found\"}".to_string(),
        ),
    }
}

fn remote_access_forbidden(
    clean_path: &str,
    settings: &AgentWatchSettings,
) -> Option<(&'static str, &'static str, String)> {
    let is_view_path = matches!(clean_path, "/" | "/watch" | "/agent-sessions" | "/devices");
    if is_view_path && !settings.remote_view_enabled {
        return Some((
            "403 Forbidden",
            "application/json",
            json_error("remote Agent Watch view is disabled"),
        ));
    }
    if clean_path == "/remote-install.sh" && !settings.remote_install_enabled {
        return Some((
            "403 Forbidden",
            "application/json",
            json_error("remote Agent Watch installer is disabled"),
        ));
    }
    None
}

fn json_error(error: &str) -> String {
    serde_json::json!({ "error": error }).to_string()
}

fn watch_page_html() -> String {
    r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Agent Watch</title>
  <style>
    :root { color-scheme: dark; font-family: "Segoe UI", system-ui, sans-serif; background: #0f1115; color: rgba(255,255,255,.92); }
    body { margin: 0; padding: 18px; }
    header { display: flex; justify-content: space-between; align-items: center; gap: 12px; margin-bottom: 16px; }
    h1 { margin: 0; font-size: 22px; }
    #status { color: rgba(255,255,255,.55); font-size: 13px; }
    .stack { display: grid; gap: 10px; }
    .card { border: 1px solid rgba(255,255,255,.12); border-radius: 8px; padding: 12px 14px; background: rgba(255,255,255,.045); }
    .top { display: flex; justify-content: space-between; gap: 12px; align-items: baseline; }
    .title { font-weight: 760; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .age { color: rgba(255,255,255,.48); font-size: 12px; }
    .meta { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; color: rgba(255,255,255,.62); font-size: 13px; }
    .meta span { border: 1px solid rgba(255,255,255,.08); border-radius: 999px; padding: 2px 8px; }
    .meta .device { border-color: rgba(126,165,232,.28); background: rgba(126,165,232,.12); color: rgba(184,211,255,.95); font-weight: 760; }
    .done { border-color: rgba(142,230,168,.32); }
    .waiting, .error { border-color: rgba(255,138,122,.42); }
    p { margin: 8px 0 0; color: rgba(255,255,255,.76); line-height: 1.45; }
  </style>
</head>
<body>
  <header>
    <h1>Agent Watch</h1>
    <div id="status">loading</div>
  </header>
  <main id="stack" class="stack"></main>
  <script>
    const stack = document.getElementById('stack');
    const status = document.getElementById('status');
    function esc(v) { return String(v ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }
    async function refresh() {
      try {
        const snap = await fetch('/agent-sessions', { cache: 'no-store' }).then(r => r.json());
        status.textContent = `${snap.sessions?.length || 0} sessions`;
        stack.innerHTML = (snap.sessions || []).map(s => {
          const meta = [];
          if (s.machine) meta.push(s.machine);
          meta.push(s.workspace_name || 'unknown');
          meta.push(s.display?.source_label || s.source);
          meta.push(s.display?.action_label || 'Task');
          return `
          <article class="card ${esc(s.status)}">
            <div class="top"><div class="title">${esc(s.display?.headline || s.status_label || s.status)}</div><div class="age">${esc(s.display?.age_label || '')}</div></div>
            <div class="meta">${meta.map((item, index) => `<span class="${index === 0 && s.machine ? 'device' : ''}">${esc(item)}</span>`).join('')}</div>
            ${s.display?.detail ? `<p>${esc(s.display.detail)}</p>` : ''}
          </article>
        `}).join('') || '<p>No sessions yet.</p>';
      } catch (e) {
        status.textContent = 'offline';
      }
    }
    refresh();
    setInterval(refresh, 2000);
  </script>
</body>
</html>"#
        .to_string()
}

pub fn handle_hook_payload(app: &AppHandle, raw: &str) -> Result<(), String> {
    let now_ms = now_ms();
    let event = parse_agent_hook_payload(raw, now_ms)?;
    let monitor: tauri::State<SharedAgentMonitor> = app.state();
    let fingerprint = agent_event_fingerprint(&event);
    {
        let mut recent = monitor
            .recent_events
            .lock()
            .map_err(|e| format!("agent recent events lock poisoned: {e}"))?;
        if !recent.should_accept(fingerprint.clone(), now_ms) {
            debug!(session_id = %event.session_id, status = %event.status.as_str(), "duplicate agent hook ignored");
            return Ok(());
        }
    }
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
    #[serde(default)]
    schema: Option<String>,
    source: String,
    #[serde(default)]
    machine: Option<String>,
    payload: Value,
}

fn agent_event_fingerprint(event: &AgentSessionEvent) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    event.source.hash(&mut hasher);
    event.session_id.hash(&mut hasher);
    event.status.hash(&mut hasher);
    event.tool_name.hash(&mut hasher);
    event.tool_input_preview.hash(&mut hasher);
    event.user_prompt_preview.hash(&mut hasher);
    event.last_response_preview.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn parse_agent_hook_payload(raw: &str, now_ms: u64) -> Result<AgentSessionEvent, String> {
    if let Ok(envelope) = serde_json::from_str::<AgentHookEnvelope>(raw) {
        let _schema = envelope.schema.as_deref();
        let source = match envelope.source.trim().to_ascii_lowercase().as_str() {
            "codex" => AgentSource::Codex,
            "claude" | "claude-code" | "claude_code" => AgentSource::ClaudeCode,
            other => return Err(format!("unknown agent hook source: {other}")),
        };
        let payload = serde_json::to_string(&envelope.payload)
            .map_err(|e| format!("agent hook envelope payload serialize failed: {e}"))?;
        return ClaudeHookEvent::from_json(&payload)?.into_session_event_from(
            source,
            now_ms,
            envelope.machine,
        );
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
            emit_nudge(app, &nudge, session);
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

fn emit_nudge(app: &AppHandle, nudge: &AgentNudge, session: &AgentSession) {
    let toast = agent_toast_payload(nudge, session);
    let bus: tauri::State<crate::pet_event_bus::SharedPetEventBus> = app.state();
    bus.emit(
        app,
        PetEvent::React {
            mood: nudge.mood,
            speech: Some(toast.title.clone()),
            ttl_ms: Some(nudge.ttl_ms),
        },
    );
    if nudge_uses_notification(nudge.kind, session) {
        let notification = agent_notification_payload(nudge, session, &toast);
        if let Err(e) = crate::notification_window::show_notification(app, notification) {
            warn!(error = %e, "agent notification failed");
            if let Err(fallback) = crate::bubble::show_agent_toast(app, toast) {
                warn!(error = %fallback, "agent toast fallback failed");
            }
        }
    }
    if nudge.use_tts {
        let message = agent_toast_payload(nudge, session).title;
        std::thread::spawn(move || crate::tts::speak(&message));
    }
}

fn nudge_uses_notification(kind: AgentNudgeKind, session: &AgentSession) -> bool {
    match kind {
        AgentNudgeKind::WaitingForUser | AgentNudgeKind::TaskDone => true,
        AgentNudgeKind::TaskError => !is_tool_level_error(session),
        AgentNudgeKind::AwayWhileWorking => false,
    }
}

fn is_tool_level_error(session: &AgentSession) -> bool {
    session.status == ai_pad_core::agent_session::AgentStatus::Error
        && session
            .tool_name
            .as_deref()
            .map(str::trim)
            .is_some_and(|tool| !tool.is_empty())
}

fn agent_notification_payload(
    nudge: &AgentNudge,
    session: &AgentSession,
    toast: &crate::bubble::AgentToastPayload,
) -> crate::notification_window::NotificationPayload {
    let title = agent_notification_title(nudge, session);
    let body = agent_notification_body(&title, toast);
    crate::notification_window::NotificationPayload {
        id: format!(
            "agent-watch-{}-{}",
            session.session_id, session.status_changed_at_ms
        ),
        title,
        body,
        tone: agent_notification_tone(nudge.kind).to_string(),
        source: "agent_watch".to_string(),
        reminder_id: None,
        ttl_ms: nudge.ttl_ms,
        actions: Vec::new(),
    }
}

fn agent_notification_title(nudge: &AgentNudge, session: &AgentSession) -> String {
    let view = AgentSessionView::from_session(session, now_ms());
    let project = view.display.project.trim();
    let source = view.display.source_label.trim();
    let action = view.display.action_label.trim();
    let status = match nudge.kind {
        AgentNudgeKind::WaitingForUser => "需要查看",
        AgentNudgeKind::TaskDone => "已完成",
        AgentNudgeKind::TaskError => "任务出错",
        AgentNudgeKind::AwayWhileWorking => "仍在运行",
    };
    let status = if action.is_empty() || action == "Task" {
        status.to_string()
    } else {
        format!("{action} {status}")
    };
    [
        if project.is_empty() { "Agent" } else { project },
        source,
        status.as_str(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ")
}

fn agent_notification_body(
    title: &str,
    toast: &crate::bubble::AgentToastPayload,
) -> Option<String> {
    let detail = toast.detail.trim();
    if detail.is_empty() || title.contains(detail) || status_detail_is_redundant(detail) {
        None
    } else {
        Some(detail.to_string())
    }
}

fn status_detail_is_redundant(detail: &str) -> bool {
    matches!(
        detail,
        "任务已完成" | "任务需要查看" | "仍在运行" | "需要确认下一步"
    ) || detail.ends_with(" 已完成")
        || detail.ends_with(" 仍在运行")
        || detail.ends_with(" 需要确认")
}

fn agent_notification_tone(kind: AgentNudgeKind) -> &'static str {
    match kind {
        AgentNudgeKind::WaitingForUser => "warning",
        AgentNudgeKind::TaskDone => "success",
        AgentNudgeKind::TaskError => "danger",
        AgentNudgeKind::AwayWhileWorking => "info",
    }
}

fn agent_toast_payload(
    nudge: &AgentNudge,
    session: &AgentSession,
) -> crate::bubble::AgentToastPayload {
    let view = AgentSessionView::from_session(session, now_ms());
    let project = view.display.project.trim();
    let source = view.display.source_label.trim();
    let context = [project, source]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let action = view.display.action_label.trim();
    let detail = match nudge.kind {
        AgentNudgeKind::WaitingForUser => {
            if action.is_empty() || action == "Task" {
                "需要确认下一步".to_string()
            } else {
                format!("{action} 需要确认")
            }
        }
        AgentNudgeKind::TaskDone => {
            if action.is_empty() || action == "Task" {
                "任务已完成".to_string()
            } else {
                format!("{action} 已完成")
            }
        }
        AgentNudgeKind::TaskError => "任务需要查看".to_string(),
        AgentNudgeKind::AwayWhileWorking => {
            if action.is_empty() || action == "Task" {
                "仍在运行".to_string()
            } else {
                format!("{action} 仍在运行")
            }
        }
    };
    let title = match nudge.kind {
        AgentNudgeKind::WaitingForUser => format!(
            "{} 正在等你",
            if project.is_empty() { "Agent" } else { project }
        ),
        AgentNudgeKind::TaskDone => format!(
            "{} 已完成",
            if project.is_empty() { "Agent" } else { project }
        ),
        AgentNudgeKind::TaskError => format!(
            "{} 需要查看",
            if project.is_empty() { "Agent" } else { project }
        ),
        AgentNudgeKind::AwayWhileWorking => format!(
            "{} 仍在运行",
            if project.is_empty() { "Agent" } else { project }
        ),
    };
    crate::bubble::AgentToastPayload {
        title,
        context,
        detail,
        tone: nudge.kind.as_str().to_string(),
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
        view_port: DEFAULT_AGENT_VIEW_PORT,
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
    ai_pad_core::logging::append_jsonl(file_name, value).map(|_| ())
}

pub fn log_dir() -> Option<PathBuf> {
    ai_pad_core::logging::log_dir().ok()
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
pub async fn cmd_get_remote_install_cmd() -> Result<RemoteInstallInfo, String> {
    if !AppSettings::load().agent_watch.remote_install_enabled {
        return Err("remote Agent Watch installer is disabled in settings".to_string());
    }
    crate::remote_endpoint::remote_install_info()
}

#[tauri::command]
pub async fn cmd_list_remote_devices(
    monitor: tauri::State<'_, SharedAgentMonitor>,
) -> Result<Vec<DeviceSummary>, String> {
    remote_devices(&monitor, now_ms())
}

fn remote_devices(monitor: &SharedAgentMonitor, now: u64) -> Result<Vec<DeviceSummary>, String> {
    let sessions = monitor
        .sessions
        .lock()
        .map_err(|e| format!("agent sessions lock poisoned: {e}"))?;
    let mut devices: HashMap<String, DeviceSummary> = HashMap::new();
    for session in sessions.values() {
        let Some(machine) = session
            .machine
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
        let entry = devices
            .entry(machine.to_string())
            .or_insert_with(|| DeviceSummary {
                machine: machine.to_string(),
                session_count: 0,
                active_count: 0,
                last_updated_at_ms: 0,
                stale: false,
            });
        entry.session_count += 1;
        if session.is_active() {
            entry.active_count += 1;
        }
        entry.last_updated_at_ms = entry.last_updated_at_ms.max(session.updated_at_ms);
    }
    let mut devices = devices.into_values().collect::<Vec<_>>();
    for device in &mut devices {
        device.stale = now.saturating_sub(device.last_updated_at_ms) > 5 * 60 * 1000;
    }
    devices.sort_by(|left, right| left.machine.cmp(&right.machine));
    Ok(devices)
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
                parent_session_id: None,
                status: AgentStatus::Working,
                tool_name: None,
                tool_input_preview: None,
                user_prompt_preview: None,
                last_response_preview: None,
                background: false,
                agent_id: None,
                agent_type: None,
                task_id: None,
                output_file: None,
                pid: None,
                machine: None,
                updated_at_ms: 10,
                status_changed_at_ms: 10,
                needs_user: false,
            },
            AgentSession {
                session_id: "wait".into(),
                source: AgentSource::ClaudeCode,
                workspace: "D:\\repo\\wait".into(),
                parent_session_id: None,
                status: AgentStatus::Waiting,
                tool_name: None,
                tool_input_preview: None,
                user_prompt_preview: None,
                last_response_preview: None,
                background: false,
                agent_id: None,
                agent_type: None,
                task_id: None,
                output_file: None,
                pid: None,
                machine: None,
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
            "machine": "macbook-pro",
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
        assert_eq!(event.machine.as_deref(), Some("macbook-pro"));
        assert_eq!(event.status, AgentStatus::ToolRunning);
        assert_eq!(event.tool_name.as_deref(), Some("Bash"));
        assert!(event.tool_input_preview.unwrap().contains("cargo test"));
    }

    #[test]
    fn recent_agent_events_deduplicates_short_window() {
        let mut recent = RecentAgentEvents::default();
        assert!(recent.should_accept("same".into(), 1_000));
        assert!(!recent.should_accept("same".into(), 1_500));
        assert!(recent.should_accept("same".into(), 3_000));
    }

    #[test]
    fn agent_toast_keeps_nudge_copy_short() {
        let session = AgentSession {
            session_id: "wait".into(),
            source: AgentSource::ClaudeCode,
            workspace: "/mnt/chestnut/chestnut/01_downstrm/01_population/data".into(),
            parent_session_id: None,
            status: AgentStatus::Waiting,
            tool_name: Some("Shell".into()),
            tool_input_preview: Some(
                "Monitor event: br91auk2l with lots of raw debug detail".into(),
            ),
            user_prompt_preview: Some("br91auk2l".into()),
            last_response_preview: None,
            background: false,
            agent_id: None,
            agent_type: None,
            task_id: None,
            output_file: None,
            pid: None,
            machine: Some("qy113".into()),
            updated_at_ms: 10,
            status_changed_at_ms: 10,
            needs_user: true,
        };
        let nudge = AgentNudge {
            session_id: "wait".into(),
            kind: AgentNudgeKind::WaitingForUser,
            message: "long legacy message".into(),
            mood: ai_pad_core::pet_event::PetMood::Confused,
            ttl_ms: 12_000,
            use_tts: false,
        };

        let toast = agent_toast_payload(&nudge, &session);

        assert_eq!(toast.title, "data 正在等你");
        assert_eq!(toast.context, "data · Claude Code");
        assert_eq!(toast.detail, "Shell 需要确认");
        assert!(!toast.title.contains("/mnt/"));
        assert!(!toast.detail.contains("Monitor event"));
        assert!(!toast.detail.contains("br91auk2l"));
    }

    #[test]
    fn agent_notification_uses_high_priority_nudge_copy() {
        let session = AgentSession {
            session_id: "wait".into(),
            source: AgentSource::Codex,
            workspace: "D:\\C\\Desktop\\ai\\bitcat".into(),
            parent_session_id: None,
            status: AgentStatus::Waiting,
            tool_name: Some("Patch".into()),
            tool_input_preview: Some("raw patch content".into()),
            user_prompt_preview: None,
            last_response_preview: None,
            background: false,
            agent_id: None,
            agent_type: None,
            task_id: None,
            output_file: None,
            pid: None,
            machine: Some("qy113".into()),
            updated_at_ms: 10,
            status_changed_at_ms: 10,
            needs_user: true,
        };
        let nudge = AgentNudge {
            session_id: "wait".into(),
            kind: AgentNudgeKind::WaitingForUser,
            message: "legacy message".into(),
            mood: ai_pad_core::pet_event::PetMood::Confused,
            ttl_ms: 12_000,
            use_tts: false,
        };
        let toast = agent_toast_payload(&nudge, &session);
        let notification = agent_notification_payload(&nudge, &session, &toast);

        assert!(nudge_uses_notification(
            AgentNudgeKind::WaitingForUser,
            &session
        ));
        assert!(nudge_uses_notification(AgentNudgeKind::TaskDone, &session));
        assert!(nudge_uses_notification(AgentNudgeKind::TaskError, &session));
        assert!(!nudge_uses_notification(
            AgentNudgeKind::AwayWhileWorking,
            &session
        ));
        assert_eq!(notification.source, "agent_watch");
        assert_eq!(notification.tone, "warning");
        assert_eq!(notification.title, "bitcat · Codex · Patch 需要查看");
        assert!(notification.body.is_none());
        assert!(!notification
            .body
            .as_deref()
            .unwrap_or("")
            .contains("bitcat"));
        assert!(notification.actions.is_empty());
        assert!(notification.reminder_id.is_none());
    }

    #[test]
    fn agent_done_notification_collapses_redundant_status_body() {
        let session = AgentSession {
            session_id: "done".into(),
            source: AgentSource::Codex,
            workspace: "D:\\C\\Desktop\\ai\\pg_gpu".into(),
            parent_session_id: None,
            status: AgentStatus::Done,
            tool_name: None,
            tool_input_preview: None,
            user_prompt_preview: None,
            last_response_preview: None,
            background: false,
            agent_id: None,
            agent_type: None,
            task_id: None,
            output_file: None,
            pid: None,
            machine: None,
            updated_at_ms: 10,
            status_changed_at_ms: 10,
            needs_user: false,
        };
        let nudge = AgentNudge {
            session_id: "done".into(),
            kind: AgentNudgeKind::TaskDone,
            message: "legacy message".into(),
            mood: ai_pad_core::pet_event::PetMood::Happy,
            ttl_ms: 8_000,
            use_tts: false,
        };
        let toast = agent_toast_payload(&nudge, &session);
        let notification = agent_notification_payload(&nudge, &session, &toast);

        assert_eq!(notification.title, "pg_gpu · Codex · 已完成");
        assert!(notification.body.is_none());
    }

    #[test]
    fn agent_tool_error_does_not_show_top_notification() {
        let mut session = AgentSession {
            session_id: "tool-error".into(),
            source: AgentSource::ClaudeCode,
            workspace: "D:\\C\\Desktop\\ai\\pg_gpu".into(),
            parent_session_id: None,
            status: AgentStatus::Error,
            tool_name: Some("Read".into()),
            tool_input_preview: Some("missing.txt".into()),
            user_prompt_preview: None,
            last_response_preview: None,
            background: false,
            agent_id: None,
            agent_type: None,
            task_id: None,
            output_file: None,
            pid: None,
            machine: None,
            updated_at_ms: 10,
            status_changed_at_ms: 10,
            needs_user: true,
        };

        assert!(!nudge_uses_notification(
            AgentNudgeKind::TaskError,
            &session
        ));

        session.tool_name = None;
        assert!(nudge_uses_notification(AgentNudgeKind::TaskError, &session));
    }

    #[test]
    fn remote_access_switches_gate_view_and_installer() {
        let mut settings = AgentWatchSettings::default();
        settings.remote_view_enabled = false;
        assert_eq!(
            remote_access_forbidden("/watch", &settings).map(|(status, _, _)| status),
            Some("403 Forbidden")
        );
        assert_eq!(
            remote_access_forbidden("/agent-sessions", &settings).map(|(status, _, _)| status),
            Some("403 Forbidden")
        );
        assert!(remote_access_forbidden("/health", &settings).is_none());

        settings.remote_view_enabled = true;
        settings.remote_install_enabled = false;
        assert_eq!(
            remote_access_forbidden("/remote-install.sh", &settings).map(|(status, _, _)| status),
            Some("403 Forbidden")
        );
        assert!(remote_access_forbidden("/watch", &settings).is_none());
    }
}
