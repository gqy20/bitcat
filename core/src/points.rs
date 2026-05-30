//! 互动积分与成就系统
//!
//! 追踪用户与 BitCat 宠物的各类互动行为（对话、游戏、记忆、提醒等），
//! 将其转化为积分、等级和成就徽章。纯本地统计，不上传、不同步。
//!
//! 数据持久化到 ~/.bitcat/logs/points_events.jsonl（追加明细）和
//! ~/.bitcat/points_state.json（聚合状态原子写入），供设置页展示。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tracing::{debug, info, warn};

// ─── 事件类型与分值 ──────────────────────────────────────────────

/// 所有可追踪的用户互动事件类型，每种类型对应固定分值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PointsEventKind {
    /// AI 对话完成（文字输入）
    ChatCompleted,
    /// AI 对话完成（语音输入）
    VoiceChat,
    /// 长期记忆创建成功
    MemoryCreated,
    /// 创建新提醒
    ReminderCreated,
    /// 完成提醒
    ReminderCompleted,
    /// 观看舞蹈表演
    DancePerformed,
    /// 开始一局游戏
    GamePlayed,
    /// 游戏胜利
    GameWon,
    /// 截图观察分析完成
    ScreenshotObserved,
    /// 摄像头观察分析完成
    CameraObserved,
    /// 夸奖宠物（A 键）
    PetPraised,
    /// 每日首次启动登录
    DailyLogin,
}

impl PointsEventKind {
    /// 返回该事件类型的固定分值。
    pub fn point_value(self) -> u32 {
        match self {
            Self::ChatCompleted => 10,
            Self::VoiceChat => 15,
            Self::MemoryCreated => 20,
            Self::ReminderCreated => 8,
            Self::ReminderCompleted => 12,
            Self::DancePerformed => 3,
            Self::GamePlayed => 2,
            Self::GameWon => 15,
            Self::ScreenshotObserved => 4,
            Self::CameraObserved => 4,
            Self::PetPraised => 1,
            Self::DailyLogin => 25,
        }
    }

    /// 用户界面显示用的中文标签。
    pub fn label(self) -> &'static str {
        match self {
            Self::ChatCompleted => "对话完成",
            Self::VoiceChat => "语音对话",
            Self::MemoryCreated => "记忆创建",
            Self::ReminderCreated => "创建提醒",
            Self::ReminderCompleted => "完成提醒",
            Self::DancePerformed => "观看舞蹈",
            Self::GamePlayed => "游戏一局",
            Self::GameWon => "游戏胜利",
            Self::ScreenshotObserved => "截图观察",
            Self::CameraObserved => "摄像头观察",
            Self::PetPraised => "夸奖宠物",
            Self::DailyLogin => "每日登录",
        }
    }

    /// 归属的统计分类组，用于前端分类汇总条形图。
    pub fn category(self) -> PointsCategory {
        match self {
            Self::ChatCompleted | Self::VoiceChat => PointsCategory::Chat,
            Self::MemoryCreated => PointsCategory::Memory,
            Self::ReminderCreated | Self::ReminderCompleted => PointsCategory::Routine,
            Self::DancePerformed | Self::GamePlayed | Self::GameWon => PointsCategory::Fun,
            Self::ScreenshotObserved | Self::CameraObserved => PointsCategory::Observation,
            Self::PetPraised => PointsCategory::Bond,
            Self::DailyLogin => PointsCategory::Daily,
        }
    }
}

/// 积分事件的统计分组类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PointsCategory {
    /// 聊天对话（含语音）
    Chat,
    /// 长期记忆
    Memory,
    /// 提醒管理
    Routine,
    /// 娱乐互动（舞蹈、游戏）
    Fun,
    /// 观察系统（截图、摄像头）
    Observation,
    /// 宠物亲密度
    Bond,
    /// 每日活跃
    Daily,
}

impl PointsCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Chat => "聊天",
            Self::Memory => "记忆",
            Self::Routine => "日常",
            Self::Fun => "娱乐",
            Self::Observation => "观察",
            Self::Bond => "互动",
            Self::Daily => "每日",
        }
    }
}

// ─── 事件记录（JSONL 明细）───────────────────────────────────────

/// 单条积分事件记录，追加写入 JSONL 文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointsEventRecord {
    pub timestamp: String,
    #[serde(rename = "event_kind")]
    pub kind: PointsEventKind,
    #[serde(rename = "points_awarded")]
    pub points: u32,
    pub extra: Option<String>,
}

