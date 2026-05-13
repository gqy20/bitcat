# Rig 原生能力提升路线图

> 日期：2026-05-13 | 状态：P0 主链路已落地，进入清理阶段 | 目标：逐步引入 rig 框架未使用的高价值能力

## 背景

当前项目（8Bit Cat）使用 `rig` v0.36.0 框架构建 AI Agent。经过 P0 改造后，chat / vision / screen_summary / memory aggregation 已经统一回到 rig 生态；剩余工作主要是清理旧的 raw Anthropic 辅助代码，并继续引入更低风险的能力。

| 已用 | 未用 |
|------|------|
| AgentBuilder + preamble | Pipeline（链式处理） |
| Tool trait + 10 个工具定义 | TypedPrompt（提示词类型化） |
| stream_prompt / prompt | Embeddings（明确不采用，见取舍文档） |
| PermissionHook + HookAction | dynamic_tools（动态工具集） |
| MultiTurnStreamItem 流式消费 | .context() / .dynamic_context() |
| PromptHook 工具生命周期钩子（部分使用） | 结构化工具事件 UI / 审计 |
| FinalResponse.usage() | output_schema（输出约束） |
| Extractor（vision / screen_summary / memory aggregation） | Pipeline.lookup（向量检索，当前不采用） |

本文档按优先级排列，规划如何分阶段引入这些能力。

---

## 当前实现快照（2026-05-13）

P0 主链路已完成：

- `vision::analyze_screenshot()` 已使用 rig `Extractor<VisionAnalysis>`；图片通过 `UserContent::image_base64()` 进入 rig message，token 用量来自 `ExtractionResponse.usage`。
- `screen_summary::generate_summary()` 已使用 rig `Extractor<StructuredSummary>`；`ScreenSummaryEntry.summary` 存储结构体，prompt 注入通过 `StructuredSummary::to_context_text()` 派生。
- `memory::aggregate_profile()` 已使用 rig `Extractor<ProfileAggregation>`；外部 API 暂保持 `String` 结果，内部不再手写 Anthropic 请求和 JSON fence 修复。
- core 侧业务主链路已经没有新的 raw Anthropic `reqwest` 调用；剩余 raw helper 主要是迁移前的请求构建、响应解析和 usage 解析测试资产。

## 调研结论：把模型从回答者提升为行为导演

这次重新查看 rig v0.36 与当前代码后，核心判断是：项目已经选对了方向，但还没有把 rig 的 agent 生命周期完整用起来。当前代码已经让模型通过 Tool Call 自主选择部分动作，例如 `perform_dance`；但外围仍有一些 Rust 侧语义猜测，例如从回复文本关键词推断宠物情绪、从关键词判断记忆是否值得保存、预先拼接较厚上下文给模型。

更充分发挥模型能力的原则应调整为：

1. **模型负责语义判断和行为编排**：是否调用工具、回复后是什么情绪、哪些信息值得记住、是否需要查看屏幕记录，应尽量通过模型结构化输出或工具调用表达。
2. **Rust 负责身体和边界**：schema、权限、校验、状态机、持久化、审计、UI 呈现继续由 Rust 控制。
3. **上下文从预塞改为按需获取**：固定注入只保留短摘要和稳定身份；屏幕记录、长期记忆、文件内容更多变成模型可调用的语义工具。
4. **生命周期事件比正文提示更重要**：工具调用不应混入正文，而应成为 `planned / started / allowed / blocked / finished / failed` 这样的独立事件。

rig 已经提供了这套架构所需的支点：

| rig 能力 | 当前用法 | 更充分的用法 |
|----------|----------|--------------|
| `AgentBuilder` + Tool | 全量注册 10 个工具 | 保持模型自主选择，同时把工具从底层 API 提升为语义能力 |
| `stream_prompt().multi_turn()` | 流式文本 + 工具调用 | 文本、工具状态、最终反应分流为独立事件 |
| `PromptHook::on_tool_call` | shell 黑名单 | 扩展为权限、审计、状态事件和异常终止控制 |
| `PromptHook::on_tool_result` | 未完整使用 | 记录成功/失败、耗时、结果摘要，并反馈 UI |
| `on_tool_call_delta` / `on_text_delta` | 未使用 | 展示更细粒度的“正在组织参数/正在思考”状态 |
| `ToolCallHookAction::{Continue, Skip, Terminate}` | Continue/Skip | 对危险、跑偏或循环工具链做可解释阻断和终止 |
| `Extractor<T>` | vision / summary / memory aggregation | 扩展到对话收尾的 `AgentReaction` / `MemoryCandidate` |
| `output_schema<T>()` | 未使用 | 作为轻量结构化输出边界，适合最终反应而非独立抽取任务 |
| `.context()` / `.dynamic_context()` | 未使用 | 与 grep-first 候选配合，减少手写大字符串拼接 |
| `dynamic_tools()` | 未使用 | 仅作为 feature flag 实验；优先用显式能力包 |

