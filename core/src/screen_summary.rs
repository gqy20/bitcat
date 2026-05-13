//! 屏幕活动摘要：将定时截图分析结果聚合为结构化事件日志并注入 AI 上下文。
//!
//! 本模块定期收集 Vision API 的截图分析描述，调用 AI 将它们压缩为
//! [`StructuredSummary`]（活动分组 + 显著变化），以滚动窗口方式保留最近 N 条，
//! 持久化到 `~/.ai-pad/memory/screen_summary.json`。
//!
//! 与 [`vision`](crate::vision) 模块协作：vision 负责单帧分析，
//! 本模块负责跨帧聚合和上下文注入，供 agent prompt 使用。
//! 存储模式为全量追加，上下文构建时按 [`ScreenSummaryConfig`] 截取最近条目。

use crate::ai_config::AiConfig;
use crate::token_tracker::{
    TokenCategory, TokenRecord, TokenUsage, new_session_id, record_token_usage,
};
use rig::client::CompletionClient;
use rig::providers::anthropic;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};

// ---- 数据结构 ----

/// 单条屏幕活动摘要，包含时间戳、时间范围和结构化内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenSummaryEntry {
    pub timestamp: String,
    pub time_range: String,
    pub summary: StructuredSummary,
}

/// AI 生成的结构化摘要：按活动类型分组 + 显著变化列表。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StructuredSummary {
    #[serde(default)]
    pub time_range: String,
    #[serde(default)]
    pub activities: Vec<ActivityGroup>,
    #[serde(default)]
    pub notable_changes: Vec<String>,
}

impl StructuredSummary {
    /// 将结构化摘要格式化为可读的多行文本，供 prompt 上下文注入。
    pub fn to_context_text(&self) -> String {
        let mut lines = Vec::new();

        for group in &self.activities {
            let range = if group.time_range.is_empty() {
                self.time_range.as_str()
            } else {
                group.time_range.as_str()
            };
            let items = if group.items.is_empty() {
                "无细节".to_string()
            } else {
                group.items.join("；")
            };
            lines.push(format!("[{}] {}: {}", range, group.category.label(), items));
        }

        for change in &self.notable_changes {
            lines.push(format!("[变化] {change}"));
        }

        if lines.is_empty() {
            if self.time_range.is_empty() {
                "无显著屏幕活动".to_string()
            } else {
                format!("[{}] 无显著屏幕活动", self.time_range)
            }
        } else {
            lines.join("\n")
        }
    }
}

/// 单个活动分组：类别、时间范围和具体事项列表。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ActivityGroup {
    #[serde(default)]
    pub category: ActivityCategory,
    #[serde(default)]
    pub time_range: String,
    #[serde(default)]
    pub items: Vec<String>,
}

/// 活动类别枚举，与 AI 输出的 JSON schema 对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivityCategory {
    Coding,
    Browsing,
    Communication,
    Entertainment,
    Documents,
    Other,
}

impl Default for ActivityCategory {
    fn default() -> Self {
        Self::Other
    }
}

impl ActivityCategory {
    /// 返回用于日志和上下文输出的英文标签。
    pub fn label(self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Browsing => "browsing",
            Self::Communication => "communication",
            Self::Entertainment => "entertainment",
            Self::Documents => "documents",
            Self::Other => "other",
        }
    }
}

/// 屏幕摘要存储，全量追加并持久化到 `~/.ai-pad/memory/screen_summary.json`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenSummaryStore {
    pub entries: Vec<ScreenSummaryEntry>,
}

/// 摘要系统配置（来自 prompts.yml 的 screen_summary 段）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScreenSummaryConfig {
    #[serde(default = "default_interval_min")]
    pub interval_min: u32,
    #[serde(default = "default_max_recent_analyses")]
    pub max_recent_analyses: u32,
    #[serde(default = "default_screen_summary_prompt")]
    pub prompt: String,
    #[serde(default = "default_max_summary_chars")]
    pub max_summary_chars: usize,
    #[serde(default = "default_max_context_entries")]
    pub max_context_entries: usize,
    #[serde(default = "default_max_context_chars")]
    pub max_context_chars: usize,
}

