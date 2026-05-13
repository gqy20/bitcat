# Rig 原生能力提升路线图

> 日期：2026-05-13 | 状态：草案 | 目标：逐步引入 rig 框架未使用的高价值能力

## 背景

当前项目（8Bit Cat）使用 `rig` v0.36.0 框架构建 AI Agent，但仅覆盖了其能力子集：

| 已用 | 未用 |
|------|------|
| AgentBuilder + preamble | Extractor（结构化提取） |
| Tool trait + 10 个工具定义 | Pipeline（链式处理） |
| stream_prompt / prompt | Embeddings（明确不采用，见取舍文档） |
| PermissionHook + HookAction | dynamic_tools（动态工具集） |
| MultiTurnStreamItem 流式消费 | .context() / .dynamic_context() |
| FinalResponse.usage() | output_schema（输出约束） |

本文档按优先级排列，规划如何分阶段引入这些能力。

---

## P0: Extractor — 结构化输出替代纯文本解析

### 问题

当前 `vision.rs` 和 `screen_summary.rs` 绕过 rig，直接用 `reqwest` 调 Anthropic Messages API，返回 `Result<String, String>` 纯文本。下游消费者需要**二次解析**文本才能获得结构化数据。

```
现状：
  vision.rs ──→ reqwest POST ──→ parse_vision_response() ──→ String（自由格式文本）
  screen_summary.rs ──→ reqwest POST ──→ parse_text_response() ──→ String（自由格式文本）

问题：
  - AI 返回格式不稳定（有时用 markdown 列表、有时用自然语言）
  - 下游代码无法可靠提取字段（应用名、活动类型、时间范围等）
  - 无法做 schema 验证——格式错误只能在运行时发现
  - 绕过 rig 导致 usage 数据丢失（与 token-tracking 方案耦合）
```

### 方案

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

### 实现路径

#### Phase 1: Vision 路径改造

1. 新增 `VisionAnalysis` / `VisionState` 结构体（含 `JsonSchema` derive）
2. 创建专用 Extractor agent（独立于 PetAgent，因为 vision 不需要 10 个工具）：
   ```rust
   use rig::extractor::Extractor;

   let vision_agent = rig::agent::AgentBuilder::new(model)
       .preamble(&prompts.vision.prompt)
       .max_tokens(1024)
       .build();

   let extractor = Extractor::new(vision_agent);
   let analysis: VisionAnalysis = extractor.extract(prompt_with_image).await?;
   ```
3. `analyze_screenshot()` 返回值从 `Result<String, String>` 改为 `Result<VisionAnalysis, String>`
4. bubble 显示层取 `analysis.description`，存储层存完整结构体

#### Phase 2: Screen Summary 路径改造

1. 新增 `StructuredSummary` / `ActivityGroup` / `ActivityCategory`
2. 同样用 Extractor 替代 raw reqwest
3. `generate_summary()` 返回值改为 `Result<StructuredSummary, String>`
4. `ScreenSummaryEntry.summary` 字段可保留字符串（用于上下文注入），同时新增 `structured: Option<StructuredSummary>` 字段

#### Phase 3: 清理遗留代码

1. 删除 `parse_vision_response()` 和 `parse_text_response()`（被 Extractor 内部处理替代）
2. 删除 `send_vision_request()` 和 `generate_summary()` 中的 raw reqwest 调用
3. `build_vision_request()` / `build_vision_request_multi()` 可能仍需保留（用于构造带图片的请求体），或改用 rig 的 image content API

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
| Extractor 对图片输入的支持需验证 | rig 的 `Prompt` trait 支持 content block；若不支持图片，保留 `build_vision_request` 构造 body 后手动调用 Extractor 的底层 submit_tool 机制 |
| JsonSchema derive 与 serde 冲突 | 使用 `schemars` crate（rig 已依赖），确保 `#[serde(rename)]` 和 `#[schemars(rename)]` 一致 |
| 大模型偶尔返回不符合 schema 的 JSON | Extractor 内部有 repair 机制；极端情况降级回纯文本 + warn log |

### 依赖关系

- **依赖 token-tracking 方案（P0 并行）**：Extractor 返回的 Usage 需要写入 TokenTracker
- **不依赖 P1/P2**：可独立实施

---

## P1: 工具开销优化 — 先统计，后优化，不做关键词意图识别

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

### 方案

不做“关键词匹配式意图识别”，也不为了简单任务额外发一次小模型分类请求。当前产品方向是：**大模型足够聪明，应该让它看到能力并自行决定是否调用工具**。因此 P1 从“dynamic_tools 裁剪”调整为“工具开销优化”，并按真实统计驱动。

优先级从高到低：

1. **观测真实成本**：使用 B2 的 `cmd_get_token_stats` 和 `token_usage.jsonl` 判断工具 schema 是否真是瓶颈。
2. **压缩工具 schema**：短描述、少废话、参数字段保持明确，减少每次固定 prompt 开销。
3. **整理工具边界**：合并重复能力，删除已经过时或极少使用的工具。
4. **显式能力包**：只有用户进入某种模式时启用一组低频能力，例如开发/系统控制/内容生成模式。
5. **默认不做 dynamic_tools**：除非未来真实数据证明固定工具 schema 成为核心瓶颈，否则不引入动态工具裁剪。