impl PointsEventRecord {
    pub fn new(kind: PointsEventKind, extra: Option<&str>) -> Self {
        Self {
            timestamp: chrono::Local::now().to_rfc3339(),
            kind,
            points: kind.point_value(),
            extra: extra.map(|s| s.to_string()),
        }
    }
}

// ─── 聚合状态（原子 JSON）────────────────────────────────────────

/// 各维度的事件计数器。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointsCategories {
    pub chats: u64,
    pub voice_chats: u64,
    pub memories: u64,
    pub reminders_created: u64,
    pub reminders_completed: u64,
    pub dances: u64,
    pub games_played: u64,
    pub games_won: u64,
    pub screenshots: u64,
    pub camera_obs: u64,
    pub praises: u64,
    pub login_days: u64,
}

impl PointsCategories {
    fn increment(&mut self, kind: PointsEventKind) {
        match kind {
            PointsEventKind::ChatCompleted => self.chats += 1,
            PointsEventKind::VoiceChat => self.voice_chats += 1,
            PointsEventKind::MemoryCreated => self.memories += 1,
            PointsEventKind::ReminderCreated => self.reminders_created += 1,
            PointsEventKind::ReminderCompleted => self.reminders_completed += 1,
            PointsEventKind::DancePerformed => self.dances += 1,
            PointsEventKind::GamePlayed => self.games_played += 1,
            PointsEventKind::GameWon => self.games_won += 1,
            PointsEventKind::ScreenshotObserved => self.screenshots += 1,
            PointsEventKind::CameraObserved => self.camera_obs += 1,
            PointsEventKind::PetPraised => self.praises += 1,
            PointsEventKind::DailyLogin => self.login_days += 1,
        }
    }
}

/// 积分聚合状态，每次 award 后原子写入磁盘。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointsState {
    pub total_points: u64,
    pub level: u32,
    #[serde(rename = "level_title")]
    pub level_title: String,
    #[serde(rename = "exp_in_current")]
    pub experience_in_current: u64,
    #[serde(rename = "exp_to_next")]
    pub experience_to_next: u64,
    /// 各分类组的累计积分（用于前端条形图）。
    #[serde(rename = "category_totals")]
    pub category_totals: HashMap<PointsCategory, u64>,
    /// 各维度事件计数（用于成就条件判断）。
    pub categories: PointsCategories,
    /// 当前连续活跃天数。
    #[serde(rename = "current_streak")]
    pub current_streak_days: u32,
    /// 历史最长连续天数。
    #[serde(rename = "longest_streak")]
    pub longest_streak_days: u32,
    /// 上次活跃日期（YYYY-MM-DD），用于连续天数判断。
    #[serde(rename = "last_active_date")]
    pub last_active_date: Option<String>,
    /// 已解锁成就 ID 列表。
    pub achievements: Vec<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: String,
}

impl Default for PointsState {
    fn default() -> Self {
        let (level, title, exp_in, exp_next) = calculate_level(0);
        Self {
            total_points: 0,
            level,
            level_title: title.to_string(),
            experience_in_current: exp_in,
            experience_to_next: exp_next,
            category_totals: HashMap::new(),
            categories: PointsCategories::default(),
            current_streak_days: 0,
            longest_streak_days: 0,
            last_active_date: None,
            achievements: Vec::new(),
            updated_at: chrono::Local::now().to_rfc3339(),
        }
    }
}

// ─── 等级体系 ───────────────────────────────────────────────────

/// 等级阈值表：索引 = 等级 - 1，值 = 所需总积分。
const LEVEL_THRESHOLDS: &[u64] = &[
    0,    // Lv1  初识
    50,   // Lv2  熟悉
    150,  // Lv3  伙伴
    400,  // Lv4  好友
    800,  // Lv5  知己
    1500, // Lv6  羁绊
    3000, // Lv7  灵犀
    6000, // Lv8  共生
];

const LEVEL_TITLES: &[&str] = &[
    "初识", // Lv1
    "熟悉", // Lv2
    "伙伴", // Lv3
    "好友", // Lv4
    "知己", // Lv5
    "羁绊", // Lv6
    "灵犀", // Lv7
    "共生", // Lv8
];

#[allow(dead_code)]
const LEVEL_ICONS: &[&str] = &[
    "\u{1F331}", // 🌱
    "\u{1F431}", // 🐱
    "\u{1F380}", // 🎀
    "\u{2B50}",  // ⭐
    "\u{1F48E}", // 💎
    "\u{1F31F}", // 🌟
    "\u{1F451}", // 👑
    "\u{1F3C6}", // 🏆
];