fn default_interval_min() -> u32 {
    15
}
fn default_max_recent_analyses() -> u32 {
    30
}
fn default_max_summary_chars() -> usize {
    500
}
fn default_max_context_entries() -> usize {
    20
}
fn default_max_context_chars() -> usize {
    2000
}

/// 从内嵌 config/prompts.yml 提取 screen_summary.prompt 作为默认值，
/// 确保与 YAML 保持同步。
fn default_screen_summary_prompt() -> String {
    const EMBEDDED: &str = include_str!("../../config/prompts.yml");
    let cfg: serde_yaml::Value =
        serde_yaml::from_str(EMBEDDED).expect("内嵌 config/prompts.yml 损坏");
    cfg.get("screen_summary")
        .and_then(|v| v.get("prompt"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

impl Default for ScreenSummaryConfig {
    fn default() -> Self {
        Self {
            interval_min: default_interval_min(),
            max_recent_analyses: default_max_recent_analyses(),
            prompt: default_screen_summary_prompt(),
            max_summary_chars: default_max_summary_chars(),
            max_context_entries: default_max_context_entries(),
            max_context_chars: default_max_context_chars(),
        }
    }
}

// ---- 存储路径 ----

fn summary_file_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取 HOME 目录".to_string())?;
    Ok(home
        .join(".ai-pad")
        .join("memory")
        .join("screen_summary.json"))
}

// ---- 核心操作 ----

impl ScreenSummaryStore {
    /// 从磁盘加载。文件不存在或损坏时返回空存储。
    pub fn load() -> Self {
        let path = match summary_file_path() {
            Ok(p) => p,
            Err(e) => {
                warn!(error = %e, "获取屏幕摘要文件路径失败");
                return Self {
                    entries: Vec::new(),
                };
            }
        };

        if !path.exists() {
            info!("屏幕摘要文件不存在，使用空存储");
            return Self {
                entries: Vec::new(),
            };
        }

        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<ScreenSummaryStore>(&content) {
                Ok(store) => {
                    info!(count = store.entries.len(), "已加载屏幕摘要");
                    store
                }
                Err(e) => {
                    warn!(error = %e, "解析屏幕摘要文件失败，使用空存储");
                    Self {
                        entries: Vec::new(),
                    }
                }
            },
            Err(e) => {
                warn!(error = %e, "读取屏幕摘要文件失败，使用空存储");
                Self {
                    entries: Vec::new(),
                }
            }
        }
    }

    /// 记录新摘要：全量追加
    pub fn record(&mut self, time_range: &str, mut summary: StructuredSummary) {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        if summary.time_range.is_empty() {
            summary.time_range = time_range.to_string();
        }

        self.entries.push(ScreenSummaryEntry {
            timestamp,
            time_range: time_range.to_string(),
            summary,
        });
    }

    /// 构建注入 prompt 的上下文文本。空存储返回空字符串。
    pub fn build_context(&self, config: &ScreenSummaryConfig) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        // 取最近 N 条（从最新到最旧），反转回时间顺序
        let recent: Vec<&ScreenSummaryEntry> = self
            .entries
            .iter()
            .rev()
            .take(config.max_context_entries)
            .collect();

        let header = "[屏幕活动摘要]\n";
        let mut result = String::from(header);

        for entry in recent.iter().rev() {
            let line = format!("{}\n", entry.summary.to_context_text());
            if result.chars().count() + line.chars().count() > config.max_context_chars {
                break;
            }
            result.push_str(&line);
        }
        result.push_str("[/屏幕活动摘要]\n");

        result
    }

    /// 持久化到磁盘（原子写入：先写临时文件再 rename）
    pub fn save(&self) -> Result<(), String> {
        let path = summary_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建摘要目录失败: {e}"))?;
        }
        let json = serde_json::to_string(self).map_err(|e| format!("序列化屏幕摘要失败: {e}"))?;
        let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())
            .map_err(|e| format!("创建临时文件失败: {e}"))?;
        std::io::Write::write_all(&mut tmp, json.as_bytes())
            .map_err(|e| format!("写入临时文件失败: {e}"))?;
        tmp.persist(&path)
            .map_err(|e| format!("原子替换屏幕摘要文件失败: {e}"))?;
        debug!(path = ?path, "屏幕摘要已持久化");
        Ok(())
    }
}

