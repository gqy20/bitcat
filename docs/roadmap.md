# BitCat Roadmap

> **目标**：将 BitCat 打磨为 **Steam 可发布的 AI 驱动桌面伴侣**。
> **核心差异化**：AI 通过结构化输出动态生成可玩内容（舞蹈 + 迷你游戏），而非仅对话。

---

## 当前基线

| 能力 | 状态 |
|------|------|
| v2 宠物资源包系统（manifest + spritesheet + 设置页选择） | 已落地（2026-05-17）；默认 `piggy`，`cat/status/core/...` 可选 |
| 语义宠物动画（非均匀帧时长 + 瞬态 repeat+fallback + idle variants） | 已有（2026-05-13 增强，2026-05-17 收敛到 v2 pack） |
| AI 对话（Anthropic Claude via rig-core，流式输出） | 已有 |
| 15 个内置工具（launch/shell/read_file/get_time/recent_screenshots/search_memory/remember/reminder/hotkey/clipboard/foreground/dance 等） | 已有 |
| SDL2 手柄输入（8BitDo Micro） | 已有 |
| 多窗口模型（pet / bubble / panel / voice / settings / game / agent-watch / notification / camera 等） | 已有 |
| 截图观察 + 摄像头观察 + Vision API 分析 | 截图已默认启用；摄像头观察默认关闭，可在设置页开启 |
| 滚动窗口记忆系统 | 已有 |
| YML 配置热加载 | 已有 |
| 舞蹈系统（内置/用户目录 YAML + AI tool 触发播放） | 已落地 |
| 程序化提醒（create/list/cancel 工具 + 到期调度 + 完成/稍后/取消/删除） | 已落地（2026-05-22）；统一顶部通知 + JSONL 生命周期日志 |
| AI 提醒润色（无工具结构化 personalizer） | 已落地（默认关闭）；prompt 统一在 `config/prompts.yml` 的 `reminder_personalizer` 段 |
| 顶部通知岛（reminder / agent_watch 共用，队列/动作/提示音） | 已落地（2026-05-22）；设置页可按来源配置提示音 |
| Agent Watch 本地/远程只读看管 | Claude/Codex hook + Remote LAN ingest/viewer MVP 已落地；控制能力仍待做 |
| 日志规范化（大文本截断、级别收敛、tracing 归一） | 已落地第一轮 |
| Token 追踪（JSONL 明细、会话汇总、按日查询） | 已落地 |
| 设置页 Token 统计（今日消耗、最近会话、链路占比） | 已落地 |
| Makefile 测试入口（通过 xtask 避免 PowerShell 语法问题） | 已落地 |

## 源码确认的技术栈

BitCat 当前不是 Web 应用套壳，而是 **Windows-first 的 Rust 桌面自动化程序 + Tauri 多透明 WebView 界面 + rig Agent 运行时**。`oc-claw` 可参考产品模型和会话状态抽象，但不建议照搬它的前端/桌面技术栈；本项目已有更贴近宠物交互和手柄场景的底座。

| 层级 | 技术 | 源码依据 | 说明 |
|------|------|----------|------|
| Workspace | Rust workspace：`core` / `app` / `xtask` | `Cargo.toml` | 逻辑、桌面壳、维护命令分离 |
| Core crate | Rust 2024 + `bitcat-core` | `core/Cargo.toml` | 纯逻辑层，无 Tauri/UI 依赖，便于快速测试 |
| App crate | Rust 2021 + Tauri 2 | `app/Cargo.toml`, `app/tauri.conf.json` | 桌面窗口、托盘、全局快捷键、IPC |
| UI Runtime | WebView2 via Tauri | `tauri.conf.json` `frontendDist: ./frontend` | 多窗口透明桌宠，而非浏览器页应用 |
| Frontend | Vanilla HTML/CSS/JS + Canvas | `app/frontend/*.html`, `app/frontend/js/*.js` | 无 React/Vue/构建步骤；Node 只用于测试 |
| Pet Assets | v2 manifest + bundled fixture packs | `app/frontend/__fixtures__/pets/*/manifest.json`, `app/frontend/js/sprite-loader.js` | 宠物视觉不再依赖硬编码默认 sprite fallback；默认加载 `piggy` |
| Frontend Tests | Vitest 3 + jsdom | `app/frontend/package.json`, `vitest.config.ts` | 测试 `bubble/pet/game/sprite` 等纯 JS 逻辑 |
| AI Agent | `rig-core` 0.36 + Anthropic provider | `core/src/agent.rs`, `core/src/vision.rs` | 流式对话、Tool、Extractor、Vision 结构化输出 |
| Reminder Personalizer | rig Extractor（no-tool） | `core/src/reminder_personalizer.rs`, `config/prompts.yml` | 到期提醒短文案可选 AI 润色，失败回退原始提醒 |
| AI 配置 | 环境变量 / `app_settings.json` / `~/.claude/settings.json` | `core/src/ai_config.rs` | 默认兼容 Claude Code 风格配置，只读读取 `.claude` |
| Tool Schema | `schemars` + `serde` | `core/src/tools.rs`, `core/src/agent.rs` | 参数类型 derive JSON Schema，减少手写 schema 漂移 |
| Notification Window | Tauri 透明顶部窗口 + Vanilla JS | `app/src/notification_window.rs`, `app/frontend/notification.html` | 提醒和 Agent Watch 共用，支持动作、队列、去重和提示音 |
| 手柄输入 | SDL2 0.38 bundled + static-link | `app/Cargo.toml`, `app/src/joystick.rs` | 8BitDo / DirectInput 轮询，Windows 构建静态 SDL2 |
| Windows API | `windows-sys` / `windows` | `core/src/hotkey.rs`, `app/src/screenshot.rs`, `app/src/tts.rs`, `app/src/audio_reactive.rs`, `app/src/main.rs` | SendInput、BitBlt、SAPI TTS、WASAPI、console、power/session 检测 |
| 截图/摄像头/Vision | Win32 BitBlt + WebView getUserMedia + `image` JPEG + rig Extractor | `app/src/screenshot.rs`, `app/src/camera.rs`, `core/src/vision.rs`, `core/src/camera_observation.rs` | 截图逐屏捕获；摄像头默认关闭，开启后保守结构化观察 |
| 配置 | YAML + 内嵌默认值 + exe 同目录覆盖 | `config/*.yml`, `core/src/config.rs` | `actions/buttons/panel_action/prompts/user` |
| 日志/审计 | `tracing` + rolling file + JSONL | `app/src/main.rs`, `core/src/tool_events.rs`, `core/src/token_tracker.rs` | 双写日志、token 用量、工具事件审计 |
| 测试 | cargo-nextest / insta / rstest / proptest / wiremock / tauri::test | `.config/nextest.toml`, `core/Cargo.toml`, `app/Cargo.toml` | core 快速测，app IPC feature 测，外部 API 用 mock |
| 打包 | `xtask` + zip + Makefile | `xtask/Cargo.toml`, `Makefile` | 配置复制、测试入口、portable zip 统一走 Rust 工具链 |