/// 根据总积分计算等级信息。
///
/// 返回 `(等级, 称号, 当前等级已获经验, 升至下一级所需经验)`。
/// 满级时 `exp_to_next == exp_in_current`，前端进度条显示 100%。
pub fn calculate_level(total_points: u64) -> (u32, &'static str, u64, u64) {
    let mut level = 1u32;
    for (i, &threshold) in LEVEL_THRESHOLDS.iter().enumerate() {
        if total_points >= threshold {
            level = (i + 1) as u32;
        } else {
            break;
        }
    }
    let level_idx = (level.saturating_sub(1)) as usize;
    let title = LEVEL_TITLES.get(level_idx).copied().unwrap_or("未知");

    // 当前等级的起始阈值
    let level_start = LEVEL_THRESHOLDS[level_idx];
    // 已在当前等级内获得的积分
    let exp_in_current = total_points.saturating_sub(level_start);

    let exp_to_next = if level_idx < LEVEL_THRESHOLDS.len() - 1 {
        // 升到下一级还需多少分（下一级阈值 - 当前等级起点）
        LEVEL_THRESHOLDS[level_idx + 1] - level_start
    } else {
        // 已满级：让进度条始终显示 100%
        exp_in_current.max(1) // 避免 0/0
    };

    (level, title, exp_in_current, exp_to_next)
}

// ─── 成就定义 ───────────────────────────────────────────────────

/// 成就解锁条件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AchievementCondition {
    /// 指定事件类型达到目标次数。
    #[serde(rename = "event_count")]
    EventCount { kind: PointsEventKind, target: u32 },
    /// 总积分达到目标值。
    #[serde(rename = "total_points")]
    TotalPoints { target: u64 },
    /// 达到目标等级。
    #[serde(rename = "level_reached")]
    LevelReached { target: u32 },
    /// 连续活跃天数达到目标。
    #[serde(rename = "streak_days")]
    StreakDays { target: u32 },
}

/// 静态成就定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchievementDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub condition: AchievementCondition,
    /// 解锁时额外奖励的积分。
    #[serde(rename = "points_reward")]
    pub bonus_points: u32,
    /// 未解锁时是否隐藏描述（显示 "?"）。
    pub hidden: bool,
}

/// 所有成就定义，按 ID 索引。
pub static ALL_ACHIEVEMENTS: OnceLock<Vec<AchievementDef>> = OnceLock::new();

