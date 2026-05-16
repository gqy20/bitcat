# Claude Code 桌宠看管计划

> 日期：2026-05-14 | 更新：2026-05-15 | 状态：设计草案 | 范围：只做 Claude Code，不含 Codex / Cursor / OpenClaw

## 背景

`oc-claw` 的源码证明了一条很有价值的产品路线：桌宠不只是自己对话，也可以看管其他 AI 编码 Agent。它通过 Claude Code hook、会话 JSONL、权限事件和前端 Mini 面板，把编码 Agent 的状态压成 `working / waiting / done / idle`，再驱动宠物动画和提醒。

8Bit Cat 的技术底座不同：当前是 Windows-first Rust workspace、Tauri 2 多透明窗口、Vanilla JS Canvas、SDL2 手柄输入和 rig Agent。第一版应参考 `oc-claw` 的状态模型，但不照搬 React/Vite 前端、巨型 `lib.rs` 或多工具混合实现。

新增产品动机：用户在让 AI 编码 Agent 工作时，常会不自觉盯着终端或对话窗口等结果。多数时间 Agent 正在推理、编辑或跑命令，用户并不需要持续观看；真正需要用户注意的是权限请求、输入请求、失败和完成。桌宠应承担“替你看着”的角色：Agent 自己干活时提醒用户可以离开屏幕做点别的，轮到用户处理时再叫回来。

## 目标

让 8Bit Cat 成为 Claude Code 的“桌宠看管员”：

- Claude Code 工作时，猫进入专注/工作状态。
- Claude Code 持续工作且暂不需要用户时，猫提醒“我帮你盯着，你可以先去做点别的”。
- Claude Code 等待权限或用户输入时，猫主动提醒。
- Claude Code 完成任务时，猫做低干扰完成提示。
- 面板能列出当前 Claude Code 会话、项目目录、状态、最近活动和快捷操作。

第一版原则：

1. **只读优先**：先观察 Claude Code，不直接替用户批准权限。
2. **强类型中枢**：core 定义统一 `AgentSession`，app 只负责 hook/socket/文件系统。
3. **UI 消费语义状态**：pet/bubble/panel 不知道 Claude hook 细节，只消费 `working / waiting / done / idle`。
4. **Windows-first**：优先解决 PowerShell UTF-8、TCP shutdown、路径编码和进程检测问题。
5. **不做 Codex**：Codex 在 `oc-claw` Windows 源码中被主动禁用，本计划不把它混进第一版。
6. **不监督用户**：第一版不做摄像头、不做眼动、不用截图推断注意力。只基于 Agent 状态和轻量本地活动信号做提醒。

---

## 源码参考结论

从 `oc-claw` 源码确认的可借鉴点：

| 设计点 | 是否采用 | 说明 |
|--------|----------|------|
| Claude Code hook → 本地 socket → Rust 事件处理 | 采用 | 最可靠的实时信号来源 |
| JSONL session watcher 兜底 | 第二阶段采用 | 用于 ESC 中断、hook 丢失、session 文件变更 |
| `PermissionRequest` 映射为 waiting | 采用 | 这是桌宠提醒最有用的场景 |
| Write/Edit/Bash 结构化预览 | 第二阶段采用 | 先提醒，后做预览 |
| Working 状态下的离屏提醒 | 采用 | 用户不需要盯着 Agent 时，桌宠做低频提醒 |
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

core/src/agent_nudge.rs
  ├── AgentNudgePolicy
  ├── 工作中离屏提醒冷却
  ├── waiting / done 去重
  └── 生成语义提醒动作

core/src/claude_code.rs
  ├── ClaudeHookEvent
  ├── hook event → AgentSessionEvent
  ├── Claude JSONL 路径解析
  └── session JSONL 活跃状态兜底

frontend
  ├── panel: Agent 管理页
  ├── bubble: working-away / waiting / done 短提示
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

## 离屏提醒策略

这部分是本计划的产品核心：桌宠不是要求用户持续看着 Agent，而是让用户放心离开。提醒策略应在 core 中实现为纯逻辑，app 只负责提供时间、状态和本地环境信号。

### 设计原则

