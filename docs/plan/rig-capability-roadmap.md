# Rig 原生能力提升路线图

> 日期：2026-05-13 | 状态：草案 | 目标：逐步引入 rig 框架未使用的高价值能力

## 背景

当前项目（8Bit Cat）使用 `rig` v0.36.0 框架构建 AI Agent，但仅覆盖了其能力子集：

| 已用 | 未用 |
|------|------|
| AgentBuilder + preamble | Extractor（结构化提取） |
| Tool trait + 10 个工具定义 | Pipeline（链式处理） |
| stream_prompt / prompt | Embeddings（向量检索） |
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

## P1: dynamic_tools — 场景化工具裁剪

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

使用 rig 的 `.dynamic_tools(fn)` 根据用户消息动态选择工具子集：

```rust
// core/src/agent.rs — 工具注册策略

use rig::agent::ToolSet;

/// 根据消息意图推断所需工具集
fn select_tools(message: &str) -> Vec<ToolSet> {
    let msg_lower = message.to_lowercase();

    // 闲聊/问答：只给轻量工具
    if is_casual_chat(&msg_lower) {
        vec![
            ToolSet::new(GetTimeTool),
            ToolSet::new(ReadFileTool),
            ToolSet::new(ClipboardTool),
            ToolSet::new(RecentScreenshotsTool),
        ]
    }
    // 旧示意：关键词裁剪会和当前"让模型自行选择工具"的产品方向冲突。
    // 真要做 dynamic_tools，应优先用模型端或保守默认策略，而不是硬编码舞蹈关键词。
    else if msg_lower.contains("跳舞") || msg_lower.contains("舞蹈") || msg_lower.contains("dance") {
        vec![
            ToolSet::new(PerformDanceTool),
            ToolSet::new(PlayDanceTool),
            ToolSet::new(GetTimeTool),
        ]
    }
    // 默认：全量工具（保持现有行为）
    else {
        vec![
            ToolSet::new(LaunchTool),
            ToolSet::new(ShellTool),
            ToolSet::new(ReadFileTool),
            ToolSet::new(GetTimeTool),
            ToolSet::new(RecentScreenshotsTool),
            ToolSet::new(HotkeyTool),
            ToolSet::new(ClipboardTool),
            ToolSet::new(ForegroundTool),
            ToolSet::new(PerformDanceTool),
            ToolSet::new(PlayDanceTool),
        ]
    }
}

fn is_casual_chat(msg: &str) -> bool {
    let keywords = ["笑话", " joke", "故事", "天气", "几点", "时间", "你好", " hello",
                    "在吗", "你是谁", "自我介绍", "帮忙", " help"];
    keywords.any(|k| msg.contains(k))
}
```

```rust
// PetAgent::new() 中替换静态 .tool() 为动态注册

let agent = rig::agent::AgentBuilder::new(model)
    .preamble(&prompts.agent.preamble)
    .max_tokens(max_tokens)
    .hook(PermissionHook)
    .dynamic_tools(|ctx| select_tools(&ctx.prompt))  // ← 关键改动
    .build();
```

### Token 节省估算

| 场景 | 当前工具数 | 裁剪后 | 节省 Token | 占比 |
|------|-----------|--------|-----------|------|
| 闲聊（"讲个笑话"） | 10 | 4 | ~420 tok | ~55% |
| 时间查询（"几点了"） | 10 | 4 | ~420 tok | ~55% |
| 舞蹈相关（"跳个舞"） | 10 | 3 | ~530 tok | ~69% |
| 复杂任务（"帮我启动 VS Code"） | 10 | 10 | 0 | 0% |

假设日常使用中 60% 为闲聊/简单查询，平均每次对话节省约 **250 tokens**。

### 实现路径

1. **定义工具分组常量**：将 10 个工具按功能分为 `CASUAL_TOOLS`、`DANCE_TOOLS`、`FULL_TOOLSET`
2. **实现工具选择策略**：优先选择模型/上下文驱动或保守默认策略，避免把简单语义理解退化成硬编码关键词匹配
3. **修改 AgentBuilder**：`.tool(x10)` → `.dynamic_tools(fn)`
4. **添加 metrics**：记录每次对话选择的工具集大小，用于评估裁剪效果
5. **A/B 对比**：先以 feature flag 控制，对比裁剪前后回复质量

### 进阶方案（P1.5）：LLM 驱动的工具选择

硬编码匹配的局限在于无法理解复杂语义（如"帮我搞一下这个"可能需要 shell），也会和当前"由模型自行选择工具"的方向冲突。进阶方案是用一个极小的分类模型（或复用已有 agent 的单轮判断）来决定工具集：

```rust
.dynamic_tools(|ctx| {
    // 用轻量级单次调用来决定工具集（仅在高置信时裁剪）
    match classify_intent(&ctx.prompt) {
        Intent::Casual => CASUAL_TOOLS.clone(),
        Intent::Dance => DANCE_TOOLS.clone(),
        Intent::System => FULL_TOOLSET.clone(),
        Intent::Unknown => FULL_TOOLSET.clone(), // 不确定时不裁剪
    }
})
```

