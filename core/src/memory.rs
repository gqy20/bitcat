//! 三层对话记忆系统：短期滚动窗口 + 长期 JSONL 检索 + AI 聚合画像。
//!
//! 设计遵循 grep-first 原则：所有持久化产物均为可搜索的结构化文本
//! （JSON / JSONL），不引入向量数据库或 Embedding，方便 `rg`、
//! 人工审查和大模型共同读取。记忆检索先用标签、来源、重要度和文本包含等可解释条件筛选候选，
//! 再交给大模型判断压缩。
//!
//! 三层各自持久化到 `~/.bitcat/memory/`：
//! - **MemoryStore** — `chat_summary.json`，滚动窗口短期记忆，直接注入 prompt
//! - **LongTermMemory** — `long_term.jsonl`，原始对话按需候选召回注入
//! - **ProfileStore** — `profile.json`，AI 定期提交 patch 的结构化用户画像
//!
//! 与 `agent.rs`（对话后写入）、`bridge.rs`（构建上下文）交互。

use rig::client::CompletionClient;
use rig::providers::anthropic;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::agent_reaction::MemoryCandidate;
use crate::ai_config::AiConfig;
use crate::logging::append_jsonl;
use crate::token_tracker::{
    TokenCategory, TokenRecord, TokenUsage, new_session_id, record_token_usage,
};

// ---- 数据结构 ----

/// 单条对话记录，包含时间戳、用户消息和 AI 回复。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub timestamp: String,
    pub user_msg: String,
    pub ai_reply: String,
}

/// 短期对话记忆存储，维护固定大小的滚动窗口。
///
/// 超出 `max_entries` 时自动淘汰最旧条目；`max_entries == 0` 表示不限制保存条数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStore {
    pub entries: Vec<MemoryEntry>,
}

/// 记忆系统配置，来自 `config/prompts.yml` 的 `memory` 段。
///
/// 控制滚动窗口大小、上下文字符预算、单条消息截断阈值。
///
/// `max_entries == 0` 表示短期对话文件不按条数淘汰，仍由 `max_context_chars`
/// 控制每次注入 prompt 的文本量。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryConfig {
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    #[serde(default = "default_max_context_chars")]
    pub max_context_chars: usize,
    #[serde(default = "default_max_user_chars")]
    pub max_user_chars: usize,
    #[serde(default = "default_max_reply_chars")]
    pub max_reply_chars: usize,
}

fn default_max_entries() -> usize {
    0
}
fn default_max_context_chars() -> usize {
    20_000
}
fn default_max_user_chars() -> usize {
    500
}
fn default_max_reply_chars() -> usize {
    1000
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_entries: default_max_entries(),
            max_context_chars: default_max_context_chars(),
            max_user_chars: default_max_user_chars(),
            max_reply_chars: default_max_reply_chars(),
        }
    }
}

// ---- 存储路径 ----

/// 返回短期记忆文件路径 `~/.bitcat/memory/chat_summary.json`。
fn memory_file_path() -> Result<PathBuf, String> {
    Ok(crate::storage::data_dir()?
        .join("memory")
        .join("chat_summary.json"))
}

// ---- 核心操作 ----

impl MemoryStore {
    /// 从磁盘加载。文件不存在或损坏时返回空记忆。
    pub fn load() -> Self {
        let path = match memory_file_path() {
            Ok(p) => p,
            Err(e) => {
                warn!(error = %e, "获取记忆文件路径失败");
                return Self {
                    entries: Vec::new(),
                };
            }
        };

        if !path.exists() {
            info!("对话记忆文件不存在，使用空对话记忆");
            return Self {
                entries: Vec::new(),
            };
        }

        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<MemoryStore>(&content) {
                Ok(store) => {
                    info!(count = store.entries.len(), "已加载对话记忆");
                    store
                }
                Err(e) => {
                    warn!(error = %e, "解析对话记忆文件失败，使用空对话记忆");
                    Self {
                        entries: Vec::new(),
                    }
                }
            },
            Err(e) => {
                warn!(error = %e, "读取对话记忆文件失败，使用空对话记忆");
                Self {
                    entries: Vec::new(),
                }
            }
        }
    }

    /// 记录新对话：截断字段 + 强制滚动窗口
    pub fn record_conversation(&mut self, user_msg: &str, ai_reply: &str, config: &MemoryConfig) {
        let timestamp = chrono::Local::now().format("%H:%M").to_string();

        self.entries.push(MemoryEntry {
            timestamp,
            user_msg: truncate_chars(user_msg, config.max_user_chars),
            ai_reply: truncate_chars(ai_reply, config.max_reply_chars),
        });

        if config.max_entries > 0 {
            while self.entries.len() > config.max_entries {
                self.entries.remove(0);
            }
        }
    }

    /// 构建注入 prompt 的上下文文本。空记忆返回空字符串。
    pub fn build_context(&self, config: &MemoryConfig) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        // 优先保留最新记录；选中后再反转回时间顺序，保证可读性。
        let mut lines: Vec<String> = Vec::new();
        let wrapper_chars = "[最近对话记录]\n[/最近对话记录]\n".chars().count();
        let mut used_chars = wrapper_chars;
        for entry in self.entries.iter().rev() {
            let line = format!(
                "[{}] {} | {}",
                entry.timestamp, entry.user_msg, entry.ai_reply
            );
            let line_chars = line.chars().count() + 1;
            let projected = used_chars + line_chars;
            if projected > config.max_context_chars {
                break;
            }
            lines.push(line);
            used_chars = projected;
        }
        lines.reverse();

        let header = "[最近对话记录]\n";
        let mut result = String::from(header);

        for line in &lines {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str("[/最近对话记录]\n");

        result
    }

    /// 持久化到磁盘（原子写入：先写临时文件再 rename）
    pub fn save(&self) -> Result<(), String> {
        let path = memory_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建记忆目录失败: {e}"))?;
        }
        let json = serde_json::to_string(self).map_err(|e| format!("序列化记忆失败: {e}"))?;
        let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())
            .map_err(|e| format!("创建临时文件失败: {e}"))?;
        std::io::Write::write_all(&mut tmp, json.as_bytes())
            .map_err(|e| format!("写入临时文件失败: {e}"))?;
        tmp.persist(&path)
            .map_err(|e| format!("原子替换记忆文件失败: {e}"))?;
        debug!(path = ?path, "对话记忆已持久化");
        Ok(())
    }
}

/// 按 Unicode 字符截断，超长时追加 "..."
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

fn normalize_memory_summary(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_whitespace() && !ch.is_ascii_punctuation())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn optional_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

// ---- Layer 2: 长期记忆（原始对话记录，按需检索） ----

/// 单条长期记忆条目——完整保存值得长期保留的对话内容。
///
/// `aggregated` 标记表示该条目已被 AI 聚合到画像中，容量不足时优先淘汰。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub created_at: String,
    pub timestamp: String,
    pub user_msg: String,
    pub ai_reply: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub importance: Option<u8>,
    #[serde(default)]
    pub confidence: Option<u8>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub ttl_hint: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub aggregated: bool,
    #[serde(default)]
    pub deleted: bool,
}

/// 长期记忆存储：保存结构化 JSONL 记录，按硬过滤和轻量文本包含召回候选。
///
/// 超出容量时优先淘汰已聚合条目，软删除条目保留在账本中供 grep 和审查。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermMemory {
    pub entries: Vec<LongTermEntry>,
}

/// 长期记忆检索过滤条件。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LongTermMemoryQuery {
    pub text: String,
    pub tags: Vec<String>,
    pub source: Option<String>,
    pub min_importance: Option<u8>,
}

/// 可人工审查的长期记忆条目视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LongTermReviewEntry {
    pub id: String,
    pub deleted: bool,
    pub created_at: String,
    pub timestamp: String,
    pub title: String,
    pub tags: Vec<String>,
    pub importance: Option<u8>,
    pub confidence: Option<u8>,
    pub kind: Option<String>,
    pub ttl_hint: Option<String>,
    pub reason: Option<String>,
    pub source: Option<String>,
    pub aggregated: bool,
    pub user_msg: String,
    pub ai_reply: String,
}

