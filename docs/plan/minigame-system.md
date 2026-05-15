# 迷你游戏系统实现设计

> 状态：Phase 1 已完成（2026-05-13，提交 `a2105ff`）
> 关联文档：
> - 产品定位：[gdd/core-gameplay.md](../gdd/core-gameplay.md) §八、Mini-Game 定位
> - GameDef schema 历史草案：[plan/archive/structured-output-design.md](archive/structured-output-design.md) §3.2
> - 路线图：[roadmap.md](../roadmap.md) §A2

## 一、核心设计决策

### 1.1 游戏渲染位置：全屏透明窗口

**结论**：游戏在覆盖整个屏幕的透明 Tauri 窗口中运行，背景是用户的真实桌面。

三种备选方案对比：

| 方案 | 空间 | 沉浸感 | 和宠物联动 | 实现复杂度 |
|------|------|--------|-----------|-----------|
| 面板内 canvas (480×320) | 小 | 低 | 中 | 低 |
| 独立游戏窗口 (640×480) | 中 | 中 | 弱 | 中 |
| **全屏透明窗口** | **全屏** | **高** | **强** | **中** |

全屏窗口技术要点：

```
Tauri 窗口属性：
  - transparent: true
  - decorations: false
  - always_on_top: true
  - skip_taskbar: true
  - fullscreen / sized to screen resolution
  - canvas 只画游戏元素，其余区域全透明
```

效果：蛇（毛线球）在用户真实的桌面上移动，食物（老鼠/鱼）随机出现在桌面任意位置。桌宠与桌面融为一体。

### 1.2 输入方式

游戏窗口直接监听键盘，不需要面板作为中介：

| 操作 | 键盘 | 手柄 |
|------|------|------|
| 方向 | 方向键 / WASD | D-pad |
| 确认/动作 | Enter / Space | A |
| 暂停 | P | Start |
| 退出 | Escape | B |

#### Battle 输入隔离补充（2026-05-15）

`Battle` 属于桌宠旁边的轻量战斗，不应复用普通桌宠按钮语义。进入 `game_active` 后，手柄输入必须先进入 `GameInput`，并且跳过普通宠物动作、`actions.yml` 动作、语音按住态和全局键盘别名。

当前语义映射：

| 操作 | 键盘 | 手柄 | 说明 |
|------|------|------|------|
| 普通攻击 | Space / J / 点击怪物 | A | 可打断怪物攻击预警 |
| 技能 1 | 1 / K | X | 默认重击 |
| 技能 2 | 2 / L | Y | 默认治疗/道具技能 |
| 防御 | Shift | L1 | 降低下一次伤害 |
| 暂停 | P | Start | 游戏内部暂停 |
| 退出 | Escape | B | 结束/退出游戏 |

注意：`actions.yml` 中 Y 默认是 `voice`，因此仅在“按下瞬间”跳过 ActionBus 不够。`gamepad_loop` 后段的 `voice` 按住检测也必须在 `game_active` 时禁用，否则 Y 会同时触发 Battle 技能和语音输入。

全局键盘快捷键同样需要隔离：`CommandOrControl+Alt+Space` 面板热键和 `actions.yml.keyboard_shortcut` 在 `game_busy` 期间只记录 debug 并跳过分发。

### 1.2.1 游戏窗口模式

Snake 这类专注型小游戏使用 focus mode：隐藏 `pet / pet-mini / pet-snap / bubble / panel / settings`，让游戏独占屏幕交互。

Battle 使用 overlay mode：只隐藏 `bubble / panel / settings`，保留真实桌宠窗口。游戏 canvas 只负责怪物、传送门、血条、技能按钮和伤害数字，不再绘制临时假宠物。桌宠本体通过 `PetEvent::SetMode(GamePlay)`、胜负状态和后续 performance 事件参与战斗表现。

下一步风险需要实际运行验证：

