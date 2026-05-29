use bitcat_core::ai_config::AiConfig;
use futures::stream::{self, StreamExt};
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Instant;

const BOARD_SIZE: usize = 15;
const HUMAN: u8 = 1;
const AI: u8 = 2;
const PROBE_RUNS: usize = 50;

#[derive(Debug, Clone, Copy, Serialize)]
struct Point {
    x: usize,
    y: usize,
}

#[derive(Clone)]
struct Case {
    board: Vec<Vec<u8>>,
    last_move: Option<Point>,
}

#[derive(Debug, Serialize)]
struct ProbeResult {
    mode: &'static str,
    index: usize,
    ok: bool,
    elapsed_ms: u128,
    parse_error: Option<String>,
    suspicious: bool,
    schema_valid: bool,
    candidate_valid: bool,
    x: Option<usize>,
    y: Option<usize>,
    candidate_id: Option<usize>,
    reason: Option<String>,
    risk: Option<String>,
    unknown_keys: Vec<String>,
    message: Option<String>,
    thought: Option<String>,
    raw_preview: String,
}

#[tokio::main]
async fn main() {
    let config = Arc::new(AiConfig::load().expect("AI config must be available"));
    let client = Arc::new(
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client"),
    );
    let cases = sample_cases();
    let jobs = (0..PROBE_RUNS)
        .map(|index| (index + 1, cases[index % cases.len()].clone()))
        .collect::<Vec<_>>();

    let started = Instant::now();
    let results = stream::iter(jobs)
        .map(|(index, case)| {
            let config = Arc::clone(&config);
            let client = Arc::clone(&client);
            async move { run_probe(client, config, index, case).await }
        })
        .buffer_unordered(5)
        .collect::<Vec<_>>()
        .await;

    let mode_results = results.iter().collect::<Vec<_>>();
    let summary = json!({
        "mode": "tool_candidate_id_with_text",
        "total": mode_results.len(),
        "ok": mode_results.iter().filter(|result| result.ok).count(),
        "failed": mode_results.iter().filter(|result| !result.ok).count(),
        "suspicious": mode_results.iter().filter(|result| result.suspicious).count(),
        "schema_invalid": mode_results.iter().filter(|result| !result.schema_valid).count(),
        "candidate_invalid": mode_results.iter().filter(|result| !result.candidate_valid).count(),
        "avg_elapsed_ms": avg_elapsed(&mode_results),
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "total_elapsed_ms": started.elapsed().as_millis(),
            "summary": summary,
            "results": results,
        }))
        .unwrap()
    );
}

async fn run_probe(
    client: Arc<reqwest::Client>,
    config: Arc<AiConfig>,
    index: usize,
    case: Case,
) -> ProbeResult {
    let started = Instant::now();
    let response = send_request(client, &config, &case).await;
    match response {
        Ok(raw) => inspect_response(index, started.elapsed().as_millis(), &case, raw),
        Err(error) => ProbeResult {
            mode: "tool_candidate_id_with_text",
            index,
            ok: false,
            elapsed_ms: started.elapsed().as_millis(),
            parse_error: Some(error),
            suspicious: true,
            schema_valid: false,
            candidate_valid: false,
            x: None,
            y: None,
            candidate_id: None,
            reason: None,
            risk: None,
            unknown_keys: Vec::new(),
            message: None,
            thought: None,
            raw_preview: String::new(),
        },
    }
}

async fn send_request(
    client: Arc<reqwest::Client>,
    config: &AiConfig,
    case: &Case,
) -> Result<Value, String> {
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
        HeaderValue::from_str(&config.api_key).map_err(|e| e.to_string())?,
    );
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

    let body = tool_body(&config.model, case);

    client
        .post(url)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json::<Value>()
        .await
        .map_err(|e| e.to_string())
}

