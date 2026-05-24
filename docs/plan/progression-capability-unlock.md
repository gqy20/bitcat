# BitCat 成长与能力解锁实施计划

状态：设计落地计划，未开始实现（2026-05-18 补充积分体系五层设计、商店、成就、每日任务、心情系统）
前置调研：[core-gameplay-progression-research.md](../research/core-gameplay-progression-research.md)
Steam 积分模式参考：Hades（多层货币）、Vampire Survivors（极简反馈）、Dead Cells（蓝图解锁）、Steam 成就系统、Tamagotchi（隐式状态）

## 目标

把 BitCat 从“一开始全功能开放的桌面 AI 伙伴”，调整为“刚醒来有点笨、随着使用逐步学会能力的智能伙伴”。用户在使用过程中能感受到：

- BitCat 的表达方式在变化；
- 已有功能按阶段自然开放；
- 用户能通过日常使用获得 Bit 和默契；
- 高风险能力需要显式授权，不会因为升级自动打开。

首版不做完整地牢或复杂 RPG，只做一条能落到现有项目上的成长薄片。

## 总体方案

采用：

```text
固定 base preamble
  + 动态 stage overlay
  + 动态 capability context
  + 代码层权限 gate
  + 本地 ProgressStore
```

不要做 6 套完整系统提示词。阶段差异只作为短 overlay 注入对话上下文；真正的工具权限必须在 Rust 代码里硬拦。

## 当前代码落点

### AI 主链路

- `core/src/agent.rs`
  - `PetAgent::new()` 目前固定加载 `prompts.agent.preamble`。
  - 所有工具都在这里固定注册。
  - 短期不建议按阶段创建多套 Agent。

- `app/src/gamepad.rs`
  - 当前在聊天前拼接用户画像、短期记忆、长期记忆、截图记录和屏幕摘要。
  - 最适合新增 `progression_ctx`。
  - `enriched_msg` 是动态成长状态注入的第一落点。

- `core/src/permission_hook.rs`
  - 当前只拦截危险 `shell`。
  - 需要升级为“工具权限 gate”：按能力解锁和用户授权决定放行/阻止。

### 设置页

- `app/src/settings.rs`
  - 已有 `SettingsSnapshot`、统计、记忆审查、提示词保存等命令。
  - 后续可新增 `cmd_get_progression` / `cmd_progression_claim_reward`。

- `app/frontend/js/settings.js`
  - 可新增“成长”tab，展示等级、Bit、下一解锁、能力树。

### 事件来源

首版优先接入已有事件：

- 聊天完成：`app/src/gamepad.rs` 中 `stream_result: Ok(reply)` 分支。
- 舞蹈播放：`core/src/tools.rs` 的 `perform_dance` / `play_dance` 执行路径，或 app 侧收到 `play-dance` 后。
- 小游戏启动/结束：`app/src/game.rs`。
- 手动截图完成：`app/src/screenshot.rs` 的 `do_screenshot_now`。
- 应用启动：`app/src/lib.rs` setup 后。

## 模块设计

新增：

```text
core/src/progression.rs
```

职责：

- 存储成长状态；
- 记录 Bit 收支；
- 计算等级；
- 判断能力是否解锁；
- 生成可注入 prompt 的成长上下文；
- 提供每日奖励上限，避免刷分。

建议数据结构：

