# Codex 监控集成计划

## 目标

让 8bit 桌宠能像看管 Claude Code 一样看管 Codex：实时显示工作状态、等待/完成/异常时主动提醒，复用现有 Agent Watch UI 和 nudge 策略。

## 现有架构速览

```
Claude Code hooks → PS 脚本 → TCP :5342 → agent_monitor.rs
  → claude_code.rs (JSON → AgentSessionEvent)
  → agent_session.rs (upsert session map)
  → agent_nudge.rs (决定是否提醒)
  → pet_event_bus.rs (宠物动画 + 气泡 + TTS)
```

关键类型：

- `AgentSource` — 事件来源（当前只有 `ClaudeCode`）
- `AgentStatus` — 8 种状态：`Idle / Working / ToolRunning / Waiting / Compacting / Done / Interrupted / Error`
- `AgentSession` — 会话快照（session_id, workspace, status, tool_name, preview 等）
- `AgentNudgePolicy` — 提醒策略（Waiting/Done/Error 各触发一次，Working 按时间门控）

## 分阶段策略

### 阶段一：Codex Hooks（MVP）

**原理**：Codex 的 hook 系统和 Claude Code 几乎同构，相同的事件名、相似的字段。只需加一个事件来源，复用整条 TCP → parser → session → nudge 链路。

**改动范围**：4 个文件新增/修改，估计 300-400 行。

#### 1.1 `core/src/agent_session.rs` — 扩展 AgentSource

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentSource {
    ClaudeCode,
    Codex,
}
```

影响点：
- `AgentSessionView::source` 从 `"Claude Code"` 衍生，Codex 应显示 `"Codex"`
- `sort_sessions()` 无需改动（排序按状态/时间，不按来源）
- nudge 文案需按来源区分

#### 1.2 `core/src/codex_hook.rs` — 新增 Codex hook parser

**源码验证结论：Codex payload 是 Claude Code 的严格超集，可复用现有 parser。** 详见下方「可行性分析」。

Codex hook JSON 全部使用 snake_case（无 `rename_all`），字段命名和 Claude Code 一致。Codex 多出的字段（`turn_id`、`model`、`permission_mode`、`transcript_path`）由 `extra: serde_json::Map` flatten 吸收。

| 字段 | Claude Code | Codex | 处理方式 |
|------|-------------|-------|----------|
| hook_event_name | snake_case | snake_case | 直接复用 `map_hook_status` |
| session_id | snake_case | snake_case | 直接复用 |
| turn_id | 无 | snake_case | extra 吸收 |
| tool_name | snake_case | snake_case | 直接复用 |
| tool_input | snake_case | snake_case | 直接复用 |
| prompt | 同 | 同 | 直接复用 |
| last_assistant_message | snake_case | snake_case | 直接复用 |
| cwd | 同 | 同 | 直接复用 |
| model | 无 | snake_case | extra 吸收（可选提取） |
| permission_mode | 无 | snake_case | extra 吸收 |

```rust
// 源码验证后：不需要单独的 CodexHookEvent。
// Codex payload 是 Claude Code 的超集（snake_case 完全一致），
// 直接扩展现有 ClaudeHookEvent 即可：
//   - 加 Optional 字段：turn_id, model
//   - into_session_event() 接受 source: AgentSource 参数
//   - extra: Map flatten 吸收剩余差异字段
```

状态映射与 Claude Code 完全一致（Codex hook 事件名相同）：

| Hook 事件 | AgentStatus |
|-----------|-------------|
| `SessionStart` | `Idle` |
| `UserPromptSubmit` | `Working` |
| `PreToolUse` | `ToolRunning` |
| `PostToolUse` | `Working` |
| `PermissionRequest` | `Waiting` |
| `PreCompact` | `Compacting` |
| `PostCompact` | `Working` |
| `Stop` | `Done` |

可以抽 `map_hook_status()` 为 `pub` 函数让两个 parser 共用。

#### 1.3 `app/src/agent_monitor.rs` — 区分来源

`handle_hook_payload` 需要区分 Claude 和 Codex 的 JSON。两种方案：

**方案 A（推荐）：Envelope 包裹**
Codex hook 脚本在原始 JSON 外包一层 `{ "source": "codex", "payload": {...} }`。
`handle_hook_payload` 先检查是否有 `source` 字段，有则分发到对应 parser。

```rust
fn handle_hook_payload(app: &AppHandle, raw: &str) -> Result<(), String> {
    let event = if let Ok(envelope) = serde_json::from_str::<Envelope>(raw) {
        match envelope.source.as_str() {
            "codex" => CodexHookEvent::from_json(&envelope.payload)?.into_session_event(now_ms)?,
            _ => ClaudeHookEvent::from_json(raw)?.into_session_event(now_ms)?,
        }
    } else {
        // 兼容旧格式（Claude Code 直传，无 envelope）
        ClaudeHookEvent::from_json(raw)?.into_session_event(now_ms)?
    };
    // ... 后续逻辑不变
}
```

优点：无需解析整个 JSON 判断来源，不依赖字段差异。向后兼容。

**方案 B：按字段探测**
检查 JSON 是否含 `model` 且无 `tool_name` 的 snake_case 形式。脆弱，不推荐。

#### 1.4 `app/src/codex_hooks.rs` — Codex hook 安装

与 `claude_hooks.rs` 平行，负责：

1. 写 PowerShell 脚本 `~/.codex/hooks/ai-pad-codex-hook.ps1`
   - 读取 stdin，包装成 `{ "source": "codex", "payload": <原始JSON> }`
   - TCP 发送到 `127.0.0.1:5342`（复用同一个端口）
2. 写 Codex 配置文件

Codex 的 hook 配置格式（基于 `codex-rs/hooks/src/schema.rs` 的测试）：

```toml
# ~/.codex/config.toml
[hooks]

