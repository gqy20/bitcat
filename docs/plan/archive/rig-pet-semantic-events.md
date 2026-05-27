# Rig 驱动的宠物语义事件改造方案

> 日期：2026-05-14  
> 状态：Phase 1-6 已完成（2026-05-15）  
> 目标：清理旧的 `SetState` 视觉状态事件，把 Rig Agent / Tool 生命周期接入为一套可解释、可测试、可扩展的宠物语义事件。

## 背景

当前项目已经使用 `rig-core = 0.36` 构建 AI Agent，并且主对话链路已经具备较好的基础：

- `PetAgent::chat_stream()` 使用 `stream_prompt().multi_turn()`，可以区分文本流、工具调用和工具结果。
- `AgentStreamEvent::Tool` 已经携带 `ToolRuntimeEvent`，包含 `planned / blocked / finished / failed` 等生命周期信息。
- `PermissionHook` 已经作为 Rig `PromptHook` 注册，可在工具调用前 `Continue / Skip / Terminate`。
- `perform_dance` / `play_dance` 等工具已经采用 Tool-native 结构化参数。

改造前宠物动画主链路混用了旧协议：

```text
AI 回复文本 -> Rust 关键词判断 -> PetCommand::SetState(Happy/Confused/Idle)
按钮/业务事件 -> PetCommand::SetState(Talk/Sleep/Happy)
前端 pet-event -> pet.applyEvent() -> 直接切视觉 state
```

这会让“语义”和“动画表现”耦合在一起，也让 Rig 已经提供的工具生命周期事件无法自然驱动宠物。Phase 1-6 已将主链路迁移到 tagged `PetEvent`，用 `AgentReaction` 接管最终情绪和长期记忆候选，并通过 `PetEventBus` / `MoodPolicy` 收口事件发送、去重、节流、情绪生命周期与可观察性。Phase 6 进一步从 rig `MultiTurnStreamItem` 派生 `AiWriting` / `ToolPreparing`，让宠物事件时间线能表达模型写作和准备工具的过程。

## 设计原则

1. **模型负责语义判断和行为编排**：工具选择、舞蹈编排、最终情绪应尽量由 Rig Agent 的工具调用或结构化输出表达。
2. **Rust 负责身体和边界**：schema、权限、状态优先级、生命周期、持久化、审计和 UI 事件都由 Rust 校验和调度。
3. **前端负责表现**：前端保留动画状态机，但不再从 IPC 收到裸 `happy/confused/talk` 这类视觉状态。
4. **清理旧视觉事件**：AI 主链路不再发送 `SetState`；保留 `WalkTo`、`ShowBubble`、`PlayDance` 这类明确动作命令。
5. **分层而不是塞进 notification**：短生命周期通知、最终情绪反应、长生命周期模式分开建模。

## Rig 源码能力依据

`rig-core 0.36.0` 的相关能力：

- `PromptHook::on_tool_call()`：工具执行前触发，可返回 `ToolCallHookAction::Continue / Skip / Terminate`。
- `PromptHook::on_tool_result()`：工具执行后触发，可观察结果。
- `PromptHook::on_text_delta()`：流式文本 delta 到达时触发。
- `PromptHook::on_tool_call_delta()`：工具参数流式生成时触发。
- `stream_prompt().multi_turn()`：当前项目已经使用，可消费 `MultiTurnStreamItem`。
- `Extractor<T>`：可用 `JsonSchema + Deserialize` 结构体约束输出，适合后续替代关键词情绪判断。

短期实现不强制把所有 UI 事件塞进 Hook。当前项目已经在 `PetAgent::chat_stream()` 中消费 `MultiTurnStreamItem`，第一阶段直接在 app 层把 `AgentStreamEvent::Tool` 映射成宠物语义事件，改动更小、可测试性更好。

## 新协议

新增 `core/src/pet_event.rs`，定义 app 与前端共享的宠物事件协议。

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PetEvent {
    Notify {
        kind: PetNotificationKind,
        body: Option<String>,
        ttl_ms: Option<u64>,
        refresh: bool,
    },
    ClearNotification {
        kind: Option<PetNotificationKind>,
    },
    React {
        mood: PetMood,
        speech: Option<String>,
    },
    SetMode {
        mode: PetMode,
    },
    WalkTo {
        x: f32,
    },
    ShowBubble {
        text: String,
    },
    PlayDance {
        name: String,
    },
    Exit,
}
```

### PetNotificationKind

短生命周期、可刷新、会过期。

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PetNotificationKind {
    AiThinking,
    ToolRunning,
    ToolBlocked,
    ToolFailed,
    Listening,
    ScreenshotObserving,
}
```