### 风险与缓解

| 风险 | 缓解 |
|------|------|
| 裁剪过度导致 AI 无法执行用户想要的操作 | 默认不裁剪（full set）；只在**高置信度**闲聊场景裁剪；feature flag 可随时关闭 |
| 硬编码匹配误判 | 不把关键词匹配作为主方案；优先保守默认全量工具或模型驱动分类 |
| dynamic_tools 在 rig 中的性能开销 | 工具选择是同步函数（无 I/O），开销 < 1ms；远小于节省的 token 成本 |

### 依赖关系

- **不依赖 P0/P2**：可独立实施
- **与 P0 协同**：P0 改造后工具定义更清晰，分组更合理

---

## P2: Embeddings — 语义记忆检索（RAG）

### 问题

当前记忆系统（`memory.rs`）采用**滚动窗口**策略：

```
MemoryStore:
  - max_entries: 20（最近 20 条对话）
  - max_context_chars: 1500（注入 prompt 的字符上限）
  - 策略：FIFO，新条目挤掉旧条目
  - 检索方式：取最近 N 条，无语义相关性
```

**核心缺陷**：

1. **无语义关联**：用户问"上次那个 Rust 错误怎么解决的"，系统只看最近的 20 条，如果该对话已滚出窗口就丢失
2. **固定窗口浪费**：连续闲聊占满窗口后，重要的技术讨论被挤出
3. **无法跨会话回忆**：每次重启后记忆清空（只有 chat_summary.json 的持久化摘要，非原始对话）

### 方案

引入 rig 的 `Embeddings` 能力，将滚动窗口升级为**语义向量检索**：

```
架构变化：

  现状（滚动窗口）：
    user_msg → MemoryStore.entries（最近20条）→ build_context() → 注入 prompt
                                              ↑ FIFO，无语义

  目标（语义检索 RAG）：
    user_msg → EmbeddingModel.encode(query)
                  ↓
              VectorStore.cosine_similarity(top_k=5)
                  ↓
              build_context(相关条目) → 注入 prompt
```

### 数据结构设计

```rust
// core/src/memory.rs — 新增向量索引

use rig::embeddings::{EmbeddingModel, Embeddings};
use rig::vector_store::VectorStoreIndex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,              // UUID
    pub timestamp: String,       // ISO 8601
    pub role: MemoryRole,        // User / Assistant
    pub content: String,         // 原始文本（截断后）
    pub embedding: Option<Vec<f32>>,  // 向量（懒计算/缓存）
    pub tags: Vec<String>,       // 手动标签（可选）
    pub session_id: String,      // 所属会话
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
}

/// 语义记忆存储
pub struct SemanticMemoryStore {
    entries: Vec<MemoryEntry>,
    index: Option<VectorStoreIndex<MemoryEntry>>,  // 向量索引
    embedding_model: Option<EmbeddingModel>,        // 嵌入模型
    config: MemoryConfig,
}
```

### 检索流程

```rust
impl SemanticMemoryStore {
    /// 语义检索：找到与 query 最相关的 K 条记忆
    pub async fn search(&self, query: &str, top_k: usize) -> Vec<&MemoryEntry> {
        if self.index.is_none() || self.embedding_model.is_none() {
            // 降级为滚动窗口
            return self.fallback_recent(top_k);
        }

        let model = self.embedding_model.as_ref().unwrap();
        let query_embedding = model.embed_text(query).await.unwrap();

        self.index.as_ref()
            .unwrap()
            .top_k::<CosineSimilarity>(&query_embedding, top_k)
            .await
            .unwrap_or_default()
    }

    /// 记录新对话并更新向量索引
    pub async fn record(&mut self, role: MemoryRole, content: &str, session_id: &str) {
        let entry = MemoryEntry {
            id: Uuid::new_v4().to_string(),
            timestamp: chrono::Local::now().to_rfc3339(),
            role,
            content: truncate_chars(content, self.config.max_entry_chars),
            embedding: None,  // 延迟计算
            tags: vec![],
            session_id: session_id.to_string(),
        };
        self.entries.push(entry);

        // 异步重建索引（不阻塞主流程）
        if self.entries.len() % 5 == 0 {
            self.rebuild_index().await;
        }
    }
}
```

### 存储格式变更

```
~/.ai-pad/memory/
  chat_summary.json     — 现有：滚动窗口 JSON（保留作为降级备份）
  memory_entries.jsonl  — 新增：append-only 行日志，每行一条 MemoryEntry
  memory_index.bin      — 新增：序列化的向量索引（可选，加速冷启动）
```

### Embedding 模型选择

| 模型 | 维度 | 速度 | 适用场景 | 推荐度 |
|------|------|------|---------|-------|
| `text-embedding-3-small` (OpenAI) | 1536 | 快 | 通用 | 需额外 API key |
| 本地 `all-MiniLM-L6-v2` (via `ort`/`candle`) | 384 | 极快 | 离线首选 | 高（零 API 成本） |
| rig 内置 embedding provider | 取决于 backend | — | 与 rig 生态一致 | 最高（统一认证） |

