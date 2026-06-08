//! Safe game-facing projections of user data.
//!
//! This module turns private app state into short, non-sensitive game targets.
//! Game engines should receive only these labels and counts, never raw memory,
//! reminder, screenshot, or agent session bodies.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const MAX_ITEMS: usize = 8;
const MAX_TITLE_CHARS: usize = 24;

/// Broad category for a projected game target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GameProjectionKind {
    Treat,
    MemoryShard,
    ReminderNote,
    AgentTask,
}

/// One safe target item available to a frontend mini-game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GameProjectionItem {
    pub id: String,
    pub kind: GameProjectionKind,
    pub title: String,
    pub weight: u32,
}

/// Safe projection payload consumed by games such as Invasion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GameProjection {
    pub version: u32,
    pub items: Vec<GameProjectionItem>,
}

impl GameProjection {
    /// Return a deterministic fallback projection for offline/demo play.
    pub fn fallback() -> Self {
        build_projection(&[
            (GameProjectionKind::Treat, "energy treat", 2),
            (GameProjectionKind::MemoryShard, "memory shard", 3),
            (GameProjectionKind::ReminderNote, "reminder note", 2),
            (GameProjectionKind::AgentTask, "agent task", 3),
            (GameProjectionKind::Treat, "focus snack", 1),
        ])
    }

    /// Build a projection from runtime labels that have already been selected
    /// by the app layer. Empty input falls back to deterministic demo targets.
    pub fn from_runtime_labels(items: Vec<(GameProjectionKind, String, u32)>) -> Self {
        if items.is_empty() {
            return Self::fallback();
        }
        let items = items
            .into_iter()
            .take(MAX_ITEMS)
            .enumerate()
            .map(|(index, (kind, title, weight))| GameProjectionItem {
                id: format!("target-{index}"),
                kind,
                title: safe_title(kind, &title),
                weight: weight.clamp(1, 5),
            })
            .collect();

        GameProjection { version: 1, items }
    }
}

/// Build a bounded projection from already-redacted labels.
pub fn build_projection(items: &[(GameProjectionKind, &str, u32)]) -> GameProjection {
    let items = items
        .iter()
        .take(MAX_ITEMS)
        .enumerate()
        .map(|(index, (kind, title, weight))| GameProjectionItem {
            id: format!("target-{index}"),
            kind: *kind,
            title: safe_title(*kind, title),
            weight: (*weight).clamp(1, 5),
        })
        .collect();

    GameProjection { version: 1, items }
}

fn safe_title(kind: GameProjectionKind, raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let fallback = match kind {
        GameProjectionKind::Treat => "treat",
        GameProjectionKind::MemoryShard => "memory shard",
        GameProjectionKind::ReminderNote => "reminder note",
        GameProjectionKind::AgentTask => "agent task",
    };
    let source = if cleaned.is_empty() {
        fallback
    } else {
        cleaned.as_str()
    };
    source.chars().take(MAX_TITLE_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_is_bounded_and_sanitized() {
        let source = vec![
            (
                GameProjectionKind::MemoryShard,
                "secret\nlong title with many words",
                99
            );
            12
        ];
        let projection = build_projection(&source);

        assert_eq!(projection.items.len(), MAX_ITEMS);
        assert_eq!(projection.items[0].weight, 5);
        assert!(!projection.items[0].title.contains('\n'));
        assert!(projection.items[0].title.chars().count() <= MAX_TITLE_CHARS);
    }

    #[test]
    fn empty_titles_get_kind_fallbacks() {
        let projection = build_projection(&[(GameProjectionKind::AgentTask, "  \t", 0)]);

        assert_eq!(projection.items[0].title, "agent task");
        assert_eq!(projection.items[0].weight, 1);
    }

    #[test]
    fn runtime_labels_fall_back_when_empty() {
        let projection = GameProjection::from_runtime_labels(Vec::new());

        assert!(projection.items.len() >= 4);
        assert!(
            projection
                .items
                .iter()
                .any(|item| item.kind == GameProjectionKind::MemoryShard)
        );
    }
}
