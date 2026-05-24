//! Agent 看管提醒策略。
//!
//! 本模块只根据归一后的 `AgentSession`、用户配置和当前时间生成提醒决策，
//! 不访问窗口、Tauri 或系统 API。app 层负责把 `AgentNudge` 转成宠物事件、
//! 气泡、TTS 和审计日志。

use crate::agent_session::{AgentSession, AgentStatus};
use crate::app_settings::AgentWatchSettings;
use crate::pet_event::PetMood;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const DEFAULT_NUDGE_TTL_MS: u64 = 12_000;

/// 看管提醒类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentNudgeKind {
    AwayWhileWorking,
    WaitingForUser,
    TaskDone,
    TaskError,
}

impl AgentNudgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AwayWhileWorking => "away_while_working",
            Self::WaitingForUser => "waiting_for_user",
            Self::TaskDone => "task_done",
            Self::TaskError => "task_error",
        }
    }
}

/// 一次要发给 app 层的提醒。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentNudge {
    pub session_id: String,
    pub kind: AgentNudgeKind,
    pub message: String,
    pub mood: PetMood,
    pub ttl_ms: u64,
    pub use_tts: bool,
}

/// 被策略跳过时的稳定原因，便于写入审计日志。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentNudgeSkipReason {
    Disabled,
    AwayDisabled,
    NotDue,
    Cooldown,
    AlreadyNotified,
    NoNudgeForStatus,
}

impl AgentNudgeSkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::AwayDisabled => "away_disabled",
            Self::NotDue => "not_due",
            Self::Cooldown => "cooldown",
            Self::AlreadyNotified => "already_notified",
            Self::NoNudgeForStatus => "no_nudge_for_status",
        }
    }
}

/// 策略运行结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentNudgeDecision {
    Send(AgentNudge),
    Skip {
        reason: AgentNudgeSkipReason,
        status: AgentStatus,
    },
}

/// 按 session 记忆提醒历史和状态转换。
#[derive(Debug, Default)]
pub struct AgentNudgePolicy {
    sessions: HashMap<String, NudgeSessionState>,
}

#[derive(Debug, Clone)]
struct NudgeSessionState {
    last_status: AgentStatus,
    status_entered_at_ms: u64,
    last_away_nudge_at_ms: Option<u64>,
    waiting_notified_for_entered_at_ms: Option<u64>,
    done_notified: bool,
    error_notified_for_entered_at_ms: Option<u64>,
}

