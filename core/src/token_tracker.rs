use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tracing::{debug, warn};

const MAX_TOKEN_SESSIONS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenCategory {
    Chat,
    Vision,
    ScreenSummary,
    MemoryAggregation,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl TokenUsage {
    pub fn is_empty(&self) -> bool {
        self.input_tokens == 0
            && self.output_tokens == 0
            && self.total_tokens == 0
            && self.cache_read_tokens == 0
            && self.cache_write_tokens == 0
    }
}

impl From<&rig::completion::Usage> for TokenUsage {
    fn from(usage: &rig::completion::Usage) -> Self {
        Self {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
            cache_read_tokens: usage.cached_input_tokens,
            cache_write_tokens: usage.cache_creation_input_tokens,
        }
    }
}

impl From<rig::completion::Usage> for TokenUsage {
    fn from(usage: rig::completion::Usage) -> Self {
        Self::from(&usage)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenRecord {
    pub timestamp: String,
    pub session_id: String,
    pub category: TokenCategory,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub elapsed_ms: Option<u64>,
    pub extra: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSession {
    pub session_id: String,
    pub started_at: String,
    pub ended_at: String,
    pub models: Vec<String>,
    pub record_count: u32,
    pub elapsed_ms_total: u64,
    pub chat_input_tokens: u64,
    pub chat_output_tokens: u64,
    pub chat_total_tokens: u64,
    pub chat_cache_read_tokens: u64,
    pub chat_cache_write_tokens: u64,
    pub vision_count: u32,
    pub vision_input_tokens: u64,
    pub vision_output_tokens: u64,
    pub vision_total_tokens: u64,
    pub screen_summary_count: u32,
    pub screen_summary_input_tokens: u64,
    pub screen_summary_output_tokens: u64,
    pub screen_summary_total_tokens: u64,
    pub memory_aggregation_count: u32,
    pub memory_aggregation_input_tokens: u64,
    pub memory_aggregation_output_tokens: u64,
    pub memory_aggregation_total_tokens: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSessions {
    pub sessions: Vec<TokenSession>,
}

impl TokenSession {
    pub fn from_record(record: &TokenRecord) -> Self {
        let mut session = Self {
            session_id: record.session_id.clone(),
            started_at: record.timestamp.clone(),
            ended_at: record.timestamp.clone(),
            models: vec![record.model.clone()],
            record_count: 0,
            elapsed_ms_total: 0,
            ..Default::default()
        };
        session.apply_record(record);
        session
    }

    pub fn apply_record(&mut self, record: &TokenRecord) {
        self.ended_at = record.timestamp.clone();
        self.record_count = self.record_count.saturating_add(1);
        self.elapsed_ms_total = self
            .elapsed_ms_total
            .saturating_add(record.elapsed_ms.unwrap_or(0));

        if !self.models.iter().any(|m| m == &record.model) {
            self.models.push(record.model.clone());
        }

        match record.category {
            TokenCategory::Chat => {
                self.chat_input_tokens += record.input_tokens;
                self.chat_output_tokens += record.output_tokens;
                self.chat_total_tokens += record.total_tokens;
                self.chat_cache_read_tokens += record.cache_read_tokens;
                self.chat_cache_write_tokens += record.cache_write_tokens;
            }
            TokenCategory::Vision => {
                self.vision_count = self.vision_count.saturating_add(1);
                self.vision_input_tokens += record.input_tokens;
                self.vision_output_tokens += record.output_tokens;
                self.vision_total_tokens += record.total_tokens;
            }
            TokenCategory::ScreenSummary => {
                self.screen_summary_count = self.screen_summary_count.saturating_add(1);
                self.screen_summary_input_tokens += record.input_tokens;
                self.screen_summary_output_tokens += record.output_tokens;
                self.screen_summary_total_tokens += record.total_tokens;
            }
            TokenCategory::MemoryAggregation => {
                self.memory_aggregation_count = self.memory_aggregation_count.saturating_add(1);
                self.memory_aggregation_input_tokens += record.input_tokens;
                self.memory_aggregation_output_tokens += record.output_tokens;
                self.memory_aggregation_total_tokens += record.total_tokens;
            }
        }
    }
}

impl TokenRecord {
    pub fn new(
        session_id: impl Into<String>,
        category: TokenCategory,
        model: impl Into<String>,
        usage: TokenUsage,
    ) -> Self {
        Self {
            timestamp: chrono::Local::now().to_rfc3339(),
            session_id: session_id.into(),
            category,
            model: model.into(),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
            cache_read_tokens: usage.cache_read_tokens,
            cache_write_tokens: usage.cache_write_tokens,
            elapsed_ms: None,
            extra: None,
        }
    }

    pub fn with_elapsed_ms(mut self, elapsed_ms: u64) -> Self {
        self.elapsed_ms = Some(elapsed_ms);
        self
    }

    pub fn with_extra(mut self, extra: impl Into<String>) -> Self {
        self.extra = Some(extra.into());
        self
    }
}

static TOKEN_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn new_session_id() -> String {
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
    format!("{now:x}")
}

pub fn token_usage_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取 HOME 目录".to_string())?;
    Ok(home.join(".ai-pad").join("logs").join("token_usage.jsonl"))
}

pub fn token_sessions_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取 HOME 目录".to_string())?;
    Ok(home
        .join(".ai-pad")
        .join("logs")
        .join("token_sessions.json"))
}

pub fn record_token_usage(record: &TokenRecord) {
    if record.total_tokens == 0
        && record.input_tokens == 0
        && record.output_tokens == 0
        && record.cache_read_tokens == 0
        && record.cache_write_tokens == 0
    {
        debug!(category = ?record.category, model = %record.model, "skip empty token usage");
        return;
    }

    let path = match token_usage_path() {
        Ok(path) => path,
        Err(e) => {
            warn!(error = %e, "token usage path unavailable");
            return;
        }
    };

    if let Err(e) = append_record(&path, record) {
        warn!(error = %e, path = ?path, "token usage write failed");
    }

    let sessions_path = match token_sessions_path() {
        Ok(path) => path,
        Err(e) => {
            warn!(error = %e, "token sessions path unavailable");
            return;
        }
    };

    if let Err(e) = update_sessions_file(&sessions_path, record) {
        warn!(error = %e, path = ?sessions_path, "token sessions update failed");
    }
}

pub fn append_record(path: &Path, record: &TokenRecord) -> Result<(), String> {
    let _guard = TOKEN_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|e| format!("token 写入锁中毒: {e}"))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 token 日志目录失败: {e}"))?;
    }

    let line = serde_json::to_string(record).map_err(|e| format!("序列化 token 记录失败: {e}"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("打开 token 日志失败: {e}"))?;
    writeln!(file, "{line}").map_err(|e| format!("写入 token 日志失败: {e}"))?;
    Ok(())
}

pub fn update_sessions_file(path: &Path, record: &TokenRecord) -> Result<(), String> {
    let _guard = TOKEN_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|e| format!("token 写入锁中毒: {e}"))?;

    let mut store = load_sessions(path)?;
    upsert_session(&mut store, record);
    save_sessions(path, &store)
}

pub fn load_sessions(path: &Path) -> Result<TokenSessions, String> {
    if !path.exists() {
        return Ok(TokenSessions::default());
    }

    let content = fs::read_to_string(path).map_err(|e| format!("读取 token 会话失败: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("解析 token 会话失败: {e}"))
}

pub fn save_sessions(path: &Path, store: &TokenSessions) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 token 会话目录失败: {e}"))?;
    }

    let json =
        serde_json::to_string_pretty(store).map_err(|e| format!("序列化 token 会话失败: {e}"))?;
    fs::write(path, json).map_err(|e| format!("写入 token 会话失败: {e}"))
}

pub fn upsert_session(store: &mut TokenSessions, record: &TokenRecord) {
    if let Some(session) = store
        .sessions
        .iter_mut()
        .find(|s| s.session_id == record.session_id)
    {
        session.apply_record(record);
    } else {
        store.sessions.push(TokenSession::from_record(record));
    }

    store.sessions.sort_by(|a, b| b.ended_at.cmp(&a.ended_at));
    if store.sessions.len() > MAX_TOKEN_SESSIONS {
        store.sessions.truncate(MAX_TOKEN_SESSIONS);
    }
}

pub fn parse_anthropic_usage(response: &Value) -> TokenUsage {
    let Some(usage) = response.get("usage") else {
        return TokenUsage::default();
    };

    let input_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cache_read_tokens = usage
        .get("cache_read_input_tokens")
        .or_else(|| usage.get("cached_input_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cache_write_tokens = usage
        .get("cache_creation_input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_tokens = usage
        .get("total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(input_tokens + output_tokens);

    TokenUsage {
        input_tokens,
        output_tokens,
        total_tokens,
        cache_read_tokens,
        cache_write_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn token_record_snapshot() {
        let record = TokenRecord {
            timestamp: "2026-05-13T12:00:00+08:00".into(),
            session_id: "session-1".into(),
            category: TokenCategory::Chat,
            model: "claude-sonnet-4-20250514".into(),
            input_tokens: 100,
            output_tokens: 20,
            total_tokens: 120,
            cache_read_tokens: 80,
            cache_write_tokens: 5,
            elapsed_ms: Some(1234),
            extra: Some("turn=1".into()),
        };
        insta::assert_yaml_snapshot!(record);
    }

    #[test]
    fn parse_anthropic_usage_reads_cache_fields() {
        let response = json!({
            "usage": {
                "input_tokens": 100,
                "output_tokens": 20,
                "cache_read_input_tokens": 30,
                "cache_creation_input_tokens": 40
            }
        });
        assert_eq!(
            parse_anthropic_usage(&response),
            TokenUsage {
                input_tokens: 100,
                output_tokens: 20,
                total_tokens: 120,
                cache_read_tokens: 30,
                cache_write_tokens: 40
            }
        );
    }

    #[test]
    fn append_record_writes_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token_usage.jsonl");
        let record = TokenRecord::new(
            "session-1",
            TokenCategory::Vision,
            "test-model",
            TokenUsage {
                input_tokens: 1,
                output_tokens: 2,
                total_tokens: 3,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            },
        );

        append_record(&path, &record).unwrap();
        let content = fs::read_to_string(path).unwrap();
        let line: TokenRecord = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(line.session_id, "session-1");
        assert_eq!(line.category, TokenCategory::Vision);
        assert_eq!(line.total_tokens, 3);
    }

    #[test]
    fn upsert_session_accumulates_by_category() {
        let mut store = TokenSessions::default();
        let chat = TokenRecord {
            timestamp: "2026-05-13T12:00:00+08:00".into(),
            session_id: "session-1".into(),
            category: TokenCategory::Chat,
            model: "model-a".into(),
            input_tokens: 10,
            output_tokens: 5,
            total_tokens: 15,
            cache_read_tokens: 3,
            cache_write_tokens: 1,
            elapsed_ms: Some(100),
            extra: None,
        };
        let vision = TokenRecord {
            timestamp: "2026-05-13T12:00:05+08:00".into(),
            session_id: "session-1".into(),
            category: TokenCategory::Vision,
            model: "model-b".into(),
            input_tokens: 100,
            output_tokens: 20,
            total_tokens: 120,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            elapsed_ms: Some(200),
            extra: None,
        };

        upsert_session(&mut store, &chat);
        upsert_session(&mut store, &vision);

        let session = &store.sessions[0];
        assert_eq!(session.record_count, 2);
        assert_eq!(session.elapsed_ms_total, 300);
        assert_eq!(session.chat_total_tokens, 15);
        assert_eq!(session.chat_cache_read_tokens, 3);
        assert_eq!(session.vision_count, 1);
        assert_eq!(session.vision_total_tokens, 120);
        assert_eq!(session.models, vec!["model-a", "model-b"]);
    }

    #[test]
    fn update_sessions_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token_sessions.json");
        let record = TokenRecord::new(
            "session-2",
            TokenCategory::ScreenSummary,
            "test-model",
            TokenUsage {
                input_tokens: 7,
                output_tokens: 8,
                total_tokens: 15,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            },
        )
        .with_elapsed_ms(50);

        update_sessions_file(&path, &record).unwrap();
        let store = load_sessions(&path).unwrap();
        assert_eq!(store.sessions.len(), 1);
        assert_eq!(store.sessions[0].session_id, "session-2");
        assert_eq!(store.sessions[0].screen_summary_count, 1);
        assert_eq!(store.sessions[0].screen_summary_total_tokens, 15);
    }
}
