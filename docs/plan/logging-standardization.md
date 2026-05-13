# 日志体系规范化设计

> 状态：2026-05-13 已完成第一轮落地。本文前半部分是长期规范，后半部分的问题清单已从“待办”转为“回归检查基线”。
>
> **目标**：让日志成为可读、可检索、可长期保留的运行叙事，而不是调试残渣。
>
> 优雅的标准不是“日志越多越好”，而是：
> - 默认 INFO 能看懂产品正在做什么；
> - WARN/ERROR 只在真的需要注意时出现；
> - DEBUG 能定位状态；
> - TRACE 才承载高频细节；
> - 任何用户隐私和大文本都不裸奔。

---

## 一、设计原则

### 1.1 三层观测模型

| 层级 | 面向对象 | 默认保留 | 典型内容 |
|------|----------|----------|----------|
| **Product timeline** | 用户/开发者日常看日志 | `INFO` | 应用启动、对话开始/结束、工具调用、截图周期摘要、舞蹈播放 |
| **Diagnostic state** | 排查问题 | `DEBUG` | 上下文长度、配置来源、缓存命中、窗口位置、状态机分支 |
| **Wire detail** | 深度调试 | `TRACE` | LLM chunk、手柄 tick、截图循环细步、前端 wheel/DOM 诊断 |

默认日志应该像一条干净时间线：少、准、结构化。

### 1.2 日志不是存档

日志不承担这些职责：

- 不保存完整用户消息；
- 不保存完整 AI 回复；
- 不保存完整语音识别文本；
- 不保存 prompt 全文、memory 上下文全文、截图分析全文；
- 不代替 `~/.ai-pad/memory/`、`screenshots/`、`token_usage.jsonl` 这类结构化数据文件。

日志只记录“发生了什么、规模多大、结果如何、哪里能继续查”。

---

## 二、级别语义

### ERROR

操作彻底失败，影响核心能力，需要人工介入或用户明显感知。

例：

```rust
error!(error = %e, "AI Agent 初始化失败，后续对话将不可用");
error!(error = %e, "截图线程 Tokio 运行时创建失败");
```

### WARN

可恢复异常、降级、数据损坏回退、外部依赖失败。

不要把“正常但不常见的业务分支”放 WARN。

例：

```rust
warn!(error = %e, "读取长期记忆文件失败，使用空存储");
warn!(status = %status, "Vision API 返回错误");
warn!(dance = %name, "舞蹈不存在，无法播放");
```

### INFO

一次业务操作的开始/结束或重要状态变化。默认一行一个事件，消息体简洁，动态内容放字段。

例：

```rust
info!(model = %model, msg_chars, msg_preview = %preview, "chat started");
info!(tool = %tool_name, "tool call");
info!(dance = %name, steps, total_ms, "dance requested");
```

### DEBUG

排查问题时需要的内部状态快照。可以更细，但仍应低频、结构化。

例：

```rust
debug!(
    memory_ctx_chars,
    profile_ctx_chars,
    screen_ctx_chars,
    user_msg_chars,
    "chat context assembled"
);
```

### TRACE

高频细节，默认关闭。

例：

```rust
trace!(chunk_chars = text.chars().count(), "llm text chunk");
trace!(cycle, "screenshot loop tick");
```

---

## 三、隐私与大文本规范

### 3.1 禁止裸输出的字段

以下内容不得完整写入日志：

| 内容 | 禁止写法 | 推荐写法 |
|------|----------|----------|
| 用户消息 | `msg = %msg` | `msg_chars`, `msg_preview` |
| AI 回复 | `reply = %reply` | `reply_chars`, `reply_preview` |
| 语音文本 | `text = %text` | `voice_chars`, `voice_preview` |
| prompt / memory context | `context = %ctx` | `context_chars`, 各部分长度 |
| shell 输出 | `stdout = %stdout` | 已截断后的 tool output 或 `stdout_chars` |

### 3.2 统一预览 helper

新增一个小工具函数，建议放在 `core/src/tools.rs` 或单独 `core/src/logging.rs`：

```rust
pub fn log_preview(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push('…');
    }
    out.replace('\n', "\\n").replace('\r', "\\r")
}
```

推荐调用模式：

```rust
let preview = log_preview(msg, 60);
info!(
    msg_chars = msg.chars().count(),
    msg_preview = %preview,
    "chat started"
);
```

### 3.3 字段命名

| 类型 | 字段名 |
|------|--------|
| 字符数 | `*_chars` |
| 字节数 | `*_bytes` |
| 截断预览 | `*_preview` |
| 耗时 | `elapsed_ms` |
| 数量 | `*_count` |
| 路径 | `path = %path.display()` 或 `path = ?path` |
| 错误 | `error = %e` |
| 模型 | `model = %model` |
| 工具名 | `tool = %tool_name` |

---

## 四、Span 与 Instrument 规范

`#[instrument]` 只记录稳定、短小、非隐私的字段。

必须 skip：

```rust
#[instrument(skip(self, message, on_chunk), fields(msg_chars = message.chars().count()))]
```

禁止：

```rust
#[instrument] // 会捕获 message / enriched_msg 全文
```

推荐 span 名：

```rust
#[instrument(name = "chat_stream", skip(self, message, on_chunk), fields(msg_chars = message.chars().count()))]
```

---

## 五、落地状态与回归清单

第一轮已经完成：

