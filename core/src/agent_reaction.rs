//! Agent 对话收尾反应抽取。
//!
//! 本模块用 rig `Extractor<T>` 将一轮自由文本对话压缩为结构化反应：宠物情绪、
//! 可显示的简短话语，以及值得长期保存的记忆候选。模型负责语义判断，Rust
//! 负责字段校验、长度截断和失败兜底，避免回到关键词分类器。

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
const MAX_TAGS: usize = 6;
const MAX_TAG_CHARS: usize = 24;

/// 一轮对话结束后的结构化反应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgentReaction {
    /// 宠物最终情绪。
    pub mood: PetMood,
    /// 可选的一句简短收尾文本；为空时使用主回复。
    #[serde(default)]
    pub speech: String,
    /// 值得长期保存的记忆候选。
    #[serde(default)]
    pub memory_candidates: Vec<MemoryCandidate>,
    /// 可选的后续建议，当前只保留结构，不直接展示。
    #[serde(default)]
    pub followups: Vec<String>,
}

/// 长期记忆候选。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryCandidate {
    /// 一条自包含、可 grep 的事实或偏好。
    pub text: String,
    /// 重要度 1..=5；低于 3 的候选会被丢弃。
    pub importance: u8,
    /// 简短英文或中文标签。
    #[serde(default)]
    pub tags: Vec<String>,
}

impl AgentReaction {
    /// 无法抽取时的确定性兜底，不做关键词判断。
    pub fn fallback(reply: &str) -> Self {
        Self {
            mood: PetMood::Idle,
            speech: truncate_chars(reply, MAX_SPEECH_CHARS),
            memory_candidates: Vec::new(),
            followups: Vec::new(),
        }
    }

    /// 校验并收敛模型输出，保证后续 UI 和记忆写入边界稳定。
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
    fn sanitize(mut self) -> Option<Self> {
        self.text = truncate_chars(self.text.trim(), MAX_MEMORY_TEXT_CHARS);
        if self.text.is_empty() || self.importance < 3 {
            return None;
        }
        self.importance = self.importance.min(5);
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

/// 使用 rig Extractor 抽取结构化对话反应。
pub async fn extract_agent_reaction(
    ai_config: &AiConfig,
    user_msg: &str,
    ai_reply: &str,
    tool_events: &[String],
) -> Result<AgentReaction, String> {
    let http_client = rig::http_client::ReqwestClient::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;
    let client = anthropic::Client::builder()
        .api_key(&ai_config.api_key)
        .base_url(&ai_config.base_url)
        .http_client(http_client)
        .build()
        .map_err(|e| format!("创建 Anthropic reaction Client 失败: {e}"))?;

    let extractor = client
        .extractor::<AgentReaction>(ai_config.model.as_str())
        .preamble(REACTION_PREAMBLE)
        .max_tokens(1024)
        .retries(1)
        .build();

    let tool_text = if tool_events.is_empty() {
        "无工具调用".to_string()
    } else {
        tool_events.join("\n")
    };
    let input =
        format!("用户消息:\n{user_msg}\n\nAI最终回复:\n{ai_reply}\n\n本轮工具事件:\n{tool_text}");

    let start = std::time::Instant::now();
    let response = extractor
        .extract_with_usage(input)
        .await
        .map_err(|e| format!("抽取 AgentReaction 失败: {e}"))?;
    let elapsed = start.elapsed();
    let reaction = response.data.sanitized(ai_reply);
    debug!(
        mood = ?reaction.mood,
        memory_candidates = reaction.memory_candidates.len(),
        elapsed_ms = elapsed.as_millis(),
        "AgentReaction 抽取完成"
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

/// 抽取失败时记录并返回兜底结构。
pub fn fallback_agent_reaction(reply: &str, error: &str) -> AgentReaction {
    warn!(error, "AgentReaction 抽取失败，使用 idle 兜底");
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

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let keep = max.saturating_sub(3);
        format!("{}...", s.chars().take(keep).collect::<String>())
    }
}

const REACTION_PREAMBLE: &str = r#"
你负责把一轮桌宠 AI 对话收尾压缩为严格结构化 JSON。

判断原则：
- mood 表示宠物最后该表现出的情绪，而不是复述文本内容。
- 如果回复是普通说明或任务完成，优先 idle/focused；用户表达开心、庆祝或舞蹈成功可用 happy/excited。
- 如果工具失败、权限被阻止或 AI 明确无法完成，可用 confused。
- caring 用于安慰、鼓励、陪伴类回复；sleepy 只在睡眠/休息语境使用。
- memory_candidates 只保存长期有价值、之后可 grep 的事实、偏好、项目背景或用户明确要求记住的信息。
- 不保存一次性寒暄、普通问答步骤、临时错误、已经明显过期的信息。
- 每条 memory text 必须自包含，不要写“这个/上面/刚才”。
- importance 范围 1..5；只有 3 以上才会被保存。
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
                MemoryCandidate {
                    text: " 用户偏好 grep-first 记忆检索 ".into(),
                    importance: 5,
                    tags: vec!["Memory!".into(), "Preference".into()],
                },
                MemoryCandidate {
                    text: "too weak".into(),
                    importance: 1,
                    tags: vec![],
                },
            ],
            followups: vec![" 下一步可以继续 ".into()],
        }
        .sanitized("fallback speech");

        assert_eq!(reaction.speech, "fallback speech");
        assert_eq!(reaction.memory_candidates.len(), 1);
        assert_eq!(
            reaction.memory_candidates[0].text,
            "用户偏好 grep-first 记忆检索"
        );
        assert_eq!(
            reaction.memory_candidates[0].tags,
            vec!["memory", "preference"]
        );
        assert_eq!(reaction.followups, vec!["下一步可以继续"]);
    }

    #[test]
    fn fallback_never_guesses_mood_from_keywords() {
        let reaction = AgentReaction::fallback("哈哈，但是失败了");
        assert_eq!(reaction.mood, PetMood::Idle);
        assert!(reaction.memory_candidates.is_empty());
    }
}
