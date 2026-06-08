# BitCat Roadmap

> **目标**：将 BitCat 打磨为 **Steam 可发布的 AI 驱动桌面伴侣**。
> **核心差异化**：AI 通过结构化输出动态生成可玩内容（舞蹈 + 迷你游戏），而非仅对话。

---

## 当前基线

| 能力 | 状态 |
|------|------|
| v2 宠物资源包系统（manifest + spritesheet + 设置页选择） | 已落地（2026-05-17）；最终内置只保留 15 个 `cat-*` 品种，默认 `cat-tabby` |
| 语义宠物动画（非均匀帧时长 + 瞬态 repeat+fallback + idle variants） | 已有（2026-05-13 增强，2026-05-17 收敛到 v2 pack） |
| AI 对话（Anthropic Claude via rig-core，流式输出） | 已有 |
| 16 个内置工具（launch/shell/read_file/get_time/recent_screenshots/search_memory/remember/reminder/hotkey/clipboard/foreground/dance/start_game 等） | 已有；工具说明以 rig schema 为主，prompt 只保留高风险政策 |
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
| 积分/等级/成就薄片 | 已落地（2026-05-30）；points JSONL + 聚合状态 + 设置页展示 |
| Bubble reader / 工具状态 UI | 已增强（2026-05-30）；阅读态、工具状态条和表演型工具退场体验已接入 |
| AI 流错误分类与用户友好兜底 | 已落地（2026-05-30）；可恢复错误进入 fallback 文案，避免空白失败 |
| Invasion / 桌面入侵核心玩法 | MVP 已落地（提交 `cc6461c`）；真实 `GameProjection` 接入已完成本地数据验证，待提交和完整窗口触发回归 |

## 当前下一步（2026-06-08）

宠物资源线已经收敛：最终软件只内置 15 个精修 `cat-*` 品种，旧的非猫形象不再进入前端 bundle。下一步不再继续扩资源包，而是转向 Steam Demo 最短路径：

1. **D1 Demo 闸门**：补首次启动权限向导和干净 Windows smoke checklist，确保 AI、截图、摄像头、shell、文件、剪贴板等高权限能力可解释、可关闭、可回归。
2. **D4 Invasion 收口**：把 `Invasion` 从 MVP 打磨成 Demo 核心切片，完成真实窗口触发回归、敌人节奏/目标反馈、手柄/键盘路径一致性和专属积分事件。
3. **A2/B7 分数与成长闭环**：补分数 JSONL、Invasion 专属 points/achievement、以及发布版能力开关，为 Steam Achievements/Cloud 打本地事实基础。
4. **D2/D3 Steam 平台与合规**：随后接 SteamPipe、成就/统计、云存档排除敏感目录、AI 内容披露和数据删除入口。

## 源码确认的技术栈

BitCat 当前不是 Web 应用套壳，而是 **Windows-first 的 Rust 桌面自动化程序 + Tauri 多透明 WebView 界面 + rig Agent 运行时**。`oc-claw` 可参考产品模型和会话状态抽象，但不建议照搬它的前端/桌面技术栈；本项目已有更贴近宠物交互和手柄场景的底座。

| 层级 | 技术 | 源码依据 | 说明 |
|------|------|----------|------|
| Workspace | Rust workspace：`core` / `app` / `xtask` | `Cargo.toml` | 逻辑、桌面壳、维护命令分离 |
| Core crate | Rust 2024 + `bitcat-core` | `core/Cargo.toml` | 纯逻辑层，无 Tauri/UI 依赖，便于快速测试 |
| App crate | Rust 2021 + Tauri 2 | `app/Cargo.toml`, `app/tauri.conf.json` | 桌面窗口、托盘、全局快捷键、IPC |
| UI Runtime | WebView2 via Tauri | `tauri.conf.json` `frontendDist: ./frontend` | 多窗口透明桌宠，而非浏览器页应用 |
| Frontend | Vanilla HTML/CSS/JS + Canvas | `app/frontend/*.html`, `app/frontend/js/*.js` | 无 React/Vue/构建步骤；Node 只用于测试 |
| Pet Assets | v2 manifest + 15 个内置猫咪品种 | `app/frontend/__fixtures__/pets/*/manifest.json`, `app/frontend/js/sprite-loader.js` | 宠物视觉不再依赖硬编码默认 sprite fallback；最终 bundle 只保留 `cat-*` 品种，默认加载 `cat-tabby` |
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
│  当前主流程：Steam Demo → Early Access → 正式版；先补核心玩法、权限合规和 Steam 集成│
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

