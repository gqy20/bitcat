//! AI-powered Gomoku move selection.
//!
//! This module turns a validated board snapshot into a narrow structured model
//! extraction request. It avoids the general chat agent and tool registry so a
//! game move is fast, schema-bound, and easy for the app layer to validate.

use crate::ai_config::AiConfig;
use crate::logging::{append_jsonl, log_preview};
use crate::token_tracker::{
    TokenCategory, TokenRecord, TokenUsage, new_session_id, record_token_usage,
};
use chrono::Local;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tracing::{debug, warn};

const BOARD_SIZE: usize = 15;
const HUMAN: u8 = 1;
const AI: u8 = 2;
const MAX_POSITIONAL_CANDIDATES: usize = 10;
const MAX_DECISION_CANDIDATES: usize = 14;
const MAX_FORK_CANDIDATES_PER_SIDE: usize = 4;
const MAX_COMMENTARY_RECOMMENDATIONS: usize = 6;
const GOMOKU_AI_SCHEMA_VERSION: u8 = 5;
const MAX_COMMENTARY_SUMMARY_CHARS: usize = 180;
const MAX_COMMENTARY_SUGGESTION_CHARS: usize = 140;
const MAX_COMMENTARY_TEXT_CHARS: usize = 120;
const MAX_AI_MOVE_ATTEMPTS: u64 = 3;
const GOMOKU_MOVE_TOOL_NAME: &str = "submit_gomoku_move";
const GOMOKU_COMMENTARY_TOOL_NAME: &str = "submit_gomoku_commentary";
const DIRS: [(isize, isize); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

/// A zero-based Gomoku board coordinate.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
pub struct GomokuPoint {
    /// Column index from left to right, 0..14.
    pub x: usize,
    /// Row index from top to bottom, 0..14.
    pub y: usize,
}

/// A compact JSON coordinate represented as `[x, y]`.
pub type GomokuCoord = [usize; 2];

/// Structured AI move returned by rig's native extractor path.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
pub struct GomokuAiMove {
    /// One-based candidate id selected by the model from the candidate consequence table.
    #[serde(default)]
    pub candidate_id: usize,
    /// Column index from left to right, 0..14.
    pub x: usize,
    /// Row index from top to bottom, 0..14.
    pub y: usize,
    /// Tactical category the model used to choose this move.
    pub reason: GomokuMoveReason,
    /// Risk level the model sees after this move.
    pub risk: GomokuMoveRisk,
    /// Black immediate winning points this move blocks, formatted as `[x, y]`.
    #[serde(default)]
    pub blocked_immediate_wins: Vec<GomokuCoord>,
    /// Black fork points this move blocks, formatted as `[x, y]`.
    #[serde(default)]
    pub blocked_forks: Vec<GomokuCoord>,
    /// The selected zero-based candidate coordinate, formatted as `[x, y]`.
    #[serde(default)]
    pub lookahead_candidate: GomokuCoord,
    /// Black's strongest visible reply after the candidate, or null if there is no useful reply.
    #[serde(default)]
    pub black_best_reply: Option<GomokuCoord>,
    /// White's best visible follow-up after that reply, or null if not determined.
    #[serde(default)]
    pub white_followup: Option<GomokuCoord>,
    /// Result label copied from the candidate consequence table.
    #[serde(default)]
    pub line_eval: GomokuLineEval,
    /// Visible Chinese one-line summary of the forecast.
    pub line_summary: String,
    /// Optional short table-talk shown in the HUD.
    #[serde(default)]
    pub message: Option<String>,
    /// Short visible reasoning summary for the game sidebar.
    #[serde(default)]
    pub thought: Option<String>,
}

/// Narrow move schema exposed to the model. Derived lookahead fields are filled by Rust.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
struct GomokuAiDraftMove {
    /// One-based id copied from the selected row in the candidate consequence table.
    pub candidate_id: usize,
    /// Tactical category the model used to choose this move.
    pub reason: GomokuMoveReason,
    /// Risk level the model sees after this move.
    pub risk: GomokuMoveRisk,
    /// Visible Chinese one-line summary of the selected candidate.
    #[serde(default)]
    pub line_summary: Option<String>,
    /// Optional short table-talk shown in the HUD.
    #[serde(default)]
    pub message: Option<String>,
    /// Short visible reasoning summary for the game sidebar.
    #[serde(default)]
    pub thought: Option<String>,
}

/// Tactical category for a Gomoku move decision.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GomokuMoveReason {
    WinNow,
    BlockImmediateWin,
    CreateFork,
    BlockFork,
    Positional,
    DesperateBlock,
}

/// Tactical risk level after the selected move.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GomokuMoveRisk {
    Safe,
    AllowsHumanSingleThreat,
    AllowsHumanFork,
    ForcedLoss,
    Unknown,
}

/// Coarse two-ply evaluation label for a candidate line.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GomokuLineEval {
    WhiteWin,
    Stable,
    Unclear,
    Dangerous,
    Losing,
}

impl Default for GomokuLineEval {
    fn default() -> Self {
        Self::Unclear
    }
}

#[derive(Debug, Serialize)]
struct GomokuAiDiagnosticRecord<'a> {
    timestamp: String,
    schema_version: u8,
    stage: &'a str,
    attempt: u64,
    model: &'a str,
    error: Option<&'a str>,
    board: &'a [Vec<u8>],
    last_move: Option<GomokuPoint>,
    human_stones: usize,
    ai_stones: usize,
    ai_immediate_wins: Vec<GomokuPoint>,
    human_immediate_wins: Vec<GomokuPoint>,
    ai_forks: Vec<GomokuPoint>,
    human_forks: Vec<GomokuPoint>,
    decision_candidates: Vec<GomokuPoint>,
    prompt_chars: usize,
    prompt_hash: u64,
    prompt_preview: String,
}

#[derive(Debug)]
struct RawGomokuMoveResponse {
    draft: GomokuAiDraftMove,
    usage: TokenUsage,
    raw_response: Value,
    raw_response_text: String,
    tool_input: Value,
}

#[derive(Debug)]
struct RawGomokuCommentaryResponse {
    commentary: GomokuCommentary,
    usage: TokenUsage,
    raw_response: Value,
    raw_response_text: String,
    tool_input: Value,
}

/// Structured board commentary returned by a separate low-frequency request.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
pub struct GomokuCommentary {
    /// One-sentence summary of the current position.
    pub summary: String,
    /// Current side with the practical initiative.
    pub advantage: GomokuAdvantage,
    /// Two or three concrete structured facts that matter in the position.
    #[serde(default)]
    pub key_points: Vec<GomokuCommentaryPoint>,
    /// Short suggestion for the human player.
    #[serde(default)]
    pub suggestion: Option<String>,
    /// Concrete empty points the human may consider next.
    #[serde(default)]
    pub recommendations: Vec<GomokuRecommendation>,
}

/// One concrete point mentioned by the Gomoku commentator.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
pub struct GomokuCommentaryPoint {
    /// Side this fact is about.
    pub side: GomokuCommentarySide,
    /// Tactical meaning of this point.
    pub kind: GomokuCommentaryKind,
    /// Optional zero-based coordinate, formatted as `[x, y]`.
    #[serde(default)]
    pub coord: Option<GomokuCoord>,
    /// Short visible Chinese explanation.
    pub text: String,
}

/// One interactive point recommendation for the human player.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
pub struct GomokuRecommendation {
    /// One-based id copied from the Black recommendation table.
    #[serde(default)]
    pub recommendation_id: usize,
    /// Zero-based empty coordinate, formatted as `[x, y]`.
    #[serde(default)]
    pub coord: GomokuCoord,
    /// Relative urgency for the player.
    pub priority: GomokuRecommendationPriority,
    /// Tactical reason for this recommendation.
    pub reason: GomokuRecommendationReason,
    /// Short visible Chinese explanation.
    pub text: String,
}

/// Side assessment for Gomoku commentary.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GomokuAdvantage {
    Human,
    Ai,
    Balanced,
}

/// Side labels accepted by commentary key points.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GomokuCommentarySide {
    Black,
    White,
    Both,
    None,
}

/// Tactical labels accepted by commentary key points.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GomokuCommentaryKind {
    ImmediateWin,
    Fork,
    Block,
    Extension,
    Shape,
}

/// Priority labels accepted by commentary recommendations.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GomokuRecommendationPriority {
    Best,
    Urgent,
    Interesting,
}

impl Default for GomokuRecommendationPriority {
    fn default() -> Self {
        Self::Interesting
    }
}

/// Tactical reason labels accepted by commentary recommendations.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GomokuRecommendationReason {
    Win,
    Block,
    Fork,
    Extend,
    Stabilize,
}

impl Default for GomokuRecommendationReason {
    fn default() -> Self {
        Self::Stabilize
    }
}

#[derive(Debug, Clone)]
struct BlackRecommendationReport {
    point: GomokuPoint,
    purpose: GomokuRecommendationReason,
    priority: GomokuRecommendationPriority,
    black_immediate_wins: Vec<GomokuPoint>,
    black_forks: Vec<GomokuPoint>,
    white_immediate_wins: Vec<GomokuPoint>,
    white_forks: Vec<GomokuPoint>,
    white_best_reply: Option<GomokuPoint>,
    line_eval: GomokuLineEval,
    score_hint: i32,
}

impl GomokuAiDraftMove {
    fn sanitized(mut self) -> Self {
        self.message = self
            .message
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        self.thought = self
            .thought
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        self.line_summary = self
            .line_summary
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        self
    }

