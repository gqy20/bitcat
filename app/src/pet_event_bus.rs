//! 宠物事件总线。
//!
//! 本模块是 app 层到前端 `pet-event` IPC 的唯一发送入口，集中处理去重、节流、情绪策略和日志。
//! 这样 gamepad、game、Rig 工具生命周期等上游只需要描述发生了什么，不需要分散了解前端状态机细节。
//! 它依赖 core 中的纯逻辑协议与 `MoodPolicy`，再通过 Tauri `emit` 把最终事件推给宠物窗口。

use bitcat_core::mood_policy::MoodPolicy;
use bitcat_core::pet_event::{PetEvent, PetNotificationKind};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tracing::{debug, trace, warn};

const DEDUPE_WINDOW: Duration = Duration::from_millis(300);
const EVENT_LOG_LIMIT: usize = 50;

/// 统一发送宠物事件的共享总线。
#[derive(Debug)]
pub struct SharedPetEventBus {
    inner: Mutex<PetEventBus>,
}

/// 宠物事件总线的可观察性快照。
#[derive(Debug, Clone, Serialize)]
pub struct PetEventLogSnapshot {
    pub generated_at: String,
    pub entries: Vec<PetEventLogEntry>,
}

/// 单条宠物事件处理记录。
#[derive(Debug, Clone, Serialize)]
pub struct PetEventLogEntry {
    pub seq: u64,
    pub timestamp: String,
    pub event_type: String,
    pub decision: PetEventDecision,
    pub reason: Option<String>,
    pub payload: serde_json::Value,
}

/// 事件总线对事件的最终处理结果。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PetEventDecision {
    Sent,
    Deduplicated,
    Throttled,
    EmitFailed,
}

impl Default for SharedPetEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedPetEventBus {
    /// 创建默认事件总线状态。
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(PetEventBus::new()),
        }
    }

    /// 发送一个宠物事件；策略可能会决定跳过重复或低价值事件。
    pub fn emit(&self, app: &AppHandle, event: PetEvent) {
        let mut bus = match self.inner.lock() {
            Ok(bus) => bus,
            Err(e) => {
                warn!(error = %e, "pet event bus lock poisoned");
                return;
            }
        };
        bus.emit(app, event);
    }

    /// 返回最近的宠物事件处理记录，最新事件在最前。
    pub fn snapshot(&self) -> Result<PetEventLogSnapshot, String> {
        let bus = self.inner.lock().map_err(|e| e.to_string())?;
        Ok(bus.snapshot())
    }
}

#[derive(Debug)]
pub(crate) struct PetEventBus {
    started_at: Instant,
    mood_policy: MoodPolicy,
    last_event_key: Option<EventKey>,
    event_log: VecDeque<PetEventLogEntry>,
    next_seq: u64,
}

impl PetEventBus {
    pub(crate) fn new() -> Self {
        Self {
            started_at: Instant::now(),
            mood_policy: MoodPolicy::new(),
            last_event_key: None,
            event_log: VecDeque::with_capacity(EVENT_LOG_LIMIT),
            next_seq: 1,
        }
    }

    fn prepare(&mut self, event: PetEvent, now: Instant) -> PreparedPetEvent {
        let original_type = event_type(&event);
        let Some(event) = self
            .mood_policy
            .apply(event, now.duration_since(self.started_at))
        else {
            trace!("pet event skipped by mood policy");
            return PreparedPetEvent::Skipped {
                decision: PetEventDecision::Throttled,
                event_type: event_type_name(original_type),
                reason: Some("mood_policy".to_string()),
                payload: serde_json::Value::Null,
            };
        };

        let key = EventKey::from_event(&event, now);
        if self.is_duplicate(&key, now) {
            trace!(?key, "pet event deduplicated");
            return PreparedPetEvent::Skipped {
                decision: PetEventDecision::Deduplicated,
                event_type: event_type_name(event_type(&event)),
                reason: Some("dedupe_window".to_string()),
                payload: event_payload(&event),
            };
        }
        self.last_event_key = Some(key);
        PreparedPetEvent::Ready(event)
    }

    fn emit(&mut self, app: &AppHandle, event: PetEvent) {
        let now = Instant::now();
        let event = match self.prepare(event, now) {
            PreparedPetEvent::Ready(event) => event,
            PreparedPetEvent::Skipped {
                decision,
                event_type,
                reason,
                payload,
            } => {
                self.push_log(now, event_type, decision, reason, payload);
                return;
            }
        };

        debug!(event_type = event_type(&event), "emit pet-event");
        if let Err(e) = app.emit("pet-event", event.clone()) {
            warn!(error = %e, "emit pet-event failed");
            self.push_log(
                now,
                event_type_name(event_type(&event)),
                PetEventDecision::EmitFailed,
                Some(e.to_string()),
                event_payload(&event),
            );
        } else {
            self.push_log(
                now,
                event_type_name(event_type(&event)),
                PetEventDecision::Sent,
                None,
                event_payload(&event),
            );
        }
    }

    fn is_duplicate(&self, key: &EventKey, now: Instant) -> bool {
        self.last_event_key
            .as_ref()
            .is_some_and(|last| last.matches(key) && now.duration_since(last.at) < DEDUPE_WINDOW)
    }

