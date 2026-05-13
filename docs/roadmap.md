# 8Bit Cat Roadmap

> **目标**：将 8Bit Cat 从 AI 桌宠进化为 **Steam 可发布的 AI 驱动桌面伴侣**。
> **核心差异化**：AI 通过结构化输出动态生成可玩内容（舞蹈 + 迷你游戏），而非仅对话。

---

## 当前基线

| 能力 | 状态 |
|------|------|
| 6 状态像素精灵动画（非均匀帧时长 + 瞬态 repeat+fallback） | 已有（2026-05-13 增强） |
| AI 对话（Anthropic Claude via rig-core，流式输出） | 已有 |
| 10 个内置工具（launch/shell/read_file/get_time/hotkey/clipboard/foreground/screenshots/perform_dance/play_dance） | 已有 |
| SDL2 手柄输入（8BitDo Micro） | 已有 |
| 多窗口模型（pet / bubble / panel / voice） | 已有 |
| 截图观察 + Vision API 分析 | 已有 |
| 滚动窗口记忆系统 | 已有 |
| YML 配置热加载 | 已有 |
| 舞蹈系统（内置/用户目录 YAML + AI tool 触发播放） | 已落地 |
| 日志规范化（大文本截断、级别收敛、tracing 归一） | 已落地第一轮 |
| Token 追踪（JSONL 明细、会话汇总、按日查询） | 已落地 |
| 设置页 Token 统计（今日消耗、最近会话、链路占比） | 已落地 |
| Makefile 测试入口（通过 xtask 避免 PowerShell 语法问题） | 已落地 |

**技术栈**：Rust workspace (core + app) + Tauri 2.0 + Vanilla JS Canvas + WebView2

---

## 发展方向总览

```
┌─────────────────────────────────────────────────────────────┐
│                      8Bit Cat 产品路线                        │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │ Track A  │  │ Track B  │  │ Track C  │  │ Track D  │   │
│  │ AI 内容  │  │ 基础设施  │  │ 渲染升级  │  │ 商业化   │   │
│  │  生成    │  │  强化    │  │          │  │          │   │
│  ├──────────┤  ├──────────┤  ├──────────┤  ├──────────┤   │
│  │ A1 舞蹈✓ │  │ B1 日志✓  │  │ C1 3D体素 │  │ D1 Steam │   │
│  │ A2 游戏  │  │ B2 Token✓│  │ C2 动画   │  │ D2 定价   │   │
│  │ A3 扩展  │  │ B3 结构化 │  │ C3 游戏3D│  │          │   │
│  │          │  │ B4 工具运行时│ │          │  │          │   │
│  │          │  │ B5 文本记忆│  │          │  │          │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
│                                                             │
│  当前优先级：B3 → A2 → B4(运行时+开销) → B5(grep-first) → C1 → A3 → D1│
└─────────────────────────────────────────────────────────────┘
```

---

## Track A: AI 内容生成

### A1. 舞蹈系统

用户说 "跳个舞" → AI 在普通对话中自行决定调用 `perform_dance` → 提交完整 `DanceDef` → pet 窗口即刻播放。

核心变化：从旧的 `create_dance(name, mood)` + `choreograph()` 查表模板，升级为 LLM 通过 Tool Call 直接输出完整动作序列（步骤数、节奏、组合全部由 AI 决定）。Rust 只负责校验、保存和播放。

- 新增 4 个 sprite 动作帧：jump / spin / wave / shake
- 前端 dancePlayer 劫持渲染循环，播完交还控制权
- 用户目录 `~/.ai-pad/dances/` 优先，内置预设 `config/dances/` 兜底

详细设计：[plan/structured-output-design.md](plan/structured-output-design.md)

### A2. 迷你游戏引擎

复用 A1 的模式：模型通过工具提交结构化 `GameDef` → panel 窗口运行游戏 → 结束联动 pet 状态。

三种原型游戏共享同一个 GameEngine 类：

| 游戏 | 操作 | 复杂度 |
|------|------|--------|
| 贪吃蛇 | 方向键转向 | 低 |
| 记忆翻牌 | 方向键移动 + A 翻牌 | 低 |
| 打地鼠 | 方向键移动光标 + A 点击 | 最低 |

输入复用 panel 现有手柄链路，无需新 IPC。胜利→Happy，失败→Confused。

详细设计：[plan/structured-output-design.md](plan/structured-output-design.md) §3.2

### A3. 内容生态扩展

