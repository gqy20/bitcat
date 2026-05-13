use rig::OneOrMany;
use rig::client::CompletionClient;
use rig::completion::Message;
use rig::message::{ImageDetail, ImageMediaType, UserContent};
use rig::providers::anthropic;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::ai_config::AiConfig;
use crate::prompts::VisionPromptConfig;
use crate::token_tracker::{
    TokenCategory, TokenRecord, TokenUsage, new_session_id, record_token_usage,
};
use tracing::{debug, info};

// ---- 配置 ----

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct VisionConfig {
    #[serde(default)]
    pub vision_model: Option<String>,
    #[serde(default)]
    pub vision_max_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VisionAnalysis {
    pub description: String,
    #[serde(default)]
    pub apps: Vec<String>,
    #[serde(default)]
    pub state: VisionState,
    #[serde(default)]
    pub text_readable: bool,
    #[serde(default)]
    pub confidence: f32,
}

impl VisionAnalysis {
    pub fn to_context_text(&self) -> String {
        let apps = if self.apps.is_empty() {
            "unknown".to_string()
        } else {
            self.apps.join(", ")
        };
        format!(
            "{} | apps: {} | state: {} | text_readable: {} | confidence: {:.2}",
            self.description,
            apps,
            self.state.label(),
            self.text_readable,
            self.confidence
        )
    }
}

impl Default for VisionAnalysis {
    fn default() -> Self {
        Self {
            description: String::new(),
            apps: Vec::new(),
            state: VisionState::Unknown,
            text_readable: false,
            confidence: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VisionState {
    Working { app: String },
    Idle,
    Media,
    OffScreen,
    Unknown,
}

impl Default for VisionState {
    fn default() -> Self {
        Self::Unknown
    }
}

impl VisionState {
    pub fn label(&self) -> &str {
        match self {
            Self::Working { .. } => "working",
            Self::Idle => "idle",
            Self::Media => "media",
            Self::OffScreen => "off_screen",
            Self::Unknown => "unknown",
        }
    }
}

// ---- 请求构建 ----

/// 构建标准 Anthropic Messages API 请求体（含图片）。
pub fn build_vision_request(model: &str, prompt: &str, base64_jpeg: &str) -> Value {
    json!({
        "model": model,
        "max_tokens": 1024,
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": prompt },
                {
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/jpeg",
                        "data": base64_jpeg
                    }
                }
            ]
        }]
    })
}

/// 多显示器变体：追加屏幕布局提示。
pub fn build_vision_request_multi(
    model: &str,
    prompt: &str,
    base64_jpeg: &str,
    monitor_count: usize,
    prompt_multi: &str,
) -> Value {
    let full_prompt = format!(
        "{}\n{}当前有 {} 个显示器。",
        prompt, prompt_multi, monitor_count
    );
    build_vision_request(model, &full_prompt, base64_jpeg)
}

// ---- 响应解析 ----

/// 从 Anthropic Messages API 响应中提取文本。
pub fn parse_vision_response(response: &Value) -> Result<String, String> {
    let content = response
        .get("content")
        .ok_or_else(|| "响应缺少 content 字段".to_string())?
        .as_array()
        .ok_or_else(|| "content 不是数组".to_string())?;

    let mut texts = Vec::new();
    for block in content {
        if block.get("type").and_then(|t| t.as_str()) == Some("text")
            && let Some(text) = block.get("text").and_then(|t| t.as_str())
        {
            texts.push(text.to_string());
        }
    }
    Ok(texts.join(""))
}