    fn into_move(
        self,
        report: &CandidateReport,
        human_wins: &[GomokuPoint],
        human_forks: &[GomokuPoint],
    ) -> GomokuAiMove {
        let blocked_immediate_wins = human_wins
            .contains(&report.point)
            .then_some([report.point.x, report.point.y])
            .into_iter()
            .collect();
        let blocked_forks = human_forks
            .contains(&report.point)
            .then_some([report.point.x, report.point.y])
            .into_iter()
            .collect();
        GomokuAiMove {
            candidate_id: self.candidate_id,
            x: report.point.x,
            y: report.point.y,
            reason: self.reason,
            risk: self.risk,
            blocked_immediate_wins,
            blocked_forks,
            lookahead_candidate: [report.point.x, report.point.y],
            black_best_reply: report.black_best_reply.map(|point| [point.x, point.y]),
            white_followup: report.white_followup.map(|point| [point.x, point.y]),
            line_eval: report.line_eval(),
            line_summary: self.line_summary.unwrap_or_default(),
            message: self.message,
            thought: self.thought,
        }
    }
}

#[cfg(test)]
impl From<GomokuAiMove> for GomokuAiDraftMove {
    fn from(value: GomokuAiMove) -> Self {
        Self {
            candidate_id: value.candidate_id,
            reason: value.reason,
            risk: value.risk,
            line_summary: Some(value.line_summary),
            message: value.message,
            thought: value.thought,
        }
    }
}

impl GomokuCommentary {
    fn sanitized(mut self) -> Self {
        self.summary = truncate_text(self.summary.trim(), MAX_COMMENTARY_SUMMARY_CHARS);
        self.key_points = self
            .key_points
            .into_iter()
            .filter_map(GomokuCommentaryPoint::sanitized)
            .take(3)
            .collect();
        self.suggestion = self
            .suggestion
            .map(|s| truncate_text(s.trim(), MAX_COMMENTARY_SUGGESTION_CHARS))
            .filter(|s| !s.is_empty());
        self.recommendations = self
            .recommendations
            .into_iter()
            .filter_map(GomokuRecommendation::sanitized)
            .take(3)
            .collect();
        self
    }
}

impl GomokuCommentaryPoint {
    fn sanitized(mut self) -> Option<Self> {
        if self
            .coord
            .is_some_and(|coord| coord_to_point(coord).is_none())
        {
            self.coord = None;
        }
        self.text = truncate_text(self.text.trim(), MAX_COMMENTARY_TEXT_CHARS);
        (!self.text.is_empty()).then_some(self)
    }
}

impl GomokuRecommendation {
    fn sanitized(mut self) -> Option<Self> {
        coord_to_point(self.coord)?;
        self.text = truncate_text(self.text.trim(), MAX_COMMENTARY_TEXT_CHARS);
        (!self.text.is_empty()).then_some(self)
    }

    fn point(&self) -> GomokuPoint {
        GomokuPoint {
            x: self.coord[0],
            y: self.coord[1],
        }
    }
}

