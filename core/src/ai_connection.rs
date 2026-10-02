//! AI connection drafts and an isolated Anthropic Messages connectivity check.
//! Drafts preserve existing credentials unless replacement or removal is explicit.
//! Settings IPC resolves drafts through AiConfig, then checks without persisting or sending conversation history.

use crate::ai_config::AiConfig;
use crate::app_settings::AiOverride;
use crate::token_tracker::TokenUsage;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Settings-only edit payload; absent API keys mean keep the saved credential.
#[derive(Clone, Default, Deserialize)]
pub struct ConnectionDraft {
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_saved_key: bool,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub max_tokens: Option<u64>,
}

impl ConnectionDraft {
    /// Validate and merge a draft without modifying storage or exposing a saved key.
    pub fn apply_to(&self, current: &AiOverride) -> Result<AiOverride, String> {
        if self.clear_saved_key && self.api_key.is_some() {
            return Err("不能同时替换和清除密钥，请选择一种操作。".into());
        }
        if self
            .api_key
            .as_ref()
            .is_some_and(|key| key.trim().is_empty())
        {
            return Err("密钥不能只含空格，请重新粘贴或留空以保留现有密钥。".into());
        }
        let base_url = self
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(url) = base_url {
            messages_url(url).map_err(|_| {
                "服务地址无效，请填写完整的 http 或 https 地址，不要附带密钥或查询参数。"
                    .to_string()
            })?;
        }
        if self.max_tokens == Some(0) {
            return Err("回复长度上限必须大于零，或留空使用默认值。".into());
        }
        Ok(AiOverride {
            api_key: if self.clear_saved_key {
                None
            } else {
                self.api_key
                    .as_ref()
                    .map(|key| key.trim().to_string())
                    .or_else(|| current.api_key.clone())
            },
            base_url: base_url.map(str::to_string),
            model: self
                .model
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            max_tokens: self.max_tokens,
        })
    }
}

/// Only classified outcomes leave the check; raw provider errors may contain credentials.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Verified,
    MissingKey,
    InvalidConfig,
    Unauthorized,
    Forbidden,
    ModelUnavailable,
    RateLimited,
    NetworkError,
    TimedOut,
    InvalidResponse,
    ServiceError,
}

/// A check validates one minimal text response, not every tool or vision capability.
#[derive(Debug, Serialize)]
pub struct ConnectionCheck {
    pub status: ConnectionStatus,
    pub elapsed_ms: u64,
    #[serde(skip)]
    pub usage: Option<TokenUsage>,
}

impl ConnectionCheck {
    /// Construct a local failure before a request has been sent.
    pub fn local(status: ConnectionStatus) -> Self {
        Self {
            status,
            elapsed_ms: 0,
            usage: None,
        }
    }
}

fn messages_url(base: &str) -> Result<String, ()> {
    let url = reqwest::Url::parse(base).map_err(|_| ())?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(());
    }
    // Match rig's Anthropic URL normalization, including gateway path prefixes.
    let trimmed = base.trim_end_matches('/');
    let root = ["/v1/messages", "/messages", "/v1"]
        .into_iter()
        .find_map(|suffix| trimmed.strip_suffix(suffix))
        .unwrap_or(trimmed);
    Ok(format!("{root}/v1/messages"))
}

fn request_body(model: &str) -> serde_json::Value {
    serde_json::json!({"model": model, "max_tokens": 8, "stream": false,
        "messages": [{"role": "user", "content": "Reply OK."}]})
}

/// Send a user-triggered minimal request with a 15-second deadline and no redirects.
/// No saved settings or user conversation content are mutated or included.
pub async fn check_connection(config: &AiConfig) -> ConnectionCheck {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return ConnectionCheck::local(ConnectionStatus::NetworkError);
    };
    check_with_client(config, &client).await
}

async fn check_with_client(config: &AiConfig, client: &reqwest::Client) -> ConnectionCheck {
    let started = Instant::now();
    let result = check_response(config, client).await;
    let (status, usage) = match result {
        Ok(usage) => (ConnectionStatus::Verified, Some(usage)),
        Err(status) => (status, None),
    };
    ConnectionCheck {
        status,
        elapsed_ms: started.elapsed().as_millis() as u64,
        usage,
    }
}

fn network_status(error: reqwest::Error) -> ConnectionStatus {
    if error.is_timeout() {
        ConnectionStatus::TimedOut
    } else {
        ConnectionStatus::NetworkError
    }
}