```rust
pub struct ProgressStore {
    pub bit_balance: u32,
    pub synergy_xp: u32,
    pub level: ProgressLevel,
    pub unlocked_features: BTreeSet<FeatureId>,
    pub enabled_features: BTreeSet<FeatureId>,
    pub daily: DailyProgress,
    pub mood: MoodState,
    pub inventory: BTreeSet<String>,       // 已购买的商品 ID
    pub achievements: BTreeMap<String, AchievementState>,  // 已达成的成就
    pub daily_quests: DailyQuestState,
    pub updated_at: String,
}

pub struct AchievementState {
    pub unlocked_at: String,
    pub claimed: bool,  // 领取过奖励
}

pub struct DailyQuestState {
    pub date: String,                       // 当日日期，跨日自动刷新
    pub quests: Vec<DailyQuest>,
    pub all_completed_bonus_claimed: bool,
}

pub enum ProgressEvent {
    AppLaunched,
    ChatCompleted,
    DancePlayed,
    GameCompleted { game_type: String, outcome: String, score: Option<u32> },
    ScreenshotManualCompleted,
    AgentWatchCompleted,
    SettingEnabled { key: String },
    SettingDisabled { key: String },
    ItemPurchased { item_id: String, cost: u32 },
}

pub enum FeatureId {
    Chat,
    ShortMemory,
    LongMemory,
    DancePlayback,
    AiChoreography,
    Minigames,
    ManualScreenshot,
    BackgroundVision,
    RecentScreenshotsTool,
    ClipboardTool,
    HotkeyTool,
    LaunchTool,
    ShellTool,
    AgentWatch,
}
```

本地文件：

```text
~/.bitcat/progression/progress.json      — 主状态（等级/Bit/默契/心情/库存/成就）
~/.bitcat/progression/ledger.jsonl       — Bit 收支流水（append-only，可 grep）
~/.bitcat/progression/daily_quests.json  — 当日任务状态
config/shop.yml                          — 商品定义
config/achievements.yml                  — 成就定义
config/daily_quests.yml                  — 每日任务模板池
```

`progress.json` 存当前状态；`ledger.jsonl` 追加奖励流水，便于排查重复奖励。

## 阶段与能力

首版建议等级：

| 阶段 | 默契门槛 | 能力表现 |
| --- | ---: | --- |
| Lv0 功能机 | 0 | 基础桌宠、拖拽、简单气泡 |
| Lv1 会聊天 | 5 | AI 对话、基础情绪 |
| Lv2 小记性 | 15 | 短期记忆、记忆审查入口 |
| Lv3 会玩 | 30 | 舞蹈、AI 编舞、小游戏奖励 |
| Lv4 会观察 | 55 | 手动截图、最近截图；后台观察需授权 |
| Lv5 会帮忙 | 90 | launch/hotkey/clipboard/shell 等工具需逐项授权 |
| Lv6 搭档 | 140 | Agent Watch、主动建议 |

注意：

- `unlocked` 只表示“可以学习/可以开启”。
- `enabled` 才表示“用户已允许使用”。
- 高风险能力必须同时满足 `unlocked && enabled`。

## 提示词设计

### 不推荐

不要这样做：

```text
agent_lv0.preamble
agent_lv1.preamble
agent_lv2.preamble
agent_lv3.preamble
agent_lv4.preamble
agent_lv5.preamble
```

问题：

- 难同步人格；
- 用户在设置页编辑 prompt 时无法理解哪份生效；
- 测试和回归成本高；
- 容易出现某个阶段忘记安全规则。

### 推荐

保留一个基础 preamble，在 `config/prompts.yml` 增加短 overlay：

```yaml
agent:
  preamble: |
    你是 BitCat，一个住在电脑屏幕边缘的桌面 AI 伙伴。
    ...
  stage_overlays:
    feature_phone: |
      你刚醒来，只能做基础陪伴。不要声称自己能记住长期信息、看屏幕或操作电脑。
    chat_phone: |
      你能自然聊天，但记性很短。回答要像刚学会说话的聪明小猫。
    memory_phone: |
      你可以记住用户明确允许你记住的信息。遇到不确定记忆时要承认可能记错。
    play_partner: |
      你开始会玩，会主动用舞蹈和小游戏表达情绪。
    vision_partner: |
      只有用户授权后才能谈论屏幕观察。不要暗示你一直在偷看。
    tool_partner: |
      你可以帮用户操作电脑，但危险操作必须先确认。
```