- 全屏透明 game 窗口 `always_on_top` 时，是否会遮挡真实桌宠的点击/拖拽。
- Battle overlay 下鼠标点击怪物和拖拽桌宠是否会相互抢输入。
- 退出 Battle 后隐藏/恢复窗口顺序是否稳定。

### 1.3 架构模式：复用 Dance 系统

舞蹈系统是已验证的生产级模板，游戏系统照搬同一架构：

```
Dance 系统：DanceDef YAML → 通道 → 前端 dancePlayer 劫持渲染循环
Game 系统：GameDef YAML → 通道 → 前端 GameEngine 劫持全屏窗口
```

### 1.4 宠物状态联动

在 `core/src/pet.rs` 现有 6 状态基础上新增 3 个：

| 新状态 | 帧数 | 触发时机 | 自动恢复 |
|--------|------|---------|---------|
| `GamePlay` | 2 | 游戏进行中 | 无（游戏控制） |
| `GameWin` | 3 | 游戏胜利 | 3s → Idle |
| `GameLose` | 2 | 游戏失败 | 3s → Idle |

游戏期间宠物窗口同步显示对应表情，胜利可触发庆祝舞蹈。

---

## 二、AI 生成层级

三个递进层级，现在实现 Level 1，预留 Level 2 扩展点。

### Level 1：AI 配置现有游戏模板（当前目标）

引擎固定（Snake / Memory / Catch），AI 通过 structured output 生成参数配置：

```yaml
game_type: snake
title: "毛线球大作战"
grid: { width: 30, height: 20, cell_size: 24 }
player:
  speed_ms: 120
  initial_length: 3
rules:
  walls_kill: true
  self_kill: true
  food_count: 1
  speed_ramp: 0.95       # 每吃一个加速 5%
  win_length: 20
theme:
  head: cat              # cat / yarn / light
  body: yarn             # yarn / dot / trail
  food: mouse            # mouse / fish / butterfly
  trail_alpha: 0.6
dialogue:
  start: "喵！看我的！"
  win: "太厉害了喵~"
  lose: "呜...再来一次！"
```

可调参数约 20+ 个，每次 AI 生成的游戏都有不同体验。宠物性格可影响生成（活泼的猫 → 高速，懒的猫 → 慢速大食物）。

### Level 2：AI 定义游戏规则（未来扩展）

在 GameDef 中加入实体 + 条件表达系统：

```yaml
engine: grid              # grid / physics / timeline
entities:
  - { id: player, sprite: cat, controlled: true }
  - { id: target, sprite: mouse, ai: wander }
  - { id: obstacle, sprite: box, static: true }
win_condition: "catch target 5 times"
lose_condition: "hit obstacle"
on_tick: "obstacle spawn every 3s at random"
```

AI 能创造模板之外的新玩法。需要设计安全的规则 DSL。

### Level 3：AI 生成游戏代码（不推荐）

AI 直接写 JavaScript，沙箱执行。灵活但安全风险大，不适合桌面应用。

### Level 1 → Level 2 扩展预留

| 方面 | Level 1 做法 | 扩展预留 |
|------|-------------|---------|
| `game_type` | 枚举 Snake/Memory/Catch | 改为 String + `engine` 字段 |
| 前端引擎 | 三个固定实现 | GameEngine 基类接口，新引擎注册实现 |
| 规则系统 | 硬编码在引擎内 | `rules` 用 `HashMap<String, Value>` |
| 验证 | 按 game_type 独立校验 | 统一 `validate()` 接口 + 注册机制 |

---

## 三、游戏引擎设计

### 3.1 GameEngine 基类

前端 `game_engine.js` 中的基类接口：

```
GameEngine
  ├── init(config: GameDef)        // 初始化游戏状态
  ├── update(dt_ms: number)        // 每帧逻辑更新
  ├── render(ctx: CanvasRenderingContext2D)  // 绘制
  ├── handleInput(input: Input)    // 处理输入
  ├── getState(): GameState        // 获取当前状态
  └── destroy()                    // 清理

GameState: 'ready' | 'playing' | 'paused' | 'win' | 'lose'

Input: { type: 'direction' | 'confirm' | 'cancel' | 'pause', dx?, dy? }
```