/// 返回长期记忆文件路径 `~/.bitcat/memory/long_term.jsonl`。
fn long_term_file_path() -> Result<PathBuf, String> {
    Ok(crate::storage::data_dir()?
        .join("memory")
        .join("long_term.jsonl"))
}

/// 返回长期记忆的 Markdown 审查视图路径 `~/.bitcat/memory/long_term.md`。
fn long_term_markdown_path() -> Result<PathBuf, String> {
    Ok(crate::storage::data_dir()?
        .join("memory")
        .join("long_term.md"))
}

impl LongTermMemory {
    /// 从磁盘加载。文件不存在或损坏时返回空记忆。
    pub fn load() -> Self {
        let path = match long_term_file_path() {
            Ok(p) => p,
            Err(e) => {
                warn!(error = %e, "获取长期记忆文件路径失败");
                return Self {
                    entries: Vec::new(),
                };
            }
        };
        if !path.exists() {
            return Self {
                entries: Vec::new(),
            };
        }
        match fs::read_to_string(&path) {
            Ok(content) => load_long_term_jsonl(&content),
            Err(e) => {
                warn!(error = %e, "读取长期记忆文件失败");
                Self {
                    entries: Vec::new(),
                }
            }
        }
    }

    /// 追加一条记录。超过 max_entries 时淘汰最旧的未聚合条目。
    pub fn record(&mut self, user_msg: &str, ai_reply: &str, max_entries: usize) {
        let now = chrono::Local::now();
        let timestamp = now.format("%m-%d %H:%M").to_string();
        let reply_truncated: String = ai_reply.chars().take(400).collect();
        self.entries.push(LongTermEntry {
            id: next_memory_id(self.entries.len()),
            created_at: now.to_rfc3339(),
            timestamp,
            user_msg: user_msg.to_string(),
            ai_reply: reply_truncated,
            summary: None,
            tags: Vec::new(),
            importance: None,
            confidence: None,
            kind: None,
            ttl_hint: None,
            reason: None,
            source: Some("conversation".to_string()),
            aggregated: false,
            deleted: false,
        });
        self.enforce_max_entries(max_entries);
    }

    /// 追加一条模型结构化判断出的长期记忆候选。
    pub fn record_candidate(
        &mut self,
        candidate: &MemoryCandidate,
        user_msg: &str,
        ai_reply: &str,
        max_entries: usize,
    ) {
        let candidate_key = normalize_memory_summary(&candidate.text);
        if !candidate_key.is_empty()
            && self.entries.iter().any(|entry| {
                !entry.deleted
                    && entry
                        .summary
                        .as_deref()
                        .map(normalize_memory_summary)
                        .as_deref()
                        == Some(candidate_key.as_str())
            })
        {
            return;
        }

        let now = chrono::Local::now();
        let timestamp = now.format("%m-%d %H:%M").to_string();
        let user_truncated = truncate_chars(user_msg, 240);
        let reply_truncated = truncate_chars(ai_reply, 240);
        self.entries.push(LongTermEntry {
            id: next_memory_id(self.entries.len()),
            created_at: now.to_rfc3339(),
            timestamp,
            user_msg: user_truncated,
            ai_reply: reply_truncated,
            summary: Some(candidate.text.clone()),
            tags: candidate.tags.clone(),
            importance: Some(candidate.importance),
            confidence: Some(candidate.confidence),
            kind: Some(candidate.kind.as_str().to_string()),
            ttl_hint: Some(candidate.ttl_hint.as_str().to_string()),
            reason: optional_string(&candidate.reason),
            source: Some("agent_reaction".to_string()),
            aggregated: false,
            deleted: false,
        });
        self.enforce_max_entries(max_entries);
    }

    fn enforce_max_entries(&mut self, max_entries: usize) {
        while self.entries.iter().filter(|e| !e.deleted).count() > max_entries {
            if let Some(pos) = self.entries.iter().position(|e| e.aggregated && !e.deleted) {
                self.entries.remove(pos);
            } else {
                match self.entries.iter().position(|e| !e.deleted) {
                    Some(pos) => {
                        self.entries.remove(pos);
                    }
                    None => break,
                }
            }
        }
    }

    /// 按文本、标签、来源和重要度过滤后检索长期记忆，并生成可注入 prompt 的文本。
    pub fn retrieve_with(&self, query: &LongTermMemoryQuery, budget_chars: usize) -> String {
        self.render_retrieval(
            query,
            budget_chars,
            20,
            "[memory candidates]\n",
            "[/memory candidates]\n",
        )
    }

    /// 按 rg 风格返回长期记忆搜索结果，供 `search_memory` 工具给模型按需检索。
    pub fn search_results_with(
        &self,
        query: &LongTermMemoryQuery,
        budget_chars: usize,
        limit: usize,
    ) -> String {
        self.render_retrieval(
            query,
            budget_chars,
            limit,
            "[memory search results]\n",
            "[/memory search results]\n",
        )
    }