1. **低频、可关、可解释**：默认只在 Agent 工作超过一段时间后提醒一次，后续有较长冷却；设置页可关闭。
2. **等待优先**：`Waiting`、`Error`、`Done` 比“离开一下”提醒优先级高。
3. **不打断正在聊天/游戏/表演**：复用 `SharedBubble::is_chat_active()`、`performance::blocks_screenshot_observation()`、`game::is_game_busy()` 与 `observation_gate`。
4. **不用 Vision 判断注意力**：截图观察可用于屏幕摘要，但 MVP 不用它判断用户是否在盯屏，避免成本、延迟和误判。
5. **固定短文案优先**：离屏提醒不需要额外模型调用，先用本地文案即可。

### 状态到提醒动作

| Agent 状态 | 条件 | 桌宠行为 |
|------------|------|----------|
| `Working` | 状态持续超过 `first_nudge_after_sec`，且本 session 未提醒过 | `Focused` 情绪 + 气泡：“我帮你盯着，你可以先去喝口水/处理点别的。” |
| `ToolRunning` | 工具运行或命令执行持续较久 | `Focused` 情绪 + 低频气泡：“命令还在跑，我盯着。” |
| `Compacting` | 上下文压缩中 | 只更新面板状态，不主动气泡，除非持续很久 |
| `Waiting` | 新进入 waiting 或冷却结束 | `Confused` 情绪 + 气泡：“Claude Code 需要你处理一下。” |
| `Done` | 同一 session 首次完成 | `Happy` 情绪 + 气泡：“这轮完成了，可以回来看看。” |
| `Error` / `Interrupted` | 新进入异常态 | `Confused` 情绪 + 气泡提示异常摘要 |
| `Idle` | 无活动 session | 清理 Agent 通知，回到 idle |

### 建议配置

配置放在 `app_settings.json` 覆盖层，不写入 `~/.claude/settings.json`。

```rust
pub struct AgentWatchSettings {
    pub enabled: bool,
    pub away_nudge_enabled: bool,
    pub first_nudge_after_sec: u64,
    pub repeat_nudge_after_min: u64,
    pub waiting_alert: bool,
    pub done_alert: bool,
    pub use_tts: bool,
}
```

建议默认值：

```text
enabled = false                  # 需要用户安装 hook 后显式启用
away_nudge_enabled = true
first_nudge_after_sec = 90
repeat_nudge_after_min = 8
waiting_alert = true
done_alert = true
use_tts = false
```

### 实现形态

`core/src/agent_nudge.rs` 建议定义：

```rust
pub enum AgentNudgeKind {
    AwayWhileWorking,
    WaitingForUser,
    TaskDone,
    TaskError,
}

pub struct AgentNudge {
    pub session_id: String,
    pub kind: AgentNudgeKind,
    pub message: String,
    pub mood: PetMood,
    pub ttl_ms: u64,
}

pub struct AgentNudgePolicy {
    // 每个 session 的上次提醒、done 是否已提醒、上次状态变更时间等
}
```

app 层收到 `AgentNudge` 后转换成现有 `PetEvent`：

- `AwayWhileWorking` → `PetEvent::React { mood: Focused, speech: Some(...), ttl_ms: Some(...) }`
- `WaitingForUser` → `PetEvent::React { mood: Confused, speech: Some(...), ttl_ms: Some(...) }`
- `TaskDone` → `PetEvent::React { mood: Happy, speech: Some(...), ttl_ms: Some(...) }`
- `TaskError` → `PetEvent::React { mood: Confused, speech: Some(...), ttl_ms: Some(...) }`

如果后续需要更细的前端动画，再给 `PetNotificationKind` 增加 `AgentWorking` / `AgentWaiting` / `AgentDone`；MVP 可先复用 `React` 和气泡。

---

## 事件与持久化协议

### 前端事件

app 层对前端只暴露归一后的会话状态，不暴露 Claude Code hook 原始 payload。

```text
emit "agent-session-update"
  {
    "sessions": [AgentSessionView],
    "primary": AgentSessionView | null,
    "generated_at_ms": 1710000000000
  }
```

`AgentSessionView` 建议字段：

```rust
pub struct AgentSessionView {
    pub session_id: String,
    pub source: String,              // "claude_code"
    pub workspace: String,
    pub workspace_name: String,
    pub status: String,              // idle / working / tool_running / waiting / ...
    pub status_label: String,        // 给 UI 直接显示的短中文
    pub tool_name: Option<String>,
    pub tool_input_preview: Option<String>,
    pub user_prompt_preview: Option<String>,
    pub last_response_preview: Option<String>,
    pub needs_user: bool,
    pub updated_at_ms: u64,
    pub age_sec: u64,
}
```