### Token 节省估算

旧估算认为裁剪工具可平均节省约 250 tokens/对话，但这只是静态 prompt 估算。现在应改用真实数据：

| 数据来源 | 用途 |
|----------|------|
| `token_usage.jsonl` | 看 chat 与 vision/summary/memory 的真实占比 |
| `token_sessions.json` | 看最近会话是否有异常高消耗 |
| 设置页用量统计 | 快速判断今天是否值得优化 token |
| 未来工具调用日志 | 统计哪些工具长期没有被模型选择 |

### 实现路径

1. **先不改 AgentBuilder**：保持全量工具，避免功能回退。
2. **统计工具使用率**：在 tool call 日志里记录 `tool`、`elapsed_ms`、成功/失败，不记录大文本。
3. **压缩工具描述**：逐个审查 tool description 和 schema，删除冗余提示。
4. **清理重复工具**：如果多个工具可以由一个更清晰的工具覆盖，优先合并。
5. **评估显式模式**：例如“开发模式”启用 shell/read_file，“娱乐模式”启用 dance/game。
6. **保留能力边界**：如果未来确实需要实验 dynamic_tools，必须 feature flag 可回滚，默认仍全量。

### 风险与缓解

| 风险 | 缓解 |
|------|------|
| 过早优化导致能力退化 | 默认不裁剪工具；先压缩 schema 和观测真实数据 |
| 关键词匹配误判 | 明确禁止作为方案；简单任务交给模型自己理解 |
| 额外分类调用反而更贵 | 不做“先分类再对话”的双调用路径 |
| dynamic_tools 实验影响体验 | 必须 feature flag，可随时回退全量工具 |

### 依赖关系

- **不依赖 P0/P2**：可独立实施
- **与 P0 协同**：P0 改造后工具定义更清晰，分组更合理

---

## P2: grep-first 文本记忆检索

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

### 实施路径

1. 将对话记忆从单一滚动 JSON 扩展为 append-only `memory_entries.jsonl`。
2. 每条记录写入稳定字段：`timestamp`、`source`、`role`、`tags`、`summary`、`text_preview`。
3. 增加 `search_memory(query, since, limit)` helper，底层使用文本扫描/grep 风格匹配。
4. `build_context()` 从“只取最近 N 条”变为“最近条目 + grep 候选 + 大模型压缩后的摘要”。
5. 保持所有文件可人类阅读，避免二进制索引成为唯一真相。

### 与 rig 能力的关系

rig 的 Embeddings / Pipeline.lookup 仍然是可用能力，但当前明确不进入项目主线。更值得优先使用的是 Extractor / output schema 这类能改善结构化文本质量的能力，因为结构越干净，grep-first 检索越好用。

---

## 优先级总结与时间线

```
0.5-1.5天  P0: Extractor 结构化输出
             ├ Vision 路径改造（VisionAnalysis struct + Extractor）
             ├ Screen Summary 路径改造（StructuredSummary struct）
             └ 清理 raw reqwest 遗留代码

0.5天      P1: 工具开销优化调研
             ├ 基于 token_usage.jsonl 看真实占比
             ├ 审查工具 schema 和描述长度
             └ 决定是否需要 feature-flag 实验

1-3天      P2: grep-first 文本记忆
             ├ memory_entries.jsonl
             ├ 文本检索 helper
             ├ 最近条目 + grep 候选混合
             └ 大模型压缩候选上下文
```

### 各方案的协同效应

```
P0 (Extractor) ──→ 结构化数据质量提升 ──→ P2 文本更容易 grep
                                      ↘
P1 (工具开销优化) ──→ 降低固定 prompt 成本 ──→ 给 P2 候选上下文留预算
                                              ↘
                                            净 token 成本更可控 + 回复质量提升
```

---

## 附录：其他未用能力简述（P3 及以后）

| 能力 | rig API | 当前状态 | 未来价值 |
|------|---------|---------|---------|
| **Pipeline** | `.map().then().prompt().extract().lookup()` | 未使用 | 复杂多步推理链（如：截图→分析→决策→行动） |
| **.context(doc)** | 静态文档注入 | 未使用（用 preamble 代替） | 注入 grep-first 检索结果，与 P2 配合 |
| **.dynamic_context(fn)** | 动态上下文选择 | 未使用 | 按 topic 选择不同的知识库片段 |
| **output_schema\<T\>()** | 输出格式约束 | 未使用（Extractor 更强大） | 轻量级输出约束，无需 full Extractor |
| **Pipeline.lookup()** | 向量查找 | 明确不采用 | 当前不做向量检索 |

这些能力在 P0-P2 实施后会变得更有价值（特别是 Pipeline + P2 的组合），但不属于当前瓶颈。