Phase 1 已完成（提交 `a2105ff`）：新增全屏透明 `game` 窗口并加入默认游戏入口，内置 Snake 可通过键盘/手柄游玩，结束后联动 `GamePlay` / `GameWin` / `GameLose` 宠物状态。当前内置游戏入口已扩展为 Snake / Memory / Catch / Battle / Gomoku / Arena / Beads / Invasion，AI 可通过 `start_game(kind)` 启动这些内置游戏。

当前已落地的数据流：

```
panel / AI start_game → ActionBus 内置游戏动作
      → cmd_start_game / cmd_start_memory / cmd_start_catch / cmd_start_battle / cmd_start_gomoku / cmd_start_invasion
      → app/src/game.rs 动态创建 game 窗口
      → game.html / game_engine.js + js/games/* 运行 Snake / Memory / Catch / Battle / Gomoku / Invasion
      → cmd_game_end(result, score) → 关闭窗口 + 切换 pet 状态
```

2026-05-30 增量：`start_game(kind)` 已作为 AI 工具注册，走 `core::game_request` bridge 到 app 的 ActionBus，只接受内置枚举，不生成代码。2026-06-07 增量：`invasion` 已接入 `StartGameKind` / `MinigameType` / ActionBus / panel / Tauri IPC，玩法主体放在独立 `app/frontend/js/games/invasion.js`，`game_engine.js` 只保留外部游戏注册、HUD 和输入壳层。下一步继续复用 A1 的模式：模型通过未来 `perform_game` 提交结构化 `GameDef` → Rust validate / save → game 窗口运行游戏 → 结束联动 pet 状态。当前尚未完成的是 GameDef 持久化、用户自定义预设和分数 JSONL。

内置原型游戏共享同一个 game window 生命周期：

| 游戏 | 操作 | 复杂度 |
|------|------|--------|
| 毛线球大作战（Snake） | 方向键转向，A 加速 | 已落地 |
| 翻牌配对（Memory） | 方向键移动 + A 翻牌 | 已落地 |
| 接食物（Catch） | 方向键移动篮子 | 已落地 |
| 飞机守护战（Battle） | 方向键移动，A 射击，X/Y/L1 技能 | 已落地 |
| 五子棋（Gomoku） | 棋盘落子 + AI 思路/讲解 | 已落地；结构化 commentary 已稳定 |
| 猫猫擂台（Arena） | 3D 对战训练 | 已落地 |
| Pixel Beads（Beads） | 调色/放置/撤销 | 已落地 |
| Desktop Invasion（Invasion） | 方向键移动，A/点击守护目标 | MVP 已落地；真实投影接入待提交 |

输入已改为游戏激活时独占：D-pad/A/B/Start 由 `gamepad_loop` 转发为 `game-input`，普通滚轮、宠物动作和面板动作暂停。胜利→`GameWin`，失败→`GameLose`，取消→`Idle`。`Invasion` 当前使用安全 `GameProjection`：长期记忆、活跃提醒和 Agent Watch 会话只投影为短标题、类别和权重；小游戏不会获得隐私正文、提醒 message、raw hook 输入或真实控制/删除能力。

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

当前 16 个工具仍全量注册到每次对话，但 B4 第一阶段已经完成了更基础的运行时治理：工具调用不再混入 bubble 正文，而是通过结构化事件单独呈现；普通工具显示低干扰状态条，舞蹈和 `start_game` 这类表演/互动型工具显示短提示并退场；工具结果会写入 `~/.bitcat/logs/tool_events.jsonl`，用于后续统计成功率、耗时和拦截次数。