    fn push_log(
        &mut self,
        now: Instant,
        event_type: String,
        decision: PetEventDecision,
        reason: Option<String>,
        payload: serde_json::Value,
    ) {
        let elapsed_ms = now.duration_since(self.started_at).as_millis();
        self.event_log.push_front(PetEventLogEntry {
            seq: self.next_seq,
            timestamp: format!("+{elapsed_ms}ms"),
            event_type,
            decision,
            reason,
            payload,
        });
        self.next_seq = self.next_seq.saturating_add(1);
        while self.event_log.len() > EVENT_LOG_LIMIT {
            self.event_log.pop_back();
        }
    }

    fn snapshot(&self) -> PetEventLogSnapshot {
        PetEventLogSnapshot {
            generated_at: chrono::Local::now().to_rfc3339(),
            entries: self.event_log.iter().cloned().collect(),
        }
    }
}

#[derive(Debug)]
enum PreparedPetEvent {
    Ready(PetEvent),
    Skipped {
        decision: PetEventDecision,
        event_type: String,
        reason: Option<String>,
        payload: serde_json::Value,
    },
}

/// 返回最近的宠物事件处理记录。
#[tauri::command]
pub async fn cmd_get_pet_event_log(
    bus: tauri::State<'_, SharedPetEventBus>,
) -> Result<PetEventLogSnapshot, String> {
    bus.snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcat_core::pet_event::{PetMood, PetNotificationKind};

    #[test]
    fn bus_adds_mood_ttl_before_emit() {
        let mut bus = PetEventBus::new();
        let event = match bus.prepare(PetEvent::react(PetMood::Happy), Instant::now()) {
            PreparedPetEvent::Ready(event) => event,
            other => panic!("expected ready event, got {other:?}"),
        };

        assert_eq!(
            event,
            PetEvent::React {
                mood: PetMood::Happy,
                speech: None,
                ttl_ms: Some(8_000),
            }
        );
    }

    #[test]
    fn bus_deduplicates_repeated_notification_in_short_window() {
        let mut bus = PetEventBus::new();
        let event = PetEvent::Notify {
            kind: PetNotificationKind::AiThinking,
            body: Some("thinking".into()),
            ttl_ms: Some(30_000),
            refresh: true,
        };

        assert!(matches!(
            bus.prepare(event.clone(), Instant::now()),
            PreparedPetEvent::Ready(_)
        ));
        assert!(matches!(
            bus.prepare(event, Instant::now() + Duration::from_millis(100)),
            PreparedPetEvent::Skipped {
                decision: PetEventDecision::Deduplicated,
                ..
            }
        ));
    }

    #[test]
    fn bus_keeps_recent_decision_log() {
        let mut bus = PetEventBus::new();
        bus.push_log(
            Instant::now(),
            "notify".into(),
            PetEventDecision::Sent,
            None,
            serde_json::json!({"type": "notify"}),
        );

        let snapshot = bus.snapshot();

        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].seq, 1);
        assert_eq!(snapshot.entries[0].decision, PetEventDecision::Sent);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EventKey {
    kind: EventKeyKind,
    at: Instant,
}

impl EventKey {
    fn from_event(event: &PetEvent, at: Instant) -> Self {
        Self {
            kind: match event {
                PetEvent::Notify { kind, body, .. } => EventKeyKind::Notify(*kind, body.clone()),
                PetEvent::ClearNotification { kind } => EventKeyKind::Clear(*kind),
                PetEvent::React { mood, .. } => EventKeyKind::React(format!("{mood:?}")),
                PetEvent::SetMode { mode } => EventKeyKind::Mode(format!("{mode:?}")),
                PetEvent::WalkTo { x } => EventKeyKind::WalkTo((x * 10.0).round() as i32),
                PetEvent::ShowBubble { text } => EventKeyKind::Bubble(text.clone()),
                PetEvent::PlayDance { name } => EventKeyKind::Dance(name.clone()),
                PetEvent::Exit => EventKeyKind::Exit,
            },
            at,
        }
    }

    fn matches(&self, other: &EventKey) -> bool {
        self.kind == other.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EventKeyKind {
    Notify(PetNotificationKind, Option<String>),
    Clear(Option<PetNotificationKind>),
    React(String),
    Mode(String),
    WalkTo(i32),
    Bubble(String),
    Dance(String),
    Exit,
}

fn event_type(event: &PetEvent) -> &'static str {
    match event {
        PetEvent::Notify { .. } => "notify",
        PetEvent::ClearNotification { .. } => "clear_notification",
        PetEvent::React { .. } => "react",
        PetEvent::SetMode { .. } => "set_mode",
        PetEvent::WalkTo { .. } => "walk_to",
        PetEvent::ShowBubble { .. } => "show_bubble",
        PetEvent::PlayDance { .. } => "play_dance",
        PetEvent::Exit => "exit",
    }
}

fn event_type_name(value: &str) -> String {
    value.to_string()
}

fn event_payload(event: &PetEvent) -> serde_json::Value {
    serde_json::to_value(event).unwrap_or(serde_json::Value::Null)
}