pub fn parse_vision_analysis_response(response: &Value) -> Result<VisionAnalysis, String> {
    let text = parse_vision_response(response)?;
    let json_text = normalize_json_text(&text);
    serde_json::from_str::<VisionAnalysis>(json_text)
        .map_err(|e| format!("解析结构化视觉分析失败: {e}"))
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

/// 构建完整 API URL。
pub fn build_api_url(config: &AiConfig) -> String {
    let base = config.base_url.trim_end_matches('/');
    format!("{}/v1/messages", base)
}

/// 发送视觉分析请求。返回结构化视觉分析。
pub async fn analyze_screenshot(
    config: &AiConfig,
    vision_config: &VisionConfig,
    prompt_config: &VisionPromptConfig,
    base64_jpeg: &str,
    monitor_count: usize,
) -> Result<VisionAnalysis, String> {
    let model = vision_config
        .vision_model
        .as_deref()
        .unwrap_or(&config.model);
    let prompt = if monitor_count > 1 {
        format!(
            "{}\n{}当前有 {} 个显示器。",
            prompt_config.prompt, prompt_config.prompt_multi, monitor_count
        )
    } else {
        prompt_config.prompt.clone()
    };

    debug!(
        model,
        base_url = %config.base_url,
        monitor_count,
        "视觉分析请求"
    );

    let http_client = rig::http_client::ReqwestClient::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("创建视觉 HTTP 客户端失败: {e}"))?;
    let client = anthropic::Client::builder()
        .api_key(&config.api_key)
        .base_url(&config.base_url)
        .http_client(http_client)
        .build()
        .map_err(|e| format!("创建 Anthropic 视觉 Client 失败: {e}"))?;
    let extractor = client
        .extractor::<VisionAnalysis>(model)
        .preamble(&prompt)
        .max_tokens(vision_config.vision_max_tokens.unwrap_or(1024) as u64)
        .retries(1)
        .build();

    let message = Message::User {
        content: OneOrMany::many([
            UserContent::text("请分析这张截图。"),
            UserContent::image_base64(
                base64_jpeg,
                Some(ImageMediaType::JPEG),
                Some(ImageDetail::Auto),
            ),
        ])
        .map_err(|e| format!("创建视觉消息失败: {e}"))?,
    };

    let start = std::time::Instant::now();
    let response = extractor
        .extract_with_usage(message)
        .await
        .map_err(|e| format!("生成结构化视觉分析失败: {e}"))?;
    let elapsed = start.elapsed();

    info!(
        elapsed_ms = elapsed.as_millis(),
        apps = response.data.apps.len(),
        state = response.data.state.label(),
        "视觉分析完成"
    );

    record_token_usage(
        &TokenRecord::new(
            new_session_id(),
            TokenCategory::Vision,
            model,
            TokenUsage::from(response.usage),
        )
        .with_elapsed_ms(elapsed.as_millis() as u64),
    );

    Ok(response.data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vision_prompt_is_non_empty() {
        let def = VisionPromptConfig::default();
        assert!(!def.prompt.is_empty());
        assert!(
            def.prompt.contains("不要瞎猜"),
            "应包含反幻觉规则（来自内嵌 YAML）"
        );
        assert!(
            def.prompt.contains("看不太清"),
            "应包含看不清指引（来自内嵌 YAML）"
        );
        assert!(def.prompt.contains("120 字"), "应使用 YAML 中的字数限制");
    }

    // ---- insta 快照测试：请求体结构 ----

    #[test]
    fn test_build_request_body_snapshot() {
        let body = build_vision_request("claude-sonnet-4-20250514", "test prompt", "AA==");
        insta::assert_yaml_snapshot!(body, {
            ".messages[0].content[0].text" => "[prompt]"
        });
    }

    #[test]
    fn test_build_request_body_multi_snapshot() {
        let def = VisionPromptConfig::default();
        let body = build_vision_request_multi(
            "claude-sonnet-4-20250514",
            "test prompt",
            "AA==",
            2,
            &def.prompt_multi,
        );
        insta::assert_yaml_snapshot!(body, {
            ".messages[0].content[0].text" => "[prompt]"
        });
    }

    // ---- insta 快照测试：配置反序列化 ----

    #[test]
    fn test_vision_config_snapshot() {
        let json = r#"{"vision_model":"claude-sonnet-4-20250514","vision_max_tokens":1024}"#;
        let cfg: VisionConfig = serde_json::from_str(json).unwrap();
        insta::assert_yaml_snapshot!(cfg);
    }

    #[test]
    fn test_vision_config_defaults() {
        let cfg = VisionConfig::default();
        assert!(cfg.vision_model.is_none());
        assert_eq!(cfg.vision_max_tokens, None);
    }

    // ---- 响应解析 ----

    #[test]
    fn test_parse_response_standard_format() {
        let response = json!({
            "content": [{
                "type": "text",
                "text": "用户正在使用 VS Code 编辑 Rust 代码。"
            }]
        });
        let result = parse_vision_response(&response).unwrap();
        assert_eq!(result, "用户正在使用 VS Code 编辑 Rust 代码。");
    }

    #[test]
    fn test_parse_response_empty_content() {
        let response = json!({ "content": [] });
        let result = parse_vision_response(&response).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_parse_response_missing_content() {
        let response = json!({});
        let result = parse_vision_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_multiple_text_blocks() {
        let response = json!({
            "content": [
                { "type": "text", "text": "第一段。" },
                { "type": "image", "source": {} },
                { "type": "text", "text": "第二段。" }
            ]
        });
        let result = parse_vision_response(&response).unwrap();
        assert_eq!(result, "第一段。第二段。");
    }

    #[test]
    fn test_parse_vision_analysis_response() {
        let response = json!({
            "content": [{
                "type": "text",
                "text": r#"{
                    "description":"用户正在 VS Code 中编写 Rust 代码",
                    "apps":["VS Code"],
                    "state":{"kind":"working","app":"VS Code"},
                    "text_readable":true,
                    "confidence":0.91
                }"#
            }]
        });
        let result = parse_vision_analysis_response(&response).unwrap();
        assert_eq!(result.description, "用户正在 VS Code 中编写 Rust 代码");
        assert_eq!(result.apps, vec!["VS Code"]);
        assert_eq!(
            result.state,
            VisionState::Working {
                app: "VS Code".into()
            }
        );
        assert!(result.text_readable);
        assert!((result.confidence - 0.91).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_vision_analysis_response_accepts_json_fence() {
        let response = json!({
            "content": [{
                "type": "text",
                "text": "```json\n{\"description\":\"桌面空闲\",\"state\":{\"kind\":\"idle\"}}\n```"
            }]
        });
        let result = parse_vision_analysis_response(&response).unwrap();
        assert_eq!(result.description, "桌面空闲");
        assert_eq!(result.state, VisionState::Idle);
        assert!(result.apps.is_empty());
        assert!(!result.text_readable);
    }

    #[test]
    fn test_vision_analysis_context_text() {
        let analysis = VisionAnalysis {
            description: "用户正在浏览文档".into(),
            apps: vec!["Browser".into()],
            state: VisionState::Working {
                app: "Browser".into(),
            },
            text_readable: true,
            confidence: 0.8,
        };
        let context = analysis.to_context_text();
        assert!(context.contains("用户正在浏览文档"));
        assert!(context.contains("Browser"));
        assert!(context.contains("working"));
        assert!(context.contains("0.80"));
    }

    // ---- API URL 构建 ----

    #[test]
    fn test_build_api_url_default() {
        let config = AiConfig {
            api_key: "test".into(),
            base_url: "https://api.anthropic.com".into(),
            model: "test".into(),
        };
        assert_eq!(
            build_api_url(&config),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn test_build_api_url_custom_base() {
        let config = AiConfig {
            api_key: "test".into(),
            base_url: "https://proxy.example.com".into(),
            model: "test".into(),
        };
        assert_eq!(
            build_api_url(&config),
            "https://proxy.example.com/v1/messages"
        );
    }

    #[test]
    fn test_build_api_url_trailing_slash() {
        let config = AiConfig {
            api_key: "test".into(),
            base_url: "https://proxy.example.com/".into(),
            model: "test".into(),
        };
        assert_eq!(
            build_api_url(&config),
            "https://proxy.example.com/v1/messages"
        );
    }
}

// ---- wiremock HTTP mock 测试 ----

#[cfg(test)]
mod wiremock_tests {
    use super::*;
    use crate::prompts::VisionPromptConfig;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_vision_api_success() {
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
                    "input_tokens": 21,
                    "output_tokens": 9
                },
                "content": [{
                    "type": "tool_use",
                    "id": "toolu_test",
                    "name": "submit",
                    "input": {
                        "description":"VS Code 编辑器",
                        "apps":["VS Code"],
                        "state":{"kind":"working","app":"VS Code"},
                        "text_readable":true,
                        "confidence":0.9
                    }
                }]
            })))
            .mount(&server)
            .await;

        let ai_config = AiConfig {
            api_key: "test-key".into(),
            base_url: server.uri(),
            model: "test-model".into(),
        };
        let vision_config = VisionConfig::default();
        let prompt_config = VisionPromptConfig::default();
        let result =
            analyze_screenshot(&ai_config, &vision_config, &prompt_config, "AA==", 1).await;
        let analysis = result.unwrap();
        assert_eq!(analysis.description, "VS Code 编辑器");
        assert_eq!(analysis.apps, vec!["VS Code"]);
    }

    #[tokio::test]
    async fn test_vision_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).set_body_json(json!({
                "error": { "type": "rate_limit_error", "message": "slow down" }
            })))
            .mount(&server)
            .await;

        let ai_config = AiConfig {
            api_key: "key".into(),
            base_url: server.uri(),
            model: "test-model".into(),
        };
        let vision_config = VisionConfig::default();
        let prompt_config = VisionPromptConfig::default();
        let result =
            analyze_screenshot(&ai_config, &vision_config, &prompt_config, "AA==", 1).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("生成结构化视觉分析失败"));
    }
}