排序规则：

1. `Waiting` / `Error` 置顶。
2. `Done` 其次。
3. `Working` / `ToolRunning` / `Compacting` 再次。
4. `Idle` 最后。
5. 同组按 `updated_at_ms` 倒序。

### 审计日志

Agent 看管涉及修改用户的 `~/.claude/settings.json` 和发出主动提醒，必须可追踪。

```text
~/.ai-pad/logs/
  agent_sessions.jsonl   # 归一后的 session event，不保存大 payload
  agent_nudges.jsonl     # 提醒决策：sent / skipped / cooled_down / gated
  agent_hooks.jsonl      # hook 安装/卸载/备份记录
```

`agent_nudges.jsonl` 建议字段：

```json
{
  "at_ms": 1710000000000,
  "session_id": "abc",
  "kind": "away_while_working",
  "decision": "sent",
  "status": "working",
  "reason": "working_for_90s",
  "cooldown_sec": 480
}
```

跳过原因建议使用稳定 snake_case：

- `disabled`
- `cooldown`
- `chat_active`
- `performance_active`
- `game_busy`
- `display_or_session_blocked`
- `panel_visible`
- `session_already_done_notified`

### Hook 安装安全

安装 `~/.claude/hooks/ai-pad-hook.ps1` 和合并 `~/.claude/settings.json` 时：

1. 读取并解析 settings JSON，解析失败时直接返回错误，不覆盖。
2. 写入前创建带时间戳备份，例如 `settings.ai-pad-backup-20260515-143000.json`。
3. 只增删 ai-pad 自己标记的 hook，不改动其他 hook。
4. 写入使用临时文件 + rename。
5. hook 脚本里端口、版本、安装来源写明注释，方便用户人工检查。

---

## Phase 1: 只读 Hook MVP

目标：约 700-1,050 行，先让桌宠知道 Claude Code 在干什么，并能做基础离屏/等待/完成提醒。

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

3. 新增 `core/src/agent_nudge.rs`
   - 定义 `AgentNudgePolicy` / `AgentNudge` / `AgentNudgeKind`。
   - 根据 `AgentSession` 状态、状态持续时间、冷却时间生成提醒。
   - 对同一 session 的 `Done` 只提醒一次。
   - `Working` / `ToolRunning` 的离屏提醒默认低频触发，不刷屏。

4. 新增 `app/src/agent_monitor.rs`
   - 启动本地 TCP server，例如 `127.0.0.1:5342` 或配置化端口，避免与 oc-claw/ooclaw 等工具常用的 `19283` 冲突。
   - 接收 PowerShell hook 原始 JSON。
   - 调用 core parser，更新 `Arc<Mutex<HashMap<String, AgentSession>>>`。
   - 调用 `AgentNudgePolicy` 生成提醒，并通过 `SharedPetEventBus` 发出。
   - emit `agent-session-update` 到前端。
   - 追加 `~/.ai-pad/logs/agent_sessions.jsonl`。
   - 可选追加 `~/.ai-pad/logs/agent_nudges.jsonl`，用于调试提醒去重和冷却。

5. 新增 `app/src/claude_hooks.rs`
   - 写入 `~/.claude/hooks/ai-pad-hook.ps1`。
   - 合并 `~/.claude/settings.json` 时遵循 Claude Code 的嵌套 hooks schema：`event -> [{ matcher?, hooks: [...] }]`，只追加带 `ai_pad_marker` 的桌宠 hook，不覆盖现有 hook。
   - 合并更新 `~/.claude/settings.json` 的 hook 配置。
   - PowerShell 脚本必须：
     - 设置 `[Console]::InputEncoding = [System.Text.Encoding]::UTF8`
     - 原样读取 stdin
     - 注入 `pid` 或 `host` 时只做最小 JSON 包装
     - 写入 TCP 后调用 socket shutdown，避免 Rust 读卡住

6. 增加 Tauri command
   - `cmd_get_agent_sessions`
   - `cmd_install_claude_code_hooks`
   - `cmd_remove_agent_session`
   - `cmd_open_agent_workspace`
   - `cmd_settings_save_agent_watch` / 或并入现有 settings 保存接口

### 前端

1. `panel.html` / `panel.js`
   - 新增 Agent 管理视图或面板入口。
   - 展示项目名、状态、工具名、更新时间。
   - Waiting 会话置顶。
   - A/Enter 打开工作区或终端；B/Esc 收起。