新的原则保持不变：**默认相信大模型自己选择工具，Rust 负责 schema、权限、生命周期事件、审计和体验呈现**。2026-05-30 已进一步减少“双写”：普通工具说明以 rig 原生 `ToolDefinition.description` 和参数 schema 为准，`build_tool_guide_prompt()` 只保留高风险/容易误用的工具政策。B4 现在已经足够支撑下一阶段游戏工具：未来 AI 层 `perform_game` 这类生成型工具应复用 `ToolKind::Performance` 或扩展出更精细的 kind，让 bubble 退到辅助位置，把主视觉交给 pet/panel/game 窗口。

建议拆分：

1. **B4.1 已完成：工具生命周期事件协议**：`planned / blocked / finished / failed` 已接入，携带 `tool_name`、`internal_call_id`、结果预览、耗时和结果状态。`allowed` 不做伪事件，除非以后把 `PermissionHook` 改为带事件 sink 的状态化 hook。
2. **B4.2 已完成：bubble 表演型工具状态 UI**：普通工具显示安静状态条；`perform_dance` / `play_dance` 走“正在编舞 / 准备开跳 / bubble 退场”的舞台体验。
3. **B4.3 已完成：工具事件审计日志**：`tool_events.jsonl` 记录成功率、失败/拦截、耗时和短结果预览，不记录大文本。
4. **B4.4 已做两轮：schema/description 压缩 + 工具政策瘦身**：已低风险压缩 `perform_dance` / `play_dance` 文案，并移除 prompt 中的普通工具目录。真实 token 预算工具暂缓，等工具继续增长或真实日志显示固定 schema 成为瓶颈再做。
5. **B4.5 暂缓：显式能力包 / dynamic_tools 实验**：当前不阻塞游戏部分。仅在真实数据证明必要时启用，必须 feature flag 可回滚。

下一步主线建议：先做 GameDef 持久化和成长权限部分。游戏工具应直接复用 B4 已完成的事件协议、UI 分层和审计日志；不要再引入关键词分类或额外小模型判断。

详细设计：[plan/archive/rig-capability-roadmap.md](plan/archive/rig-capability-roadmap.md) §P1

### B5. grep-first 文本记忆检索

长期记忆的 grep-first 主链路已落地：`LongTermMemory` 使用 `~/.bitcat/memory/long_term.jsonl`，一行一条当前有效 record，包含稳定 `id`、`created_at` 和 `deleted` 软删除字段，并同步生成 `long_term.md` 作为人类/rg 友好的审查视图；`record_candidate()` / `remember` 写入结构化候选，`retrieve_with()` 按 text/tag/source/min_importance 做可解释召回并最多返回 20 条候选，`search_memory` 支持按需指定返回条数和字符预算。本项目仍不采用 Embeddings / Vector RAG。2026-05-30 调整了短期记忆截断默认值：单条 user 500 字符、reply 1000 字符；自动长期记忆注入预算为 8000 字符，工具按需检索预算上限为 12000 字符。后续 B5 剩余工作是候选压缩和按需召回策略。

详细取舍：[architecture/design-tradeoffs.md](architecture/design-tradeoffs.md)

### B6. 程序化提醒与顶部通知

提醒主链路已落地：AI Agent 通过 `create_reminder` / `list_reminders` / `cancel_reminder` 管理确定性提醒，`core/src/reminder.rs` 负责 store、原子写入和生命周期事件，`app/src/reminder_scheduler.rs` 每 5 秒扫描到期提醒，`app/src/notification_window.rs` 用统一顶部通知窗口呈现提醒、完成、稍后、取消和删除动作。

2026-05-22 增量：

1. **顶部通知岛**：reminder 和 Agent Watch 共用同一个通知窗口，支持队列、去重、动作按钮和来源字段。
2. **提示音**：设置页可按来源配置系统提示音，覆盖 `info / success / warning / danger` 等等级，失败时只记录日志，不影响通知展示。
3. **AI 提醒润色**：到期提醒可选调用无工具结构化 `ReminderPersonalizer`，根据标题、备注、到期时间和用户上下文生成更自然的短提醒；默认关闭，失败时回退确定性原文，prompt 统一在 `config/prompts.yml` 的 `reminder_personalizer` 段。
4. **可诊断性**：提醒生命周期写入 `~/.bitcat/logs/reminder_events.jsonl`，字段保留 `reminder_id`、`source`、`ui_source`、`store_path` 和异常上下文。