Rust 结构调整：

```rust
pub struct AgentPromptConfig {
    pub preamble: String,
    pub stage_overlays: BTreeMap<String, String>,
}
```

聊天前注入：

```text
[BitCat成长状态]
当前阶段：Lv2 小记性
阶段表现：你可以记住用户明确允许你记住的信息。遇到不确定记忆时要承认可能记错。
已开放能力：chat, short_memory, memory_review
尚未开放能力：background_vision, shell, hotkey, agent_watch
安全规则：不要声称你拥有尚未开放或用户未授权的能力。
[/BitCat成长状态]
```

这个上下文应该在 `app/src/gamepad.rs` 中与 memory/profile/screen_summary 一起拼进 `context_parts`。

## 工具权限 Gate

提示词只负责角色表现；权限必须由代码控制。

建议工具分级：

| 工具 | 风险 | 首版策略 |
| --- | --- | --- |
| `get_time` | 低 | 始终开放 |
| `perform_dance` / `play_dance` | 低 | Lv3 软开放，也可提前允许 |
| `search_memory` / `remember` | 中 | Lv2 且记忆功能 enabled |
| `recent_screenshots` | 中 | Lv4 且截图记录 enabled |
| `read_clipboard` | 高 | Lv5 且用户逐项授权 |
| `launch_program` | 高 | Lv5 且用户逐项授权 |
| `send_hotkey` | 高 | Lv5 且用户逐项授权 |
| `force_foreground` | 高 | Lv5 且用户逐项授权 |
| `shell` | 最高 | Lv5 且用户逐项授权，仍保留危险命令黑名单 |

实现路径：

1. `PermissionHook` 读取 `ProgressStore`。
2. 对每个 tool_name 查询 `is_tool_allowed(tool_name)`。
3. 不允许时返回稳定、可解释的 `ToolCallHookAction::Skip` reason。
4. 保留现有 shell 危险命令检查。

后续如果 rig 支持动态工具注册，也可以在 `PetAgent::new()` 根据能力只注册部分工具。但首版更稳的是注册不变、执行前 gate。

## 奖励规则

首版只做少量奖励：

| 事件 | Bit | 默契 | 日上限 |
| --- | ---: | ---: | --- |
| 启动应用 | 20 | 0 | 1 |
| 聊天完成 | 10 | 1 | 3 |
| 舞蹈播放 | 5 | 1 | 5 |
| 小游戏完成 | 10-40 | 2 | 5 |
| 手动截图完成 | 10 | 1 | 5 |
| Agent Watch 完成 | 20 | 3 | 3 |
| 关闭/禁用敏感能力 | 0 | 1 | 3 |

原则：

- 不奖励 token 消耗；
- 不奖励后台观察时长；
- 不奖励 shell 次数；
- 不奖励纯挂机；
- 关闭敏感能力也能获得少量默契，表达”尊重边界也是关系成长”。

## 积分体系总览（五层设计）

首版设计的 Bit + 默契 + 工具 Gate 三层仍然成立。以下是按 Steam 热门模式验证后的完整五层架构：