默认映射：

| kind | 默认动画 | 默认 TTL | refresh |
|---|---|---:|---|
| `ai_thinking` | `talk` | 30000ms | true |
| `tool_running` | `talk` 或 `focused` | 30000ms | true |
| `tool_blocked` | `confused` | 15000ms | true |
| `tool_failed` | `confused` | 15000ms | true |
| `listening` | `talk` | 无，手动清理 | true |
| `screenshot_observing` | `focused` 或 `talk` | 5000ms | true |

### PetMood

对话结束或明确反应使用，不表示长期模式。

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PetMood {
    Idle,
    Happy,
    Confused,
    Focused,
    Caring,
    Excited,
    Sleepy,
}
```

第一阶段可以只映射到现有视觉状态：

| mood | 当前动画 |
|---|---|
| `idle` | `idle` |
| `happy` | `happy` |
| `confused` | `confused` |
| `focused` | `talk` |
| `caring` | `happy` |
| `excited` | `happy` |
| `sleepy` | `sleep` |

### PetMode

长生命周期模式，不应被普通短通知永久覆盖。

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PetMode {
    Idle,
    Sleep,
    GamePlay,
}
```

## 前端状态优先级

`app/frontend/js/pet.js` 保留现有时间轴动画引擎，增加语义层。

推荐优先级：

```text
mode: Sleep/GamePlay
  > active notification
  > reaction mood
  > idle
```

说明：

- `Sleep` 是长模式，普通 `ToolRunning` 不应把睡眠永久打断。
- `GamePlay` 由游戏系统控制，普通 AI 文本通知不应覆盖游戏中状态。
- notification 可刷新、可过期。
- reaction 是最终情绪，播完当前 repeat 后自然 fallback。

前端新增方法：

```javascript
setNotification(kind, body, ttlMs, refresh)
clearNotification(kind)
react(mood, speech)
setMode(mode)
currentVisualState()
```

`currentVisualState()` 将语义层映射到当前已有动画状态：`idle / walk / sleep / talk / happy / confused / gameplay / gamewin / gamelose`。

## Rig 事件映射

当前 `core/src/agent.rs` 已定义：

```rust
pub enum AgentStreamEvent {
    Text { text: String },
    Tool { event: ToolRuntimeEvent },
}
```

第一阶段保持这个结构不变，在 app 层消费时映射成 `PetEvent`。

建议在 `core/src/pet_event.rs` 提供纯函数：

```rust
pub fn tool_event_to_pet_event(event: &ToolRuntimeEvent) -> Option<PetEvent> {
    match event.phase {
        ToolPhase::Planned => Some(PetEvent::Notify {
            kind: PetNotificationKind::ToolRunning,
            body: Some(event.label.clone()),
            ttl_ms: Some(30_000),
            refresh: true,
        }),
        ToolPhase::Blocked => Some(PetEvent::Notify {
            kind: PetNotificationKind::ToolBlocked,
            body: event.result_preview.clone(),
            ttl_ms: Some(15_000),
            refresh: true,
        }),
        ToolPhase::Failed => Some(PetEvent::Notify {
            kind: PetNotificationKind::ToolFailed,
            body: event.result_preview.clone(),
            ttl_ms: Some(15_000),
            refresh: true,
        }),
        ToolPhase::Finished => Some(PetEvent::ClearNotification {
            kind: Some(PetNotificationKind::ToolRunning),
        }),
    }
}
```

文本流映射：

```text
第一段 Text 到达 -> Notify(AiThinking, refresh=true)
后续 Text 到达 -> refresh AiThinking
最终回复完成 -> ClearNotification(AiThinking)
```

注意：文本 chunk 同时继续走 bubble 流式渲染；宠物事件只负责状态表现。

## 旧协议清理

### 删除或停止暴露

- `PetCommand::SetState`
- `PetEvent { state: Option<String>, bubble, walk_to }`
- `resolve_agent_response()` 中的关键词情绪判断
- 前端 `applyEvent(event)` 对 `event.state` 的直接处理

### 保留

- `WalkTo`：明确动作命令。
- `ShowBubble`：明确 UI 输出命令。
- `PlayDance`：现阶段继续使用现有 dance bridge，不强行纳入状态机。
- `Exit`：应用生命周期命令。
- `PetState`：作为内部动画状态保留，不再作为 AI 主链路 IPC 协议。

## AgentReaction 第二阶段