impl BlackRecommendationReport {
    fn line_eval_label(&self) -> &'static str {
        match self.line_eval {
            GomokuLineEval::WhiteWin => "white_win",
            GomokuLineEval::Stable => "stable",
            GomokuLineEval::Unclear => "unclear",
            GomokuLineEval::Dangerous => "dangerous",
            GomokuLineEval::Losing => "losing",
        }
    }

    fn priority_label(&self) -> &'static str {
        match self.priority {
            GomokuRecommendationPriority::Best => "best",
            GomokuRecommendationPriority::Urgent => "urgent",
            GomokuRecommendationPriority::Interesting => "interesting",
        }
    }

    fn purpose_label(&self) -> &'static str {
        match self.purpose {
            GomokuRecommendationReason::Win => "win",
            GomokuRecommendationReason::Block => "block",
            GomokuRecommendationReason::Fork => "fork",
            GomokuRecommendationReason::Extend => "extend",
            GomokuRecommendationReason::Stabilize => "stabilize",
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
    let http_client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to create Gomoku AI HTTP client: {e}"))?;

    let base_input = build_prompt(board, last_move);
    let mut input = base_input.clone();
    let start = std::time::Instant::now();
    let mut response = None;
    let mut selected_move = None;
    let mut validation_feedback = None;
    let mut last_error = None;
    for attempt in 1..=MAX_AI_MOVE_ATTEMPTS {
        if let Some(feedback) = validation_feedback.take() {
            input = format!(
                "{base_input}\n\nPrevious structured move was rejected: {feedback}\nChoose again from the candidate consequence table. Keep candidate_id, reason, risk, and visible text consistent with the selected candidate. For visible text, write concise Chinese sidebar copy and avoid English, internal labels, scores, or candidate-table wording."
            );
        }
        debug_gomoku_ai_request(ai_config.model.as_str(), board, last_move, &input, attempt);
        let attempt_response = match request_gomoku_move_raw(
            &http_client,
            ai_config,
            board,
            last_move,
            &input,
            attempt,
        )
        .await
        {
            Ok(response) => response,
            Err(e) => {
                let error = format!("failed to extract Gomoku move: {e}");
                record_gomoku_ai_diagnostic(
                    "extract_error",
                    ai_config.model.as_str(),
                    board,
                    last_move,
                    &input,
                    attempt,
                    Some(error.as_str()),
                );
                if attempt < MAX_AI_MOVE_ATTEMPTS {
                    validation_feedback = Some(error);
                    continue;
                }
                last_error = Some(error);
                break;
            }
        };
        let selected =
            match complete_ai_selection(board, attempt_response.draft.clone().sanitized()) {
                Ok(selected) => selected,
                Err(e) => {
                    record_gomoku_ai_raw_event(
                        "move_validation_error",
                        ai_config.model.as_str(),
                        board,
                        last_move,
                        &input,
                        attempt,
                        Some(e.as_str()),
                        Some(&attempt_response.raw_response),
                        Some(&attempt_response.raw_response_text),
                        Some(&attempt_response.tool_input),
                        Some(&attempt_response.draft),
                        None,
                    );
                    record_gomoku_ai_diagnostic(
                        "validation_error",
                        ai_config.model.as_str(),
                        board,
                        last_move,
                        &input,
                        attempt,
                        Some(e.as_str()),
                    );
                    if attempt < MAX_AI_MOVE_ATTEMPTS {
                        validation_feedback = Some(e);
                        continue;
                    }
                    return Err(e);
                }
            };
        record_gomoku_ai_raw_event(
            "move_success",
            ai_config.model.as_str(),
            board,
            last_move,
            &input,
            attempt,
            None,
            Some(&attempt_response.raw_response),
            Some(&attempt_response.raw_response_text),
            Some(&attempt_response.tool_input),
            Some(&attempt_response.draft),
            Some(&selected),
        );
        selected_move = Some(selected);
        response = Some(attempt_response);
        break;
    }
    let response = response.ok_or_else(|| {
        last_error.unwrap_or_else(|| "failed to extract valid Gomoku move".to_string())
    })?;
    let elapsed = start.elapsed();

    let selected =
        selected_move.ok_or_else(|| "failed to extract valid Gomoku move".to_string())?;

    record_token_usage(
        &TokenRecord::new(
            new_session_id(),
            TokenCategory::Chat,
            ai_config.model.clone(),
            response.usage,
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

/// Ask the configured model for a short board commentary for the sidebar.
pub async fn comment_position(
    ai_config: &AiConfig,
    board: &[Vec<u8>],
    last_move: Option<GomokuPoint>,
) -> Result<GomokuCommentary, String> {
    validate_board(board)?;
    if let Some(point) = last_move {
        validate_point(point)?;
    }

    let http_client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to create Gomoku commentary HTTP client: {e}"))?;

    let input = build_commentary_prompt(board, last_move);
    let start = std::time::Instant::now();
    let response =
        request_gomoku_commentary_raw(&http_client, ai_config, board, last_move, &input, 1)
            .await
            .map_err(|e| {
                let error = format!("failed to extract Gomoku commentary: {e}");
                record_gomoku_ai_diagnostic(
                    "commentary_extract_error",
                    ai_config.model.as_str(),
                    board,
                    last_move,
                    &input,
                    1,
                    Some(error.as_str()),
                );
                error
            })?;
    let elapsed = start.elapsed();
    let commentary = complete_commentary(board, response.commentary.clone().sanitized())?;
    validate_commentary(board, &commentary).map_err(|e| {
        record_gomoku_commentary_raw_event(
            "commentary_validation_error",
            ai_config.model.as_str(),
            board,
            last_move,
            &input,
            1,
            Some(e.as_str()),
            Some(&response.raw_response),
            Some(&response.raw_response_text),
            Some(&response.tool_input),
            Some(&response.commentary),
            None,
        );
        record_gomoku_ai_diagnostic(
            "commentary_validation_error",
            ai_config.model.as_str(),
            board,
            last_move,
            &input,
            1,
            Some(e.as_str()),
        );
        e
    })?;
    record_gomoku_commentary_raw_event(
        "commentary_success",
        ai_config.model.as_str(),
        board,
        last_move,
        &input,
        1,
        None,
        Some(&response.raw_response),
        Some(&response.raw_response_text),
        Some(&response.tool_input),
        Some(&response.commentary),
        Some(&commentary),
    );

    record_token_usage(
        &TokenRecord::new(
            new_session_id(),
            TokenCategory::Chat,
            ai_config.model.clone(),
            response.usage,
        )
        .with_extra("gomoku_commentary".to_string())
        .with_elapsed_ms(elapsed.as_millis() as u64),
    );

    Ok(commentary)
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

#[cfg(test)]
fn validate_ai_selection(board: &[Vec<u8>], selected: &GomokuAiMove) -> Result<(), String> {
    complete_ai_selection(board, GomokuAiDraftMove::from(selected.clone())).map(|_| ())
}

fn complete_ai_selection(
    board: &[Vec<u8>],
    selected: GomokuAiDraftMove,
) -> Result<GomokuAiMove, String> {
    let ai_wins = immediate_winning_points(board, AI);
    let human_wins = immediate_winning_points(board, HUMAN);
    let raw_ai_forks = fork_points(board, AI);
    let raw_human_forks = fork_points(board, HUMAN);
    let ai_forks = relevant_fork_points(&raw_ai_forks, MAX_FORK_CANDIDATES_PER_SIDE);
    let human_forks = relevant_fork_points(&raw_human_forks, MAX_FORK_CANDIDATES_PER_SIDE);
    let decision_candidates =
        decision_candidate_points(board, &ai_wins, &human_wins, &ai_forks, &human_forks);
    let reports = candidate_reports(
        board,
        &decision_candidates,
        &ai_wins,
        &human_wins,
        &human_forks,
    );
    if selected.candidate_id == 0 || selected.candidate_id > reports.len() {
        return Err(format!(
            "AI selected candidate_id {} outside the candidate consequence table range 1..={}",
            selected.candidate_id,
            reports.len()
        ));
    }
    let report = reports
        .into_iter()
        .nth(selected.candidate_id - 1)
        .ok_or_else(|| "candidate table lookup failed".to_string())?;

    validate_reason_consistency(
        &selected,
        &report,
        &ai_wins,
        &human_wins,
        &ai_forks,
        &human_forks,
    )?;
    validate_risk_consistency(&selected, &report)?;
    Ok(selected.into_move(&report, &human_wins, &human_forks))
}

fn validate_commentary(board: &[Vec<u8>], commentary: &GomokuCommentary) -> Result<(), String> {
    let allowed_recommendations = commentary_recommendation_points(board);
    for point in &commentary.key_points {
        if let Some(coord) = point.coord {
            coord_to_point(coord).ok_or_else(|| {
                format!("commentary key point coordinate {coord:?} is out of bounds")
            })?;
        }
    }
    for recommendation in &commentary.recommendations {
        let point = recommendation.point();
        validate_point(point)?;
        if board[point.y][point.x] != 0 {
            return Err(format!(
                "commentary recommendation {},{} is not empty",
                point.x, point.y
            ));
        }
        if !allowed_recommendations.contains(&point) {
            return Err(format!(
                "commentary recommendation {},{} is outside allowed recommendation points",
                point.x, point.y
            ));
        }
    }
    Ok(())
}

fn complete_commentary(
    board: &[Vec<u8>],
    mut commentary: GomokuCommentary,
) -> Result<GomokuCommentary, String> {
    let reports = black_recommendation_reports(board);
    let mut completed = Vec::new();
    for mut recommendation in commentary.recommendations.into_iter().take(3) {
        if recommendation.recommendation_id == 0 || recommendation.recommendation_id > reports.len()
        {
            return Err(format!(
                "commentary recommendation_id {} outside the Black recommendation table range 1..={}",
                recommendation.recommendation_id,
                reports.len()
            ));
        }
        let report = &reports[recommendation.recommendation_id - 1];
        recommendation.coord = [report.point.x, report.point.y];
        recommendation.reason = report.purpose.clone();
        recommendation.priority = report.priority.clone();
        completed.push(recommendation);
    }
    commentary.recommendations = completed;
    Ok(commentary)
}

fn validate_reason_consistency(
    selected: &GomokuAiDraftMove,
    report: &CandidateReport,
    ai_wins: &[GomokuPoint],
    human_wins: &[GomokuPoint],
    _ai_forks: &[GomokuPoint],
    human_forks: &[GomokuPoint],
) -> Result<(), String> {
    let point = report.point;
    match selected.reason {
        GomokuMoveReason::WinNow if !ai_wins.contains(&point) => {
            Err("reason win_now requires selecting one of your immediate winning points".into())
        }
        GomokuMoveReason::BlockImmediateWin if !human_wins.contains(&point) => Err(
            "reason block_immediate_win requires selecting a human immediate winning point".into(),
        ),
        GomokuMoveReason::CreateFork if report.ai_next_wins.len() < 2 => {
            Err("reason create_fork requires creating multiple next-turn wins".into())
        }
        GomokuMoveReason::BlockFork if !human_forks.contains(&point) => {
            Err("reason block_fork requires selecting a human fork point".into())
        }
        GomokuMoveReason::DesperateBlock
            if report.risk() != GomokuMoveRisk::ForcedLoss && human_wins.len() < 2 =>
        {
            Err(
                "reason desperate_block should be used only for forced-loss defensive positions"
                    .into(),
            )
        }
        _ => Ok(()),
    }
}

fn validate_risk_consistency(
    selected: &GomokuAiDraftMove,
    report: &CandidateReport,
) -> Result<(), String> {
    let actual = report.risk();
    match (&selected.risk, &actual) {
        (GomokuMoveRisk::Safe, GomokuMoveRisk::Safe)
        | (GomokuMoveRisk::AllowsHumanSingleThreat, GomokuMoveRisk::AllowsHumanSingleThreat)
        | (GomokuMoveRisk::AllowsHumanFork, GomokuMoveRisk::AllowsHumanFork)
        | (GomokuMoveRisk::ForcedLoss, GomokuMoveRisk::ForcedLoss)
        | (GomokuMoveRisk::Unknown, _) => Ok(()),
        (claimed, actual) => Err(format!(
            "risk mismatch: model claimed {claimed:?}, candidate table says {actual:?}"
        )),
    }
}

async fn request_gomoku_move_raw(
    http_client: &reqwest::Client,
    ai_config: &AiConfig,
    board: &[Vec<u8>],
    last_move: Option<GomokuPoint>,
    input: &str,
    attempt: u64,
) -> Result<RawGomokuMoveResponse, String> {
    let url = format!(
        "{}/v1/messages",
        ai_config
            .base_url
            .trim_end_matches("/v1/messages")
            .trim_end_matches("/messages")
            .trim_end_matches("/v1")
            .trim_end_matches('/')
    );
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(&ai_config.api_key)
            .map_err(|e| format!("invalid Gomoku AI API key header: {e}"))?,
    );
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

    let body = json!({
        "model": ai_config.model,
        "max_tokens": 1024,
        "system": GOMOKU_PREAMBLE,
        "messages": [{"role": "user", "content": input}],
        "tool_choice": {"type": "tool", "name": GOMOKU_MOVE_TOOL_NAME},
        "tools": [{
            "name": GOMOKU_MOVE_TOOL_NAME,
            "description": "Submit one structured Gomoku move for White.",
            "input_schema": gomoku_move_tool_schema()
        }]
    });

    let response = match http_client
        .post(url)
        .headers(headers)
        .json(&body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            let error = format!("Gomoku AI HTTP request failed: {e}");
            record_gomoku_ai_raw_event(
                "move_http_request_error",
                ai_config.model.as_str(),
                board,
                last_move,
                input,
                attempt,
                Some(error.as_str()),
                None,
                None,
                None,
                None,
                None,
            );
            return Err(error);
        }
    };
    let status = response.status();
    let raw_response_text = match response.text().await {
        Ok(text) => text,
        Err(e) => {
            let error = format!("failed to read Gomoku AI response body: {e}");
            record_gomoku_ai_raw_event(
                "move_body_read_error",
                ai_config.model.as_str(),
                board,
                last_move,
                input,
                attempt,
                Some(error.as_str()),
                None,
                None,
                None,
                None,
                None,
            );
            return Err(error);
        }
    };
    if !status.is_success() {
        let error = format!(
            "Gomoku AI HTTP status {status}: {}",
            log_preview(&raw_response_text, 800)
        );
        record_gomoku_ai_raw_event(
            "move_http_error",
            ai_config.model.as_str(),
            board,
            last_move,
            input,
            attempt,
            Some(error.as_str()),
            None,
            Some(&raw_response_text),
            None,
            None,
            None,
        );
        return Err(format!(
            "Gomoku AI HTTP status {status}: {}",
            log_preview(&raw_response_text, 800)
        ));
    }

    let raw_response = serde_json::from_str::<Value>(&raw_response_text).map_err(|e| {
        let error = format!("failed to parse Gomoku AI raw response JSON: {e}");
        record_gomoku_ai_raw_event(
            "move_raw_json_error",
            ai_config.model.as_str(),
            board,
            last_move,
            input,
            attempt,
            Some(error.as_str()),
            None,
            Some(&raw_response_text),
            None,
            None,
            None,
        );
        error
    })?;
    let tool_input = extract_gomoku_tool_input(&raw_response).ok_or_else(|| {
        let error =
            format!("Gomoku AI response did not contain {GOMOKU_MOVE_TOOL_NAME} tool input");
        record_gomoku_ai_raw_event(
            "move_missing_tool_input",
            ai_config.model.as_str(),
            board,
            last_move,
            input,
            attempt,
            Some(error.as_str()),
            Some(&raw_response),
            Some(&raw_response_text),
            None,
            None,
            None,
        );
        error
    })?;
    let draft = serde_json::from_value::<GomokuAiDraftMove>(tool_input.clone()).map_err(|e| {
        let error = format!("failed to parse Gomoku AI tool input: {e}");
        record_gomoku_ai_raw_event(
            "move_tool_input_parse_error",
            ai_config.model.as_str(),
            board,
            last_move,
            input,
            attempt,
            Some(error.as_str()),
            Some(&raw_response),
            Some(&raw_response_text),
            Some(&tool_input),
            None,
            None,
        );
        error
    })?;
    let usage = token_usage_from_anthropic_response(&raw_response);

    Ok(RawGomokuMoveResponse {
        draft,
        usage,
        raw_response,
        raw_response_text,
        tool_input,
    })
}

fn gomoku_move_tool_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["candidate_id", "reason", "risk", "message", "thought"],
        "properties": {
            "candidate_id": {"type": "integer", "minimum": 1},
            "reason": {"type": "string", "enum": [
                "win_now",
                "block_immediate_win",
                "create_fork",
                "block_fork",
                "positional",
                "desperate_block"
            ]},
            "risk": {"type": "string", "enum": [
                "safe",
                "allows_human_single_threat",
                "allows_human_fork",
                "forced_loss",
                "unknown"
            ]},
            "line_summary": {"type": "string"},
            "message": {"type": "string"},
            "thought": {"type": "string"}
        }
    })
}