[[hooks.PreToolUse]]
matcher = "*"

[[hooks.PreToolUse.hooks]]
type = "command"
command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File '$HOME/.codex/hooks/ai-pad-codex-hook.ps1'"

# ... 类似注册所有事件
```

需要确认 Codex 的 config.toml 路径和 hook 注册格式的稳定性。当前 Codex 用的是 `~/.codex/config.toml`，但可能随版本变化。

#### 1.5 `core/src/agent_nudge.rs` — 文案按来源区分

把硬编码的 `"Claude Code 需要你处理一下"` 改成按 `AgentSource` 动态生成：

```rust
fn nudge_message(kind: &AgentNudgeKind, source: &AgentSource) -> String {
    let name = match source {
        AgentSource::ClaudeCode => "Claude Code",
        AgentSource::Codex => "Codex",
    };
    match kind {
        AgentNudgeKind::WaitingForUser => format!("{name} 需要你处理一下。"),
        AgentNudgeKind::TaskDone => "这轮完成了，可以回来看看。".into(),
        AgentNudgeKind::TaskError => format!("{name} 这轮遇到异常了。"),
        AgentNudgeKind::AwayWhileWorking => "我帮你盯着，你可以先去做点别的。".into(),
    }
}
```

#### 1.6 前端 Agent Watch UI

`AgentSessionView` 已包含 `source` 字段（JSON 序列化为字符串）。前端显示时在会话卡片上加来源标签即可：
- Claude Code 会话显示 "Claude Code" 标签（保持现状）
- Codex 会话显示 "Codex" 标签

改动极小，可能只是 CSS 差异（不同颜色的 tag）。

### 阶段二：Codex App Server 事件流（增强）

**原理**：Codex 的 `app-server` 提供更细粒度的事件流（turn/item 级别），可以实现：
- 实时显示当前正在执行的工具名和命令
- 审批按钮（在 Agent Watch 面板内直接 approve/deny）
- Token 使用统计
- 多线程（多个 Codex 会话）统一看管

**集成方式**：连接 Codex app-server 的 JSON-RPC 端点，订阅 thread/turn/item 事件。

#### 2.1 事件映射

Codex TUI 自己的 pet 只有 4 个状态（`Running / Waiting / Review / Failed`），但 app-server 事件更丰富：

| App Server 事件 | 8bit AgentStatus | 补充信息 |
|----------------|------------------|----------|
| `TurnStarted` | `Working` | turn_id, input preview |
| `ItemStarted(CommandExecution)` | `ToolRunning` | command preview |
| `ItemStarted(FileChange)` | `ToolRunning` | file paths |
| `ItemStarted(McpToolCall)` | `ToolRunning` | tool name |
| `ItemCompleted(*)` | `Working` | 恢复 |
| `CommandExecutionRequestApproval` | `Waiting` | command detail |
| `FileChangeRequestApproval` | `Waiting` | file detail |
| `DynamicToolCall` (需审批) | `Waiting` | tool/args |
| `TurnCompleted(Completed)` | `Done` | last message preview |
| `TurnCompleted(Failed)` | `Error` | error detail |
| `TurnCompleted(Interrupted)` | `Interrupted` | reason |
| `ContextCompaction` item | `Compacting` | - |

#### 2.2 架构扩展

```
Codex App Server (JSON-RPC over stdio/TCP)
  → app/src/codex_app_server.rs (连接管理 + 事件路由)
  → core/src/codex_events.rs (app-server 事件 → AgentSessionEvent)
  → 复用 agent_session / agent_nudge / pet_event_bus
