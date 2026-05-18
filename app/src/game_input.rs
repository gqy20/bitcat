//! Typed input bridge for the transparent mini-game window.
//!
//! This module keeps game-time controls separate from app-level actions such as
//! opening chat, taking screenshots, or launching programs. Rust producers send
//! a stable semantic payload to the frontend, while each game engine decides how
//! to interpret attack, skill, guard, direction, and lifecycle inputs.
//! Keeping this layer small avoids spreading ad-hoc JSON across gamepad code.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Semantic input consumed by `app/frontend/js/game_engine.js`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GameInput {
    /// Confirm a menu, start a ready game, or accept an end screen.
    Confirm,
    /// Cancel or leave the active game.
    Cancel,
    /// Toggle pause in games that support pausing.
    Pause,
    /// Hold-to-speed-up input for games that support boost.
    Boost { active: bool },
    /// Directional input from a d-pad, hat, arrow keys, or WASD.
    Direction { dx: i32, dy: i32 },
    /// Primary battle attack.
    AttackPrimary,
    /// Battle skill slot, starting at 1.
    Skill { slot: u8 },
    /// Battle guard/shield action.
    Guard,
    /// Battle target cycling or dodge direction.
    TargetCycle { dir: i32 },
}

/// Emit one typed game input event to the active game window.
pub fn emit_game_input(app: &AppHandle, input: GameInput) {
    if let Err(e) = app.emit("game-input", input) {
        tracing::warn!(error = %e, "emit game-input failed");
    }
}
