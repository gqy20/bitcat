//! AI-powered Gomoku move selection.
//!
//! This module turns a validated board snapshot into a narrow structured model
//! extraction request. It avoids the general chat agent and tool registry so a
//! game move is fast, schema-bound, and easy for the app layer to validate.

use crate::ai_config::AiConfig;
use crate::token_tracker::{
    TokenCategory, TokenRecord, TokenUsage, new_session_id, record_token_usage,
};
use rig::client::CompletionClient;
use rig::providers::anthropic;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

const BOARD_SIZE: usize = 15;
const HUMAN: u8 = 1;
const AI: u8 = 2;
const MAX_CANDIDATES: usize = 18;
const DIRS: [(isize, isize); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

/// A zero-based Gomoku board coordinate.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
pub struct GomokuPoint {
    /// Column index from left to right, 0..14.
    pub x: usize,
    /// Row index from top to bottom, 0..14.
    pub y: usize,
}

/// Structured AI move returned by rig's native extractor path.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
pub struct GomokuAiMove {
    /// Column index from left to right, 0..14.
    pub x: usize,
    /// Row index from top to bottom, 0..14.
    pub y: usize,
    /// Optional short table-talk shown in the HUD.
    #[serde(default)]
    pub message: Option<String>,
}

impl GomokuAiMove {
    fn sanitized(mut self) -> Self {
        self.message = self
            .message
            .map(|s| s.trim().chars().take(24).collect::<String>())
            .filter(|s| !s.is_empty());
        self
    }

    fn point(&self) -> GomokuPoint {
        GomokuPoint {
            x: self.x,
            y: self.y,
        }
    }
}

/// Ask the configured model for one legal Gomoku move and validate it.
pub async fn choose_ai_move(
    ai_config: &AiConfig,
    board: &[Vec<u8>],
    last_move: Option<GomokuPoint>,
) -> Result<GomokuAiMove, String> {
    validate_board(board)?;
    if let Some(point) = last_move {
        validate_point(point)?;
    }
    if has_five(board, HUMAN) || has_five(board, AI) {
        return Err("game already ended".into());
    }
    let http_client = rig::http_client::ReqwestClient::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to create Gomoku AI HTTP client: {e}"))?;
    let client = anthropic::Client::builder()
        .api_key(&ai_config.api_key)
        .base_url(&ai_config.base_url)
        .http_client(http_client)
        .build()
        .map_err(|e| format!("failed to create Gomoku AI client: {e}"))?;

    let extractor = client
        .extractor::<GomokuAiMove>(ai_config.model.as_str())
        .preamble(GOMOKU_PREAMBLE)
        .max_tokens(256)
        .retries(1)
        .build();

    let input = build_prompt(board, last_move);
    let start = std::time::Instant::now();
    let response = extractor
        .extract_with_usage(input)
        .await
        .map_err(|e| format!("failed to extract Gomoku move: {e}"))?;
    let elapsed = start.elapsed();

    let selected = response.data.sanitized();
    validate_point(selected.point())?;
    if board[selected.y][selected.x] != 0 {
        return Err(format!(
            "AI selected an occupied point: {},{}",
            selected.x, selected.y
        ));
    }

    record_token_usage(
        &TokenRecord::new(
            new_session_id(),
            TokenCategory::Chat,
            ai_config.model.clone(),
            TokenUsage::from(response.usage),
        )
        .with_extra("gomoku_ai_move".to_string())
        .with_elapsed_ms(elapsed.as_millis() as u64),
    );
    debug!(
        x = selected.x,
        y = selected.y,
        elapsed_ms = elapsed.as_millis(),
        "Gomoku AI move extracted"
    );

    Ok(selected)
}

/// Validate the frontend board snapshot before it reaches the model.
pub fn validate_board(board: &[Vec<u8>]) -> Result<(), String> {
    if board.len() != BOARD_SIZE {
        return Err("gomoku board must contain 15 rows".into());
    }
    for row in board {
        if row.len() != BOARD_SIZE {
            return Err("gomoku board rows must contain 15 cells".into());
        }
        if row.iter().any(|cell| !matches!(*cell, 0 | HUMAN | AI)) {
            return Err("gomoku board cells must be 0, 1, or 2".into());
        }
    }
    Ok(())
}

/// Validate a zero-based board point.
pub fn validate_point(point: GomokuPoint) -> Result<(), String> {
    if point.x >= BOARD_SIZE || point.y >= BOARD_SIZE {
        return Err("gomoku point out of bounds".into());
    }
    Ok(())
}

/// Return true when one side has five contiguous stones.
pub fn has_five(board: &[Vec<u8>], stone: u8) -> bool {
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

fn build_prompt(board: &[Vec<u8>], last_move: Option<GomokuPoint>) -> String {
    let board_lines = board
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| match cell {
                    &HUMAN => 'X',
                    &AI => 'O',
                    _ => '.',
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let last = last_move
        .map(|p| format!("last human move: x={}, y={}", p.x, p.y))
        .unwrap_or_else(|| "last human move: none".into());
    let human_stones = list_stones(board, HUMAN);
    let ai_stones = list_stones(board, AI);
    let ai_wins = immediate_winning_points(board, AI);
    let human_wins = immediate_winning_points(board, HUMAN);
    let ai_forks = fork_points(board, AI);
    let human_forks = fork_points(board, HUMAN);
    let candidates = candidate_points(board);

    format!(
        "{last}\n\
Board rows y=0..14, using . empty, X human, O you:\n{board_lines}\n\
Current stones:\n\
- X human stones: {}\n\
- O your stones: {}\n\
Tactical facts:\n\
- Your immediate winning points: {}\n\
- Human immediate winning points that must be blocked: {}\n\
- Your fork points that create multiple next-turn wins: {}\n\
- Human fork points that must be occupied or neutralized: {}\n\
- Recommended candidate points near existing stones: {}\n\
Decision guidance:\n\
1. Treat the tactical facts as exact legal candidate lists, not as commands.\n\
2. If your immediate winning points is not empty, usually choose the best one, but compare all listed choices.\n\
3. Otherwise, if human immediate winning points is not empty, choose the strongest block from that list.\n\
4. Otherwise, compare your fork points and human fork points; create a double threat when safe, or neutralize the human's strongest double-threat candidate.\n\
5. Otherwise, choose a recommended candidate that extends your line or blocks the strongest human line.\n\
Choose one empty point for O.",
        format_points(&human_stones),
        format_points(&ai_stones),
        format_points(&ai_wins),
        format_points(&human_wins),
        format_points(&ai_forks),
        format_points(&human_forks),
        format_points(&candidates),
    )
}

fn list_stones(board: &[Vec<u8>], stone: u8) -> Vec<GomokuPoint> {
    let mut points = Vec::new();
    for y in 0..BOARD_SIZE {
        for x in 0..BOARD_SIZE {
            if board[y][x] == stone {
                points.push(GomokuPoint { x, y });
            }
        }
    }
    points
}

fn immediate_winning_points(board: &[Vec<u8>], stone: u8) -> Vec<GomokuPoint> {
    empty_points(board)
        .into_iter()
        .filter(|point| would_win(board, *point, stone))
        .collect()
}

fn fork_points(board: &[Vec<u8>], stone: u8) -> Vec<GomokuPoint> {
    let mut scored = empty_points(board)
        .into_iter()
        .filter(|point| has_neighbor(board, *point, 2))
        .filter_map(|point| {
            let mut next = board.to_vec();
            next[point.y][point.x] = stone;
            let wins = immediate_winning_points(&next, stone);
            (wins.len() >= 2).then_some((point, wins.len(), point_score(board, point, stone)))
        })
        .collect::<Vec<_>>();
    scored.sort_by(
        |(left, left_wins, left_score), (right, right_wins, right_score)| {
            right_wins
                .cmp(left_wins)
                .then_with(|| right_score.cmp(left_score))
                .then_with(|| center_distance(*left).cmp(&center_distance(*right)))
                .then_with(|| left.y.cmp(&right.y))
                .then_with(|| left.x.cmp(&right.x))
        },
    );
    scored.into_iter().map(|(point, _, _)| point).collect()
}

fn candidate_points(board: &[Vec<u8>]) -> Vec<GomokuPoint> {
    let stones = list_stones(board, HUMAN)
        .into_iter()
        .chain(list_stones(board, AI))
        .collect::<Vec<_>>();
    if stones.is_empty() {
        return vec![GomokuPoint { x: 7, y: 7 }];
    }

    let mut scored = empty_points(board)
        .into_iter()
        .filter(|point| has_neighbor(board, *point, 2))
        .map(|point| {
            let score = point_score(board, point, AI) * 2 + point_score(board, point, HUMAN);
            (point, score)
        })
        .collect::<Vec<_>>();
    scored.sort_by(|(left, left_score), (right, right_score)| {
        right_score
            .cmp(left_score)
            .then_with(|| center_distance(*left).cmp(&center_distance(*right)))
            .then_with(|| left.y.cmp(&right.y))
            .then_with(|| left.x.cmp(&right.x))
    });
    scored
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|(point, _)| point)
        .collect()
}

fn empty_points(board: &[Vec<u8>]) -> Vec<GomokuPoint> {
    let mut points = Vec::new();
    for y in 0..BOARD_SIZE {
        for x in 0..BOARD_SIZE {
            if board[y][x] == 0 {
                points.push(GomokuPoint { x, y });
            }
        }
    }
    points
}

fn would_win(board: &[Vec<u8>], point: GomokuPoint, stone: u8) -> bool {
    DIRS.iter()
        .any(|(dx, dy)| line_len_with_point(board, point, stone, *dx, *dy) >= 5)
}

fn point_score(board: &[Vec<u8>], point: GomokuPoint, stone: u8) -> i32 {
    DIRS.iter()
        .map(|(dx, dy)| {
            let len = line_len_with_point(board, point, stone, *dx, *dy);
            let open = open_ends_with_point(board, point, stone, *dx, *dy);
            len * len * 10 + open * 7
        })
        .sum()
}

fn line_len_with_point(
    board: &[Vec<u8>],
    point: GomokuPoint,
    stone: u8,
    dx: isize,
    dy: isize,
) -> i32 {
    1 + count_dir(board, point, stone, dx, dy) + count_dir(board, point, stone, -dx, -dy)
}

fn open_ends_with_point(
    board: &[Vec<u8>],
    point: GomokuPoint,
    stone: u8,
    dx: isize,
    dy: isize,
) -> i32 {
    open_end(board, point, stone, dx, dy) as i32 + open_end(board, point, stone, -dx, -dy) as i32
}

fn count_dir(board: &[Vec<u8>], point: GomokuPoint, stone: u8, dx: isize, dy: isize) -> i32 {
    let mut count = 0;
    let mut x = point.x as isize + dx;
    let mut y = point.y as isize + dy;
    while in_bounds_isize(x, y) && board[y as usize][x as usize] == stone {
        count += 1;
        x += dx;
        y += dy;
    }
    count
}

fn open_end(board: &[Vec<u8>], point: GomokuPoint, stone: u8, dx: isize, dy: isize) -> bool {
    let mut x = point.x as isize + dx;
    let mut y = point.y as isize + dy;
    while in_bounds_isize(x, y) && board[y as usize][x as usize] == stone {
        x += dx;
        y += dy;
    }
    in_bounds_isize(x, y) && board[y as usize][x as usize] == 0
}

fn has_neighbor(board: &[Vec<u8>], point: GomokuPoint, radius: isize) -> bool {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx == 0 && dy == 0 {
                continue;
            }
            let x = point.x as isize + dx;
            let y = point.y as isize + dy;
            if in_bounds_isize(x, y) && board[y as usize][x as usize] != 0 {
                return true;
            }
        }
    }
    false
}

