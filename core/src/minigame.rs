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
    Memory,
    Catch,
    Battle,
    Gomoku,
    Arena,
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

/// 桌宠守护战的宠物侧数值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BattlePetConfig {
    pub hp: u32,
    pub attack: u32,
    pub auto_attack_ms: u32,
}

/// 桌宠守护战的怪物数值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BattleMonsterConfig {
    pub id: String,
    pub name: String,
    pub hp: u32,
    pub attack: u32,
    pub attack_interval_ms: u32,
    pub reward_exp: u32,
}

/// 桌宠守护战的技能槽位。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BattleSkillConfig {
    pub id: String,
    pub name: String,
    pub cooldown_ms: u32,
    pub damage: u32,
    #[serde(default)]
    pub heal: u32,
}

/// 桌宠守护战配置；只承载安全边界，实时规则由前端引擎执行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BattleConfig {
    pub pet: BattlePetConfig,
    pub monster: BattleMonsterConfig,
    pub skills: Vec<BattleSkillConfig>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battle: Option<BattleConfig>,
}

impl GameDef {
    /// Phase 1 的内置 Snake 预设。
    pub fn default_snake() -> Self {
        Self {
            game_type: MinigameType::Snake,
            title: "毛线球大作战".into(),
            grid: GameGrid {
                width: 48,
                height: 32,
                cell_size: 16,
            },
            player: PlayerConfig {
                speed_ms: 95,
                initial_length: 5,
            },
            rules: GameRules {
                walls_kill: true,
                self_kill: true,
                food_count: 1,
                speed_ramp: 0.975,
                win_length: 140,
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
            battle: None,
        }
    }

    /// Built-in card-memory preset. It reuses the generic grid/rules fields so
    /// the app can launch it through the same `GameDef` path as Snake.
    pub fn default_memory() -> Self {
        Self {
            game_type: MinigameType::Memory,
            title: "Memory Match".into(),
            grid: GameGrid {
                width: 4,
                height: 4,
                cell_size: 96,
            },
            player: PlayerConfig {
                speed_ms: 140,
                initial_length: 3,
            },
            rules: GameRules {
                walls_kill: false,
                self_kill: false,
                food_count: 1,
                speed_ramp: 0.95,
                win_length: 16,
            },
            theme: GameTheme {
                head: "cat".into(),
                body: "yarn".into(),
                food: "fish".into(),
                trail_alpha: 0.55,
            },
            dialogue: GameDialogue {
                start: "Find every pair".into(),
                win: "All matched".into(),
                lose: "Try again".into(),
            },
            battle: None,
        }
    }

    /// Built-in falling-catch preset. `win_length` is the score target.
    pub fn default_catch() -> Self {
        Self {
            game_type: MinigameType::Catch,
            title: "Catch Treats".into(),
            grid: GameGrid {
                width: 24,
                height: 16,
                cell_size: 32,
            },
            player: PlayerConfig {
                speed_ms: 180,
                initial_length: 3,
            },
            rules: GameRules {
                walls_kill: false,
                self_kill: false,
                food_count: 1,
                speed_ramp: 0.97,
                win_length: 30,
            },
            theme: GameTheme {
                head: "cat".into(),
                body: "dot".into(),
                food: "fish".into(),
                trail_alpha: 0.55,
            },
            dialogue: GameDialogue {
                start: "Catch the treats".into(),
                win: "Nice catch".into(),
                lose: "Missed too many".into(),
            },
            battle: None,
        }
    }

    /// Built-in Gomoku preset. The frontend owns the board interaction, while
    /// Rust validates the game envelope and asks the AI for each response move.
    pub fn default_gomoku() -> Self {
        Self {
            game_type: MinigameType::Gomoku,
            title: "AI 五子棋".into(),
            grid: GameGrid {
                width: 15,
                height: 15,
                cell_size: 36,
            },
            player: PlayerConfig {
                speed_ms: 0,
                initial_length: 0,
            },
            rules: GameRules {
                walls_kill: false,
                self_kill: false,
                food_count: 0,
                speed_ramp: 1.0,
                win_length: 5,
            },
            theme: GameTheme {
                head: "cat".into(),
                body: "dot".into(),
                food: "fish".into(),
                trail_alpha: 0.55,
            },
            dialogue: GameDialogue {
                start: "和 BitCat 下五子棋".into(),
                win: "你连成五子".into(),
                lose: "BitCat 连成五子".into(),
            },
            battle: None,
        }
    }

    /// Built-in 3D arena fighting preset. The first implementation uses
    /// simple Three.js geometry while keeping combat rules data-driven.
    pub fn default_arena() -> Self {
        Self {
            game_type: MinigameType::Arena,
            title: "BitCat Arena".into(),
            grid: GameGrid {
                width: 24,
                height: 12,
                cell_size: 32,
            },
            player: PlayerConfig {
                speed_ms: 120,
                initial_length: 3,
            },
            rules: GameRules {
                walls_kill: false,
                self_kill: false,
                food_count: 0,
                speed_ramp: 1.0,
                win_length: 2,
            },
            theme: GameTheme {
                head: "cat".into(),
                body: "trail".into(),
                food: "fish".into(),
                trail_alpha: 0.55,
            },
            dialogue: GameDialogue {
                start: "训练场启动，准备开打。".into(),
                win: "漂亮！这拳有章法。".into(),
                lose: "先喘口气，下一局找回节奏。".into(),
            },
            battle: None,
        }
    }

    /// 内置守护召唤战预设。
    pub fn default_battle() -> Self {
        Self {
            game_type: MinigameType::Battle,
            title: "守护召唤战".into(),
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
                win_length: 50,
            },
            theme: GameTheme {
                head: "cat".into(),
                body: "yarn".into(),
                food: "mouse".into(),
                trail_alpha: 0.55,
            },
            dialogue: GameDialogue {
                start: "传送门打开了，帮我一起打！".into(),
                win: "赢啦！材料到手！".into(),
                lose: "呜...下次我会更强。".into(),
            },
            battle: Some(BattleConfig {
                pet: BattlePetConfig {
                    hp: 48,
                    attack: 1,
                    auto_attack_ms: 420,
                },
                monster: BattleMonsterConfig {
                    id: "intruder".into(),
                    name: "小史莱姆".into(),
                    hp: 10,
                    attack: 4,
                    attack_interval_ms: 1200,
                    reward_exp: 50,
                },
                skills: vec![
                    BattleSkillConfig {
                        id: "burst".into(),
                        name: "重击".into(),
                        cooldown_ms: 4500,
                        damage: 2,
                        heal: 0,
                    },
                    BattleSkillConfig {
                        id: "repair".into(),
                        name: "小鱼干".into(),
                        cooldown_ms: 7000,
                        damage: 0,
                        heal: 10,
                    },
                ],
            }),
        }
    }
}

