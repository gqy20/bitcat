# Claude Code 桌宠看管计划

> 日期：2026-05-14 | 状态：设计草案 | 范围：只做 Claude Code，不含 Codex / Cursor / OpenClaw

## 背景

`oc-claw` 的源码证明了一条很有价值的产品路线：桌宠不只是自己对话，也可以看管其他 AI 编码 Agent。它通过 Claude Code hook、会话 JSONL、权限事件和前端 Mini 面板，把编码 Agent 的状态压成 `working / waiting / done / idle`，再驱动宠物动画和提醒。

8Bit Cat 的技术底座不同：当前是 Windows-first Rust workspace、Tauri 2 多透明窗口、Vanilla JS Canvas、SDL2 手柄输入和 rig Agent。第一版应参考 `oc-claw` 的状态模型，但不照搬 React/Vite 前端、巨型 `lib.rs` 或多工具混合实现。

## 目标

让 8Bit Cat 成为 Claude Code 的“桌宠看管员”：

- Claude Code 工作时，猫进入专注/工作状态。
- Claude Code 等待权限或用户输入时，猫主动提醒。
- Claude Code 完成任务时，猫做低干扰完成提示。
- 面板能列出当前 Claude Code 会话、项目目录、状态、最近活动和快捷操作。

第一版原则：

1. **只读优先**：先观察 Claude Code，不直接替用户批准权限。
2. **强类型中枢**：core 定义统一 `AgentSession`，app 只负责 hook/socket/文件系统。
3. **UI 消费语义状态**：pet/bubble/panel 不知道 Claude hook 细节，只消费 `working / waiting / done / idle`。
4. **Windows-first**：优先解决 PowerShell UTF-8、TCP shutdown、路径编码和进程检测问题。
5. **不做 Codex**：Codex 在 `oc-claw` Windows 源码中被主动禁用，本计划不把它混进第一版。

---

## 源码参考结论

从 `oc-claw` 源码确认的可借鉴点：

| 设计点 | 是否采用 | 说明 |
|--------|----------|------|
| Claude Code hook → 本地 socket → Rust 事件处理 | 采用 | 最可靠的实时信号来源 |
| JSONL session watcher 兜底 | 第二阶段采用 | 用于 ESC 中断、hook 丢失、session 文件变更 |
| `PermissionRequest` 映射为 waiting | 采用 | 这是桌宠提醒最有用的场景 |
| Write/Edit/Bash 结构化预览 | 第二阶段采用 | 先提醒，后做预览 |
| 权限按钮直接回写 hook | 第三阶段可选 | 风险更高，需要审计和确认 |
| 单个巨型后端文件 | 不采用 | 拆 core/app 模块 |
| React/Vite Mini 面板 | 不采用 | 复用现有 `panel.html` / `bubble.html` / `pet.html` |
| Windows Codex hook | 不采用 | `oc-claw` 当前 Windows 分支禁用 Codex |

---

## 目标架构

```text
Claude Code Hook
  └── PowerShell hook script
      └── TCP 127.0.0.1:<port>
          └── app/src/agent_monitor.rs
              ├── parse raw hook JSON
              ├── update AgentSessionMap
              ├── append agent_sessions.jsonl
              └── emit agent-session-update

core/src/agent_session.rs
  ├── AgentSource / AgentStatus
  ├── AgentSession / AgentSessionEvent
  └── 状态归一化与排序

core/src/claude_code.rs
  ├── ClaudeHookEvent
  ├── hook event → AgentSessionEvent
  ├── Claude JSONL 路径解析
  └── session JSONL 活跃状态兜底

frontend
  ├── panel: Agent 管理页
  ├── bubble: waiting / done 短提示
  └── pet: working / waiting / done / idle 动画映射
```

### 状态模型

```rust
pub enum AgentSource {
    ClaudeCode,
}

pub enum AgentStatus {
    Idle,
    Working,
    ToolRunning,
    Waiting,
    Compacting,
    Done,
    Interrupted,
    Error,
}

pub struct AgentSession {
    pub session_id: String,
    pub source: AgentSource,
    pub workspace: String,
    pub status: AgentStatus,
    pub tool_name: Option<String>,
    pub tool_input_preview: Option<String>,
    pub user_prompt_preview: Option<String>,
    pub last_response_preview: Option<String>,
    pub pid: Option<u32>,
    pub updated_at_ms: u64,
    pub needs_user: bool,
}
```

状态归一化：