### Track E 的技术落点

Agent 管理线应优先复用现有栈：

- 本地会话监听放在 `app` 侧线程或 `core` 侧纯解析模块中：进程/文件/hook JSONL 探测需要 Windows 和文件系统能力，UI 只消费归一化事件。
- 会话抽象放在 `core`：新增 `agent_session` / `agent_monitor` 类型，保持可测试；`app` 只负责实际扫描、窗口打开和 IPC。
- UI 使用现有 `panel.html` / `bubble.html` / `pet.html` 扩展：不引入 React/Electron，不新增前端构建链。
- Claude Code/Codex 适配从“只读”开始：读取 `~/.claude/settings.json`、会话目录、Codex 工作区/线程元数据、进程命令行和 git 状态；控制动作等 E3 再接入审计与确认。
- 远程/多工作区用 JSONL sidecar 或配置化 connector：不要先引入数据库、消息队列或向量索引。

---

## 发展方向总览

```
┌─────────────────────────────────────────────────────────────┐
│                      BitCat 产品路线                        │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │ Track A  │  │ Track B  │  │ Track C  │  │ Track E  │   │
│  │ AI 内容  │  │ 基础设施  │  │ 渲染升级  │  │ Agent管理│   │
│  │  生成    │  │  强化    │  │          │  │          │   │
│  ├──────────┤  ├──────────┤  ├──────────┤  ├──────────┤   │
│  │ A1 舞蹈✓ │  │ B1 日志✓  │  │ C1 3D体素 │  │ E1 会话监听│  │
│  │ A2 游戏  │  │ B2 Token✓│  │ C2 动画   │  │ E2 桌宠管家│  │
│  │ A3 扩展  │  │ B3 结构化 │  │ C3 游戏3D│  │ E3 多Agent│  │
│  │          │  │ B4 工具运行时│ │          │  │ E4 远程机 │   │
│  │          │  │ B5 文本记忆│  │          │  │          │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
│                                                             │
│  当前优先级：A2 Phase 2 → E2/E3 → Observation Hints → 资源包发布策略 → C1 → D1│
└─────────────────────────────────────────────────────────────┘
```

---

## Track A: AI 内容生成

### A1. 舞蹈系统

用户说 "跳个舞" → AI 在普通对话中自行决定调用 `perform_dance` → 提交完整 `DanceDef` → pet 窗口即刻播放。

核心变化：从旧的 `create_dance(name, mood)` + `choreograph()` 查表模板，升级为 LLM 通过 Tool Call 直接输出完整动作序列（步骤数、节奏、组合全部由 AI 决定）。Rust 只负责校验、保存和播放。

- 新增 4 个 sprite 动作帧：jump / spin / wave / shake
- 前端 dancePlayer 劫持渲染循环，播完交还控制权
- 用户目录 `~/.bitcat/dances/` 优先，内置预设 `config/dances/` 兜底

详细设计：[plan/archive/structured-output-design.md](plan/archive/structured-output-design.md)

### A2. 迷你游戏引擎

Phase 1 已完成（提交 `a2105ff`）：新增全屏透明 `game` 窗口并加入默认游戏入口，内置 Snake 可通过键盘/手柄游玩，结束后联动 `GamePlay` / `GameWin` / `GameLose` 宠物状态。当前面板已收敛为 2×2 游戏启动器，默认入口是 Snake / Memory / Catch / Battle。

当前已落地的数据流：

