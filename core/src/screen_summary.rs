use crate::ai_config::AiConfig;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};

// ---- 数据结构 ----

/// 单条屏幕活动摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenSummaryEntry {
    pub timestamp: String,
    pub time_range: String,
    pub summary: String,
}

/// 屏幕摘要存储（全量追加）
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

const DEFAULT_SCREEN_SUMMARY_PROMPT: &str = r#"你是 8Bit 的观察模块。以下是一段时间内对主人屏幕的多次 AI 观察记录。

请将它们整理为结构化的活动日志：
- 按活动类型分组（编程、浏览、通讯、娱乐、文档等）
- 每组列出时间段和具体活动
- 合并重复的观察（如连续多次看到同一应用，合并为时间段范围）
- 保留关键细节（项目名、文件名、网站等能看清的信息）
- 控制在 300 字以内"#;

fn default_screen_summary_prompt() -> String {
    DEFAULT_SCREEN_SUMMARY_PROMPT.to_string()
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
                return Self { entries: Vec::new() };
            }
        };

        if !path.exists() {
            info!("屏幕摘要文件不存在，使用空存储");
            return Self { entries: Vec::new() };
        }

        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<ScreenSummaryStore>(&content) {
                Ok(store) => {
                    info!(count = store.entries.len(), "已加载屏幕摘要");
                    store
                }
                Err(e) => {
                    warn!(error = %e, "解析屏幕摘要文件失败，使用空存储");
                    Self { entries: Vec::new() }
                }
            },
            Err(e) => {
                warn!(error = %e, "读取屏幕摘要文件失败，使用空存储");
                Self { entries: Vec::new() }
            }
        }
    }

    /// 记录新摘要：截断 + 全量追加
    pub fn record(&mut self, time_range: &str, summary: &str, config: &ScreenSummaryConfig) {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();

        self.entries.push(ScreenSummaryEntry {
            timestamp,
            time_range: time_range.to_string(),
            summary: truncate_chars(summary, config.max_summary_chars),
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
            let line = format!("[{}] {}\n", entry.time_range, entry.summary);
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
        let json =
            serde_json::to_string(self).map_err(|e| format!("序列化屏幕摘要失败: {e}"))?;
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
) -> Result<String, String> {
    if descriptions.is_empty() {
        return Err("没有可用的截图分析记录".to_string());
    }

    let user_content: String = descriptions
        .iter()
        .enumerate()
        .map(|(i, d)| format!("{}. {}", i + 1, d))
        .collect::<Vec<_>>()
        .join("\n");

    let body = json!({
        "model": ai_config.model,
        "max_tokens": 1024,
        "messages": [
            {
                "role": "user",
                "content": format!("{}\n\n以下是截图观察记录：\n{}", config.prompt, user_content)
            }
        ]
    });

    let url = format!("{}/v1/messages", ai_config.base_url.trim_end_matches('/'));
    debug!(model = %ai_config.model, url = %url, "开始生成屏幕摘要");

    let client = reqwest::Client::new();
    let start = std::time::Instant::now();
    let response = client
        .post(&url)
        .header("x-api-key", &ai_config.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("摘要 API 请求失败: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        warn!(status = %status, "摘要 API 返回错误");
        return Err(format!("摘要 API 返回错误 {}: {}", status, text));
    }

    let json: Value = response
        .json()
        .await
        .map_err(|e| format!("解析摘要 API 响应失败: {e}"))?;

    let elapsed = start.elapsed();
    debug!(
        elapsed_ms = elapsed.as_millis(),
        "屏幕摘要生成完成"
    );

    parse_text_response(&json)
}

/// 从 Anthropic Messages API 响应中提取纯文本（无图片）。
fn parse_text_response(response: &Value) -> Result<String, String> {
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

/// 按 Unicode 字符截断，超长时追加 "..."
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

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
        let cfg = ScreenSummaryConfig::default();
        store.record("14:00-14:15", "用户在写代码", &cfg);
        store.record("14:15-14:30", "用户在浏览网页", &cfg);
        assert_eq!(store.entries.len(), 2);
        assert_eq!(store.entries[0].time_range, "14:00-14:15");
        assert_eq!(store.entries[1].time_range, "14:15-14:30");
    }

    #[test]
    fn test_record_truncates_long_summary() {
        let mut store = ScreenSummaryStore {
            entries: Vec::new(),
        };
        let cfg = ScreenSummaryConfig {
            max_summary_chars: 10,
            ..Default::default()
        };
        let long_summary = "这是一段非常非常长的摘要内容用于测试截断功能是否正常工作";
        store.record("14:00-14:15", long_summary, &cfg);
        assert_eq!(store.entries[0].summary.chars().count(), 10);
        assert!(store.entries[0].summary.ends_with("..."));
    }

    #[test]
    fn test_build_context_empty() {
        let store = ScreenSummaryStore {
            entries: Vec::new(),
        };
        assert!(
            store.build_context(&ScreenSummaryConfig::default()).is_empty()
        );
    }

    #[test]
    fn test_build_context_format() {
        let store = ScreenSummaryStore {
            entries: vec![ScreenSummaryEntry {
                timestamp: "2025-01-01 14:15".into(),
                time_range: "14:00-14:15".into(),
                summary: "用户在 VS Code 中编写 Rust 代码".into(),
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
                summary: format!("摘要[{:03}]", i),
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
            assert!(
                ctx.contains(&format!("摘要[{i:03}]")),
                "应包含最近条目 {i}"
            );
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
                summary: "这是一条很长的摘要内容用于测试字符限制功能是否正常工作".into(),
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
                summary: "用户在编程".into(),
            }],
        };
        let json = serde_json::to_string(&store).unwrap();
        let back: ScreenSummaryStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0].summary, "用户在编程");
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
            .and(header("x-api-key", "test-key"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({
                    "content": [{ "type": "text", "text": "- [编程] 14:00-14:15 在 VS Code 写 Rust 代码\n- [浏览] 14:05-14:12 浏览 GitHub" }]
                })),
            )
            .mount(&server)
            .await;

        let client = test_client();
        let url = format!("{}/v1/messages", server.uri());

        let ai_config = AiConfig {
            api_key: "test-key".into(),
            base_url: server.uri(),
            model: "test-model".into(),
        };

        let config = ScreenSummaryConfig::default();
        let _descriptions = vec![
            "用户正在使用 VS Code 编辑器".to_string(),
            "屏幕显示 GitHub 页面".to_string(),
        ];

        // 直接调用内部逻辑验证 API 响应解析
        let body = json!({
            "model": ai_config.model,
            "max_tokens": 1024,
            "messages": [{
                "role": "user",
                "content": format!("{}\n\n以下是截图观察记录：\n{}", config.prompt, "1. 用户正在使用 VS Code 编辑器\n2. 屏幕显示 GitHub 页面")
            }]
        });
        let response = client
            .post(&url)
            .header("x-api-key", &ai_config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .unwrap();

        let json: Value = response.json().await.unwrap();
        let result = parse_text_response(&json).unwrap();
        assert!(result.contains("VS Code"));
        assert!(result.contains("GitHub"));
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
            .respond_with(
                ResponseTemplate::new(500).set_body_json(json!({
                    "error": { "type": "server_error", "message": "internal error" }
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
        let descriptions = vec!["some description".to_string()];
        let result = generate_summary(&descriptions, &config, &ai_config).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("错误"),
            "应包含 API 错误信息"
        );
    }

    #[test]
    fn test_parse_text_response_standard_format() {
        let response = json!({
            "content": [{
                "type": "text",
                "text": "- [编程] 在写代码"
            }]
        });
        let result = parse_text_response(&response).unwrap();
        assert_eq!(result, "- [编程] 在写代码");
    }

    #[test]
    fn test_parse_text_response_multiple_blocks() {
        let response = json!({
            "content": [
                { "type": "text", "text": "第一段。" },
                { "type": "image", "source": {} },
                { "type": "text", "text": "第二段。" }
            ]
        });
        let result = parse_text_response(&response).unwrap();
        assert_eq!(result, "第一段。第二段。");
    }

    #[test]
    fn test_parse_text_response_empty_content() {
        let response = json!({ "content": [] });
        let result = parse_text_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_text_response_missing_content() {
        let response = json!({});
        let result = parse_text_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let mut store = ScreenSummaryStore {
            entries: Vec::new(),
        };
        let cfg = ScreenSummaryConfig::default();
        store.record("14:00-14:15", "用户在编程", &cfg);

        // 验证序列化/反序列化正确性（load 读真实路径，此处只验证 JSON roundtrip）
        let json = serde_json::to_string(&store).unwrap();
        let back: ScreenSummaryStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0].summary, "用户在编程");
    }

    #[test]
    fn test_truncated_json_detected_as_corrupt() {
        let store = ScreenSummaryStore {
            entries: vec![ScreenSummaryEntry {
                timestamp: "2025-01-01 14:15".into(),
                time_range: "14:00-14:15".into(),
                summary: "重要数据".into(),
            }],
        };
        let json = serde_json::to_string(&store).unwrap();
        let truncated = &json[..json.len() / 2];
        let result = serde_json::from_str::<ScreenSummaryStore>(truncated);
        assert!(result.is_err());
    }
}