后续只保留打磨项：根据真实使用决定是否把 `complete_reminder` / `snooze_reminder` 也暴露给 Agent 工具；完善到期批量并发、费用门控和设置页的失败诊断。

### B7. 积分 / 等级 / 成就薄片

2026-05-30 已落地第一片：`core/src/points.rs` 记录互动积分、等级、连续活跃和 12 个内置成就，事件明细追加到 `~/.bitcat/logs/points_events.jsonl`，聚合状态写入 `~/.bitcat/points_state.json`，设置页展示当前状态、最近事件和成就视图。

已接入的事件包括文字/语音对话、长期记忆创建、提醒创建/完成、舞蹈、游戏启动/胜利、截图/摄像头观察、每日登录和 A 键夸奖宠物。它对应 [plan/progression-capability-unlock.md](plan/progression-capability-unlock.md) 中“Bit/积分 + 成就”的最小闭环。

后续不要平行新建重复账本。成长上下文、权限 gate、商店、每日任务和心情系统应复用 `points` 的事件和状态，再在更高层补 `Progression`/授权 overlay。

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
- ~~硬编码小猫迁移到资源包~~ ✅ **已完成 (2026-05-17)** — v2-only manifest loader，默认 `cat-tabby`；2026-06-08 收敛为 15 个内置猫咪品种
- ~~语义短动作 overlay~~ ✅ **已完成 (2026-05-17)** — manifest action timeline，`observe/nudge/acknowledge/blocked/dragging` 可用于截图、输入和拖拽反馈
- ~~宠物资源包发布策略~~ ✅ **已完成 (2026-06-08)** — 最终 bundle 只打包 15 个 `cat-*` 品种，旧非猫形象从前端资源目录移除；见 [plan/pet-asset-packaging.md](plan/pet-asset-packaging.md)
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

### D1. Steam 发布主流程

BitCat 当前已经具备可打包、可运行、可接 Steamworks 的 Windows 桌面应用底座，但还不能直接按“正式 Steam 游戏”发布。当前更准确的定位是：**AI 桌面伙伴 + 轻游戏体验**。下一阶段主流程不再只按 A/B/C/E 模块推进，而是按 Steam 发布闸门推进：先做可审查 Demo，再做 Early Access，最后收束正式版。

核心判断：

- 技术底座已有：Tauri bundle、`make release` / `make dist`、portable zip、Steamworks 启动探针、多窗口桌宠和 5 个内置小游戏。
- 最大缺口不是“能不能启动”，而是“玩家买到的核心游戏闭环是什么”。当前 Snake / Memory / Catch / Battle / Gomoku 更像薄片，需要补一个 BitCat 专属主玩法。
- Steam 审核风险主要来自 AI、截图/摄像头观察、读文件、shell、剪贴板和前台控制等高权限能力。发布版必须默认收敛权限，并把授权、隐私、日志和删除路径讲清楚。
- Steam 平台能力仍处于探针级：`app/src/steam.rs` 只验证 DLL/AppID/客户端链路，尚未接成就、统计、云存档、SteamPipe depot 或 Workshop。

推荐发布节奏：

| 阶段 | 目标 | 必须完成 | 不做/后置 |
|------|------|----------|-----------|
| **Steam Demo / Playtest** | 验证安装、启动、权限说明、AI 对话、小游戏窗口和 Steam 链路 | 干净机器 smoke test、首次启动权限页、AI 内容披露草案、SteamPipe 上传脚本、最小 Steam init 诊断、`Invasion` 可反复游玩的核心玩法切片 | DLC、Workshop、复杂社区功能、完整 RPG |
| **Early Access** | 以“AI 桌面伙伴 + 轻游戏”诚实发售 | `Invasion` 语义化小游戏深化、积分/成就闭环、Steam Achievements、Steam Cloud、隐私/数据删除、崩溃恢复、商店素材 | 大规模 3D 重写、多人/借猫社区、生成完整 3D 关卡 |
| **正式版** | 功能承诺、商店页和实际体验一致 | 核心玩法稳定、权限/合规成熟、Steam 平台功能完整、资产/包体策略稳定、长期存档迁移策略、回归测试矩阵 | 未验证的高风险 AI 控制能力 |

