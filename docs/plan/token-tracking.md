# Token 追踪方案

> 日期：2026-05-13 | 状态：草案 | 目标：实现全链路 token 消耗可追踪

## 1. 问题定义

当前系统有 **3 条独立的 API 调用路径**，token 消耗情况如下：

| 调用路径 | 方法 | 框架/协议 | usage 可获取？ | 当前状态 |
|---------|------|----------|--------------|---------|
| AI 对话（流式） | `agent.chat_stream()` | rig Agent (stream) | ✅ `FinalResponse.usage()` | 仅 log total_tokens 一行 |
| AI 对话（同步） | `agent.chat()` | rig Agent (sync) | ✅ 返回值含 usage | 完全未记录 |
| 截图视觉分析 | `vision::analyze_screenshot()` | raw reqwest → Anthropic API | ✅ 响应体含 `usage` 字段 | 未解析 |
| 屏幕摘要聚合 | `screen_summary::generate_summary()` | raw reqwest → Anthropic API | ✅ 响应体含 `usage` 字段 | 未解析 |

**核心问题**：
1. chat_stream 只记了 `total_tokens`，丢了 input/output/cache 明细
2. 多轮 tool call 中每轮 FinalResponse 有独立 usage，未累加
3. vision / screen_summary 绕过 rig 直接调 HTTP，usage 数据被丢弃
4. 无持久化、无汇总、无法回答"今天花了多少 token"

## 2. 目标

1. **完整明细**：每次 API 调用的 input/output/cache token 都被记录
2. **零侵入**：不改变现有函数签名的行为语义（返回值不变），只追加 side-effect
3. **持久化**：append-only 行日志写入 `~/.ai-pad/token_usage.jsonl`
4. **可扩展**：后续可对接费用计算、每日限额告警、设置面板展示

## 3. 架构设计

### 3.1 数据结构

```rust
// core/src/token_tracker.rs

/// 单次 API 调用的 token 记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecord {
    pub timestamp: String,          // ISO 8601: "2026-05-13T14:30:00+08:00"
    pub session_id: String,         // 对话批次 UUID（同一次对话的 chat + tool calls 共享）
    pub category: TokenCategory,    // 见下方枚举
    pub model: String,              // 实际使用的模型名
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,     // cached_input_tokens
    pub cache_write_tokens: u64,    // cache_creation_input_tokens
    pub extra: Option<String>,      // 扩展字段：如 tool_name, screenshot_count 等
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TokenCategory {
    Chat,           // AI 对话（含多轮 tool use）
    Vision,         // 截图视觉分析
    ScreenSummary,  // 屏幕摘要聚合
}

/// 会话级汇总（内存中累加，对话结束时写入）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionUsage {
    pub session_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub chat_input: u64,
    pub chat_output: u64,
    pub chat_cache_read: u64,
    pub vision_count: u32,
    pub vision_total_input: u64,
    pub vision_total_output: u64,
    pub summary_input: u64,
    pub summary_output: u64,
}
```

### 3.2 存储格式

**行日志** `~/.ai-pad/token_usage.jsonl`（append-only）：

```json
{"timestamp":"2026-05-13T14:30:00+08:00","session_id":"a1b2c3","category":"Chat","model":"claude-sonnet-4-20250514","input_tokens":850,"output_tokens":120,"cache_read_tokens":800,"cache_write_tokens":0,"extra":null}
{"timestamp":"2026-05-13T14:30:01+08:00","session_id":"a1b2c3","category":"Chat","model":"claude-sonnet-4-20250514","input_tokens":920,"output_tokens":45,"cache_read_tokens":810,"cache_write_tokens":0,"extra":"shell"}
{"timestamp":"2026-05-13T14:30:02+08:00","session_id":"a1b2c3","category":"Vision","model":"claude-sonnet-4-20250514","input_tokens":3500,"output_tokens":80,"cache_read_tokens":0,"cache_write_tokens":0,"extra":null}
```

**会话日志** `~/.ai-pad/token_sessions.json`（覆盖写入，仅保留最近 N 条）：

```json
{
  "sessions": [
    {
      "session_id": "a1b2c3",
      "started_at": "2026-05-13T14:29:55+08:00",
      "ended_at": "2026-05-13T14:30:05+08:00",
      "chat_input": 1770,
      "chat_output": 165,
      "chat_cache_read": 1610,
      "vision_count": 1,
      "vision_total_input": 3500,
      "vision_total_output": 80,
      "summary_input": 0,
      "summary_output": 0
    }
  ]
}
```

### 3.3 模块关系

```
app/src/gamepad.rs (run_ai_chat)
  │
  ├── agent.chat_stream() ──→ core/src/agent.rs
  │     │                      │
  │     │  FinalResponse       │  累加 Usage → TokenTracker.record()
  │     │  (每轮 tool call)     │
  │     │
  │  ← 返回 (String, SessionUsage)
  │
  ├── vision::analyze_screenshot() ──→ core/src/vision.rs
  │     │                              │  解析 response["usage"]
  │     │  ← 返回 (String, Usage)      │  → TokenTracker.record()
  │
  └── screen_summary::generate_summary() ──→ core/src/screen_summary.rs
                                    │  解析 response["usage"]
                                    │  ← 返回 (String, Usage)
                                    │  → TokenTracker.record()

core/src/token_tracker.rs  ←─ 全局单例，线程安全，负责:
                               ├ append jsonl 行
                               ├ 维护当前 SessionUsage
                               ├ 定期滚动清理（>30天）
                               └ 提供 query 接口（今日总计等）
```

