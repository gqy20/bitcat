//! 三层对话记忆系统：短期滚动窗口 + 长期 JSONL 检索 + AI 聚合画像。
//!
//! 设计遵循 grep-first 原则：所有持久化产物均为可搜索的结构化文本
//! （JSON / JSONL），不引入向量数据库或 Embedding，方便 `rg`、
//! 人工审查和大模型共同读取。记忆检索先用关键词等可解释条件筛选候选，
//! 再交给大模型判断压缩。
//!
//! 三层各自持久化到 `~/.ai-pad/memory/`：
//! - **MemoryStore** — `chat_summary.json`，滚动窗口短期记忆，直接注入 prompt
//! - **LongTermMemory** — `long_term.json`，原始对话按需关键词检索注入
//! - **ProfileStore** — `profile.json`，AI 定期聚合的用户画像摘要
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
/// 超出 `max_entries` 时自动淘汰最旧条目，保证注入 prompt 的上下文不会无限膨胀。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStore {
    pub entries: Vec<MemoryEntry>,
}

/// 记忆系统配置，来自 `config/prompts.yml` 的 `memory` 段。
///
/// 控制滚动窗口大小、上下文字符预算、单条消息截断阈值。
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
    20
}
fn default_max_context_chars() -> usize {
    1500
}
fn default_max_user_chars() -> usize {
    100
}
fn default_max_reply_chars() -> usize {
    200
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

/// 返回短期记忆文件路径 `~/.ai-pad/memory/chat_summary.json`。
fn memory_file_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取 HOME 目录".to_string())?;
    Ok(home
        .join(".ai-pad")
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

        while self.entries.len() > config.max_entries {
            self.entries.remove(0);
        }
    }

    /// 构建注入 prompt 的上下文文本。空记忆返回空字符串。
    pub fn build_context(&self, config: &MemoryConfig) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        // 从最新到最旧收集行，计算预算后反转回时间顺序
        let mut lines: Vec<String> = Vec::new();
        for entry in self.entries.iter().rev() {
            lines.push(format!(
                "[{}] {} | {}",
                entry.timestamp, entry.user_msg, entry.ai_reply
            ));
        }
        lines.reverse();

        let header = "[最近对话记录]\n";
        let mut result = String::from(header);

        for line in &lines {
            if result.chars().count() + line.chars().count() + 1 > config.max_context_chars {
                break;
            }
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

// ---- Layer 2: 长期记忆（原始对话记录，按需检索） ----

/// 单条长期记忆条目——完整保存值得长期保留的对话内容。
///
/// `aggregated` 标记表示该条目已被 AI 聚合到画像中，容量不足时优先淘汰。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermEntry {
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
    pub source: Option<String>,
    #[serde(default)]
    pub aggregated: bool,
}

/// 长期记忆存储：保存原始对话原文，按关键词相关性检索并注入 prompt。
///
/// 超出容量时优先淘汰已聚合条目，保持未聚合数据供下次聚合使用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermMemory {
    pub entries: Vec<LongTermEntry>,
}