### D2. Steam 工程清单

| 项目 | 发布要求 | 当前状态 | 下一步 |
|------|----------|----------|--------|
| AppID / Partner 设置 | Steam 后台创建应用、配置 depot、分支和商店基础信息 | 待申请/配置 | 先建立 dev AppID，明确 Demo/Playtest/主应用关系 |
| Steamworks SDK | 运行时初始化、成就/统计/云存档 API | 只有 `steam_api64.dll` 动态加载探针 | 接入正式封装层，保留非 Steam 构建可运行 |
| SteamPipe | 上传 build 到 depot，维护 default/beta/internal 分支 | 目前只有 portable zip | 在 `xtask` 增加或旁路维护 SteamPipe build 脚本，生成上传清单 |
| Steam Achievements | 映射游戏内成就到 Steam 成就 | 本地 points/12 个成就已可用 | 把 points 事件桥接到 Steam stats/achievements，离线失败可重试 |
| Steam Cloud | 同步进度和设置 | 本地 `~/.bitcat` 数据目录已规划 | 只同步非敏感数据：points、小游戏分数、宠物状态、用户可选设置；记忆/截图/摄像头记录默认不同步 |
| Store 页面 | capsule、截图、预告片、短描述、长描述、标签、系统需求 | 待准备 | 先按 Demo 口径写“一句话承诺”，避免夸大 AI 能力 |
| Build Review | 商店页功能和实际 build 一致，干净环境可启动 | 未做 Steam 环境回归 | 建 Windows 干净机 smoke checklist：安装、启动、关闭、重启、权限、手柄、小游戏、AI fallback |
| 自动更新 | Steam 默认负责分发；Tauri updater 可后置 | Tauri 支持但不必首发接 | Steam 版优先走 Steam 更新，非 Steam portable 再评估 updater |

### D3. AI、隐私与高权限合规

Steam 版必须把 AI 能力和高权限能力做成可解释、可关闭、可审计的产品面，而不是隐藏在设置深处。

发布前必做：

1. **首次启动权限向导**：明确 AI 对话、截图观察、摄像头观察、文件读取、shell、剪贴板、前台窗口控制分别会做什么；截图可默认启用但必须可见可关，摄像头继续默认关闭，shell/文件/剪贴板保持显式确认或安全边界。
2. **AI 内容披露**：商店页和内容问卷说明使用运行时 AI：对话、舞蹈/小游戏结构化生成、视觉分析、提醒润色。写清楚 guardrails：权限 gate、工具 schema 校验、危险命令拦截、摄像头保守描述、敏感属性不推断。
3. **隐私与数据删除**：设置页提供数据位置、导出/清理入口，覆盖 memory、screenshots、camera、logs、points、reminders、agent watch。Steam Cloud 默认排除敏感目录。
4. **发布版默认权限收敛**：高风险工具不因“AI 想调用”自动执行。Rust 负责 schema、权限、审计和执行边界，模型只负责建议和生成结构化意图。
5. **诊断日志分级**：发布版日志继续保留故障复盘字段，但大文本、图片、隐私正文不裸写 INFO；用户可以关闭或清理诊断数据。

### D4. 核心玩法收束

Steam 玩家需要一个明确理由每天打开 BitCat。下一阶段不建议只增加第 6 个传统小游戏，而是做一个能体现项目差异化的 BitCat 语义化小游戏。

推荐主玩法原型：`Invasion / 桌面小怪入侵`。MVP 已落地，下一步从“能玩”转向“可作为 Steam Demo 核心切片反复玩”。