- 生产代码统一走 `tracing`，避免新增 `eprintln!`；
- 用户消息、AI 回复、语音文本、前端日志使用 `log_preview()` 或长度字段；
- 高频 chunk / 截图细节降到 DEBUG/TRACE；
- Token 明细从普通日志迁移到 `~/.ai-pad/logs/token_usage.jsonl` 和 `token_sessions.json`；
- 设置页通过 `cmd_get_token_stats` 读取结构化统计，不解析日志。

下面清单保留为回归检查项：每次新增 chat / voice / screenshot / tool / settings 日志时，应确认没有重新引入全文裸写、WARN 滥用或高频 INFO。

### P0：隐私和大文本污染

| 文件 | 当前问题 | 目标 |
|------|----------|------|
| `app/src/gamepad.rs:487` | `info!(msg = %msg, "→ AI: {msg}")` | `msg_chars + msg_preview` |
| `app/src/gamepad.rs:570` | 语音识别全文 | `voice_chars + voice_preview` |
| `app/src/gamepad.rs:653` | bubble 输入全文 | `msg_chars + msg_preview` |
| `app/src/gamepad.rs:656` | Agent 未就绪时输出全文 | `msg_chars + msg_preview` |
| `app/src/gamepad.rs:805` | chat start 带全文 | `msg_chars + msg_preview` |
| `app/src/gamepad.rs:908` | AI 回复全文 | `reply_chars + reply_preview` |
| `app/src/panel.rs:44` | 前端日志全文 | `msg_chars + msg_preview` |

### P1：高频日志降级

| 文件 | 当前问题 | 目标 |
|------|----------|------|
| `core/src/agent.rs:98` | LLM chunk 用 `debug!` | 改 `trace!` |
| `app/src/gamepad.rs:877` | AI chunk 用 `debug!` | 改 `trace!` |
| `core/src/agent.rs:76` | `#[instrument]` 未 skip `message` | `skip(self, message, on_chunk)` |

### P2：调试输出归一

| 文件 | 当前问题 | 目标 |
|------|----------|------|
| `app/src/screenshot.rs` | 多处 `eprintln!("[SS-DBG] ...")` | `debug!` / `trace!` |
| `app/src/lib.rs:317` | `eprintln!` | `debug!` |
| `app/src/gamepad.rs:306` | `eprintln!` | `debug!` |

测试中的 `eprintln!` 可以保留，但应只出现在 `#[cfg(test)]` 范围。

---

## 六、推荐落地顺序

### Phase 1：建立 helper + 修 P0

1. 新增 `log_preview()`。
2. 替换用户消息、语音文本、AI 回复、前端 JS 日志全文。
3. 保持行为不变，只改日志字段。

验收：

```powershell
rg -n "msg = %msg|text = %text|reply = %reply|识别全文|回复全文" app\src core\src
```

### Phase 2：修 span 和高频日志

1. `agent.rs` 的 `#[instrument]` skip `message`。
2. LLM chunk 从 `debug!` 降到 `trace!`。
3. `gamepad.rs` enriched context 构建后增加一条 DEBUG 审计。

验收：

```powershell
rg -n "debug!\(len = .*chunk|#\[instrument\(skip\([^)]*$" app\src core\src
```

### Phase 3：清理 eprintln

1. 生产代码中的 `eprintln!` 全部替换为 tracing。
2. 高频截图循环细节使用 `trace!`。
3. 低频状态变化使用 `debug!`。

验收：

```powershell
rg -n "eprintln!" app\src core\src
```

### Phase 4：写入规则文件

新增 `.claude/rules/logging.md`，内容来自本文档的精简版：

- 级别语义；
- 禁止大文本；
- 必须使用 `log_preview()`；
- instrument skip 规则；
- PR 自查命令。

---

## 七、最终验收标准

### 静态检查

```powershell
rg -n "msg = %msg|text = %text|reply = %reply|context = %|prompt = %" app\src core\src
rg -n "eprintln!" app\src core\src
rg -n "debug!\(len = .*chunk" app\src core\src
```

期望：

- 第一条无生产代码命中；
- 第二条只允许测试代码命中；
- 第三条无命中，chunk 应为 `trace!`。

### 行为检查

手动触发：

1. 一次手柄 Start 对话；
2. 一次 bubble 输入对话；
3. 一次语音输入；
4. 一次截图分析；
5. 一次 `perform_dance`。

日志应该表现为：

- INFO 时间线能看出操作开始/结束；
- 不出现完整用户消息/AI 回复；
- DEBUG 下能看到上下文长度；
- TRACE 下才能看到 chunk 和截图循环细节。

### 测试

```powershell
cargo fmt --check
cargo test -p ai-pad-core
$env:CMAKE_POLICY_VERSION_MINIMUM="3.5"; cargo check -p ai-pad-app
```

---

## 八、后续增强

### Token 追踪衔接

B2 `token_usage.jsonl` 不应混进普通日志。日志只记录：

```rust
info!(input_tokens, output_tokens, cache_tokens, "token usage recorded");
```

完整明细写入 `~/.ai-pad/logs/token_usage.jsonl`。

### JSON 日志模式

当前 tracing 已有 JSON formatter 依赖。后续可为 release 增加可选 JSON 日志：

- human log：本地调试可读；
- jsonl log：问题上报/自动分析可用。

### 模块 target 规范

长期可按模块 target 过滤：

```text
ai_pad_core::agent=debug
ai_pad_app::screenshot=trace
ai_pad_core::memory=debug
```

这要求日志消息本身保持简短，字段保持稳定。