async fn request_gomoku_commentary_raw(
    http_client: &reqwest::Client,
    ai_config: &AiConfig,
    board: &[Vec<u8>],
    last_move: Option<GomokuPoint>,
    input: &str,
    attempt: u64,
) -> Result<RawGomokuCommentaryResponse, String> {
    let url = format!(
        "{}/v1/messages",
        ai_config
            .base_url
            .trim_end_matches("/v1/messages")
            .trim_end_matches("/messages")
            .trim_end_matches("/v1")
            .trim_end_matches('/')
    );
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(&ai_config.api_key)
            .map_err(|e| format!("invalid Gomoku commentary API key header: {e}"))?,
    );
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

    let body = json!({
        "model": ai_config.model,
        "max_tokens": 1024,
        "system": GOMOKU_COMMENTARY_PREAMBLE,
        "messages": [{"role": "user", "content": input}],
        "tool_choice": {"type": "tool", "name": GOMOKU_COMMENTARY_TOOL_NAME},
        "tools": [{
            "name": GOMOKU_COMMENTARY_TOOL_NAME,
            "description": "Submit structured Gomoku commentary and Black recommendations.",
            "input_schema": gomoku_commentary_tool_schema()
        }]
    });

    let response = match http_client
        .post(url)
        .headers(headers)
        .json(&body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            let error = format!("Gomoku commentary HTTP request failed: {e}");
            record_gomoku_commentary_raw_event(
                "commentary_http_request_error",
                ai_config.model.as_str(),
                board,
                last_move,
                input,
                attempt,
                Some(error.as_str()),
                None,
                None,
                None,
                None,
                None,
            );
            return Err(error);
        }
    };
    let status = response.status();
    let raw_response_text = match response.text().await {
        Ok(text) => text,
        Err(e) => {
            let error = format!("failed to read Gomoku commentary response body: {e}");
            record_gomoku_commentary_raw_event(
                "commentary_body_read_error",
                ai_config.model.as_str(),
                board,
                last_move,
                input,
                attempt,
                Some(error.as_str()),
                None,
                None,
                None,
                None,
                None,
            );
            return Err(error);
        }
    };
    if !status.is_success() {
        let error = format!(
            "Gomoku commentary HTTP status {status}: {}",
            log_preview(&raw_response_text, 800)
        );
        record_gomoku_commentary_raw_event(
            "commentary_http_error",
            ai_config.model.as_str(),
            board,
            last_move,
            input,
            attempt,
            Some(error.as_str()),
            None,
            Some(&raw_response_text),
            None,
            None,
            None,
        );
        return Err(error);
    }

    let raw_response = serde_json::from_str::<Value>(&raw_response_text).map_err(|e| {
        let error = format!("failed to parse Gomoku commentary raw response JSON: {e}");
        record_gomoku_commentary_raw_event(
            "commentary_raw_json_error",
            ai_config.model.as_str(),
            board,
            last_move,
            input,
            attempt,
            Some(error.as_str()),
            None,
            Some(&raw_response_text),
            None,
            None,
            None,
        );
        error
    })?;
    let tool_input = extract_named_tool_input(&raw_response, GOMOKU_COMMENTARY_TOOL_NAME)
        .ok_or_else(|| {
            let error = format!(
                "Gomoku commentary response did not contain {GOMOKU_COMMENTARY_TOOL_NAME} tool input"
            );
            record_gomoku_commentary_raw_event(
                "commentary_missing_tool_input",
                ai_config.model.as_str(),
                board,
                last_move,
                input,
                attempt,
                Some(error.as_str()),
                Some(&raw_response),
                Some(&raw_response_text),
                None,
                None,
                None,
            );
            error
        })?;
    let commentary =
        serde_json::from_value::<GomokuCommentary>(tool_input.clone()).map_err(|e| {
            let error = format!("failed to parse Gomoku commentary tool input: {e}");
            record_gomoku_commentary_raw_event(
                "commentary_tool_input_parse_error",
                ai_config.model.as_str(),
                board,
                last_move,
                input,
                attempt,
                Some(error.as_str()),
                Some(&raw_response),
                Some(&raw_response_text),
                Some(&tool_input),
                None,
                None,
            );
            error
        })?;
    let usage = token_usage_from_anthropic_response(&raw_response);
    Ok(RawGomokuCommentaryResponse {
        commentary,
        usage,
        raw_response,
        raw_response_text,
        tool_input,
    })
}

fn gomoku_commentary_tool_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "advantage", "key_points", "suggestion", "recommendations"],
        "properties": {
            "summary": {"type": "string"},
            "advantage": {"type": "string", "enum": ["human", "ai", "balanced"]},
            "key_points": {
                "type": "array",
                "maxItems": 3,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["side", "kind", "coord", "text"],
                    "properties": {
                        "side": {"type": "string", "enum": ["black", "white", "both", "none"]},
                        "kind": {"type": "string", "enum": ["immediate_win", "fork", "block", "extension", "shape"]},
                        "coord": {
                            "anyOf": [
                                {"type": "array", "prefixItems": [{"type": "integer"}, {"type": "integer"}], "minItems": 2, "maxItems": 2},
                                {"type": "null"}
                            ]
                        },
                        "text": {"type": "string"}
                    }
                }
            },
            "suggestion": {"type": "string"},
            "recommendations": {
                "type": "array",
                "maxItems": 3,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["recommendation_id", "priority", "reason", "text"],
                    "properties": {
                        "recommendation_id": {"type": "integer", "minimum": 1},
                        "coord": {"type": "array", "prefixItems": [{"type": "integer"}, {"type": "integer"}], "minItems": 2, "maxItems": 2},
                        "priority": {"type": "string", "enum": ["best", "urgent", "interesting"]},
                        "reason": {"type": "string", "enum": ["win", "block", "fork", "extend", "stabilize"]},
                        "text": {"type": "string"}
                    }
                }
            }
        }
    })
}

fn extract_gomoku_tool_input(raw_response: &Value) -> Option<Value> {
    extract_named_tool_input(raw_response, GOMOKU_MOVE_TOOL_NAME)
}

fn extract_named_tool_input(raw_response: &Value, tool_name: &str) -> Option<Value> {
    raw_response
        .get("content")?
        .as_array()?
        .iter()
        .find(|block| {
            block.get("type").and_then(Value::as_str) == Some("tool_use")
                && block.get("name").and_then(Value::as_str) == Some(tool_name)
        })
        .and_then(|block| block.get("input"))
        .cloned()
}

fn token_usage_from_anthropic_response(raw_response: &Value) -> TokenUsage {
    let usage = raw_response.get("usage").unwrap_or(&Value::Null);
    let input_tokens = json_u64(usage, "input_tokens");
    let output_tokens = json_u64(usage, "output_tokens");
    let cache_read_tokens = json_u64(usage, "cache_read_input_tokens");
    let cache_write_tokens = json_u64(usage, "cache_creation_input_tokens");
    TokenUsage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens + output_tokens + cache_read_tokens + cache_write_tokens,
        cache_read_tokens,
        cache_write_tokens,
    }
}

