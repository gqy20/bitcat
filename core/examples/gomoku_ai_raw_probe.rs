use bitcat_core::ai_config::AiConfig;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::json;

#[tokio::main]
async fn main() {
    let config = AiConfig::load().expect("AI config must be available");
    let url = format!(
        "{}/v1/messages",
        config
            .base_url
            .trim_end_matches("/v1")
            .trim_end_matches('/')
    );
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(&config.api_key).expect("valid api key header"),
    );
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

    let system = "\
You are an AI assistant whose purpose is to extract structured data from the provided text.
You will have access to a `submit` function that defines the structure of the data to extract from the provided text.
Use the `submit` function to submit the structured data.
Be sure to fill out every field and ALWAYS CALL THE `submit` function, even with default values!!!.

=============== ADDITIONAL INSTRUCTIONS ===============
You are BitCat playing Gomoku as White against the human Black on a 15x15 board.
Coordinates are zero-based internally.
Choose exactly one empty cell.
Use 黑棋 for the human and 白棋 for BitCat in message/thought.
The optional thought should be a visible Chinese decision summary for the sidebar, at most 180 Chinese characters.";

    let prompt = "\
last human move: x=7, y=7
Board rows y=0..14, using . empty, 黑 black/human, 白 white/BitCat:
...............
...............
...............
...............
...............
...............
...............
.......黑.......
...............
...............
...............
...............
...............
...............
...............

Current stones:
- Black human stones: (7, 7)
- White BitCat stones: none

Tactical facts:
- White immediate winning points: none
- Black immediate winning points that must be blocked: none
- Top White fork points that create multiple next-turn wins: none
- Top Black fork points that must be occupied or neutralized: none
- Decision candidate points covered by consequence table: (7, 6), (8, 7), (6, 7), (7, 8)
Candidate consequence table:
- White at [7,6]: reason=positional, risk=safe, score_hint=494, white_next_wins=[], black_next_wins=[], black_fork_replies=[], black_best_reply=[6,6], white_followup=[8,8], line_eval=stable
- White at [8,7]: reason=positional, risk=safe, score_hint=494, white_next_wins=[], black_next_wins=[], black_fork_replies=[], black_best_reply=[6,6], white_followup=[8,8], line_eval=stable

Decision guidance:
Choose one empty point for White and set reason, risk, blocked_immediate_wins, blocked_forks, and line_summary consistently with the table.";

    let body = json!({
        "model": config.model,
        "max_tokens": 1024,
        "system": system,
        "messages": [{"role": "user", "content": prompt}],
        "tool_choice": {"type": "tool", "name": "submit"},
        "tools": [{
            "name": "submit",
            "description": "Submit the structured data you extracted from the provided text.",
            "input_schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["x", "y", "reason", "risk", "blocked_immediate_wins", "blocked_forks", "line_summary", "message", "thought"],
                "properties": {
                    "x": {"type": "integer"},
                    "y": {"type": "integer"},
                    "reason": {"type": "string", "enum": ["win_now", "block_immediate_win", "create_fork", "block_fork", "positional", "desperate_block"]},
                    "risk": {"type": "string", "enum": ["safe", "allows_human_single_threat", "allows_human_fork", "forced_loss", "unknown"]},
                    "blocked_immediate_wins": {"type": "array", "items": {"type": "array", "items": {"type": "integer"}}},
                    "blocked_forks": {"type": "array", "items": {"type": "array", "items": {"type": "integer"}}},
                    "line_summary": {"type": "string"},
                    "message": {"type": ["string", "null"]},
                    "thought": {"type": ["string", "null"]}
                }
            }
        }]
    });

    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client");
    let response = client
        .post(url)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .expect("request")
        .error_for_status()
        .expect("successful status")
        .json::<serde_json::Value>()
        .await
        .expect("json response");

    println!("{}", serde_json::to_string_pretty(&response).unwrap());
}
