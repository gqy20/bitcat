//! Agent conversation wrap-up extraction.
//!
//! This module uses rig `Extractor<T>` to compress one free-form chat turn into a
//! structured pet reaction: final mood, optional short speech, and durable memory candidates.
//! The model owns semantic judgment; Rust only validates fields, truncates lengths, and falls back deterministically.

use crate::ai_config::AiConfig;
use crate::pet_event::PetMood;
use crate::token_tracker::{
    TokenCategory, TokenRecord, TokenUsage, new_session_id, record_token_usage,
};
use rig::client::CompletionClient;
use rig::providers::anthropic;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

const MAX_SPEECH_CHARS: usize = 160;
const MAX_MEMORY_CANDIDATES: usize = 5;
const MAX_MEMORY_TEXT_CHARS: usize = 180;
const MAX_MEMORY_REASON_CHARS: usize = 160;
const MAX_MEMORY_KIND_CHARS: usize = 32;
const MAX_MEMORY_TTL_CHARS: usize = 24;
const MAX_TAGS: usize = 6;
const MAX_TAG_CHARS: usize = 24;
const MIN_MEMORY_CONFIDENCE: u8 = 3;

/// Structured response generated after one chat turn finishes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgentReaction {
    /// Final pet mood to display after the turn.
    pub mood: PetMood,
    /// Optional short closing line; the main reply is used as fallback when empty.
    #[serde(default)]
    pub speech: String,
    /// Durable memory candidates judged by the model.
    #[serde(default)]
    pub memory_candidates: Vec<MemoryCandidate>,
    /// Optional follow-up suggestions, retained for future UI use.
    #[serde(default)]
    pub followups: Vec<String>,
}

/// One candidate long-term memory item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryCandidate {
    /// Self-contained, grep-friendly fact or preference.
    pub text: String,
    /// Importance from 1..=5; candidates below 3 are dropped.
    pub importance: u8,
    /// Confidence that this is worth saving long-term, from 1..=5.
    #[serde(default = "default_memory_confidence")]
    pub confidence: u8,
    /// Candidate type, such as preference/profile/project/constraint/relationship/other.
    #[serde(default)]
    pub kind: String,
    /// Lifetime hint: stable/evolving/temporary. Temporary candidates are dropped.
    #[serde(default)]
    pub ttl_hint: String,
    /// Short reason explaining why the model thinks this is durable memory.
    #[serde(default)]
    pub reason: String,
    /// Short English or Chinese tags.
    #[serde(default)]
    pub tags: Vec<String>,
}

impl AgentReaction {
    /// Deterministic fallback when extraction fails; it never guesses from keywords.
    pub fn fallback(reply: &str) -> Self {
        Self {
            mood: PetMood::Idle,
            speech: truncate_chars(reply, MAX_SPEECH_CHARS),
            memory_candidates: Vec::new(),
            followups: Vec::new(),
        }
    }

    /// Validate and normalize model output before UI or memory persistence consumes it.
    pub fn sanitized(mut self, fallback_speech: &str) -> Self {
        if self.speech.trim().is_empty() {
            self.speech = fallback_speech.to_string();
        }
        self.speech = truncate_chars(self.speech.trim(), MAX_SPEECH_CHARS);

        self.memory_candidates = self
            .memory_candidates
            .into_iter()
            .filter_map(MemoryCandidate::sanitize)
            .take(MAX_MEMORY_CANDIDATES)
            .collect();
        self.followups = self
            .followups
            .into_iter()
            .map(|s| truncate_chars(s.trim(), 80))
            .filter(|s| !s.is_empty())
            .take(3)
            .collect();
        self
    }
}

impl MemoryCandidate {
    /// Build a high-confidence candidate for explicit `remember` tool requests and tests.
    pub fn explicit(text: String, importance: u8, tags: Vec<String>) -> Self {
        Self {
            text,
            importance,
            confidence: 5,
            kind: "other".to_string(),
            ttl_hint: "stable".to_string(),
            reason: "explicitly requested memory".to_string(),
            tags,
        }
    }

    fn sanitize(mut self) -> Option<Self> {
        self.text = truncate_chars(self.text.trim(), MAX_MEMORY_TEXT_CHARS);
        self.importance = self.importance.min(5);
        self.confidence = self.confidence.min(5);
        self.kind = normalize_label(&self.kind, MAX_MEMORY_KIND_CHARS);
        self.ttl_hint = normalize_label(&self.ttl_hint, MAX_MEMORY_TTL_CHARS);
        self.reason = truncate_chars(self.reason.trim(), MAX_MEMORY_REASON_CHARS);
        if self.text.is_empty()
            || self.importance < 3
            || self.confidence < MIN_MEMORY_CONFIDENCE
            || self.ttl_hint == "temporary"
        {
            return None;
        }
        self.tags = self
            .tags
            .into_iter()
            .map(|tag| normalize_tag(&tag))
            .filter(|tag| !tag.is_empty())
            .take(MAX_TAGS)
            .collect();
        Some(self)
    }
}

fn default_memory_confidence() -> u8 {
    3
}