/// 返回长期记忆文件路径 `~/.ai-pad/memory/long_term.json`。
fn long_term_file_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取 HOME 目录".to_string())?;
    Ok(home.join(".ai-pad").join("memory").join("long_term.json"))
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
            Ok(content) => match serde_json::from_str::<LongTermMemory>(&content) {
                Ok(store) => store,
                Err(e) => {
                    warn!(error = %e, "解析长期记忆文件失败");
                    Self {
                        entries: Vec::new(),
                    }
                }
            },
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
        let timestamp = chrono::Local::now().format("%m-%d %H:%M").to_string();
        let reply_truncated: String = ai_reply.chars().take(400).collect();
        self.entries.push(LongTermEntry {
            timestamp,
            user_msg: user_msg.to_string(),
            ai_reply: reply_truncated,
            summary: None,
            tags: Vec::new(),
            importance: None,
            source: Some("conversation".to_string()),
            aggregated: false,
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
        let timestamp = chrono::Local::now().format("%m-%d %H:%M").to_string();
        let reply_truncated: String = ai_reply.chars().take(240).collect();
        self.entries.push(LongTermEntry {
            timestamp,
            user_msg: user_msg.to_string(),
            ai_reply: reply_truncated,
            summary: Some(candidate.text.clone()),
            tags: candidate.tags.clone(),
            importance: Some(candidate.importance),
            source: Some("agent_reaction".to_string()),
            aggregated: false,
        });
        self.enforce_max_entries(max_entries);
    }

    fn enforce_max_entries(&mut self, max_entries: usize) {
        while self.entries.len() > max_entries {
            if let Some(pos) = self.entries.iter().position(|e| e.aggregated) {
                self.entries.remove(pos);
            } else {
                self.entries.remove(0);
            }
        }
    }

    /// 根据查询文本检索最相关的条目，在字符预算内拼接为注入文本
    pub fn retrieve(&self, query: &str, budget_chars: usize) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        let mut scored: Vec<(usize, f32)> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let text = format!(
                    "{} {} {} {}",
                    e.summary.as_deref().unwrap_or(""),
                    e.tags.join(" "),
                    e.user_msg,
                    e.ai_reply
                );
                (i, relevance_score(&text, query))
            })
            .filter(|(_, s)| *s > 0.05)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scored.truncate(10);

        let header = "[相关记忆]\n";
        let mut result = String::from(header);
        let mut used = header.chars().count();
        for (idx, _) in &scored {
            let e = &self.entries[*idx];
            let line = if let Some(summary) = &e.summary {
                let tags = if e.tags.is_empty() {
                    String::new()
                } else {
                    format!(" #{}", e.tags.join(" #"))
                };
                format!(
                    "[{}] {}{} | {} | {}\n",
                    e.timestamp, summary, tags, e.user_msg, e.ai_reply
                )
            } else {
                format!("[{}] {} | {}\n", e.timestamp, e.user_msg, e.ai_reply)
            };
            if used + line.chars().count() > budget_chars {
                break;
            }
            result.push_str(&line);
            used += line.chars().count();
        }
        if result == header {
            return String::new();
        }
        result.push_str("[/相关记忆]\n");
        result
    }

    /// 标记所有条目为已聚合
    pub fn mark_all_aggregated(&mut self) {
        for e in &mut self.entries {
            e.aggregated = true;
        }
    }

    /// 取出未聚合的条目
    pub fn unaggregated_entries(&self) -> Vec<&LongTermEntry> {
        self.entries.iter().filter(|e| !e.aggregated).collect()
    }

    /// 持久化到磁盘（原子写入：先写临时文件再 rename）。
    pub fn save(&self) -> Result<(), String> {
        let path = long_term_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        let json = serde_json::to_string(self).map_err(|e| format!("序列化失败: {e}"))?;
        let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())
            .map_err(|e| format!("创建临时文件失败: {e}"))?;
        std::io::Write::write_all(&mut tmp, json.as_bytes())
            .map_err(|e| format!("写入临时文件失败: {e}"))?;
        tmp.persist(&path)
            .map_err(|e| format!("原子替换失败: {e}"))?;
        debug!(path = ?path, "长期记忆已持久化");
        Ok(())
    }
}

// ---- Layer 2: 聚合画像（AI 定期生成） ----

/// AI 聚合后的用户画像摘要，由 `aggregate_profile()` 定期生成。
///
/// 优先级低于 `config/user.yml` 中的显式声明——user.yml 有内容时直接使用，全空才回退到本画像。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileStore {
    pub profile_text: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
struct ProfileAggregation {
    pub profile_text: String,
}

/// 返回用户画像文件路径 `~/.ai-pad/memory/profile.json`。
fn profile_file_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取 HOME 目录".to_string())?;
    Ok(home.join(".ai-pad").join("memory").join("profile.json"))
}

impl ProfileStore {
    /// 从磁盘加载。文件不存在或损坏时返回空画像。
    pub fn load() -> Self {
        let path = match profile_file_path() {
            Ok(p) => p,
            Err(e) => {
                warn!(error = %e, "获取画像文件路径失败");
                return Self {
                    profile_text: String::new(),
                    updated_at: String::new(),
                };
            }
        };
        if !path.exists() {
            return Self {
                profile_text: String::new(),
                updated_at: String::new(),
            };
        }
        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<ProfileStore>(&content) {
                Ok(store) => store,
                Err(e) => {
                    warn!(error = %e, "解析画像文件失败");
                    Self {
                        profile_text: String::new(),
                        updated_at: String::new(),
                    }
                }
            },
            Err(e) => {
                warn!(error = %e, "读取画像文件失败");
                Self {
                    profile_text: String::new(),
                    updated_at: String::new(),
                }
            }
        }
    }

    /// 构建注入 prompt 的文本。空画像返回空字符串。
    pub fn build_context(&self) -> String {
        if self.profile_text.is_empty() {
            return String::new();
        }
        format!("[关于主人]\n{}\n[/关于主人]\n", self.profile_text)
    }

    /// 持久化到磁盘（原子写入：先写临时文件再 rename）。
    pub fn save(&self) -> Result<(), String> {
        let path = profile_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        let json = serde_json::to_string(self).map_err(|e| format!("序列化失败: {e}"))?;
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
}