```text
┌─────────────────────────────────────────────────────┐
│                 BitCat 积分体系                          │
│                                                     │
│  第一层：Bit（日常软货币，可花费）                       │
│    赚：聊天/游戏/签到/截图                              │
│    花：配饰商店/气泡皮肤/桌面摆件/小游戏道具              │
│    节奏：每天都有赚有花，轻量循环                         │
│    参考：Vampire Survivors（Gold）                     │
│                                                     │
│  第二层：默契（经验值，不可花费）                        │
│    赚：所有互动累积                                    │
│    用：等级提升 → 能力解锁 → BitCat 变聪明                │
│    节奏：长线累积，几周爬一级                            │
│    参考：Dead Cells（Cells → 蓝图解锁）                │
│                                                     │
│  第三层：成就（里程碑，纯记录）                          │
│    触发：特定条件达成                                   │
│    展示：成就列表 + 稀有度 + 时间戳                      │
│    不花不赚，但有收集感                                 │
│    参考：Steam 成就系统                                 │
│                                                     │
│  第四层：每日任务（短期目标）                            │
│    形式：每天 3 个随机小目标                             │
│    奖励：Bit + 偶尔触发隐藏成就                          │
│    心理：给用户”今天打开 BitCat 要做什么”的理由             │
│    参考：Slay the Spire 每日挑战                       │
│                                                     │
│  第五层：BitCat 心情（隐式积分）                          │
│    机制：受互动频率和类型影响，不显示具体数值              │
│    效果：高心情 → 主动搭话/更积极的回复                  │
│          低心情 → 安静/犯困/回复变简短                   │
│    参考：Tamagotchi（不显示数值但用户能感知）             │
└─────────────────────────────────────────────────────┘
```

为什么是五层而不是两层：

- Bit + 默契处理经济和成长（已有设计）
- 成就提供**长期收集目标**（Bit 花完了不会消失的东西）
- 每日任务提供**短期登录理由**（日活驱动）
- 心情提供**情感反馈**（不靠数字，靠用户自然感知）

每层独立运作，不强制耦合。首版可以只实现前三层。

## Bit 商店设计

首版消耗池需要更具体的商品定义。原则：**只卖装饰和表达，不卖功能和权限**。

### 商品分类

| 分类 | 商品示例 | 价格区间 | 解锁条件 |
|------|---------|---------|---------|
| **气泡主题** | 暗色模式 / 樱花粉 / 终端绿 / 复古像素 | 50~200 Bit | Lv1+ |
| **宠物配饰** | 小帽子 / 墨镜 / 领结 / 皇冠 / 蝴蝶结 | 80~300 Bit | Lv2+ |
| **舞蹈解锁** | 街舞 / 芭蕾 / 迪斯科 / 机械舞 / 雨中曲 | 100~500 Bit | Lv3+ |
| **小游戏皮肤** | 蛇头外观 / 记忆牌面 / 接东西主题 | 60~200 Bit | Lv3+ |
| **问候语包** | 元气早安 / 毒舌问候 / 文艺开场 / 猫语 | 30~100 Bit | Lv1+ |
| **桌面摆件** | 小鱼缸 / 像素花盆 / 迷你地球仪 | 150~400 Bit | Lv4+ |
| **称号** | “猫奴” / “夜猫子” / “游戏达人” / “话痨之友” | 100~300 Bit | 特定成就触发购买资格 |

### 商店数据结构

```rust
pub struct ShopItem {
    pub id: String,
    pub name: String,
    pub category: ShopCategory,
    pub price_bit: u32,
    pub required_level: u8,
    pub required_achievement: Option<String>,  // 某些商品需要先达成成就
    pub asset_ref: String,                     // 精灵/主题资源引用
    pub description: String,
}

pub enum ShopCategory {
    BubbleTheme,
    PetAccessory,
    Dance,
    GameSkin,
    GreetingPack,
    DesktopDecor,
    Title,
}
```

### 商店文件

```text
config/shop.yml          — 商品定义（可扩展，无需改代码）
~/.bitcat/progression/inventory.json  — 已购买商品记录
```

商店内容在 `shop.yml` 中定义而非硬编码，方便后续追加商品而不用改 Rust 代码。

### 防通胀设计

Bit 的日常获取上限约 300~400 Bit（按奖励表推算），商品价格分布在 30~500 Bit，意味着：
- 轻度用户：每天攒够买一个气泡主题（~3 天）
- 活跃用户：每天买一个配饰或存着买舞蹈
- 不存在”买完所有东西没事做”的问题：持续追加新商品即可

## 成就系统

独立于 Bit 和默契的纯收集系统。成就不花不赚，但有稀有度和展示价值。