### 3.2 三个引擎实现

#### Snake（贪吃蛇 / 追毛线球）

- 方向键改变蛇的移动方向（非即时定位）
- 碰撞检测：墙壁（可配置穿墙）+ 自身
- 食物随机生成，吃到后蛇变长 + 加分
- speed_ramp 每吃一个逐步加速
- 达到 win_length 或撞墙结束

#### Memory（记忆翻牌）

- 4×4 或 6×6 卡牌网格
- 方向键移动光标，A 翻牌
- 翻两张，匹配则保留，不匹配则翻回
- flip_time_ms 控制展示时间
- 全部匹配胜利，超时则失败（可选）

#### Catch（打地鼠 / 抓老鼠）

- 目标在随机位置出现，限时消失
- 方向键移动光标到目标位置，A 确认击打
- show_time_range 控制出现时长（可随分数缩短）
- duration_seconds 总时长到结束
- 按分数判定胜负

### 3.3 主题渲染器

根据 GameDef 的 `theme` 字段决定视觉风格，三个引擎共享：

| theme 属性 | 可选值 | 效果 |
|------------|--------|------|
| head | cat / yarn / light | 蛇头/玩家外观 |
| body | yarn / dot / trail | 蛇身/轨迹外观 |
| food | mouse / fish / butterfly | 食物/目标外观 |
| trail_alpha | 0.0 - 1.0 | 轨迹透明度 |

所有主题元素用 Canvas 2D 绘制，像素风格保持和宠物一致。

---

## 四、数据流

```
用户点击面板"游戏"格子
  → panel.js emit("panel-confirm") → cmd_start_game()
  → app/src/game.rs: 创建全屏透明窗口
  → game.html 加载 → game_engine.js 初始化
  → 键盘/手柄输入 → handleInput()
  → 游戏循环: update(dt) → render(ctx)
  → 游戏结束:
      → emit("game-end", { result, score })
      → game.rs: 关闭全屏窗口
      → pet.set_state(GameWin/GameLose)
      → 记分板持久化 ~/.ai-pad/scores/
      → 胜利可触发庆祝舞蹈
```

AI 生成游戏配置的路径（Level 1）：

```
用户聊天"我想玩个游戏"
  → 普通对话 Agent 自行决定调用 play_game 工具
  → 提交 GameDef 结构化参数（JsonSchema 约束）
  → Rust 验证 bounds → 保存到 ~/.ai-pad/games/
  → 同上流程启动游戏
```

---

## 五、文件清单与代码量

### 新增文件（7 个，~1050 行）

| 文件 | 内容 | 行数 |
|------|------|------|
| `core/src/minigame.rs` | GameDef 类型 + JsonSchema + 验证 + `generate_game()` | ~180 |
| `app/src/game.rs` | 全屏窗口管理 + 游戏生命周期 + 通道 + 记分持久化 | ~220 |
| `app/frontend/game.html` | 全屏 canvas + 分数 HUD + 开始/结束画面 | ~50 |
| `app/frontend/js/game_engine.js` | 基类 + Snake + Memory + Catch + 主题渲染 | ~500 |
| `app/frontend/css/game.css` | canvas / HUD / 结束动画 / 开始画面样式 | ~60 |
| `config/minigames.yml` | 三种游戏默认配置 + 难度预设 | ~40 |

### 修改文件（9 个，~191 行）

| 文件 | 改动 | 行数 |
|------|------|------|
| `core/src/lib.rs` | `pub mod minigame` | 1 |
| `core/src/pet.rs` | GamePlay / GameWin / GameLose 三状态 | 30 |
| `core/src/prompts.rs` | 游戏生成提示词段 | 15 |
| `core/src/agent.rs` | `generate_game` 方法 + `play_game` 工具 | 30 |
| `app/src/lib.rs` | 注册游戏窗口 + game 模块 | 20 |
| `app/src/commands.rs` | 4 个 IPC 命令 | 50 |
| `app/src/gamepad.rs` | 游戏激活时输入转发 | 25 |
| `app/src/action_bus.rs` | `PlayGame` action 类型 | 10 |
| `app/frontend/js/panel.js` | 面板加"游戏"格子 | 10 |