| Claude hook | 归一状态 | UI 含义 |
|-------------|----------|---------|
| `UserPromptSubmit` | `Working` | 已提交任务，Claude 正在思考 |
| `PreToolUse` | `ToolRunning` | Claude 准备执行工具 |
| `PostToolUse` | `Working` | 工具完成，继续推理 |
| `PermissionRequest` | `Waiting` | 需要用户处理 |
| `PreCompact` | `Compacting` | 上下文压缩中 |
| `Stop` | `Done` | 任务完成 |
| `SessionEnd` | `Idle` | 会话结束 |

---

## Phase 1: 只读 Hook MVP

目标：约 600-900 行，先让桌宠知道 Claude Code 在干什么。

### 后端

1. 新增 `core/src/agent_session.rs`
   - 定义 `AgentSource` / `AgentStatus` / `AgentSession` / `AgentSessionEvent`。
   - 提供 `sort_sessions()`：`Waiting > Done > Working > Idle`，再按 `updated_at_ms`。
   - 提供 `is_active()` / `needs_user()` helper。

2. 新增 `core/src/claude_code.rs`
   - 定义 `ClaudeHookEvent`，兼容 Claude 原始字段：
     - `session_id`
     - `hook_event_name`
     - `cwd`
     - `status`
     - `tool_name`
     - `tool_input`
     - `prompt`
     - `last_assistant_message`
   - 实现 `event_to_session_event()`。
   - 对 `tool_input` 做短 preview，不保存大文本。

3. 新增 `app/src/agent_monitor.rs`
   - 启动本地 TCP server，例如 `127.0.0.1:19283` 或配置化端口。
   - 接收 PowerShell hook 原始 JSON。
   - 调用 core parser，更新 `Arc<Mutex<HashMap<String, AgentSession>>>`。
   - emit `agent-session-update` 到前端。
   - 追加 `~/.ai-pad/logs/agent_sessions.jsonl`。

4. 新增 `app/src/claude_hooks.rs`
   - 写入 `~/.claude/hooks/ai-pad-hook.ps1`。
   - 合并更新 `~/.claude/settings.json` 的 hook 配置。
   - PowerShell 脚本必须：
     - 设置 `[Console]::InputEncoding = [System.Text.Encoding]::UTF8`
     - 原样读取 stdin
     - 注入 `pid` 或 `host` 时只做最小 JSON 包装
     - 写入 TCP 后调用 socket shutdown，避免 Rust 读卡住

5. 增加 Tauri command
   - `cmd_get_agent_sessions`
   - `cmd_install_claude_code_hooks`
   - `cmd_remove_agent_session`
   - `cmd_open_agent_workspace`

### 前端

1. `panel.html` / `panel.js`
   - 新增 Agent 管理视图或面板入口。
   - 展示项目名、状态、工具名、更新时间。
   - Waiting 会话置顶。
   - A/Enter 打开工作区或终端；B/Esc 收起。

2. `bubble.js`
   - 监听 `agent-session-update`。
   - `Waiting` 显示短提示：“Claude Code 需要你处理一下”。
   - `Done` 显示短提示：“Claude Code 完成了”。

3. `app.js` / `pet.js`
   - 新增 Agent 状态映射：
     - `Working` / `ToolRunning` → `Talk` 或后续 `Focused`
     - `Waiting` → `Confused`
     - `Done` → `Happy`
     - `Idle` → `Idle`

### Phase 1 不做

- 不做权限批准按钮。
- 不做 Codex/Cursor。
- 不做远程机器。
- 不解析完整 Claude 对话历史。
- 不做 token 统计。

---

## Phase 2: JSONL 兜底与实用提醒

目标：累计约 1,200-1,800 行，接近真正日常可用。

