//! 迷你游戏定义与校验逻辑。
//!
//! 本模块只描述游戏配置和安全边界，不依赖 Tauri 或前端渲染实现。
//! app 层读取这些结构创建游戏窗口，前端根据同一份 `GameDef` 初始化具体引擎。
//! 当前 Phase 1 只支持 Snake，后续 Memory / Catch 和 AI 生成会复用这里的 schema。

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 当前支持的迷你游戏类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MinigameType {
    Snake,
}

/// 网格尺寸配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GameGrid {
    pub width: u32,
    pub height: u32,
    pub cell_size: u32,
}

/// 玩家/蛇的初始参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlayerConfig {
    pub speed_ms: u32,
    pub initial_length: u32,
}

/// 游戏规则参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GameRules {
    pub walls_kill: bool,
    pub self_kill: bool,
    pub food_count: u32,
    pub speed_ramp: f32,
    pub win_length: u32,
}

/// 主题渲染参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GameTheme {
    pub head: String,
    pub body: String,
    pub food: String,
    pub trail_alpha: f32,
}

/// 游戏开始/结束时的对话文案。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GameDialogue {
    pub start: String,
    pub win: String,
    pub lose: String,
}

/// 迷你游戏完整配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GameDef {
    pub game_type: MinigameType,
    pub title: String,
    pub grid: GameGrid,
    pub player: PlayerConfig,
    pub rules: GameRules,
    pub theme: GameTheme,
    pub dialogue: GameDialogue,
}

impl GameDef {
    /// Phase 1 的内置 Snake 预设。
    pub fn default_snake() -> Self {
        Self {
            game_type: MinigameType::Snake,
            title: "毛线球大作战".into(),
            grid: GameGrid {
                width: 30,
                height: 20,
                cell_size: 24,
            },
            player: PlayerConfig {
                speed_ms: 140,
                initial_length: 3,
            },
            rules: GameRules {
                walls_kill: true,
                self_kill: true,
                food_count: 1,
                speed_ramp: 0.95,
                win_length: 20,
            },
            theme: GameTheme {
                head: "cat".into(),
                body: "yarn".into(),
                food: "mouse".into(),
                trail_alpha: 0.55,
            },
            dialogue: GameDialogue {
                start: "喵！看我的！".into(),
                win: "太厉害了喵~".into(),
                lose: "呜...再来一次！".into(),
            },
        }
    }
}

/// 校验游戏定义，防止 AI 或配置给出会卡死/越界的参数。
pub fn validate_game_def(def: &GameDef) -> Result<(), String> {
    match def.game_type {
        MinigameType::Snake => validate_snake(def),
    }
}

fn validate_snake(def: &GameDef) -> Result<(), String> {
    if def.title.trim().is_empty() {
        return Err("游戏标题不能为空".into());
    }
    if def.title.chars().count() > 40 {
        return Err("游戏标题最多 40 个字符".into());
    }

    ensure_range("grid.width", def.grid.width, 10, 80)?;
    ensure_range("grid.height", def.grid.height, 8, 60)?;
    ensure_range("grid.cell_size", def.grid.cell_size, 8, 48)?;
    ensure_range("player.speed_ms", def.player.speed_ms, 60, 1000)?;
    ensure_range("player.initial_length", def.player.initial_length, 1, 10)?;
    ensure_range("rules.win_length", def.rules.win_length, 5, 200)?;

    if def.rules.food_count != 1 {
        return Err("Phase 1 仅支持 food_count = 1".into());
    }
    if !(0.70..=1.00).contains(&def.rules.speed_ramp) {
        return Err("rules.speed_ramp 必须在 0.70..=1.00 之间".into());
    }
    if def.player.initial_length >= def.rules.win_length {
        return Err("player.initial_length 必须小于 rules.win_length".into());
    }

    let cells = def.grid.width.saturating_mul(def.grid.height);
    if def.rules.win_length >= cells {
        return Err("rules.win_length 必须小于网格总格子数".into());
    }

    ensure_choice("theme.head", &def.theme.head, &["cat", "yarn", "light"])?;
    ensure_choice("theme.body", &def.theme.body, &["yarn", "dot", "trail"])?;
    ensure_choice(
        "theme.food",
        &def.theme.food,
        &["mouse", "fish", "butterfly"],
    )?;
    if !(0.0..=1.0).contains(&def.theme.trail_alpha) {
        return Err("theme.trail_alpha 必须在 0.0..=1.0 之间".into());
    }

    Ok(())
}

fn ensure_range(name: &str, value: u32, min: u32, max: u32) -> Result<(), String> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(format!("{name} 必须在 {min}..={max} 之间"))
    }
}

fn ensure_choice(name: &str, value: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!("{name} 不支持: {value}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid(mut update: impl FnMut(&mut GameDef)) -> Result<(), String> {
        let mut def = GameDef::default_snake();
        update(&mut def);
        validate_game_def(&def)
    }

    #[test]
    fn default_snake_is_valid() {
        assert!(validate_game_def(&GameDef::default_snake()).is_ok());
    }

    #[test]
    fn rejects_grid_bounds() {
        assert!(invalid(|d| d.grid.width = 9).is_err());
        assert!(invalid(|d| d.grid.height = 61).is_err());
        assert!(invalid(|d| d.grid.cell_size = 7).is_err());
    }

    #[test]
    fn rejects_player_bounds() {
        assert!(invalid(|d| d.player.speed_ms = 59).is_err());
        assert!(invalid(|d| d.player.speed_ms = 1001).is_err());
        assert!(invalid(|d| d.player.initial_length = 0).is_err());
        assert!(invalid(|d| d.player.initial_length = 11).is_err());
    }

    #[test]
    fn rejects_rule_bounds() {
        assert!(invalid(|d| d.rules.win_length = 4).is_err());
        assert!(invalid(|d| d.rules.win_length = 201).is_err());
        assert!(invalid(|d| d.rules.speed_ramp = 0.69).is_err());
        assert!(invalid(|d| d.rules.speed_ramp = 1.01).is_err());
        assert!(invalid(|d| d.rules.food_count = 2).is_err());
    }

    #[test]
    fn rejects_impossible_lengths() {
        assert!(
            invalid(|d| {
                d.player.initial_length = 10;
                d.rules.win_length = 10;
            })
            .is_err()
        );
        assert!(
            invalid(|d| {
                d.grid.width = 10;
                d.grid.height = 8;
                d.rules.win_length = 80;
            })
            .is_err()
        );
    }

    #[test]
    fn rejects_unknown_theme_values() {
        assert!(invalid(|d| d.theme.head = "dog".into()).is_err());
        assert!(invalid(|d| d.theme.body = "rope".into()).is_err());
        assert!(invalid(|d| d.theme.food = "star".into()).is_err());
        assert!(invalid(|d| d.theme.trail_alpha = 1.1).is_err());
    }
}
