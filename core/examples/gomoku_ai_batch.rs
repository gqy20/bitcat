use bitcat_core::ai_config::AiConfig;
use bitcat_core::gomoku_ai::{GomokuPoint, choose_ai_move};
use futures::stream::{self, StreamExt};
use serde::Serialize;
use std::sync::Arc;
use std::time::Instant;

const BOARD_SIZE: usize = 15;
const HUMAN: u8 = 1;
const AI: u8 = 2;

#[derive(Debug, Serialize)]
struct CaseResult {
    index: usize,
    ok: bool,
    elapsed_ms: u128,
    x: Option<usize>,
    y: Option<usize>,
    reason: Option<String>,
    risk: Option<String>,
    line_eval: Option<String>,
    thought: Option<String>,
    message: Option<String>,
    suspicious_text: bool,
    error: Option<String>,
}

#[derive(Clone)]
struct Case {
    board: Vec<Vec<u8>>,
    last_move: Option<GomokuPoint>,
}

#[tokio::main]
async fn main() {
    let config = Arc::new(AiConfig::load().expect("AI config must be available"));
    let cases = sample_cases();
    let started = Instant::now();
    let results = stream::iter(cases.into_iter().enumerate())
        .map(|(index, case)| {
            let config = Arc::clone(&config);
            async move { run_case(index + 1, config, case).await }
        })
        .buffer_unordered(5)
        .collect::<Vec<_>>()
        .await;
    let ok = results.iter().filter(|result| result.ok).count();
    let suspicious = results
        .iter()
        .filter(|result| result.suspicious_text)
        .count();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "total": results.len(),
            "ok": ok,
            "failed": results.len() - ok,
            "suspicious_text": suspicious,
            "elapsed_ms": started.elapsed().as_millis(),
            "results": results,
        }))
        .unwrap()
    );
}

async fn run_case(index: usize, config: Arc<AiConfig>, case: Case) -> CaseResult {
    let started = Instant::now();
    match choose_ai_move(&config, &case.board, case.last_move).await {
        Ok(mv) => {
            let thought = mv.thought.clone();
            let message = mv.message.clone();
            CaseResult {
                index,
                ok: true,
                elapsed_ms: started.elapsed().as_millis(),
                x: Some(mv.x),
                y: Some(mv.y),
                reason: Some(format!("{:?}", mv.reason)),
                risk: Some(format!("{:?}", mv.risk)),
                line_eval: Some(format!("{:?}", mv.line_eval)),
                suspicious_text: thought.as_deref().is_some_and(has_suspicious_trailing_text)
                    || message.as_deref().is_some_and(has_suspicious_trailing_text),
                thought,
                message,
                error: None,
            }
        }
        Err(error) => CaseResult {
            index,
            ok: false,
            elapsed_ms: started.elapsed().as_millis(),
            x: None,
            y: None,
            reason: None,
            risk: None,
            line_eval: None,
            thought: None,
            message: None,
            suspicious_text: false,
            error: Some(error),
        },
    }
}

fn has_suspicious_trailing_text(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.ends_with('(')
        || trimmed.ends_with('[')
        || trimmed.ends_with(',')
        || trimmed.chars().last().is_some_and(|ch| ch.is_ascii_digit())
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
    ]
}

fn case(moves: &[(usize, usize, u8)]) -> Case {
    let mut board = vec![vec![0u8; BOARD_SIZE]; BOARD_SIZE];
    let mut last_move = None;
    for &(x, y, stone) in moves {
        board[y][x] = stone;
        if stone == HUMAN {
            last_move = Some(GomokuPoint { x, y });
        }
    }
    Case { board, last_move }
}
