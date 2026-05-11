use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::ai_config::AiConfig;
use crate::prompts::VisionPromptConfig;
use tracing::{debug, warn};

// ---- 配置 ----

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct VisionConfig {
    #[serde(default)]
    pub vision_model: Option<String>,
    #[serde(default)]
    pub vision_max_tokens: Option<u32>,
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

/// 构建完整 API URL。
pub fn build_api_url(config: &AiConfig) -> String {
    let base = config.base_url.trim_end_matches('/');
    format!("{}/v1/messages", base)
}

/// 发送视觉分析 HTTP 请求并解析响应。
async fn send_vision_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    body: &Value,
) -> Result<String, String> {
    let start = std::time::Instant::now();
    let response = client
        .post(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|e| format!("视觉 API 请求失败: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        warn!(status = %status, "视觉 API 返回错误");
        return Err(format!("视觉 API 返回错误 {}: {}", status, text));
    }

    let json: Value = response
        .json()
        .await
        .map_err(|e| format!("解析视觉 API 响应失败: {e}"))?;
    let elapsed = start.elapsed();
    debug!(
        elapsed_ms = elapsed.as_millis(),
        chars = json.to_string().chars().count(),
        "视觉分析完成"
    );

    parse_vision_response(&json)
}

/// 发送视觉分析请求。返回 AI 描述文本。
pub async fn analyze_screenshot(
    config: &AiConfig,
    vision_config: &VisionConfig,
    prompt_config: &VisionPromptConfig,
    base64_jpeg: &str,
    monitor_count: usize,
) -> Result<String, String> {
    let model = vision_config
        .vision_model
        .as_deref()
        .unwrap_or(&config.model);

    let body = if monitor_count > 1 {
        build_vision_request_multi(
            model,
            &prompt_config.prompt,
            base64_jpeg,
            monitor_count,
            &prompt_config.prompt_multi,
        )
    } else {
        build_vision_request(model, &prompt_config.prompt, base64_jpeg)
    };

    let url = build_api_url(config);
    debug!(model, url, "视觉分析请求");

    let client = reqwest::Client::new();
    send_vision_request(&client, &url, &config.api_key, &body).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vision_prompt_is_non_empty() {
        let def = VisionPromptConfig::default();
        assert!(!def.prompt.is_empty());
        assert!(def.prompt.contains("不要"));
        assert!(def.prompt.contains("编造"));
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
    use wiremock::matchers::{header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    #[tokio::test]
    async fn test_vision_api_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("x-api-key", "test-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{ "type": "text", "text": "VS Code 编辑器" }]
            })))
            .mount(&server)
            .await;

        let client = test_client();
        let url = format!("{}/v1/messages", server.uri());
        let body = build_vision_request("test-model", "prompt", "AA==");
        let result = send_vision_request(&client, &url, "test-key", &body).await;
        assert_eq!(result.unwrap(), "VS Code 编辑器");
    }

    #[tokio::test]
    async fn test_vision_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(429).set_body_json(json!({
                    "error": { "type": "rate_limit_error", "message": "slow down" }
                })),
            )
            .mount(&server)
            .await;

        let client = test_client();
        let url = format!("{}/v1/messages", server.uri());
        let body = build_vision_request("test-model", "prompt", "AA==");
        let result = send_vision_request(&client, &url, "key", &body).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("429"));
    }
}
