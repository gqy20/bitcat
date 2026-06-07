//! Game launch request bridge between core tools and the app runtime.
//!
//! The core crate cannot touch Tauri `AppHandle`, but the agent still needs a
//! typed way to ask the app layer to open built-in minigames. This module keeps
//! that boundary narrow by exposing a small request enum plus a one-time sender
//! injection API, mirroring the existing dance request bridge.
//!
//! The app crate injects a channel sender during startup and consumes requests
//! on its own async task. Core tools call `request_start_game()` and get a
//! regular `Result`, without learning anything about windows, IPC, or Tauri.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;

use crate::minigame::GameDef;

/// Built-in game kinds that the agent may request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StartGameKind {
    Snake,
    Memory,
    Catch,
    Battle,
    Gomoku,
    Arena,
    Beads,
}

/// Core -> app request payload for starting a built-in game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StartGameRequest {
    pub kind: StartGameKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_def: Option<GameDef>,
}

static START_GAME_TX: OnceLock<UnboundedSender<StartGameRequest>> = OnceLock::new();

/// Inject the app-owned sender once during startup.
pub fn set_start_game_sender(tx: UnboundedSender<StartGameRequest>) -> Result<(), String> {
    START_GAME_TX
        .set(tx)
        .map_err(|_| "game request sender already initialized".to_string())
}

/// Send a start-game request from core to app.
pub fn request_start_game(req: StartGameRequest) -> Result<(), String> {
    match START_GAME_TX.get() {
        Some(tx) => tx
            .send(req)
            .map_err(|e| format!("game request send failed: {e}")),
        None => {
            warn!("[game_request] START_GAME_TX not initialized");
            Err("game request channel not initialized".to_string())
        }
    }
}
