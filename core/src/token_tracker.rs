use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tracing::{debug, warn};

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
}