// ---- AI 摘要生成 ----

/// 调用 AI 将多条截图分析描述聚合为结构化事件日志。
pub async fn generate_summary(
    descriptions: &[String],
    config: &ScreenSummaryConfig,
    ai_config: &AiConfig,
) -> Result<StructuredSummary, String> {
    if descriptions.is_empty() {
        return Err("没有可用的截图分析记录".to_string());
    }

    let user_content: String = descriptions
        .iter()
        .enumerate()
        .map(|(i, d)| format!("{}. {}", i + 1, d))
        .collect::<Vec<_>>()
        .join("\n");

    debug!(
        model = %ai_config.model,
        base_url = %ai_config.base_url,
        "开始生成屏幕摘要"
    );

    let http_client = rig::http_client::ReqwestClient::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("创建摘要 HTTP 客户端失败: {e}"))?;
    let client = anthropic::Client::builder()
        .api_key(&ai_config.api_key)
        .base_url(&ai_config.base_url)
        .http_client(http_client)
        .build()
        .map_err(|e| format!("创建 Anthropic 摘要 Client 失败: {e}"))?;
    let extractor = client
        .extractor::<StructuredSummary>(ai_config.model.as_str())
        .preamble(&config.prompt)
        .max_tokens(1024)
        .retries(1)
        .build();

    let start = std::time::Instant::now();
    let response = extractor
        .extract_with_usage(format!("以下是截图观察记录：\n{user_content}"))
        .await
        .map_err(|e| format!("生成结构化屏幕摘要失败: {e}"))?;

    let elapsed = start.elapsed();
    info!(
        elapsed_ms = elapsed.as_millis(),
        activities = response.data.activities.len(),
        "屏幕摘要生成完成"
    );

    record_token_usage(
        &TokenRecord::new(
            new_session_id(),
            TokenCategory::ScreenSummary,
            ai_config.model.clone(),
            TokenUsage::from(response.usage),
        )
        .with_elapsed_ms(elapsed.as_millis() as u64),
    );

    Ok(response.data)
}

/// 从 Anthropic Messages API 响应中提取 text blocks。
fn extract_text_response(response: &Value) -> Result<String, String> {
    let content = response
        .get("content")
        .ok_or_else(|| "响应缺少 content 字段".to_string())?
        .as_array()
        .ok_or_else(|| "content 不是数组".to_string())?;

    let texts: Vec<String> = content
        .iter()
        .filter_map(|block| {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                block.get("text").and_then(|t| t.as_str()).map(String::from)
            } else {
                None
            }
        })
        .collect();

    if texts.is_empty() {
        return Err("响应中没有文本内容".to_string());
    }
    Ok(texts.join(""))
}

/// 从 API 响应中提取文本并反序列化为 [`StructuredSummary`]，自动剥离 markdown 代码围栏。
pub fn parse_structured_summary_response(response: &Value) -> Result<StructuredSummary, String> {
    let text = extract_text_response(response)?;
    let json_text = normalize_json_text(&text);
    serde_json::from_str::<StructuredSummary>(json_text)
        .map_err(|e| format!("解析结构化屏幕摘要失败: {e}"))
}

