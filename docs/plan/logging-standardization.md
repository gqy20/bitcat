# 日志体系规范化设计

> **问题**：当前 222 处日志调用存在大文本污染、级别语义模糊、eprintln 绕过 tracing 三大问题，
> 导致日志可读性差、排查困难（如"对话记录为什么是空的"误判事件）。
>
> **目标**：建立统一的日志层级规范，清理历史债务，并用 `.claude/rules/logging.md` 约束后续新增日志。

---

## 一、现状诊断

### 1.1 级别分布

| 级别 | core (89处) | app (133处) | 问题 |
|------|:-----------:|:-----------:|------|
| `error!` | 0 | 7 | core 错误全靠 Result 传递，app 层兜底 |
| `warn!` | 19 | 33 | 混了"可恢复错误"+"降级提示"+"正常流程告警" |
| `info!` | 17 | **91** | 混了"生命周期事件"+**AI 回复全文(400+字符)**+用户消息原文+语音识别全文 |
| `debug!` | 29 | 6 | 混了"内部状态"+chunk 流(200次/对话) |
| `trace!` | **0** | **0** | 完全空置 |

核心矛盾：**91 个 info! 承载了跨度极大的信息，trace 完全没用**。

### 1.2 典型问题案例

| 问题 | 位置 | 影响 |
|------|------|------|
| AI 回复全文写进 info! | `gamepad.rs:857` | 单条日志 400+ 字符，日志文件 10MB/天 |
| 用户消息原文写进 info! | `gamepad.rs:478,620,768` | 敏感信息泄露 + 日志噪声 |
| 语音识别全文写进 info! | `gamepad.rs:537` | 同上 |
| chunk 逐条 debug! | `agent.rs:84`, `gamepad.rs:826` | 单次对话 200+ 条重复日志 |
| `#[instrument]` 未 skip message | `agent.rs:66` | span 名称被 2000 字符的 enriched_msg 污染 |
| 10 处 eprintln! 调试残留 | `screenshot.rs×7`, `gamepad.rs×1`, `lib.rs×1` | 绕过 tracing，release 也输出 |

### 1.3 根因分析："对话记录为空"误判事件

2026-05-12 用户报告"最近对话记录都是空的"。排查发现：

```
日志中显示：
DEBUG gamepad_loop:chat_stream{[最近对话记录]
[/最近对话记录]        ← 看起来是空的
```

**实际原因**：`#[instrument]` 自动捕获了 `message` 参数（即 2000+ 字符的 enriched_msg），
tracing 只输出了多行文本的第一行 `[最近对话记录]`。记忆系统本身正常工作。
**根本问题**：日志体系无法让人正确判断系统状态。

---

## 二、层级规范定义

### 2.1 五级日志的职责边界

```
ERROR  操作彻底失败，需要人工介入
       例：Agent 初始化失败、截图线程崩溃、配置损坏无法启动
       频率：极少（正常运行的系统应该 0 error）

WARN   操作部分成功/降级/可自动恢复的异常
       例：记忆文件损坏回退空记忆、手柄断开重连、Vision API 返回非 200
       频率：偶尔

INFO   一次业务操作的"起止标记"，一行一个事件
       关键规则：不超过 120 字符、稳定频率、可读摘要、不含大文本
       例：程序启动、对话完成(截断预览)、截图周期完成、热键触发
       频率：按用户操作频率

DEBUG  排查问题时需要的内部状态快照
       关键规则：结构化字段、描述"当前状态是什么"、按需开启
       例：memory 加载条数、上下文各部分字符数、持久化摘要
       频率：默认 core 开启、app 关闭

TRACE  高频细粒度：逐 chunk、逐帧、逐迭代
       关键规则：默认关闭、量极大、仅开发调试用
       例：stream 每个 text chunk、gamepad tick 原始按钮值
       频率：按需 `RUST_LOG=trace`
```

### 2.2 大文本处理规则

**任何超过 80 字符的动态内容，不直接写进日志消息体。** 处理方式：

```rust
// 截断预览放 fields
info!(
    chars = text.chars().count(),
    preview = %text.chars().take(60).collect::<String>(),
    "xxx complete"
);

// 禁止：
info!(text = %text, "全文: {text}");   // ✗ 大文本直接输出
info!(msg = %msg, "用户说: {msg}");    // ✗ 用户消息原文
```

### 2.3 结构化字段规范

```rust
// 推荐：纯结构化字段，消息体只是操作名
info!(model = %config.model, chars = 42, preview = %preview, "ai response complete");

// 允许：消息体包含简短可读摘要（但不超过 80 字符）
info!("程序启动，加载了 {count} 个配置");

// 禁止：消息体包含大文本变量插值
info!("AI 回复: {reply}");  // ✗ reply 可能 400+ 字符
```

---

## 三、改动清单

### P0：修复大文本污染（6 处）

| # | 文件:行 | 改前 | 改后 |
|---|---------|------|------|
| 1 | `gamepad.rs:853-857` | 两种格式不统一，else 分支输出 AI 回复全文 | 统一为截断预览 + 结构化字段 |
| 2 | `gamepad.rs:478` | `info!(msg = %msg, ...)` 用户消息原文 | `info!(msg_len, msg_preview=take(40), ...)` |
| 3 | `gamepad.rs:620` | 同上（bubble 输入入口） | 同上 |
| 4 | `gamepad.rs:768` | 同上（run_ai_chat 入口） | 同上 |
| 5 | `gamepad.rs:537` | `info!(text = %text, ...)` 语音识别全文 | `info!(text_len, text_preview=take(60), ...)` |
| 6 | `panel.rs:44` | `info!(msg = %msg, ...)` 前端日志 | `info!(msg_len, msg_preview=take(60), ...)` |

### P1：上下文可观测性 + chunk 降级（4 处）

| # | 文件:行 | 改动 |
|---|---------|------|
| 7 | `gamepad.rs` enriched_msg 构建后 | 新增 `debug!` 上下文审计：memory_ctx_chars、profile_ctx_chars 等各部分占比 |
| 8 | `agent.rs:84` + `gamepad.rs:826` | chunk `debug!` → `trace!` |
| 9 | `agent.rs:66` | `#[instrument]` 加 `skip(message)` |
| 10 | `gamepad.rs:623` | warn! 中 `msg = %msg` 截断 |

### P2：eprintln 清理 + memory 日志增强（11 处）

| # | 文件:行 | 改动 |
|---|---------|------|
| 11-17 | `screenshot.rs:294,301,304,306,313,317,332` | 7 处 eprintln → debug!/trace! |
| 18 | `lib.rs:172` | eprintln → debug! |
| 19 | `gamepad.rs:291` | eprintln → debug! |
| 20-22 | `memory.rs:175,331,409` | save() 日志增加 entries 总数 |

### 规范文档

| 文件 | 内容 |
|------|------|
| `.claude/rules/logging.md` | 五级定义、大文本规则、结构化字段规范、instrument 指南、自查清单 |

---

## 四、验证

1. `make build` — 编译通过
2. `make test` — 所有测试通过（日志改动不影响逻辑）
3. `grep -r "eprintln!" app/src/ core/src/` — 确认只剩 test 内的使用
4. `grep -r "= %reply\|= %msg\|= %text" app/src/` — 确认无全文输出