/// 初始化并返回所有成就定义。
pub fn all_achievements() -> &'static Vec<AchievementDef> {
    ALL_ACHIEVEMENTS.get_or_init(|| {
        vec![
            AchievementDef {
                id: "first_chat".into(),
                name: "初次对话".into(),
                description: "完成第一次 AI 对话".into(),
                icon: "\u{1F4AC}".into(), // 💬
                condition: AchievementCondition::EventCount {
                    kind: PointsEventKind::ChatCompleted,
                    target: 1,
                },
                bonus_points: 10,
                hidden: false,
            },
            AchievementDef {
                id: "chatterbox".into(),
                name: "话痨".into(),
                description: "累计完成 50 次对话".into(),
                icon: "\u{1F4E3}".into(), // 📣
                condition: AchievementCondition::EventCount {
                    kind: PointsEventKind::ChatCompleted,
                    target: 50,
                },
                bonus_points: 25,
                hidden: false,
            },
            AchievementDef {
                id: "memory_keeper".into(),
                name: "记忆守护".into(),
                description: "积累 10 条长期记忆".into(),
                icon: "\u{1F4BE}".into(), // 💾
                condition: AchievementCondition::EventCount {
                    kind: PointsEventKind::MemoryCreated,
                    target: 10,
                },
                bonus_points: 30,
                hidden: false,
            },
            AchievementDef {
                id: "reminder_master".into(),
                name: "提醒达人".into(),
                description: "完成 20 个提醒".into(),
                icon: "\u{23F0}".into(), // ⏰
                condition: AchievementCondition::EventCount {
                    kind: PointsEventKind::ReminderCompleted,
                    target: 20,
                },
                bonus_points: 35,
                hidden: false,
            },
            AchievementDef {
                id: "game_veteran".into(),
                name: "游戏老手".into(),
                description: "玩了 10 局游戏".into(),
                icon: "\u{1F3AE}".into(), // 🎮
                condition: AchievementCondition::EventCount {
                    kind: PointsEventKind::GamePlayed,
                    target: 10,
                },
                bonus_points: 20,
                hidden: false,
            },
            AchievementDef {
                id: "champion".into(),
                name: "冠军".into(),
                description: "赢得 5 场游戏胜利".into(),
                icon: "\u{1F3C6}".into(), // 🏆
                condition: AchievementCondition::EventCount {
                    kind: PointsEventKind::GameWon,
                    target: 5,
                },
                bonus_points: 40,
                hidden: false,
            },
            AchievementDef {
                id: "dance_fan".into(),
                name: "舞蹈爱好者".into(),
                description: "观看 20 次舞蹈表演".into(),
                icon: "\u{1F483}".into(), // 💃
                condition: AchievementCondition::EventCount {
                    kind: PointsEventKind::DancePerformed,
                    target: 20,
                },
                bonus_points: 25,
                hidden: false,
            },
            AchievementDef {
                id: "observer".into(),
                name: "观察者".into(),
                description: "完成 30 次截图观察".into(),
                icon: "\u{1F441}".into(), // 👁
                condition: AchievementCondition::EventCount {
                    kind: PointsEventKind::ScreenshotObserved,
                    target: 30,
                },
                bonus_points: 30,
                hidden: false,
            },
            AchievementDef {
                id: "streak_7".into(),
                name: "七日连续".into(),
                description: "连续 7 天使用 BitCat".into(),
                icon: "\u{1F525}".into(), // 🔥
                condition: AchievementCondition::StreakDays { target: 7 },
                bonus_points: 50,
                hidden: false,
            },
            AchievementDef {
                id: "streak_30".into(),
                name: "月度常客".into(),
                description: "连续 30 天使用 BitCat".into(),
                icon: "\u{1F4CA}".into(), // 📊
                condition: AchievementCondition::StreakDays { target: 30 },
                bonus_points: 100,
                hidden: true,
            },
            AchievementDef {
                id: "voice_explorer".into(),
                name: "声控玩家".into(),
                description: "使用 10 次语音对话".into(),
                icon: "\u{1F3A4}".into(), // 🎤
                condition: AchievementCondition::EventCount {
                    kind: PointsEventKind::VoiceChat,
                    target: 10,
                },
                bonus_points: 30,
                hidden: false,
            },
            AchievementDef {
                id: "level_max".into(),
                name: "满级羁绊".into(),
                description: "达到最高等级（共生）".into(),
                icon: "\u{1F451}".into(), // 👑
                condition: AchievementCondition::LevelReached { target: 8 },
                bonus_points: 100,
                hidden: true,
            },
        ]
    })
}

/// 给前端使用的成就视图（包含动态解锁状态）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchievementView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    #[serde(rename = "points_reward")]
    pub bonus_points: u32,
    pub hidden: bool,
    pub unlocked: bool,
    #[serde(rename = "unlocked_at", skip_serializing_if = "Option::is_none")]
    pub unlocked_at: Option<String>,
}

/// 从静态定义生成完整视图列表，标注每个成就的解锁状态。
pub fn achievement_views(state: &PointsState) -> Vec<AchievementView> {
    all_achievements()
        .iter()
        .map(|a| {
            let unlocked = state.achievements.contains(&a.id);
            AchievementView {
                id: a.id.clone(),
                name: a.name.clone(),
                description: a.description.clone(),
                icon: a.icon.clone(),
                bonus_points: a.bonus_points,
                hidden: a.hidden,
                unlocked,
                unlocked_at: if unlocked {
                    Some(state.updated_at.clone())
                } else {
                    None
                },
            }
        })
        .collect()
}

fn is_condition_met(condition: &AchievementCondition, state: &PointsState) -> bool {
    match condition {
        AchievementCondition::EventCount { kind, target } => {
            let count = match kind {
                PointsEventKind::ChatCompleted => state.categories.chats,
                PointsEventKind::VoiceChat => state.categories.voice_chats,
                PointsEventKind::MemoryCreated => state.categories.memories,
                PointsEventKind::ReminderCreated => state.categories.reminders_created,
                PointsEventKind::ReminderCompleted => state.categories.reminders_completed,
                PointsEventKind::DancePerformed => state.categories.dances,
                PointsEventKind::GamePlayed => state.categories.games_played,
                PointsEventKind::GameWon => state.categories.games_won,
                PointsEventKind::ScreenshotObserved => state.categories.screenshots,
                PointsEventKind::CameraObserved => state.categories.camera_obs,
                PointsEventKind::PetPraised => state.categories.praises,
                PointsEventKind::DailyLogin => state.categories.login_days,
            };
            count >= *target as u64
        }
        AchievementCondition::TotalPoints { target } => state.total_points >= *target,
        AchievementCondition::LevelReached { target } => state.level >= *target,
        AchievementCondition::StreakDays { target } => state.current_streak_days >= *target,
    }
}