fn normalize_json_text(text: &str) -> &str {
    let trimmed = text.trim();
    let Some(after_opening) = trimmed.strip_prefix("```") else {
        return trimmed;
    };

    let after_language = after_opening
        .trim_start()
        .strip_prefix("json")
        .unwrap_or(after_opening)
        .trim_start();

    match after_language.rfind("```") {
        Some(end) => after_language[..end].trim(),
        None => after_language.trim(),
    }
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn summary_with_item(time_range: &str, item: impl Into<String>) -> StructuredSummary {
        StructuredSummary {
            time_range: time_range.into(),
            activities: vec![ActivityGroup {
                category: ActivityCategory::Other,
                time_range: time_range.into(),
                items: vec![item.into()],
            }],
            notable_changes: Vec::new(),
        }
    }

    #[test]
    fn test_default_config() {
        let cfg = ScreenSummaryConfig::default();
        assert_eq!(cfg.interval_min, 15);
        assert_eq!(cfg.max_recent_analyses, 30);
        assert_eq!(cfg.max_summary_chars, 500);
        assert_eq!(cfg.max_context_entries, 20);
        assert_eq!(cfg.max_context_chars, 2000);
        assert!(!cfg.prompt.is_empty());
        assert!(cfg.prompt.contains("结构化"));
    }

    #[test]
    fn test_record_appends_entry() {
        let mut store = ScreenSummaryStore {
            entries: Vec::new(),
        };
        store.record(
            "14:00-14:15",
            summary_with_item("14:00-14:15", "用户在写代码"),
        );
        store.record(
            "14:15-14:30",
            summary_with_item("14:15-14:30", "用户在浏览网页"),
        );
        assert_eq!(store.entries.len(), 2);
        assert_eq!(store.entries[0].time_range, "14:00-14:15");
        assert_eq!(store.entries[1].time_range, "14:15-14:30");
    }

    #[test]
    fn test_record_fills_empty_summary_time_range() {
        let mut store = ScreenSummaryStore {
            entries: Vec::new(),
        };
        store.record(
            "14:00-14:15",
            StructuredSummary {
                time_range: String::new(),
                activities: Vec::new(),
                notable_changes: Vec::new(),
            },
        );
        assert_eq!(store.entries[0].summary.time_range, "14:00-14:15");
    }

    #[test]
    fn test_build_context_empty() {
        let store = ScreenSummaryStore {
            entries: Vec::new(),
        };
        assert!(
            store
                .build_context(&ScreenSummaryConfig::default())
                .is_empty()
        );
    }

    #[test]
    fn test_build_context_format() {
        let store = ScreenSummaryStore {
            entries: vec![ScreenSummaryEntry {
                timestamp: "2025-01-01 14:15".into(),
                time_range: "14:00-14:15".into(),
                summary: summary_with_item("14:00-14:15", "用户在 VS Code 中编写 Rust 代码"),
            }],
        };
        let ctx = store.build_context(&ScreenSummaryConfig::default());
        assert!(ctx.contains("[屏幕活动摘要]"));
        assert!(ctx.contains("14:00-14:15"));
        assert!(ctx.contains("VS Code"));
        assert!(ctx.contains("[/屏幕活动摘要]"));
    }

    #[test]
    fn test_build_context_respects_max_entries() {
        let mut store = ScreenSummaryStore {
            entries: Vec::new(),
        };
        for i in 0..30 {
            store.entries.push(ScreenSummaryEntry {
                timestamp: format!("2025-01-01 {:02}:{:02}", 14, i),
                time_range: format!("{:02}:{:02}-{:02}:{:02}", 14, i, 14, i + 1),
                summary: summary_with_item(
                    &format!("{:02}:{:02}-{:02}:{:02}", 14, i, 14, i + 1),
                    format!("摘要[{i:03}]"),
                ),
            });
        }
        let cfg = ScreenSummaryConfig {
            max_context_entries: 5,
            ..Default::default()
        };
        let ctx = store.build_context(&cfg);
        // 应只包含最近 5 条（编号最大的 5 个）
        for i in 0..25 {
            assert!(
                !ctx.contains(&format!("摘要[{i:03}]")),
                "不应包含旧条目 {i}"
            );
        }
        for i in 25..30 {
            assert!(ctx.contains(&format!("摘要[{i:03}]")), "应包含最近条目 {i}");
        }
    }

    #[test]
    fn test_build_context_respects_char_limit() {
        let mut store = ScreenSummaryStore {
            entries: Vec::new(),
        };
        for _ in 0..50 {
            store.entries.push(ScreenSummaryEntry {
                timestamp: "2025-01-01 14:00".into(),
                time_range: "14:00-14:15".into(),
                summary: summary_with_item(
                    "14:00-14:15",
                    "这是一条很长的摘要内容用于测试字符限制功能是否正常工作",
                ),
            });
        }
        let cfg = ScreenSummaryConfig {
            max_context_chars: 200,
            ..Default::default()
        };
        let ctx = store.build_context(&cfg);
        assert!(ctx.chars().count() <= 250);
    }

    #[test]
    fn test_json_roundtrip() {
        let store = ScreenSummaryStore {
            entries: vec![ScreenSummaryEntry {
                timestamp: "2025-01-01 14:15".into(),
                time_range: "14:00-14:15".into(),
                summary: summary_with_item("14:00-14:15", "用户在编程"),
            }],
        };
        let json = serde_json::to_string(&store).unwrap();
        let back: ScreenSummaryStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0].summary.activities[0].items[0], "用户在编程");
    }

    #[test]
    fn test_summary_file_path() {
        let p = summary_file_path().unwrap();
        let s = p.to_string_lossy();
        assert!(s.contains(".ai-pad"), "应在 .ai-pad 下");
        assert!(s.contains("memory"), "应有 memory 子目录");
        assert!(s.ends_with("screen_summary.json"));
    }

    #[tokio::test]
    async fn test_generate_summary_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({
                    "type": "message",
                    "id": "msg_test",
                    "model": "test-model",
                    "role": "assistant",
                    "stop_reason": "tool_use",
                    "stop_sequence": null,
                    "usage": {
                        "input_tokens": 12,
                        "output_tokens": 8
                    },
                    "content": [{
                        "type": "tool_use",
                        "id": "toolu_test",
                        "name": "submit",
                        "input": {
                            "time_range":"14:00-14:15",
                            "activities":[
                                {"category":"coding","time_range":"14:00-14:10","items":["在 VS Code 写 Rust 代码"]},
                                {"category":"browsing","time_range":"14:10-14:15","items":["浏览 GitHub"]}
                            ],
                            "notable_changes":["从编辑器切换到浏览器"]
                        }
                    }]
                })),
            )
            .mount(&server)
            .await;

        let ai_config = AiConfig {
            api_key: "test-key".into(),
            base_url: server.uri(),
            model: "test-model".into(),
        };

        let config = ScreenSummaryConfig::default();
        let descriptions = vec![
            "用户正在使用 VS Code 编辑器".to_string(),
            "屏幕显示 GitHub 页面".to_string(),
        ];

        let result = generate_summary(&descriptions, &config, &ai_config)
            .await
            .unwrap();
        assert_eq!(result.time_range, "14:00-14:15");
        assert_eq!(result.activities.len(), 2);
        assert_eq!(result.activities[0].category, ActivityCategory::Coding);
        assert!(result.to_context_text().contains("GitHub"));
    }

    #[tokio::test]
    async fn test_generate_summary_empty_descriptions() {
        let ai_config = AiConfig {
            api_key: "test".into(),
            base_url: "https://api.anthropic.com".into(),
            model: "test".into(),
        };
        let config = ScreenSummaryConfig::default();
        let result = generate_summary(&[], &config, &ai_config).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("没有可用"));
    }

    #[tokio::test]
    async fn test_generate_summary_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_json(json!({
                "error": { "type": "server_error", "message": "internal error" }
            })))
            .mount(&server)
            .await;

        let ai_config = AiConfig {
            api_key: "test-key".into(),
            base_url: server.uri(),
            model: "test-model".into(),
        };

        let config = ScreenSummaryConfig::default();
        let descriptions = vec!["some description".to_string()];
        let result = generate_summary(&descriptions, &config, &ai_config).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("生成结构化屏幕摘要失败"),
            "应包含 rig extractor 错误信息"
        );
    }

    #[test]
    fn test_extract_text_response_standard_format() {
        let response = json!({
            "content": [{
                "type": "text",
                "text": "- [编程] 在写代码"
            }]
        });
        let result = extract_text_response(&response).unwrap();
        assert_eq!(result, "- [编程] 在写代码");
    }

    #[test]
    fn test_extract_text_response_multiple_blocks() {
        let response = json!({
            "content": [
                { "type": "text", "text": "第一段。" },
                { "type": "image", "source": {} },
                { "type": "text", "text": "第二段。" }
            ]
        });
        let result = extract_text_response(&response).unwrap();
        assert_eq!(result, "第一段。第二段。");
    }

    #[test]
    fn test_extract_text_response_empty_content() {
        let response = json!({ "content": [] });
        let result = extract_text_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_text_response_missing_content() {
        let response = json!({});
        let result = extract_text_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_structured_summary_response() {
        let response = json!({
            "content": [{
                "type": "text",
                "text": r#"{
                    "time_range":"14:00-14:15",
                    "activities":[{
                        "category":"coding",
                        "time_range":"14:00-14:12",
                        "items":["在 VS Code 中编写 Rust 代码","查看测试输出"]
                    }],
                    "notable_changes":["从浏览器切换到编辑器"]
                }"#
            }]
        });
        let summary = parse_structured_summary_response(&response).unwrap();
        assert_eq!(summary.time_range, "14:00-14:15");
        assert_eq!(summary.activities.len(), 1);
        assert_eq!(summary.activities[0].category, ActivityCategory::Coding);
        assert_eq!(summary.activities[0].items.len(), 2);
        assert_eq!(summary.notable_changes, vec!["从浏览器切换到编辑器"]);
    }

    #[test]
    fn test_parse_structured_summary_response_accepts_json_fence() {
        let response = json!({
            "content": [{
                "type": "text",
                "text": "```json\n{\"time_range\":\"15:00-15:15\",\"activities\":[{\"category\":\"browsing\",\"items\":[\"浏览 GitHub\"]}]}\n```"
            }]
        });
        let summary = parse_structured_summary_response(&response).unwrap();
        assert_eq!(summary.time_range, "15:00-15:15");
        assert_eq!(summary.activities[0].category, ActivityCategory::Browsing);
        assert_eq!(summary.activities[0].items, vec!["浏览 GitHub"]);
    }

    #[test]
    fn test_structured_summary_context_text() {
        let summary = StructuredSummary {
            time_range: "14:00-14:15".into(),
            activities: vec![ActivityGroup {
                category: ActivityCategory::Coding,
                time_range: "14:00-14:12".into(),
                items: vec!["在 VS Code 中编写 Rust 代码".into()],
            }],
            notable_changes: vec!["开始调试结构化输出".into()],
        };
        let context = summary.to_context_text();
        assert!(context.contains("14:00-14:12"));
        assert!(context.contains("coding"));
        assert!(context.contains("VS Code"));
        assert!(context.contains("开始调试结构化输出"));
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let mut store = ScreenSummaryStore {
            entries: Vec::new(),
        };
        store.record(
            "14:00-14:15",
            summary_with_item("14:00-14:15", "用户在编程"),
        );

        // 验证序列化/反序列化正确性（load 读真实路径，此处只验证 JSON roundtrip）
        let json = serde_json::to_string(&store).unwrap();
        let back: ScreenSummaryStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0].summary.activities[0].items[0], "用户在编程");
    }

    #[test]
    fn test_truncated_json_detected_as_corrupt() {
        let store = ScreenSummaryStore {
            entries: vec![ScreenSummaryEntry {
                timestamp: "2025-01-01 14:15".into(),
                time_range: "14:00-14:15".into(),
                summary: summary_with_item("14:00-14:15", "重要数据"),
            }],
        };
        let json = serde_json::to_string(&store).unwrap();
        let truncated = &json[..json.len() / 2];
        let result = serde_json::from_str::<ScreenSummaryStore>(truncated);
        assert!(result.is_err());
    }
}