### 行数分布

```
game_engine.js  ████████████████████  500  (40%)
game.rs         █████████            220  (18%)
minigame.rs     ███████              180  (15%)
其余 12 个文件  ███████              340  (27%)
─────────────────────────────────────────────
总计                                1240
```

---

## 六、当前代码现状校准

本节是基于 2026-05 的当前仓库状态补充的实施路线。原设计的方向仍然成立，但落地入口需要贴合现有架构：

- 当前项目已经有 `app/src/action_bus.rs`，并且舞蹈入口已通过 `ActionBus::PlayDance` 归一分发。Phase 1 已新增 `Action::PlayGameDefault`，面板入口会启动默认 Snake；`app/src/game.rs` 也已经提供 `start_game(GameDef)` / `cmd_start_game_with_def`，未来 AI 工具应直接复用这条 GameDef 启动通道。
- 舞蹈系统的成熟模式是 `core::dance` 持久化与请求通道 → app bridge 消费 → 前端播放。游戏可复用这个思路；当前窗口生命周期、输入独占和默认 Snake 已稳定，下一步是把 AI 工具注册到 agent/tools，并补齐 GameDef 持久化和更多引擎。
- `panel.js` 已扩为 3×3，保留原 6 个入口并新增"游戏 / 设置 / 聊天"。后续新增入口时要同步 `COLS/ROWS`、面板尺寸和手柄导航边界。
- `gamepad_loop` 已实现游戏激活优先级：game active > panel visible > voice held > 普通动作/滚轮。游戏激活时 D-pad/A/B/Start 转发为 `game-input`，普通滚轮和宠物动作暂停。
- Tauri capabilities 已加入 `"game"`，动态 `game` 窗口可以使用 Tauri event / invoke API。

### 推荐最小闭环

第一版已实现：面板点"游戏"后打开透明全屏 Snake，键盘和手柄都能玩，退出后宠物回到正确状态。

数据流调整为：

```
用户点击面板"游戏"格子
  → panel.js invoke("cmd_start_game") 或 ActionBus::PlayGameDefault
  → app/src/game.rs 创建/聚焦 game 窗口，并写入 SharedGame.current_def
  → game.html 加载后 invoke("cmd_get_current_game")
  → game_engine.js 初始化 Snake
  → 键盘直接 handleInput；手柄由 gamepad_loop emit("game-input")
  → 游戏结束 invoke("cmd_game_end", { result, score })
  → game.rs 关闭 game 窗口、清理 active、切换宠物 GameWin/GameLose
```

AI 路径推迟到 Phase 2：

```
用户聊天"我想玩个游戏"
  → 普通对话 Agent 自行决定调用 play_game / perform_game 工具
  → 工具提交 GameDef 参数
  → core::minigame validate + save_game
  → app bridge / ActionBus 触发启动游戏
```

---

## 七、实现阶段

### Phase 1A：窗口与入口骨架（已完成）

目标：不用 AI、不完整游戏规则，先证明窗口生命周期和前后端通信可靠。已在提交 `a2105ff` 中完成。

