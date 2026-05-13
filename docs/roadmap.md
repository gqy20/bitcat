# 8Bit Cat Roadmap

> **目标**：将 8Bit Cat 从 AI 桌宠进化为 **Steam 可发布的 AI 驱动桌面伴侣**。
> **核心差异化**：AI 通过结构化输出动态生成可玩内容（舞蹈 + 迷你游戏），而非仅对话。

---

## 当前基线

| 能力 | 状态 |
|------|------|
| 6 状态像素精灵动画（idle/walk/sleep/talk/happy/confused） | 已有 |
| AI 对话（Anthropic Claude via rig-core，流式输出） | 已有 |
| 10 个内置工具（launch/shell/read_file/get_time/hotkey/clipboard/foreground/screenshots/perform_dance/play_dance） | 已有 |
| SDL2 手柄输入（8BitDo Micro） | 已有 |
| 多窗口模型（pet / bubble / panel / voice） | 已有 |
| 截图观察 + Vision API 分析 | 已有 |
| 滚动窗口记忆系统 | 已有 |
| YML 配置热加载 | 已有 |

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
│  │ A1 舞蹈  │  │ B1 日志   │  │ C1 3D体素 │  │ D1 Steam │   │
│  │ A2 游戏  │  │ B2 Token │  │ C2 动画   │  │ D2 定价   │   │
│  │ A3 扩展  │  │ B3 结构化 │  │ C3 游戏3D│  │          │   │
│  │          │  │ B4 工具裁剪│  │          │  │          │   │
│  │          │  │ B5 RAG记忆│  │          │  │          │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
│                                                             │
│  优先级：A1 → B1/B2 → A2 → B3/B4 → C1 → A3 → B5 → C2/C3 → D1│
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

当前 222 处日志存在三大问题：大文本污染（AI 回复全文写 info!）、级别语义模糊（warn 混了多种场景）、eprintln 绕过 tracing。建立五级规范（ERROR/WARN/INFO/DEBUG/TRACE），清理历史债务，用 `.claude/rules/logging.md` 约束后续代码。

详细设计：[plan/logging-standardization.md](plan/logging-standardization.md)

### B2. Token 全链路追踪

当前 4 条 API 调用路径的 token 消耗几乎未记录。目标是每次调用的 input/output/cache token 明细全部落盘到 `~/.ai-pad/token_usage.jsonl`，支持会话级汇总和按日查询。零侵入式改造——不改变函数签名行为，只追加 side-effect。

详细设计：[plan/token-tracking.md](plan/token-tracking.md)

### B3. 结构化输出（Extractor 改造）

当前 vision 和 screen_summary 绕过 rig 直接用 reqwest 调 API，返回纯文本字符串，下游需二次解析。使用 rig 的 `Extractor<M, T>` 将输出约束为强类型结构体（如 `VisionAnalysis`），消除格式不稳定问题，同时回归 rig 生态对接 token 追踪。

详细设计：[plan/rig-capability-roadmap.md](plan/rig-capability-roadmap.md) §P0

### B4. 场景化工具裁剪（dynamic_tools）

当前 10 个工具全量注册到每次对话（占 ~81% prompt token）。根据用户消息意图动态选择工具子集——闲聊只需 4 个工具，预计平均节省 ~250 tokens/对话。

详细设计：[plan/rig-capability-roadmap.md](plan/rig-capability-roadmap.md) §P1

### B5. 语义记忆检索（RAG）

当前滚动窗口策略（最近 20 条）无语义关联，重要讨论会被闲聊挤出。引入 Embeddings 向量检索，按语义相关性召回历史对话，支持跨会话记忆。这是最大改动量的基础设施升级。

详细设计：[plan/rig-capability-roadmap.md](plan/rig-capability-roadmap.md) §P2

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

- 呼吸微动、眨眼、走路改进
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
Week 1-2     ┌─────────────────────────────────────┐
             │  B1 日志规范化                       │  ← 无依赖，立即收益
             │  A1 舞蹈系统 (MVP 核心功能)         │
             └─────────────────────────────────────┘
                    ↓
Week 3       ┌─────────────────────────────────────┐
             │  B2 Token 追踪                       │  ← B1 清理后更清晰
             │  A2 迷你游戏引擎                     │  ← 复用 A1 模式
             └─────────────────────────────────────┘
                    ↓
Week 4-5     ┌─────────────────────────────────────┐
             │  B3 Extractor 结构化输出              │  ← 为 B5 打基础
             │  B4 dynamic_tools 工具裁剪            │  ← 节省 token
             │  A3 内容扩展（更多游戏类型）           │
             └─────────────────────────────────────┘
                    ↓
Week 6-8     ┌─────────────────────────────────────┐
             │  C1 桌宠 3D 体素化（最大块）           │  ← 渲染层重写
             └─────────────────────────────────────┘
                    ↓
Week 9-10    ┌─────────────────────────────────────┐
             │  B5 语义记忆 RAG                      │  ← 最大改动量
             │  C2 动画增强                         │
             └─────────────────────────────────────┘
                    ↓
Week 11+     ┌─────────────────────────────────────┐
             │  C3 3D 游戏生成能力                   │
             │  D1 Steam 发布准备                   │
             └─────────────────────────────────────┘
```

### 关键依赖

```
B1(日志) ──→ B2(Token追踪) 更清晰的代码基础
A1(舞蹈) ──→ A2(游戏) 复用同一模式
B3(Extractor) ──→ B5(RAG) 结构化数据质量提升向量准确度
B4(工具裁剪) ──→ 抵消 B5 embedding API 额外开销
C1(3D化) ──→ C2/C3 渲染层就绪
A1+A2+C1 ──→ D1(Steam) MVP 功能完备
```

---

## 工作量估算

| Track | 内容 | 新代码量（估） | 新依赖 | 优先级 |
|-------|------|---------------|--------|--------|
| **B1** | 日志规范化 | ~50 行改动 | 0 | P0 — 立即做 |
| **A1** | 舞蹈系统 | ~215 行 | 0 | P0 — MVP 核心 |
| **B2** | Token 追踪 | ~200 行 | 0 | P1 |
| **A2** | 迷你游戏引擎 | ~500 行 | 0 | P1 |
| **B3** | Extractor 改造 | ~150 行 | 0 | P2 |
| **B4** | 工具裁剪 | ~100 行 | 0 | P2 |
| **A3** | 内容扩展 | ~300 行/种 | 0 | P2 |
| **C1** | 3D 体素化 | ~1500 行 | three.js | P2 — 最大块 |
| **B5** | RAG 记忆 | ~600 行 | embedding provider | P3 |
| **C2** | 动画增强 | ~400 行 | 0 | P3 |
| **C3** | 3D 游戏生成 | ~800 行 | cannon-es 等 | P3 |
| **D1** | Steam 发布 | 集成工作 | Steamworks SDK | P4 |

**MVP（B1 + A1 + B2 + A2）：约 1165 行，4 周内可出可玩 Demo。**

---

## 数据目录规划

```
~/.ai-pad/
├── dances/              # AI 生成的舞蹈 (A1)
├── games/               # AI 生成的游戏 (A2)
├── screenshots/         # 已有
├── memory/              # 已有 → 升级为语义索引 (B5)
├── logs/                # 已有
├── token_usage.jsonl    # Token 追踪行日志 (B2)
├── token_sessions.json  # 会话级汇总 (B2)
├── workshop/            # Steam Workshop 订阅内容 (A3)
└── config/
    ├── actions.yml
    ├── buttons.yml
    └── prompts.yml
```