/// 检查并解锁满足条件的成就，返回新解锁的成就 ID 列表。
fn check_achievements(state: &mut PointsState) -> Vec<String> {
    let mut newly_unlocked = Vec::new();
    for achievement in all_achievements() {
        if state.achievements.contains(&achievement.id) {
            continue;
        }
        if is_condition_met(&achievement.condition, state) {
            state.achievements.push(achievement.id.clone());
            state.total_points += achievement.bonus_points as u64;
            newly_unlocked.push(achievement.id.clone());
            info!(achievement = %achievement.id, bonus = achievement.bonus_points, "achievement unlocked");
        }
    }
    newly_unlocked
}

// ─── 持久化路径 ─────────────────────────────────────────────────

static POINTS_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// 返回积分事件日志路径 `~/.bitcat/logs/points_events.jsonl`。
pub fn points_events_path() -> Result<PathBuf, String> {
    Ok(crate::logging::log_dir()?.join("points_events.jsonl"))
}

/// 返回积分聚合状态路径 `~/.bitcat/points_state.json`。
///
/// 注意：放在 data_dir 而非 log_dir 下，因为这是用户数据而非诊断日志。
pub fn points_state_path() -> Result<PathBuf, String> {
    Ok(crate::storage::data_dir()?.join("points_state.json"))
}

// ─── 核心引擎：award / record_event ─────────────────────────────

/// 记录一条积分事件：写入 JSONL 明细 + 更新聚合状态。
///
/// 这是唯一的公开入口点，所有 hook 点调用此函数。
/// 设计为 fire-and-forget：失败只记 warn 日志，不 panic、不返回错误。
pub fn award(kind: PointsEventKind, extra: Option<&str>) {
    let record = PointsEventRecord::new(kind, extra);

    // 1. 追加 JSONL 明细（审计用途）
    if let Ok(path) = points_events_path() {
        if let Err(e) = crate::logging::append_jsonl_path(&path, &record) {
            warn!(error = %e, "points event jsonl write failed");
        }
    }

    // 2. 更新聚合状态（原子写入）
    if let Ok(path) = points_state_path() {
        if let Err(e) = update_state(&path, &record) {
            warn!(error = %e, "points state update failed");
        }
    }
}

/// 仅记录事件到 JSONL（不更新聚合状态），供 core 层无法访问 app state 时使用。
///
/// app 层会在合适的时机通过 `award()` 批量同步状态。
pub fn record_event(kind: PointsEventKind, extra: Option<&str>) {
    let record = PointsEventRecord::new(kind, extra);
    if let Ok(path) = points_events_path() {
        if let Err(e) = crate::logging::append_jsonl_path(&path, &record) {
            warn!(error = %e, "points event record failed");
        }
    }
}

/// 处理每日登录逻辑：检测是否跨天，更新连续天数，发放登录积分。
///
/// 应在应用启动时调用一次。
pub fn check_daily_login() {
    let path = match points_state_path() {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "points state path unavailable for daily login");
            return;
        }
    };

    let today = chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();

    let _guard = POINTS_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|e| warn!("points write lock poisoned: {e}"));

    let mut state = match load_state(&path) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to load points state for daily login; using default");
            PointsState::default()
        }
    };

    // 同一天内重复调用 → 忽略
    if state.last_active_date.as_deref() == Some(&today) {
        debug!("daily login already counted for today");
        return;
    }

    match &state.last_active_date {
        None => {
            // 首次使用
            state.current_streak_days = 1;
        }
        Some(last) => {
            let yesterday = (chrono::Local::now() - chrono::Duration::days(1)).date_naive();
            if let Ok(last_date) = chrono::NaiveDate::parse_from_str(last, "%Y-%m-%d") {
                if last_date == yesterday {
                    state.current_streak_days += 1;
                } else {
                    state.current_streak_days = 1;
                }
            } else {
                state.current_streak_days = 1;
            }
        }
    }

    if state.current_streak_days > state.longest_streak_days {
        state.longest_streak_days = state.current_streak_days;
    }
    state.last_active_date = Some(today.clone());

    // 发放登录积分
    let login_record = PointsEventRecord::new(PointsEventKind::DailyLogin, None);
    apply_record_to_state(&mut state, &login_record);

    // 检查成就
    let _newly = check_achievements(&mut state);

    state.updated_at = chrono::Local::now().to_rfc3339();
    if let Err(e) = save_state(&path, &state) {
        warn!(error = %e, "failed to save points state after daily login");
    }

    info!(
        streak = state.current_streak_days,
        longest = state.longest_streak_days,
        points = state.total_points,
        "daily login processed"
    );
}

