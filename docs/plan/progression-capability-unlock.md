# 8Bit 成长与能力解锁实施计划

状态：设计落地计划，未开始实现  
前置调研：[core-gameplay-progression-research.md](../research/core-gameplay-progression-research.md)

## 目标

把 8Bit 从“一开始全功能开放的 AI 桌宠”，调整为“刚醒来有点笨、随着使用逐步学会能力的智能伙伴”。用户在使用过程中能感受到：

- 8Bit 的表达方式在变化；
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
    pub updated_at: String,
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
~/.ai-pad/progression/progress.json
~/.ai-pad/progression/ledger.jsonl
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
    你是 8Bit，一只住在电脑屏幕上的像素风小猫助手。
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
[8Bit成长状态]
当前阶段：Lv2 小记性
阶段表现：你可以记住用户明确允许你记住的信息。遇到不确定记忆时要承认可能记错。
已开放能力：chat, short_memory, memory_review
尚未开放能力：background_vision, shell, hotkey, agent_watch
安全规则：不要声称你拥有尚未开放或用户未授权的能力。
[/8Bit成长状态]
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
- 关闭敏感能力也能获得少量默契，表达“尊重边界也是关系成长”。

## UI 计划

新增设置页 tab：`成长`。

首版展示：

- 当前等级；
- Bit 余额；
- 默契进度条；
- 下一解锁能力；
- 最近 10 条奖励流水；
- 已解锁/已启用能力；
- 高风险能力的授权状态。

宠物/气泡表现：

- 升级时触发 `PetEvent::Notify` 或新增成长事件；
- 展示一句短台词；
- 如果新能力需要授权，气泡给出“去设置开启”的轻提示。

不建议首版做：

- 大型技能树；
- 抽卡；
- 复杂商店；
- 排行榜；
- 成就同步到 Steam。

## 实施阶段

### Phase 0：数据与纯逻辑

- 新增 `core/src/progression.rs`。
- 实现 load/save、事件奖励、等级计算、每日上限。
- 添加 `insta` 快照测试和 `rstest` 参数化测试。
- `core/src/lib.rs` 导出模块。

验收：

- 事件重复触发不会突破日上限；
- 中文字段序列化稳定；
- 等级边界正确；
- ledger JSONL 可 grep。

### Phase 1：接入聊天上下文

- 在 `app/src/gamepad.rs` 加载 `ProgressStore`。
- 拼接 `[8Bit成长状态]` 到 `context_parts`。
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

### Phase 4：设置页成长 tab

- 后端新增 `cmd_get_progression`。
- 前端新增 `growth/progression` tab。
- 展示等级、Bit、下一解锁、奖励流水、能力授权。

验收：

- 用户能看懂为什么某能力还没开；
- 用户能关闭敏感能力；
- 关闭敏感能力后不会被升级流程重新打开。

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

- 核心功能锁太久会像“买了但不给用”。首版应在 30 分钟内开放聊天和基础玩法。
- 靠提示词控制权限不可靠，必须 Rust gate。
- 奖励若绑定 API 消耗，会鼓励浪费 token。
- 后台截图不能成为积分来源，否则隐私观感很差。
- 多套完整 prompt 会难维护；只使用 overlay。

## 推荐首个 PR 范围

只做 Phase 0 + Phase 1：

- `core/src/progression.rs`
- `core/src/lib.rs`
- `core/src/prompts.rs` 和 `config/prompts.yml` 加 `stage_overlays`
- `app/src/gamepad.rs` 注入成长上下文并记录聊天完成
- 基础测试

这个 PR 完成后，8Bit 就能在对话里表现出“当前阶段”，但还不动工具权限和设置页，风险最低。

