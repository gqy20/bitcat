//! 宠物事件总线。
//!
//! 本模块是 app 层到前端 `pet-event` IPC 的唯一发送入口，集中处理去重、节流、情绪策略和日志。
//! 这样 gamepad、game、Rig 工具生命周期等上游只需要描述发生了什么，不需要分散了解前端状态机细节。
//! 它依赖 core 中的纯逻辑协议与 `MoodPolicy`，再通过 Tauri `emit` 把最终事件推给宠物窗口。

use ai_pad_core::mood_policy::MoodPolicy;
use ai_pad_core::pet_event::{PetEvent, PetNotificationKind};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tracing::{debug, trace, warn};

const DEDUPE_WINDOW: Duration = Duration::from_millis(300);

/// 统一发送宠物事件的共享总线。
#[derive(Debug)]
pub struct SharedPetEventBus {
    inner: Mutex<PetEventBus>,
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
}

#[derive(Debug)]
pub(crate) struct PetEventBus {
    started_at: Instant,
    mood_policy: MoodPolicy,
    last_event_key: Option<EventKey>,
}

impl PetEventBus {
    pub(crate) fn new() -> Self {
        Self {
            started_at: Instant::now(),
            mood_policy: MoodPolicy::new(),
            last_event_key: None,
        }
    }

    fn prepare(&mut self, event: PetEvent, now: Instant) -> Option<PetEvent> {
        let Some(event) = self
            .mood_policy
            .apply(event, now.duration_since(self.started_at))
        else {
            trace!("pet event skipped by mood policy");
            return None;
        };

        let key = EventKey::from_event(&event, now);
        if self.is_duplicate(&key, now) {
            trace!(?key, "pet event deduplicated");
            return None;
        }
        self.last_event_key = Some(key);
        Some(event)
    }

    fn emit(&mut self, app: &AppHandle, event: PetEvent) {
        let Some(event) = self.prepare(event, Instant::now()) else {
            return;
        };

        debug!(event_type = event_type(&event), "emit pet-event");
        if let Err(e) = app.emit("pet-event", event) {
            warn!(error = %e, "emit pet-event failed");
        }
    }

    fn is_duplicate(&self, key: &EventKey, now: Instant) -> bool {
        self.last_event_key
            .as_ref()
            .is_some_and(|last| last.matches(key) && now.duration_since(last.at) < DEDUPE_WINDOW)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_pad_core::pet_event::{PetMood, PetNotificationKind};

    #[test]
    fn bus_adds_mood_ttl_before_emit() {
        let mut bus = PetEventBus::new();
        let event = bus
            .prepare(PetEvent::react(PetMood::Happy), Instant::now())
            .unwrap();

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

        assert!(bus.prepare(event.clone(), Instant::now()).is_some());
        assert!(bus
            .prepare(event, Instant::now() + Duration::from_millis(100))
            .is_none());
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