// ─── 内部：状态读写 ─────────────────────────────────────────────

fn update_state(path: &Path, record: &PointsEventRecord) -> Result<(), String> {
    let _guard = POINTS_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|e| format!("points write lock poisoned: {e}"))?;

    let mut state = load_state(path)?;
    apply_record_to_state(&mut state, record);

    // 更新连续天数（基于事件时间戳而非"现在"，以支持离线场景）
    update_streak_from_event(&mut state, &record.timestamp);

    // 检查成就
    let _newly_unlocked = check_achievements(&mut state);

    state.updated_at = record.timestamp.clone();
    save_state(path, &state)
}

/// 将单条事件记录应用到聚合状态上。
fn apply_record_to_state(state: &mut PointsState, record: &PointsEventRecord) {
    state.total_points += record.points as u64;
    state.categories.increment(record.kind);

    // 分类组累计积分
    let cat = record.kind.category();
    *state.category_totals.entry(cat).or_insert(0) += record.points as u64;

    // 重算等级
    let (level, title, exp_in, exp_next) = calculate_level(state.total_points);
    state.level = level;
    state.level_title = title.to_string();
    state.experience_in_current = exp_in;
    state.experience_to_next = exp_next;
}

/// 根据事件时间戳更新连续活跃天数。
fn update_streak_from_event(state: &mut PointsState, timestamp: &str) {
    let event_date = timestamp.split('T').next().unwrap_or("").to_string();

    if event_date.is_empty() {
        return;
    }

    match &state.last_active_date {
        Some(last) if *last == event_date => {}
        Some(last) => {
            if let (Ok(last_date), Ok(event_dt)) = (
                chrono::NaiveDate::parse_from_str(last, "%Y-%m-%d"),
                chrono::NaiveDate::parse_from_str(&event_date, "%Y-%m-%d"),
            ) {
                let yesterday = event_dt - chrono::Duration::days(1);
                if last_date == yesterday {
                    state.current_streak_days += 1;
                } else if last_date < event_dt {
                    state.current_streak_days = 1;
                }
                // 如果 last_date > event_dt（时钟回拨），不做处理
            }
        }
        None => {
            state.current_streak_days = 1;
        }
    }

    if state.current_streak_days > state.longest_streak_days {
        state.longest_streak_days = state.current_streak_days;
    }
    state.last_active_date = Some(event_date);
}

fn load_state(path: &Path) -> Result<PointsState, String> {
    if !path.exists() {
        return Ok(PointsState::default());
    }
    let content = fs::read_to_string(path).map_err(|e| format!("读取积分状态失败: {e}"))?;
    let content = content.trim_start_matches('\u{feff}');
    serde_json::from_str(&content).map_err(|e| format!("解析积分状态失败: {e}"))
}

fn save_state(path: &Path, state: &PointsState) -> Result<(), String> {
    crate::logging::write_json_atomic(path, state)
}

// ─── 查询接口 ───────────────────────────────────────────────────

/// 读取最近 N 条积分事件（按时间倒序）。
pub fn read_recent_events(limit: usize) -> Result<Vec<PointsEventRecord>, String> {
    let path = points_events_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(&path).map_err(|e| format!("打开积分事件文件失败: {e}"))?;
    use std::io::BufRead;
    let reader = std::io::BufReader::new(file);
    let mut events: Vec<PointsEventRecord> = Vec::new();
    for line in reader.lines().flatten() {
        if let Ok(record) = serde_json::from_str::<PointsEventRecord>(&line) {
            events.push(record);
        }
    }
    events.reverse(); // 最新的在末尾，倒序使最新在前
    events.truncate(limit);
    Ok(events)
}

/// 加载当前聚合状态（文件不存在则返回默认值）。
pub fn load_points_state() -> Result<PointsState, String> {
    let path = points_state_path()?;
    load_state(&path)
}