```

关键设计决策：
- **连接管理**：需要启动/发现 Codex app-server 进程，维护 JSON-RPC 连接
- **Thread → Session 映射**：每个 Codex thread 对应一个 AgentSession
- **审批交互**：需要在 Agent Watch 面板加 approve/deny 按钮，通过 JSON-RPC 回调
- **断线重连**：app-server 可能随时退出/重启

这部分成本显著高于 hooks，建议在阶段一稳定后再启动。

#### 2.3 Codex TUI Pet 的设计启示

Codex TUI 的 pet 非常克制——只表达 4 个高层语义状态，不承担事件日志职责。具体来说：
- **Running**：turn 开始就设，不管内部跑什么工具（TTL 3 分钟）
- **Waiting**：所有需要用户处理的场景（命令审批、文件审批、MCP elicitation、用户输入）
- **Review**：turn 完成（TTL 7 天，等用户查看）
- **Failed**：非重试错误

这与 8bit 的 `AgentStatus` 压缩思路一致。阶段一可以直接沿用 Codex 的状态压缩比例，不用把每个 item 都映射成宠物事件。

## 实施步骤（阶段一）

### Step 1: core 层扩展
- [ ] `core/src/agent_session.rs`：`AgentSource` 加 `Codex` 变体
- [ ] `core/src/claude_code.rs`：`ClaudeHookEvent` 加 `turn_id`/`model` 可选字段，`into_session_event` 接受 `source: AgentSource`，把 `map_hook_status` 提为 `pub`
- [ ] ~~新增 `core/src/codex_hook.rs`~~（源码验证后：不需要，复用 `claude_code.rs`）
- [x] `core/src/agent_nudge.rs`：nudge 文案按 `AgentSource` 区分

### Step 2: app 层扩展
- [x] `app/src/agent_monitor.rs`：`handle_hook_payload` 支持 envelope 区分来源
- [x] 新增 `app/src/codex_hooks.rs`：Codex hook 脚本生成 + config.toml 写入
- [x] 注册 Tauri 命令：`cmd_install_codex_hooks`
- [x] 更新 `app/src/lib.rs`：注册新模块和命令

### Step 3: 测试
- [ ] `core/src/claude_code.rs`：新增 Codex 风格 fixture 测试（含 `turn_id`、`model` 等超集字段）
- [ ] `core/src/agent_session.rs`：更新测试覆盖 `Codex` 来源的 session view
- [x] `core/src/agent_nudge.rs`：更新测试验证 Codex nudge 文案
- [x] `app/src/agent_monitor.rs`：集成测试验证 envelope 路由 + Codex source

### Step 4: 前端
- [x] Agent Watch 面板：来源标签显示（`CC` / `CX` 徽标），done/waiting 优先置顶
- [x] 设置页：Codex hook 修复按钮，复用 Hook Doctor 语义

### Step 5: 文档
- [ ] 更新 `CLAUDE.md` 架构部分：标注 Codex 监控链路
- [ ] 模块 `//!` 文档

## 源码验证后的可行性分析

### 结论：阶段一完全可行，需要修正几个原计划假设

---

### 发现 1：Codex hook JSON 全部是 snake_case（原计划误判为 camelCase）

**源码证据**：`codex-rs/hooks/src/schema.rs` 所有 `*CommandInput` 结构体均无 `#[serde(rename_all = ...)]`。

```rust
// schema.rs:242 — 无 rename_all，字段原样序列化
pub(crate) struct PreToolUseCommandInput {
    pub session_id: String,       // → JSON: "session_id"
    pub turn_id: String,          // → JSON: "turn_id"
    pub hook_event_name: String,  // → JSON: "hook_event_name"
    pub tool_name: String,        // → JSON: "tool_name"
    pub tool_input: Value,        // → JSON: "tool_input"
    // ...
}
```

**影响**：原计划 1.2 的字段映射表错误。Codex 的字段和 Claude Code 一样是 snake_case，不需要 camelCase alias 处理。实际上 **Codex hook payload 和 Claude Code 的差异比预期更小**。

修正后的字段对比：