fn json_u64(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn debug_gomoku_ai_request(
    model: &str,
    board: &[Vec<u8>],
    last_move: Option<GomokuPoint>,
    prompt: &str,
    attempt: u64,
) {
    let ai_wins = immediate_winning_points(board, AI);
    let human_wins = immediate_winning_points(board, HUMAN);
    let ai_forks = relevant_fork_points(&fork_points(board, AI), MAX_FORK_CANDIDATES_PER_SIDE);
    let human_forks =
        relevant_fork_points(&fork_points(board, HUMAN), MAX_FORK_CANDIDATES_PER_SIDE);
    let decision_candidates =
        decision_candidate_points(board, &ai_wins, &human_wins, &ai_forks, &human_forks);
    debug!(
        schema_version = GOMOKU_AI_SCHEMA_VERSION,
        attempt,
        model,
        ?last_move,
        human_stones = list_stones(board, HUMAN).len(),
        ai_stones = list_stones(board, AI).len(),
        ai_immediate_wins = ai_wins.len(),
        human_immediate_wins = human_wins.len(),
        ai_forks = ai_forks.len(),
        human_forks = human_forks.len(),
        decision_candidates = decision_candidates.len(),
        prompt_chars = prompt.chars().count(),
        prompt_hash = prompt_hash(prompt),
        "[gomoku] AI move extraction request"
    );
}

fn record_gomoku_ai_diagnostic(
    stage: &str,
    model: &str,
    board: &[Vec<u8>],
    last_move: Option<GomokuPoint>,
    prompt: &str,
    attempt: u64,
    error: Option<&str>,
) {
    let ai_wins = immediate_winning_points(board, AI);
    let human_wins = immediate_winning_points(board, HUMAN);
    let ai_forks = relevant_fork_points(&fork_points(board, AI), MAX_FORK_CANDIDATES_PER_SIDE);
    let human_forks =
        relevant_fork_points(&fork_points(board, HUMAN), MAX_FORK_CANDIDATES_PER_SIDE);
    let decision_candidates =
        decision_candidate_points(board, &ai_wins, &human_wins, &ai_forks, &human_forks);
    let record = GomokuAiDiagnosticRecord {
        timestamp: Local::now().to_rfc3339(),
        schema_version: GOMOKU_AI_SCHEMA_VERSION,
        stage,
        attempt,
        model,
        error,
        board,
        last_move,
        human_stones: list_stones(board, HUMAN).len(),
        ai_stones: list_stones(board, AI).len(),
        ai_immediate_wins: ai_wins,
        human_immediate_wins: human_wins,
        ai_forks,
        human_forks,
        decision_candidates,
        prompt_chars: prompt.chars().count(),
        prompt_hash: prompt_hash(prompt),
        prompt_preview: log_preview(prompt, 1200),
    };
    if let Err(e) = append_jsonl("gomoku_ai_errors.jsonl", &record) {
        warn!(error = %e, "[gomoku] failed to write AI diagnostic record");
    }
}

#[allow(clippy::too_many_arguments)]
fn record_gomoku_ai_raw_event(
    stage: &str,
    model: &str,
    board: &[Vec<u8>],
    last_move: Option<GomokuPoint>,
    prompt: &str,
    attempt: u64,
    error: Option<&str>,
    raw_response: Option<&Value>,
    raw_response_text: Option<&str>,
    tool_input: Option<&Value>,
    parsed_draft: Option<&GomokuAiDraftMove>,
    completed_move: Option<&GomokuAiMove>,
) {
    let ai_wins = immediate_winning_points(board, AI);
    let human_wins = immediate_winning_points(board, HUMAN);
    let ai_forks = relevant_fork_points(&fork_points(board, AI), MAX_FORK_CANDIDATES_PER_SIDE);
    let human_forks =
        relevant_fork_points(&fork_points(board, HUMAN), MAX_FORK_CANDIDATES_PER_SIDE);
    let decision_candidates =
        decision_candidate_points(board, &ai_wins, &human_wins, &ai_forks, &human_forks);
    let record = json!({
        "timestamp": Local::now().to_rfc3339(),
        "schema_version": GOMOKU_AI_SCHEMA_VERSION,
        "stage": stage,
        "attempt": attempt,
        "model": model,
        "error": error,
        "board": board,
        "last_move": last_move,
        "human_stones": list_stones(board, HUMAN).len(),
        "ai_stones": list_stones(board, AI).len(),
        "ai_immediate_wins": ai_wins,
        "human_immediate_wins": human_wins,
        "ai_forks": ai_forks,
        "human_forks": human_forks,
        "decision_candidates": decision_candidates,
        "prompt_chars": prompt.chars().count(),
        "prompt_hash": prompt_hash(prompt),
        "prompt_preview": log_preview(prompt, 1200),
        "raw_response_text_chars": raw_response_text.map(|text| text.chars().count()),
        "raw_response_text_preview": raw_response_text.map(|text| log_preview(text, 2000)),
        "raw_response": raw_response,
        "tool_input": tool_input,
        "parsed_draft": parsed_draft,
        "completed_move": completed_move,
        "tool_input_text": visible_text_from_value(tool_input),
        "parsed_text": visible_text_from_draft(parsed_draft),
        "completed_text": visible_text_from_move(completed_move),
        "response_meta": raw_response.map(raw_response_meta),
    });
    if let Err(e) = append_jsonl("gomoku_ai_raw.jsonl", &record) {
        warn!(error = %e, "[gomoku] failed to write raw AI response record");
    }
}

#[allow(clippy::too_many_arguments)]
fn record_gomoku_commentary_raw_event(
    stage: &str,
    model: &str,
    board: &[Vec<u8>],
    last_move: Option<GomokuPoint>,
    prompt: &str,
    attempt: u64,
    error: Option<&str>,
    raw_response: Option<&Value>,
    raw_response_text: Option<&str>,
    tool_input: Option<&Value>,
    parsed_commentary: Option<&GomokuCommentary>,
    completed_commentary: Option<&GomokuCommentary>,
) {
    let reports = black_recommendation_reports(board);
    let record = json!({
        "timestamp": Local::now().to_rfc3339(),
        "schema_version": GOMOKU_AI_SCHEMA_VERSION,
        "stage": stage,
        "attempt": attempt,
        "model": model,
        "error": error,
        "board": board,
        "last_move": last_move,
        "human_stones": list_stones(board, HUMAN).len(),
        "ai_stones": list_stones(board, AI).len(),
        "black_recommendation_reports": commentary_report_json(&reports),
        "prompt_chars": prompt.chars().count(),
        "prompt_hash": prompt_hash(prompt),
        "prompt_preview": log_preview(prompt, 1600),
        "raw_response_text_chars": raw_response_text.map(|text| text.chars().count()),
        "raw_response_text_preview": raw_response_text.map(|text| log_preview(text, 2400)),
        "raw_response": raw_response,
        "tool_input": tool_input,
        "parsed_commentary": parsed_commentary,
        "completed_commentary": completed_commentary,
        "tool_recommendation_ids": recommendation_ids_from_value(tool_input),
        "parsed_recommendation_ids": parsed_commentary.map(commentary_recommendation_ids),
        "completed_recommendation_ids": completed_commentary.map(commentary_recommendation_ids),
        "response_meta": raw_response.map(raw_response_meta),
    });
    if let Err(e) = append_jsonl("gomoku_commentary_raw.jsonl", &record) {
        warn!(error = %e, "[gomoku] failed to write raw commentary response record");
    }
}

fn commentary_report_json(reports: &[BlackRecommendationReport]) -> Value {
    Value::Array(
        reports
            .iter()
            .enumerate()
            .map(|(index, report)| {
                json!({
                    "recommendation_id": index + 1,
                    "black_at": [report.point.x, report.point.y],
                    "purpose": report.purpose_label(),
                    "priority": report.priority_label(),
                    "score_hint": report.score_hint,
                    "black_next_wins": report.black_immediate_wins,
                    "black_forks": report.black_forks,
                    "white_next_wins_after": report.white_immediate_wins,
                    "white_forks_after": report.white_forks,
                    "white_best_reply": report.white_best_reply.map(|p| [p.x, p.y]),
                    "line_eval_for_black": report.line_eval_label(),
                })
            })
            .collect(),
    )
}

fn recommendation_ids_from_value(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let ids = value
        .get("recommendations")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("recommendation_id").and_then(Value::as_u64))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!(ids)
}

fn commentary_recommendation_ids(commentary: &GomokuCommentary) -> Vec<usize> {
    commentary
        .recommendations
        .iter()
        .map(|recommendation| recommendation.recommendation_id)
        .collect()
}

fn visible_text_from_value(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "line_summary": text_stats(value.get("line_summary").and_then(Value::as_str)),
        "message": text_stats(value.get("message").and_then(Value::as_str)),
        "thought": text_stats(value.get("thought").and_then(Value::as_str)),
    })
}

fn visible_text_from_draft(draft: Option<&GomokuAiDraftMove>) -> Value {
    let Some(draft) = draft else {
        return Value::Null;
    };
    json!({
        "line_summary": text_stats(draft.line_summary.as_deref()),
        "message": text_stats(draft.message.as_deref()),
        "thought": text_stats(draft.thought.as_deref()),
    })
}

fn visible_text_from_move(mv: Option<&GomokuAiMove>) -> Value {
    let Some(mv) = mv else {
        return Value::Null;
    };
    json!({
        "line_summary": text_stats(Some(mv.line_summary.as_str())),
        "message": text_stats(mv.message.as_deref()),
        "thought": text_stats(mv.thought.as_deref()),
    })
}

fn text_stats(text: Option<&str>) -> Value {
    let Some(text) = text else {
        return Value::Null;
    };
    let chars = text.chars().count();
    let tail = text
        .chars()
        .rev()
        .take(24)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    json!({
        "text": text,
        "chars": chars,
        "bytes": text.len(),
        "tail": tail,
    })
}

fn raw_response_meta(raw_response: &Value) -> Value {
    let content_types = raw_response
        .get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| block.get("type").and_then(Value::as_str))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "id": raw_response.get("id").and_then(Value::as_str),
        "type": raw_response.get("type").and_then(Value::as_str),
        "role": raw_response.get("role").and_then(Value::as_str),
        "model": raw_response.get("model").and_then(Value::as_str),
        "stop_reason": raw_response.get("stop_reason").and_then(Value::as_str),
        "stop_sequence": raw_response.get("stop_sequence"),
        "usage": raw_response.get("usage"),
        "content_types": content_types,
    })
}

fn prompt_hash(prompt: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    prompt.hash(&mut hasher);
    hasher.finish()
}

fn coord_to_point(coord: GomokuCoord) -> Option<GomokuPoint> {
    let point = GomokuPoint {
        x: coord[0],
        y: coord[1],
    };
    validate_point(point).ok()?;
    Some(point)
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = text
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    out.push('…');
    out
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

#[derive(Debug, Clone)]
struct CandidateReport {
    point: GomokuPoint,
    score_hint: i32,
    role: CandidateRole,
    ai_next_wins: Vec<GomokuPoint>,
    human_next_wins: Vec<GomokuPoint>,
    human_fork_replies: Vec<HumanForkReply>,
    black_best_reply: Option<GomokuPoint>,
    white_followup: Option<GomokuPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateRole {
    WinNow,
    BlockImmediateWin,
    CreateFork,
    BlockFork,
    Positional,
}

#[derive(Debug, Clone)]
struct HumanForkReply {
    point: GomokuPoint,
    winning_points: Vec<GomokuPoint>,
}

impl CandidateReport {
    fn risk(&self) -> GomokuMoveRisk {
        if !self.human_next_wins.is_empty() {
            GomokuMoveRisk::ForcedLoss
        } else if !self.human_fork_replies.is_empty() {
            GomokuMoveRisk::AllowsHumanFork
        } else {
            GomokuMoveRisk::Safe
        }
    }

    fn role_label(&self) -> &'static str {
        match self.role {
            CandidateRole::WinNow => "win_now",
            CandidateRole::BlockImmediateWin => "block_immediate_win",
            CandidateRole::CreateFork => "create_fork",
            CandidateRole::BlockFork => "block_fork",
            CandidateRole::Positional => "positional",
        }
    }

    fn risk_label(&self) -> &'static str {
        match self.risk() {
            GomokuMoveRisk::Safe => "safe",
            GomokuMoveRisk::AllowsHumanSingleThreat => "allows_human_single_threat",
            GomokuMoveRisk::AllowsHumanFork => "allows_human_fork",
            GomokuMoveRisk::ForcedLoss => "forced_loss",
            GomokuMoveRisk::Unknown => "unknown",
        }
    }

    fn line_eval(&self) -> GomokuLineEval {
        if !self.ai_next_wins.is_empty() {
            GomokuLineEval::WhiteWin
        } else if !self.human_next_wins.is_empty() {
            GomokuLineEval::Losing
        } else if !self.human_fork_replies.is_empty() {
            GomokuLineEval::Dangerous
        } else if self.white_followup.is_some() {
            GomokuLineEval::Stable
        } else {
            GomokuLineEval::Unclear
        }
    }

    fn line_eval_label(&self) -> &'static str {
        match self.line_eval() {
            GomokuLineEval::WhiteWin => "white_win",
            GomokuLineEval::Stable => "stable",
            GomokuLineEval::Unclear => "unclear",
            GomokuLineEval::Dangerous => "dangerous",
            GomokuLineEval::Losing => "losing",
        }
    }
}

