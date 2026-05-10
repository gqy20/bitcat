# AI Agent 记忆与知识库设计方案

> 项目代号：8Bit Cat | 技术栈：Tauri 2.0 + rig-core (Rust)
> 设计日期：2026-05-10 | 方案选型：Markdown 文件记忆（仿 Claude Code 架构）

## 一、现状分析

### 当前 Agent 架构

```
PetAgent {
    agent: Agent<CompletionModel>,   // rig-core stateless executor
    config: AiConfig,                // API key / base_url / model
}
```

每次调用 `chat_stream(message)` 是**完全无状态的独立对话**：
- 只有静态 `PREAMBLE`（人设："你是 8Bit，一只像素风小猫"）+ 当条 user message
- **无历史上下文** — AI 不知道之前聊过什么
- **无持久记忆** — 重启后一切归零
- **无知识库** — `read_file` Tool 只是裸读文件，不是检索

### rig-core 的能力边界

rig-core v0.36 的 `Agent` 是纯 stateless prompt executor。不内置对话历史管理，需要调用方自己维护 message list 再拼入 prompt。

## 二、方案选型

### 为什么不用数据库

| 方案 | 适用场景 | 8Bit Cat 的数据量 |
|------|---------|------------------|
| SQLite | 结构化数据、复杂查询 | 不需要 |
| LanceDB / Qdrant | 向量检索、海量文档 | 知识库 <500 页 |
| MCP Memory Server | 多客户端共享记忆 | 单实例桌面应用 |
| **Markdown 文件** | **轻量、可读、可编辑、零依赖** | **总数据 <200KB** |

结论：**Markdown 文件就是正确选择**。和 Claude Code 用同一套方案——经过生产验证的最简解法。

### 为什么不用第三方框架

| 框架 | 问题 |
|------|------|
| Mem0 | 需要 Node.js/Python 运行时，对一个 Rust 桌面应用太重 |
| Letta (MemGPT) | 完整 agent runtime，overkill |
| Zep | Go 服务，引入外部进程依赖 |
| CrewAI Memory | 绑定 Python 框架 |

## 三、设计方案：仿 Claude Code 文件记忆

### 存储目录结构

```
~/.ai-pad/
├── memory/                          # 记忆层
│   ├── MEMORY.md                    # 索引文件（≤200行，指针而非内容）
│   ├── user_prefs.md               # 用户偏好（习惯、常用工具等）
│   ├── chat_summary.md             # 对话摘要（滚动更新，保留最近 N 轮）
│   └── facts.md                    # 提取的事实（"用户喜欢 VSCode" 等）
├── knowledge/                       # 知识库层（可选）
│   ├── README.md                   # 知识库说明
│   └── *.md                        # 用户放入的参考文档
└── sessions/                        # 原始会话记录（可选，调试用）
    └── YYYY-MM-DD.md
```

### 三层记忆架构

```
┌─────────────────────────────────────────────┐
│  Layer 1: PREAMBLE（已有）                   │
│  静态人设 + 工具定义                         │
│  "你是 8Bit，一只住在屏幕上的像素风小猫"       │
├─────────────────────────────────────────────┤
│  Layer 2: 持久记忆（新增）                    │
│  ├─ user_prefs.md  → 用户偏好               │
│  ├─ facts.md       → 提取的事实              │
│  └─ chat_summary.md → 最近对话摘要            │
│  启动时加载，拼入每次 prompt 的 system 部分     │
├─────────────────────────────────────────────┤
│  Layer 3: 会话上下文（新增）                   │
│  当前对话的最近 N 轮消息                      │
│  对话结束后压缩为摘要写入 Layer 2              │
└─────────────────────────────────────────────┘
```

### 数据流

```
启动
  │
  ▼
MemoryStore::load("~/.ai-pad/memory/")
  │  读取 MEMORY.md索引 → 定位各文件 → 解析内容
  ▼
用户按 Start 键发消息
  │
  ▼
chat_stream(user_msg + layer2_memory + layer3_context)
  │  把持久记忆+近期上下文拼入 prompt
  ▼
AI 流式回复 → 气泡显示
  │
  ▼
对话结束
  │
  ├── 提取事实 → 追加到 facts.md
  ├── 追加到 chat_summary.md（超阈值则滚动摘要压缩）
  └── 更新 MEMORY.md 索引
```

## 四、模块设计

### 新增模块

#### `core/src/memory.rs` (~280 行)

```rust
// 核心结构
pub struct MemoryStore {
    base_path: PathBuf,          // ~/.ai-pad/memory/
    index: Vec<MemoryEntry>,      // MEMORY.md 解析后的索引
    prefs: HashMap<String, String>, // user_prefs.md 内容
    facts: Vec<FactEntry>,        // facts.md 内容
    summary: ChatSummary,         // chat_summary.md 内容
}

pub struct MemoryEntry {
    topic: String,        // 如 "user_prefs", "facts", "chat_summary"
    file: String,         // 相对路径
    updated_at: DateTime, // 最后更新时间
    line_count: usize,    // 行数（用于索引控制 ≤200行）
}

// 主要接口
impl MemoryStore {
    pub fn new() -> Self;                          // 创建默认实例
    pub fn load(base_path: &Path) -> Result<Self>; // 从磁盘加载所有记忆文件
    pub fn ensure_dir(&self) -> Result<()>;        // 创建目录结构

    // 读操作 — 构建 prompt 上下文
    pub fn build_context(&self, max_chars: usize) -> String;  // 拼接所有记忆为 prompt 文本
    pub fn query_facts(&self, keyword: &str) -> Vec<&FactEntry>; // grep 搜索事实

    // 写操作 — 对话后持久化
    pub fn remember_chat(&mut self, user_msg: &str, ai_reply: &str); // 记录新对话
    pub fn add_fact(&mut self, key: &str, value: &str);            // 写入新事实
    pub fn update_preference(&mut self, key: &str, value: &str);   // 更新偏好
    pub fn rebuild_index(&mut self) -> Result<()>;                  // 重写 MEMORY.md

    // 内部
    fn summarize_if_needed(&mut self);           // 超阈值时压缩旧对话
    fn compress_messages(messages: &[Message]) -> String; // 规则摘要或调 AI 压缩
}
```