```
panel → cmd_start_game / cmd_start_memory / cmd_start_catch / cmd_start_battle
      → app/src/game.rs 动态创建 game 窗口
      → game.html / game_engine.js 运行 Snake / Memory / Catch / Battle
      → cmd_game_end(result, score) → 关闭窗口 + 切换 pet 状态
```

下一步继续复用 A1 的模式：模型通过工具提交结构化 `GameDef` → Rust validate / save → game 窗口运行游戏 → 结束联动 pet 状态。当前底层已经可以启动任意 `GameDef`：`cmd_start_game_with_def` / `start_game(GameDef)` 已落地，`ActionBus::PlayGameDefault` 和面板入口会启动默认 Snake。尚未完成的是把这个能力注册成 AI 可调用的 `play_game` / `perform_game` 工具，以及把生成的游戏配置持久化到用户目录。

四种原型游戏共享同一个 GameEngine 入口：

| 游戏 | 操作 | 复杂度 |
|------|------|--------|
| 毛线球大作战（Snake） | 方向键转向，A 加速 | 已落地 |
| 翻牌配对（Memory） | 方向键移动 + A 翻牌 | 已落地 |
| 接食物（Catch） | 方向键移动篮子 | 已落地 |
| 飞机守护战（Battle） | 方向键移动，A 射击，X/Y/L1 技能 | 已落地 |

输入已改为游戏激活时独占：D-pad/A/B/Start 由 `gamepad_loop` 转发为 `game-input`，普通滚轮、宠物动作和面板动作暂停。胜利→`GameWin`，失败→`GameLose`，取消→`Idle`。

详细设计：[plan/minigame-system.md](plan/minigame-system.md)

### A3. 内容生态扩展

- 更多游戏类型（节奏点击 / Quiz / 躲避障碍 / 2048）
- 舞蹈编辑器（可视化时间轴，可选）
- Steam Workshop 集成（分享/订阅 YAML）
- 工具选择策略升级（固定全量工具 → 模型/上下文驱动的工具选择）

---

## Track B: 基础设施强化

### B1. 日志体系规范化

已完成第一轮规范化：大文本不再裸写 INFO，前端日志桥和 chat/vision/memory 等高价值链路已收敛到 tracing 字段；`AGENTS.md` / `CLAUDE.md` 已补充日志规范。下一步只保留少量持续治理：新增功能必须继续使用 `log_preview()` 和稳定字段，避免把结构化数据塞回普通日志。

详细设计：[plan/archive/logging-standardization.md](plan/archive/logging-standardization.md)

### B2. Token 全链路追踪

已完成 MVP：chat / vision / screen_summary / memory_aggregation 的 input/output/cache token 明细写入 `~/.bitcat/logs/token_usage.jsonl`，最近会话汇总写入 `~/.bitcat/logs/token_sessions.json`，并通过设置页 `cmd_get_token_stats` 展示今日消耗、最近会话和各链路占比。下一步是把统计用于决策：先观察真实用量，再决定是否优化工具 schema、上下文注入或模型路由。

详细设计：[plan/archive/token-tracking.md](plan/archive/token-tracking.md)

### B3. 结构化输出（Extractor 改造）

已完成主链路与 cleanup：vision / screen_summary / memory aggregation 都已接入 rig `Extractor`，分别输出 `VisionAnalysis`、`StructuredSummary`、`ProfileAggregation`，token 用量也改为读取 `ExtractionResponse.usage`。旧 raw request / text parser / Anthropic usage parser 已删除，不再生效的 `screen_summary.max_summary_chars` 配置也已移除；保留基于 Anthropic `tool_use` 协议的 wiremock 回归测试。

详细设计：[plan/archive/rig-capability-roadmap.md](plan/archive/rig-capability-roadmap.md) §P0

### B4. 工具运行时与开销优化（谨慎，不做关键词意图识别）

当前 15 个工具仍全量注册到每次对话，但 B4 第一阶段已经完成了更基础的运行时治理：工具调用不再混入 bubble 正文，而是通过结构化事件单独呈现；普通工具显示低干扰状态条，舞蹈这类表演型工具显示“正在编舞 / 准备开跳”并短暂退场；工具结果会写入 `~/.bitcat/logs/tool_events.jsonl`，用于后续统计成功率、耗时和拦截次数。

新的原则保持不变：**默认相信大模型自己选择工具，Rust 负责 schema、权限、生命周期事件、审计和体验呈现**。B4 现在已经足够支撑下一阶段游戏工具接入：未来 AI 层 `perform_game` / `play_game` 这类表演型或互动型工具应复用 `ToolKind::Performance` 或扩展出更精细的 kind，让 bubble 退到辅助位置，把主视觉交给 pet/panel/game 窗口；底层 `start_game(GameDef)` 已经存在，工具只需要接入这条通道。

建议拆分：

1. **B4.1 已完成：工具生命周期事件协议**：`planned / blocked / finished / failed` 已接入，携带 `tool_name`、`internal_call_id`、结果预览、耗时和结果状态。`allowed` 不做伪事件，除非以后把 `PermissionHook` 改为带事件 sink 的状态化 hook。
2. **B4.2 已完成：bubble 表演型工具状态 UI**：普通工具显示安静状态条；`perform_dance` / `play_dance` 走“正在编舞 / 准备开跳 / bubble 退场”的舞台体验。
3. **B4.3 已完成：工具事件审计日志**：`tool_events.jsonl` 记录成功率、失败/拦截、耗时和短结果预览，不记录大文本。
4. **B4.4 已做首轮：schema/description 压缩**：已低风险压缩 `perform_dance` / `play_dance` 文案。真实 token 预算工具暂缓，等工具继续增长或真实日志显示固定 schema 成为瓶颈再做。
5. **B4.5 暂缓：显式能力包 / dynamic_tools 实验**：当前不阻塞游戏部分。仅在真实数据证明必要时启用，必须 feature flag 可回滚。