- 小怪从游戏窗口或屏幕边缘出现，试图偷走鱼干、记忆碎片、提醒便签、Agent 任务卡等“投影目标”。
- 这些目标只影响本局分数、反馈和宠物情绪，不直接修改真实记忆、提醒或 Agent 任务。
- Rust 只暴露安全摘要，不把隐私正文、真实控制权或删除能力交给小游戏。
- 胜负仍走现有 `game` 窗口生命周期、`GameWin` / `GameLose` 宠物状态、points 事件和成就系统。

当前实现状态：

| 项 | 状态 | 下一步 |
|----|------|--------|
| 独立前端玩法文件 | 已完成：`app/frontend/js/games/invasion.js`，外部注册到 `window.BitCatGames.invasion` | 调整敌人节奏、目标布局和视觉反馈 |
| Rust 游戏入口 | 已完成：`MinigameType::Invasion`、`StartGameKind::Invasion`、`cmd_start_invasion`、panel/AI 启动路径 | 补游戏窗口真实触发回归，确认 panel 第 8 项、手柄/键盘路径一致 |
| 安全投影模型 | 已完成：`core/src/game_projection.rs`，只含 `kind/title/weight` | 继续扩大投影来源，但坚持不传隐私正文和控制能力 |
| 真实数据接入 | 已完成本地验证：长期记忆进入目标；提醒无活跃项时自动缺席；Agent Watch 使用 display 摘要 | 提交当前接入变更；补 app IPC 单元测试或 Tauri mock 测试 |
| 成长/成就闭环 | 部分已有：游戏启动/胜利已进入 points | 增加 Invasion 专属事件：守护目标数、连击、每日防守、无损胜利 |

这条线的目标不是做大 RPG，而是把 BitCat 已有系统变成可玩的闭环：

```text
memory/reminder/agent_watch/points 安全摘要
        ↓
Invasion 本局目标和风险
        ↓
game window 输入/胜负/分数
        ↓
PetEvent + points + achievements + Steam stats
        ↓
形成“陪伴关系正在被游戏化”的可售卖体验
```

### D5. 商店定位与定价建议

一句话定位建议：

> 一只会记住你、陪你工作、偶尔把桌面变成小游戏战场的 AI 桌面伙伴。

商店页不要把 BitCat 写成通用 AI 助手或完整 RPG。更稳妥的品类是：桌面宠物、AI companion、casual、utility-light game、cozy/productivity companion。正式卖点应围绕“桌宠陪伴 + 可玩互动 + 本地成长 + 玩家可控权限”。

| 对比产品 | 价格 | 特点 |
|---------|------|------|
| VPet | 免费 | 开源社区驱动 |
| Weyrdlets 2.0 | $5-8 | 有迷你游戏但无 AI |
| AI Desktop Pet | ~$8 | Live2D + 本地 LLM + Workshop |
| **BitCat Demo** | 免费 | 验证 AI 桌宠和核心玩法切片 |
| **BitCat Early Access** | **$4.99-6.99** | 桌面陪伴 + AI 对话 + 轻游戏 + 成长/成就 |
| **BitCat 正式版** | **$6.99-9.99** | 更完整玩法、Steam 成就/云存档、资产包和长期打磨 |

### D6. 首发质量闸门

发布前每个 build 必须过这些检查：

- `make test-fast`
- `cd app/frontend && npx vitest run`
- `make release` 或 `make dist`
- 干净 Windows 用户目录启动，不依赖开发环境 `.env` 或源码路径
- 无 Steam 客户端 / 有 Steam 客户端 / Steam AppID 缺失三种模式均有可解释行为
- 首次启动权限向导可完成，关闭截图/摄像头/AI 后不会反复打扰
- 游戏窗口可打开、输入、结束、关闭，手柄独占不会卡住普通桌宠操作
- AI API 不可用时有友好 fallback，不出现空白气泡或未创建却口头承诺的提醒/任务
- 日志和用户数据目录可在设置页定位并清理

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
1-3天      │  D1 Demo 闸门：权限向导 + smoke 清单    │  ← 先保证可审查、可解释、可回归
           │  D4 Invasion MVP 收口                  │  ← 已能玩；补真实窗口触发、手感、专属积分事件
           │  A2 分数 JSONL / GameDef 持久化         │  ← 为 Demo 分数、成就和云存档打底
           │  B7 成长上下文 / 权限 gate             │  ← points 已有，补发布版能力开关
           └─────────────────────────────────────┘
                  ↓