#### `core/src/knowledge.rs` (~100 行)

```rust
pub struct KnowledgeBase {
    base_path: PathBuf,          // ~/.ai-pad/knowledge/
}

impl KnowledgeBase {
    pub fn new(base_path: &Path) -> Self;
    pub fn load_all(&self) -> Result<Vec<KnowledgeDoc>>; // 扫描目录读取所有 .md
    pub fn build_prompt_section(&self, max_chars: usize) -> Option<String>;
    // <500页直接塞context；>500页未来可升级LanceDB向量检索
}
```

### 需修改的现有文件

| 文件 | 改动 |
|------|------|
| `core/src/lib.rs` | 加 `pub mod memory; pub mod knowledge;` |
| `core/src/agent.rs` | `PetAgent` 加 `memory: MemoryStore` 字段；`chat_stream()` 拼记忆上下文；对话后调 `memory.remember_chat()` |
| `core/Cargo.toml` | 可能加 `walkdir`（扫描 knowledge 目录） |
| `app/src/lib.rs` gamepad_loop | AI 回复后加一行 `agent.remember(&reply)` |

## 五、关键设计决策

### 5.1 摘要策略（分两阶段实现）

**Phase 1（当前够用）：规则滚动窗口**
- 保留最近 10 轮完整消息（user + assistant）
- 更早的消息压缩为一行摘要：`"[上午] 用户问了天气，AI 回复了晴转多云"`
- 超过 50 条摘要时合并为一段概述

**Phase 2（可选升级）：AI 压缩**
- 当积累超过阈值时，调一次 AI（用便宜模型或短 prompt）
- 输入：最近 20 条摘要 → 输出：3-5 句概括
- 成本极低（每月几次而已）

### 5.2 事实提取

初期用**简单规则**从回复中提取：
- 用户说"我喜欢 XXX" → 写入 facts.md
- 用户提到具体偏好 → 记录
- AI 回复中包含明确的事实陈述 → 记录

不需要 NER/实体识别模型，正则+关键词匹配就够了。桌宠场景的对话量很小。

### 5.3 Token 预算控制

```
PREAMBLE（固定）          ~200 tokens
用户偏好 user_prefs.md    ~100 tokens
事实 facts.md             ~150 tokens
对话摘要 chat_summary.md  ~300 tokens（滚动窗口）
知识库 knowledge/         ~2000 tokens（可选）
────────────────────────────────
总计                     ~2750 tokens

占 256K context 的 ~1%，完全可忽略
```

### 5.4 并发安全

当前架构是单线程 `gamepad_loop`（80ms tick），AI 对话通过 `rt.block_on` 顺序执行。
- 不存在并行写入问题
- 如果未来面板也触发对话，Rust 的 `Mutex<MemoryStore>` 即可
- 文件 IO 用 `std::fs::read_to_string / write` 就行，不需要 async

## 六、代码量估算

| 类别 | 文件 | 行数 |
|------|------|------|
| 生产代码 | `memory.rs` | ~280 |
| 生产代码 | `knowledge.rs` | ~100 |
| 修改现有 | `agent.rs` + `lib.rs` + `lib.rs`(app) | ~60 |
| 测试 | `memory.rs` tests | ~150 |
| 测试 | `knowledge.rs` tests | ~40 |
| 测试 | `agent.rs` 新增测试 | ~40 |
| **合计** | | **~670 行** |

## 七、演进路线图

```
Now (Phase 1)
  ├── Markdown 文件记忆 ✅ 本文档目标
  │   ├── user_prefs.md + facts.md + chat_summary.md
  │   ├── MEMORY.md 索引自动维护
  │   └── 规则滚动摘要（保留近10轮+旧轮压缩）
  │
Future (Phase 2, 按需)
  ├── AI 压缩摘要（调便宜模型做定期归档）
  ├── knowledge/ 目录支持（用户放参考文档自动加载）
  │
Later (Phase 3, 数据量大时再考虑)
  ├── LanceDB 向量检索（知识库 >500 页时）
  ├── MCP Memory Server 对接（多设备同步场景）
  └── SQLite 替换文件 IO（高频并发写入时）
```

## 八、参考资源

- [Claude Code 记忆架构分析](https://ianlpaterson.com/blog/claude-code-memory-architecture/)
- [Anthropic Contextual Retrieval](https://www.anthropic.com/engineering/contextual-retrieval)
- [官方 MCP Memory Server](https://github.com/modelcontextprotocol/servers)
- [Mem0 OpenMemory MCP](https://mem0.ai/blog/how-to-make-your-clients-more-context-aware-with-openmemory-mcp)
