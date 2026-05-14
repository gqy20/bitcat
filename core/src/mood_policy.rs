//! 宠物情绪策略层。
//!
//! 本模块把业务层给出的 `React` 事件整理成前端可稳定播放的情绪反应，负责默认持续时间、
//! 优先级覆盖与低优先级事件节流。它保持为纯逻辑，不依赖 Tauri 或窗口系统，方便 app 层事件总线复用和单元测试。
//! 前端仍只接收语义事件，不需要理解 Rig、工具执行或记忆抽取细节。

use crate::pet_event::{PetEvent, PetMood};
use std::time::Duration;

const DEFAULT_MOOD_TTL_MS: u64 = 8_000;
const IDLE_TTL_MS: u64 = 2_000;
const MIN_LOW_PRIORITY_REACT_GAP_MS: u64 = 1_200;

/// 管理宠物情绪反应的覆盖、持续时间与节流规则。
#[derive(Debug, Clone)]
pub struct MoodPolicy {
    last_mood: PetMood,
    last_react_at_ms: Option<u64>,
}

impl Default for MoodPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl MoodPolicy {
    /// 创建默认情绪策略。
    pub fn new() -> Self {
        Self {
            last_mood: PetMood::Idle,
            last_react_at_ms: None,
        }
    }

    /// 整理单个宠物事件；返回 `None` 表示该事件被策略节流。
    pub fn apply(&mut self, event: PetEvent, now: Duration) -> Option<PetEvent> {
        match event {
            PetEvent::React {
                mood,
                speech,
                ttl_ms,
            } => self.apply_react(mood, speech, ttl_ms, now),
            other => Some(other),
        }
    }

    fn apply_react(
        &mut self,
        mood: PetMood,
        speech: Option<String>,
        ttl_ms: Option<u64>,
        now: Duration,
    ) -> Option<PetEvent> {
        let now_ms = now.as_millis().min(u128::from(u64::MAX)) as u64;
        if self.should_drop_react(mood, now_ms) {
            return None;
        }

        self.last_mood = mood;
        self.last_react_at_ms = Some(now_ms);
        Some(PetEvent::React {
            mood,
            speech,
            ttl_ms: Some(ttl_ms.unwrap_or_else(|| default_ttl_ms(mood))),
        })
    }

    fn should_drop_react(&self, mood: PetMood, now_ms: u64) -> bool {
        if mood_priority(mood) > mood_priority(self.last_mood) {
            return false;
        }
        if let Some(last_at) = self.last_react_at_ms {
            let elapsed = now_ms.saturating_sub(last_at);
            mood == self.last_mood
                && mood_priority(mood) <= 1
                && elapsed < MIN_LOW_PRIORITY_REACT_GAP_MS
        } else {
            false
        }
    }
}

fn default_ttl_ms(mood: PetMood) -> u64 {
    match mood {
        PetMood::Idle => IDLE_TTL_MS,
        _ => DEFAULT_MOOD_TTL_MS,
    }
}

fn mood_priority(mood: PetMood) -> u8 {
    match mood {
        PetMood::Idle => 0,
        PetMood::Sleepy => 1,
        PetMood::Happy | PetMood::Caring | PetMood::Focused => 2,
        PetMood::Confused => 3,
        PetMood::Excited => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn react(mood: PetMood) -> PetEvent {
        PetEvent::React {
            mood,
            speech: None,
            ttl_ms: None,
        }
    }

    #[test]
    fn adds_default_ttl_to_reaction() {
        let mut policy = MoodPolicy::new();
        let event = policy
            .apply(react(PetMood::Happy), Duration::from_millis(0))
            .unwrap();

        assert_eq!(
            event,
            PetEvent::React {
                mood: PetMood::Happy,
                speech: None,
                ttl_ms: Some(DEFAULT_MOOD_TTL_MS),
            }
        );
    }

    #[test]
    fn preserves_explicit_ttl() {
        let mut policy = MoodPolicy::new();
        let event = policy
            .apply(
                PetEvent::React {
                    mood: PetMood::Caring,
                    speech: Some("收到".into()),
                    ttl_ms: Some(3_000),
                },
                Duration::from_millis(0),
            )
            .unwrap();

        assert_eq!(
            event,
            PetEvent::React {
                mood: PetMood::Caring,
                speech: Some("收到".into()),
                ttl_ms: Some(3_000),
            }
        );
    }

    #[test]
    fn throttles_repeated_low_priority_reaction() {
        let mut policy = MoodPolicy::new();
        assert!(
            policy
                .apply(react(PetMood::Sleepy), Duration::from_millis(0))
                .is_some()
        );

        assert!(
            policy
                .apply(react(PetMood::Sleepy), Duration::from_millis(500))
                .is_none()
        );

        assert!(
            policy
                .apply(react(PetMood::Sleepy), Duration::from_millis(1_300))
                .is_some()
        );
    }

    #[test]
    fn higher_priority_reaction_can_override_immediately() {
        let mut policy = MoodPolicy::new();
        assert!(
            policy
                .apply(react(PetMood::Sleepy), Duration::from_millis(0))
                .is_some()
        );

        assert!(
            policy
                .apply(react(PetMood::Confused), Duration::from_millis(100))
                .is_some()
        );
    }
}