| 字段 | Claude Code | Codex | 差异 |
|------|-------------|-------|------|
| `session_id` | snake_case | snake_case | 无 |
| `hook_event_name` | snake_case | snake_case | 无 |
| `tool_name` | snake_case | snake_case | 无 |
| `tool_input` | snake_case | snake_case | 无 |
| `cwd` | 同 | 同 | 无 |
| `prompt` | 同 | 同 | 无 |
| `last_assistant_message` | 同 | 同 | 无 |
| `turn_id` | **无** | **有** | Codex 独有（非 SessionStart） |
| `model` | **无** | **有** | Codex 独有（所有事件） |
| `permission_mode` | **无** | **有** | Codex 独有（大多数事件） |
| `stop_hook_active` | **无** | **有** | Codex 独有（Stop 事件） |
| `transcript_path` | **无** | **有** | Codex 独有（可为 null） |
| `tool_response` | **无** | **有** | Codex 独有（PostToolUse） |

**可行度：高**。Codex payload 是 Claude Code 的严格超集，用 `#[serde(flatten)] extra: Map` 吸收多出的字段即可，`claude_code.rs` 的 `ClaudeHookEvent` 稍作调整甚至可以直接复用。

---

### 发现 2：不需要单独的 `codex_hook.rs` — 可以统一 parser

由于字段命名和事件名完全一致，Codex hook JSON 可以被现有的 `ClaudeHookEvent::from_json()` 直接解析。多出的 `turn_id`、`model`、`permission_mode` 等字段会被 `extra: serde_json::Map`（flatten）自动吸收。

**简化方案**：不再新增 `codex_hook.rs`，而是在 `claude_code.rs` 的 `ClaudeHookEvent` 基础上：
- 把 `extra` Map 里的 `turn_id`、`model` 提到具名字段（Optional）
- `into_session_event()` 接受 `source: AgentSource` 参数
- 重命名为更通用的名字（如 `AgentHookEvent`）或保留原名加文档说明

**可行度：高**。这比原计划少一个文件。

---

### 发现 3：Windows 下 hook 命令通过 cmd.exe 执行，需要 commandWindows 字段

**源码证据**：`codex-rs/hooks/src/engine/command_runner.rs:119-135`

```rust
#[cfg(windows)]
{
    let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
    let mut command = Command::new(comspec);
    command.arg("/C");
    command
}
```

Codex 在 Windows 下用 `cmd.exe /C <command>` 执行 hook 命令。`HookHandlerConfig` 支持 `commandWindows` 字段：

```rust
// hook_config.rs:129
pub command: String,
pub command_windows: Option<String>,  // Windows 覆盖
```

**影响**：hook 配置必须设置 `commandWindows`，因为 `cmd.exe /C powershell -File script.ps1` 可以工作，但直接写 PowerShell 语法不行。

修正后的 TOML 配置：

```toml
[[hooks.PreToolUse]]
matcher = "*"

[[hooks.PreToolUse.hooks]]
type = "command"
command = "powershell -NoProfile -ExecutionPolicy Bypass -File '$HOME/.codex/hooks/ai-pad-codex-hook.ps1'"
commandWindows = "powershell -NoProfile -ExecutionPolicy Bypass -File \"%USERPROFILE%\\.codex\\hooks\\ai-pad-codex-hook.ps1\""
timeout = 60
```

**可行度：高**。`commandWindows` 字段已经存在，不是 hack。

---

### 发现 4：Hook 信任模型 — 新 hook 默认 Untrusted，不会执行

**源码证据**：`codex-rs/hooks/src/engine/discovery.rs:488-491`

```rust
// 只有 Managed 和 Trusted 的 hook 才会执行
let should_run = matches!(trust, HookTrust::Managed | HookTrust::Trusted);
```

新写入的 hook 条目没有 trust 记录，初始状态为 `Untrusted`，**不会被执行**。用户需要手动在 Codex 里信任这个 hook。

**影响**：这是最大的用户体验障碍。安装后 hook 不生效，用户需要：
1. 启动 Codex
2. Codex 检测到新 hook，提示用户信任
3. 用户确认信任后 hook 才生效

或者研究是否有编程方式设置 trust（需要进一步确认 `[hooks.state]` 的写入方式）。

### 发现 4.1：安装入口已升级为 Hook Doctor

当前设置页的 Codex 按钮不再只是追加配置，而是执行可重复的检查与修复：

- 只清理带 `ai_pad_marker = "ai-pad-codex-watch"` 的 8Bit Cat hook；
- 保留用户或其他工具写入的 hook，即使它们在同一个 event/matcher 下；
- 旧版本生成的重复 ai-pad hook 会先移除，再写入当前标准 hook；
- 旧的无效事件名下如果只剩 ai-pad hook，会被移除；
- PowerShell 脚本内容变化时会重写脚本；
- 写入 `config.toml` 前仍保留备份。

