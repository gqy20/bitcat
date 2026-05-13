//! 视觉分析：构建 Anthropic Messages 图片分析请求并解析结构化响应。
//!
//! 将截图的 base64 JPEG 发送给 Vision API，通过 JSON schema 约束返回
//! [`VisionAnalysis`]（描述、应用列表、屏幕状态、置信度）等结构化字段，
//! 确保下游可程序化处理而非依赖自由文本。
//!
//! 与 [`app/src/screenshot.rs`](crate::screenshot) 协作：app 侧负责 BitBlt 截屏
//! 并调用本模块的 [`analyze_screenshot`] 完成 API 交互，结果经
//! [`screen_summary`](crate::screen_summary) 聚合后注入 AI prompt 上下文。

use rig::OneOrMany;
use rig::client::CompletionClient;
use rig::completion::Message;
use rig::message::{ImageDetail, ImageMediaType, UserContent};
use rig::providers::anthropic;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::ai_config::AiConfig;
use crate::prompts::VisionPromptConfig;
use crate::token_tracker::{
    TokenCategory, TokenRecord, TokenUsage, new_session_id, record_token_usage,
};
use tracing::{debug, info};

// ---- 配置 ----

/// Vision API 的可选模型与 token 上限配置，来自 prompts.yml 的 vision 段。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct VisionConfig {
    #[serde(default)]
    pub vision_model: Option<String>,
    #[serde(default)]
    pub vision_max_tokens: Option<u32>,
}

/// Vision API 返回的结构化分析结果，通过 JSON schema 约束模型输出。
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
    /// 将分析结果格式化为单行上下文文本，供 prompt 注入使用。
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

/// 屏幕状态的分类枚举，作为 VisionAnalysis 的子字段。
///
/// serde 使用 `tag = "kind"` 内部标签枚举，但 schemars 默认对内部标签枚举生成 `oneOf`，
/// 而 `glm-5v-turbo`（智谱）对 `oneOf` 类型会返回字符串化的 JSON 而非内联对象。
/// 因此手动实现 `JsonSchema`，生成扁平的 `type: object` schema（kind 为 enum + app 为可选），
/// 避免触发模型的 `oneOf` 兼容性问题。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VisionState {
    Working {
        app: String,
    },
    Idle,
    Media,
    OffScreen,
    #[default]
    Unknown,
}

impl<'de> Deserialize<'de> for VisionState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case")]
        enum RawVisionState {
            Working { app: Option<String> },
            Idle,
            Media,
            OffScreen,
            Unknown,
        }

        fn from_raw(raw: RawVisionState) -> VisionState {
            match raw {
                RawVisionState::Working { app } => VisionState::Working {
                    app: app.unwrap_or_default(),
                },
                RawVisionState::Idle => VisionState::Idle,
                RawVisionState::Media => VisionState::Media,
                RawVisionState::OffScreen => VisionState::OffScreen,
                RawVisionState::Unknown => VisionState::Unknown,
            }
        }

        let value = serde_json::Value::deserialize(deserializer)?;
        if let Some(raw) = value.as_str() {
            if let Ok(raw_state) = serde_json::from_str::<RawVisionState>(raw) {
                return Ok(from_raw(raw_state));
            }
            return match raw {
                "working" => Ok(VisionState::Working { app: String::new() }),
                "idle" => Ok(VisionState::Idle),
                "media" => Ok(VisionState::Media),
                "off_screen" => Ok(VisionState::OffScreen),
                "unknown" => Ok(VisionState::Unknown),
                other => Err(serde::de::Error::unknown_variant(
                    other,
                    &["working", "idle", "media", "off_screen", "unknown"],
                )),
            };
        }

        serde_json::from_value::<RawVisionState>(value)
            .map(from_raw)
            .map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for VisionState {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "VisionState".into()
    }

    fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "properties": {
                "kind": {
                    "type": "string",
                    "enum": ["working", "idle", "media", "off_screen", "unknown"]
                },
                "app": { "type": "string" }
            },
            "required": ["kind"]
        })
    }
}

impl VisionState {
    /// 返回用于日志和上下文的简短标签字符串。
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

    #[test]
    fn test_vision_state_deserializes_stringified_object() {
        let state: VisionState = serde_json::from_str(r#""{\"kind\":\"working\"}""#).unwrap();
        assert_eq!(state, VisionState::Working { app: String::new() });
    }

    #[test]
    fn test_vision_state_deserializes_string_label() {
        let state: VisionState = serde_json::from_str(r#""off_screen""#).unwrap();
        assert_eq!(state, VisionState::OffScreen);
    }
}

// ---- wiremock HTTP mock 测试 ----

#[cfg(test)]
mod wiremock_tests {
    use super::*;
    use crate::prompts::VisionPromptConfig;
    use serde_json::json;
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