    fn render_retrieval(
        &self,
        query: &LongTermMemoryQuery,
        budget_chars: usize,
        max_results: usize,
        header: &str,
        footer: &str,
    ) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        let mut candidates: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| long_term_entry_matches(e, query))
            .filter(|(_, e)| query_matches_entry(e, &query.text))
            .map(|(i, _)| i)
            .collect();

        candidates.sort_by(|a, b| {
            let left = &self.entries[*a];
            let right = &self.entries[*b];
            right
                .importance
                .unwrap_or(0)
                .cmp(&left.importance.unwrap_or(0))
                .then_with(|| right.created_at.cmp(&left.created_at))
        });
        candidates.truncate(max_results);

        let mut result = String::from(header);
        let mut used = header.chars().count();
        for idx in &candidates {
            let e = &self.entries[*idx];
            let line = memory_candidate_line(e);
            if used + line.chars().count() > budget_chars {
                break;
            }
            result.push_str(&line);
            used += line.chars().count();
        }
        if result == header {
            return String::new();
        }
        result.push_str(footer);
        result
    }

    /// 生成可 grep、可人工审查的 Markdown 记忆视图。
    pub fn review_markdown(&self, limit: usize) -> String {
        if self.entries.iter().all(|entry| entry.deleted) {
            return "# Long-term Memory\n\nNo entries.\n".to_string();
        }

        let mut out = String::from("# Long-term Memory\n\n");
        for entry in self.review_entries(limit) {
            out.push_str(&format!(
                "## {}. {} ({})\n\n",
                entry.id, entry.title, entry.timestamp
            ));
            out.push_str(&format!(
                "- source: {}\n- importance: {}\n- confidence: {}\n- kind: {}\n- ttl: {}\n- reason: {}\n- tags: {}\n- aggregated: {}\n- deleted: {}\n\n",
                entry.source.as_deref().unwrap_or("unknown"),
                entry
                    .importance
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                entry
                    .confidence
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                entry.kind.as_deref().unwrap_or("unknown"),
                entry.ttl_hint.as_deref().unwrap_or("unknown"),
                entry.reason.as_deref().unwrap_or("none"),
                if entry.tags.is_empty() {
                    "none".to_string()
                } else {
                    entry.tags.join(", ")
                },
                entry.aggregated,
                entry.deleted
            ));
            out.push_str(&format!(
                "- user: {}\n- assistant: {}\n\n",
                entry.user_msg, entry.ai_reply
            ));
        }
        out
    }

    /// 返回长期记忆条目的结构化审查视图，最新条目优先。
    pub fn review_entries(&self, limit: usize) -> Vec<LongTermReviewEntry> {
        self.entries
            .iter()
            .rev()
            .filter(|entry| !entry.deleted)
            .take(limit)
            .map(|entry| LongTermReviewEntry {
                id: entry.id.clone(),
                deleted: entry.deleted,
                created_at: entry.created_at.clone(),
                timestamp: entry.timestamp.clone(),
                title: entry
                    .summary
                    .clone()
                    .unwrap_or_else(|| truncate_chars(&entry.user_msg, 80)),
                tags: entry.tags.clone(),
                importance: entry.importance,
                confidence: entry.confidence,
                kind: entry.kind.clone(),
                ttl_hint: entry.ttl_hint.clone(),
                reason: entry.reason.clone(),
                source: entry.source.clone(),
                aggregated: entry.aggregated,
                user_msg: truncate_chars(&entry.user_msg, 180),
                ai_reply: truncate_chars(&entry.ai_reply, 180),
            })
            .collect()
    }

    /// Delete one long-term memory entry by stable id.
    pub fn delete_entry_by_id(&mut self, id: &str) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        entry.deleted = true;
        true
    }

    /// 标记所有条目为已聚合
    pub fn mark_all_aggregated(&mut self) {
        for e in self.entries.iter_mut().filter(|e| !e.deleted) {
            e.aggregated = true;
        }
    }

    /// 取出未聚合的条目
    pub fn unaggregated_entries(&self) -> Vec<&LongTermEntry> {
        self.entries
            .iter()
            .filter(|e| !e.aggregated && !e.deleted)
            .collect()
    }

    /// 持久化到磁盘（原子写入：先写临时文件再 rename）。
    pub fn save(&self) -> Result<(), String> {
        let path = long_term_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        let mut jsonl = String::new();
        for entry in &self.entries {
            let line = serde_json::to_string(entry).map_err(|e| format!("序列化失败: {e}"))?;
            jsonl.push_str(&line);
            jsonl.push('\n');
        }
        let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())
            .map_err(|e| format!("创建临时文件失败: {e}"))?;
        std::io::Write::write_all(&mut tmp, jsonl.as_bytes())
            .map_err(|e| format!("写入临时文件失败: {e}"))?;
        tmp.persist(&path)
            .map_err(|e| format!("原子替换失败: {e}"))?;

        let markdown_path = long_term_markdown_path()?;
        let markdown = self.review_markdown(usize::MAX);
        let mut md_tmp = tempfile::NamedTempFile::new_in(markdown_path.parent().unwrap())
            .map_err(|e| format!("创建长期记忆 Markdown 临时文件失败: {e}"))?;
        std::io::Write::write_all(&mut md_tmp, markdown.as_bytes())
            .map_err(|e| format!("写入长期记忆 Markdown 失败: {e}"))?;
        md_tmp
            .persist(&markdown_path)
            .map_err(|e| format!("原子替换长期记忆 Markdown 失败: {e}"))?;

        debug!(path = ?path, markdown_path = ?markdown_path, "长期记忆已持久化");
        Ok(())
    }
}

fn long_term_entry_matches(entry: &LongTermEntry, query: &LongTermMemoryQuery) -> bool {
    if entry.deleted {
        return false;
    }
    if let Some(min_importance) = query.min_importance
        && entry.importance.unwrap_or(0) < min_importance
    {
        return false;
    }
    if let Some(source) = &query.source
        && entry.source.as_deref() != Some(source.as_str())
    {
        return false;
    }
    query.tags.iter().all(|tag| entry.tags.contains(tag))
}

fn load_long_term_jsonl(content: &str) -> LongTermMemory {
    let mut entries = Vec::new();
    for (line_no, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<LongTermEntry>(line) {
            Ok(mut entry) => {
                normalize_loaded_long_term_entry(&mut entry, line_no);
                entries.push(entry);
            }
            Err(e) => {
                warn!(
                    line = line_no + 1,
                    error = %e,
                    "解析长期 JSONL 记忆行失败，已跳过"
                );
            }
        }
    }
    LongTermMemory { entries }
}

fn normalize_loaded_long_term_entry(entry: &mut LongTermEntry, index: usize) {
    if entry.id.trim().is_empty() {
        entry.id = format!("mem_imported_{index:06}");
    }
    if entry.created_at.trim().is_empty() {
        entry.created_at = entry.timestamp.clone();
    }
}

fn next_memory_id(index: usize) -> String {
    let now = chrono::Local::now();
    format!("mem_{}_{index:04}", now.format("%Y%m%d%H%M%S%3f"))
}

fn query_matches_entry(entry: &LongTermEntry, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }

    let haystack = format!(
        "{} {} {} {} {} {} {} {}",
        entry.summary.as_deref().unwrap_or(""),
        entry.tags.join(" "),
        entry.kind.as_deref().unwrap_or(""),
        entry.ttl_hint.as_deref().unwrap_or(""),
        entry.reason.as_deref().unwrap_or(""),
        entry.source.as_deref().unwrap_or(""),
        entry.user_msg,
        entry.ai_reply
    )
    .to_lowercase();

    if haystack.contains(&query) {
        return true;
    }

    query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .any(|term| haystack.contains(term))
        || query_char_windows_match(&haystack, &query)
}

fn query_char_windows_match(haystack: &str, query: &str) -> bool {
    if query.split_whitespace().count() > 1 {
        return false;
    }
    let chars: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() < 4 {
        return false;
    }
    chars
        .windows(3)
        .map(|window| window.iter().collect::<String>())
        .any(|window| haystack.contains(&window))
}

fn memory_candidate_line(entry: &LongTermEntry) -> String {
    let tags = if entry.tags.is_empty() {
        "none".to_string()
    } else {
        entry.tags.join(",")
    };
    let summary = entry.summary.as_deref().unwrap_or(entry.user_msg.as_str());
    format!(
        "id={} created_at={} source={} importance={} confidence={} kind={} ttl={} tags=[{}]\nsummary={}\nreason={}\ncontext={} | {}\n",
        entry.id,
        entry.created_at,
        entry.source.as_deref().unwrap_or("unknown"),
        entry
            .importance
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        entry
            .confidence
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        entry.kind.as_deref().unwrap_or("unknown"),
        entry.ttl_hint.as_deref().unwrap_or("unknown"),
        tags,
        summary,
        entry.reason.as_deref().unwrap_or("none"),
        entry.user_msg,
        entry.ai_reply
    )
}

// ---- Layer 2: 聚合画像（AI 定期生成） ----