Hook Doctor 只解决配置一致性，不绕过 Codex 的 hook trust 流程。VS Code/Codex 插件已启动时，通常还需要 reload/restart 后重新读取配置。

**可行度：中**。功能上完全可行，但需要设计好安装引导流程，让用户知道需要手动信任。

---

### 发现 5：Codex config 路径确认 — `$CODEX_HOME/config.toml`，默认 `~/.codex/`

**源码证据**：
- `codex-rs/utils/home-dir/src/lib.rs:13`：`$CODEX_HOME || ~/.codex`
- `codex-rs/config/src/lib.rs:28`：`CONFIG_TOML_FILE = "config.toml"`
- `codex-rs/config/src/loader/mod.rs`：配置层级从低到高：System → User → CWD → Project → Runtime

**Hook 配置可写在两个位置**（discovery.rs:109-111）：
1. `$CODEX_HOME/config.toml` 的 `[hooks]` 段（用户层）
2. `$CODEX_HOME/hooks.json`（用户层）

两者可共存但有 warning。选择 `config.toml` 更一致。

**可行度：高**。路径稳定，多层配置有优先级保证。

---

### 发现 6：阶段二的 App Server 连接方式明确

**源码证据**：
- Transport 支持：`Stdio` / `UnixSocket` / `WebSocket` / `Off`（`codex-rs/app-server/src/transport.rs:15-28`）
- TUI 默认用 `InProcess`（stdio 嵌入）；远程连接走 WebSocket
- 启动参数 `--listen ws://0.0.0.0:8080` 暴露 WebSocket 端点
- JSON-RPC 2.0 协议，60+ 通知类型，9 种 ServerRequest

**对于 8bit 集成**：8bit 作为外部进程，需要连接已运行的 Codex app-server。最佳路径：
1. 检测 Codex app-server 是否运行（尝试 WebSocket 连接）
2. 发送 `Initialize` 握手
3. 订阅 `TurnStarted` / `ItemStarted` / `TurnCompleted` 等通知
4. 审批请求通过 `resolve_server_request()` JSON-RPC 回调

**可行度：中**。协议清晰但实现复杂度高（JSON-RPC 客户端、连接管理、重连、thread 映射）。建议阶段一稳定后再评估。

---

### 发现 7：Envelope 方案仍然是最优的路由策略

原计划的方案 A（envelope 包裹）仍然是最干净的。但有了发现 2（统一 parser）之后，envelope 可以更轻量：

```rust
fn handle_hook_payload(app: &AppHandle, raw: &str) -> Result<(), String> {
    // 尝试解析 envelope
    let (source, inner_json) = if let Ok(env) = serde_json::from_str::<Envelope>(raw) {
        (env.source, env.payload)
    } else {
        // 无 envelope = Claude Code 直传
        (AgentSource::ClaudeCode, raw.to_string())
    };
    // 统一 parser，仅 source 不同
    let event = ClaudeHookEvent::from_json(&inner_json)?
        .into_session_event(now_ms, source)?;
    // ...
}
```

---

### 修正后的风险评估

| 风险 | 原评估 | 源码验证后 | 缓解措施 |
|------|--------|-----------|---------|
| 字段格式不兼容 | 中 | **低** | snake_case 完全一致，Codex 是超集 |
| Windows hook 执行 | 中 | **低** | `commandWindows` 字段原生支持 |
| Config 路径不稳定 | 中 | **低** | `$CODEX_HOME/config.toml` 稳定 |
| Hook 信任机制 | 未识别 | **中** | 需用户手动信任或研究 `[hooks.state]` |
| 端口并发 | 低 | **低** | Envelope 区分，实测无问题 |
| Parser 复杂度 | 中 | **低** | 可复用现有 parser，不需新文件 |

---

### 最终结论

**阶段一（Codex Hooks）完全可行**，且比原计划更简单：

- 不需要单独的 `codex_hook.rs`，扩展现有 `claude_code.rs` 即可
- 字段格式完全兼容（snake_case 超集），不需要复杂的 alias 处理
- Windows 兼容性有 `commandWindows` 原生支持
- 主要用户侧障碍是 hook 信任流程，需要在设置页做好引导

**阶段二（App Server）可行但成本高**：
- WebSocket + JSON-RPC 2.0 协议清晰完整
- 可实现审批按钮、token 统计、多线程看管等增强功能
- 实现量估计 800-1200 行（连接管理 + 协议类型 + 事件映射 + 审批回调）
- 建议在阶段一跑通后再评估优先级