- 更多游戏类型（节奏点击 / Quiz / 躲避障碍 / 2048）
- 舞蹈编辑器（可视化时间轴，可选）
- Steam Workshop 集成（分享/订阅 YAML）
- 工具选择策略升级（固定全量工具 → 模型/上下文驱动的工具选择）

---

## Track B: 基础设施强化

### B1. 日志体系规范化

已完成第一轮规范化：大文本不再裸写 INFO，前端日志桥和 chat/vision/memory 等高价值链路已收敛到 tracing 字段；`AGENTS.md` / `CLAUDE.md` 已补充日志规范。下一步只保留少量持续治理：新增功能必须继续使用 `log_preview()` 和稳定字段，避免把结构化数据塞回普通日志。

详细设计：[plan/logging-standardization.md](plan/logging-standardization.md)

### B2. Token 全链路追踪

已完成 MVP：chat / vision / screen_summary / memory_aggregation 的 input/output/cache token 明细写入 `~/.ai-pad/logs/token_usage.jsonl`，最近会话汇总写入 `~/.ai-pad/logs/token_sessions.json`，并通过设置页 `cmd_get_token_stats` 展示今日消耗、最近会话和各链路占比。下一步是把统计用于决策：先观察真实用量，再决定是否优化工具 schema、上下文注入或模型路由。

详细设计：[plan/token-tracking.md](plan/token-tracking.md)

### B3. 结构化输出（Extractor 改造）

已完成主链路与 cleanup：vision / screen_summary / memory aggregation 都已接入 rig `Extractor`，分别输出 `VisionAnalysis`、`StructuredSummary`、`ProfileAggregation`，token 用量也改为读取 `ExtractionResponse.usage`。旧 raw request / text parser / Anthropic usage parser 已删除，不再生效的 `screen_summary.max_summary_chars` 配置也已移除；保留基于 Anthropic `tool_use` 协议的 wiremock 回归测试。

详细设计：[plan/rig-capability-roadmap.md](plan/rig-capability-roadmap.md) §P0

### B4. 工具运行时与开销优化（谨慎，不做关键词意图识别）

当前 10 个工具仍全量注册到每次对话，但 B4 第一阶段已经完成了更基础的运行时治理：工具调用不再混入 bubble 正文，而是通过结构化事件单独呈现；普通工具显示低干扰状态条，舞蹈这类表演型工具显示“正在编舞 / 准备开跳”并短暂退场；工具结果会写入 `~/.ai-pad/logs/tool_events.jsonl`，用于后续统计成功率、耗时和拦截次数。

新的原则保持不变：**默认相信大模型自己选择工具，Rust 负责 schema、权限、生命周期事件、审计和体验呈现**。B4 现在已经足够支撑下一阶段游戏工具接入：未来 `start_game` / `play_game` 这类表演型或互动型工具应复用 `ToolKind::Performance` 或扩展出更精细的 kind，让 bubble 退到辅助位置，把主视觉交给 pet/panel/game 窗口。

建议拆分：

1. **B4.1 已完成：工具生命周期事件协议**：`planned / blocked / finished / failed` 已接入，携带 `tool_name`、`internal_call_id`、结果预览、耗时和结果状态。`allowed` 不做伪事件，除非以后把 `PermissionHook` 改为带事件 sink 的状态化 hook。
2. **B4.2 已完成：bubble 表演型工具状态 UI**：普通工具显示安静状态条；`perform_dance` / `play_dance` 走“正在编舞 / 准备开跳 / bubble 退场”的舞台体验。
3. **B4.3 已完成：工具事件审计日志**：`tool_events.jsonl` 记录成功率、失败/拦截、耗时和短结果预览，不记录大文本。
4. **B4.4 已做首轮：schema/description 压缩**：已低风险压缩 `perform_dance` / `play_dance` 文案。真实 token 预算工具暂缓，等工具继续增长或真实日志显示固定 schema 成为瓶颈再做。
5. **B4.5 暂缓：显式能力包 / dynamic_tools 实验**：当前不阻塞游戏部分。仅在真实数据证明必要时启用，必须 feature flag 可回滚。

下一步主线建议：先做游戏部分。游戏工具应直接复用 B4 已完成的事件协议、UI 分层和审计日志；不要再引入关键词分类或额外小模型判断。

详细设计：[plan/rig-capability-roadmap.md](plan/rig-capability-roadmap.md) §P1

### B5. grep-first 文本记忆检索

当前滚动窗口策略（最近 20 条）无长期召回能力，重要讨论会被闲聊挤出。但本项目不采用 Embeddings / Vector RAG：记忆规模和使用方式更适合可 grep 的结构化文本。长期方向是 append-only JSONL / Markdown 记忆，配合 `rg` 风格检索、时间范围、来源和标签筛选，再让大模型对候选片段做判断和压缩。