中期        ┌─────────────────────────────────────┐
2-5天      │  D2 SteamPipe / Achievements / Cloud  │
           │  D3 AI 披露、隐私删除、敏感目录排除     │
           │  E2 Agent Watch panel 收敛             │  ← 作为 Early Access 差异化配套
           └─────────────────────────────────────┘
                  ↓
大块        ┌─────────────────────────────────────┐
4-10天     │  D5 商店素材 + Demo/EA 分支回归        │
           │  E3 Agent 控制动作与安全审计           │
           │  C1 桌宠 3D 体素化                    │
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
D1(权限/Steam闸门) ──→ Demo/Playtest 可审查
D4(Invasion) ──→ Early Access 核心玩法承诺
A2+B7 ──→ Steam Achievements / Stats / Cloud 的本地事实来源
D3(合规) ──→ 发布版默认权限收敛，降低 AI/高权限审核风险
C1(3D化) ──→ C2/C3 渲染层就绪，但不阻塞 Demo/EA
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
| **Pet v2 assets** | 宠物 manifest loader、默认 `cat-tabby`、15 个内置猫咪品种、catalog preset、最终 bundle 收敛 | 已完成；用户目录加载和资源诊断转为 P2 可选扩展 | 0 | Done |
| **B6** | 程序化提醒 + 顶部通知 + 提示音 | create/list/cancel、scheduler、notification island、AI personalizer 已完成；剩余费用/批量/更多动作工具打磨 | 0 | Done/P2 follow-up |
| **E1** | AI 编码工具会话监听 | 本地 Claude/Codex hook + Remote LAN ingest/viewer MVP 已完成；剩余 JSONL watcher、PID 存活和端到端回归 | 0 | Done/P1 follow-up |
| **E2** | 桌宠化 Agent 状态管理 | 独立浮动任务栈 + 顶部通知已完成；剩余 panel 收敛、已查看去重和手柄入口 | 0 | Done/P1 follow-up |
| **A2** | 迷你游戏引擎 | Phase 1 已完成；Snake/Memory/Catch/Battle/Gomoku 可玩；AI `start_game(kind)` 已接入；Phase 2 待做 GameDef 持久化、分数 JSONL 和未来 `perform_game` | 0 | P1 |
| **B4** | 工具运行时与开销优化 | 生命周期事件、bubble reader/tool UI、审计日志、schema 压缩和工具政策瘦身已完成；动态能力包暂缓 | 0 | Done/P2 follow-up |
| **E3** | 远程/多工作区 Agent 管理 | ~400-800 行 | SSH 可选 | P2 |
| **A3** | 内容扩展 | ~200-350 行/种 | 0 | P2，0.5-1 天/种 |
| **B5** | grep-first 文本记忆 | JSONL/id/软删除/search_memory 主链路已完成；剩余上下文瘦身和候选压缩 | 0 | Done/P2 follow-up |
| **B7** | 积分/等级/成就 | points 事件、等级、成就和设置页展示已完成；剩余成长上下文、权限 gate、商店/每日任务/心情 | 0 | Done/P1 follow-up |
| **C1** | 3D 体素化 | ~1200-1800 行 | three.js | P3，4-8 天 |
| **C2** | 动画增强 | ~300-500 行 | 0 | P3，1-3 天 |
| **C3** | 3D 游戏生成 | ~700-1000 行 | cannon-es 等 | P3，3-6 天 |
| **D1-D6** | Steam 发布主流程 | Demo/EA 闸门、权限合规、SteamPipe、成就/云存档、商店素材和 smoke 回归 | Steamworks SDK / SteamPipe | P0 主线，分阶段推进 |
| **D4 Invasion** | BitCat 语义化核心玩法 | MVP 已完成；真实投影接入已验证，剩真实窗口触发回归、手感打磨、专属积分事件 | 0 | P0，1-2 天收口 |