- 新增 `core/src/minigame.rs`：定义 `GameDef`、`MinigameType::Snake`、`GameGrid`、`PlayerConfig`、`GameRules`、`GameTheme`、`GameDialogue`，实现 `default_snake()` 与 `validate_game_def()`。
- 修改 `core/src/lib.rs`：导出 `pub mod minigame;`。
- 新增 `app/src/game.rs`：定义 `SharedGame { active, current_def }`，实现动态 `game` 窗口创建、关闭、当前游戏读取和结束回调。
- 修改 `app/src/lib.rs`：注册 `game` 模块、`SharedGame`、`cmd_start_game`、`cmd_get_current_game`、`cmd_game_end`、`cmd_game_log`。
- 修改 `app/capabilities/default.json`：窗口列表加入 `"game"`。
- 新增 `app/frontend/game.html`、`app/frontend/css/game.css`、`app/frontend/js/game_engine.js`：页面能加载配置、绘制透明 canvas、显示简单 HUD。
- 修改 `app/frontend/js/panel.js` / `panel.css` / `panel.rs`：面板扩为 3×3，新增"游戏"格子，触发 `cmd_start_game`。

验收结果：

- 面板点击"游戏"能打开透明置顶游戏窗口。
- `game.html` 能成功 `cmd_get_current_game` 并初始化 Snake。
- Esc / B 退出后窗口关闭，`SharedGame.active` 复位。
- `make test-core` 通过。

### Phase 1B：Snake 可玩闭环（已完成）

目标：Snake 规则完整，键盘与手柄都能玩。已在提交 `a2105ff` 中完成。

- `game_engine.js` 实现 `GameEngine` 基类、`SnakeEngine`、主题绘制器、游戏状态机。
- 键盘输入：方向键/WASD、Enter/Space、P、Escape 直接进入 `handleInput()`。
- 手柄输入：`gamepad_loop` 在 `SharedGame.active == true` 时优先转发：
  - D-pad → `game-input` direction
  - A → confirm
  - B → cancel / exit
  - Start → pause
- 游戏结束通过 `cmd_game_end(result, score)` 回传 app。
- `core/src/pet.rs` 与 `core/src/bridge.rs` 新增 `GamePlay` / `GameWin` / `GameLose` 状态；前端 sprite 暂时可映射到已有 happy/confused/idle 帧，避免素材阻塞。

验收结果：

- Snake 能移动、吃食物、增长、加速、撞墙/撞自己失败、达到长度胜利。
- 游戏激活时 D-pad 不再触发滚轮，A/B 不再触发普通宠物动作。
- 胜利后宠物进入 `GameWin`，失败或取消后进入 `GameLose` 或 `Idle`。
- `make test-core`、`cd app/frontend && npx vitest run`、`make test-app`、`make test-fast` 均通过。

### Phase 1 代码量预估

第一阶段已完成，实际提交约 **1278 行新增/修改**。上浮主要来自完整前端 Snake 实现、Tauri game 窗口生命周期、3×3 面板扩展、宠物新状态测试和 capabilities schema 同步。

| 文件 | Phase | 预计行数 | 说明 |
|------|-------|----------|------|
| `core/src/minigame.rs` | 1A | 140-190 | `GameDef` 数据结构、默认 Snake、bounds 校验、基础测试 |
| `core/src/lib.rs` | 1A | 1 | 导出 `minigame` 模块 |
| `app/src/game.rs` | 1A | 170-230 | `SharedGame`、动态 `game` 窗口、IPC、结束清理、日志 |
| `app/src/lib.rs` | 1A | 10-20 | 注册模块、state、IPC 命令 |
| `app/capabilities/default.json` | 1A | 1-3 | 增加 `"game"` 窗口权限 |
| `app/frontend/game.html` | 1A | 35-55 | canvas、HUD、脚本/CSS 引入 |
| `app/frontend/css/game.css` | 1A | 45-75 | 全屏透明 canvas、HUD、开始/结束状态 |
| `app/frontend/js/panel.js` | 1A | 8-20 | 增加或替换"游戏"入口 |
| `app/frontend/js/game_engine.js` | 1A+1B | 320-430 | engine 基类、Snake 规则、输入、渲染、主题绘制 |
| `app/src/gamepad.rs` | 1B | 35-70 | 游戏激活时输入独占与 `game-input` 转发 |
| `core/src/pet.rs` | 1B | 20-35 | `GamePlay` / `GameWin` / `GameLose` 状态与测试更新 |
| `core/src/bridge.rs` | 1B | 20-35 | `PetStateName` 同步、序列化快照更新 |
| `app/frontend/js/sprite.js` 或 `app.js` | 1B | 10-35 | 游戏状态临时映射到现有 sprite 帧 |
| 测试与快照 | 1A+1B | 60-100 | core 单测、insta 快照、前端轻量测试（如抽离纯逻辑） |