第一阶段可以先不做结构化最终反应，只在对话结束时发送：

```rust
PetEvent::React {
    mood: PetMood::Idle,
    speech: None,
}
```

第二阶段再用 Rig `Extractor<T>` 替代 `resolve_agent_response()`。

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentReaction {
    pub mood: PetMood,
    pub speech: String,
    pub memory_candidates: Vec<MemoryCandidate>,
    pub followups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemoryCandidate {
    pub text: String,
    pub importance: u8,
    pub tags: Vec<String>,
}
```

输入建议包含：

- 用户消息
- 最终回复
- 本轮工具事件摘要
- 当前模式或最近通知

抽取失败策略：

- 不做关键词 fallback。
- mood 使用 `Idle`。
- speech 使用最终回复。
- memory_candidates 为空。

## 实施步骤

### Phase 1：新协议与前端语义层（已完成）

1. 新增 `core/src/pet_event.rs`。
2. 在 `core/src/lib.rs` 导出 `pet_event`。
3. 修改 `app/src/gamepad.rs`：
   - 使用 `bitcat_core::pet_event::PetEvent`。
   - 删除本地旧 `PetEvent` 结构。
   - 替换 `commands_to_events()`。
4. 修改 `app/frontend/js/pet.js`：
   - 增加 notification/mode/reaction 字段。
   - 增加语义方法。
   - `update()` 每帧检查 notification 过期。
5. 修改 `app/frontend/js/app.js`：
   - `pet-event` listener 改为识别 `type` 字段。
   - 保留 `walk_to`、`show_bubble`、`play_dance` 的处理路径。

### Phase 2：Rig 生命周期接入（已完成）

1. 在 `chat_stream` 消费处识别第一段文本流，emit `Notify(AiThinking)`。
2. 每个 `AgentStreamEvent::Tool` 通过 `tool_event_to_pet_event()` 映射并 emit。
3. 对话结束后 clear `AiThinking` / `ToolRunning`。
4. 删除 AI 回复后的 `resolve_agent_response()` 调用。
5. 按钮映射改成语义事件：
   - AiChat -> `Notify(AiThinking)` 或仅打开输入。
   - ToggleSleep -> `SetMode(Sleep/Idle)`。
   - Praise -> `React(Happy)`。
   - Wander -> `WalkTo`。
   - PlayDance -> `PlayDance`。

### Phase 3：结构化最终反应（已完成）

1. 新增 `AgentReaction` 结构体。
2. 在独立 `agent_reaction` 模块中实现 `extract_agent_reaction()`。
3. 对话完成后调用 Extractor，生成 `React { mood, speech }`。
4. 将 `memory_candidates` 接入长期记忆候选流程。
5. 删除 `resolve_agent_response()` 及其测试。
6. 删除 `should_store()` 关键词记忆判断；长期记忆写入由 `memory_candidates` 驱动。
7. 为 `AgentReaction` 抽取增加 8 秒超时，失败时 fallback 到 `Idle`。

### Phase 4：事件总线、情绪策略与记忆审查（已完成）

1. 新增 app 层 `PetEventBus`，作为 `pet-event` IPC 的统一发送入口。
2. 在总线中集中处理短窗口去重、低优先级情绪节流和事件日志。
3. 新增 core 层 `MoodPolicy`，为 `React` 事件补默认 TTL，并管理情绪覆盖规则。
4. 前端 `PetStateMachine` 支持 `React.ttl_ms`，情绪到期后自动回到 idle 或当前语义状态。
5. `LongTermMemory` 新增 `retrieve_with()`，支持 text/tag/source/min_importance 的 grep-first 检索过滤。
6. `LongTermMemory` 新增 `review_entries()` 和 `review_markdown()`，提供可人工审查、可 grep 的长期记忆视图。

### Phase 5：可观察性与调试面板（已完成）

1. `PetEventBus` 记录最近 50 条事件决策日志。
2. 日志包含序号、相对时间、事件类型、payload、处理结果和跳过原因。
3. 新增 `cmd_get_pet_event_log` IPC，供设置页读取事件队列。
4. 设置页用量 tab 增加“宠物事件”区域，展示 sent / deduplicated / throttled / emit_failed。

### Phase 6：Rig 流式细粒度状态（已完成）

1. 新增 `AgentStreamStatus`，从 rig `MultiTurnStreamItem` 派生 `AiWriting` 和 `ToolPreparing`。
2. `StreamedAssistantContent::Text` 首次进入文本流时触发 `AiWriting`。
3. `StreamedAssistantContent::ToolCall` 到达时触发 `ToolPreparing`，随后沿用工具生命周期进入 `ToolRunning`。
4. 新增 `PetNotificationKind::AiWriting` 和 `PetNotificationKind::ToolPreparing`，前端分别映射到 `talk` / `preparing`。
5. 对话结束时统一清理 thinking / writing / preparing / running 通知。

### Phase 7：后续优化

1. 评估是否需要自定义 `PromptHook` 直接消费 `on_text_delta()` / `on_tool_call_delta()`，目前 `MultiTurnStreamItem` 已足够表达主 UI 状态。
2. `Focused` / `preparing` 专用动画帧已接入前端状态机。
3. 评估 Dance 是否纳入统一状态机。
4. 长期记忆 review markdown 已接入设置页，支持查看和人工删除。

## 测试计划

### Rust

- `pet_event` 序列化快照：用 `insta::assert_yaml_snapshot!`。
- `agent_status_to_pet_event()` 参数化测试：`AiWriting` / `ToolPreparing`。
- `tool_event_to_pet_event()` 参数化测试：
  - `Planned -> Notify(ToolRunning)`
  - `Blocked -> Notify(ToolBlocked)`
  - `Failed -> Notify(ToolFailed)`
  - `Finished -> ClearNotification(ToolRunning)`
- `commands_to_events()` 或替代函数测试：
  - `WalkTo` 保留。
  - `ShowBubble` 保留。
  - 不再产生裸 state。
- `AgentReaction` 边界测试：
  - 字段 sanitization。
  - fallback 不做关键词情绪猜测。
  - `memory_candidates` 低重要度过滤和标签规范化。
- 长期记忆测试：
  - `record_candidate()` 写入 summary/tags/importance/source。
  - `retrieve()` 可匹配 summary 和 tags。
  - `retrieve_with()` 可按 tag/source/importance 解释性过滤。
  - `review_markdown()` 输出可 grep 的审查视图。
- `MoodPolicy` 测试：默认 TTL、显式 TTL、低优先级节流、高优先级覆盖。
- `PetEventBus` 测试：React TTL 补全、短窗口重复通知去重、最近决策日志快照。

### Frontend Vitest

- notification 设置后映射到正确 visual state。
- refresh 延长生命周期。
- notification 和 reaction TTL 到期后 fallback 到 reaction 或 idle。
- `SetMode(Sleep)` 优先级高于 notification。
- `ClearNotification(kind)` 只清对应 kind。
- 旧 `state` payload 不再作为主路径测试；如保留兼容，只测 warn 和忽略。

## 代码量预估

| 范围 | 预估变更 |
|---|---:|
| Phase 1 + Phase 2 干净版 | 700-1100 行 |
| 其中新增 `pet_event.rs` | 180-280 行 |
| `gamepad.rs` 改造 | 150-250 行 |
| `pet.js` 语义层 | 150-250 行 |
| `app.js` listener | 60-120 行 |
| Rust + Vitest 测试 | 200-400 行 |
| 删除旧协议/旧测试 | -100 到 -250 行 |
| Phase 3 AgentReaction | 已完成，约 400 行 |
| Phase 4 EventBus + MoodPolicy + memory review | 已完成，约 500 行 |
| Phase 5 observability panel | 已完成，约 320 行 |
| Phase 6 rig stream statuses | 已完成，约 120 行 |

实际实现拆为多笔提交：Phase 1 新协议、Phase 2 Rig 生命周期、旧状态事件清理、Phase 3 AgentReaction + memory candidates、Phase 4 EventBus + MoodPolicy + memory review、Phase 5 observability panel、Phase 6 rig stream statuses。

## 验收标准

1. AI 主链路不再通过 `SetState` 发送 `talk/happy/confused/idle`。
2. 工具调用期间宠物会进入可刷新的 `ToolRunning` 表现。
3. 工具被阻止或失败时，宠物进入 `ToolBlocked` / `ToolFailed` 表现，并按 TTL 回落。
4. 睡眠和游戏模式不会被普通工具通知永久覆盖。
5. `make test-core` 通过。
6. `cd app/frontend && npx vitest run` 通过。
7. 手动验证一次聊天、一次工具调用、一次被阻止的 shell、一次跳舞、一次睡眠切换。
8. 对话结束后 `AgentReaction` 在 8 秒内完成或 fallback，不阻塞主回复。
9. 长期记忆不再由 `should_store()` 关键词规则写入，而由结构化 `memory_candidates` 写入。
