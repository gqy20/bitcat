use bitcat_core::ai_config::AiConfig;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::json;

#[tokio::main]
async fn main() {
    let config = AiConfig::load().expect("AI config must be available");
    let base = config
        .base_url
        .trim_end_matches("/v1")
        .trim_end_matches('/');
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client");

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(&config.api_key).expect("valid api key header"),
    );
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

    let anthropic_body = json!({
        "model": config.model,
        "max_tokens": 256,
        "system": "Return only data conforming to the requested schema.",
        "messages": [{
            "role": "user",
            "content": "Choose a Gomoku move for White after Black played [7,7]."
        }],
        "output_config": {
            "format": {
                "type": "json_schema",
                "schema": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["x", "y", "reason", "message"],
                    "properties": {
                        "x": {"type": "integer"},
                        "y": {"type": "integer"},
                        "reason": {"type": "string", "enum": ["positional", "block", "win"]},
                        "message": {"type": "string"}
                    }
                }
            }
        }
    });

    let anthropic = client
        .post(format!("{base}/v1/messages"))
        .headers(headers.clone())
        .json(&anthropic_body)
        .send()
        .await;

    println!("=== anthropic output_config probe ===");
    match anthropic {
        Ok(response) => {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            println!("status: {status}");
            println!("{text}");
        }
        Err(error) => println!("request error: {error}"),
    }

    let openai_body = json!({
        "model": config.model,
        "messages": [{
            "role": "user",
            "content": "Choose a Gomoku move for White after Black played [7,7]."
        }],
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "gomoku_move",
                "strict": true,
                "schema": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["x", "y", "reason", "message"],
                    "properties": {
                        "x": {"type": "integer"},
                        "y": {"type": "integer"},
                        "reason": {"type": "string", "enum": ["positional", "block", "win"]},
                        "message": {"type": "string"}
                    }
                }
            }
        }
    });

    println!("=== openai response_format probe ===");
    match client
        .post(format!("{base}/v1/chat/completions"))
        .bearer_auth(&config.api_key)
        .json(&openai_body)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            println!("status: {status}");
            println!("{text}");
        }
        Err(error) => println!("request error: {error}"),
    }
}