// ─── 测试 ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn point_values_are_positive() {
        for kind in [
            PointsEventKind::ChatCompleted,
            PointsEventKind::VoiceChat,
            PointsEventKind::MemoryCreated,
            PointsEventKind::ReminderCreated,
            PointsEventKind::ReminderCompleted,
            PointsEventKind::DancePerformed,
            PointsEventKind::GamePlayed,
            PointsEventKind::GameWon,
            PointsEventKind::ScreenshotObserved,
            PointsEventKind::CameraObserved,
            PointsEventKind::PetPraised,
            PointsEventKind::DailyLogin,
        ] {
            assert!(
                kind.point_value() > 0,
                "{kind:?} should have positive points"
            );
            assert!(!kind.label().is_empty(), "{kind:?} should have a label");
        }
    }

    #[test]
    fn level_calculation_boundaries() {
        // 边界值测试：(level, title, exp_in_current, exp_to_next)
        assert_eq!(calculate_level(0), (1, "初识", 0, 50)); // Lv1 起点
        assert_eq!(calculate_level(49), (1, "初识", 49, 50)); // Lv1 即将升级
        assert_eq!(calculate_level(50), (2, "熟悉", 0, 100)); // Lv2 刚到
        assert_eq!(calculate_level(149), (2, "熟悉", 99, 100)); // Lv2 即将升级
        assert_eq!(calculate_level(150), (3, "伙伴", 0, 250)); // Lv3 刚到
        assert_eq!(calculate_level(399), (3, "伙伴", 249, 250)); // Lv3 即将升级
        assert_eq!(calculate_level(400), (4, "好友", 0, 400)); // Lv4 刚到
        assert_eq!(calculate_level(5999), (7, "灵犀", 2999, 3000)); // Lv7 即将满级
        assert_eq!(calculate_level(6000), (8, "共生", 0, 1)); // 刚达满级，进度条显示 100%
        assert_eq!(calculate_level(99999), (8, "共生", 93999, 93999)); // 满级溢出
    }

    #[test]
    fn level_titles_match_thresholds_count() {
        assert_eq!(
            LEVEL_THRESHOLDS.len(),
            LEVEL_TITLES.len(),
            "thresholds and titles must be 1:1"
        );
        assert_eq!(
            LEVEL_THRESHOLDS.len(),
            LEVEL_ICONS.len(),
            "thresholds and icons must be 1:1"
        );
    }

    #[test]
    fn state_serialization_roundtrip() {
        let original = PointsState {
            total_points: 42,
            level: 2,
            level_title: "熟悉".into(),
            experience_in_current: 0,
            experience_to_next: 150,
            category_totals: [(PointsCategory::Chat, 10u64)].into(),
            categories: PointsCategories {
                chats: 1,
                ..Default::default()
            },
            current_streak_days: 3,
            longest_streak_days: 5,
            last_active_date: Some("2026-05-30".into()),
            achievements: vec!["first_chat".into()],
            updated_at: "2026-05-30T12:00:00+08:00".into(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: PointsState = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn event_record_serialization_roundtrip() {
        let record = PointsEventRecord::new(PointsEventKind::ChatCompleted, Some("test"));
        let json = serde_json::to_string(&record).unwrap();
        let restored: PointsEventRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, restored);
        assert_eq!(restored.points, 10); // ChatCompleted = 10
    }

    #[test]
    fn award_creates_files_and_accumulates() {
        let tmp = temp_dir();
        let events_path = tmp.path().join("logs").join("points_events.jsonl");
        let state_path = tmp.path().join("points_state.json");

        // Mock paths by using our own functions with overridden base dir is tricky.
        // Instead we test the logic directly through load/save.
        fs::create_dir_all(tmp.path().join("logs")).unwrap();

        // 手动构建初始状态
        let initial = PointsState::default();
        crate::logging::write_json_atomic(&state_path, &initial).unwrap();

        // 模拟 award 流程
        let record = PointsEventRecord::new(PointsEventKind::ChatCompleted, None);
        crate::logging::append_jsonl_path(&events_path, &record).unwrap();

        let mut state = load_state(&state_path).unwrap();
        apply_record_to_state(&mut state, &record);
        save_state(&state_path, &state).unwrap();

        // 验证
        let loaded = load_state(&state_path).unwrap();
        assert_eq!(loaded.total_points, 10);
        assert_eq!(loaded.level, 1); // 10 < 50, still Lv1
        assert_eq!(loaded.categories.chats, 1);

        // 第二次 award
        let record2 = PointsEventRecord::new(PointsEventKind::DailyLogin, None);
        crate::logging::append_jsonl_path(&events_path, &record2).unwrap();
        let mut state2 = load_state(&state_path).unwrap();
        apply_record_to_state(&mut state2, &record2);
        save_state(&state_path, &state2).unwrap();

        let loaded2 = load_state(&state_path).unwrap();
        assert_eq!(loaded2.total_points, 35); // 10 + 25
        assert_eq!(loaded2.categories.login_days, 1);
    }

    #[test]
    fn achievement_first_chat_unlocks_on_first_chat() {
        let mut state = PointsState::default();
        let newly = check_achievements(&mut state);
        assert!(newly.is_empty()); // nothing unlocked yet

        // 模拟一次对话
        state.categories.chats = 1;
        state.total_points = 10;
        let newly = check_achievements(&mut state);
        assert!(newly.contains(&"first_chat".to_string()));
        assert!(state.achievements.contains(&"first_chat".to_string()));
        // first_chat bonus = 10
        assert_eq!(state.total_points, 20); // 10 original + 10 bonus
    }

    #[test]
    fn achievement_streak_unlocks_at_target() {
        let mut state = PointsState::default();
        state.current_streak_days = 6;
        state.total_points = 100;

        let newly = check_achievements(&mut state);
        assert!(!newly.contains(&"streak_7".to_string()));

        state.current_streak_days = 7;
        let newly = check_achievements(&mut state);
        assert!(newly.contains(&"streak_7".to_string()));
        // streak_7 bonus = 50
        assert_eq!(state.total_points, 150); // 100 + 50
    }

    #[test]
    fn category_covers_all_event_kinds() {
        // 确保每个事件都有对应的分类
        for kind in [
            PointsEventKind::ChatCompleted,
            PointsEventKind::VoiceChat,
            PointsEventKind::MemoryCreated,
            PointsEventKind::ReminderCreated,
            PointsEventKind::ReminderCompleted,
            PointsEventKind::DancePerformed,
            PointsEventKind::GamePlayed,
            PointsEventKind::GameWon,
            PointsEventKind::ScreenshotObserved,
            PointsEventKind::CameraObserved,
            PointsEventKind::PetPraised,
            PointsEventKind::DailyLogin,
        ] {
            let _cat = kind.category(); // just ensure no panic
        }
    }

    #[test]
    fn all_achievements_initialized() {
        let achievements = all_achievements();
        assert_eq!(achievements.len(), 12); // 12 predefined achievements
        // 验证所有 ID 唯一
        let ids: Vec<&String> = achievements.iter().map(|a| &a.id).collect();
        let unique_ids: std::collections::HashSet<&String> = ids.iter().copied().collect();
        assert_eq!(
            ids.len(),
            unique_ids.len(),
            "achievement IDs must be unique"
        );
    }

    #[test]
    fn achievement_views_show_unlock_status() {
        let mut state = PointsState::default();
        state.categories.chats = 1;
        state.total_points = 10;
        // 不手动 push achievements — 让 check_achievements 来
        let _ = check_achievements(&mut state);

        let views = achievement_views(&state);
        let first_chat = views.iter().find(|v| v.id == "first_chat").unwrap();
        assert!(first_chat.unlocked);

        let chatterbox = views.iter().find(|v| v.id == "chatterbox").unwrap();
        assert!(!chatterbox.unlocked);
    }

    #[test]
    fn categories_increment_correctly() {
        let mut cats = PointsCategories::default();
        cats.increment(PointsEventKind::ChatCompleted);
        cats.increment(PointsEventKind::VoiceChat);
        cats.increment(PointsEventKind::GameWon);
        assert_eq!(cats.chats, 1);
        assert_eq!(cats.voice_chats, 1);
        assert_eq!(cats.games_won, 1);
        assert_eq!(cats.games_played, 0); // GameWon ≠ GamePlayed
    }

    #[test]
    fn recent_events_reads_from_jsonl() {
        let tmp = temp_dir();
        let path = tmp.path().join("events.jsonl");

        for i in 0..5u8 {
            let record = PointsEventRecord {
                timestamp: format!("2026-05-30T12:0{i}:00+08:00"),
                kind: PointsEventKind::ChatCompleted,
                points: 10,
                extra: None,
            };
            crate::logging::append_jsonl_path(&path, &record).unwrap();
        }

        // 内联读取逻辑（避免与公开函数签名冲突）
        let file = fs::File::open(&path).unwrap();
        use std::io::BufRead;
        let reader = std::io::BufReader::new(file);
        let mut all_events: Vec<PointsEventRecord> = Vec::new();
        for line in reader.lines().flatten() {
            if let Ok(record) = serde_json::from_str::<PointsEventRecord>(&line) {
                all_events.push(record);
            }
        }
        all_events.reverse();
        all_events.truncate(3);

        assert_eq!(all_events.len(), 3);
        // 最新在前（倒序）
        assert!(all_events[0].timestamp.contains("T12:04"));
        assert!(all_events[2].timestamp.contains("T12:02"));
    }
}
