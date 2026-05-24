//! Token 用量追踪与统计
//!
//! 记录每次 AI 调用的 token 消耗到 JSONL 文件，按会话聚合统计。
//! 支持按日期、类别（Chat/Vision/ScreenSummary/MemoryAggregation）查询汇总。
//! 数据持久化到 ~/.bitcat/logs/ 目录，供设置界面展示用量统计。

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tracing::{debug, warn};

const MAX_TOKEN_SESSIONS: usize = 200;

/// token 用量类别：聊天对话、截图视觉分析、屏幕摘要、记忆聚合
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenCategory {
    Chat,
    Vision,
    ScreenSummary,
    MemoryAggregation,
}

/// 单次 API 调用的 token 用量统计
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

/// 一条完整的 token 使用记录，写入 JSONL 文件
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

/// 按会话聚合的 token 用量汇总
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenTotals {
    pub record_count: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub chat_total_tokens: u64,
    pub vision_total_tokens: u64,
    pub screen_summary_total_tokens: u64,
    pub memory_aggregation_total_tokens: u64,
}

impl TokenTotals {
    pub fn add_record(&mut self, record: &TokenRecord) {
        self.record_count = self.record_count.saturating_add(1);
        self.input_tokens += record.input_tokens;
        self.output_tokens += record.output_tokens;
        self.total_tokens += record.total_tokens;
        self.cache_read_tokens += record.cache_read_tokens;
        self.cache_write_tokens += record.cache_write_tokens;

        match record.category {
            TokenCategory::Chat => self.chat_total_tokens += record.total_tokens,
            TokenCategory::Vision => self.vision_total_tokens += record.total_tokens,
            TokenCategory::ScreenSummary => self.screen_summary_total_tokens += record.total_tokens,
            TokenCategory::MemoryAggregation => {
                self.memory_aggregation_total_tokens += record.total_tokens
            }
        }
    }
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

/// 生成基于 UTC 纳秒时间戳的十六进制会话 ID
pub fn new_session_id() -> String {
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
    format!("{now:x}")
}

/// 返回 token 用量明细文件路径 `~/.bitcat/logs/token_usage.jsonl`
pub fn token_usage_path() -> Result<PathBuf, String> {
    Ok(crate::logging::log_dir()?.join("token_usage.jsonl"))
}

/// 返回 token 会话聚合文件路径 `~/.bitcat/logs/token_sessions.json`
pub fn token_sessions_path() -> Result<PathBuf, String> {
    Ok(crate::logging::log_dir()?.join("token_sessions.json"))
}

/// 记录一条 token 用量：追加到 JSONL 明细 + 更新会话聚合文件。零 token 记录自动跳过。
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

/// 向 JSONL 文件追加一条 token 记录（线程安全，通过全局 Mutex 序列化写入）
pub fn append_record(path: &Path, record: &TokenRecord) -> Result<(), String> {
    crate::logging::append_jsonl_path(path, record)
}

/// 用一条记录更新会话聚合文件：存在则累加，不存在则新建，保留最近 200 个会话
pub fn update_sessions_file(path: &Path, record: &TokenRecord) -> Result<(), String> {
    let _guard = TOKEN_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|e| format!("token 写入锁中毒: {e}"))?;

    let mut store = load_sessions(path)?;
    upsert_session(&mut store, record);
    save_sessions(path, &store)
}

/// 从磁盘加载会话聚合数据，文件不存在返回空
pub fn load_sessions(path: &Path) -> Result<TokenSessions, String> {
    if !path.exists() {
        return Ok(TokenSessions::default());
    }

    let content = fs::read_to_string(path).map_err(|e| format!("读取 token 会话失败: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("解析 token 会话失败: {e}"))
}

/// 将会话聚合数据序列化写入磁盘（pretty-print JSON）
pub fn save_sessions(path: &Path, store: &TokenSessions) -> Result<(), String> {
    crate::logging::write_json_atomic(path, store)
}

/// 按 session_id 查找并累加记录，不存在则新建；按时间降序排列，超出上限截断
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

/// 从 JSONL 文件逐行读取所有 token 用量记录
pub fn read_usage_records(path: &Path) -> Result<Vec<TokenRecord>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path).map_err(|e| format!("打开 token 明细失败: {e}"))?;
    let reader = std::io::BufReader::new(file);
    let mut records = Vec::new();

    for (idx, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| format!("读取 token 明细第 {} 行失败: {e}", idx + 1))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record: TokenRecord = serde_json::from_str(trimmed)
            .map_err(|e| format!("解析 token 明细第 {} 行失败: {e}", idx + 1))?;
        records.push(record);
    }

    Ok(records)
}

/// 按日期过滤并汇总 token 用量，返回当日各类别的 token 总计
pub fn totals_for_date(path: &Path, date: chrono::NaiveDate) -> Result<TokenTotals, String> {
    let date_prefix = date.format("%Y-%m-%d").to_string();
    let mut totals = TokenTotals::default();
    for record in read_usage_records(path)? {
        if record.timestamp.starts_with(&date_prefix) {
            totals.add_record(&record);
        }
    }
    Ok(totals)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn totals_for_date_sums_matching_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token_usage.jsonl");
        let records = [
            TokenRecord {
                timestamp: "2026-05-13T12:00:00+08:00".into(),
                session_id: "s1".into(),
                category: TokenCategory::Chat,
                model: "model".into(),
                input_tokens: 10,
                output_tokens: 5,
                total_tokens: 15,
                cache_read_tokens: 2,
                cache_write_tokens: 1,
                elapsed_ms: None,
                extra: None,
            },
            TokenRecord {
                timestamp: "2026-05-13T12:01:00+08:00".into(),
                session_id: "s2".into(),
                category: TokenCategory::Vision,
                model: "model".into(),
                input_tokens: 100,
                output_tokens: 20,
                total_tokens: 120,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                elapsed_ms: None,
                extra: None,
            },
            TokenRecord {
                timestamp: "2026-05-14T12:01:00+08:00".into(),
                session_id: "s3".into(),
                category: TokenCategory::ScreenSummary,
                model: "model".into(),
                input_tokens: 1000,
                output_tokens: 200,
                total_tokens: 1200,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                elapsed_ms: None,
                extra: None,
            },
        ];

        for record in &records {
            append_record(&path, record).unwrap();
        }

        let totals =
            totals_for_date(&path, chrono::NaiveDate::from_ymd_opt(2026, 5, 13).unwrap()).unwrap();
        assert_eq!(totals.record_count, 2);
        assert_eq!(totals.input_tokens, 110);
        assert_eq!(totals.output_tokens, 25);
        assert_eq!(totals.total_tokens, 135);
        assert_eq!(totals.cache_read_tokens, 2);
        assert_eq!(totals.cache_write_tokens, 1);
        assert_eq!(totals.chat_total_tokens, 15);
        assert_eq!(totals.vision_total_tokens, 120);
        assert_eq!(totals.screen_summary_total_tokens, 0);
    }
}
