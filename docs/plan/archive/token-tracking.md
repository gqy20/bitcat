# Token 追踪方案

> 日期：2026-05-13 | 状态：已落地 MVP | 目标：实现全链路 token 消耗可追踪，并把关键统计暴露给设置页

## 1. 问题定义

当前系统有 **4 条独立的 API 调用路径**，token 消耗情况如下：

| 调用路径 | 方法 | 框架/协议 | usage 可获取？ | 当前状态 |
|---------|------|----------|--------------|---------|
| AI 对话（流式） | `agent.chat_stream()` | rig Agent (stream) | ✅ `FinalResponse.usage()` | 已记录 |
| 截图视觉分析 | `vision::analyze_screenshot()` | rig Extractor | ✅ `ExtractionResponse.usage` | 已记录 |
| 屏幕摘要聚合 | `screen_summary::generate_summary()` | rig Extractor | ✅ `ExtractionResponse.usage` | 已记录 |
| 长期记忆聚合 | `memory::aggregate_profile()` | rig Extractor | ✅ `ExtractionResponse.usage` | 已记录 |

> B3（Extractor 结构化输出）主链路与 cleanup 已完成。vision / screen_summary / memory aggregation 都直接消费 rig usage；旧 raw HTTP 路径的 `parse_anthropic_usage()` 已删除。

**核心问题**：
已解决的问题：

1. chat_stream 记录 input/output/cache 明细；
2. vision / screen_summary / memory aggregation 记录 Extractor usage；
3. 明细持久化为 append-only JSONL；
4. 最近会话维护独立汇总文件；
5. 设置页可查询今日消耗、最近会话、各链路占比。

剩余问题：

1. 同步 `agent.chat()` 目前不是主要业务路径，暂未作为独立统计重点；
2. 尚未做费用计算和限额提醒；
3. 统计仍是本机文件查询，未做跨设备或长期归档。

## 2. 目标

1. **完整明细**：每次 API 调用的 input/output/cache token 都被记录
2. **零侵入**：不改变现有函数签名的行为语义（返回值不变），只追加 side-effect
3. **持久化**：append-only 行日志写入 `~/.bitcat/logs/token_usage.jsonl`
4. **汇总查询**：最近会话写入 `~/.bitcat/logs/token_sessions.json`，按日统计由 helper 读取 JSONL
5. **可扩展**：后续可对接费用计算、每日限额告警、模型路由建议

## 2.1 当前实现快照

已落地文件：

| 文件 | 作用 |
|------|------|
| `core/src/token_tracker.rs` | `TokenRecord` / `TokenSession` / `TokenTotals`、JSONL 写入、会话汇总、按日查询 |
| `core/src/agent.rs` | chat `FinalResponse.usage()` 落盘 |
| `core/src/vision.rs` | Vision Extractor usage 落盘 |
| `core/src/screen_summary.rs` | 屏幕摘要 Extractor usage 落盘 |
| `core/src/memory.rs` | 记忆聚合 Extractor usage 落盘 |
| `app/src/settings.rs` | `cmd_get_token_stats` 查询今日统计和最近会话 |
| `app/frontend/settings.html/js/css` | 设置页“用量统计”tab |

当前查询契约：

```rust
pub async fn cmd_get_token_stats() -> Result<TokenStatsView, String>
```

返回内容包括：

- `today: TokenTotals`
- `recent_sessions: Vec<TokenSessionView>`
- `paths: { usage_jsonl, sessions_json }`
- `generated_at`

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
    pub total_tokens: u64,
    pub cache_read_tokens: u64,     // cached_input_tokens
    pub cache_write_tokens: u64,    // cache_creation_input_tokens
    pub elapsed_ms: Option<u64>,
    pub extra: Option<String>,      // 扩展字段：如 tool_name, screenshot_count 等
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TokenCategory {
    Chat,           // AI 对话（含多轮 tool use）
    Vision,         // 截图视觉分析
    ScreenSummary,  // 屏幕摘要聚合
    MemoryAggregation,
}