fn center_distance(point: GomokuPoint) -> i32 {
    (point.x as i32 - 7).abs() + (point.y as i32 - 7).abs()
}

fn in_bounds_isize(x: isize, y: isize) -> bool {
    x >= 0 && y >= 0 && x < BOARD_SIZE as isize && y < BOARD_SIZE as isize
}

fn format_points(points: &[GomokuPoint]) -> String {
    if points.is_empty() {
        return "none".into();
    }
    points
        .iter()
        .map(|p| format!("({}, {})", p.x, p.y))
        .collect::<Vec<_>>()
        .join(", ")
}

const GOMOKU_PREAMBLE: &str = "\
You are BitCat playing Gomoku as O against the human X on a 15x15 board.
Coordinates are zero-based: x is column 0..14 from left to right, y is row 0..14 from top to bottom.
Choose exactly one empty cell. Five in a row wins horizontally, vertically, or diagonally.
Prefer legal tactical moves: win immediately if possible, otherwise block the human's immediate five, otherwise build your strongest line.
Trust the tactical facts in the user input. They are computed from the board and should override vague visual impressions.
The optional message should be short Chinese table-talk, at most 24 Chinese characters.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_five_diagonal() {
        let mut board = vec![vec![0u8; BOARD_SIZE]; BOARD_SIZE];
        for i in 0..5 {
            board[4 + i][3 + i] = AI;
        }
        assert!(has_five(&board, AI));
        assert!(!has_five(&board, HUMAN));
    }

    #[test]
    fn validates_board_shape_and_values() {
        let mut board = vec![vec![0u8; BOARD_SIZE]; BOARD_SIZE];
        assert!(validate_board(&board).is_ok());
        board[0][0] = 9;
        assert!(validate_board(&board).is_err());
        board[0][0] = 0;
        board.pop();
        assert!(validate_board(&board).is_err());
    }

    #[test]
    fn prompt_contains_board_and_last_move() {
        let mut board = vec![vec![0u8; BOARD_SIZE]; BOARD_SIZE];
        board[7][7] = HUMAN;
        let prompt = build_prompt(&board, Some(GomokuPoint { x: 7, y: 7 }));
        assert!(prompt.contains("last human move: x=7, y=7"));
        assert!(prompt.contains(".......X......."));
        assert!(prompt.contains("Current stones:"));
    }

    #[test]
    fn prompt_lists_human_immediate_win_to_block() {
        let mut board = vec![vec![0u8; BOARD_SIZE]; BOARD_SIZE];
        for x in 3..7 {
            board[7][x] = HUMAN;
        }
        let prompt = build_prompt(&board, Some(GomokuPoint { x: 6, y: 7 }));
        assert!(prompt.contains("Human immediate winning points"));
        assert!(prompt.contains("(2, 7)"));
        assert!(prompt.contains("(7, 7)"));
    }

    #[test]
    fn detects_human_fork_from_recent_losing_shape() {
        let mut board = vec![vec![0u8; BOARD_SIZE]; BOARD_SIZE];
        for (x, y) in [(7, 7), (8, 7), (7, 8), (9, 6), (4, 6)] {
            board[y][x] = HUMAN;
        }
        for (x, y) in [(6, 6), (7, 6), (8, 6), (5, 6)] {
            board[y][x] = AI;
        }
        let forks = fork_points(&board, HUMAN);
        assert!(forks.contains(&GomokuPoint { x: 6, y: 9 }));
        let prompt = build_prompt(&board, Some(GomokuPoint { x: 4, y: 6 }));
        assert!(prompt.contains("Human fork points"));
        assert!(prompt.contains("(6, 9)"));
    }
}