/// Extract structured reaction data with rig `Extractor`.
pub async fn extract_agent_reaction(
    ai_config: &AiConfig,
    user_msg: &str,
    ai_reply: &str,
    tool_events: &[String],
) -> Result<AgentReaction, String> {
    let http_client = rig::http_client::ReqwestClient::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to create HTTP client: {e}"))?;
    let client = anthropic::Client::builder()
        .api_key(&ai_config.api_key)
        .base_url(&ai_config.base_url)
        .http_client(http_client)
        .build()
        .map_err(|e| format!("failed to create Anthropic reaction client: {e}"))?;

    let extractor = client
        .extractor::<AgentReaction>(ai_config.model.as_str())
        .preamble(REACTION_PREAMBLE)
        .max_tokens(1024)
        .retries(1)
        .build();

    let tool_text = if tool_events.is_empty() {
        "No tool calls.".to_string()
    } else {
        tool_events.join("\n")
    };
    let input = format!(
        "User message:\n{user_msg}\n\nFinal assistant reply:\n{ai_reply}\n\nTool events this turn:\n{tool_text}"
    );

    let start = std::time::Instant::now();
    let response = extractor
        .extract_with_usage(input)
        .await
        .map_err(|e| format!("failed to extract AgentReaction: {e}"))?;
    let elapsed = start.elapsed();
    let reaction = response.data.sanitized(ai_reply);
    debug!(
        mood = ?reaction.mood,
        memory_candidates = reaction.memory_candidates.len(),
        elapsed_ms = elapsed.as_millis(),
        "AgentReaction extraction completed"
    );
    record_token_usage(
        &TokenRecord::new(
            new_session_id(),
            TokenCategory::MemoryAggregation,
            ai_config.model.clone(),
            TokenUsage::from(response.usage),
        )
        .with_extra("agent_reaction".to_string())
        .with_elapsed_ms(elapsed.as_millis() as u64),
    );

    Ok(reaction)
}

/// Log extraction failure and return a deterministic idle fallback.
pub fn fallback_agent_reaction(reply: &str, error: &str) -> AgentReaction {
    warn!(
        error,
        "AgentReaction extraction failed, using idle fallback"
    );
    AgentReaction::fallback(reply)
}

fn normalize_tag(tag: &str) -> String {
    tag.trim()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .take(MAX_TAG_CHARS)
        .collect::<String>()
        .to_lowercase()
}

fn normalize_label(label: &str, max_chars: usize) -> String {
    label
        .trim()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .take(max_chars)
        .collect::<String>()
        .to_lowercase()
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let keep = max.saturating_sub(3);
        format!("{}...", s.chars().take(keep).collect::<String>())
    }
}

const REACTION_PREAMBLE: &str = r#"
You compress one desktop-pet AI conversation turn into strict structured JSON.

Judgment rules:
- `mood` is the final pet mood, not a summary of the text.
- For normal explanations or completed tasks, prefer `idle` or `focused`; use `happy`/`excited` only for clear celebration or delight.
- For tool failures, blocked permissions, or explicit inability to complete the request, use `confused`.
- Use `caring` for comfort, encouragement, or companionship; use `sleepy` only in sleep/rest contexts.
- `memory_candidates` must contain only durable facts, preferences, project background, stable constraints, relationship context, or information the user explicitly asked to remember.
- Each memory candidate must include `confidence`, `kind`, `ttl_hint`, and `reason`.
- `kind` must be one of `preference`, `profile`, `project`, `constraint`, `relationship`, or `other`.
- `ttl_hint` must be one of `stable`, `evolving`, or `temporary`.
- The decision criterion is whether the information will help in future turns, not whether it happened in this turn.
- If uncertain, lower `confidence`; if it is only a one-off task, set `ttl_hint` to `temporary`.
- Do not save one-off greetings, ordinary Q&A steps, temporary errors, one-off reminders, completed reminders, single tool results, or clearly expired information.
- Each memory `text` must be self-contained; avoid "this", "above", or "just now".
- `importance` is 1..5; only candidates above 3 are persisted.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_reaction_fields() {
        let reaction = AgentReaction {
            mood: PetMood::Happy,
            speech: String::new(),
            memory_candidates: vec![
                MemoryCandidate::explicit(
                    " user prefers grep-first memory retrieval ".into(),
                    5,
                    vec!["Memory!".into(), "Preference".into()],
                ),
                MemoryCandidate::explicit("too weak".into(), 1, vec![]),
            ],
            followups: vec![" next step ".into()],
        }
        .sanitized("fallback speech");

        assert_eq!(reaction.speech, "fallback speech");
        assert_eq!(reaction.memory_candidates.len(), 1);
        assert_eq!(
            reaction.memory_candidates[0].text,
            "user prefers grep-first memory retrieval"
        );
        assert_eq!(
            reaction.memory_candidates[0].tags,
            vec!["memory", "preference"]
        );
        assert_eq!(reaction.memory_candidates[0].confidence, 5);
        assert_eq!(reaction.memory_candidates[0].kind, "other");
        assert_eq!(reaction.memory_candidates[0].ttl_hint, "stable");
        assert_eq!(reaction.followups, vec!["next step"]);
    }

    #[test]
    fn drops_low_confidence_and_temporary_memory_candidates() {
        let reaction = AgentReaction {
            mood: PetMood::Focused,
            speech: String::new(),
            memory_candidates: vec![
                MemoryCandidate {
                    text: "User asked for a one-off reminder".into(),
                    importance: 4,
                    confidence: 5,
                    kind: "other".into(),
                    ttl_hint: "temporary".into(),
                    reason: "one-off task".into(),
                    tags: vec!["reminder".into()],
                },
                MemoryCandidate {
                    text: "User may like dashboards".into(),
                    importance: 4,
                    confidence: 2,
                    kind: "preference".into(),
                    ttl_hint: "evolving".into(),
                    reason: "uncertain inference".into(),
                    tags: vec!["preference".into()],
                },
            ],
            followups: vec![],
        }
        .sanitized("fallback speech");

        assert!(reaction.memory_candidates.is_empty());
    }

    #[test]
    fn fallback_never_guesses_mood_from_keywords() {
        let reaction = AgentReaction::fallback("Haha, but it failed");
        assert_eq!(reaction.mood, PetMood::Idle);
        assert!(reaction.memory_candidates.is_empty());
    }
}