impl AgentNudgePolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// 根据当前 session 快照生成提醒或跳过原因。
    pub fn evaluate(
        &mut self,
        session: &AgentSession,
        settings: &AgentWatchSettings,
        now_ms: u64,
    ) -> AgentNudgeDecision {
        let state = self
            .sessions
            .entry(session.session_id.clone())
            .or_insert_with(|| NudgeSessionState {
                last_status: session.status,
                status_entered_at_ms: session.status_changed_at_ms,
                last_away_nudge_at_ms: None,
                waiting_notified_for_entered_at_ms: None,
                done_notified: false,
                error_notified_for_entered_at_ms: None,
            });

        if state.last_status != session.status {
            state.last_status = session.status;
            state.status_entered_at_ms = now_ms;
            if session.status != AgentStatus::Done {
                state.done_notified = false;
            }
        } else if session.status_changed_at_ms > 0 {
            state.status_entered_at_ms = session.status_changed_at_ms;
        }

        if !settings.enabled {
            return skip(AgentNudgeSkipReason::Disabled, session.status);
        }

        match session.status {
            AgentStatus::Waiting => self.waiting_nudge(session, settings, now_ms),
            AgentStatus::Done => self.done_nudge(session, settings),
            AgentStatus::Error | AgentStatus::Interrupted => {
                self.error_nudge(session, settings, now_ms)
            }
            AgentStatus::Working | AgentStatus::ToolRunning => {
                self.away_nudge(session, settings, now_ms)
            }
            _ => skip(AgentNudgeSkipReason::NoNudgeForStatus, session.status),
        }
    }

    pub fn remove_session(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
    }

    /// 记录提醒已经实际展示，用于只在成功发送后推进冷却。
    pub fn mark_sent(&mut self, nudge: &AgentNudge, now_ms: u64) {
        if nudge.kind != AgentNudgeKind::AwayWhileWorking {
            return;
        }
        if let Some(state) = self.sessions.get_mut(&nudge.session_id) {
            state.last_away_nudge_at_ms = Some(now_ms);
        }
    }

    fn waiting_nudge(
        &mut self,
        session: &AgentSession,
        settings: &AgentWatchSettings,
        _now_ms: u64,
    ) -> AgentNudgeDecision {
        if !settings.waiting_alert {
            return skip(AgentNudgeSkipReason::Disabled, session.status);
        }
        let state = self.sessions.get_mut(&session.session_id).unwrap();
        if state.waiting_notified_for_entered_at_ms == Some(state.status_entered_at_ms) {
            return skip(AgentNudgeSkipReason::AlreadyNotified, session.status);
        }
        state.waiting_notified_for_entered_at_ms = Some(state.status_entered_at_ms);
        AgentNudgeDecision::Send(AgentNudge {
            session_id: session.session_id.clone(),
            kind: AgentNudgeKind::WaitingForUser,
            message: nudge_message(session, "需要你处理一下。", task_context(session)),
            mood: PetMood::Confused,
            ttl_ms: DEFAULT_NUDGE_TTL_MS,
            use_tts: settings.use_tts,
        })
    }

    fn done_nudge(
        &mut self,
        session: &AgentSession,
        settings: &AgentWatchSettings,
    ) -> AgentNudgeDecision {
        if !settings.done_alert {
            return skip(AgentNudgeSkipReason::Disabled, session.status);
        }
        let state = self.sessions.get_mut(&session.session_id).unwrap();
        if state.done_notified {
            return skip(AgentNudgeSkipReason::AlreadyNotified, session.status);
        }
        state.done_notified = true;
        AgentNudgeDecision::Send(AgentNudge {
            session_id: session.session_id.clone(),
            kind: AgentNudgeKind::TaskDone,
            message: nudge_message(session, "这轮完成了，可以回来看看。", task_context(session)),
            mood: PetMood::Happy,
            ttl_ms: DEFAULT_NUDGE_TTL_MS,
            use_tts: settings.use_tts,
        })
    }

    fn error_nudge(
        &mut self,
        session: &AgentSession,
        settings: &AgentWatchSettings,
        _now_ms: u64,
    ) -> AgentNudgeDecision {
        let state = self.sessions.get_mut(&session.session_id).unwrap();
        if state.error_notified_for_entered_at_ms == Some(state.status_entered_at_ms) {
            return skip(AgentNudgeSkipReason::AlreadyNotified, session.status);
        }
        state.error_notified_for_entered_at_ms = Some(state.status_entered_at_ms);
        let message = match session.status {
            AgentStatus::Interrupted => {
                nudge_message(session, "这轮被中断了。", task_context(session))
            }
            _ => nudge_message(session, "这轮遇到异常了。", task_context(session)),
        };
        AgentNudgeDecision::Send(AgentNudge {
            session_id: session.session_id.clone(),
            kind: AgentNudgeKind::TaskError,
            message,
            mood: PetMood::Confused,
            ttl_ms: DEFAULT_NUDGE_TTL_MS,
            use_tts: settings.use_tts,
        })
    }

    fn away_nudge(
        &mut self,
        session: &AgentSession,
        settings: &AgentWatchSettings,
        now_ms: u64,
    ) -> AgentNudgeDecision {
        if !settings.away_nudge_enabled {
            return skip(AgentNudgeSkipReason::AwayDisabled, session.status);
        }
        let first_after_ms = settings.first_nudge_after_sec.saturating_mul(1000);
        let repeat_after_ms = settings
            .repeat_nudge_after_min
            .saturating_mul(60)
            .saturating_mul(1000);
        let state = self.sessions.get_mut(&session.session_id).unwrap();
        let status_age_ms = now_ms.saturating_sub(state.status_entered_at_ms);
        if status_age_ms < first_after_ms {
            return skip(AgentNudgeSkipReason::NotDue, session.status);
        }
        if let Some(last) = state.last_away_nudge_at_ms
            && now_ms.saturating_sub(last) < repeat_after_ms
        {
            return skip(AgentNudgeSkipReason::Cooldown, session.status);
        }
        let message = match session.status {
            AgentStatus::ToolRunning => {
                nudge_message(session, "命令还在跑，我帮你盯着。", task_context(session))
            }
            _ => nudge_message(
                session,
                "我帮你盯着，你可以先去做点别的。",
                task_context(session),
            ),
        };
        AgentNudgeDecision::Send(AgentNudge {
            session_id: session.session_id.clone(),
            kind: AgentNudgeKind::AwayWhileWorking,
            message,
            mood: PetMood::Focused,
            ttl_ms: DEFAULT_NUDGE_TTL_MS,
            use_tts: false,
        })
    }
}