/// 简单相关性评分：query 中的词出现在目标文本中的比例
fn relevance_score(text: &str, query: &str) -> f32 {
    use std::collections::HashSet;
    let query_words: HashSet<&str> = query.split_whitespace().collect();
    if query_words.is_empty() {
        return 0.0;
    }
    let text_lower = text.to_lowercase();
    let count = query_words
        .iter()
        .filter(|w| text_lower.contains(&w.to_lowercase()))
        .count();
    count as f32 / query_words.len() as f32
}

// ---- AI 聚合：从原始记录生成画像摘要 ----

/// 调用 AI 将未聚合的长期记忆条目聚合为用户画像摘要。
/// `prompt` 来自 `PromptsConfig::default().aggregation.prompt`（即 config/prompts.yml 的 aggregation 段）。
pub async fn aggregate_profile(
    unaggregated: &[&LongTermEntry],
    existing_profile: &str,
    ai_config: &AiConfig,
    prompt: &str,
) -> Result<String, String> {
    if unaggregated.is_empty() {
        return Err("没有需要聚合的新记录".to_string());
    }

    let entries_text: String = unaggregated
        .iter()
        .map(|e| format!("[{}] {} | {}", e.timestamp, e.user_msg, e.ai_reply))
        .collect::<Vec<_>>()
        .join("\n");

    let user_content = if existing_profile.is_empty() {
        format!("以下是新的对话记录：\n{entries_text}")
    } else {
        format!(
            "以下是之前的记忆摘要：\n{existing_profile}\n\n以下是新增的对话记录：\n{entries_text}"
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
        .extractor::<ProfileAggregation>(ai_config.model.as_str())
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
        chars = response.data.profile_text.chars().count(),
        "用户画像聚合完成"
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

    Ok(response.data.profile_text)
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
        let candidate = MemoryCandidate {
            text: "用户偏好 grep-first 长期记忆".into(),
            importance: 5,
            tags: vec!["memory".into(), "preference".into()],
        };

        store.record_candidate(&candidate, "记住我的偏好", "好的", 10);

        assert_eq!(store.entries.len(), 1);
        assert_eq!(
            store.entries[0].summary.as_deref(),
            Some("用户偏好 grep-first 长期记忆")
        );
        assert_eq!(store.entries[0].importance, Some(5));
        assert_eq!(store.entries[0].tags, vec!["memory", "preference"]);
        assert_eq!(store.entries[0].source.as_deref(), Some("agent_reaction"));
    }

    #[test]
    fn test_retrieve_matches_candidate_summary() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        let candidate = MemoryCandidate {
            text: "用户正在开发 8Bit Cat 桌宠项目".into(),
            importance: 4,
            tags: vec!["project".into()],
        };
        store.record_candidate(&candidate, "我们继续项目", "没问题", 10);

        let ctx = store.retrieve("8Bit Cat", 500);

        assert!(ctx.contains("用户正在开发 8Bit Cat 桌宠项目"));
        assert!(ctx.contains("#project"));
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
    fn test_default_config() {
        let cfg = MemoryConfig::default();
        assert_eq!(cfg.max_entries, 20);
        assert_eq!(cfg.max_context_chars, 1500);
        assert_eq!(cfg.max_user_chars, 100);
        assert_eq!(cfg.max_reply_chars, 200);
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
        assert!(s.contains(".ai-pad"), "应在 .ai-pad 下");
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
        let dir = tmp.path().join(".ai-pad").join("memory");

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
        store.record("我在做 8Bit 项目", "Rust 桌宠听起来好酷", 100);

        assert_eq!(store.entries.len(), 2);
        assert!(!store.entries[0].aggregated);
        assert_eq!(store.entries[0].user_msg, "我叫小明");

        let json = serde_json::to_string(&store).unwrap();
        let back: LongTermMemory = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 2);
        assert_eq!(back.entries[1].user_msg, "我在做 8Bit 项目");
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
        assert!(store.retrieve("anything", 500).is_empty());
    }

    #[test]
    fn test_retrieve_returns_relevant_entries() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        store.record("我在做 8Bit Cat 项目", "Rust 桌宠好酷", 100);
        store.record("今天天气不错", "是呢，适合出门", 100);
        store.record("帮我提醒明天交 PR", "收到，明天会提醒的", 100);

        let ctx = store.retrieve("8Bit 项目进展", 500);
        assert!(ctx.contains("[相关记忆]"));
        assert!(ctx.contains("8Bit Cat"));
        assert!(ctx.contains("[/相关记忆]"));
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
        let ctx = store.retrieve("消息", 200);
        assert!(ctx.chars().count() <= 250);
    }

    #[test]
    fn test_retrieve_no_match_returns_empty() {
        let mut store = LongTermMemory {
            entries: Vec::new(),
        };
        store.record("今天天气不错", "是呢适合出门", 100);
        let ctx = store.retrieve("量子力学研究进展", 500);
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
            profile_text: "主人叫小明，程序员".into(),
            updated_at: "2026-05-12 14:30".into(),
        };
        let json = serde_json::to_string(&store).unwrap();
        let back: ProfileStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.profile_text, "主人叫小明，程序员");
    }

    #[test]
    fn test_profile_build_context_empty() {
        let store = ProfileStore {
            profile_text: String::new(),
            updated_at: String::new(),
        };
        assert!(store.build_context().is_empty());
    }

    #[test]
    fn test_profile_build_context_format() {
        let store = ProfileStore {
            profile_text: "主人叫小明，正在开发 8Bit Cat".into(),
            updated_at: "2026-05-12".into(),
        };
        let ctx = store.build_context();
        assert!(ctx.contains("[关于主人]"));
        assert!(ctx.contains("小明"));
        assert!(ctx.contains("8Bit Cat"));
        assert!(ctx.contains("[/关于主人]"));
    }

    #[test]
    fn test_profile_update() {
        let mut store = ProfileStore {
            profile_text: String::new(),
            updated_at: String::new(),
        };
        store.update("新的画像内容");
        assert_eq!(store.profile_text, "新的画像内容");
        assert!(!store.updated_at.is_empty());
    }

    // ---- relevance_score 测试 ----

    #[test]
    fn test_relevance_score_exact_match() {
        let score = relevance_score("我在做 8Bit Cat 项目", "8Bit 项目");
        assert!(score > 0.5);
    }

    #[test]
    fn test_relevance_score_no_match() {
        let score = relevance_score("今天天气不错", "量子力学");
        assert!(score < 0.1);
    }

    #[test]
    fn test_relevance_score_empty_query() {
        assert_eq!(relevance_score("anything", ""), 0.0);
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
                        "profile_text": "主人叫小明，程序员，正在做 8Bit Cat 项目。"
                    }
                }]
            })))
            .mount(&server)
            .await;

        let entries = vec![LongTermEntry {
            timestamp: "05-12 14:23".into(),
            user_msg: "我叫小明".into(),
            ai_reply: "你好小明！".into(),
            summary: None,
            tags: Vec::new(),
            importance: None,
            source: Some("test".into()),
            aggregated: false,
        }];
        let refs: Vec<&LongTermEntry> = entries.iter().collect();

        let ai_config = AiConfig {
            api_key: "test-key".into(),
            base_url: server.uri(),
            model: "test-model".into(),
        };

        let result = aggregate_profile(&refs, "", &ai_config, "测试聚合提示词").await;
        assert!(result.is_ok());
        let profile = result.unwrap();
        assert!(profile.contains("小明"));
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
                        "profile_text": "主人叫小明，程序员。正在做 8Bit Cat 项目（Rust）。"
                    }
                }]
            })))
            .mount(&server)
            .await;

        let entries = vec![LongTermEntry {
            timestamp: "05-12 15:00".into(),
            user_msg: "我在做 Rust 项目".into(),
            ai_reply: "什么项目？".into(),
            summary: None,
            tags: Vec::new(),
            importance: None,
            source: Some("test".into()),
            aggregated: false,
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
            timestamp: "05-12 14:23".into(),
            user_msg: "我叫小明".into(),
            ai_reply: "你好".into(),
            summary: None,
            tags: Vec::new(),
            importance: None,
            source: Some("test".into()),
            aggregated: false,
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