### 成就定义

```rust
pub struct Achievement {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,          // emoji 或精灵引用
    pub rarity: Rarity,
    pub hidden: bool,          // 隐藏成就：条件未知直到达成
    pub condition: AchievementCondition,
    pub unlocked_at: Option<DateTime<Utc>>,
}

pub enum Rarity {
    Common,     // >50% 用户会获得
    Uncommon,   // 20~50%
    Rare,       // 5~20%
    Epic,       // 1~5%
    Legendary,  // <1%
}

pub enum AchievementCondition {
    TotalChats(u32),                    // 累计聊天 N 次
    GameScore { game: String, min: u32 }, // 小游戏达到指定分数
    ConsecutiveDays(u32),               // 连续使用 N 天
    TotalDances(u32),                   // 累计跳舞 N 次
    TotalScreenshots(u32),              // 累计手动截图 N 次
    SpecificTime { hour_start: u32, hour_end: u32 }, // 特定时间段使用
    SpecificPhrase(String),             // 对 BitCat 说出特定内容（隐藏成就）
    Custom(String),                     // 自定义条件表达式
}
```

### 首版成就列表（~30 个）

**成长类**：
| ID | 名称 | 条件 | 稀有度 |
|----|------|------|--------|
| first_chat | 初次对话 | 聊天 1 次 | Common |
| chat_10 | 渐入佳境 | 聊天 10 次 | Common |
| chat_50 | 无话不谈 | 聊天 50 次 | Uncommon |
| chat_100 | AI 之友 | 聊天 100 次 | Rare |
| first_memory | 我记住了 | 首条长期记忆 | Common |
| memory_10 | 过目不忘 | 10 条长期记忆 | Uncommon |

**游戏类**：
| ID | 名称 | 条件 | 稀有度 |
|----|------|------|--------|
| snake_50 | 毛线球新手 | Snake 达到 50 分 | Common |
| snake_100 | 毛线球大师 | Snake 达到 100 分 | Rare |
| memory_perfect | 完美记忆 | Memory 零失误通关 | Uncommon |
| catch_master | 接物达人 | Catch 达到 50 分 | Uncommon |
| battle_first_win | 首次胜利 | Battle 胜利 1 次 | Common |
| battle_10_wins | 身经百战 | Battle 胜利 10 次 | Rare |

**时间类**：
| ID | 名称 | 条件 | 稀有度 |
|----|------|------|--------|
| daily_3 | 三日之约 | 连续使用 3 天 | Common |
| daily_7 | 一周常客 | 连续使用 7 天 | Uncommon |
| daily_30 | 月度搭档 | 连续使用 30 天 | Rare |
| night_owl | 夜猫子 | 凌晨 2-5 点使用 | Uncommon |
| early_bird | 早起鸟 | 早上 5-7 点使用 | Uncommon |

**隐藏成就**（条件不公开，达成后揭晓）：
| ID | 名称 | 条件 | 稀有度 |
|----|------|------|--------|
| secret_cat_person | ??? | 对 BitCat 说”你是最好的” | Rare |
| secret_hacker | ??? | shell 命令执行成功 5 次 | Epic |
| secret_midnight | ??? | 0:00 正好在和 BitCat 聊天 | Rare |
| secret_patience | ??? | 连续 10 分钟不操作 | Uncommon |

**成就解锁奖励**：大部分成就不给 Bit（避免刷成就通胀），但有 2 个例外：
- 每个 Epic 以上成就给 50 Bit（稀有度本身控制了频率）
- 特定成就解锁对应商品的购买资格（如”毛线球大师”解锁限定蛇皮肤购买权）

## 每日任务系统

给用户”今天打开 BitCat 要做什么”的短期目标。每天 3 个随机任务，次日 0 点刷新。

### 任务定义