**当前可玩 Demo 的基础设施已超过原 MVP 预期；下一阶段最短路径是 D1 Demo 闸门 → D4 Invasion MVP 收口 → A2/B7 分数、成长和权限闭环 → D2/D3 Steam 平台与合规收束。目标是先让 BitCat 成为可审查、可解释、可反复游玩的 Steam Demo，再推进 Early Access。**

### 当前打磨队列

这些项已经有可用主链路，后续不按“大功能从零实现”估算，而按体验和可靠性收尾推进：

| 领域 | 当前边界 | 打磨目标 |
|------|----------|----------|
| B6 提醒与顶部通知 | 已支持确定性提醒、顶部通知、提示音和可选 AI 润色 | 控制 AI 润色费用/频率；优化多个提醒同时到期；补失败诊断；评估 complete/snooze 是否开放给 Agent 工具 |
| E1/E2 Agent Watch | 已支持本地/远程只读 hook、浮动任务栈和顶部通知 | 补 JSONL watcher、PID 存活检测、结构化 Write/Edit/Bash 预览、panel 收敛和已查看/静音/置顶 |
| B4 工具运行时 | 生命周期事件、bubble UI 和审计日志已可用 | 用真实 token/工具日志决定 schema 预算和 dynamic tools，不做关键词意图识别 |
| B5 记忆 | grep-first 长期记忆主链路已可用 | 减少默认预塞上下文；让 `search_memory` 按需召回后再由模型压缩判断 |
| B7 积分与成就 | points JSONL、等级、成就和设置页展示已可用 | 接成长上下文、权限 gate、商店、每日任务和心情系统 |
| D4 Invasion | MVP 可运行，真实长期记忆投影已验证 | 提交真实投影接入；补窗口触发回归；调敌人节奏/目标反馈；加 Invasion 专属 points/achievement |
| Pet v2 assets | 15 个内置猫咪品种已收敛，旧非猫资源不再打包 | P2 可选：用户目录加载、资源诊断、外部包/DLC 分层 |
| 音乐响应舞动 | 第一版音乐模式可用 | 增强舞感状态机、fake source 诊断、节奏/静音/高潮回落表现 |

---

## 数据目录规划

```
~/.bitcat/
├── dances/              # AI 生成的舞蹈 (A1)
├── games/               # AI 生成的游戏 (A2)
├── screenshots/         # 已有
├── camera/              # 摄像头观察记录（默认关闭，开启后写 analysis JSON/可选帧图片）
├── memory/              # 已有：chat_summary.json + long_term.jsonl/long_term.md grep-first 记忆
├── logs/
│   ├── bitcat.YYYY-MM-DD.log
│   ├── token_usage.jsonl    # Token 追踪行日志 (B2)
│   ├── token_sessions.json  # 会话级汇总 (B2)
│   ├── tool_events.jsonl    # 工具生命周期审计 (B4)
│   ├── reminder_events.jsonl # 提醒生命周期与存储异常 (B6)
│   ├── points_events.jsonl # 积分/成就事件明细 (B7)
│   ├── agent_watch_events.jsonl # Agent Watch 原始归一事件 (E1/E2)
│   ├── agent_watch_sessions.jsonl # Claude Code / Codex 等会话状态 (E1/E2)
│   ├── agent_watch_nudges.jsonl # Agent Watch 提醒决策 (E1/E2)
│   └── agent_actions.jsonl  # 桌宠触发的 Agent 控制动作 (E3)
├── reminders/
│   └── reminders.json       # 程序化提醒 store (B6)
├── points_state.json        # 积分、等级、连续活跃和成就聚合状态 (B7)
├── agents/
│   ├── sessions.json        # 当前活跃 Agent 会话缓存 (E1/E2)
│   └── connectors.yml       # 自定义工具/目录/hook 适配配置 (E1/E4)
├── workshop/            # Steam Workshop 订阅内容 (A3)
└── config/
    ├── actions.yml
    ├── buttons.yml
    └── prompts.yml
```