/// 校验游戏定义，防止 AI 或配置给出会卡死/越界的参数。
pub fn validate_game_def(def: &GameDef) -> Result<(), String> {
    match def.game_type {
        MinigameType::Snake => validate_snake(def),
        MinigameType::Memory => validate_memory(def),
        MinigameType::Catch => validate_catch(def),
        MinigameType::Battle => validate_battle(def),
        MinigameType::Gomoku => validate_gomoku(def),
        MinigameType::Arena => validate_arena(def),
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
    ensure_range("rules.win_length", def.rules.win_length, 5, 500)?;

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

fn validate_memory(def: &GameDef) -> Result<(), String> {
    validate_common_game_fields(def)?;
    ensure_range("grid.width", def.grid.width, 2, 8)?;
    ensure_range("grid.height", def.grid.height, 2, 8)?;
    ensure_range("grid.cell_size", def.grid.cell_size, 24, 160)?;
    ensure_range("rules.win_length", def.rules.win_length, 4, 64)?;
    let cells = def.grid.width.saturating_mul(def.grid.height);
    if !cells.is_multiple_of(2) {
        return Err("memory grid must contain an even number of cells".into());
    }
    if def.rules.win_length != cells {
        return Err("memory win_length must match total grid cells".into());
    }
    Ok(())
}

fn validate_catch(def: &GameDef) -> Result<(), String> {
    validate_common_game_fields(def)?;
    ensure_range("grid.width", def.grid.width, 10, 80)?;
    ensure_range("grid.height", def.grid.height, 8, 60)?;
    ensure_range("grid.cell_size", def.grid.cell_size, 8, 48)?;
    ensure_range("player.speed_ms", def.player.speed_ms, 60, 1000)?;
    ensure_range("rules.win_length", def.rules.win_length, 5, 500)?;
    ensure_range("rules.food_count", def.rules.food_count, 1, 5)?;
    if !(0.70..=1.00).contains(&def.rules.speed_ramp) {
        return Err("rules.speed_ramp must be within 0.70..=1.00".into());
    }
    Ok(())
}

fn validate_gomoku(def: &GameDef) -> Result<(), String> {
    validate_common_game_fields(def)?;
    ensure_range("grid.width", def.grid.width, 9, 19)?;
    ensure_range("grid.height", def.grid.height, 9, 19)?;
    ensure_range("grid.cell_size", def.grid.cell_size, 16, 80)?;
    if def.grid.width != def.grid.height {
        return Err("gomoku board must be square".into());
    }
    if def.rules.win_length != 5 {
        return Err("gomoku win_length must be 5".into());
    }
    Ok(())
}

fn validate_arena(def: &GameDef) -> Result<(), String> {
    validate_common_game_fields(def)?;
    ensure_range("grid.width", def.grid.width, 12, 40)?;
    ensure_range("grid.height", def.grid.height, 6, 24)?;
    ensure_range("grid.cell_size", def.grid.cell_size, 16, 80)?;
    ensure_range("player.speed_ms", def.player.speed_ms, 60, 300)?;
    if def.rules.win_length == 0 || def.rules.win_length > 5 {
        return Err("arena win_length must be within 1..=5 rounds".into());
    }
    if def.rules.food_count != 0 {
        return Err("arena food_count must be 0".into());
    }
    if (def.rules.speed_ramp - 1.0).abs() > f32::EPSILON {
        return Err("arena speed_ramp must be 1.0".into());
    }
    Ok(())
}

fn validate_common_game_fields(def: &GameDef) -> Result<(), String> {
    if def.title.trim().is_empty() {
        return Err("game title must not be empty".into());
    }
    if def.title.chars().count() > 40 {
        return Err("game title must be at most 40 chars".into());
    }
    ensure_choice("theme.head", &def.theme.head, &["cat", "yarn", "light"])?;
    ensure_choice("theme.body", &def.theme.body, &["yarn", "dot", "trail"])?;
    ensure_choice(
        "theme.food",
        &def.theme.food,
        &["mouse", "fish", "butterfly"],
    )?;
    if !(0.0..=1.0).contains(&def.theme.trail_alpha) {
        return Err("theme.trail_alpha must be within 0.0..=1.0".into());
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

fn validate_battle(def: &GameDef) -> Result<(), String> {
    if def.title.trim().is_empty() {
        return Err("battle title must not be empty".into());
    }
    if def.title.chars().count() > 40 {
        return Err("battle title must be at most 40 chars".into());
    }

    let battle = def
        .battle
        .as_ref()
        .ok_or_else(|| "battle config is required".to_string())?;

    ensure_range("battle.pet.hp", battle.pet.hp, 1, 999)?;
    ensure_range("battle.pet.attack", battle.pet.attack, 1, 99)?;
    ensure_range(
        "battle.pet.auto_attack_ms",
        battle.pet.auto_attack_ms,
        300,
        10_000,
    )?;

    if battle.monster.id.trim().is_empty() || battle.monster.name.trim().is_empty() {
        return Err("battle monster id/name must not be empty".into());
    }
    ensure_range("battle.monster.hp", battle.monster.hp, 1, 999)?;
    ensure_range("battle.monster.attack", battle.monster.attack, 0, 99)?;
    ensure_range(
        "battle.monster.attack_interval_ms",
        battle.monster.attack_interval_ms,
        500,
        20_000,
    )?;
    ensure_range(
        "battle.monster.reward_exp",
        battle.monster.reward_exp,
        1,
        999,
    )?;

    if battle.skills.len() > 4 {
        return Err("battle supports at most 4 skills".into());
    }
    for skill in &battle.skills {
        if skill.id.trim().is_empty() || skill.name.trim().is_empty() {
            return Err("battle skill id/name must not be empty".into());
        }
        ensure_range("battle.skill.cooldown_ms", skill.cooldown_ms, 500, 60_000)?;
        if skill.damage == 0 && skill.heal == 0 {
            return Err("battle skill must deal damage or heal".into());
        }
        ensure_range("battle.skill.damage", skill.damage, 0, 999)?;
        ensure_range("battle.skill.heal", skill.heal, 0, 999)?;
    }

    Ok(())
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
    fn default_battle_is_valid() {
        assert!(validate_game_def(&GameDef::default_battle()).is_ok());
    }

    #[test]
    fn default_memory_is_valid() {
        assert!(validate_game_def(&GameDef::default_memory()).is_ok());
    }

    #[test]
    fn default_catch_is_valid() {
        assert!(validate_game_def(&GameDef::default_catch()).is_ok());
    }

    #[test]
    fn default_gomoku_is_valid() {
        assert!(validate_game_def(&GameDef::default_gomoku()).is_ok());
    }

    #[test]
    fn default_arena_is_valid() {
        assert!(validate_game_def(&GameDef::default_arena()).is_ok());
    }

    #[test]
    fn battle_requires_config() {
        let mut def = GameDef::default_snake();
        def.game_type = MinigameType::Battle;
        assert!(validate_game_def(&def).is_err());
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
        assert!(invalid(|d| d.rules.win_length = 501).is_err());
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