```rust
pub struct DailyQuest {
    pub id: String,
    pub description: String,
    pub target: u32,
    pub progress: u32,
    pub reward_bit: u32,
    pub quest_type: QuestType,
}

pub enum QuestType {
    Chat { count: u32 },             // 聊天 N 次
    PlayGame { game_type: String },  // 玩一局指定游戏
    Dance { count: u32 },            // 跳舞 N 次
    Screenshot,                      // 手动截图一次
    UseTool,                         // 使用任何 AI 工具一次
    PlayAnyGame,                     // 玩任意一局游戏
}
```

### 任务池

| 任务模板 | 概率权重 | Bit 奖励 |
|---------|---------|---------|
| 和 BitCat 聊 3 次 | 3 | 15 |
| 和 BitCat 聊 5 次 | 2 | 25 |
| 玩一局 Snake | 2 | 20 |
| 玩一局 Memory | 2 | 20 |
| 玩任意一局游戏 | 3 | 15 |
| 让 BitCat 跳一段舞 | 2 | 10 |
| 手动截一张图 | 1 | 15 |
| 使用 AI 工具 1 次 | 2 | 20 |
| 连续对话 3 轮 | 1 | 30 |

每日从池中按权重随机抽 3 个，不重复。全部完成额外奖励 30 Bit。

### 任务数据

```text
~/.bitcat/progression/daily_quests.json  — 当日任务状态
config/daily_quests.yml                  — 任务模板池
```

任务刷新时如果前一天的未完成，**不惩罚**，直接替换为新任务（参考研究文档的”断签不惩罚”原则）。

## BitCat 心情系统

隐式积分：影响 BitCat 的回复风格和主动行为，不向用户显示具体数值。

### 心情因子

```rust
pub struct MoodState {
    /// 0.0~1.0，由近期互动频率和类型计算
    pub happiness: f32,
    /// 上次互动时间戳，用于衰减计算
    pub last_interaction: DateTime<Utc>,
    /// 心情更新时间
    pub updated_at: DateTime<Utc>,
}
```

### 心情计算规则

```
每次有效互动（聊天/游戏/舞蹈）→ happiness += 0.05~0.15（根据互动类型）
每小时自然衰减 → happiness -= 0.02
连续 2 天无互动 → happiness 快速衰减到 0.1 以下
happiness 钳位到 [0.0, 1.0]
```

### 心情对行为的影响

| happiness 范围 | 表现 |
|---------------|------|
| 0.8~1.0 高兴 | 回复更活泼、主动搭话频率高、偶尔给惊喜（”我在想昨天你说的事...”） |
| 0.5~0.8 平静 | 正常回复、按需互动、不主动打扰 |
| 0.2~0.5 低落 | 回复变简短、更多犯困动画、不太主动 |
| 0.0~0.2 孤单 | 安静待机、偶尔叹气动画、用户回来时特别高兴 |

**关键设计**：心情不显示具体数值。用户通过 BitCat 的行为自然感知”它今天好像不太开心”。如果用户主动关心（”你还好吗”），BitCat 可以诚实回答自己的感受，这本身就是互动的一部分。

### 心情不影响的东西

- **不影响 AI 回复质量**：模型能力不随心情下降，只是表达风格变化
- **不影响工具能力**：心情低不会拒绝执行工具
- **不产生惩罚**：不会因为心情低而损失 Bit 或默契
- **不可购买**：没有”花 Bit 提升心情”的选项

## 可视化反馈设计

积分反馈必须可感知，不能默默存进 json。参考 Vampire Survivors 的”满屏飞金币”。

### Bit 获取动画

```
获得 Bit 时：
  1. 宠物头顶弹出 “+10 Bit” 浮动文字，持续 1.2s 后上浮消失
  2. 右上角 Bit 计数器数字跳动更新（CSS transition）
  3. 如果是首次获得某种奖励，额外弹出一个小气泡说明

关键场景：
  - 聊天完成 → 宠物头顶 “+10” 飘出
  - 游戏结束结算 → 屏幕中央 “+40 Bit” 大字 + 星星粒子
  - 每日任务完成 → 右侧滑入提示 “任务完成！+20 Bit”
  - 成就解锁 → 全屏横幅 + 特效 + 打字机文案
```