1. JSONL 路径解析
   - Claude Code session 文件通常在 `~/.claude/projects/<project_dir>/<session_id>.jsonl`。
   - Windows project dir 需要把 `/`、`\`、`:`、`.` 替换为 `-`。

2. 文件 watcher
   - 使用轻量轮询或 `notify`（如果接受新增依赖）。
   - 仅作为 hook 兜底，不与 hook 抢状态。
   - 主要处理：
     - ESC 中断
     - session 文件截断/compact
     - hook 丢失后的状态恢复

3. PID 存活检测
   - Claude 进程退出但没有发 Stop 时，清理 stuck working/waiting。
   - Windows 可用 `windows-sys` / ToolHelp 或简单进程查询。

4. Waiting 预览
   - 对 Write/Edit/Bash 做结构化 preview：
     - 文件名
     - 代码前 N 行
     - Bash command
   - preview 只用于 UI，不写入大日志。

5. 完成提醒去重
   - 同一 session 的 Done 只提醒一次。
   - 如果 panel 当前打开或用户已经看过，不重复弹。

---

## Phase 3: 权限控制（可选）

目标：累计约 2,200-3,200 行，接近 `oc-claw` 的权限体验，但风险更高。

能力：

- `PermissionRequest` hook 阻塞等待本地响应。
- panel/bubble 提供：
  - 拒绝
  - 允许一次
  - 本次会话允许
  - 跳转到终端处理
- 所有响应写入 `agent_actions.jsonl`。
- 高风险工具默认不自动批准。

约束：

- 默认仍建议跳转到 Claude Code 原生 UI 处理。
- 自动批准必须设置页显式开启。
- 权限动作必须复用 B4 的审计原则。

---

## 代码量预估

| 模块 | Phase 1 | Phase 2 增量 | Phase 3 增量 |
|------|--------:|-------------:|-------------:|
| `core/src/agent_session.rs` | 120-200 | 40-80 | 20-40 |
| `core/src/claude_code.rs` | 180-300 | 120-220 | 80-140 |
| `app/src/agent_monitor.rs` | 180-300 | 100-180 | 120-220 |
| `app/src/claude_hooks.rs` | 220-380 | 60-120 | 180-300 |
| `app/src/commands.rs` 增量 | 80-140 | 40-80 | 40-80 |
| 前端 panel 增量 | 180-320 | 120-220 | 220-360 |
| 前端 bubble/pet 增量 | 80-160 | 40-80 | 40-80 |
| 测试 | 150-300 | 120-240 | 120-220 |

总计：

| 版本 | 预计代码量 |
|------|-----------:|
| Phase 1 只读 MVP | 600-900 行 |
| Phase 1 + Phase 2 实用版 | 1,200-1,800 行 |
| Phase 1-3 接近 oc-claw 权限体验 | 2,200-3,200 行 |

---

## 测试计划

### Core

- `ClaudeHookEvent` 反序列化：
  - 原始 Claude Code 字段
  - 缺失 cwd
  - 缺失 tool_input
  - Stop 带 last assistant message
- 状态转换测试：
  - `UserPromptSubmit → Working`
  - `PreToolUse → ToolRunning`
  - `PermissionRequest → Waiting`
  - `Stop → Done`
- Windows session path 测试：
  - `D:\C\Desktop\ai\8bit` → Claude project dir 格式。
- preview 截断测试：
  - 中文按字符截断，不按字节切。

### App

- hook config merge 测试：
  - 保留用户已有 hooks。
  - 去重旧 ai-pad hook。
  - settings 文件损坏时不覆盖，返回错误。
- TCP event 处理：
  - 收到事件后更新 session map。
  - malformed JSON 不 panic。
  - 大 payload 只写 preview。

### Frontend

- Agent session 排序：
  - waiting 置顶。
  - done 高于 working 或按产品决策固定。
  - 同状态按更新时间倒序。
- 状态文案：
  - waiting / working / done / idle。
- 手柄/键盘操作：
  - 选中 session 后打开工作区。

---

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| 修改 `~/.claude/settings.json` 损坏用户配置 | 解析后结构化合并，写入前备份，失败不覆盖 |
| PowerShell CJK 编码导致 JSON 损坏 | hook 开头强制 UTF-8 stdin，Rust 只解析 UTF-8 |
| TCP 读取卡住 | hook 端显式 socket shutdown，server 设置 read timeout |
| 大工具输入污染日志 | 只保存 preview，完整内容不落盘 |
| hook 和 watcher 状态打架 | hook 是主信号，watcher 只处理中断/丢失/恢复 |
| waiting 提醒太吵 | 去重、冷却时间、panel 打开时视为已看 |
| Claude Code hook 格式变化 | parser 接受 raw 字段和兼容字段；失败时只 warn |

---

## 推荐落地顺序

1. 先做 `core/src/agent_session.rs` 和 `core/src/claude_code.rs`，把事件模型跑通。
2. 再做 `app/src/agent_monitor.rs`，用手写 JSON 通过 TCP 模拟 hook。
3. 再写 hook installer，真实接 Claude Code。
4. 最后接 panel/pet/bubble UI。
5. Phase 1 稳定后，再决定是否进入 JSONL watcher 和权限按钮。