下一步主线建议：先做游戏部分。游戏工具应直接复用 B4 已完成的事件协议、UI 分层和审计日志；不要再引入关键词分类或额外小模型判断。

详细设计：[plan/archive/rig-capability-roadmap.md](plan/archive/rig-capability-roadmap.md) §P1

### B5. grep-first 文本记忆检索

长期记忆的 grep-first 主链路已落地：`LongTermMemory` 使用 `~/.bitcat/memory/long_term.jsonl`，一行一条当前有效 record，包含稳定 `id`、`created_at` 和 `deleted` 软删除字段；`record_candidate()` / `remember` 写入结构化候选，`retrieve_with()` 按 text/tag/source/min_importance 做可解释召回并最多返回 20 条候选，设置页按 id 审查和软删除。本项目仍不采用 Embeddings / Vector RAG；后续 B5 剩余工作是上下文瘦身和候选压缩：减少默认预塞长期记忆，让模型更多通过 `search_memory` 按需取候选，再自行判断语义相关性。

详细取舍：[architecture/design-tradeoffs.md](architecture/design-tradeoffs.md)

### B6. 程序化提醒与顶部通知

提醒主链路已落地：AI Agent 通过 `create_reminder` / `list_reminders` / `cancel_reminder` 管理确定性提醒，`core/src/reminder.rs` 负责 store、原子写入和生命周期事件，`app/src/reminder_scheduler.rs` 每 5 秒扫描到期提醒，`app/src/notification_window.rs` 用统一顶部通知窗口呈现提醒、完成、稍后、取消和删除动作。

2026-05-22 增量：

1. **顶部通知岛**：reminder 和 Agent Watch 共用同一个通知窗口，支持队列、去重、动作按钮和来源字段。
2. **提示音**：设置页可按来源配置系统提示音，覆盖 `info / success / warning / danger` 等等级，失败时只记录日志，不影响通知展示。
3. **AI 提醒润色**：到期提醒可选调用无工具结构化 `ReminderPersonalizer`，根据标题、备注、到期时间和用户上下文生成更自然的短提醒；默认关闭，失败时回退确定性原文，prompt 统一在 `config/prompts.yml` 的 `reminder_personalizer` 段。
4. **可诊断性**：提醒生命周期写入 `~/.bitcat/logs/reminder_events.jsonl`，字段保留 `reminder_id`、`source`、`ui_source`、`store_path` 和异常上下文。

后续只保留打磨项：根据真实使用决定是否把 `complete_reminder` / `snooze_reminder` 也暴露给 Agent 工具；完善到期批量并发、费用门控和设置页的失败诊断。

---

## Track C: 渲染升级

### C1. 桌宠 3D 体素化

将 pet 窗口从 Canvas 2D 像素升级为 Three.js 体素风格（类似 Minecraft 角色）。核心创意：**组合式部位模型**——Q 版大头猫在进入游戏模式时"长出"完整身体（躯干→手臂→腿部依次弹出，~1.5s 变身动画）。

技术选型 Three.js 的理由：
- WebView2 透明 + WebGL 成熟可行
- InstancedMesh 高效体素渲染（5000 以内 voxel 稳定 60fps）
- AI 友好：voxel 数据与 JSON 结构天然对齐
- 与未来游戏共用同一渲染引擎

详细设计：[plan/3d-architecture.md](plan/3d-architecture.md)

### C2. 动画增强

- ~~呼吸微动、眨眼、走路改进~~ ✅ **已完成 (2026-05-13)** — 非均匀帧时长 + 瞬态 repeat+fallback
- ~~硬编码小猫迁移到资源包~~ ✅ **已完成 (2026-05-17)** — v2-only manifest loader，默认 `piggy`，`cat` 作为普通可选包
- ~~语义短动作 overlay~~ ✅ **已完成 (2026-05-17)** — manifest action timeline，`observe/nudge/acknowledge/blocked/dragging` 可用于截图、输入和拖拽反馈
- 宠物资源包发布策略：决定大 WebP pack 是进入 bundle 还是外置下载，见 [plan/pet-asset-packaging.md](plan/pet-asset-packaging.md)
- 粒子系统迁移到 Three.js Points
- 舞蹈系统 3D 化（真实抛物线轨迹、翻滚感）
- 鼠标交互：hover 时猫转头看鼠标

### C3. 3D 游戏生成能力

新增独立 game 窗口（PerspectiveCamera + OrbitControls），AI 生成可交互的 3D 游戏内容（地形、角色、规则）。集成物理引擎（cannon-es）、碰撞检测、音效系统。

---

## Track E: AI 编码工具管理（参考 oc-claw）