fn build_prompt(board: &[Vec<u8>], last_move: Option<GomokuPoint>) -> String {
    build_position_prompt(board, last_move, true)
}

fn build_commentary_prompt(board: &[Vec<u8>], last_move: Option<GomokuPoint>) -> String {
    build_position_prompt(board, last_move, false)
}

fn build_position_prompt(
    board: &[Vec<u8>],
    last_move: Option<GomokuPoint>,
    include_decision_guidance: bool,
) -> String {
    let board_lines = board
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| match cell {
                    &HUMAN => 'B',
                    &AI => 'W',
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
    let decision_candidates =
        decision_candidate_points(board, &ai_wins, &human_wins, &ai_forks, &human_forks);
    let candidate_section = if include_decision_guidance {
        format!(
            "Candidate consequence table:\n{}",
            format_candidate_reports(&candidate_reports(
                board,
                &decision_candidates,
                &ai_wins,
                &human_wins,
                &human_forks,
            ))
        )
    } else {
        format!(
            "Black recommendation consequence table:\n{}",
            format_black_recommendation_reports(&black_recommendation_reports(board))
        )
    };
    let commentary_recommendations = commentary_recommendation_points(board);
    let decision_guidance = if include_decision_guidance {
        "\nDecision guidance:\n\
1. Treat the tactical facts as exact legal candidate lists, not as commands.\n\
2. If White immediate winning points is not empty, usually choose the best one, but compare all listed choices.\n\
3. Otherwise, if Black immediate winning points is not empty, choose the strongest block from that list.\n\
4. Otherwise, compare White fork points and Black fork points; create a double threat when safe, or neutralize Black's strongest double-threat candidate.\n\
5. Use the candidate consequence table to avoid moves that allow an immediate Black win or a Black fork reply when safer candidates exist.\n\
6. Choose one empty point for White and set reason, risk, and line_summary consistently with the table.\n\
7. The output schema does not include lookahead_candidate, black_best_reply, white_followup, or line_eval. The app derives those fields from the selected x,y using this same candidate table."
    } else {
        "\nCommentary guidance:\n\
Explain the position for the human Black in Chinese. Choose recommendations by copying recommendation_id from the Black recommendation consequence table. Do not invent recommendation coordinates. Treat white_next_wins_after and white_forks_after as warnings against bad recommendations. Prefer urgent blocks when White has immediate wins or forks; prefer win/fork recommendations only when Black can force a threat. Use 黑棋 for the human and 白棋 for BitCat. Mention concrete threats only when visible in the facts or recommendation table."
    };

    format!(
        "{last}\n\
Board rows y=0..14, using . empty, B black/human, W white/BitCat:\n{board_lines}\n\
Current stones:\n\
- Black human stones: {}\n\
- White BitCat stones: {}\n\
Tactical facts:\n\
- White immediate winning points: {}\n\
- Black immediate winning points that must be blocked: {}\n\
- Top White fork points that create multiple next-turn wins: {}\n\
- Top Black fork points that must be occupied or neutralized: {}\n\
- Recommended candidate points near existing stones: {}\n\
- Decision candidate points covered by consequence table: {}\n\
- Human recommendation candidates for commentary: {}\n\
{}{}",
        format_points(&human_stones),
        format_points(&ai_stones),
        format_points(&ai_wins),
        format_points(&human_wins),
        format_points(&ai_forks),
        format_points(&human_forks),
        format_points(&candidates),
        format_points(&decision_candidates),
        format_points(&commentary_recommendations),
        candidate_section,
        decision_guidance,
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
        .take(MAX_POSITIONAL_CANDIDATES)
        .map(|(point, _)| point)
        .collect()
}

fn relevant_fork_points(points: &[GomokuPoint], limit: usize) -> Vec<GomokuPoint> {
    points.iter().copied().take(limit).collect()
}

fn push_unique(points: &mut Vec<GomokuPoint>, point: GomokuPoint) {
    if !points.contains(&point) {
        points.push(point);
    }
}

fn push_unique_limited(points: &mut Vec<GomokuPoint>, source: &[GomokuPoint], limit: usize) {
    for point in source.iter().copied().take(limit) {
        push_unique(points, point);
    }
}

fn decision_candidate_points(
    board: &[Vec<u8>],
    ai_wins: &[GomokuPoint],
    human_wins: &[GomokuPoint],
    ai_forks: &[GomokuPoint],
    human_forks: &[GomokuPoint],
) -> Vec<GomokuPoint> {
    let mut points = Vec::new();
    for point in ai_wins.iter().chain(human_wins).copied() {
        push_unique(&mut points, point);
    }
    push_unique_limited(&mut points, human_forks, MAX_FORK_CANDIDATES_PER_SIDE);
    push_unique_limited(&mut points, ai_forks, MAX_FORK_CANDIDATES_PER_SIDE);
    for point in candidate_points(board) {
        if points.len() >= MAX_DECISION_CANDIDATES {
            break;
        }
        push_unique(&mut points, point);
    }
    points
}

fn commentary_recommendation_points(board: &[Vec<u8>]) -> Vec<GomokuPoint> {
    let human_wins = immediate_winning_points(board, HUMAN);
    let ai_wins = immediate_winning_points(board, AI);
    let human_forks =
        relevant_fork_points(&fork_points(board, HUMAN), MAX_FORK_CANDIDATES_PER_SIDE);
    let ai_forks = relevant_fork_points(&fork_points(board, AI), MAX_FORK_CANDIDATES_PER_SIDE);
    let mut points = Vec::new();
    for point in human_wins.iter().chain(ai_wins.iter()).copied() {
        push_unique(&mut points, point);
    }
    push_unique_limited(&mut points, &human_forks, MAX_FORK_CANDIDATES_PER_SIDE);
    push_unique_limited(&mut points, &ai_forks, MAX_FORK_CANDIDATES_PER_SIDE);
    for point in candidate_points(board) {
        if points.len() >= MAX_COMMENTARY_RECOMMENDATIONS {
            break;
        }
        push_unique(&mut points, point);
    }
    points
}

fn black_recommendation_reports(board: &[Vec<u8>]) -> Vec<BlackRecommendationReport> {
    let human_wins = immediate_winning_points(board, HUMAN);
    let ai_wins = immediate_winning_points(board, AI);
    let human_forks =
        relevant_fork_points(&fork_points(board, HUMAN), MAX_FORK_CANDIDATES_PER_SIDE);
    let ai_forks = relevant_fork_points(&fork_points(board, AI), MAX_FORK_CANDIDATES_PER_SIDE);
    commentary_recommendation_points(board)
        .into_iter()
        .map(|point| {
            black_recommendation_report(
                board,
                point,
                &human_wins,
                &ai_wins,
                &human_forks,
                &ai_forks,
            )
        })
        .collect()
}

fn black_recommendation_report(
    board: &[Vec<u8>],
    point: GomokuPoint,
    human_wins: &[GomokuPoint],
    ai_wins: &[GomokuPoint],
    human_forks: &[GomokuPoint],
    ai_forks: &[GomokuPoint],
) -> BlackRecommendationReport {
    let mut next = board.to_vec();
    next[point.y][point.x] = HUMAN;
    let black_immediate_wins = immediate_winning_points(&next, HUMAN);
    let black_forks = fork_points(&next, HUMAN);
    let white_immediate_wins = immediate_winning_points(&next, AI);
    let white_forks = relevant_fork_points(&fork_points(&next, AI), MAX_FORK_CANDIDATES_PER_SIDE);
    let white_best_reply = strongest_reply(&next, AI);
    let purpose = if human_wins.contains(&point) {
        GomokuRecommendationReason::Win
    } else if ai_wins.contains(&point) || ai_forks.contains(&point) {
        GomokuRecommendationReason::Block
    } else if human_forks.contains(&point)
        || black_immediate_wins.len() >= 2
        || black_forks.len() >= 2
    {
        GomokuRecommendationReason::Fork
    } else if point_score(board, point, HUMAN) >= point_score(board, point, AI) {
        GomokuRecommendationReason::Extend
    } else {
        GomokuRecommendationReason::Stabilize
    };
    let priority = if human_wins.contains(&point) {
        GomokuRecommendationPriority::Best
    } else if ai_wins.contains(&point)
        || ai_forks.contains(&point)
        || !white_immediate_wins.is_empty()
    {
        GomokuRecommendationPriority::Urgent
    } else if matches!(purpose, GomokuRecommendationReason::Fork) {
        GomokuRecommendationPriority::Best
    } else {
        GomokuRecommendationPriority::Interesting
    };
    let line_eval = if human_wins.contains(&point) {
        GomokuLineEval::Losing
    } else if !white_immediate_wins.is_empty() {
        GomokuLineEval::WhiteWin
    } else if white_forks.len() >= 2 {
        GomokuLineEval::Dangerous
    } else if matches!(purpose, GomokuRecommendationReason::Fork) {
        GomokuLineEval::Stable
    } else {
        GomokuLineEval::Unclear
    };
    BlackRecommendationReport {
        point,
        purpose,
        priority,
        black_immediate_wins,
        black_forks,
        white_immediate_wins,
        white_forks,
        white_best_reply,
        line_eval,
        score_hint: point_score(board, point, HUMAN) * 2 + point_score(board, point, AI),
    }
}

fn candidate_reports(
    board: &[Vec<u8>],
    candidates: &[GomokuPoint],
    ai_wins: &[GomokuPoint],
    human_wins: &[GomokuPoint],
    human_forks: &[GomokuPoint],
) -> Vec<CandidateReport> {
    candidates
        .iter()
        .copied()
        .map(|point| candidate_report(board, point, ai_wins, human_wins, human_forks))
        .collect()
}

fn candidate_report(
    board: &[Vec<u8>],
    point: GomokuPoint,
    ai_wins: &[GomokuPoint],
    human_wins: &[GomokuPoint],
    human_forks: &[GomokuPoint],
) -> CandidateReport {
    let mut next = board.to_vec();
    next[point.y][point.x] = AI;
    let ai_next_wins = immediate_winning_points(&next, AI);
    let human_next_wins = immediate_winning_points(&next, HUMAN);
    let human_fork_replies = human_fork_replies(&next);
    let black_best_reply = strongest_reply(&next, HUMAN);
    let white_followup = black_best_reply.and_then(|reply| {
        let mut after_reply = next.clone();
        after_reply[reply.y][reply.x] = HUMAN;
        strongest_reply(&after_reply, AI)
    });
    let role = if ai_wins.contains(&point) {
        CandidateRole::WinNow
    } else if human_wins.contains(&point) {
        CandidateRole::BlockImmediateWin
    } else if ai_next_wins.len() >= 2 {
        CandidateRole::CreateFork
    } else if human_forks.contains(&point) {
        CandidateRole::BlockFork
    } else {
        CandidateRole::Positional
    };
    CandidateReport {
        point,
        score_hint: point_score(board, point, AI) * 2 + point_score(board, point, HUMAN),
        role,
        ai_next_wins,
        human_next_wins,
        human_fork_replies,
        black_best_reply,
        white_followup,
    }
}

fn strongest_reply(board: &[Vec<u8>], stone: u8) -> Option<GomokuPoint> {
    let wins = immediate_winning_points(board, stone);
    if let Some(point) = wins.first() {
        return Some(*point);
    }
    let forks = fork_points(board, stone);
    if let Some(point) = forks.first() {
        return Some(*point);
    }
    candidate_points(board).into_iter().max_by(|left, right| {
        point_score(board, *left, stone)
            .cmp(&point_score(board, *right, stone))
            .then_with(|| center_distance(*right).cmp(&center_distance(*left)))
            .then_with(|| right.y.cmp(&left.y))
            .then_with(|| right.x.cmp(&left.x))
    })
}

fn human_fork_replies(board_after_ai: &[Vec<u8>]) -> Vec<HumanForkReply> {
    let mut replies = empty_points(board_after_ai)
        .into_iter()
        .filter(|point| has_neighbor(board_after_ai, *point, 2))
        .filter_map(|point| {
            let mut next = board_after_ai.to_vec();
            next[point.y][point.x] = HUMAN;
            let winning_points = immediate_winning_points(&next, HUMAN);
            (winning_points.len() >= 2).then_some(HumanForkReply {
                point,
                winning_points,
            })
        })
        .collect::<Vec<_>>();
    replies.sort_by(|left, right| {
        right
            .winning_points
            .len()
            .cmp(&left.winning_points.len())
            .then_with(|| {
                point_score(board_after_ai, right.point, HUMAN).cmp(&point_score(
                    board_after_ai,
                    left.point,
                    HUMAN,
                ))
            })
            .then_with(|| center_distance(left.point).cmp(&center_distance(right.point)))
            .then_with(|| left.point.y.cmp(&right.point.y))
            .then_with(|| left.point.x.cmp(&right.point.x))
    });
    replies.truncate(4);
    replies
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

fn format_candidate_reports(reports: &[CandidateReport]) -> String {
    if reports.is_empty() {
        return "- none".into();
    }
    reports
        .iter()
        .enumerate()
        .map(|(index, report)| {
            let human_fork_replies = if report.human_fork_replies.is_empty() {
                "[]".into()
            } else {
                report
                    .human_fork_replies
                    .iter()
                    .map(|reply| {
                        format!(
                            "{{reply:{}, then_black_next_wins:{}}}",
                            format_coord(reply.point),
                            format_coord_list(&reply.winning_points)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!(
                "- Candidate {}: white_at={}, reason={}, risk={}, score_hint={}, white_next_wins={}, black_next_wins={}, black_fork_replies=[{}], black_best_reply={}, white_followup={}, line_eval={}",
                index + 1,
                format_coord(report.point),
                report.role_label(),
                report.risk_label(),
                report.score_hint,
                format_coord_list(&report.ai_next_wins),
                format_coord_list(&report.human_next_wins),
                human_fork_replies,
                format_optional_coord(report.black_best_reply),
                format_optional_coord(report.white_followup),
                report.line_eval_label(),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_black_recommendation_reports(reports: &[BlackRecommendationReport]) -> String {
    if reports.is_empty() {
        return "- none".into();
    }
    reports
        .iter()
        .enumerate()
        .map(|(index, report)| {
            format!(
                "- Recommendation {}: black_at={}, purpose={}, priority={}, score_hint={}, black_next_wins={}, black_forks={}, white_next_wins_after={}, white_forks_after={}, white_best_reply={}, line_eval_for_black={}",
                index + 1,
                format_coord(report.point),
                report.purpose_label(),
                report.priority_label(),
                report.score_hint,
                format_coord_list(&report.black_immediate_wins),
                format_coord_list(&report.black_forks),
                format_coord_list(&report.white_immediate_wins),
                format_coord_list(&report.white_forks),
                format_optional_coord(report.white_best_reply),
                report.line_eval_label(),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_coord(point: GomokuPoint) -> String {
    format!("[{},{}]", point.x, point.y)
}

fn format_coord_list(points: &[GomokuPoint]) -> String {
    if points.is_empty() {
        return "[]".into();
    }
    format!(
        "[{}]",
        points
            .iter()
            .map(|point| format_coord(*point))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn format_optional_coord(point: Option<GomokuPoint>) -> String {
    point.map(format_coord).unwrap_or_else(|| "null".into())
}

const GOMOKU_PREAMBLE: &str = "\
You are BitCat playing Gomoku as White against the human Black on a 15x15 board.
Coordinates are zero-based internally: x is column 0..14 from left to right, y is row 0..14 from top to bottom.
Choose exactly one empty cell. Five in a row wins horizontally, vertically, or diagonally.
Prefer legal tactical moves: win immediately if possible, otherwise block the human's immediate five, otherwise build your strongest line.
Trust the tactical facts in the user input. They are computed from the board and should override vague visual impressions.
For the structured candidate_id field, copy exactly one one-based Candidate number from the candidate consequence table. Do not invent coordinates as the selected move.
Set reason and risk by copying the exact values from the chosen candidate in the candidate consequence table, except use desperate_block only when every visible defensive move is losing.
The output schema does not include x, y, lookahead_candidate, black_best_reply, white_followup, or line_eval. Use candidate_id to choose the move; the app maps it to x,y and derives lookahead after validation.
If a candidate allows a Black fork reply or immediate Black win, do not label it safe. If all moves are losing, choose the best desperate block and label the risk honestly.
Use 黑棋 for the human and 白棋 for BitCat in message/thought.
message, thought, and line_summary are visible raw model text shown directly in a live game sidebar.
Write all visible text in Chinese. Do not use English.
Do not mention Candidate, candidate_id, score_hint, schema, table, enum, JSON, or internal field names in visible text.
message: one short Chinese sentence suitable for a HUD.
line_summary: one Chinese sentence explaining the main tactical judgment.
thought: two to four short Chinese sentences for the sidebar. Explain the move naturally, not as a candidate-table audit.
If a line has many stones, describe it as 这条横线, 这条纵线, or 黑棋连四 instead of listing the stones.
Do not list stone chains, candidate alternatives, scores, or multiple future points in visible text.
Do not write enum labels such as positional, safe, forced_loss, block_immediate_win, or create_fork in visible text.
When explaining a line, say 黑棋中路三连 or 纵向连四 instead of listing every stone coordinate.
Use words like 连四, 封堵, 延伸, 中腹, 先手, 稳住局面 instead of copying raw table rows.";
const GOMOKU_COMMENTARY_PREAMBLE: &str = "\
You are a calm Chinese Gomoku commentator for BitCat.
Use the exact board facts from the user input. Do not invent invisible threats.
Return concise Chinese commentary that makes the position more interesting for the human player.
For key_points.side, use only black, white, both, or none.
For key_points.kind, use only immediate_win, fork, block, extension, or shape.
For key_points.coord, use a zero-based [x,y] array from the board facts, or null when the fact has no single point.
For recommendations, include up to three empty points the human Black may consider next by copying recommendation_id from the Black recommendation consequence table. The app maps recommendation_id to coord and ignores invented recommendation coordinates.
For recommendations.priority, use only best, urgent, or interesting. For recommendations.reason, use only win, block, fork, extend, or stabilize.
For advantage, use only human, ai, or balanced. Never use black, white, both, or none in advantage.
Use 黑棋 for the human and 白棋 for BitCat in visible text.
The board facts use zero-based internal coordinates. Structured key_points.coord and recommendations.coord drive board highlights; visible text is shown as raw model text.
Avoid long analysis; this is a sidebar note, not a lesson.";
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
        assert!(prompt.contains(".......B......."));
        assert!(prompt.contains("Current stones:"));
    }

    #[test]
    fn prompt_lists_human_immediate_win_to_block() {
        let mut board = vec![vec![0u8; BOARD_SIZE]; BOARD_SIZE];
        for x in 3..7 {
            board[7][x] = HUMAN;
        }
        let prompt = build_prompt(&board, Some(GomokuPoint { x: 6, y: 7 }));
        assert!(prompt.contains("Black immediate winning points"));
        assert!(prompt.contains("[2,7]"));
        assert!(prompt.contains("[7,7]"));
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
        assert!(prompt.contains("Black fork points"));
        assert!(prompt.contains("[6,9]"));
    }

    #[test]
    fn candidate_table_exposes_recent_double_threat_after_ai_move() {
        let board = board_before(&[
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
            (6, 9, AI),
            (6, 5, HUMAN),
            (6, 4, AI),
            (9, 6, HUMAN),
            (10, 8, AI),
            (11, 8, HUMAN),
            (8, 9, AI),
            (8, 6, HUMAN),
        ]);
        let ai_wins = immediate_winning_points(&board, AI);
        let human_wins = immediate_winning_points(&board, HUMAN);
        let ai_forks = fork_points(&board, AI);
        let human_forks = fork_points(&board, HUMAN);
        let reports = candidate_reports(
            &board,
            &decision_candidate_points(&board, &ai_wins, &human_wins, &ai_forks, &human_forks),
            &ai_wins,
            &human_wins,
            &human_forks,
        );

        let selected = reports
            .iter()
            .find(|report| report.point == GomokuPoint { x: 7, y: 6 })
            .expect("recent AI move should be in candidate reports");
        assert_eq!(selected.risk(), GomokuMoveRisk::AllowsHumanFork);
        assert!(selected.human_fork_replies.iter().any(|reply| {
            reply.point == GomokuPoint { x: 9, y: 5 }
                && reply.winning_points.contains(&GomokuPoint { x: 10, y: 4 })
                && reply.winning_points.contains(&GomokuPoint { x: 5, y: 9 })
        }));
    }

    #[test]
    fn validation_rejects_safe_claim_for_forced_loss_block() {
        let board = board_before(&[
            (7, 7, HUMAN),
            (8, 7, AI),
            (6, 7, HUMAN),
            (9, 7, AI),
            (6, 6, HUMAN),
            (8, 8, AI),
            (7, 6, HUMAN),
            (8, 6, AI),
            (8, 5, HUMAN),
            (5, 8, AI),
            (9, 4, HUMAN),
            (10, 3, AI),
            (7, 5, HUMAN),
            (7, 8, AI),
            (6, 8, HUMAN),
            (6, 9, AI),
            (6, 5, HUMAN),
            (6, 4, AI),
            (5, 5, HUMAN),
        ]);
        let selected = GomokuAiMove {
            candidate_id: candidate_id_for_point(&board, GomokuPoint { x: 4, y: 5 })
                .expect("candidate id"),
            x: 4,
            y: 5,
            reason: GomokuMoveReason::BlockImmediateWin,
            risk: GomokuMoveRisk::Safe,
            blocked_immediate_wins: vec![[4, 5]],
            blocked_forks: vec![],
            lookahead_candidate: [4, 5],
            black_best_reply: None,
            white_followup: None,
            line_eval: GomokuLineEval::Stable,
            line_summary: "白棋先挡住黑棋冲五。".into(),
            message: None,
            thought: None,
        };

        let error = validate_ai_selection(&board, &selected).unwrap_err();
        assert!(error.contains("risk mismatch"));
    }

    #[test]
    fn commentary_rejects_recommendations_on_occupied_points() {
        let board = board_before(&[(7, 7, HUMAN), (8, 8, AI)]);
        let commentary = GomokuCommentary {
            summary: "黑棋需要先稳住中腹。".into(),
            advantage: GomokuAdvantage::Balanced,
            key_points: vec![],
            suggestion: None,
            recommendations: vec![GomokuRecommendation {
                recommendation_id: 1,
                coord: [7, 7],
                priority: GomokuRecommendationPriority::Urgent,
                reason: GomokuRecommendationReason::Block,
                text: "这里已经有黑棋，不能推荐为空位。".into(),
            }],
        };

        let error = validate_commentary(&board, &commentary).unwrap_err();
        assert!(error.contains("not empty"));
    }

    #[test]
    fn commentary_prompt_contains_black_recommendation_consequence_table() {
        let board = board_before(&[(7, 7, HUMAN), (8, 8, AI), (6, 7, HUMAN), (8, 7, AI)]);
        let prompt = build_commentary_prompt(&board, Some(GomokuPoint { x: 6, y: 7 }));
        assert!(prompt.contains("Black recommendation consequence table"));
        assert!(prompt.contains("Recommendation 1: black_at="));
        assert!(prompt.contains("white_best_reply="));
        assert!(prompt.contains("line_eval_for_black="));
    }

    #[test]
    fn commentary_completion_maps_recommendation_id_to_report() {
        let board = board_before(&[(7, 7, HUMAN), (8, 8, AI), (6, 7, HUMAN)]);
        let reports = black_recommendation_reports(&board);
        let report = reports.first().expect("recommendation report");
        let commentary = GomokuCommentary {
            summary: "黑棋需要看清下一手。".into(),
            advantage: GomokuAdvantage::Balanced,
            key_points: vec![],
            suggestion: Some("按推荐点处理。".into()),
            recommendations: vec![GomokuRecommendation {
                recommendation_id: 1,
                coord: [14, 14],
                priority: GomokuRecommendationPriority::Interesting,
                reason: GomokuRecommendationReason::Stabilize,
                text: "先走这个点。".into(),
            }],
        };

        let completed = complete_commentary(&board, commentary).expect("commentary completes");
        let recommendation = &completed.recommendations[0];
        assert_eq!(recommendation.coord, [report.point.x, report.point.y]);
        assert_eq!(recommendation.priority, report.priority);
        assert_eq!(recommendation.reason, report.purpose);
    }

    #[test]
    fn completion_overwrites_lookahead_from_candidate_table() {
        let board = board_before(&[(7, 7, HUMAN), (8, 8, AI), (6, 7, HUMAN)]);
        let ai_wins = immediate_winning_points(&board, AI);
        let human_wins = immediate_winning_points(&board, HUMAN);
        let ai_forks = fork_points(&board, AI);
        let human_forks = fork_points(&board, HUMAN);
        let decision_candidates =
            decision_candidate_points(&board, &ai_wins, &human_wins, &ai_forks, &human_forks);
        let point = decision_candidates
            .iter()
            .copied()
            .next()
            .expect("candidate");
        let candidate_id = decision_candidates
            .iter()
            .position(|candidate| *candidate == point)
            .map(|index| index + 1)
            .expect("candidate id");
        let report = candidate_reports(&board, &[point], &ai_wins, &human_wins, &human_forks)
            .into_iter()
            .next()
            .expect("report");
        let selected = GomokuAiMove {
            candidate_id,
            x: point.x,
            y: point.y,
            reason: match report.role {
                CandidateRole::WinNow => GomokuMoveReason::WinNow,
                CandidateRole::BlockImmediateWin => GomokuMoveReason::BlockImmediateWin,
                CandidateRole::CreateFork => GomokuMoveReason::CreateFork,
                CandidateRole::BlockFork => GomokuMoveReason::BlockFork,
                CandidateRole::Positional => GomokuMoveReason::Positional,
            },
            risk: report.risk(),
            blocked_immediate_wins: vec![],
            blocked_forks: vec![],
            lookahead_candidate: [point.x, point.y],
            black_best_reply: report.black_best_reply.map(|p| [p.x, p.y]),
            white_followup: report.white_followup.map(|p| [p.x, p.y]),
            line_eval: GomokuLineEval::Losing,
            line_summary: "模型这里即使填错，也以候选表为准。".into(),
            message: None,
            thought: None,
        };

        let completed =
            complete_ai_selection(&board, selected.into()).expect("selection should complete");
        assert_eq!(
            completed.lookahead_candidate,
            [report.point.x, report.point.y]
        );
        assert_eq!(
            completed.black_best_reply,
            report.black_best_reply.map(|point| [point.x, point.y])
        );
        assert_eq!(
            completed.white_followup,
            report.white_followup.map(|point| [point.x, point.y])
        );
        assert_eq!(completed.line_eval, report.line_eval());
    }

    #[test]
    fn late_game_prompt_prunes_noisy_fork_candidates() {
        let board = board_before(&[
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
        ]);
        let raw_ai_forks = fork_points(&board, AI);
        assert!(raw_ai_forks.len() > MAX_DECISION_CANDIDATES);

        let ai_wins = immediate_winning_points(&board, AI);
        let human_wins = immediate_winning_points(&board, HUMAN);
        let ai_forks = relevant_fork_points(&raw_ai_forks, MAX_FORK_CANDIDATES_PER_SIDE);
        let human_forks =
            relevant_fork_points(&fork_points(&board, HUMAN), MAX_FORK_CANDIDATES_PER_SIDE);
        let candidates =
            decision_candidate_points(&board, &ai_wins, &human_wins, &ai_forks, &human_forks);
        assert!(candidates.len() <= MAX_DECISION_CANDIDATES);

        let move_prompt = build_prompt(&board, Some(GomokuPoint { x: 10, y: 6 }));
        let commentary_prompt = build_commentary_prompt(&board, Some(GomokuPoint { x: 10, y: 6 }));
        assert!(move_prompt.contains("Top White fork points"));
        assert!(!commentary_prompt.contains("- White at"));
        assert!(commentary_prompt.len() < move_prompt.len());
    }

    fn board_before(moves: &[(usize, usize, u8)]) -> Vec<Vec<u8>> {
        let mut board = vec![vec![0u8; BOARD_SIZE]; BOARD_SIZE];
        for &(x, y, stone) in moves {
            board[y][x] = stone;
        }
        board
    }

    fn candidate_id_for_point(board: &[Vec<u8>], point: GomokuPoint) -> Option<usize> {
        let ai_wins = immediate_winning_points(board, AI);
        let human_wins = immediate_winning_points(board, HUMAN);
        let ai_forks = fork_points(board, AI);
        let human_forks = fork_points(board, HUMAN);
        decision_candidate_points(board, &ai_wins, &human_wins, &ai_forks, &human_forks)
            .iter()
            .position(|candidate| *candidate == point)
            .map(|index| index + 1)
    }
}