## 4. 改动清单

### 4.1 新增文件

| 文件 | 职责 |
|------|------|
| `core/src/token_tracker.rs` | TokenRecord / SessionUsage 定义 + TokenTracker 单例 + 持久化逻辑 |
| `core/src/snapshots/*_snapshot.snap` | 新增 insta 快照（TokenRecord 序列化） |

### 4.2 修改文件

#### `core/src/agent.rs`

**改动点 A — `chat_stream()` 返回值增强**

```rust
// Before:
pub async fn chat_stream<F>(&self, message: &str, mut on_chunk: F) -> Result<String, String>

// After:
pub async fn chat_stream<F>(&self, message: &str, mut on_chunk: F) -> Result<(String, SessionUsage), String>
```

内部改动：
1. 创建 `SessionUsage { session_id: Uuid::new_v4(), ... }`
2. `FinalResponse` 分支：`usage += res.usage()`，明细 log
3. 流结束时 return `(accumulated, session_usage)`
4. 同时调用 `TokenTracker::instance().record(...)` 写入每轮 record

**改动点 B — `chat()` 同步方法**

同样追加 `TokenTracker.record()` 调用，log usage 明细。

#### `core/src/vision.rs`

**改动点 C — `send_vision_request()` 返回值增强**

```rust
// Before:
async fn send_vision_request(...) -> Result<String, String>

// After:
async fn send_vision_request(...) -> Result<(String, Usage), String>
```

新增 `parse_usage(response: &Value) -> Usage` 函数：
```rust
fn parse_usage(response: &Value) -> Usage {
    response.get("usage").map(|u| Usage {
        input_tokens: u["input_tokens"].as_u64().unwrap_or(0),
        output_tokens: u["output_tokens"].as_u64().unwrap_or(0),
        // ...
    }).unwrap_or_default()
}
```

调用链：`analyze_screenshot()` → `send_vision_request()` → 解析 usage → `TokenTracker.record()`

#### `core/src/screen_summary.rs`

**改动点 D — `generate_summary()` 返回值增强**

与 vision 同理。复用 `parse_usage()` 或内联解析。

#### `core/src/lib.rs`（或 mod.rs）

注册 `pub mod token_tracker;`

#### `app/src/gamepad.rs`（`run_ai_chat`）

适配新的 `(String, SessionUsage)` 返回值，取 `.0` 为文本，`.1` 为本次会话统计（可用于 bubble 尾部显示或 log）。

### 4.3 测试策略

| 测试类型 | 文件 | 内容 |
|---------|------|------|
| unit | `token_tracker.rs` tests | TokenRecord 序列化/反序列化、SessionUsage 累加、滚动清理逻辑 |
| unit | `vision.rs` tests | `parse_usage()` 正常/缺失/部分字段、wiremock 响应含 usage 的端到端 |
| unit | `screen_summary.rs` tests | 同上 |
| unit | `agent.rs` tests | `chat_stream` 返回的 SessionUsage 验证、多轮 tool call usage 累加正确性 |
| snapshot | snapshots/ | TokenRecord / SessionUsage 的 insta 快照 |

## 5. 不做的事（边界）

- **不做费用计算**：不同 provider 价格差异大，留 extra 字段后续扩展
- **不改 rig 内部**：不 fork 或 patch rig 框架，只用其公开 API
- **不做实时限制**：不拦截请求做 quota 校验（留给未来需求）
- **不改现有 IPC 命令签名**：`cmd_submit_chat` 等不变，usage 是内部 side-effect
- **bubble 不显示 token 数**（本阶段）：只落盘 + log，UI 展示作为独立 feature

## 6. 实施顺序

```
Phase 1: 基础设施（core/src/token_tracker.rs）
  ├ TokenRecord / SessionUsage / TokenCategory 定义
  ├ TokenTracker 单例（Arc<Mutex<>>）
  ├ record() / flush_session() / cleanup_old()
  └ 单测 + insta 快照

Phase 2: Chat 路径打通（core/src/agent.rs）
  ├ chat_stream() 返回 (String, SessionUsage)
  ├ FinalResponse usage 累加 + 明细 log
  ├ chat() 同步方法追加
  └ app/src/gamepad.rs 适配新返回值

Phase 3: Vision 路径打通（core/src/vision.rs）
  ├ parse_usage() 函数
  ├ send_vision_request() 返回 (String, Usage)
  ├ analyze_screenshot() 透传 usage
  └ wiremock 测试补 usage 字段

Phase 4: Summary 路径打通（core/src/screen_summary.rs）
  ├ generate_summary() 返回 (String, Usage)
  └ wiremock 测试补 usage 字段

Phase 5: 验证
  ├ make test-core 全绿
  ├ 手动 smoke test：触发一次完整对话 + 截图 + 摘要
  └ 确认 ~/.ai-pad/token_usage.jsonl 有正确内容
```

## 7. 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| 某些 proxy provider 不返回 usage 字段 | vision/summary 的 usage 为 0 | `parse_usage` 用 `unwrap_or(0)` 兜底，log warn |
| TokenTracker 写盘失败（磁盘满等） | 丢一条记录，不影响主流程 | write 失败仅 warn，不向上传播 Error |
| jsonl 文件无限增长 | 占用磁盘 | cleanup_old() 默认保留 30 天，每次启动时执行一次 |
| 多线程并发写 jsonl | 数据交错损坏 | TokenTracker 内部用 Mutex 保护，append 用 OpenOptions::append() |
