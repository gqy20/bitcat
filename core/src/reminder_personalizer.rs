//! AI-assisted reminder notification copy.
//!
//! This module turns a deterministic `ReminderRecord` into a short notification
//! payload without changing whether the reminder fires. It uses the configured
//! AI provider with a tiny structured extractor and no tools, so scheduler
//! reliability stays separate from model creativity.

use crate::ai_config::AiConfig;
use crate::prompts::PromptsConfig;
use crate::reminder::{ReminderRecord, ReminderSchedule};
use crate::token_tracker::{
    TokenCategory, TokenRecord, TokenUsage, new_session_id, record_token_usage,
};
use rig::client::CompletionClient;
use rig::providers::anthropic;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

const MAX_TITLE_CHARS: usize = 48;
const MAX_BODY_CHARS: usize = 120;

/// Personalized copy for a fired reminder notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReminderNotificationCopy {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub tone: ReminderNotificationTone,
}

/// Visual and sound tone accepted by the notification island.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReminderNotificationTone {
    Info,
    Success,
    Warning,
    Danger,
}

impl ReminderNotificationTone {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Danger => "danger",
        }
    }
}

impl Default for ReminderNotificationTone {
    fn default() -> Self {
        Self::Warning
    }
}

impl ReminderNotificationCopy {
    /// Clamp model output so it fits the compact top notification surface.
    pub fn sanitized(mut self, fallback: &ReminderRecord) -> Self {
        self.title = truncate_chars(self.title.trim(), MAX_TITLE_CHARS);
        if self.title.is_empty() {
            self.title = truncate_chars(fallback.title.trim(), MAX_TITLE_CHARS);
        }

        self.body = truncate_chars(self.body.trim(), MAX_BODY_CHARS);
        if self.body.is_empty() {
            self.body = fallback.message.clone().unwrap_or_default();
            self.body = truncate_chars(self.body.trim(), MAX_BODY_CHARS);
        }
        self
    }
}

/// Generate notification copy for a fired reminder using a no-tool extractor.
pub async fn personalize_reminder_notification(
    ai_config: &AiConfig,
    reminder: &ReminderRecord,
) -> Result<ReminderNotificationCopy, String> {
    let http_client = rig::http_client::ReqwestClient::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;
    let client = anthropic::Client::builder()
        .api_key(&ai_config.api_key)
        .base_url(&ai_config.base_url)
        .http_client(http_client)
        .build()
        .map_err(|e| format!("创建 Anthropic reminder Client 失败: {e}"))?;

    let extractor = client
        .extractor::<ReminderNotificationCopy>(ai_config.model.as_str())
        .preamble(&PromptsConfig::load().reminder_personalizer.preamble)
        .max_tokens(512)
        .retries(0)
        .build();

    let input = reminder_copy_input(reminder);
    let start = std::time::Instant::now();
    let response = extractor
        .extract_with_usage(input)
        .await
        .map_err(|e| format!("生成提醒文案失败: {e}"))?;
    let elapsed = start.elapsed();
    let copy = response.data.sanitized(reminder);
    debug!(
        reminder_id = %reminder.id,
        elapsed_ms = elapsed.as_millis(),
        "reminder notification copy personalized"
    );
    record_token_usage(
        &TokenRecord::new(
            new_session_id(),
            TokenCategory::MemoryAggregation,
            ai_config.model.clone(),
            TokenUsage::from(response.usage),
        )
        .with_extra(format!("reminder_personalizer:{}", reminder.id))
        .with_elapsed_ms(elapsed.as_millis() as u64),
    );
    Ok(copy)
}

fn reminder_copy_input(reminder: &ReminderRecord) -> String {
    format!(
        "提醒标题: {title}\n提醒详情: {message}\n计划: {schedule}\n触发次数: {fire_count}\n下次触发: {next_fire_at}\n来源: {source}",
        title = reminder.title,
        message = reminder.message.clone().unwrap_or_default(),
        schedule = schedule_label(&reminder.schedule),
        fire_count = reminder.fire_count,
        next_fire_at = reminder.next_fire_at,
        source = reminder.source,
    )
}

fn schedule_label(schedule: &ReminderSchedule) -> String {
    match schedule {
        ReminderSchedule::Once { at } => format!("once at {at}"),
        ReminderSchedule::Interval { every_minutes } => format!("every {every_minutes} minutes"),
        ReminderSchedule::Daily { time } => format!("daily at {time}"),
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let keep = max.saturating_sub(3);
        format!("{}...", s.chars().take(keep).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reminder::{ReminderSchedule, ReminderStatus};

    fn reminder() -> ReminderRecord {
        ReminderRecord {
            id: "rem_1".to_string(),
            title: "喝水".to_string(),
            message: Some("该喝水了".to_string()),
            schedule: ReminderSchedule::Interval { every_minutes: 60 },
            next_fire_at: "2026-05-22T10:00:00+08:00".to_string(),
            status: ReminderStatus::Active,
            source: "agent".to_string(),
            created_at: "2026-05-22T09:00:00+08:00".to_string(),
            updated_at: "2026-05-22T10:00:00+08:00".to_string(),
            last_fired_at: Some("2026-05-22T10:00:00+08:00".to_string()),
            fire_count: 2,
        }
    }

    #[test]
    fn sanitizes_empty_copy_with_reminder_fallback() {
        let copy = ReminderNotificationCopy {
            title: " ".to_string(),
            body: " ".to_string(),
            tone: ReminderNotificationTone::Info,
        }
        .sanitized(&reminder());

        assert_eq!(copy.title, "喝水");
        assert_eq!(copy.body, "该喝水了");
        assert_eq!(copy.tone, ReminderNotificationTone::Info);
    }

    #[test]
    fn clamps_overlong_copy() {
        let copy = ReminderNotificationCopy {
            title: "标题".repeat(80),
            body: "正文".repeat(160),
            tone: ReminderNotificationTone::Warning,
        }
        .sanitized(&reminder());

        assert!(copy.title.chars().count() <= MAX_TITLE_CHARS);
        assert!(copy.body.chars().count() <= MAX_BODY_CHARS);
    }

    #[test]
    fn tone_serializes_as_notification_string() {
        assert_eq!(ReminderNotificationTone::Info.as_str(), "info");
        assert_eq!(ReminderNotificationTone::Success.as_str(), "success");
        assert_eq!(ReminderNotificationTone::Warning.as_str(), "warning");
        assert_eq!(ReminderNotificationTone::Danger.as_str(), "danger");
    }
}
