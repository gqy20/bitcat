# 8Bit Cat Roadmap

> 目标：将 8Bit Cat 从 AI 桌宠进化为 **Steam 可发布的 AI 驱动桌面伴侣**。
> 核心差异化：AI 通过工具调用 **动态生成可玩内容**（舞蹈 + 迷你游戏），而非仅对话。

---

## Phase 0: 当前基线

**已有能力：**
- 6 状态像素精灵动画（idle/walk/sleep/talk/happy/confused）
- AI 对话（Anthropic Claude via rig-core，流式输出）
- 8 个内置工具（launch/shell/read_file/get_time/hotkey/clipboard/foreground/screenshots）
- SDL2 手柄输入（8BitDo Micro）
- 多窗口模型（pet 128x128 / bubble / panel 480x320 / voice）
- 截图观察 + Vision API 分析
- 滚动窗口记忆系统
- YML 配置热加载（actions/prompts/buttons）

**技术栈：** Rust workspace (core + app) + Tauri 2.0 + Vanilla JS Canvas + WebView2

---

## Phase 1: 舞蹈系统 (Dance)

### 目标

用户对猫说 "跳个舞" → AI 调用 `create_dance` 工具 → 生成 YML 定义 → pet 窗口即刻播放动作序列。

### 设计

舞蹈 = **按时间轴切换 sprite 动作帧**，不引入骨骼动画或新渲染管线。

#### 新增 Sprite 动作（4 个）

基于现有 `IDLE_BASE` 的 `cloneSprite` 变体，每个只需改几个像素：

| 动作 | 视觉效果 | 实现方式 |
|------|---------|---------|
| `jump` | 整体上移 | 底部脚位像素清空（模拟腾空） |
| `spin` | 快速旋转 | 渲染层快速翻转 `facingRight` |
| `wave` | 前爪抬起 | 左上角像素清空 |
| `shake` | 左右晃动 | 渲染层 x 坐标偏移 |

#### DanceDef Schema（YML）

```yaml
# ~/.ai-pad/dances/happy_twist.yaml
name: "开心扭扭"
loop: true
steps:
  - action: jump    duration_ms: 300
  - action: shake   duration_ms: 400
  - action: spin    duration_ms: 500
  - action: wave    duration_ms: 300
  - action: idle    duration_ms: 200
```

#### AI 工具定义

```
create_dance(name, mood) → ToolResult
  - name:   舞蹈文件名（如 "happy_twist"）
  - mood:   情绪关键词（happy/excited/sleepy/angry/cute）
  - 输出:   生成 YAML → 写入 ~/.ai-pad/dances/{name}.yaml → 返回路径
```

#### 前端播放机制

舞蹈播放时 **劫持 `app.js` 主循环的渲染分支**：

```
正常模式: pet.update(dt) → renderSprite(pet.state, ...)
舞蹈模式: dancePlayer.update(dt) → renderSprite(currentStep.action, 0, ...)
          └─ step 时间到 → 切下一个 action
          └─ 全部播完 → dancePlayer = null → 交还控制权给状态机
```

### 文件改动清单

| 文件 | 操作 | 行数 | 说明 |
|------|------|------|------|
| `core/src/dance.rs` | **新建** | ~60 | DanceDef 结构体 + YML 序列化/反序列化 + 目录管理 (load/save/list) |
| `core/src/lib.rs` | 编辑 | +1 | `pub mod dance;` |
| `core/src/tools.rs` | 编辑 | +25 | `CreateDanceArgs` 结构体 + `execute_create_dance()` 函数 |
| `core/src/agent.rs` | 编辑 | +15 | `define_tool_sync!(CreateDanceTool, ...)` + `.tool(CreateDanceTool)` 注册 |
| `app/src/commands.rs` | 编辑 | +20 | `cmd_play_dance` — 加载 YML → emit `play-dance` 事件给前端 |
| `app/src/lib.rs` | 编辑 | +1 | invoke_handler 加 `cmd_play_dance` |
| `app/frontend/js/sprite.js` | 编辑 | +30 | 4 个新动作帧 (jump/spin/wave/shake) 加入 SPRITES 字典 |
| `app/frontend/js/app.js` | 编辑 | +40 | dancePlayer 变量 + `updateDance(dt)` + 监听 `play-dance` 事件 + loop() 分支 |

**小计：~192 行新代码，零新依赖**

### 数据目录

```
~/.ai-pad/
├── dances/                    # AI 生成的舞蹈（新增）
│   ├── happy_twist.yaml
│   ├── sleepy_sway.yaml
│   └── ...
├── games/                     # AI 生成的游戏（Phase 2 新增）
├── screenshots/               # 已有
├── memory/                    # 已有
├── logs/                      # 已有
├── config/                    # 运行时配置（actions/buttons/prompts .yml）
│   ├── actions.yml
│   ├── buttons.yml
│   └── prompts.yml
```