2. `bubble.js`
   - 监听 `agent-session-update`。
   - `Working` 持续一段时间后显示短提示：“我帮你盯着，你可以先去做点别的”。
   - `Waiting` 显示短提示：“Claude Code 需要你处理一下”。
   - `Done` 显示短提示：“Claude Code 完成了”。

3. `app.js` / `pet.js`
   - 新增 Agent 状态映射：
     - `Working` / `ToolRunning` → `Talk` 或后续 `Focused`
     - `Waiting` → `Confused`
     - `Done` → `Happy`
     - `Idle` → `Idle`

4. `settings.html` / `settings.js`
   - 新增 Agent 看管开关。
   - 新增“工作中提醒我离开屏幕”的开关。
   - 可配置首次提醒时间和重复提醒冷却。

### Phase 1 不做

- 不做权限批准按钮。
- 不做 Codex/Cursor。
- 不做远程机器。
- 不解析完整 Claude 对话历史。
- 不做 token 统计。
- 不用截图 Vision 判断用户是否盯屏。
- 不做摄像头、眼动或注意力检测。

### Phase 1 验收标准

- 设置页能安装 Claude Code hook，安装前会备份原 `~/.claude/settings.json`。
- 用手写 JSON 发送到本地 TCP 端口时，`cmd_get_agent_sessions` 能返回归一后的 session。
- 真实 Claude Code 提交 prompt 后，面板能看到对应 workspace 和 `Working` 状态。
- 进入 `PermissionRequest` 后，桌宠在 1 秒内提示需要用户处理。
- `Working` 持续超过 90 秒后，只提示一次“可以先去做点别的”；8 分钟冷却内不重复提示。
- `Stop` 后，同一 session 只提示一次完成。
- chat 输入中、舞蹈/游戏中、显示器关闭/会话锁定时，不弹低优先级离屏提醒。
- `make test-core` 通过；前端新增逻辑有 Vitest 覆盖。

---

## Phase 2: JSONL 兜底与实用提醒

目标：累计约 1,400-2,100 行，接近真正日常可用。

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

6. 离屏提醒增强
   - 增加 Windows `GetLastInputInfo`：用户长时间无输入时，降低“离开一下”提醒频率。
   - 增加前台窗口进程/标题检测：如果前台仍是 Claude Code/终端且 Agent 已工作一段时间，可更明确地提醒不要继续盯。
   - 如果屏幕锁定、显示器关闭或变暗，暂停离屏提醒，只保留状态记录。
   - 面板打开时视为用户正在查看状态，不再重复弹气泡。

7. 文案和 TTS
   - waiting / done 可选 TTS；working 离屏提醒默认不 TTS。
   - 文案使用本地短句池，不调用模型。
   - 支持用户在设置页关闭某类提醒。

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

## 实施 Checklist

### A. Core 模型

- [ ] 新增 `core/src/agent_session.rs`，带 3 句模块文档。
- [ ] 新增 `core/src/claude_code.rs`，反序列化 hook payload。
- [ ] 新增 `core/src/agent_nudge.rs`，纯策略、无 Tauri 依赖。
- [ ] 在 `core/src/lib.rs` 暴露新模块。
- [ ] 为 session 状态转换、排序、preview 截断和 nudge policy 写单测。

### B. App 后端

- [ ] 新增 `app/src/agent_monitor.rs`，启动本地 TCP server。
- [ ] 新增 `app/src/claude_hooks.rs`，安装/卸载 hook。
- [ ] 在 Tauri builder 中 manage session map / nudge policy。
- [ ] 注册 `cmd_get_agent_sessions`、`cmd_install_claude_code_hooks` 等命令。
- [ ] nudge 统一通过 `SharedPetEventBus` 发 `PetEvent`。
- [ ] 日志写入 `agent_sessions.jsonl` / `agent_nudges.jsonl`。

### C. 设置与配置

- [ ] `AppSettings` 增加 `agent_watch` 段，默认 disabled。
- [ ] 设置页增加 Agent 看管区域。
- [ ] 保存/重置/回显覆盖新增字段。
- [ ] hook 安装按钮明确显示只读观察，不自动批准权限。

### D. 前端体验

- [ ] panel 增加 Agent 会话列表或入口。
- [ ] bubble 支持 working-away / waiting / done 文案。
- [ ] pet 映射 Agent 状态到 focused/confused/happy。
- [ ] panel 打开时标记 Agent 状态已被查看，避免重复 done/waiting 提醒。