详细取舍：[architecture/design-tradeoffs.md](architecture/design-tradeoffs.md)

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
- 粒子系统迁移到 Three.js Points
- 舞蹈系统 3D 化（真实抛物线轨迹、翻滚感）
- 鼠标交互：hover 时猫转头看鼠标

### C3. 3D 游戏生成能力

新增独立 game 窗口（PerspectiveCamera + OrbitControls），AI 生成可交互的 3D 游戏内容（地形、角色、规则）。集成物理引擎（cannon-es）、碰撞检测、音效系统。

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
| **8Bit Cat** | **$5-7** | 像素风 + AI 对话 + **AI 生成内容** + 开源 |

---

## 实施优先级与依赖关系

```
已完成      ┌─────────────────────────────────────┐
2026-05-13 │  A1 舞蹈系统                         │
           │  B1 日志规范化第一轮                  │
           │  B2 Token 追踪 + 设置页统计            │
           │  测试入口/Makefile/xtask 稳定化        │
           └─────────────────────────────────────┘
                  ↓
短期        ┌─────────────────────────────────────┐
1-3天      │  A2 迷你游戏引擎 MVP                  │  ← 复用 A1 的 YAML/Tool 模式
           │  B4 工具运行时与开销优化              │  ← 先规范生命周期，再基于真实统计优化
           └─────────────────────────────────────┘
                  ↓
中期        ┌─────────────────────────────────────┐
1-3天      │  B5 grep-first 文本记忆检索            │
           │  A3 内容扩展（更多游戏类型）           │
           └─────────────────────────────────────┘
                  ↓
大块        ┌─────────────────────────────────────┐
4-8天      │  C1 桌宠 3D 体素化                    │
           │  C2/C3 3D 动画与游戏能力               │
           └─────────────────────────────────────┘
```

### 关键依赖

```
B1(日志) ──→ B2(Token追踪) 已完成，提供干净观测面
B2(Token) ─→ B4(工具运行时) 用真实数据决定是否值得优化
A1(舞蹈) ──→ A2(游戏) 复用同一模式
B3(Extractor) ──→ B5(文本记忆) 结构化摘要更容易 grep 和压缩
A1/A2(内容型工具) ─→ B4(工具运行时) 提供舞蹈/游戏两类真实样本，验证工具事件协议
B4(工具运行时) ──→ 控制固定 prompt 成本，给记忆候选留预算，也为工具事件记忆化打基础
C1(3D化) ──→ C2/C3 渲染层就绪
A1+A2+C1 ──→ D1(Steam) MVP 功能完备
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
| **A2** | 迷你游戏引擎 | ~450-700 行 | 0 | P1，1-3 天 |
| **B4** | 工具运行时与开销优化 | ~250-450 行（B4.1-B4.3）+ ~50-150 行（B4.4） | 0 | P1，1-2 天；B4.5 实验项 |
| **A3** | 内容扩展 | ~200-350 行/种 | 0 | P2，0.5-1 天/种 |
| **B5** | grep-first 文本记忆 | ~250-450 行 | 0 | P2，1-3 天 |
| **C1** | 3D 体素化 | ~1200-1800 行 | three.js | P3，4-8 天 |
| **C2** | 动画增强 | ~300-500 行 | 0 | P3，1-3 天 |
| **C3** | 3D 游戏生成 | ~700-1000 行 | cannon-es 等 | P3，3-6 天 |
| **D1** | Steam 发布 | 集成工作 | Steamworks SDK | P4，2-5 天 |

**当前可玩 Demo 的基础设施已超过原 MVP 预期；下一阶段最短路径是 B3 → A2，让“观察/摘要”和“AI 生成可玩内容”都进入结构化闭环。**

---

## 数据目录规划

```
~/.ai-pad/
├── dances/              # AI 生成的舞蹈 (A1)
├── games/               # AI 生成的游戏 (A2)
├── screenshots/         # 已有
├── memory/              # 已有 → 升级为可 grep 的文本记忆 (B5)
├── logs/
│   ├── ai-pad.YYYY-MM-DD.log
│   ├── token_usage.jsonl    # Token 追踪行日志 (B2)
│   └── token_sessions.json  # 会话级汇总 (B2)
├── workshop/            # Steam Workshop 订阅内容 (A3)
└── config/
    ├── actions.yml
    ├── buttons.yml
    └── prompts.yml
```