### 内置预设（可选）

项目内 `data/dances/` 放 1-2 个默认舞蹈，首次启动时检测 `dances/` 为空则复制过去。

---

## Phase 2: 迷你游戏引擎 (MiniGame)

### 目标

用户说 "来局贪吃蛇" → AI 调用 `create_game` 工具 → 生成游戏 YML → panel 窗口运行游戏 → 结束联动 pet 状态。

### 三种游戏原型

| 游戏 | 类型标识 | 操作 | 复杂度 | 适合场景 |
|------|---------|------|--------|---------|
| **贪吃蛇** | `snake` | 方向键转向 | 低 | 经典零教学成本 |
| **记忆翻牌** | `memory` | 方向键移动 + A 翻牌 | 低 | 休闲，可爱调性 |
| **打地鼠** | `whack` | 方向键移动光标 + A 点击 | 最低 | 测试输入链路 |

三种游戏共享同一个 `GameEngine` 类，差异只在 `init/update/render/input` 四个方法的实现。

### GameDef Schema（YML）

```yaml
# ~/.ai-pad/games/snake_neon.yaml
game:
  type: snake
  title: "抓星星"
grid:
  width: 20
  height: 14
  cell_size: 16
player:
  start_position: [10, 7]
  speed_ms: 120
  initial_length: 3
rules:
  walls_kill: true
  self_kill: true
dialogue:
  start: "用方向键控制，吃到星星~"
  win: "太厉害了！你是抓星高手！"
  lose: "咬到自己啦... 再试一次？"
```

```yaml
# ~/.ai-pad/games/memory_stars.yaml
game:
  type: memory
  title: "猫猫记忆"
grid:
  cols: 4
  rows: 3
  card_size: 60
symbols: ["★", "♦", "♥", "♠", "●", "▲"]
rules:
  flip_time_ms: 800
  mismatch_penalty_ms: 500
dialogue:
  start: "找出相同的配对！"
  win: "记忆力满分！"
```

```yaml
# ~/.ai-pad/games/whack_mole.yaml
game:
  type: whack
  title: "打地鼠"
grid:
  cols: 4
  rows: 3
target:
  show_time_min_ms: 600
  show_time_max_ms: 1200
  score_per_hit: 10
duration_seconds: 30
dialogue:
  start: "点冒出来的目标！快！"
  win: "30秒打了 {score} 分！"
}
```

### AI 工具定义

```
create_game(game_type, theme, difficulty?) → ToolResult
  - game_type:   "snake" | "memory" | "whack"
  - theme:       主题关键词（neon/pastel/dark/retro/ocean）
  - difficulty:  "easy" | "normal" | "hard"（可选）
  - 输出:        生成 YAML → 写入 ~/.ai-pad/games/{name}.yaml → 返回路径
```

### 架构

```
用户说 "来局贪吃蛇"
  → agent.chat_stream("来局贪吃蛇")
  → AI 决定调用 create_game(type="snake", theme="neon")
  → execute_create_game() 生成 GameDef → 序列化 YAML → save_game()
  → 工具返回成功 + 文件名
  → Rust 侧 cmd_start_game("snake_neon") 加载 YML
  → emit("start-game", game_def) 给 panel 窗口
  → panel.js 切换到游戏模式（隐藏 grid，显示 canvas）
  → new GameEngine(canvas, def) 开始游戏循环
  → 手柄方向键/A键 → panel-nav/panel-confirm 事件 → engine.input()
  → 游戏结束 → emit("game-end", {won, score}) → Rust 收到
  → cmd_set_state(Happy 或 Confused) 联动 pet 表情
```

### 文件改动清单

| 文件 | 操作 | 行数 | 说明 |
|------|------|------|------|
| `core/src/minigame.rs` | **新建** | ~80 | GameDef / MinigameType 枚举 + YML I/O (load/save/list) |
| `core/src/lib.rs` | 编辑 | +1 | `pub mod minigame;` |
| `core/src/tools.rs` | 编辑 | +20 | `CreateGameArgs` + `execute_create_game()` |
| `core/src/agent.rs` | 编辑 | +15 | `define_tool_sync!(CreateGameTool, ...)` + `.tool(CreateGameTool)` |
| `app/src/commands.rs` | 编辑 | +30 | `cmd_start_game` + `cmd_game_input` |
| `app/src/lib.rs` | 编辑 | +2 | invoke_handler 加 2 个命令 |
| `app/frontend/js/game_engine.js` | **新建** | ~250 | GameEngine 类：Snake/Memory/Whack 三种实现 |
| `app/frontend/panel.html` | 编辑 | +10 | `<canvas id="game-canvas">` （默认 hidden） |
| `app/frontend/js/panel.js` | 编辑 | +20 | 游戏模式切换 + 输入转发给 engine + game-end 回调 |