**推荐策略**：优先使用 rig 内置的 embedding provider（与 Anthropic API 共享认证）；若 rig 不支持 Anthropic embedding，退而使用 OpenAI `text-embedding-3-small` 或本地模型。

### 实施阶段

#### Phase 2.1: 基础设施

1. 新增 `SemanticMemoryStore` 结构体和基本 CRUD
2. 实现 `EmbeddingModel` wrapper（对接 rig embeddings API）
3. 向量持久化：JSONL 条目 + 内存中的 `Vec<f32>` 索引
4. 单测：embedding 维度正确性、cosine similarity 排序

#### Phase 2.2: 检索集成

1. 修改 `build_context()` 从"取最近 N 条"改为"语义搜索 top_k"
2. 保留 `max_context_chars` 截断限制
3. 添加 hybrid 策略：50% 语义相关 + 50% 最近条目（避免全新话题时返回无关旧记忆）
4. A/B 对比：feature flag 切换滚动窗口 vs 语义检索

#### Phase 2.3: 生命周期管理

1. 索引重建策略：每 N 条记录或启动时增量重建
2. 向量清理：与截图存储类似的过期策略（如 90 天）
3. 冷启动优化：首次加载时反序列化已有条目的 embedding（或 lazy compute）

### 收益

| 维度 | 改善 |
|------|------|
| 召回率 | 从"最近 20 条"变为"最相关的 K 条"，历史重要信息不会因时间流失 |
| Context 质量 | 注入 prompt 的记忆更聚焦于当前话题，减少噪声 token |
| 跨会话记忆 | 重启后仍可通过向量索引检索历史对话 |
| 可扩展性 | 向量索引天然支持 metadata filter（按时间范围/标签/会话过滤） |

### Token 影响

- **正面**：语义检索选出的 K 条更精准，`max_context_chars` 预算内信息密度更高
- **负面**：每次对话前多一次 embedding API 调用（query encoding），约消耗 ~1K tokens
- **净效果**：取决于使用频率。高频短对话可能略微增加成本；低频长对话显著降低成本（更好的 context 质量 → 更短的 AI 回复）

### 风险与缓解

| 风险 | 缓解 |
|------|------|
| Embedding API 额外成本 | 使用小维度模型（384-dim 本地模型零成本）；query embedding 缓存相似查询 |
| 向量索引构建慢（大量历史数据） | Lazy 构建：首次搜索时才对旧条目 embed；后台异步 rebuild |
| 语义漂移（同一话题不同时间的表述差异） | Hybrid 策略混合时间衰减权重；定期 re-embed 近期条目 |
| rig embeddings API 稳定性 | 抽象 EmbeddingProvider trait，支持切换 backend |

### 依赖关系

- **依赖 P0**：结构化输出后记忆条目质量更高，向量的语义更准确
- **可与 P1 并行**：工具裁剪节省的 token 可以抵消 embedding 的额外开销
- **最大改动量**：涉及 memory.rs 重构 + 新增 embedding 依赖，建议在 P0/P1 完成后实施

---

## 优先级总结与时间线

```
Week 1-2    P0: Extractor 结构化输出
             ├ Vision 路径改造（VisionAnalysis struct + Extractor）
             ├ Screen Summary 路径改造（StructuredSummary struct）
             └ 清理 raw reqwest 遗留代码

Week 3      P1: dynamic_tools 场景化裁剪
             ├ 工具分组 + 关键词意图分类
             ├ AgentBuilder 改造
             └ metrics 收集 + 效果评估

Week 4-6    P2: Embeddings 语义记忆（最大块）
             ├ SemanticMemoryStore 基础设施
             ├ EmbeddingModel 对接
             ├ 检索集成 + hybrid 策略
             └ 生命周期管理 + 冷启动优化
```

### 各方案的协同效应

```
P0 (Extractor) ──→ 结构化数据质量提升 ──→ P2 向量更准确
                                      ↘
P1 (dynamic_tools) ──→ 节省 ~250 tok/对话 ──→ 抵消 P2 embedding API 开销
                                            ↘
                                          净 token 成本降低 + 回复质量提升
```

---

## 附录：其他未用能力简述（P3 及以后）

| 能力 | rig API | 当前状态 | 未来价值 |
|------|---------|---------|---------|
| **Pipeline** | `.map().then().prompt().extract().lookup()` | 未使用 | 复杂多步推理链（如：截图→分析→决策→行动） |
| **.context(doc)** | 静态文档注入 | 未使用（用 preamble 代替） | 注入长文档/RAG 检索结果，与 P2 配合 |
| **.dynamic_context(fn)** | 动态上下文选择 | 未使用 | 按 topic 选择不同的知识库片段 |
| **output_schema\<T\>()** | 输出格式约束 | 未使用（Extractor 更强大） | 轻量级输出约束，无需 full Extractor |
| **Pipeline.lookup()** | 向量查找 | 未使用 | 与 P2 embeddings 结合的知识检索 |

这些能力在 P0-P2 实施后会变得更有价值（特别是 Pipeline + P2 的组合），但不属于当前瓶颈。