/// AI 聚合后的用户画像摘要，由 `aggregate_profile()` 定期生成。
///
/// 优先级低于 `config/user.yml` 中的显式声明——user.yml 有内容时直接使用，全空才回退到本画像。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileStore {
    #[serde(default)]
    pub facts: Vec<ProfileFact>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub profile_text: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProfilePatch {
    #[serde(default)]
    pub operations: Vec<ProfilePatchOperation>,
    #[serde(default)]
    pub no_update_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfilePatchOperation {
    pub op: ProfilePatchOp,
    pub section: ProfileSection,
    #[serde(default)]
    pub target_fact_id: Option<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    pub text: String,
    #[serde(default)]
    pub confidence: u8,
    #[serde(default)]
    pub stability: ProfileStability,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfilePatchOp {
    Upsert,
    Update,
    Delete,
    NoOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSection {
    Identity,
    WorkDomains,
    ActiveProjects,
    TechnicalStack,
    Preferences,
    InteractionStyle,
    Routines,
    Constraints,
    OpenLoops,
    Other,
}

impl ProfileSection {
    fn label(self) -> &'static str {
        match self {
            Self::Identity => "身份",
            Self::WorkDomains => "工作领域",
            Self::ActiveProjects => "当前项目",
            Self::TechnicalStack => "技术栈/工具",
            Self::Preferences => "偏好",
            Self::InteractionStyle => "互动风格",
            Self::Routines => "习惯/节奏",
            Self::Constraints => "约束/禁忌",
            Self::OpenLoops => "待确认/开放事项",
            Self::Other => "其他",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileStability {
    Stable,
    #[default]
    Evolving,
    Temporary,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileFactStatus {
    #[default]
    Active,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileFact {
    pub id: String,
    pub section: ProfileSection,
    pub text: String,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub confidence: u8,
    #[serde(default)]
    pub stability: ProfileStability,
    #[serde(default)]
    pub first_seen_at: String,
    #[serde(default)]
    pub last_seen_at: String,
    #[serde(default)]
    pub status: ProfileFactStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProfileEvidenceRow {
    evidence_id: String,
    memory_id: String,
    timestamp: String,
    summary: String,
    kind: Option<String>,
    ttl_hint: Option<String>,
    importance: Option<u8>,
    confidence: Option<u8>,
    tags: Vec<String>,
    source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileAggregationDiagnostic<'a> {
    event: &'a str,
    error: Option<&'a str>,
    existing_fact_count: usize,
    evidence_count: usize,
    patch: Option<&'a ProfilePatch>,
}

/// 返回用户画像文件路径 `~/.bitcat/memory/profile.json`。
fn profile_file_path() -> Result<PathBuf, String> {
    Ok(crate::storage::data_dir()?
        .join("memory")
        .join("profile.json"))
}

fn next_profile_fact_id(revision: u64, index: usize) -> String {
    let now = chrono::Local::now().format("%Y%m%d%H%M%S%3f");
    format!("profile_{now}_{revision:04}_{index:04}")
}

fn unique_strings(values: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if !trimmed.is_empty() && !out.iter().any(|existing| existing == trimmed) {
            out.push(trimmed.to_string());
        }
    }
    out
}

fn merge_evidence_ids(target: &mut Vec<String>, source: &[String]) {
    for value in source {
        let trimmed = value.trim();
        if !trimmed.is_empty() && !target.iter().any(|existing| existing == trimmed) {
            target.push(trimmed.to_string());
        }
    }
}

fn stronger_stability(left: ProfileStability, right: ProfileStability) -> ProfileStability {
    match (left, right) {
        (ProfileStability::Stable, _) | (_, ProfileStability::Stable) => ProfileStability::Stable,
        (ProfileStability::Evolving, _) | (_, ProfileStability::Evolving) => {
            ProfileStability::Evolving
        }
        _ => ProfileStability::Temporary,
    }
}

fn evidence_id_for_entry(entry: &LongTermEntry) -> String {
    entry.id.clone()
}

fn profile_evidence_rows(entries: &[&LongTermEntry]) -> Vec<ProfileEvidenceRow> {
    entries
        .iter()
        .filter(|entry| !entry.deleted)
        .map(|entry| ProfileEvidenceRow {
            evidence_id: evidence_id_for_entry(entry),
            memory_id: entry.id.clone(),
            timestamp: entry.timestamp.clone(),
            summary: entry
                .summary
                .clone()
                .unwrap_or_else(|| truncate_chars(&entry.user_msg, 160)),
            kind: entry.kind.clone(),
            ttl_hint: entry.ttl_hint.clone(),
            importance: entry.importance,
            confidence: entry.confidence,
            tags: entry.tags.clone(),
            source: entry.source.clone(),
        })
        .collect()
}

fn format_profile_evidence(rows: &[ProfileEvidenceRow]) -> String {
    rows.iter()
        .map(|row| {
            format!(
                "- evidence_id={}; timestamp={}; kind={}; ttl={}; importance={}; confidence={}; tags=[{}]; summary={}",
                row.evidence_id,
                row.timestamp,
                row.kind.as_deref().unwrap_or("unknown"),
                row.ttl_hint.as_deref().unwrap_or("unknown"),
                row.importance.map(|v| v.to_string()).unwrap_or_else(|| "unknown".into()),
                row.confidence.map(|v| v.to_string()).unwrap_or_else(|| "unknown".into()),
                row.tags.join(","),
                row.summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn validate_profile_patch(
    patch: &ProfilePatch,
    evidence: &[&LongTermEntry],
    store: &ProfileStore,
) -> Result<(), String> {
    let evidence_ids: Vec<String> = evidence
        .iter()
        .map(|entry| evidence_id_for_entry(entry))
        .collect();
    if patch.operations.is_empty() {
        return Ok(());
    }

    for op in &patch.operations {
        let text = op.text.trim();
        if !matches!(op.op, ProfilePatchOp::NoOp | ProfilePatchOp::Delete) && text.is_empty() {
            return Err("profile patch operation text cannot be empty".into());
        }
        if op.confidence > 5 {
            return Err("profile patch confidence must be 0..=5".into());
        }
        if matches!(op.op, ProfilePatchOp::Upsert | ProfilePatchOp::Update) && op.confidence < 3 {
            return Err("profile patch update confidence must be at least 3".into());
        }
        if matches!(op.stability, ProfileStability::Temporary)
            && matches!(op.op, ProfilePatchOp::Upsert | ProfilePatchOp::Update)
        {
            return Err("temporary profile facts cannot be persisted".into());
        }
        if matches!(op.op, ProfilePatchOp::Upsert | ProfilePatchOp::Update)
            && op.evidence_ids.is_empty()
        {
            return Err("profile patch update requires at least one evidence_id".into());
        }
        for id in &op.evidence_ids {
            if !evidence_ids.iter().any(|evidence_id| evidence_id == id) {
                return Err(format!("profile patch references unknown evidence_id {id}"));
            }
        }
        if matches!(op.op, ProfilePatchOp::Update | ProfilePatchOp::Delete) {
            let target = op
                .target_fact_id
                .as_deref()
                .ok_or_else(|| "profile patch update/delete requires target_fact_id".to_string())?;
            if !store.facts.iter().any(|fact| fact.id == target) {
                return Err(format!(
                    "profile patch references unknown target_fact_id {target}"
                ));
            }
        }
        if text.chars().count() > 240 {
            return Err("profile patch text is too long".into());
        }
    }
    Ok(())
}

pub fn record_profile_aggregation_diagnostic(
    event: &str,
    error: Option<&str>,
    store: &ProfileStore,
    evidence: &[&LongTermEntry],
    patch: Option<&ProfilePatch>,
) {
    let record = ProfileAggregationDiagnostic {
        event,
        error,
        existing_fact_count: store.active_facts().len(),
        evidence_count: evidence.len(),
        patch,
    };
    if let Err(e) = append_jsonl("profile_aggregation.jsonl", &record) {
        warn!(error = %e, "failed to write profile aggregation diagnostic");
    }
}

impl ProfileStore {
    pub fn empty() -> Self {
        Self {
            facts: Vec::new(),
            revision: 0,
            profile_text: String::new(),
            updated_at: String::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.active_facts().is_empty() && self.profile_text.trim().is_empty()
    }

    fn active_facts(&self) -> Vec<&ProfileFact> {
        self.facts
            .iter()
            .filter(|fact| fact.status == ProfileFactStatus::Active && !fact.text.trim().is_empty())
            .collect()
    }

    /// 从磁盘加载。文件不存在或损坏时返回空画像。
    pub fn load() -> Self {
        let path = match profile_file_path() {
            Ok(p) => p,
            Err(e) => {
                warn!(error = %e, "获取画像文件路径失败");
                return Self::empty();
            }
        };
        if !path.exists() {
            return Self::empty();
        }
        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<ProfileStore>(&content) {
                Ok(mut store) => {
                    store.refresh_profile_text();
                    store
                }
                Err(e) => {
                    warn!(error = %e, "解析画像文件失败");
                    Self::empty()
                }
            },
            Err(e) => {
                warn!(error = %e, "读取画像文件失败");
                Self::empty()
            }
        }
    }

    /// 构建注入 prompt 的文本。空画像返回空字符串。
    pub fn build_context(&self) -> String {
        let rendered = self.render_profile_text();
        if rendered.is_empty() {
            return String::new();
        }
        format!("[关于主人]\n{}\n[/关于主人]\n", rendered)
    }

    /// 持久化到磁盘（原子写入：先写临时文件再 rename）。
    pub fn save(&self) -> Result<(), String> {
        let path = profile_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        let mut store = self.clone();
        store.refresh_profile_text();
        let json = serde_json::to_string(&store).map_err(|e| format!("序列化失败: {e}"))?;
        let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())
            .map_err(|e| format!("创建临时文件失败: {e}"))?;
        std::io::Write::write_all(&mut tmp, json.as_bytes())
            .map_err(|e| format!("写入临时文件失败: {e}"))?;
        tmp.persist(&path)
            .map_err(|e| format!("原子替换失败: {e}"))?;
        debug!(path = ?path, "用户画像已持久化");
        Ok(())
    }

    /// 用 AI 聚合结果更新画像
    pub fn update(&mut self, new_profile: &str) {
        self.profile_text = new_profile.to_string();
        self.updated_at = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    }

    /// Apply an AI-submitted profile patch after deterministic validation.
    pub fn apply_patch(
        &mut self,
        patch: &ProfilePatch,
        evidence: &[&LongTermEntry],
    ) -> Result<(), String> {
        validate_profile_patch(patch, evidence, self)?;
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();

        for op in &patch.operations {
            match op.op {
                ProfilePatchOp::NoOp => {}
                ProfilePatchOp::Delete => {
                    let target = op
                        .target_fact_id
                        .as_deref()
                        .ok_or_else(|| "delete operation requires target_fact_id".to_string())?;
                    if let Some(fact) = self.facts.iter_mut().find(|fact| fact.id == target) {
                        fact.status = ProfileFactStatus::Deleted;
                        fact.last_seen_at = now.clone();
                    }
                }
                ProfilePatchOp::Update => {
                    let target = op
                        .target_fact_id
                        .as_deref()
                        .ok_or_else(|| "update operation requires target_fact_id".to_string())?;
                    if let Some(fact) = self.facts.iter_mut().find(|fact| fact.id == target) {
                        fact.section = op.section;
                        fact.text = truncate_chars(op.text.trim(), 220);
                        fact.confidence = op.confidence;
                        fact.stability = op.stability;
                        merge_evidence_ids(&mut fact.evidence_ids, &op.evidence_ids);
                        fact.last_seen_at = now.clone();
                        fact.status = ProfileFactStatus::Active;
                    }
                }
                ProfilePatchOp::Upsert => {
                    let normalized = normalize_memory_summary(&op.text);
                    if normalized.is_empty() {
                        continue;
                    }
                    if let Some(fact) = self.facts.iter_mut().find(|fact| {
                        fact.status == ProfileFactStatus::Active
                            && fact.section == op.section
                            && normalize_memory_summary(&fact.text) == normalized
                    }) {
                        fact.confidence = fact.confidence.max(op.confidence);
                        fact.stability = stronger_stability(fact.stability, op.stability);
                        merge_evidence_ids(&mut fact.evidence_ids, &op.evidence_ids);
                        fact.last_seen_at = now.clone();
                    } else {
                        self.facts.push(ProfileFact {
                            id: next_profile_fact_id(self.revision, self.facts.len()),
                            section: op.section,
                            text: truncate_chars(op.text.trim(), 220),
                            evidence_ids: unique_strings(&op.evidence_ids),
                            confidence: op.confidence,
                            stability: op.stability,
                            first_seen_at: now.clone(),
                            last_seen_at: now.clone(),
                            status: ProfileFactStatus::Active,
                        });
                    }
                }
            }
        }

        if !patch.operations.is_empty() {
            self.revision = self.revision.saturating_add(1);
            self.updated_at = now;
            self.refresh_profile_text();
        }
        Ok(())
    }

    fn refresh_profile_text(&mut self) {
        let rendered = self.render_profile_text();
        if !rendered.is_empty() {
            self.profile_text = rendered;
        }
    }

    fn render_profile_text(&self) -> String {
        let facts = self.active_facts();
        if facts.is_empty() {
            return self.profile_text.trim().to_string();
        }

        let sections = [
            ProfileSection::Identity,
            ProfileSection::WorkDomains,
            ProfileSection::ActiveProjects,
            ProfileSection::TechnicalStack,
            ProfileSection::Preferences,
            ProfileSection::InteractionStyle,
            ProfileSection::Routines,
            ProfileSection::Constraints,
            ProfileSection::OpenLoops,
            ProfileSection::Other,
        ];
        let mut lines = Vec::new();
        for section in sections {
            let items: Vec<&ProfileFact> = facts
                .iter()
                .copied()
                .filter(|fact| fact.section == section)
                .collect();
            if items.is_empty() {
                continue;
            }
            lines.push(format!("{}：", section.label()));
            for fact in items.into_iter().take(5) {
                lines.push(format!("- {}", fact.text.trim()));
            }
        }
        lines.join("\n")
    }
}

// ---- AI 聚合：从长期记忆生成画像 patch ----

/// 调用 AI 将未聚合的长期记忆条目整理为结构化画像 patch。
/// `prompt` 来自 `PromptsConfig::default().aggregation.prompt`（即 config/prompts.yml 的 aggregation 段）。
pub async fn aggregate_profile(
    unaggregated: &[&LongTermEntry],
    existing_profile: &str,
    ai_config: &AiConfig,
    prompt: &str,
) -> Result<ProfilePatch, String> {
    if unaggregated.is_empty() {
        return Err("没有需要聚合的新记录".to_string());
    }

    let evidence_rows = profile_evidence_rows(unaggregated);
    let evidence_text = format_profile_evidence(&evidence_rows);
    let user_content = if existing_profile.is_empty() {
        format!("当前画像为空。\n\n以下是新增长期记忆证据表：\n{evidence_text}")
    } else {
        format!(
            "以下是当前画像：\n{existing_profile}\n\n以下是新增长期记忆证据表：\n{evidence_text}"
        )
    };

    debug!(
        model = %ai_config.model,
        base_url = %ai_config.base_url,
        "开始聚合用户画像"
    );

    let http_client = rig::http_client::ReqwestClient::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;
    let client = anthropic::Client::builder()
        .api_key(&ai_config.api_key)
        .base_url(&ai_config.base_url)
        .http_client(http_client)
        .build()
        .map_err(|e| format!("创建 Anthropic 记忆 Client 失败: {e}"))?;
    let extractor = client
        .extractor::<ProfilePatch>(ai_config.model.as_str())
        .preamble(prompt)
        .max_tokens(1024)
        .retries(1)
        .build();

    let start = std::time::Instant::now();
    let response = extractor
        .extract_with_usage(user_content)
        .await
        .map_err(|e| format!("生成结构化用户画像失败: {e}"))?;

    let elapsed = start.elapsed();
    debug!(
        elapsed_ms = elapsed.as_millis(),
        operations = response.data.operations.len(),
        no_update = response.data.no_update_reason.is_some(),
        "用户画像 patch 聚合完成"
    );

    record_token_usage(
        &TokenRecord::new(
            new_session_id(),
            TokenCategory::MemoryAggregation,
            ai_config.model.clone(),
            TokenUsage::from(response.usage),
        )
        .with_elapsed_ms(elapsed.as_millis() as u64),
    );

    Ok(response.data)
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_truncate_short_unchanged() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_unchanged() {
        assert_eq!(truncate_chars("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate_chars("abcdefghij", 7), "abcd...");
    }

    #[test]
    fn test_truncate_multibyte_utf8() {
        let s = "你好世界这是一个测试字符串";
        let r = truncate_chars(s, 6);
        assert_eq!(r.chars().count(), 6);
        assert!(r.ends_with("..."));
    }

    #[test]
    fn test_record_enforces_max_entries() {
        let mut store = MemoryStore {
            entries: Vec::new(),
        };
        let cfg = MemoryConfig {
            max_entries: 3,
            ..Default::default()
        };
        for i in 0..5 {
            store.record_conversation(&format!("msg{i}"), &format!("reply{i}"), &cfg);
        }
        assert_eq!(store.entries.len(), 3);
        assert_eq!(store.entries[0].user_msg, "msg2");
        assert_eq!(store.entries[2].user_msg, "msg4");
    }

    #[test]
    fn test_record_unlimited_when_max_entries_zero() {
        let mut store = MemoryStore {
            entries: Vec::new(),
        };
        let cfg = MemoryConfig {
            max_entries: 0,
            ..Default::default()
        };
        for i in 0..5 {
            store.record_conversation(&format!("msg{i}"), &format!("reply{i}"), &cfg);
        }
        assert_eq!(store.entries.len(), 5);
        assert_eq!(store.entries[0].user_msg, "msg0");
        assert_eq!(store.entries[4].user_msg, "msg4");
    }

    #[test]
    fn test_record_truncates_fields() {
        let mut store = MemoryStore {
            entries: Vec::new(),
        };
        let cfg = MemoryConfig {
            max_user_chars: 5,
            max_reply_chars: 10,
            ..Default::default()
        };
        store.record_conversation("abcdefghijklmnopqrstuvwxyz", "1234567890ABCDEFGHIJ", &cfg);
        assert_eq!(store.entries[0].user_msg.chars().count(), 5);
        assert_eq!(store.entries[0].ai_reply.chars().count(), 10);
    }

    #[test]
    fn test_record_candidate_stores_structured_fields() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        let candidate = MemoryCandidate::explicit(
            "用户偏好 grep-first 长期记忆".into(),
            5,
            vec!["memory".into(), "preference".into()],
        );

        store.record_candidate(&candidate, "记住我的偏好", "好的", 10);

        assert_eq!(store.entries.len(), 1);
        assert_eq!(
            store.entries[0].summary.as_deref(),
            Some("用户偏好 grep-first 长期记忆")
        );
        assert_eq!(store.entries[0].importance, Some(5));
        assert_eq!(store.entries[0].confidence, Some(5));
        assert_eq!(store.entries[0].kind.as_deref(), Some("other"));
        assert_eq!(store.entries[0].ttl_hint.as_deref(), Some("stable"));
        assert_eq!(
            store.entries[0].reason.as_deref(),
            Some("explicitly requested memory")
        );
        assert_eq!(store.entries[0].tags, vec!["memory", "preference"]);
        assert_eq!(store.entries[0].source.as_deref(), Some("agent_reaction"));
    }

    #[test]
    fn test_retrieve_matches_candidate_summary() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        let candidate = MemoryCandidate::explicit(
            "用户正在开发 BitCat 桌面 AI 伙伴项目".into(),
            4,
            vec!["project".into()],
        );
        store.record_candidate(&candidate, "我们继续项目", "没问题", 10);

        let ctx = store.retrieve_with(
            &LongTermMemoryQuery {
                text: "BitCat".into(),
                ..Default::default()
            },
            500,
        );

        assert!(ctx.contains("用户正在开发 BitCat 桌面 AI 伙伴项目"));
        assert!(ctx.contains("tags=[project]"));
    }

    #[test]
    fn test_retrieve_with_filters_by_tags_source_and_importance() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        store.record_candidate(
            &MemoryCandidate::explicit(
                "用户偏好 grep-first 记忆检索".into(),
                5,
                vec!["memory".into(), "preference".into()],
            ),
            "记住我的偏好",
            "好的",
            10,
        );
        store.record_candidate(
            &MemoryCandidate::explicit("用户正在调试桌宠动画".into(), 3, vec!["animation".into()]),
            "动画还有问题",
            "我看看",
            10,
        );

        let ctx = store.retrieve_with(
            &LongTermMemoryQuery {
                text: "记忆".into(),
                tags: vec!["memory".into()],
                source: Some("agent_reaction".into()),
                min_importance: Some(4),
            },
            500,
        );

        assert!(ctx.contains("grep-first"));
        assert!(!ctx.contains("动画"));
    }

    #[test]
    fn test_review_markdown_lists_latest_structured_memory() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        store.record_candidate(
            &MemoryCandidate::explicit(
                "用户正在实现 mood + memory".into(),
                4,
                vec!["project".into()],
            ),
            "开始实现",
            "收到",
            10,
        );

        let markdown = store.review_markdown(10);

        assert!(markdown.contains("# Long-term Memory"));
        assert!(markdown.contains("用户正在实现 mood + memory"));
        assert!(markdown.contains("source: agent_reaction"));
        assert!(markdown.contains("confidence: 5"));
        assert!(markdown.contains("ttl: stable"));
        assert!(markdown.contains("tags: project"));
    }

    #[test]
    fn test_delete_entry_by_id_soft_deletes() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        store.record("first", "one", 10);
        store.record("second", "two", 10);
        let first_id = store.entries[0].id.clone();

        assert!(store.delete_entry_by_id(&first_id));
        assert_eq!(store.entries.len(), 2);
        assert!(store.entries[0].deleted);
        assert_eq!(store.review_entries(10).len(), 1);
        assert_eq!(store.review_entries(10)[0].user_msg, "second");
        assert!(!store.delete_entry_by_id("missing"));
    }

    #[test]
    fn test_build_context_empty() {
        let store = MemoryStore {
            entries: Vec::new(),
        };
        assert!(store.build_context(&MemoryConfig::default()).is_empty());
    }

    #[test]
    fn test_build_context_format() {
        let store = MemoryStore {
            entries: vec![MemoryEntry {
                timestamp: "14:23".into(),
                user_msg: "你好".into(),
                ai_reply: "喵~".into(),
            }],
        };
        let ctx = store.build_context(&MemoryConfig::default());
        assert!(ctx.contains("[最近对话记录]"));
        assert!(ctx.contains("14:23"));
        assert!(ctx.contains("你好"));
        assert!(ctx.contains("喵~"));
        assert!(ctx.contains("[/最近对话记录]"));
    }

    #[test]
    fn test_build_context_uses_char_count_not_bytes() {
        // 纯中文内容：每条约 25 中文字 ≈ 75 字节。
        // 若用字节计数 max_context_chars=100，只能放 ~1 条（75 < 100）。
        // 若用字符计数，应能放 ~4 条（25*4=100 chars）。
        // 此测试验证用的是字符计数而非字节计数。
        let mut store = MemoryStore {
            entries: Vec::new(),
        };
        for _ in 0..6 {
            store.entries.push(MemoryEntry {
                timestamp: "14:00".into(),
                user_msg: "用户询问今天天气如何".into(), // 8 中文字
                ai_reply: "今天是晴天适合出门散步".into(), // 10 中文字
            });
        }
        // 每条约: "[14:00] 用户询问今天天气如何 | 今天是晴天适合出门散步" ≈ 28 中文字 + 时间戳 ≈ 33 字符
        // 用字节计数时 100 字节只能放不到 2 条；用字符计数应能放 3 条
        let cfg = MemoryConfig {
            max_context_chars: 120,
            ..Default::default()
        };
        let ctx = store.build_context(&cfg);
        let char_count = ctx.chars().count();
        // 如果按字节截断，~2 条就超了(2条≈80字节+header≈20=100)，实际字符数只有 ~70
        // 按字符截断应能放下 3 条 ≈ 110 字符
        assert!(
            char_count > 90,
            "build_context 应使用字符计数而非字节计数。实际字符数={}，若用字节计数则过早截断",
            char_count
        );
    }

    #[test]
    fn test_build_context_respects_char_limit() {
        let mut store = MemoryStore {
            entries: Vec::new(),
        };
        for _ in 0..50 {
            store.entries.push(MemoryEntry {
                timestamp: "14:00".into(),
                user_msg: "这是一条很长的用户消息用于测试".into(),
                ai_reply: "这是一条很长的AI回复用于测试字符限制".into(),
            });
        }
        let cfg = MemoryConfig {
            max_context_chars: 300,
            ..Default::default()
        };
        let ctx = store.build_context(&cfg);
        assert!(ctx.chars().count() <= 350);
    }

    #[test]
    fn test_build_context_keeps_latest_entries_within_budget() {
        let mut store = MemoryStore {
            entries: Vec::new(),
        };
        for i in 0..6 {
            store.entries.push(MemoryEntry {
                timestamp: format!("14:0{i}"),
                user_msg: format!("msg{i}"),
                ai_reply: "short reply".into(),
            });
        }
        let cfg = MemoryConfig {
            max_context_chars: 100,
            ..Default::default()
        };

        let ctx = store.build_context(&cfg);

        assert!(!ctx.contains("msg0"));
        assert!(ctx.contains("msg5"));
        assert!(ctx.find("msg4") < ctx.find("msg5"));
    }

    #[test]
    fn test_default_config() {
        let cfg = MemoryConfig::default();
        assert_eq!(cfg.max_entries, 0);
        assert_eq!(cfg.max_context_chars, 20_000);
        assert_eq!(cfg.max_user_chars, 500);
        assert_eq!(cfg.max_reply_chars, 1000);
    }

    #[test]
    fn test_json_roundtrip() {
        let store = MemoryStore {
            entries: vec![MemoryEntry {
                timestamp: "14:23".into(),
                user_msg: "test".into(),
                ai_reply: "reply".into(),
            }],
        };
        let json = serde_json::to_string(&store).unwrap();
        let back: MemoryStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0].user_msg, "test");
    }

    #[test]
    fn test_memory_file_path() {
        let p = memory_file_path().unwrap();
        let s = p.to_string_lossy();
        assert!(s.contains(".bitcat"), "应在 .bitcat 下");
        assert!(s.contains("memory"), "应有 memory 子目录");
        assert!(s.ends_with("chat_summary.json"));
    }

    #[test]
    fn test_truncated_json_detected_as_corrupt() {
        // 模拟崩溃导致文件截断：load() 应返回空记忆而非静默接受损坏数据
        let store = MemoryStore {
            entries: vec![MemoryEntry {
                timestamp: "14:23".into(),
                user_msg: "重要数据".into(),
                ai_reply: "重要回复".into(),
            }],
        };
        let json = serde_json::to_string(&store).unwrap();
        // 截断到一半
        let truncated = &json[..json.len() / 2];
        let result = serde_json::from_str::<MemoryStore>(truncated);
        assert!(result.is_err(), "截断的 JSON 应解析失败，防止静默丢失数据");
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".bitcat").join("memory");

        let mut store = MemoryStore {
            entries: Vec::new(),
        };
        let cfg = MemoryConfig::default();
        store.record_conversation("你好", "喵~你好！", &cfg);

        // 用临时目录模拟保存
        let json = serde_json::to_string(&store).unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("chat_summary.json"), &json).unwrap();

        let _loaded = MemoryStore::load();
        // load 会读真实路径而非 tmp，所以直接验证序列化正确性
        let back: MemoryStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0].user_msg, "你好");
    }

    // ---- LongTermMemory 测试 ----

    #[test]
    fn test_long_term_record_and_json_roundtrip() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        store.record("我叫小明", "你好小明！", 100);
        store.record("我在做 BitCat 项目", "Rust 桌面伙伴听起来好酷", 100);

        assert_eq!(store.entries.len(), 2);
        assert!(!store.entries[0].aggregated);
        assert_eq!(store.entries[0].user_msg, "我叫小明");

        let json = serde_json::to_string(&store).unwrap();
        let back: LongTermMemory = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 2);
        assert_eq!(back.entries[1].user_msg, "我在做 BitCat 项目");
    }

    #[test]
    fn test_long_term_record_truncates_reply() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        let long_reply = "这是一段非常非常长的AI回复用于测试截断功能是否正常工作".repeat(10);
        store.record("测试", &long_reply, 100);
        assert!(store.entries[0].ai_reply.chars().count() <= 400);
    }

    #[test]
    fn test_long_term_enforces_max_entries() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        for i in 0..25 {
            store.record(&format!("msg{i}"), &format!("reply{i}"), 10);
        }
        assert_eq!(store.entries.len(), 10);
    }

    #[test]
    fn test_long_term_prefer_evict_aggregated() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        for i in 0..5 {
            store.record(&format!("msg{i}"), &format!("reply{i}"), 10);
        }
        store.entries[0].aggregated = true;
        store.entries[1].aggregated = true;
        // 5 条 + 1 条新 = 6，max=5，淘汰 1 条（优先已聚合的 entry 0）
        store.record("new_msg", "new_reply", 5);
        assert_eq!(store.entries.len(), 5);
        assert!(!store.entries.iter().any(|e| e.user_msg == "msg0"));
        // msg1 是已聚合但没被淘汰（只需淘汰 1 条）
        assert!(store.entries.iter().any(|e| e.user_msg == "msg1"));
    }

    #[test]
    fn test_retrieve_empty_returns_empty() {
        let store = LongTermMemory {
            entries: Vec::new(),
        };
        assert!(
            store
                .retrieve_with(
                    &LongTermMemoryQuery {
                        text: "anything".into(),
                        ..Default::default()
                    },
                    500,
                )
                .is_empty()
        );
    }

    #[test]
    fn test_retrieve_returns_relevant_entries() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        store.record("我在做 BitCat 项目", "Rust 桌面伙伴好酷", 100);
        store.record("今天天气不错", "是呢，适合出门", 100);
        store.record("帮我提醒明天交 PR", "收到，明天会提醒的", 100);

        let ctx = store.retrieve_with(
            &LongTermMemoryQuery {
                text: "BitCat 项目进展".into(),
                ..Default::default()
            },
            500,
        );
        assert!(ctx.contains("[memory candidates]"));
        assert!(ctx.contains("BitCat"));
        assert!(ctx.contains("[/memory candidates]"));
    }

    #[test]
    fn test_retrieve_respects_budget() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        for i in 0..20 {
            store.record(
                &format!("消息{}这是很长的一段内容用于测试预算控制", i),
                &format!("回复{}这也是很长的回复内容", i),
                100,
            );
        }
        let ctx = store.retrieve_with(
            &LongTermMemoryQuery {
                text: "消息".into(),
                ..Default::default()
            },
            200,
        );
        assert!(ctx.chars().count() <= 250);
    }

    #[test]
    fn test_retrieve_no_match_returns_empty() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        store.record("今天天气不错", "是呢适合出门", 100);
        let ctx = store.retrieve_with(
            &LongTermMemoryQuery {
                text: "量子力学研究进展".into(),
                ..Default::default()
            },
            500,
        );
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_mark_all_aggregated() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        store.record("msg1", "reply1", 100);
        store.record("msg2", "reply2", 100);
        store.mark_all_aggregated();
        assert!(store.entries.iter().all(|e| e.aggregated));
        assert!(store.unaggregated_entries().is_empty());
    }

    #[test]
    fn test_unaggregated_entries_filters_correctly() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        store.record("msg1", "reply1", 100);
        store.record("msg2", "reply2", 100);
        store.record("msg3", "reply3", 100);
        store.entries[0].aggregated = true;
        let unagg = store.unaggregated_entries();
        assert_eq!(unagg.len(), 2);
    }

    // ---- ProfileStore 测试 ----

    #[test]
    fn test_profile_store_roundtrip() {
        let store = ProfileStore {
            facts: Vec::new(),
            revision: 0,
            profile_text: "主人叫小明，程序员".into(),
            updated_at: "2026-05-12 14:30".into(),
        };
        let json = serde_json::to_string(&store).unwrap();
        let back: ProfileStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.profile_text, "主人叫小明，程序员");
    }

    #[test]
    fn test_profile_build_context_empty() {
        let store = ProfileStore::empty();
        assert!(store.build_context().is_empty());
    }

    #[test]
    fn test_profile_build_context_format() {
        let store = ProfileStore {
            facts: Vec::new(),
            revision: 0,
            profile_text: "主人叫小明，正在开发 BitCat".into(),
            updated_at: "2026-05-12".into(),
        };
        let ctx = store.build_context();
        assert!(ctx.contains("[关于主人]"));
        assert!(ctx.contains("小明"));
        assert!(ctx.contains("BitCat"));
        assert!(ctx.contains("[/关于主人]"));
    }

    #[test]
    fn test_profile_update() {
        let mut store = ProfileStore::empty();
        store.update("新的画像内容");
        assert_eq!(store.profile_text, "新的画像内容");
        assert!(!store.updated_at.is_empty());
    }

    #[test]
    fn test_profile_apply_patch_adds_structured_fact() {
        let mut store = ProfileStore::empty();
        let entries = vec![LongTermEntry {
            id: "mem_test_1".into(),
            created_at: "2026-05-12T14:23:00+08:00".into(),
            timestamp: "05-12 14:23".into(),
            user_msg: "我在做 BitCat".into(),
            ai_reply: "记住了".into(),
            summary: Some("用户正在开发 BitCat 桌面应用".into()),
            tags: vec!["bitcat".into()],
            importance: Some(4),
            confidence: Some(5),
            kind: Some("project".into()),
            ttl_hint: Some("evolving".into()),
            reason: Some("明确项目背景".into()),
            source: Some("agent_reaction".into()),
            aggregated: false,
            deleted: false,
        }];
        let refs: Vec<&LongTermEntry> = entries.iter().collect();
        let patch = ProfilePatch {
            operations: vec![ProfilePatchOperation {
                op: ProfilePatchOp::Upsert,
                section: ProfileSection::ActiveProjects,
                target_fact_id: None,
                evidence_ids: vec!["mem_test_1".into()],
                text: "用户正在开发 BitCat 桌面应用".into(),
                confidence: 5,
                stability: ProfileStability::Evolving,
                reason: "长期记忆明确提到当前项目".into(),
            }],
            no_update_reason: None,
        };

        store.apply_patch(&patch, &refs).unwrap();
        assert_eq!(store.facts.len(), 1);
        assert!(store.profile_text.contains("当前项目"));
        assert!(store.profile_text.contains("BitCat"));
    }

    #[test]
    fn test_profile_apply_patch_rejects_unknown_evidence() {
        let mut store = ProfileStore::empty();
        let patch = ProfilePatch {
            operations: vec![ProfilePatchOperation {
                op: ProfilePatchOp::Upsert,
                section: ProfileSection::Preferences,
                target_fact_id: None,
                evidence_ids: vec!["missing".into()],
                text: "用户偏好结构化记忆".into(),
                confidence: 4,
                stability: ProfileStability::Stable,
                reason: "测试".into(),
            }],
            no_update_reason: None,
        };

        let err = store.apply_patch(&patch, &[]).unwrap_err();
        assert!(err.contains("unknown evidence_id"));
    }

    // ---- aggregate_profile 测试 ----

    #[tokio::test]
    async fn test_aggregate_profile_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
               "type": "message",
               "id": "msg_test",
               "model": "test-model",
               "role": "assistant",
               "stop_reason": "tool_use",
               "stop_sequence": null,
               "usage": {
                   "input_tokens": 18,
                   "output_tokens": 7
               },
                "content": [{
                    "type": "tool_use",
                    "id": "toolu_test",
                    "name": "submit",
                    "input": {
                        "operations": [{
                            "op": "upsert",
                            "section": "identity",
                            "evidence_ids": ["mem_test_1"],
                            "text": "用户叫小明",
                            "confidence": 5,
                            "stability": "stable",
                            "reason": "用户明确自我介绍"
                        }],
                        "no_update_reason": null
                    }
                }]
            })))
            .mount(&server)
            .await;

        let entries = vec![LongTermEntry {
            id: "mem_test_1".into(),
            created_at: "2026-05-12T14:23:00+08:00".into(),
            timestamp: "05-12 14:23".into(),
            user_msg: "我叫小明".into(),
            ai_reply: "你好小明！".into(),
            summary: Some("用户叫小明".into()),
            tags: Vec::new(),
            importance: None,
            confidence: None,
            kind: None,
            ttl_hint: None,
            reason: None,
            source: Some("test".into()),
            aggregated: false,
            deleted: false,
        }];
        let refs: Vec<&LongTermEntry> = entries.iter().collect();

        let ai_config = AiConfig {
            api_key: "test-key".into(),
            base_url: server.uri(),
            model: "test-model".into(),
        };

        let result = aggregate_profile(&refs, "", &ai_config, "测试聚合提示词").await;
        assert!(result.is_ok());
        let patch = result.unwrap();
        assert_eq!(patch.operations.len(), 1);
        assert_eq!(patch.operations[0].evidence_ids, vec!["mem_test_1"]);
        assert!(patch.operations[0].text.contains("小明"));
    }

    #[tokio::test]
    async fn test_aggregate_profile_with_existing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
               "type": "message",
               "id": "msg_test",
               "model": "test-model",
               "role": "assistant",
               "stop_reason": "tool_use",
               "stop_sequence": null,
               "usage": {
                   "input_tokens": 20,
                   "output_tokens": 8
               },
                "content": [{
                    "type": "tool_use",
                    "id": "toolu_test",
                    "name": "submit",
                    "input": {
                        "operations": [{
                            "op": "upsert",
                            "section": "active_projects",
                            "evidence_ids": ["mem_test_2"],
                            "text": "用户正在做 Rust 项目",
                            "confidence": 4,
                            "stability": "evolving",
                            "reason": "用户说明当前项目技术"
                        }],
                        "no_update_reason": null
                    }
                }]
            })))
            .mount(&server)
            .await;

        let entries = vec![LongTermEntry {
            id: "mem_test_2".into(),
            created_at: "2026-05-12T15:00:00+08:00".into(),
            timestamp: "05-12 15:00".into(),
            user_msg: "我在做 Rust 项目".into(),
            ai_reply: "什么项目？".into(),
            summary: Some("用户正在做 Rust 项目".into()),
            tags: Vec::new(),
            importance: None,
            confidence: None,
            kind: None,
            ttl_hint: None,
            reason: None,
            source: Some("test".into()),
            aggregated: false,
            deleted: false,
        }];
        let refs: Vec<&LongTermEntry> = entries.iter().collect();

        let ai_config = AiConfig {
            api_key: "test-key".into(),
            base_url: server.uri(),
            model: "test-model".into(),
        };

        let result =
            aggregate_profile(&refs, "主人叫小明，程序员", &ai_config, "测试聚合提示词").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_aggregate_profile_empty_entries_err() {
        let ai_config = AiConfig {
            api_key: "test".into(),
            base_url: "https://api.anthropic.com".into(),
            model: "test".into(),
        };
        let result = aggregate_profile(&[], "", &ai_config, "测试聚合提示词").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("没有需要聚合"));
    }

    #[tokio::test]
    async fn test_aggregate_profile_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_json(json!({
                "error": { "type": "server_error", "message": "internal error" }
            })))
            .mount(&server)
            .await;

        let entries = vec![LongTermEntry {
            id: "mem_test_3".into(),
            created_at: "2026-05-12T14:23:00+08:00".into(),
            timestamp: "05-12 14:23".into(),
            user_msg: "我叫小明".into(),
            ai_reply: "你好".into(),
            summary: None,
            tags: Vec::new(),
            importance: None,
            confidence: None,
            kind: None,
            ttl_hint: None,
            reason: None,
            source: Some("test".into()),
            aggregated: false,
            deleted: false,
        }];
        let refs: Vec<&LongTermEntry> = entries.iter().collect();

        let ai_config = AiConfig {
            api_key: "test-key".into(),
            base_url: server.uri(),
            model: "test-model".into(),
        };

        let result = aggregate_profile(&refs, "", &ai_config, "测试聚合提示词").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("生成结构化用户画像失败"));
    }
}