fn skip(reason: AgentNudgeSkipReason, status: AgentStatus) -> AgentNudgeDecision {
    AgentNudgeDecision::Skip { reason, status }
}

fn nudge_message(session: &AgentSession, headline: &str, detail: Option<String>) -> String {
    let mut lines = vec![format!("{} {}", session_context(session), headline)];
    if !session.workspace.trim().is_empty() {
        lines.push(format!("位置：{}", session.workspace));
    }
    if let Some(detail) = detail {
        lines.push(detail);
    }
    lines.join("\n")
}

fn session_context(session: &AgentSession) -> String {
    let machine = session
        .machine
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("本机");
    let project = session.workspace_name();
    format!("{machine} / {project} / {}", session.source.display_name())
}

fn task_context(session: &AgentSession) -> Option<String> {
    if let Some(tool) = session
        .tool_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(format!("任务：{}", truncate_chars(tool, 48)));
    }
    if let Some(input) = session
        .tool_input_preview
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(format!("任务：{}", truncate_chars(input, 72)));
    }
    if let Some(prompt) = session
        .user_prompt_preview
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(format!("请求：{}", truncate_chars(prompt, 72)));
    }
    None
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut out = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_session::{AgentSession, AgentSource};

    fn session(status: AgentStatus, entered_at: u64) -> AgentSession {
        AgentSession {
            session_id: "s1".into(),
            source: AgentSource::ClaudeCode,
            workspace: "D:\\repo".into(),
            parent_session_id: None,
            status,
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
            updated_at_ms: entered_at,
            status_changed_at_ms: entered_at,
            needs_user: status.needs_user(),
        }
    }

    #[test]
    fn working_before_threshold_does_not_nudge() {
        let mut policy = AgentNudgePolicy::new();
        let settings = AgentWatchSettings {
            enabled: true,
            ..AgentWatchSettings::default()
        };
        let decision = policy.evaluate(&session(AgentStatus::Working, 0), &settings, 29_000);
        assert!(matches!(
            decision,
            AgentNudgeDecision::Skip {
                reason: AgentNudgeSkipReason::NotDue,
                ..
            }
        ));
    }

    #[test]
    fn working_after_threshold_sends_away_nudge() {
        let mut policy = AgentNudgePolicy::new();
        let settings = AgentWatchSettings {
            enabled: true,
            ..AgentWatchSettings::default()
        };
        let decision = policy.evaluate(&session(AgentStatus::Working, 0), &settings, 30_000);
        match decision {
            AgentNudgeDecision::Send(nudge) => {
                assert_eq!(nudge.kind, AgentNudgeKind::AwayWhileWorking);
                assert_eq!(nudge.mood, PetMood::Focused);
            }
            other => panic!("expected nudge, got {other:?}"),
        }
    }

    #[test]
    fn away_nudge_respects_cooldown() {
        let mut policy = AgentNudgePolicy::new();
        let settings = AgentWatchSettings {
            enabled: true,
            ..AgentWatchSettings::default()
        };
        let s = session(AgentStatus::Working, 0);
        let AgentNudgeDecision::Send(nudge) = policy.evaluate(&s, &settings, 30_000) else {
            panic!("expected first nudge");
        };
        policy.mark_sent(&nudge, 30_000);
        let second = policy.evaluate(&s, &settings, 120_000);
        assert!(matches!(
            second,
            AgentNudgeDecision::Skip {
                reason: AgentNudgeSkipReason::Cooldown,
                ..
            }
        ));
    }

    #[test]
    fn gated_away_nudge_does_not_consume_cooldown_until_marked_sent() {
        let mut policy = AgentNudgePolicy::new();
        let settings = AgentWatchSettings {
            enabled: true,
            ..AgentWatchSettings::default()
        };
        let s = session(AgentStatus::Working, 0);
        assert!(matches!(
            policy.evaluate(&s, &settings, 30_000),
            AgentNudgeDecision::Send(_)
        ));
        assert!(matches!(
            policy.evaluate(&s, &settings, 40_000),
            AgentNudgeDecision::Send(_)
        ));
    }

    #[test]
    fn waiting_sends_immediately_once_per_entry() {
        let mut policy = AgentNudgePolicy::new();
        let settings = AgentWatchSettings {
            enabled: true,
            ..AgentWatchSettings::default()
        };
        let mut s = session(AgentStatus::Waiting, 10);
        s.machine = Some("qy113".into());
        s.source = AgentSource::Codex;
        s.workspace = "D:\\C\\Desktop\\ai\\bitcat".into();
        s.tool_name = Some("Patch".into());
        let decision = policy.evaluate(&s, &settings, 20);
        match decision {
            AgentNudgeDecision::Send(nudge) => {
                assert_eq!(nudge.kind, AgentNudgeKind::WaitingForUser);
                assert!(nudge.message.contains("qy113 / bitcat / Codex"));
                assert!(nudge.message.contains("位置：D:\\C\\Desktop\\ai\\bitcat"));
                assert!(nudge.message.contains("任务：Patch"));
            }
            other => panic!("expected waiting nudge, got {other:?}"),
        }
        assert!(matches!(
            policy.evaluate(&s, &settings, 30),
            AgentNudgeDecision::Skip {
                reason: AgentNudgeSkipReason::AlreadyNotified,
                ..
            }
        ));
    }

    #[test]
    fn done_sends_once() {
        let mut policy = AgentNudgePolicy::new();
        let settings = AgentWatchSettings {
            enabled: true,
            ..AgentWatchSettings::default()
        };
        let s = session(AgentStatus::Done, 10);
        assert!(matches!(
            policy.evaluate(&s, &settings, 20),
            AgentNudgeDecision::Send(AgentNudge {
                kind: AgentNudgeKind::TaskDone,
                ..
            })
        ));
        assert!(matches!(
            policy.evaluate(&s, &settings, 30),
            AgentNudgeDecision::Skip {
                reason: AgentNudgeSkipReason::AlreadyNotified,
                ..
            }
        ));
    }

    #[test]
    fn error_nudge_respects_tts_setting() {
        let mut policy = AgentNudgePolicy::new();
        let settings = AgentWatchSettings {
            enabled: true,
            use_tts: false,
            ..AgentWatchSettings::default()
        };
        let decision = policy.evaluate(&session(AgentStatus::Error, 10), &settings, 20);
        match decision {
            AgentNudgeDecision::Send(nudge) => {
                assert_eq!(nudge.kind, AgentNudgeKind::TaskError);
                assert!(!nudge.use_tts);
            }
            other => panic!("expected error nudge, got {other:?}"),
        }
    }

    #[test]
    fn disabled_settings_skip_all() {
        let mut policy = AgentNudgePolicy::new();
        let settings = AgentWatchSettings::default();
        let decision = policy.evaluate(&session(AgentStatus::Waiting, 0), &settings, 0);
        assert!(matches!(
            decision,
            AgentNudgeDecision::Skip {
                reason: AgentNudgeSkipReason::Disabled,
                ..
            }
        ));
    }
}