async fn check_response(
    config: &AiConfig,
    client: &reqwest::Client,
) -> Result<TokenUsage, ConnectionStatus> {
    if config.api_key.trim().is_empty() {
        return Err(ConnectionStatus::MissingKey);
    }
    if config.model.trim().is_empty() {
        return Err(ConnectionStatus::InvalidConfig);
    }
    let endpoint = messages_url(&config.base_url).map_err(|_| ConnectionStatus::InvalidConfig)?;
    let mut response = client
        .post(endpoint)
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&request_body(&config.model))
        .send()
        .await
        .map_err(network_status)?;
    match response.status().as_u16() {
        200 => {}
        401 => return Err(ConnectionStatus::Unauthorized),
        403 => return Err(ConnectionStatus::Forbidden),
        400 | 404 => return Err(ConnectionStatus::ModelUnavailable),
        429 => return Err(ConnectionStatus::RateLimited),
        _ => return Err(ConnectionStatus::ServiceError),
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_status)? {
        if body.len() + chunk.len() > 65_536 {
            return Err(ConnectionStatus::InvalidResponse);
        }
        body.extend_from_slice(&chunk);
    }
    let data: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| ConnectionStatus::InvalidResponse)?;
    if data["type"] != "message"
        || data["role"] != "assistant"
        || !data["content"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["type"] == "text" && item["text"].as_str().is_some_and(|s| !s.is_empty())
            })
        })
    {
        return Err(ConnectionStatus::InvalidResponse);
    }
    let input = data["usage"]["input_tokens"].as_u64().unwrap_or(0);
    let output = data["usage"]["output_tokens"].as_u64().unwrap_or(0);
    Ok(TokenUsage {
        input_tokens: input,
        output_tokens: output,
        total_tokens: input.saturating_add(output),
        cache_read_tokens: data["usage"]["cache_read_input_tokens"]
            .as_u64()
            .unwrap_or(0),
        cache_write_tokens: data["usage"]["cache_creation_input_tokens"]
            .as_u64()
            .unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[rstest]
    #[case(None, false, Some("saved"))]
    #[case(Some(" new "), false, Some("new"))]
    #[case(None, true, None)]
    fn credential_changes_are_explicit(
        #[case] key: Option<&str>,
        #[case] clear: bool,
        #[case] expected: Option<&str>,
    ) {
        let current = AiOverride {
            api_key: Some("saved".into()),
            ..Default::default()
        };
        let draft = ConnectionDraft {
            api_key: key.map(str::to_string),
            clear_saved_key: clear,
            ..Default::default()
        };
        assert_eq!(
            draft.apply_to(&current).unwrap().api_key.as_deref(),
            expected
        );
        assert_eq!(current.api_key.as_deref(), Some("saved"));
    }

    #[rstest]
    #[case("https://example.com", "https://example.com/v1/messages")]
    #[case("https://example.com/v1/", "https://example.com/v1/messages")]
    #[case(
        "https://example.com/gateway/v1/messages",
        "https://example.com/gateway/v1/messages"
    )]
    fn endpoint_normalization(#[case] base: &str, #[case] expected: &str) {
        assert_eq!(messages_url(base).unwrap(), expected);
    }

    #[rstest]
    #[case(Some(" "), false, None, None)]
    #[case(Some("new"), true, None, None)]
    #[case(None, false, Some("https://example.com?key=secret"), None)]
    #[case(None, false, Some("ftp://example.com"), None)]
    #[case(None, false, None, Some(0))]
    fn rejects_invalid_drafts(
        #[case] key: Option<&str>,
        #[case] clear: bool,
        #[case] base: Option<&str>,
        #[case] max_tokens: Option<u64>,
    ) {
        let draft = ConnectionDraft {
            api_key: key.map(str::to_string),
            clear_saved_key: clear,
            base_url: base.map(str::to_string),
            max_tokens,
            ..Default::default()
        };
        assert!(draft.apply_to(&AiOverride::default()).is_err());
    }

    #[test]
    fn request_and_result_contract() {
        insta::assert_yaml_snapshot!(request_body("test-model"));
        insta::assert_yaml_snapshot!(ConnectionCheck::local(ConnectionStatus::MissingKey));
    }

    #[rstest]
    #[case(200, ConnectionStatus::InvalidResponse)]
    #[case(401, ConnectionStatus::Unauthorized)]
    #[case(403, ConnectionStatus::Forbidden)]
    #[case(404, ConnectionStatus::ModelUnavailable)]
    #[case(429, ConnectionStatus::RateLimited)]
    #[case(500, ConnectionStatus::ServiceError)]
    #[tokio::test]
    async fn classifies_failures_without_provider_body(
        #[case] status: u16,
        #[case] expected: ConnectionStatus,
    ) {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).set_body_string("secret provider error"))
            .mount(&server)
            .await;
        let cfg = AiConfig {
            api_key: "key".into(),
            base_url: server.uri(),
            model: "model".into(),
        };
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        assert_eq!(check_with_client(&cfg, &client).await.status, expected);
    }

    #[tokio::test]
    async fn checks_an_actual_message_and_collects_usage() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "type": "message", "role": "assistant", "content": [{"type":"text", "text":"OK"}],
                "usage": {"input_tokens": 4, "output_tokens": 1}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let cfg = AiConfig {
            api_key: "key".into(),
            base_url: server.uri(),
            model: "model".into(),
        };
        let result = check_with_client(&cfg, &client).await;
        assert_eq!(result.status, ConnectionStatus::Verified);
        assert_eq!(result.usage.unwrap().total_tokens, 5);
        let requests = server.received_requests().await.unwrap();
        insta::assert_yaml_snapshot!(
            serde_json::from_slice::<serde_json::Value>(&requests[0].body).unwrap()
        );
    }

    #[tokio::test]
    async fn redirects_do_not_forward_credentials() {
        let redirect = MockServer::start().await;
        let target = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&target)
            .await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(307)
                    .insert_header("Location", format!("{}/v1/messages", target.uri())),
            )
            .mount(&redirect)
            .await;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let cfg = AiConfig {
            api_key: "key".into(),
            base_url: redirect.uri(),
            model: "model".into(),
        };
        assert_eq!(
            check_with_client(&cfg, &client).await.status,
            ConnectionStatus::ServiceError
        );
    }

    #[tokio::test]
    async fn request_deadline_is_reported() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(200)))
            .mount(&server)
            .await;
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(30))
            .build()
            .unwrap();
        let cfg = AiConfig {
            api_key: "key".into(),
            base_url: server.uri(),
            model: "model".into(),
        };
        assert_eq!(
            check_with_client(&cfg, &client).await.status,
            ConnectionStatus::TimedOut
        );
    }
}