参考项目：[rainnoon/oc-claw](https://github.com/rainnoon/oc-claw)。它的核心价值不在具体 UI 栈，而在产品模型：把 Claude Code / Codex / Cursor / OpenClaw 等编码 Agent 的会话活动抽象为 `working / idle / waiting` 等状态，再用桌面宠物、会话面板、历史记录和用量指标持续呈现。BitCat 可以把这条思路做得更“伙伴原生”：手柄、pet 动画、bubble、panel 和截图记忆都参与 Agent 管理，而不是只做一个独立状态仪表盘。

### oc-claw 源码核验结论

本地核验 `oc-claw` 源码后，确认它是 **Tauri 2 + React 19 + Vite + Tailwind + Motion** 的构建型前端项目，真正应用位于 `frontend/`，Tauri 后端主要集中在 `frontend/src-tauri/src/lib.rs`（约 12K 行）。因此对 BitCat 来说，它更适合作为 Agent 管理交互参考，而不是技术栈模板。

关键源码事实：

| 主题 | oc-claw 源码现状 | 对 BitCat 的启发 |
|------|------------------|-------------------|
| 桌面壳 | Tauri 2，单个 `mini` 透明窗口，前端走 Vite 构建 | 我们保留多 WebView 静态窗口，不引入 React/Vite |
| Agent 状态模型 | `ClaudeSession` 统一承载 `cc/codex/cursor`，字段含 `status/tool/toolInput/lastResponse/source` | 可以借鉴统一 `AgentSession`，但应拆进 core 类型和 app 监听层 |
| Claude Code | 安装 hook 到 `~/.claude/hooks/`，事件进本地 socket，再由 Rust `process_claude_event()` 归一化 | E1 第一优先级可参考这条路径 |
| Windows hook | Claude Code 在 Windows 通过 Git Bash 调 hook，源码改用 PowerShell 脚本 + TCP `127.0.0.1:5342`，并设置 UTF-8 stdin | 这是高价值坑位，Windows 必须按 UTF-8 和显式 TCP shutdown 处理；端口避开 oc-claw/ooclaw 常用的 `19283` |
| Codex | 非 Windows 使用 `~/.Codex/hooks.json`；但源码 Windows 分支会清理并禁用 oc-claw 的 Codex hook，TCP 也 drop `source=codex` | 不能照搬 oc-claw 做 Windows Codex；需要单独验证 Codex 当前 hook/会话格式 |
| Cursor | 通过 `~/.cursor/hooks.json` + 自带 VS Code/Cursor extension，Windows 用 TCP `127.0.0.1:19284` | 可放到 E3/E4，复杂度高于 Claude Code |
| OpenClaw | 读取 `~/.openclaw/agents/*/sessions/*.jsonl` 和 `sessions.json`，用 JSONL 最后消息判断 active | 可借鉴 JSONL 活跃判定，但不要绑定 OpenClaw 格式到通用模型 |
| 权限提醒 | `PermissionRequest` 进入 `waiting`，前端显示工具名、Write/Edit/Bash 预览和四个操作按钮 | 我们可复用到 bubble/panel，但必须走 B4 审计和确认 |
| 状态排序 | Mini 面板按 waiting / compacting / working / idle 和更新时间排序 | 适合迁移到 Agent 管理页 |
| 统计 | Claude/Codex 从 JSONL 提取 token；Codex token_count 是累计快照，需转 delta；Cursor 不可靠，返回空统计 | 统计逻辑要按 source 分支，不能假设所有工具都有 usage |

最重要的修正：**oc-claw 源码并不证明 Windows Codex hook 已可用**。它保留了非 Windows Codex hook 和统计解析，但 Windows 分支明确写着 “Codex support is dropped on Windows”，并主动删除旧 hook。BitCat 是 Windows-first，所以 E1 里 Codex 只能列为“待验证适配”，不能直接按 oc-claw 实现路线排期。

### E1. Claude Code / Codex 会话监听

第一阶段不直接控制 Agent，只做可靠观察。目标是本地识别当前有哪些 AI 编码工具在运行、它们在哪个项目目录、最近是否有 token/文件/命令活动、是否进入等待用户输入状态。

当前 Claude Code / Codex 只读 hook MVP 已落地（2026-05-16 起，2026-05-22 更新）：`core` 侧已有 `AgentSession` / `ClaudeHookEvent` / `AgentNudgePolicy`，`app` 侧已有本地 TCP monitor、Claude/Codex hook installer、settings 集成、审计 JSONL、独立 `agent-watch` 浮动任务栈和统一顶部通知。Remote Agent Watch LAN ingest/viewer MVP 已归档，用户侧说明在 `docs/guide/remote-agent-watch.md`。它可以作为 E1/E2 的第一版基础。失败生命周期要分级处理：`StopFailure` 这类会话级失败才异常提醒，`PostToolUseFailure` 是 Claude Code 自我修复中的常见中间态，只记录并继续 working；`PermissionDenied` 进入 waiting，而不是按异常打扰。`SubagentStopFailure` 不是当前 Claude Code 支持的 hook event，只作为旧版 BitCat 配置的 Hook Doctor 清理对象。

建议监听源：

| 工具 | 第一版信号 | 后续增强 |
|------|------------|----------|
| Claude Code | `~/.claude/` 会话、transcript、hook 输出、进程命令行 | 标准化 hook bridge |
| Codex | Codex 工作区/线程元数据、git/worktree 状态、进程命令行；Windows hook 需重新验证 | Codex app 自动化事件 |
| OpenClaw / 其他工具 | 配置化目录扫描 + 进程探测 | 适配器式 connector |

本项目落地形态：

```text
agent_monitor_loop()
  ├── Claude hook TCP / 后续扫描进程 / 会话文件
  ├── 归一化为 AgentSession
  ├── 推送 agent-session-update 到 settings / agent-watch 浮动窗 / remote viewer
  ├── 通过统一顶部通知窗口发 waiting / done / error 提醒
  ├── 通过 PetEventBus 发低频状态反馈
  └── 写入 ~/.bitcat/logs/agent_watch_events.jsonl / agent_watch_sessions.jsonl / agent_watch_nudges.jsonl
```

`AgentSession` 建议字段：

- `tool`: `claude_code | codex | cursor | openclaw | custom`
- `workspace`
- `status`: `working | idle | waiting | blocked | done | error`
- `last_activity_at`
- `current_task`
- `branch`
- `token_hint`
- `risk_hint`

### E2. 桌宠化 Agent 状态管理

oc-claw 的状态宠物可以作为参考，但 BitCat 应把状态直接接进现有宠物身体语言：

| Agent 状态 | 桌宠表现 | UI 表现 |
|------------|----------|---------|
| `working` | Focused / Coding 动画，轻微键盘敲击粒子 | bubble 显示当前工具名和项目 |
| `waiting` | 举牌/歪头/轻微抖动，提醒主人处理 | panel 置顶显示“需要输入”的会话 |
| `blocked` | Confused 状态，短提示失败原因 | bubble 展示可执行的恢复动作 |
| `idle` | Idle / Sleep | panel 收纳到历史列表 |
| `done` | Happy / 小庆祝舞步 | 可一键打开 diff / PR / 测试结果 |

面板应新增“Agent 管理”页签：

- 当前活跃会话列表：工具、目录、分支、状态、最后活动时间。
- 快捷操作：打开终端、打开目录、复制下一步提示、暂停/恢复监控。
- 等待队列：优先显示需要用户确认、测试失败、merge conflict、权限询问的任务。
- 历史记录：按项目和日期查看最近 Agent 做过什么。

当前 UI 先落在独立 `agent-watch` 浮动任务栈、设置页“Agent 看管”区域和顶部通知窗口，暂未把完整 Agent 管理页塞进主 panel。后续如果要进入手柄工作流，再把浮动窗能力收敛到 panel 页签，并补“已查看”标记，避免 done/waiting 提醒重复打扰。

手柄映射建议：

- Start：打开当前最需要关注的 Agent 会话。
- D-pad：在活跃会话之间切换。
- A：打开对应终端/窗口或确认安全动作。
- B：收起提醒，保留低干扰状态。
- Select 长按：切换“专注看管 / 安静陪伴”模式。

### E3. 工具控制与安全边界

第二阶段再做控制能力，避免一上来变成不透明的“远程遥控 Agent”。原则和 B4 一致：模型和桌宠可以建议，Rust 负责权限、审计和边界。

优先做窄动作：

1. `open_agent_workspace`：打开会话对应目录或终端。
2. `copy_agent_prompt`：复制后续提示到剪贴板，让用户自己粘贴。
3. `run_project_test`：按项目约定执行 `make test-core` / `npx vitest run` 等安全命令。
4. `summarize_agent_session`：读取会话日志，生成短摘要写入记忆。
5. `nudge_agent`：仅对显式支持 hook/stdin 的工具发送下一步提示，默认需要确认。

所有控制动作都写入 `tool_events.jsonl` 或新的 `agent_actions.jsonl`，字段包含工具名、工作区、动作、触发来源、是否需要确认、结果预览和耗时。

### E4. 多工作区 / 远程机器管理

后续可以把 oc-claw 的“多 Agent 看板”扩展到多工作区：

- 本机多个 repo 的 Claude Code / Codex 会话统一汇总。
- WSL / SSH 远程机器通过轻量 sidecar 输出 JSONL 状态，本机桌宠只订阅。
- 对长任务设置心跳提醒：超过 N 分钟无活动、测试失败、等待输入时让桌宠提示。
- 与 B5 记忆系统联动：每个项目沉淀“Agent 做过什么、失败在哪里、下次怎么接上”。

### 与现有路线的关系

E 线不是替代 A/B/C，而是让 BitCat 从“自己能陪伴和协作”进化成“帮主人看管其他 AI 编码 Agent 的桌面伙伴”：

```text
B4 工具事件协议 ──→ E3 控制动作审计
B5 grep-first 记忆 ──→ E4 项目/会话历史召回
pet/bubble/panel ──→ E2 状态呈现与手柄操作
截图观察 ──→ 判断 Agent 是否卡在终端、浏览器、测试失败页
```

推荐最短路径：

1. E1 已有本地/远程只读 MVP：继续补 JSONL watcher、PID 存活检测和真实 hook 端到端回归。
2. E2 已有浮动任务栈和顶部通知：下一步收敛到 panel 页签，补已查看/静音/置顶等交互。
3. 复用 B4：把会话控制动作纳入同一套审计和安全提示。
4. 最后考虑 E3/E4：可控唤醒、多工作区、远程机 sidecar。

---

## Track D: 商业化

### D1. Steam 发布准备

| 项目 | 要求 | 当前状态 |
|------|------|---------|
| Steamworks SDK | 集成 | 待做 |
| App ID | $100（可回收） | 待申请 |
| 年龄评级 | IARC（免费） | 待做 |
| Store 页面 | capsule + 截片 + trailer | 待准备 |
| AI 内容披露 | 实时生成内容声明 | 需要（舞蹈+游戏+对话+视觉分析） |
| 自动更新 | tauri-plugin-updater | Tauri 已支持 |

### D2. 定价建议

| 对比产品 | 价格 | 特点 |
|---------|------|------|
| VPet | 免费 | 开源社区驱动 |
| Weyrdlets 2.0 | $5-8 | 有迷你游戏但无 AI |
| AI Desktop Pet | ~$8 | Live2D + 本地 LLM + Workshop |
| **BitCat** | **$5-7** | 桌面陪伴 + AI 对话 + **AI 生成内容** + 开源 |

---

## 实施优先级与依赖关系
```
已完成      ┌─────────────────────────────────────┐
2026-05-13 │  A1 舞蹈系统                         │
           │  B1 日志规范化第一轮                  │
           │  B2 Token 追踪 + 设置页统计            │
           │  B3 Extractor 主链路                  │
           │  B4 工具运行时事件与审计               │
           │  B5 grep-first 记忆主链路              │
           │  B6 提醒 + 顶部通知 + 提示音            │
           │  E1/E2 本地/远程 Agent Watch MVP       │
           │  测试入口/Makefile/xtask 稳定化        │
           └─────────────────────────────────────┘
                  ↓
短期        ┌─────────────────────────────────────┐
1-3天      │  A2 迷你游戏引擎 Phase 2              │  ← AI play_game/perform_game + Memory/Catch
           │  E2 Agent Watch panel 收敛             │  ← 已有浮动窗/顶部通知，补手柄工作流
           │  Observation Hints                     │  ← 让截图观察变成可复用提示资产
           └─────────────────────────────────────┘
                  ↓
中期        ┌─────────────────────────────────────┐
1-3天      │  宠物资源包发布策略                    │
           │  E3 Agent 控制动作与安全审计           │
           │  A3 内容扩展（更多游戏类型）           │
           └─────────────────────────────────────┘
                  ↓
大块        ┌─────────────────────────────────────┐
4-8天      │  C1 桌宠 3D 体素化                    │
           │  E4 多工作区 / 远程机器 Agent 管理     │
           │  C2/C3 3D 动画与游戏能力               │
           └─────────────────────────────────────┘
```

### 关键依赖
```
B1(日志) ──→ B2(Token追踪) 已完成，提供干净观测面
B2(Token) ─→ B4(工具运行时) 用真实数据决定是否值得优化
A1(舞蹈) ──→ A2(游戏) 复用同一模式
B3(Extractor) ──→ B5(文本记忆) 结构化摘要更容易 grep 和压缩
B4(工具运行时) ──→ E3(Agent控制) 复用权限、生命周期事件和审计日志
E1(会话监听) ──→ E2(桌宠管家) 先只读观察，再做状态提醒
E2(桌宠管家) ──→ E3(控制动作) 只对明确会话提供窄动作入口
B5(文本记忆) ──→ E4(多项目历史) 让 Agent 会话摘要可 grep、可回忆
A1/A2(内容型工具) ─→ B4(工具运行时) 提供舞蹈/游戏两类真实样本，验证工具事件协议
B4(工具运行时) ──→ 控制固定 prompt 成本，给记忆候选留预算，也为工具事件记忆化打基础
C1(3D化) ──→ C2/C3 渲染层就绪
A1+A2+E1/E2+C1 ──→ D1(Steam) MVP 差异化更完整
```

---

## 工作量估算

### 个人节奏校准

以下估算按最近 git 记录校准，而不是按通用团队排期估算。最近提交节奏：

| 日期 | 提交数 |
|------|--------|
| 2026-05-08 | 10 |
| 2026-05-09 | 23 |
| 2026-05-10 | 8 |
| 2026-05-11 | 23 |
| 2026-05-12 | 45 |
| 2026-05-13 | 44 |

这个节奏说明：小到中等工程切片通常能在同一天内完成并提交多轮；真正需要跨天的是渲染重写、Steam 发布这类不确定性高的块。因此后续不再使用“周”为默认单位，而使用“个人开发日”。

| Track | 内容 | 新代码量（估） | 新依赖 | 优先级 |
|-------|------|---------------|--------|--------|
| **B1** | 日志规范化 | 已完成第一轮 | 0 | Done |
| **A1** | 舞蹈系统 | 已完成 MVP | 0 | Done |
| **B2** | Token 追踪 + 设置页统计 | 已完成 MVP | 0 | Done |
| **B3** | Extractor 改造主链路 | 已完成 | 0 | Done |
| **B3 cleanup** | 删除旧 raw helper / parser / 惰性配置 | 已完成，净删为主 | 0 | Done |
| **Pet v2 assets** | 宠物 manifest loader、默认 `piggy`、`cat` 资源包命名、catalog preset | 已完成；后续只剩发布包体积分层和用户目录加载 | 0 | Done/P1 packaging |
| **B6** | 程序化提醒 + 顶部通知 + 提示音 | create/list/cancel、scheduler、notification island、AI personalizer 已完成；剩余费用/批量/更多动作工具打磨 | 0 | Done/P2 follow-up |
| **E1** | AI 编码工具会话监听 | 本地 Claude/Codex hook + Remote LAN ingest/viewer MVP 已完成；剩余 JSONL watcher、PID 存活和端到端回归 | 0 | Done/P1 follow-up |
| **E2** | 桌宠化 Agent 状态管理 | 独立浮动任务栈 + 顶部通知已完成；剩余 panel 收敛、已查看去重和手柄入口 | 0 | Done/P1 follow-up |
| **A2** | 迷你游戏引擎 | Phase 1 已完成；`start_game(GameDef)` / `cmd_start_game_with_def` 已可启动任意 GameDef；Phase 2 待接 AI 工具 + Memory/Catch + 持久化 | 0 | P1 |
| **B4** | 工具运行时与开销优化 | 生命周期事件、bubble UI、审计日志和首轮 schema 压缩已完成；动态能力包暂缓 | 0 | Done/P2 follow-up |
| **E3** | 远程/多工作区 Agent 管理 | ~400-800 行 | SSH 可选 | P2 |
| **A3** | 内容扩展 | ~200-350 行/种 | 0 | P2，0.5-1 天/种 |
| **B5** | grep-first 文本记忆 | JSONL/id/软删除/search_memory 主链路已完成；剩余上下文瘦身和候选压缩 | 0 | Done/P2 follow-up |
| **C1** | 3D 体素化 | ~1200-1800 行 | three.js | P3，4-8 天 |
| **C2** | 动画增强 | ~300-500 行 | 0 | P3，1-3 天 |
| **C3** | 3D 游戏生成 | ~700-1000 行 | cannon-es 等 | P3，3-6 天 |
| **D1** | Steam 发布 | 集成工作 | Steamworks SDK | P4，2-5 天 |

**当前可玩 Demo 的基础设施已超过原 MVP 预期；下一阶段最短路径是 A2 Phase 2 → E2/E3 → Observation Hints，让“AI 生成可玩内容”“看管其他 Agent”和“观察经验沉淀”进入同一套可审计闭环。**

### 当前打磨队列

这些项已经有可用主链路，后续不按“大功能从零实现”估算，而按体验和可靠性收尾推进：

| 领域 | 当前边界 | 打磨目标 |
|------|----------|----------|
| B6 提醒与顶部通知 | 已支持确定性提醒、顶部通知、提示音和可选 AI 润色 | 控制 AI 润色费用/频率；优化多个提醒同时到期；补失败诊断；评估 complete/snooze 是否开放给 Agent 工具 |
| E1/E2 Agent Watch | 已支持本地/远程只读 hook、浮动任务栈和顶部通知 | 补 JSONL watcher、PID 存活检测、结构化 Write/Edit/Bash 预览、panel 收敛和已查看/静音/置顶 |
| B4 工具运行时 | 生命周期事件、bubble UI 和审计日志已可用 | 用真实 token/工具日志决定 schema 预算和 dynamic tools，不做关键词意图识别 |
| B5 记忆 | grep-first 长期记忆主链路已可用 | 减少默认预塞上下文；让 `search_memory` 按需召回后再由模型压缩判断 |
| Pet v2 assets | 内置 v2 pack 已可切换 | 明确 bundle vs 外部包边界、用户目录加载、资源诊断和 Steam/DLC 分层 |
| 音乐响应舞动 | 第一版音乐模式可用 | 增强舞感状态机、fake source 诊断、节奏/静音/高潮回落表现 |

---

## 数据目录规划

```
~/.bitcat/
├── dances/              # AI 生成的舞蹈 (A1)
├── games/               # AI 生成的游戏 (A2)
├── screenshots/         # 已有
├── camera/              # 摄像头观察记录（默认关闭，开启后写 analysis JSON/可选帧图片）
├── memory/              # 已有：chat_summary.json + long_term.jsonl grep-first 记忆
├── logs/
│   ├── bitcat.YYYY-MM-DD.log
│   ├── token_usage.jsonl    # Token 追踪行日志 (B2)
│   ├── token_sessions.json  # 会话级汇总 (B2)
│   ├── tool_events.jsonl    # 工具生命周期审计 (B4)
│   ├── reminder_events.jsonl # 提醒生命周期与存储异常 (B6)
│   ├── agent_watch_events.jsonl # Agent Watch 原始归一事件 (E1/E2)
│   ├── agent_watch_sessions.jsonl # Claude Code / Codex 等会话状态 (E1/E2)
│   ├── agent_watch_nudges.jsonl # Agent Watch 提醒决策 (E1/E2)
│   └── agent_actions.jsonl  # 桌宠触发的 Agent 控制动作 (E3)
├── reminders/
│   └── reminders.json       # 程序化提醒 store (B6)
├── agents/
│   ├── sessions.json        # 当前活跃 Agent 会话缓存 (E1/E2)
│   └── connectors.yml       # 自定义工具/目录/hook 适配配置 (E1/E4)
├── workshop/            # Steam Workshop 订阅内容 (A3)
└── config/
    ├── actions.yml
    ├── buttons.yml
    └── prompts.yml
```