### 等级提升动画

```
默契达到门槛时：
  1. 宠物进入特殊动画（发光 + 跳跃）
  2. 全屏半透明遮罩
  3. “BitCat 长大了！” 标题
  4. 新等级名称 + 解锁能力描述
  5. 如果新能力需要授权，显示”去设置开启”按钮
  6. 3s 后自动消失，或用户点击关闭
```

### 商店购买动画

```
购买成功时：
  1. 商品卡片翻转/闪光
  2. 宠物做出对应反应（戴帽子/换气泡/跳舞）
  3. Bit 计数器数字减少动画
```

### 前端实现位置

这些动画主要复用现有管线：
- 浮动数字：`game_engine.js` 的 `drawFloaters` 模式
- 宠物特殊动画：现有 `PetStateMachine` + `PetEvent`
- 全屏遮罩/横幅：CSS animation，类似游戏结束 overlay
- Bit 计数器：panel 窗口或宠物窗口角落的 DOM 元素

## UI 计划

新增设置页 tab：`成长`。

首版展示（四个子面板）：

### 成长概览
- 当前等级 + 等级名称
- Bit 余额 + 今日获取/消耗
- 默契进度条 + 距下一级差值
- 下一解锁能力预览
- 最近 10 条奖励流水

### 能力与权限
- 已解锁/已启用能力列表
- 高风险能力的授权状态
- 每个能力的"首次解锁"时间

### 成就（独立滚动列表）
- 按稀有度分组展示
- 已达成：全彩 + 解锁时间
- 未达成：灰色 + 条件描述（隐藏成就显示 "???"）
- 总成就数 / 总数 进度

### 每日任务（当日 3 个）
- 任务描述 + 进度条 + 奖励
- 全部完成额外奖励提示
- 距刷新倒计时

宠物/气泡表现：

- 升级时触发 `PetEvent::Notify` 或新增成长事件；
- 展示一句短台词；
- 如果新能力需要授权，气泡给出“去设置开启”的轻提示。

不建议首版做：

- 大型技能树；
- 抽卡 / 盲盒机制；
- 复杂商店（需要服务端验证的）；
- 排行榜 / 社交比较；
- 成就同步到 Steam；
- Bit 交易 / 转赠；
- 花钱提升心情。

## 实施阶段

### Phase 0：数据与纯逻辑

- 新增 `core/src/progression.rs`。
- 实现 load/save、事件奖励、等级计算、每日上限。
- 实现 Bit 商店验证（购买 / 库存 / 余额扣除）。
- 实现成就条件检查和解锁。
- 实现每日任务生成和进度跟踪。
- 实现心情因子计算。
- 添加 `insta` 快照测试和 `rstest` 参数化测试。
- `core/src/lib.rs` 导出模块。

验收：

- 事件重复触发不会突破日上限；
- 中文字段序列化稳定；
- 等级边界正确；
- ledger JSONL 可 grep；
- 商店购买余额不足时拒绝；
- 成就条件达成时正确触发且不重复；
- 每日任务跨日自动刷新且不惩罚；
- 心情在无互动时自然衰减。

### Phase 1：接入聊天上下文

- 在 `app/src/gamepad.rs` 加载 `ProgressStore`。
- 拼接 `[BitCat成长状态]` 到 `context_parts`。
- 聊天成功后记录 `ProgressEvent::ChatCompleted`。
- 升级时用 bubble 或 pet event 提示。

验收：

- Lv0/Lv1 的回复会承认自己能力有限；
- Lv2 后才会自然提及记忆；
- 未授权截图时不会暗示一直观察屏幕。