/// 最近会话级汇总（覆盖写入 token_sessions.json）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenSession {
    pub session_id: String,
    pub started_at: String,
    pub ended_at: String,
    pub models: Vec<String>,
    pub record_count: u32,
    pub elapsed_ms_total: u64,
    pub chat_total_tokens: u64,
    pub vision_count: u32,
    pub vision_total_tokens: u64,
    pub screen_summary_count: u32,
    pub screen_summary_total_tokens: u64,
    pub memory_aggregation_count: u32,
    pub memory_aggregation_total_tokens: u64,
}
```

### 3.2 存储格式

**行日志** `~/.bitcat/logs/token_usage.jsonl`（append-only）：

```json
{"timestamp":"2026-05-13T14:30:00+08:00","session_id":"a1b2c3","category":"Chat","model":"claude-sonnet-4-20250514","input_tokens":850,"output_tokens":120,"total_tokens":970,"cache_read_tokens":800,"cache_write_tokens":0,"elapsed_ms":1234,"extra":null}
{"timestamp":"2026-05-13T14:30:02+08:00","session_id":"a1b2c3","category":"Vision","model":"claude-sonnet-4-20250514","input_tokens":3500,"output_tokens":80,"total_tokens":3580,"cache_read_tokens":0,"cache_write_tokens":0,"elapsed_ms":800,"extra":null}
```

**会话日志** `~/.bitcat/logs/token_sessions.json`（覆盖写入，仅保留最近 200 条）：

```json
{
  "sessions": [
    {
      "session_id": "a1b2c3",
      "started_at": "2026-05-13T14:29:55+08:00",
      "ended_at": "2026-05-13T14:30:05+08:00",
      "models": ["claude-sonnet-4-20250514"],
      "record_count": 2,
      "elapsed_ms_total": 2034,
      "chat_total_tokens": 970,
      "vision_count": 1,
      "vision_total_tokens": 3580,
      "screen_summary_count": 0,
      "screen_summary_total_tokens": 0,
      "memory_aggregation_count": 0,
      "memory_aggregation_total_tokens": 0
    }
  ]
}
```

### 3.3 模块关系

```
core/src/agent.rs
  └── chat_stream() FinalResponse.usage()
      └── TokenRecord(category=Chat) → token_tracker::record_token_usage()

core/src/vision.rs
  └── Extractor<VisionAnalysis>::extract()
      └── TokenRecord(category=Vision) → token_tracker::record_token_usage()

core/src/screen_summary.rs
  └── Extractor<StructuredSummary>::extract()
      └── TokenRecord(category=ScreenSummary) → token_tracker::record_token_usage()

core/src/memory.rs
  └── Extractor<ProfileAggregation>::extract()
      └── TokenRecord(category=MemoryAggregation) → token_tracker::record_token_usage()

core/src/token_tracker.rs
  ├── append token_usage.jsonl
  ├── update token_sessions.json
  └── query totals_for_date()

app/src/settings.rs
  └── cmd_get_token_stats() → 设置页统计视图