风险缓冲：

- **窗口透明与焦点**：Tauri/WebView2 在全屏透明窗口上可能需要额外 `set_focus()`、`always_on_top` 或 monitor 尺寸兜底，预留 30-80 行。
- **手柄输入优先级**：`gamepad_loop` 当前已有 panel 独占、voice 按住态、滚轮映射，插入游戏独占分支时要避免影响这些路径，预留 20-50 行。
- **前端测试难度**：如果 `game_engine.js` 直接绑定 DOM/canvas，测试会变重；建议把 Snake 状态更新函数保持可纯测，减少后续维护成本。

建议第一阶段的实现顺序：

1. 先做 `core/src/minigame.rs` 和 `app/src/game.rs`，让 `cmd_start_game` 能打开窗口。
2. 再做 `game.html/css/js` 的加载与静态绘制，确认 capabilities 和 Tauri API 可用。
3. 然后补 Snake 规则和键盘输入。
4. 最后插入 `gamepad_loop` 独占转发与宠物状态联动。

### Phase 2：体系补全（下一步）

目标：从"能玩 Snake"扩展到可配置游戏系统，并接入 AI 工具。

- `game_engine.js` 增加 Memory 和 Catch，实现固定模板注册机制。
- `config/minigames.yml` 增加三种游戏默认配置与难度预设。
- `core/src/minigame.rs` 增加 `save_game()`、`load_game()`、`list_games()`，目录为 `~/.ai-pad/games/`，格式优先 YAML，规则保持可人工审查。
- 新增分数持久化，目录为 `~/.ai-pad/scores/`，采用 append-only JSONL，方便 `rg` 检索和人工排查。
- `ActionBus` 在现有 `PlayGameDefault` 基础上增加 `PlayGame(GameDef)` 或 `PlayGamePreset(String)`，前端、手柄、AI 工具都走同一入口。

验收标准：

- Snake / Memory / Catch 三种游戏都可通过预设启动。
- 分数能持久化并可按游戏名、日期、结果 grep。
- 游戏配置非法时 Rust 侧拒绝启动，并给出可读错误。

### Phase 3：AI 工具接入

目标：让普通对话 Agent 自行决定何时启动游戏，而不是 Rust 侧做关键词分类。

- `core/src/tools.rs` 增加 `perform_game`：AI 提交完整 `GameDef`，Rust validate 后保存，并通过 app 层已有 `start_game(GameDef)` 通道触发启动。
- 增加 `play_game`：按已保存名称或内置预设启动；默认 Snake 的非 AI 启动路径已经由 `ActionBus::PlayGameDefault` / `cmd_start_game` 覆盖。
- `core/src/agent.rs` 注册游戏工具；`core/src/prompts.rs` 补充能力说明。
- 不做 Rust 侧关键词匹配，不做小分类器；遵循现有原则，让模型在普通对话中自行选择工具。

验收标准：

- 用户说"来一局慢一点的毛线球小游戏"，AI 能生成 bounded `GameDef` 并启动。
- AI 生成参数越界时被 `validate_game_def()` 拒绝或归一化，不影响桌面应用稳定性。
- 工具测试覆盖无效配置、保存失败、通道未初始化等路径。

### Phase 4：体验打磨

- 游戏进入/退出动画过渡。
- 音效反馈（吃到食物、翻牌、击中）。
- 手柄震动支持。
- 难度曲线自适应。
- 更多主题皮肤。
- 胜利触发庆祝舞蹈，失败触发短暂安慰/鼓励气泡。