### Phase 2：接入事件奖励

- 启动应用奖励。
- 舞蹈播放奖励。
- 小游戏完成奖励。
- 手动截图奖励。
- Agent Watch 完成奖励。

验收：

- 每个事件有 ledger 记录；
- 日上限生效；
- 没有奖励后台观察/纯挂机。

### Phase 3：工具权限 Gate

- 扩展 `PermissionHook`。
- 增加 `FeatureId` 与 tool_name 映射。
- 未开放/未授权工具返回稳定错误文案。
- 保留 shell 危险命令黑名单。

验收：

- Lv1 调用 `shell` 被拒绝；
- Lv5 未授权 `shell` 仍被拒绝；
- Lv5 授权后危险命令仍被拒绝；
- `get_time` 始终可用。

### Phase 4：设置页成长 tab + 可视化反馈

- 后端新增 `cmd_get_progression` / `cmd_get_achievements` / `cmd_get_daily_quests` / `cmd_shop_list` / `cmd_shop_buy`。
- 前端新增 `growth` tab（四个子面板：成长概览 / 能力与权限 / 成就 / 每日任务）。
- Bit 获取动画（宠物头顶浮动数字 + 计数器跳动）。
- 等级提升全屏动画。
- 成就解锁横幅。
- 心情影响宠物 idle 动画选择。

验收：

- 用户能看懂为什么某能力还没开；
- 用户能关闭敏感能力；
- 关闭敏感能力后不会被升级流程重新打开；
- Bit 获取时有可见的动画反馈；
- 等级提升时有仪式感动画；
- 成就列表按稀有度正确分组；
- 每日任务跨日刷新无惩罚。

## 测试策略

core：

- `ProgressStore` 序列化用 `insta::assert_yaml_snapshot!`。
- 奖励规则用 `rstest` 参数化。
- 日上限和等级边界用普通单测。
- 权限映射测试覆盖每个 tool。

app：

- 重点测试 settings command 返回结构。
- IPC 测试可放后，首版先保证 core 和手动联调。

frontend：

- `app/frontend` 用 Vitest 测成长 tab 的渲染函数。

手动验证：

- 新用户首次启动；
- 连续聊天升级；
- 未授权截图时对话表现；
- 授权/关闭敏感能力；
- Steam/portable 干净目录运行。

## 风险

- 核心功能锁太久会像”买了但不给用”。首版应在 30 分钟内开放聊天和基础玩法。
- 靠提示词控制权限不可靠，必须 Rust gate。
- 奖励若绑定 API 消耗，会鼓励浪费 token。
- 后台截图不能成为积分来源，否则隐私观感很差。
- 多套完整 prompt 会难维护；只使用 overlay。
- Bit 只赚不花会通胀。商店必须有足够多的消费出口，首版至少 15 个商品。
- 成就刷分：稀有度本身限制频率，Epic+ 成就给 Bit 的上限是每天 50，不构成通胀路径。
- 每日任务变成负担：任务要简单（”聊 3 次”而非”聊 30 分钟”），完不成不惩罚。
- 心情系统让用户焦虑：心情不影响功能和积分，低心情只是表达风格变化。不在 UI 显示具体数值。

## 推荐首个 PR 范围

只做 Phase 0 + Phase 1：

- `core/src/progression.rs`
- `core/src/lib.rs`
- `core/src/prompts.rs` 和 `config/prompts.yml` 加 `stage_overlays`
- `config/shop.yml`（15~20 个首版商品）
- `config/achievements.yml`（30 个首版成就）
- `config/daily_quests.yml`（9 个任务模板）
- `app/src/gamepad.rs` 注入成长上下文并记录聊天完成
- 基础测试

这个 PR 完成后，BitCat 就能在对话里表现出”当前阶段”，Bit / 默契 / 成就 / 每日任务的数据层就位，但还不动工具权限和设置页 UI，风险最低。