```

## 4. 改动清单

### 4.1 新增文件

| 文件 | 职责 |
|------|------|
| `core/src/token_tracker.rs` | TokenRecord / TokenSession / TokenTotals 定义 + 持久化与查询逻辑 |
| `core/src/snapshots/*_snapshot.snap` | 新增 insta 快照（TokenRecord 序列化） |

### 4.2 修改文件

#### `core/src/agent.rs`

**改动点 A — `chat_stream()` 追加 side-effect**

保持原有返回值语义不变，在 `FinalResponse` 分支读取 `usage()`，构造 `TokenRecord` 写入 JSONL，并同步更新最近会话汇总。

**改动点 B — `chat()` 同步方法**

同步 `chat()` 不是当前主业务路径，暂不作为 MVP 必须项；如果后续恢复使用，同样调用 `record_token_usage()`。

#### `core/src/vision.rs`

**改动点 C — 记录 Extractor usage**

当前调用链：`analyze_screenshot()` → rig `Extractor<VisionAnalysis>` → `VisionAnalysis` + `ExtractionResponse.usage` → `record_token_usage()`。

cleanup 已完成：旧 raw request / response helper 和旧 usage parser 测试已删除，保留 Extractor 协议的 wiremock 回归测试。

#### `core/src/screen_summary.rs`

**改动点 D — `generate_summary()` usage 记录**

与 vision 同理。`generate_summary()` 返回 `StructuredSummary`，usage 记录仍归入 `TokenCategory::ScreenSummary`。

#### `core/src/memory.rs`

**改动点 E — `aggregate_profile()` usage 记录**

`aggregate_profile()` 已使用 `Extractor<ProfileAggregation>`，usage 记录归入 `TokenCategory::MemoryAggregation`。外部返回值暂保持 `String`，用于兼容当前 `ProfileStore` 写入契约。

#### `core/src/lib.rs`（或 mod.rs）

注册 `pub mod token_tracker;`

#### `app/src/settings.rs`

新增 `cmd_get_token_stats()`，独立于配置快照读取，供设置页按需刷新 token 统计。

### 4.3 测试策略

| 测试类型 | 文件 | 内容 |
|---------|------|------|
| unit | `token_tracker.rs` tests | TokenRecord 序列化/反序列化、TokenSession 累加、按日统计 |
| unit | `vision.rs` tests | wiremock 模拟 Anthropic `tool_use` 响应，断言结构体结果和 token side-effect |
| unit | `screen_summary.rs` tests | 同上 |
| unit | `memory.rs` tests | 同上，覆盖 `ProfileAggregation` 输出 |
| unit | `settings.rs` tests | TokenSession → 设置页视图转换 |
| snapshot | snapshots/ | TokenRecord 的 insta 快照 |

## 5. 不做的事（边界）

- **不做费用计算**：不同 provider 价格差异大，留 extra 字段后续扩展
- **不改 rig 内部**：不 fork 或 patch rig 框架，只用其公开 API
- **不做实时限制**：不拦截请求做 quota 校验（留给未来需求）
- **不改现有 IPC 命令签名**：`cmd_submit_chat` 等不变，usage 是内部 side-effect
- **bubble 不显示 token 数**：设置页是统计入口，bubble 继续只负责对话体验

## 6. 实施顺序

```
Done 1: 基础设施（core/src/token_tracker.rs）
  ├ TokenRecord / TokenSession / TokenCategory / TokenTotals 定义
  ├ append_record() / update_sessions_file() / totals_for_date()
  └ 单测 + insta 快照

Done 2: API 路径打通
  ├ Chat: FinalResponse usage 记录
  ├ Vision: Extractor usage 记录
  ├ ScreenSummary: Extractor usage 记录
  └ MemoryAggregation: Extractor usage 记录

Done 3: 设置页查询
  ├ app/src/settings.rs: cmd_get_token_stats
  ├ app/frontend/settings.html/js/css: 用量统计 tab
  └ 最近会话 + 今日汇总 + 链路占比

Next:
  ├ 增加费用估算（可选，需要 provider price table）
  ├ 增加 7/30 天趋势查询
  └ 将真实统计反馈给工具 schema / context 注入优化
```

## 7. 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| 某些 proxy provider 不返回 usage 字段 | vision/summary 的 usage 为 0 | `parse_usage` 用 `unwrap_or(0)` 兜底，log warn |
| token 写盘失败（磁盘满等） | 丢一条记录，不影响主流程 | write 失败仅 warn，不向上传播 Error |
| jsonl 文件长期增长 | 占用磁盘 | 下一步增加 30 天趋势/归档策略；当前文件位于 logs 目录，可按普通日志运维 |
| 多线程并发写 jsonl | 数据交错损坏 | `token_tracker` 内部用 Mutex 保护，append 用 OpenOptions::append() |