**小计：~428 行新代码，零新依赖**

### 输入复用

Panel 已有完整的手柄输入链路：
- `panel-nav(dx, dy)` 事件 → 方向键
- `panel-confirm` 事件 → A 键确认

游戏模式下拦截这些事件，转发给 `engine.input("up"/"down"/"left"/"right"/"a")`。无需任何新的 IPC 或手柄代码。

### Pet 联动

| 游戏结果 | Pet 状态 | 触发方式 |
|---------|---------|---------|
| 胜利 | Happy | `cmd_set_state(PetStateName::Happy)` |
| 失败 | Confused | `cmd_set_state(PetStateName::Confused)` |

---

## Phase 3: 内容生态扩展

### 3a. 更多游戏类型

在 GameEngine 中加新的 init/render/update/input 分支：

| 游戏 | 类型 | 描述 | 复杂度 |
|------|------|------|--------|
| **节奏点击** | `rhythm` | 节拍从上方落下，按键时机判定 | 中 |
| **问答 Quiz** | `quiz` | AI 出题（文字），玩家选答案 | 低（纯 DOM） |
| **躲避障碍** | `dodge` | 控制角色躲避下落物 | 中 |
| **2048** | `2048` | 经典数字合并 | 中 |

每种新类型 = GameEngine 加一个分支 (~50-80 行) + 一个 YML schema 扩展。

### 3b. 舞蹈编辑器（可选）

- Panel 窗口中加入可视化时间轴编辑器
- 拖拽排列动作、调整时长
- 导出为 YAML → 可分享
- 优先级低于 AI 生成，作为进阶功能

### 3c. Steam Workshop 集成

- 用户上传/下载舞蹈和游戏 YML
- `~/.ai-pad/workshop/` 目录存放订阅内容
- 自动同步 + 版本管理

---

## Phase 4: Steam 发布准备

### 发布清单

| 项目 | 要求 | 当前状态 |
|------|------|---------|
| Steamworks SDK | 集成 | 待做 |
| App ID | $100（可回收） | 待申请 |
| 年龄评级 | IARC（免费） | 待做 |
| Store 页面 | capsule + 截片 + trailer | 待准备 |
| AI 内容披露 | 实时生成内容声明 | 需要（舞蹈+游戏+对话） |
| DRM | Steam DRM（可选） | 默认开启即可 |
| 自动更新 | tauri-plugin-updater | Tauri 已支持 |

### AI 披露策略

Valve 要求区分：
- **预生成 AI 内容**（开发时用 AI 生成的素材）→ 需披露
- **实时 AI 内容**（运行时动态生成）→ 额外审查

本项目需要披露：
1. AI 对话回复（实时）— Anthropic Claude
2. AI 生成的舞蹈定义（实时）— 工具调用输出
3. AI 生成的游戏配置（实时）— 工具调用输出
4. 截图视觉分析描述（实时）— Vision API

不需要披露：
- 开发过程中使用 AI 辅助编码

### 定价建议

| 对比 | 价格 | 特点 |
|------|------|------|
| VPet | 免费 | 开源，社区驱动 |
| Weyrdlets 2.0 | $5-8 | 有迷你游戏但无 AI |
| AI Desktop Pet | ~$8 | Live2D + 本地 LLM + Workshop |
| **8Bit Cat** | **$5-7** | 像素风 + AI 对话 + **AI 生成内容** + 开源 |

---

## 工作量与优先级总结

| Phase | 内容 | 新代码量 | 新依赖 | 预计时间 |
|-------|------|---------|--------|---------|
| **Phase 1** | 舞蹈系统 | ~192 行 | 0 | 1-2 天 |
| **Phase 2** | 迷你游戏引擎 | ~428 行 | 0 | 2-3 天 |
| **Phase 3** | 内容扩展 | ~300 行/种 | 0 | 按需迭代 |
| **Phase 4** | Steam 发布 | 集成工作 | Steamworks SDK | 1-2 周 |

**MVP（Phase 1 + 2）：总计 ~620 行新代码，零依赖，一周内可出可玩 Demo。**

---

## 不需要改的文件

以下文件在 Phase 1-2 中完全不动：

- `core/src/pet.rs` — 状态机不变，舞蹈是前端渲染层劫持
- `core/src/bridge.rs` — 按键映射不变
- `core/src/prompts.rs` / `config/prompts.yml` — 可能微调 prompt（加舞蹈/游戏能力说明）
- `app/src/bubble.rs` — 对话气泡不变
- `app/src/screenshot.rs` — 截图系统不变
- `app/frontend/js/pet.js` — 前端状态机不变
- `app/frontend/js/particles.js` — 粒子系统不变
- `app/src/gamepad.rs` — 手柄循环不变（复用现有 panel 导航事件）