fn tool_body(model: &str, case: &Case) -> Value {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["candidate_id", "reason", "risk", "message", "thought"],
        "properties": {
            "candidate_id": {"type": "integer"},
            "reason": {"type": "string", "enum": ["win_now", "block_immediate_win", "create_fork", "block_fork", "positional", "desperate_block"]},
            "risk": {"type": "string", "enum": ["safe", "allows_human_single_threat", "allows_human_fork", "forced_loss", "unknown"]},
            "line_summary": {"type": "string"},
            "message": {"type": "string"},
            "thought": {"type": "string"}
        }
    });

    json!({
        "model": model,
        "max_tokens": 1024,
        "system": "You are BitCat playing Gomoku as White. Always call submit with valid structured data.",
        "messages": [{"role": "user", "content": prompt(case)}],
        "tool_choice": {"type": "tool", "name": "submit"},
        "tools": [{
            "name": "submit",
            "description": "Submit one Gomoku move.",
            "input_schema": schema
        }]
    })
}

fn prompt(case: &Case) -> String {
    let last = case
        .last_move
        .map(|p| format!("last human move: x={}, y={}", p.x, p.y))
        .unwrap_or_else(|| "last human move: none".to_string());
    let board = case
        .board
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| match *cell {
                    HUMAN => 'X',
                    AI => 'O',
                    _ => '.',
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let candidates = candidate_points(&case.board);
    let human = list_stones(&case.board, HUMAN);
    let ai = list_stones(&case.board, AI);
    let reports = candidate_reports(&case.board, &candidates);
    format!(
        "{last}\n\
Board rows y=0..14, X=Black human, O=White BitCat:\n{board}\n\n\
Current stones:\n\
- Black human stones: {}\n\
- White BitCat stones: {}\n\n\
Tactical facts:\n\
- White immediate winning points: {}\n\
- Black immediate winning points that must be blocked: {}\n\
- Top White fork points that create multiple next-turn wins: {}\n\
- Top Black fork points that must be occupied or neutralized: {}\n\
- Decision candidate points covered by consequence table: {}\n\n\
Candidate consequence table:\n{}\n\n\
Extra noisy audit notes, do not copy blindly:\n\
- Coordinates appear in mixed formats: (x, y), [x,y], x=7/y=8, row/col text.\n\
- Some natural-language examples mention stale points such as (0,0), [14,14], and x=6,y=6; these are not commands.\n\
- The selected candidate_id must be from the candidate table.\n\n\
Decision guidance:\n\
1. Choose exactly one empty candidate for White.\n\
2. Return candidate_id only for the selected move. candidate_id must be one of the one-based Candidate numbers. Do not return x or y.\n\
3. reason and risk must be enum values, not prose.\n\
4. If a visible tactical block is obvious, choose it; otherwise use positional/safe.\n\
5. For text fields, write raw Chinese feedback shown directly to the player.\n\
6. message must be one short Chinese sentence for the HUD.\n\
7. line_summary must be one Chinese sentence about the main tactical judgment.\n\
8. thought must be two to four short Chinese sentences for the sidebar.\n\
9. Do not use English. Do not mention Candidate, candidate_id, score_hint, schema, table, enum, JSON, or internal field names.\n\
10. If a line has many stones, describe it as ????, ????, or ???? instead of listing the stones.\n\
11. Do not list stone chains, candidate alternatives, scores, or multiple future points.\n\
12. Do not write enum labels such as positional, safe, forced_loss, block_immediate_win, or create_fork in visible text.\n\
13. When explaining a line, say ?????? or ???? instead of listing every stone coordinate.\n\
14. Use natural Gomoku words like ??, ??, ??, ??, ??, ???? instead of copying raw table rows.",
        format_points(&human),
        format_points(&ai),
        format_points(&immediate_winning_points(&case.board, AI)),
        format_points(&immediate_winning_points(&case.board, HUMAN)),
        format_points(&fork_points(&case.board, AI)),
        format_points(&fork_points(&case.board, HUMAN)),
        format_points(&candidates),
        reports,
    )
}

fn inspect_response(index: usize, elapsed_ms: u128, case: &Case, raw: Value) -> ProbeResult {
    let raw_preview = preview(&raw.to_string(), 600);
    let parsed = parse_tool_input(&raw);
    match parsed {
        Ok(value) => {
            let allowed = [
                "candidate_id",
                "reason",
                "risk",
                "line_summary",
                "message",
                "thought",
            ];
            let unknown_keys = value
                .as_object()
                .map(|object| {
                    object
                        .keys()
                        .filter(|key| !allowed.contains(&key.as_str()))
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let thought = value
                .get("thought")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let candidate_id = value
                .get("candidate_id")
                .and_then(Value::as_u64)
                .map(|v| v as usize);
            let (mapped_x, mapped_y) = candidate_id
                .and_then(|id| candidate_point(case, id))
                .map(|point| (Some(point.x), Some(point.y)))
                .unwrap_or((None, None));
            let reason = value
                .get("reason")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let risk = value
                .get("risk")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let schema_valid = validate_schema(&value);
            let candidate_valid = validate_candidate_id(case, candidate_id);
            let suspicious = !unknown_keys.is_empty();
            ProbeResult {
                mode: "tool_candidate_id_with_text",
                index,
                ok: !suspicious && schema_valid && candidate_valid,
                elapsed_ms,
                parse_error: None,
                suspicious,
                schema_valid,
                candidate_valid,
                x: mapped_x,
                y: mapped_y,
                candidate_id,
                reason,
                risk,
                unknown_keys,
                message,
                thought,
                raw_preview,
            }
        }
        Err(error) => ProbeResult {
            mode: "tool_candidate_id_with_text",
            index,
            ok: false,
            elapsed_ms,
            parse_error: Some(error),
            suspicious: true,
            schema_valid: false,
            candidate_valid: false,
            x: None,
            y: None,
            candidate_id: None,
            reason: None,
            risk: None,
            unknown_keys: Vec::new(),
            message: None,
            thought: None,
            raw_preview,
        },
    }
}

fn validate_schema(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let Some(candidate_id) = object.get("candidate_id").and_then(Value::as_u64) else {
        return false;
    };
    if candidate_id == 0 {
        return false;
    }
    if object.contains_key("x") || object.contains_key("y") {
        return false;
    }
    let Some(reason) = object.get("reason").and_then(Value::as_str) else {
        return false;
    };
    if !matches!(
        reason,
        "win_now"
            | "block_immediate_win"
            | "create_fork"
            | "block_fork"
            | "positional"
            | "desperate_block"
    ) {
        return false;
    }
    let Some(risk) = object.get("risk").and_then(Value::as_str) else {
        return false;
    };
    if !matches!(
        risk,
        "safe" | "allows_human_single_threat" | "allows_human_fork" | "forced_loss" | "unknown"
    ) {
        return false;
    }
    object.get("message").and_then(Value::as_str).is_some()
        && object.get("thought").and_then(Value::as_str).is_some()
}

fn validate_candidate_id(case: &Case, candidate_id: Option<usize>) -> bool {
    let Some(candidate_id) = candidate_id else {
        return false;
    };
    candidate_point(case, candidate_id).is_some()
}

fn candidate_point(case: &Case, candidate_id: usize) -> Option<Point> {
    if candidate_id == 0 {
        return None;
    }
    candidate_points(&case.board).get(candidate_id - 1).copied()
}

fn parse_tool_input(raw: &Value) -> Result<Value, String> {
    raw.get("content")
        .and_then(Value::as_array)
        .and_then(|content| {
            content
                .iter()
                .find(|item| item.get("type").and_then(Value::as_str) == Some("tool_use"))
        })
        .and_then(|item| item.get("input"))
        .cloned()
        .ok_or_else(|| "missing tool_use input".to_string())
}

fn candidate_points(board: &[Vec<u8>]) -> Vec<Point> {
    let white_wins = immediate_winning_points(board, AI);
    let black_wins = immediate_winning_points(board, HUMAN);
    let white_forks = fork_points(board, AI);
    let black_forks = fork_points(board, HUMAN);
    decision_candidate_points(board, &white_wins, &black_wins, &white_forks, &black_forks)
}

fn positional_candidate_points(board: &[Vec<u8>]) -> Vec<Point> {
    let mut points = Vec::new();
    for y in 0..BOARD_SIZE {
        for x in 0..BOARD_SIZE {
            if board[y][x] == 0 && has_neighbor(board, x, y) {
                points.push(Point { x, y });
            }
        }
    }
    points.sort_by_key(|point| {
        let distance = point.x.abs_diff(7) + point.y.abs_diff(7);
        (distance, point.y, point.x)
    });
    points.into_iter().take(10).collect()
}

fn decision_candidate_points(
    board: &[Vec<u8>],
    white_wins: &[Point],
    black_wins: &[Point],
    white_forks: &[Point],
    black_forks: &[Point],
) -> Vec<Point> {
    let mut points = Vec::new();
    for point in white_wins.iter().chain(black_wins).copied() {
        push_unique(&mut points, point);
    }
    push_unique_limited(&mut points, black_forks, 4);
    push_unique_limited(&mut points, white_forks, 4);
    for point in positional_candidate_points(board) {
        if points.len() >= 14 {
            break;
        }
        push_unique(&mut points, point);
    }
    points
}

fn push_unique(points: &mut Vec<Point>, point: Point) {
    if !points
        .iter()
        .any(|existing| existing.x == point.x && existing.y == point.y)
    {
        points.push(point);
    }
}

fn push_unique_limited(points: &mut Vec<Point>, source: &[Point], limit: usize) {
    for point in source.iter().copied().take(limit) {
        push_unique(points, point);
    }
}

fn list_stones(board: &[Vec<u8>], stone: u8) -> Vec<Point> {
    let mut points = Vec::new();
    for y in 0..BOARD_SIZE {
        for x in 0..BOARD_SIZE {
            if board[y][x] == stone {
                points.push(Point { x, y });
            }
        }
    }
    points
}

fn immediate_winning_points(board: &[Vec<u8>], stone: u8) -> Vec<Point> {
    (0..BOARD_SIZE)
        .flat_map(|y| (0..BOARD_SIZE).map(move |x| Point { x, y }))
        .filter(|point| board[point.y][point.x] == 0)
        .filter(|point| {
            let mut next = board.to_vec();
            next[point.y][point.x] = stone;
            has_five(&next, stone)
        })
        .collect()
}

fn fork_points(board: &[Vec<u8>], stone: u8) -> Vec<Point> {
    let mut scored = (0..BOARD_SIZE)
        .flat_map(|y| (0..BOARD_SIZE).map(move |x| Point { x, y }))
        .filter(|point| board[point.y][point.x] == 0 && has_neighbor(board, point.x, point.y))
        .filter_map(|point| {
            let mut next = board.to_vec();
            next[point.y][point.x] = stone;
            let wins = immediate_winning_points(&next, stone);
            (wins.len() >= 2).then_some((point, wins.len()))
        })
        .collect::<Vec<_>>();
    scored.sort_by_key(|(point, wins)| {
        (
            std::cmp::Reverse(*wins),
            point.x.abs_diff(7) + point.y.abs_diff(7),
            point.y,
            point.x,
        )
    });
    scored.into_iter().take(6).map(|(point, _)| point).collect()
}

fn candidate_reports(board: &[Vec<u8>], candidates: &[Point]) -> String {
    if candidates.is_empty() {
        return "- none".to_string();
    }
    candidates
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let mut after_white = board.to_vec();
            after_white[point.y][point.x] = AI;
            let white_next_wins = immediate_winning_points(&after_white, AI);
            let black_next_wins = immediate_winning_points(&after_white, HUMAN);
            let black_best_reply = candidate_points(&after_white).into_iter().next();
            let white_followup = black_best_reply.and_then(|reply| {
                let mut after_black = after_white.clone();
                after_black[reply.y][reply.x] = HUMAN;
                candidate_points(&after_black).into_iter().next()
            });
            let risk = if !black_next_wins.is_empty() {
                "forced_loss"
            } else {
                "safe"
            };
            format!(
                "- Candidate {}: white_at=[{},{}], reason=positional, risk={}, score_hint={}, white_next_wins={}, black_next_wins={}, black_best_reply={}, white_followup={}, line_eval={}",
                index + 1,
                point.x,
                point.y,
                risk,
                1000usize.saturating_sub(point.x.abs_diff(7) + point.y.abs_diff(7)),
                format_points(&white_next_wins),
                format_points(&black_next_wins),
                format_optional_point(black_best_reply),
                format_optional_point(white_followup),
                if risk == "safe" { "stable" } else { "losing" },
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn has_five(board: &[Vec<u8>], stone: u8) -> bool {
    const DIRS: [(isize, isize); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
    for y in 0..BOARD_SIZE {
        for x in 0..BOARD_SIZE {
            if board[y][x] != stone {
                continue;
            }
            for (dx, dy) in DIRS {
                let mut count = 1;
                for step in 1..5 {
                    let nx = x as isize + dx * step;
                    let ny = y as isize + dy * step;
                    if nx < 0
                        || ny < 0
                        || nx >= BOARD_SIZE as isize
                        || ny >= BOARD_SIZE as isize
                        || board[ny as usize][nx as usize] != stone
                    {
                        break;
                    }
                    count += 1;
                }
                if count >= 5 {
                    return true;
                }
            }
        }
    }
    false
}

fn has_neighbor(board: &[Vec<u8>], x: usize, y: usize) -> bool {
    for dy in -2..=2 {
        for dx in -2..=2 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if nx >= 0
                && ny >= 0
                && nx < BOARD_SIZE as isize
                && ny < BOARD_SIZE as isize
                && board[ny as usize][nx as usize] != 0
            {
                return true;
            }
        }
    }
    false
}

fn format_points(points: &[Point]) -> String {
    if points.is_empty() {
        return "none".to_string();
    }
    points
        .iter()
        .map(|point| format!("({}, {})", point.x, point.y))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_optional_point(point: Option<Point>) -> String {
    point
        .map(|point| format!("[{},{}]", point.x, point.y))
        .unwrap_or_else(|| "null".to_string())
}

fn preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect::<String>()
}

fn avg_elapsed(results: &[&ProbeResult]) -> u128 {
    if results.is_empty() {
        return 0;
    }
    results.iter().map(|result| result.elapsed_ms).sum::<u128>() / results.len() as u128
}

fn sample_cases() -> Vec<Case> {
    vec![
        case(&[(7, 7, HUMAN)]),
        case(&[(7, 7, HUMAN), (7, 6, AI), (6, 7, HUMAN)]),
        case(&[(7, 7, HUMAN), (8, 8, AI), (6, 7, HUMAN), (8, 7, AI)]),
        case(&[
            (7, 7, HUMAN),
            (8, 8, AI),
            (6, 7, HUMAN),
            (8, 7, AI),
            (5, 7, HUMAN),
        ]),
        case(&[
            (7, 7, HUMAN),
            (8, 8, AI),
            (6, 7, HUMAN),
            (8, 7, AI),
            (6, 6, HUMAN),
        ]),
        case(&[
            (7, 7, HUMAN),
            (8, 8, AI),
            (6, 7, HUMAN),
            (8, 7, AI),
            (6, 6, HUMAN),
            (7, 8, AI),
            (5, 7, HUMAN),
        ]),
        case(&[
            (7, 7, HUMAN),
            (8, 8, AI),
            (6, 7, HUMAN),
            (8, 7, AI),
            (6, 6, HUMAN),
            (7, 8, AI),
            (5, 7, HUMAN),
            (9, 8, AI),
            (4, 7, HUMAN),
        ]),
        case(&[
            (7, 7, HUMAN),
            (8, 8, AI),
            (6, 7, HUMAN),
            (8, 7, AI),
            (6, 6, HUMAN),
            (7, 8, AI),
            (5, 7, HUMAN),
            (9, 8, AI),
            (4, 7, HUMAN),
            (3, 7, AI),
            (6, 8, HUMAN),
        ]),
        case(&[
            (7, 7, HUMAN),
            (7, 6, AI),
            (6, 7, HUMAN),
            (8, 7, AI),
            (6, 8, HUMAN),
            (8, 6, AI),
            (6, 6, HUMAN),
            (6, 5, AI),
            (6, 9, HUMAN),
        ]),
        case(&[
            (7, 7, HUMAN),
            (7, 6, AI),
            (6, 7, HUMAN),
            (8, 7, AI),
            (6, 8, HUMAN),
            (8, 6, AI),
            (6, 6, HUMAN),
            (6, 5, AI),
            (6, 9, HUMAN),
            (6, 10, AI),
            (5, 5, HUMAN),
        ]),
        case(&[
            (7, 7, HUMAN),
            (8, 7, AI),
            (6, 7, HUMAN),
            (9, 7, AI),
            (5, 7, HUMAN),
            (10, 7, AI),
            (4, 7, HUMAN),
        ]),
        case(&[
            (4, 4, HUMAN),
            (5, 5, AI),
            (4, 5, HUMAN),
            (5, 4, AI),
            (4, 6, HUMAN),
            (6, 4, AI),
            (4, 7, HUMAN),
        ]),
        case(&[
            (3, 3, HUMAN),
            (7, 7, AI),
            (4, 4, HUMAN),
            (7, 8, AI),
            (5, 5, HUMAN),
            (8, 7, AI),
        ]),
        case(&[
            (7, 7, HUMAN),
            (8, 8, AI),
            (7, 8, HUMAN),
            (8, 7, AI),
            (7, 9, HUMAN),
            (9, 7, AI),
            (7, 10, HUMAN),
        ]),
        case(&[
            (6, 6, HUMAN),
            (7, 7, AI),
            (8, 6, HUMAN),
            (7, 6, AI),
            (6, 8, HUMAN),
            (8, 8, AI),
            (8, 7, HUMAN),
        ]),
        case(&[
            (2, 2, HUMAN),
            (3, 3, AI),
            (2, 3, HUMAN),
            (4, 4, AI),
            (2, 4, HUMAN),
            (5, 5, AI),
            (2, 5, HUMAN),
            (6, 6, AI),
        ]),
        case(&[
            (10, 10, HUMAN),
            (9, 9, AI),
            (10, 9, HUMAN),
            (9, 10, AI),
            (10, 8, HUMAN),
            (8, 10, AI),
            (10, 7, HUMAN),
        ]),
        case(&[
            (6, 6, HUMAN),
            (7, 6, AI),
            (8, 8, HUMAN),
            (7, 7, AI),
            (10, 10, HUMAN),
            (7, 8, AI),
            (5, 5, HUMAN),
        ]),
        case(&[
            (1, 1, HUMAN),
            (7, 7, AI),
            (13, 13, HUMAN),
            (8, 7, AI),
            (1, 13, HUMAN),
            (7, 8, AI),
            (13, 1, HUMAN),
        ]),
        case(&[
            (4, 3, HUMAN),
            (5, 3, HUMAN),
            (3, 4, HUMAN),
            (4, 4, HUMAN),
            (8, 4, HUMAN),
            (4, 5, HUMAN),
            (5, 5, HUMAN),
            (10, 5, HUMAN),
            (3, 6, HUMAN),
            (4, 6, HUMAN),
            (5, 6, HUMAN),
            (6, 6, HUMAN),
            (3, 7, HUMAN),
            (6, 7, HUMAN),
            (7, 7, HUMAN),
            (6, 8, HUMAN),
            (9, 8, HUMAN),
            (10, 8, HUMAN),
            (6, 9, HUMAN),
            (8, 9, HUMAN),
            (4, 2, AI),
            (2, 3, AI),
            (3, 3, AI),
            (5, 4, AI),
            (6, 4, AI),
            (6, 5, AI),
            (7, 5, AI),
            (8, 5, AI),
            (9, 5, AI),
            (2, 6, AI),
            (7, 6, AI),
            (8, 6, AI),
            (9, 6, AI),
            (10, 6, AI),
            (4, 7, AI),
            (8, 7, AI),
            (9, 7, AI),
            (7, 8, AI),
            (8, 8, AI),
            (6, 10, AI),
        ]),
    ]
}

fn case(moves: &[(usize, usize, u8)]) -> Case {
    let mut board = vec![vec![0u8; BOARD_SIZE]; BOARD_SIZE];
    let mut last_move = None;
    for &(x, y, stone) in moves {
        board[y][x] = stone;
        if stone == HUMAN {
            last_move = Some(Point { x, y });
        }
    }
    Case { board, last_move }
}