### E. 验证

- [ ] 手写 TCP payload 验证所有 hook event 映射。
- [ ] 真实 Claude Code 跑一轮：working → tool_running → done。
- [ ] 真实权限请求：waiting 提醒能及时出现。
- [ ] 长任务：90 秒离屏提醒出现且不刷屏。
- [ ] 锁屏/熄屏/聊天/游戏/舞蹈门控有效。

---

## 代码量预估

| 模块 | Phase 1 | Phase 2 增量 | Phase 3 增量 |
|------|--------:|-------------:|-------------:|
| `core/src/agent_session.rs` | 120-200 | 40-80 | 20-40 |
| `core/src/agent_nudge.rs` | 90-160 | 60-120 | 30-60 |
| `core/src/claude_code.rs` | 180-300 | 120-220 | 80-140 |
| `app/src/agent_monitor.rs` | 180-300 | 100-180 | 120-220 |
| `app/src/claude_hooks.rs` | 220-380 | 60-120 | 180-300 |
| `app/src/settings.rs` / `app_settings.rs` 增量 | 80-140 | 40-80 | 40-80 |
| `app/src/commands.rs` 增量 | 80-140 | 40-80 | 40-80 |
| 前端 panel/settings 增量 | 220-380 | 140-260 | 220-360 |
| 前端 bubble/pet 增量 | 100-180 | 40-80 | 40-80 |
| 测试 | 200-360 | 160-300 | 120-220 |

总计：

| 版本 | 预计代码量 |
|------|-----------:|
| Phase 1 只读 MVP + 基础离屏提醒 | 700-1,050 行 |
| Phase 1 + Phase 2 实用版 | 1,400-2,100 行 |
| Phase 1-3 接近 oc-claw 权限体验 | 2,400-3,600 行 |

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
- `AgentNudgePolicy` 测试：
  - Working 未达到阈值不提醒。
  - Working 达到阈值生成 `AwayWhileWorking`。
  - repeat 冷却内不重复提醒。
  - Waiting 立即生成 `WaitingForUser`，且优先级高于离屏提醒。
  - Done 同一 session 只提醒一次。
  - Error / Interrupted 生成异常提醒。

### App

- hook config merge 测试：
  - 保留用户已有 hooks。
  - 去重旧 ai-pad hook。
  - settings 文件损坏时不覆盖，返回错误。
- TCP event 处理：
  - 收到事件后更新 session map。
  - malformed JSON 不 panic。
  - 大 payload 只写 preview。
- 提醒派发：
  - `AgentNudge` 转换为正确的 `PetEvent::React`。
  - chat_active / performance / game busy 时跳过或延后低优先级离屏提醒。
  - waiting / done 提醒写入可审计日志。

### Frontend

- Agent session 排序：
  - waiting 置顶。
  - done 高于 working 或按产品决策固定。
  - 同状态按更新时间倒序。
- 状态文案：
  - working-away / waiting / done / idle。
- 手柄/键盘操作：
  - 选中 session 后打开工作区。
- 设置页：
  - Agent 看管开关默认关闭。
  - 离屏提醒开关和时间输入能保存、回显、重置。

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
| working 离屏提醒让用户烦 | 默认低频、可关闭、长冷却；只在 Agent 确实持续工作后触发 |
| 把“提醒离开”误做成用户监控 | MVP 不用摄像头/眼动/Vision 推断注意力，只看 Agent 状态和轻量活动信号 |
| 用户已经离开时仍弹气泡 | display/session gate 暂停低优先级提醒；waiting/done 仍记录状态，必要时等用户回来再展示 |
| Claude Code hook 格式变化 | parser 接受 raw 字段和兼容字段；失败时只 warn |

---

## 推荐落地顺序

1. 先做 `core/src/agent_session.rs` 和 `core/src/claude_code.rs`，把事件模型跑通。
2. 再做 `core/src/agent_nudge.rs`，用纯单元测试把离屏/等待/完成提醒策略跑稳。
3. 再做 `app/src/agent_monitor.rs`，用手写 JSON 通过 TCP 模拟 hook，并验证提醒事件。
4. 再写 hook installer，真实接 Claude Code。
5. 接 panel/pet/bubble/settings UI，先完成“不用盯着，我叫你回来”的闭环。
6. Phase 1 稳定后，再决定是否进入 JSONL watcher、前台窗口检测和权限按钮。