### 推荐的优雅落地点

#### 1. AgentReaction：替代关键词情绪与记忆判断

当前 `bridge::resolve_agent_response()` 通过 `"错误" / "失败" / "哈哈" / "喵"` 选择宠物状态，`memory::should_store()` 通过关键词决定是否写长期记忆。这两块都属于模型更擅长的语义判断。

建议新增对话收尾结构：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentReaction {
    pub speech: String,
    pub mood: PetMood,
    pub memory_candidates: Vec<MemoryCandidate>,
    pub followups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PetMood {
    Idle,
    Happy,
    Confused,
    Focused,
    Caring,
    Sleepy,
    Excited,
}
```

短期可以有两种实现路径：

- 继续保留主对话流式输出，结束后用一次轻量 Extractor 生成 `AgentReaction`。
- 或在主 Agent 增加一个 `set_pet_mood` / `remember` 工具，让模型在对话中主动表达行为。

第一种改动更小，第二种更像完整 agent。无论哪种，Rust 都只校验 mood 枚举、记忆字段和长度，不再猜测自然语言。

#### 2. PromptHook 作为运行时总线

`PermissionHook` 不应只是 shell 黑名单。它应该成为工具生命周期事件的单一入口：

```rust
pub enum ToolPhase {
    Planned,
    Started,
    Allowed,
    Blocked,
    Finished,
    Failed,
}
```

推荐事件字段：

- `session_id`
- `tool_name`
- `internal_call_id`
- `phase`
- `risk`
- `args_preview`
- `result_preview`
- `success`
- `elapsed_ms`
- `block_reason`

这样 bubble 可以显示安静的工具状态行，pet 可以进入表演或思考状态，日志也能统计工具成功率、失败原因和真实使用频率。模型工具链跑偏时，可以用 `Terminate` 明确终止，而不是只依赖 `MAX_AGENT_TURNS`。

#### 3. 语义工具优先于底层工具

当前工具偏底层：`shell`、`read_file`、`force_foreground`、`recent_screenshots`。这些能力仍然有价值，但为了让模型更自然地规划，应逐步补充语义工具：

| 语义工具 | 价值 |
|----------|------|
| `remember` | 让模型主动提出长期记忆候选，Rust 做去重、长度和来源校验 |
| `set_pet_mood` / `show_reaction` | 让模型直接表达情绪和动作，而不是 Rust 从文本猜 |
| `ask_user_confirmation` | 高风险动作前把确认变成显式工具协议 |
| `inspect_recent_activity` | 比 `recent_screenshots` 更贴近用户意图，内部可读截图摘要 |
| `search_memory` | grep-first 检索入口，模型按需调用并判断候选相关性 |
| `run_project_test` | 比裸 `shell` 更安全、更懂项目约定，可内部走 `make test-core` 等 |

底层 `shell` 可以继续保留作为逃生通道，但高频场景应尽量给模型更窄、更语义化、更可审计的工具。

#### 4. 上下文注入瘦身

当前 app 层会拼接用户画像、短期记忆、长期检索、最近截图、屏幕摘要后再发给模型。这个做法稳定，但会让模型被动消费大量上下文。

建议调整为：

1. 固定注入只保留用户显式画像、最近极短摘要、必要系统状态。
2. 长期记忆通过 `search_memory` 按需检索。
3. 屏幕记录通过 `inspect_recent_activity` 按需读取。
4. 文件内容通过 `read_file` / 更窄的项目工具按需获取。
5. grep-first 候选仍保持可解释文本；候选是否相关交给模型判断和压缩。

这并不违背“不做 Vector RAG”的决策。相反，结构化 JSONL / Markdown 越干净，模型越容易在 grep 候选上做高质量判断。

#### 5. dynamic_tools 不是当前主线

rig 支持 `dynamic_tools()`，但当前只有 10 个工具，固定 schema 成本还不是核心瓶颈。过早根据意图裁剪工具，反而可能削弱模型自主规划。

更稳的中间方案是显式能力包：

- 默认：聊天、时间、最近活动、舞蹈
- 开发模式：`shell`、`read_file`、`search_memory`、`run_project_test`
- 系统控制模式：`launch_program`、`send_hotkey`、`force_foreground`、`read_clipboard`
- 娱乐模式：`perform_dance`、`play_dance`、未来 `perform_game`

能力包应由用户显式模式、设置页、面板入口或长期使用数据启用；不要回到关键词意图分类。

## P0: Extractor — 结构化输出替代纯文本解析

### 问题

改造前 `vision.rs` 和 `screen_summary.rs` 绕过 rig，直接用 `reqwest` 调 Anthropic Messages API，返回 `Result<String, String>` 纯文本。下游消费者需要**二次解析**文本才能获得结构化数据。

```
改造前：
  vision.rs ──→ reqwest POST ──→ parse_vision_response() ──→ String（自由格式文本）
  screen_summary.rs ──→ reqwest POST ──→ parse_text_response() ──→ String（自由格式文本）

问题：
  - AI 返回格式不稳定（有时用 markdown 列表、有时用自然语言）
  - 下游代码无法可靠提取字段（应用名、活动类型、时间范围等）
  - 无法做 schema 验证——格式错误只能在运行时发现
  - 绕过 rig 导致 usage 数据丢失（与 token-tracking 方案耦合）
```

### 方案

采用**直接清理兼容层**的改造方式：`vision` 和 `screen_summary` 不再保留旧的 `Result<String, String>` 主接口，也不再保留旧的自由文本解析路径。新主线直接返回强类型结构体；bubble、截图存储、摘要注入等调用点同步迁移到结构字段。

优先目标是把输出边界强类型化。理想实现使用 rig 的 `Extractor<M, T>`；如果 rig v0.36 对 Anthropic 图片 content block 的 Extractor 支持不满足需求，则短期可保留 Anthropic-compatible request 构造，但必须要求模型返回 JSON 并反序列化到同一批结构体。外部 API 不因此保留旧字符串兼容层。

使用 rig 的 `Extractor<M, T>` 将输出约束为强类型结构体：

```rust
// core/src/vision.rs — 定义结构化输出目标

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VisionAnalysis {
    /// 主活动描述（一句话）
    pub description: String,
    /// 识别到的应用名称列表
    pub apps: Vec<String>,
    /// 屏幕状态
    pub state: VisionState,
    /// 是否能看清文字内容
    pub text_readable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum VisionState {
    /// 正常工作状态
    Working { app: String },
    /// 空闲/桌面
    Idle,
    /// 全屏媒体/游戏
    Media,
    /// 锁屏/黑屏
    OffScreen,
}
```

```rust
// core/src/screen_summary.rs — 定义结构化摘要输出

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StructuredSummary {
    /// 按活动类型分组的事件列表
    pub activities: Vec<ActivityGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActivityGroup {
    /// 活动类型枚举
    pub category: ActivityCategory,
    /// 时间段范围
    pub time_range: String,
    /// 具体活动描述
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivityCategory {
    Coding,
    Browsing,
    Communication,
    Entertainment,
    Documents,
    Other,
}
```

### 已完成实现

#### 改造原则

1. **不保留旧 String 主接口**：`analyze_screenshot()` 直接改为 `Result<VisionAnalysis, String>`；`generate_summary()` 直接改为 `Result<StructuredSummary, String>`。
2. **不兼容旧存储结构**：截图分析记录和屏幕摘要记录直接升级为新结构。开发期旧的 `~/.ai-pad/screenshots/` 与 screen summary 数据可清空或忽略，不写迁移器。
3. **删除旧解析函数**：`parse_vision_response()`、`parse_text_response()` 和只为兼容自由文本而存在的 fallback 不继续保留。
4. **调用点同步改造**：bubble 显示取 `analysis.description`；记忆/摘要注入使用结构体格式化后的稳定文本。
5. **测试以结构体为中心**：保留请求体快照测试，但主要断言 `VisionAnalysis` / `StructuredSummary` 字段，而不是 `String contains(...)`。

#### Phase 1: Vision 路径改造

已完成：

1. `VisionAnalysis` / `VisionState` 补齐 `JsonSchema` derive。
2. `analyze_screenshot()` 通过 rig Extractor 提交图片 message。
3. 返回值改为 `Result<VisionAnalysis, String>`。
4. app 截图观察路径使用 `analysis.description` 显示 bubble，并把结构化分析写入截图记录。

#### Phase 2: Screen Summary 路径改造

已完成：

1. 新增 `StructuredSummary` / `ActivityGroup` / `ActivityCategory`。
2. `generate_summary()` 使用 Extractor 替代 raw reqwest。
3. 返回值改为 `Result<StructuredSummary, String>`。
4. `ScreenSummaryEntry` 直接存结构化字段，prompt 注入由 `StructuredSummary::to_context_text()` 派生。

#### Phase 3: Memory Aggregation 路径改造

已完成：

1. 新增 `ProfileAggregation` 作为用户画像聚合的 Extractor 输出目标。
2. `aggregate_profile()` 通过 Extractor 获取结构化结果，再保留现有 `String` 返回契约。
3. memory aggregation 的 token 记录改为读取 `ExtractionResponse.usage`。

#### Phase 4: 清理遗留代码

已删除只为旧实现服务的代码：

1. `core/src/vision.rs`：旧 raw request / response helper 与对应测试已删除。
2. `core/src/screen_summary.rs`：旧文本解析 helper 与对应测试已删除。
3. `core/src/token_tracker.rs`：`parse_anthropic_usage()` 和对应测试已删除；Extractor 链路直接消费 rig usage。
4. `ScreenSummaryConfig::max_summary_chars`：已从 struct、默认配置、`config/prompts.yml`、快照和配置文档中移除。
5. wiremock 测试继续使用 Anthropic `tool_use` + submit 工具的 Extractor 协议，而不是旧纯文本 content block。

### 工作量预估

主链路改造和 cleanup 已完成。cleanup 的实际效果以净删除为主：

| 清理项 | 预计改动 |
|------|---------:|
| `vision.rs` 删除旧请求/解析 helper 与测试 | -80 到 -160 行 |
| `screen_summary.rs` 删除旧文本解析 helper 与测试 | -40 到 -90 行 |
| `token_tracker.rs` 删除 Anthropic response parser 与测试 | -30 到 -70 行 |
| `max_summary_chars` 配置/快照/文档清理 | -10 到 -30 行 |

净删除为主，触碰范围集中在 core 代码、提示词配置、快照和 roadmap 文档。后续风险转移到 P1 工具 schema 单一事实源。

### 收益

| 维度 | 改善 |
|------|------|
| 类型安全 | 编译期保证字段存在，不再有运行时解析 panic |
| 可靠性 | Extractor 内置 retry（默认 3 次），自动修复 JSON 格式错误 |
| Token 追踪 | 回归 rig 生态，`Extractor::extract()` 返回 Usage，与 token-tracking 方案无缝对接 |
| 可测试性 | wiremock 测试可直接断言结构体字段，无需正则匹配文本 |
| 扩展性 | 新增字段只需改 struct + 重编译，不改解析逻辑 |

### 风险与缓解

| 风险 | 缓解 |
|------|------|
| 误删测试仍依赖的旧 helper | 先用 `rg` 确认调用点，再删除测试专用旧路径 |
| 某些 proxy provider 的 Extractor/tool_use 协议不完整 | 保持 wiremock 协议测试；真实 provider 异常只记录 warn，不恢复自由文本主路径 |
| 大模型偶尔返回不符合 schema 的 JSON | Extractor 内部有 repair 机制；极端情况返回错误并记录 warn |
| 旧本地截图/摘要数据读不回 | 本阶段明确不做迁移；开发期可清空或忽略旧数据 |

### 依赖关系

- **已接入 token-tracking 方案**：Extractor 返回的 Usage 已写入 TokenTracker
- **不依赖 P1/P2**：可独立实施

---

## P1: 工具运行时与开销优化 — 先规范生命周期，再基于统计优化

### 问题

当前所有 10 个工具在每次对话时**全量注册**到 AgentBuilder，无论用户意图是什么：

```
每次对话固定开销（来自 prompt-token-budget.md）：
  Preamble:     ~45 tokens  (4.8%)
  Tools (x10):  ~767 tokens (81.6%)  ← 最大头
  ──────────────────────────────
  总计:        ~940 tokens
```

闲聊场景（"讲个笑话"、"现在几点了"）完全不需要 `launch_program`、`perform_dance`、`play_dance` 等 6 个工具，但它们仍然占用大量 context 空间。

另一个更直接的体验问题是：工具调用如果混在正文流里，生命周期就没有独立语义，无法区分执行中、成功、失败、被权限策略拦截，也无法为舞蹈/游戏这类“表演型工具”提供专门状态。当前 core/app 已经开始把 `AgentStreamEvent::Text` 与 `AgentStreamEvent::Tool` 拆开，后续应继续补齐 `started / allowed / blocked / finished / failed` 等阶段，而不是只停留在 `planned`。

### 方案

不做“关键词匹配式意图识别”，也不为了简单任务额外发一次小模型分类请求。当前产品方向是：**大模型足够聪明，应该让它看到能力并自行决定是否调用工具**。因此 P1 从“dynamic_tools 裁剪”调整为“工具运行时与开销优化”：先把工具调用生命周期规范化，再按真实统计驱动 token/schema 优化。

rig v0.36 已经提供足够的原生支点：

| rig 能力 | 用法 |
|------|------|
| `StreamedAssistantContent::ToolCall` | 模型决定调用工具时，产生 `tool_call` 和 `internal_call_id` |
| `StreamedUserContent::ToolResult` | 工具结果返回给模型时，可关联同一个 `internal_call_id` |
| `PromptHook::on_tool_call` | 工具执行前做权限、审计、`started/allowed/blocked` 事件 |
| `PromptHook::on_tool_result` | 工具执行后做 `finished/failed` 事件和耗时统计 |
| `PromptHook::on_tool_call_delta` | 可选：展示“正在组织工具参数”这类更细状态 |
| `ToolCallHookAction::{Continue, Skip, Terminate}` | 原生表达允许、拒绝、终止，并把拒绝原因返回给模型 |

统一事件建议：

```rust
pub enum ToolPhase {
    Planned,
    Started,
    Allowed,
    Blocked,
    Finished,
    Failed,
}

pub struct ToolRuntimeEvent {
    pub session_id: String,
    pub tool_name: String,
    pub label: String,
    pub kind: ToolKind,
    pub call_id: Option<String>,
    pub internal_call_id: String,
    pub phase: ToolPhase,
    pub args_preview: Option<String>,
    pub result_preview: Option<String>,
    pub success: Option<bool>,
    pub elapsed_ms: Option<u64>,
}
```

UI 分层：

- 普通工具（`shell/read_file/get_time/recent_screenshots/hotkey/clipboard/foreground`）：bubble 中显示低干扰状态行，完成后淡出或折叠。
- 表演型工具（`perform_dance/play_dance`，未来 `start_game`）：bubble 只短暂显示“正在编舞/准备开跳”，随后退场，让 pet/panel 成为主视觉。
- 被拦截工具：显示清晰但不恐吓的安全提示，并让模型继续基于 skip reason 解释或改用其他方案。

优先级从高到低：

1. **生命周期事件协议**：把工具调用从正文流中拆出，建立 `ToolRuntimeEvent`。
2. **扩展 Hook 阶段**：从 `on_tool_call` 扩展到 `on_tool_result`、`on_tool_call_delta`，必要时使用 `Terminate` 中止跑偏工具链。
3. **工具状态 UI**：bubble/pet/panel 根据 `ToolKind` 展示不同状态，尤其区分普通工具与表演型工具。
4. **工具统计**：记录次数、成功率、失败原因、耗时、被安全策略拦截次数，不记录大文本。
5. **单一事实源**：工具参数 schema 从 `Args` 类型和 `JsonSchema` derive 生成，避免 `Args` 与手写 `json!` 漂移。
6. **语义工具优先**：新增 `remember`、`set_pet_mood`、`ask_user_confirmation`、`search_memory`、`run_project_test` 等窄工具，减少模型直接依赖裸 `shell`。
7. **压缩工具 schema**：短描述、少废话、参数字段保持明确，减少每次固定 prompt 开销。
8. **整理工具边界**：合并重复能力，删除已经过时或极少使用的工具。
9. **显式能力包**：只有用户进入某种模式时启用一组低频能力，例如开发/系统控制/内容生成模式。
10. **默认不做 dynamic_tools**：除非未来真实数据证明固定工具 schema 成为核心瓶颈，否则不引入动态工具裁剪。

### Token 节省估算

旧估算认为裁剪工具可平均节省约 250 tokens/对话，但这只是静态 prompt 估算。现在应改用真实数据：

| 数据来源 | 用途 |
|----------|------|
| `token_usage.jsonl` | 看 chat 与 vision/summary/memory 的真实占比 |
| `token_sessions.json` | 看最近会话是否有异常高消耗 |
| 设置页用量统计 | 快速判断今天是否值得优化 token |
| 工具运行时事件日志 | 统计哪些工具长期没有被模型选择、耗时异常或失败率偏高 |

### 实现路径

1. **已完成：工具元信息雏形**：`ToolKind` / label 映射已进入 core，UI 不再直接暴露 `perform_dance` / `read_file` 这类内部名。
2. **已完成：升级流式回调契约**：`chat_stream<F: FnMut(&str)>` 已改为产出 `AgentStreamEvent::Text | AgentStreamEvent::Tool`，并移除了正文中的 `[正在执行: ...]`。
3. **已完成：app 层桥接事件**：新增 `bubble-tool-event`，由 `bubble.rs`/聊天循环发送到前端。
4. **已完成：bubble 工具状态 UI 雏形**：普通工具显示低干扰状态条；`perform_dance/play_dance` 作为 `performance` 类型使用不同视觉样式。
5. **已完成：工具结果阶段雏形**：`StreamUserItem::ToolResult` 已关联回计划事件并发出 `Finished / Failed`，包含 `success` 和短 `result_preview`；下一步再把 PermissionHook 的 `Allowed / Blocked` 接进同一事件流。
6. **下一步：工具事件日志**：写稳定字段，如 `session_id`、`tool`、`phase`、`success`、`elapsed_ms`、`blocked`、`error_kind`；参数和结果只写短 preview。
7. **已完成：Args → JsonSchema 单一事实源**：`LaunchArgs` / `ShellArgs` / `ReadFileArgs` / `GetTimeArgs` / `RecentScreenshotsArgs` / `HotkeyArgs` / `ClipboardArgs` / `ForegroundArgs` / `PerformDanceArgs` / `PlayDanceArgs` 已 derive `JsonSchema`，`agent.rs` 不再维护大块手写参数 JSON。
8. **已完成：类型级枚举约束**：`GetTimeArgs.format` 从自由字符串提升为 `GetTimeFormat` enum，schema 枚举和执行逻辑共用同一类型。
9. **补齐约束语义**：把舞蹈动作、步骤数量、时长范围、窗口句柄整数类型等校验逐步沉到类型/schema 层，减少仅靠描述文字约束。
10. **新增语义工具雏形**：优先做 `set_pet_mood` / `remember` / `ask_user_confirmation`，因为它们能直接替代关键词规则和隐式权限流程。
11. **压缩工具描述**：逐个审查 tool description 和字段 doc comment，删除冗余提示。
12. **清理重复工具**：如果多个工具可以由一个更清晰的工具覆盖，优先合并。
13. **评估显式模式**：例如“开发模式”启用 shell/read_file，“娱乐模式”启用 dance/game。
14. **保留能力边界**：如果未来确实需要实验 dynamic_tools，必须 feature flag 可回滚，默认仍全量。

### 风险与缓解

| 风险 | 缓解 |
|------|------|
| 过早优化导致能力退化 | 默认不裁剪工具；先压缩 schema 和观测真实数据 |
| 关键词匹配误判 | 明确禁止作为方案；简单任务交给模型自己理解 |
| 额外分类调用反而更贵 | 不做“先分类再对话”的双调用路径 |
| dynamic_tools 实验影响体验 | 必须 feature flag，可随时回退全量工具 |
| 工具事件与正文流竞态 | 事件只表达状态，不参与正文累积；前端以 `internal_call_id` 合并更新 |
| UI 过度打扰 | 默认低干扰、短暂显示；表演型工具让 bubble 退场 |

### 依赖关系

- **不依赖 P2**：可独立实施
- **与 P0 协同**：P0 改造后工具定义更清晰，分组更合理
- **与 A1/A2 协同**：舞蹈和迷你游戏提供两类真实内容型工具样本，适合验证普通工具/表演型工具的 UI 分层

---

## P1.5: AgentReaction — 让模型显式表达最终行为

### 问题

当前主对话的最终行为仍有一部分由 Rust 从自然语言里猜：

- `bridge::resolve_agent_response()` 根据关键词推断 `Happy / Confused`。
- `memory::should_store()` 根据关键词判断是否写入长期记忆。
- bubble 主要消费文本，而 pet 状态和记忆副作用分散在后处理逻辑里。

这些都不是 Rust 最擅长的事。模型已经理解了完整对话、工具调用结果和用户语气，让它显式表达“这次对话应该产生什么行为”更自然。

### 方案

新增轻量结构化收尾对象：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentReaction {
    pub speech: String,
    pub mood: PetMood,
    pub memory_candidates: Vec<MemoryCandidate>,
    pub followups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemoryCandidate {
    pub summary: String,
    pub tags: Vec<String>,
    pub source: MemorySource,
    pub confidence: f32,
}
```

实现上有两条路线：

| 路线 | 说明 | 适用阶段 |
|------|------|----------|
| 对话后 Extractor | 主对话仍流式输出；结束后用 `Extractor<AgentReaction>` 读取 user/reply/tool summary 生成结构化反应 | 改动小，适合第一版 |
| 行为工具 | 在主 Agent 注册 `set_pet_mood` / `remember` / `show_reaction`，让模型在对话中主动调用 | 更像完整 agent，适合第二版 |

第一版建议先采用“对话后 Extractor”。它不会打断当前流式体验，也能快速替换关键词规则。后续如果发现模型经常能在对话中自然决定行为，再迁移到行为工具。

### 收益

- 情绪状态从关键词匹配变成模型显式判断。
- 长期记忆从关键词表变成模型主动候选 + Rust 校验。
- 流式文本、宠物状态、记忆写入三者边界更清楚。
- 后续可支持更丰富状态：`focused`、`caring`、`excited`、`sleepy`，不必继续加关键词。

### 约束

- `speech` 仍需按字符长度截断，避免 bubble 溢出。
- `memory_candidates` 必须做长度、来源、置信度和去重校验。
- 低置信度候选只写调试日志，不进入长期记忆。
- `PetMood` 必须是枚举，不能让模型自由发明状态名。
- AgentReaction 失败时可回退到 `Idle + 原回复`，不要阻塞主对话。

---

## P2: grep-first 文本记忆检索与模型参与记忆

### 决策

不采用 Embeddings / Vector RAG。原因见：[../architecture/design-tradeoffs.md](../architecture/design-tradeoffs.md)。

当前方向是把记忆做成可 grep、可读、可 diff 的结构化文本：

```
~/.ai-pad/memory/
  chat_summary.json        — 当前滚动摘要，继续保留
  memory_entries.jsonl     — append-only 记忆条目，可 grep
  daily/YYYY-MM-DD.md      — 可选：按天沉淀的人类可读摘要
  index/tags.json          — 可选：轻量标签索引，不是向量索引
```

### 为什么这样更适合当前项目

- `rg` / grep 对本地文本记忆足够快，甚至比维护向量索引更直接。
- 记忆结果可解释：能看到命中的文件、行号、时间和来源。
- 不需要 embedding provider、本地模型、向量持久化、重建和冷启动策略。
- 大模型已经足够聪明：给它候选文本片段，它能判断哪些有用并压缩成上下文。
- 模型也足够适合做记忆候选生成：它能区分闲聊、偏好、纠正、承诺、待办和长期上下文。

### 实施路径

1. 将对话记忆从单一滚动 JSON 扩展为 append-only `memory_entries.jsonl`。
2. 每条记录写入稳定字段：`timestamp`、`source`、`tags`、`summary`、`text_preview`、`confidence`、`expires_at`。
3. 由 `AgentReaction.memory_candidates` 或 `remember` 工具提供记忆候选，Rust 负责校验、去重和落盘。
4. 增加 `search_memory(query, since, tags, limit)` helper，底层使用文本扫描/grep 风格匹配。
5. 增加 `search_memory` 工具，让模型按需查询长期记忆，而不是每次由 Rust 预塞大量上下文。
6. `build_context()` 从“只取最近 N 条”变为“极短最近摘要 + 必要用户画像”；候选上下文优先由模型主动检索。
7. 对 grep 候选再用模型做相关性判断和压缩，输出可注入的短上下文。
8. 保持所有文件可人类阅读，避免二进制索引成为唯一真相。

### 与 rig 能力的关系

rig 的 Embeddings / Pipeline.lookup 仍然是可用能力，但当前明确不进入项目主线。更值得优先使用的是 Extractor / output schema 这类能改善结构化文本质量的能力，因为结构越干净，grep-first 检索越好用。

`.context()` / `.dynamic_context()` 的思想仍然可借鉴：上下文应按需、按主题、按预算进入主对话。实现上不必依赖向量 lookup，可以用 grep-first 候选作为 dynamic context 的数据源。

---

## 优先级总结与时间线

```
已完成      P0: Extractor 结构化输出主链路
             ├ Vision 路径改造（VisionAnalysis struct + Extractor）
             ├ Screen Summary 路径改造（StructuredSummary struct）
             └ Memory Aggregation 路径改造（ProfileAggregation struct + Extractor）

已完成      P0-cleanup: 清理 raw reqwest 遗留代码
             ├ 删除旧 request/parse helper 与测试
             ├ 删除 parse_anthropic_usage
             └ 移除 max_summary_chars 惰性配置

1-2天      P1: 工具运行时与开销优化
             ├ 建立工具生命周期事件协议
             ├ 扩展 PromptHook: result / delta / terminate
             ├ bubble/pet 工具状态 UI
             ├ 工具调用统计：次数、成功率、耗时、拦截
             ├ 新增语义工具：remember / set_pet_mood / ask_confirmation
             ├ 基于真实数据压缩 schema 和描述
             └ 决定是否需要 feature-flag 实验

1天        P1.5: AgentReaction 结构化收尾
             ├ 定义 AgentReaction / PetMood / MemoryCandidate
             ├ 对话后 Extractor 生成 mood 和记忆候选
             ├ 替换 resolve_agent_response 关键词情绪判断
             └ 替换 should_store 关键词记忆判断

1-3天      P2: grep-first 文本记忆与按需检索
             ├ memory_entries.jsonl
             ├ search_memory helper + tool
             ├ 模型生成记忆候选，Rust 校验落盘
             ├ 固定上下文瘦身
             └ grep 候选 + 大模型压缩上下文
```

### 各方案的协同效应

```
P0 (Extractor) ──→ 结构化数据质量提升 ──→ P2 文本更容易 grep
                                      ↘
P1 (工具运行时) ──→ 工具事件可观测 + 语义工具更清晰 ──→ 模型更敢自主行动
                                      ↘
P1.5 (AgentReaction) ──→ 情绪/记忆从关键词转向结构化判断 ──→ P2 记忆质量提升
                                                        ↘
                                                      净 token 成本更可控 + 回复质量提升
```

---

## 附录：其他未用能力简述（P3 及以后）

| 能力 | rig API | 当前状态 | 未来价值 |
|------|---------|---------|---------|
| **Pipeline** | `.map().then().prompt().extract().lookup()` | 未使用 | 复杂多步推理链（如：截图→分析→决策→行动） |
| **.context(doc)** | 静态文档注入 | 未使用（手工拼接上下文） | 注入稳定系统上下文或 grep-first 检索结果，与 P2 配合 |
| **.dynamic_context(fn)** | 动态上下文选择 | 未使用 | 按 topic / tags / 时间范围选择 grep-first 候选 |
| **output_schema\<T\>()** | 输出格式约束 | 未使用（Extractor 更强大） | 轻量级约束最终反应，可评估用于 `AgentReaction` |
| **PromptHook result/delta** | `on_tool_result` / `on_tool_call_delta` | 部分未用 | 工具生命周期 UI、审计、失败恢复 |
| **dynamic_tools()** | 动态工具集 | 未使用 | 仅作为 feature flag 实验；默认优先显式能力包 |
| **Pipeline.lookup()** | 向量查找 | 明确不采用 | 当前不做向量检索 |

这些能力在 P0-P2 实施后会变得更有价值（特别是 Pipeline + P2 的组合），但不属于当前瓶颈。
