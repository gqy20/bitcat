# 俯视角 RPG + AI NPC 对话 / AI 关卡生成

> 创建日期：2026-05-18
> 状态：规划中（未开始实施）
> 关联文档：
> - 现有游戏系统：[plan/minigame-system.md](minigame-system.md)
> - AI Agent 架构：CLAUDE.md §AI Agent
> - 结构化收尾：`core/src/agent_reaction.rs`（rig Extractor 复用）
> - 宠物精灵管线：`core/src/pet.rs` + `app/frontend/js/sprite-loader.js`

## 一、背景与目标

### 为什么做这个

当前桌宠已有完整的 AI Agent 管线（rig → Anthropic API → 流式回复 → bubble 渲染）和四个迷你游戏（Snake / Memory / Catch / Battle），但游戏和 AI 之间没有交集。这个计划把 **AI NPC 对话** 和 **AI 关卡生成** 组合进一个**俯视角网格 RPG**，让桌宠不只是桌面装饰，而是游戏世界的 AI 伙伴。

### 为什么选俯视角而不是横版

| 维度 | 冒险岛式横版 | **俯视角 RPG（选定）** |
|------|-------------|----------------------|
| 物理引擎 | AABB + 重力 + 跳跃 + 斜坡 | 网格移动（接近零物理） |
| 角色动画 | 8 方向 × 10 动作 × N 帧 | 4 方向 × 2 动作（站立/行走） |
| 地图碰撞 | 平台可达性 + 梯子/绳子 | `map[y][x].walkable` 一行判断 |
| AI 关卡生成 | 需保证跳跃可达性 | Flood Fill 连通性验证即可 |
| NPC 对话 | 需打断动作 | 原生停步对话，零冲突 |
| 新增代码量 | ~8600 行 | **~5000 行** |
| exe 体积增量 | +6~18 MB | **+3~8 MB** |

### 核心目标

1. 玩家能在俯视角像素地图上四方向移动，遇到 NPC 按 A 键触发对话
2. NPC 对话由 AI 实时生成，每次不同，且能根据对话结果改变游戏状态（给道具、开门、变敌对）
3. AI 能根据主题生成新的地图关卡，保证可玩（起点到终点连通、敌人不在墙里）
4. 宠物作为 AI 伙伴跟随玩家，自主给出建议

---

## 二、核心设计决策

### 2.1 渲染方案：复用现有全屏透明 game 窗口

复用 `minigame-system.md` 已验证的全屏透明 Tauri 窗口方案，不做额外窗口。

```
Tauri game 窗口属性（复用现有）：
  - transparent: true
  - decorations: false
  - always_on_top: true
  - skip_taskbar: true
  - sized to screen resolution
```

RPG 画面在 Canvas 2D 上绘制，透明区域露出真实桌面。可以选择：
- **沉浸模式**：全屏不透明，完全替代桌面 → 传统 RPG 体验
- **桌面融合模式**：半透明地图叠加在桌面上 → 和桌宠生态一致

**结论**：先做沉浸模式（不透明背景），验证核心玩法。后续再加半透明融合模式作为可选开关。

### 2.2 地图格式：二维 Tile 数组 + Tiled 兼容

```jsonc
{
  "id": "forest_01",
  "theme": "forest",
  "width": 20,          // 格子数
  "height": 15,
  "tile_size": 32,      // 像素
  "layers": {
    "ground": [[0,0,1,1,...], ...],   // 地面装饰（全可通行）
    "collision": [[0,1,0,0,...], ...], // 0=可通行 1=不可通行
    "decor": [[-1,-1,5,-1,...], ...]   // -1=空 其他=装饰精灵索引（前景层）
  },
  "entities": [
    { "type": "npc", "id": "guard", "x": 12, "y": 7, "facing": "left",
      "personality": "gruff_warrior", "interaction_radius": 1.5 },
    { "type": "chest", "x": 5, "y": 3, "item": "iron_key", "opened": false },
    { "type": "enemy_spawn", "id": "slime_zone", "bounds": [8,10,14,14],
      "pool": ["slime", "blue_slime"], "max_active": 3 },
    { "type": "exit", "x": 19, "y": 7, "target": "forest_02", "spawn": "west" }
  ],
  "player_spawn": { "x": 1, "y": 7, "facing": "right" }
}
```

不直接用 Tiled 编辑器格式（TMX），但保持兼容性：后续可以写一个 TMX → 自有 JSON 的转换器。AI 生成时直接输出上述 JSON。

### 2.3 角色精灵格式：复用 v2 asset pack manifest

复用 `sprite-loader.js` 的 manifest v2 格式，为 RPG 角色定义新的状态集：

```jsonc
{
  "schemaVersion": 2,
  "id": "player_hero",
  "name": "冒险者",
  "mode": "sheet",
  "frameWidth": 32,
  "frameHeight": 32,
  "columns": 8,
  "spritesheet": "spritesheet.webp",
  "states": {
    "idle_down":   { "frames": [0], "loop": true },
    "idle_up":     { "frames": [8], "loop": true },
    "idle_left":   { "frames": [16], "loop": true },
    "idle_right":  { "frames": [24], "loop": true },
    "walk_down":   { "frames": [0,1,2,3], "frameDuration": 150, "loop": true },
    "walk_up":     { "frames": [8,9,10,11], "frameDuration": 150, "loop": true },
    "walk_left":   { "frames": [16,17,18,19], "frameDuration": 150, "loop": true },
    "walk_right":  { "frames": [24,25,26,27], "frameDuration": 150, "loop": true },
    "attack_down": { "frames": [32,33,34], "frameDuration": 80, "repeat": 1, "fallback": "idle_down" },
    "hurt":        { "frames": [40,41], "frameDuration": 100, "repeat": 2, "fallback": "idle_down" }
  }
}
```

每个角色 32×32 px，4 方向各一套站立/行走 + 攻击/受击，约 48 帧，一个 spritesheet 即可。

### 2.4 AI NPC 对话架构

核心思路：**复用现有 `agent.rs` 的 `chat_stream()` 管线**，不做第二套对话系统。

```
玩家按 A 触发对话
  ↓
Rust 侧拼装 context：
  ├── NPC preamble（从配置加载的人格设定）
  ├── 游戏状态摘要（玩家等级/背包/当前位置/已完成任务）
  ├── 最近对话历史（该 NPC 的短期记忆）
  └── 可用动作列表（give_item / open_path / attack / give_quest）
  ↓
调用现有 agent.chat_stream(preamble + context)
  ↓
AI 回复分两路：
  ├── 自然语言 → 对话框打字机显示
  └── 尾部 JSON 动作块 → 解析并执行游戏状态变更
       例：{"action":"give_item","item":"forest_map"}
       例：{"action":"open_path","target":"east_gate"}
       例：{"action":"change_attitude","to":"hostile","reason":"你冒犯了我"}
```

**动作解析方案**：复用 `agent_reaction.rs` 的 rig Extractor 模式。NPC 对话结束后，用一次结构化提取调用，从 AI 回复中抽取 0~N 个 `NpcAction`。不依赖 JSON mode（ Anthropic 不总是可靠输出 JSON），而是用 Extractor 做后置解析。

### 2.5 AI 关卡生成架构

```
玩家进入传送门 / 选择"探索新区域"
  ↓
Rust 侧拼装生成 prompt：
  ├── 主题描述（forest / cave / dungeon / castle / lava）
  ├── 难度参数（基于玩家等级：敌人密度、地图大小、陷阱数量）
  ├── 输出格式说明 + few-shot 示例（1-2 个完整 tilemap JSON）
  └── 约束（起点必须在边缘、必须有至少一条到出口的路径）
  ↓
调用 AI（非流式，一次性输出完整 JSON）
  ↓
后处理管道：
  ├── JSON schema 校验
  ├── Flood Fill 连通性验证（起点→所有出口）
  ├── 实体位置修正（NPC/敌人/宝箱不能在墙里）
  ├── 难度归一化（敌人数量 × 等级系数）
  └── 失败则带错误信息重试（最多 3 次）
  ↓
缓存到 localStorage（seed 相同不重复生成）
  ↓
前端加载并渲染
```

---

## 三、模块设计

### 3.1 前端 RPG 引擎（新增 `rpg_engine.js`）

从 `game_engine.js` 的 `GameEngine` 基类模式扩展：

```
RpgEngine
  ├── init(config: RpgSceneDef)     // 加载地图 + 精灵 + 初始化实体
  ├── update(dt_ms)                 // 游戏逻辑 tick
  │     ├── updatePlayer(dt)        // 输入→意图→网格移动
  │     ├── updateNpcs(dt)          // NPC idle 动画/巡逻
  │     ├── updateEnemies(dt)       // 敌人 AI（巡逻/追逐）
  │     ├── updateCamera(dt)        // 摄像机整数偏移
  │     └── checkInteractions()     // 玩家与实体碰撞检测
  ├── render(ctx)                   // 绘制
  │     ├── renderGround()          // 地面层
  │     ├── renderEntities()        // NPC/敌人/宝箱（按 y 排序伪深度）
  │     ├── renderPlayer()          // 玩家精灵
  │     ├── renderDecor()           // 前景装饰层
  │     └── renderHud()             // 血条/小地图/技能栏
  ├── handleInput(input)            // 方向/A/B/Start
  └── destroy()
```

**网格移动核心**（极简）：

```javascript
// 玩家输入 → 移动意图
const intent = { dx: 0, dy: 0 };
if (keys.left)  intent.dx = -1;
if (keys.right) intent.dx = 1;
if (keys.up)    intent.dy = -1;
if (keys.down)  intent.dy = 1;

// 碰撞检测（一行）
const nx = player.tileX + intent.dx;
const ny = player.tileY + intent.dy;
if (map.collision[ny]?.[nx] === 0) {
  player.tileX = nx;
  player.tileY = ny;
  // 视觉插值（像素坐标平滑过渡）
  player.visualX += (nx * TILE - player.visualX) * 0.3;
  player.visualY += (ny * TILE - player.visualY) * 0.3;
}
```

### 3.2 对话框 UI（`dialogue_ui.js`）

独立于 bubble 窗口，渲染在 game canvas 内部或 canvas 上层 DOM：

```
┌──────────────────────────────────┐
│  [NPC 头像]  守卫队长             │
│  "这片森林最近不太安宁……         │
│   你要往东边去？那可不容易。"     │
│                                  │
│  ┌──────────┐  ┌──────────┐     │
│  │ 说服他    │  │ 离开      │     │
│  └──────────┘  └──────────┘     │
└──────────────────────────────────┘
```

- 打字机效果逐字显示 AI 回复（复用 `bubble.js` 的逐字渲染思路）
- 选项由 AI 动态生成（不是预设在配置里），每轮对话可能不同
- 方向键选选项，A 确认

### 3.3 AI NPC 系统（Rust 侧）

#### 数据结构

```rust
// core/src/rpg.rs

/// NPC 模板定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcDef {
    pub id: String,
    pub role: String,           // guard / merchant / quest_giver / ...
    pub personality: String,    // preamble 标识符
    pub interaction_radius: f32,
    pub default_attitude: NpcAttitude,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum NpcAttitude { Friendly, Neutral, Hostile }

/// AI 对话结束后提取的结构化动作。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NpcAction {
    pub action: String,         // give_item / open_path / attack / change_attitude / give_quest
    #[serde(default)]
    pub item: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub to: Option<String>,     // 态度目标
    #[serde(default)]
    pub reason: Option<String>,
}

/// 对话上下文（注入 prompt）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueContext {
    pub npc_name: String,
    pub npc_role: String,
    pub npc_attitude: String,
    pub player_level: u32,
    pub player_inventory: Vec<String>,
    pub current_location: String,
    pub completed_quests: Vec<String>,
    pub recent_history: Vec<DialogueTurn>,  // 最近 5 轮
}
```

#### 对话流程

```rust
impl PetAgent {
    /// NPC 对话：复用 chat_stream，替换 preamble 为 NPC 人格。
    pub async fn npc_dialogue(
        &self,
        context: DialogueContext,
        player_message: String,
    ) -> Result<NpcDialogueResponse> {
        // 1. 构建 NPC 专用 preamble
        let preamble = format_npc_preamble(&context);

        // 2. 调用现有 chat_stream（复用管线）
        let reply = self.chat_stream(&player_message, Some(&preamble)).await?;

        // 3. 用 Extractor 提取动作（复用 agent_reaction 的模式）
        let actions = self.extract_npc_actions(&reply).await?;

        Ok(NpcDialogueResponse { text: reply, actions })
    }
}
```

### 3.4 AI 关卡生成（Rust 侧）

```rust
// core/src/rpg.rs

/// AI 生成关卡请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateMapRequest {
    pub theme: String,          // forest / cave / dungeon / castle / lava
    pub difficulty: u8,         // 1-10，由玩家等级推算
    pub width: u32,
    pub height: u32,
    pub seed: Option<String>,
}

/// 生成结果（AI 输出经后处理后的合法地图）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedMap {
    pub id: String,
    pub theme: String,
    pub width: u32,
    pub height: u32,
    pub layers: MapLayers,
    pub entities: Vec<MapEntity>,
    pub player_spawn: SpawnPoint,
}
```

后处理管道（最关键）：

```rust
fn postprocess_generated_map(raw: Value) -> Result<GeneratedMap> {
    // 1. Schema 校验
    let map: GeneratedMap = serde_json::from_value(raw)?;

    // 2. Flood Fill 连通性：起点必须能到达所有 exit
    ensure_connectivity(&map)?;

    // 3. 实体位置修正：NPC/敌人/宝箱不能在 collision=1 的格子上
    fix_entity_positions(&mut map)?;

    // 4. 难度归一化：敌人数量不超过 difficulty × width × height / 50
    cap_enemy_density(&mut map, difficulty)?;

    Ok(map)
}
```

---

## 四、数据流

### 4.1 完整游戏流程

```
用户从面板选择"冒险" 或 对桌宠说"我们去冒险吧"
  ↓
ActionBus::PlayRpg / AI 工具调用
  ↓
app/src/game.rs: 创建全屏 game 窗口（复用现有逻辑）
  ↓
加载初始地图（内置或 AI 生成缓存）
  ↓
rpg_engine.js 初始化：
  ├── 解析地图 JSON
  ├── 加载角色 spritesheet（复用 sprite-loader.js）
  ├── 初始化玩家位置
  └── 开始游戏循环 update(dt) → render(ctx)
  ↓
游戏循环中：
  ├── 方向键 → 网格移动 → 碰撞检测
  ├── 接近 NPC → 显示交互提示 → 按 A 触发对话
  │     ↓
  │   Tauri IPC invoke("cmd_npc_dialogue", { npc_id, message })
  │     ↓
  │   Rust: 拼装 DialogueContext → agent.npc_dialogue()
  │     ↓
  │   AI 回复 → emit("npc-reply", { text, actions })
  │     ↓
  │   JS: 打字机显示文本 → 解析 actions → 执行游戏状态变更
  │     ↓
  │   如果有选项 → 等待玩家选择 → 再次 invoke
  │
  ├── 走到出口 → 检查是否已缓存目标地图
  │     ├── 有缓存 → 直接加载
  │     └── 无缓存 → invoke("cmd_generate_map", { theme, difficulty })
  │           ↓
  │         Rust: 构建生成 prompt → 调用 AI → 后处理 → 缓存 → 返回
  │           ↓
  │         JS: 加载新地图 → 播放场景切换动画
  │
  └── 随机遇敌 → 回合制战斗（改造现有 BattleEngine）
        ↓
      战斗结束 → 经验/道具 → 继续
```

### 4.2 IPC 命令清单

| 命令 | 方向 | 说明 |
|------|------|------|
| `cmd_rpg_start` | JS→Rust | 启动 RPG，加载初始地图 |
| `cmd_npc_dialogue` | JS→Rust | NPC 对话请求（NPC ID + 玩家消息） |
| `npc-reply` | Rust→JS | AI 回复 + 动作（emit 事件） |
| `cmd_generate_map` | JS→Rust | 请求 AI 生成新地图 |
| `map-generated` | Rust→JS | 生成结果（emit 事件） |
| `cmd_rpg_save` | JS→Rust | 存档（玩家状态 + 当前地图 + NPC 态度） |
| `cmd_rpg_load` | JS→Rust | 读档 |
| `cmd_rpg_end` | JS→Rust | 退出 RPG，关闭 game 窗口 |

---

## 五、文件清单与代码量

### 新增文件

| 文件 | 内容 | 预估行数 |
|------|------|---------|
| `core/src/rpg.rs` | NpcDef / NpcAction / DialogueContext / GenerateMapRequest / 后处理 / validate | ~450 |
| `app/frontend/js/rpg_engine.js` | RpgEngine：地图渲染/网格移动/摄像机/NPC交互/敌人AI/战斗触发/HUD | ~1200 |
| `app/frontend/js/dialogue_ui.js` | 对话框 UI：打字机/选项/头像/动画 | ~350 |
| `config/rpg_npc_personas.yml` | NPC 人格 preamble 模板库 | ~120 |
| `config/rpg_themes.yml` | 地图主题定义（tile集/敌人池/背景色） | ~80 |
| `app/frontend/__fixtures__/rpg/tiles/` | 基础 tileset 精灵图（32×32 像素块） | 美术资产 |
| `app/frontend/__fixtures__/rpg/player/` | 玩家 spritesheet + manifest | 美术资产 |
| `app/frontend/__fixtures__/rpg/npcs/` | 2-3 个 NPC spritesheet + manifest | 美术资产 |

### 修改文件

| 文件 | 改动 | 预估行数 |
|------|------|---------|
| `core/src/lib.rs` | `pub mod rpg` | 1 |
| `core/src/agent.rs` | `npc_dialogue()` 方法 + `generate_map()` | ~120 |
| `core/src/prompts.rs` | 新增 `rpg_npc` / `rpg_dungeon` prompt 段 | ~30 |
| `app/src/game.rs` | RPG 启动路径 + 新增 IPC 命令 | ~200 |
| `app/src/lib.rs` | 注册 rpg 模块和新命令 | ~20 |
| `app/src/action_bus.rs` | `PlayRpg` action | ~15 |
| `app/frontend/js/sprite-loader.js` | RPG 状态集支持（idle_up/down/walk 等） | ~50 |
| `app/frontend/js/game_engine.js` | BattleEngine 改造为 RPG 内回合制战斗 | ~100 |
| `app/frontend/game.html` | 引入 rpg_engine.js + dialogue_ui.js | ~5 |
| `app/frontend/css/game.css` | RPG HUD 样式 + 对话框样式 | ~80 |

### 行数分布

```
rpg_engine.js       ████████████████████████  1200  (43%)
rpg.rs              █████████                450   (16%)
game.rs 修改        ████                     200   (7%)
dialogue_ui.js      ███████                  350   (13%)
其余 12 个文件      ████████                 650   (21%)
──────────────────────────────────────────────────────
新增+修改总计                               ~2850 行（Rust）
                                            ~1950 行（JS）
                                            ~200 行（配置）
                                            ────────
                                            ~5000 行
```

---

## 六、实现阶段

### Phase 1：基础引擎 + 1 个 NPC 对话（MVP，~2200 行）

**目标**：能在地图上走路，走近 NPC 按 A 触发 AI 对话，对话能改变游戏状态。

1. `rpg_engine.js` 核心：地图渲染 + 网格移动 + 摄像机 + 1 个内置测试地图
2. `rpg.rs`：`NpcDef` / `DialogueContext` / `NpcAction` 结构定义
3. `agent.rs`：`npc_dialogue()` 方法（复用 chat_stream + Extractor）
4. `dialogue_ui.js`：打字机文本 + 选项按钮
5. `game.rs`：`cmd_rpg_start` / `cmd_npc_dialogue` / `npc-reply` IPC
6. `config/rpg_npc_personas.yml`：1-2 个 NPC 人格（守卫 + 商人）
7. 美术：最小 tileset（草地/树/水/路 4 种）+ 玩家精灵（4方向×2动作=8帧）+ 1 个 NPC 精灵

**验收**：
- 方向键移动，碰到树/水不穿墙
- 走近 NPC 显示"按 A 对话"提示
- 按 A 后对话框出现，AI 实时生成回复（打字机效果）
- 选择"说服"选项后 AI 决定成功/失败，对应改变 NPC 态度
- `make test-core` + 前端测试通过

### Phase 2：AI 关卡生成（~1200 行）

**目标**：走到地图出口时 AI 生成新区域。

1. `rpg.rs`：`GenerateMapRequest` / `GeneratedMap` / 后处理管道（Flood Fill + 实体修正）
2. `agent.rs`：`generate_map()` 方法
3. `prompts.rs`：`rpg_dungeon` 生成 prompt + few-shot 示例
4. `config/rpg_themes.yml`：3 个主题（森林/洞穴/地牢）
5. `rpg_engine.js`：场景切换动画 + 新地图加载
6. `game.rs`：`cmd_generate_map` / `map-generated` IPC

**验收**：
- 走到地图出口 → loading 动画 → AI 生成新地图 → 自动加载
- 生成的地图起点到出口连通（Flood Fill 验证通过）
- 不同主题有不同的 tile 视觉和敌人池
- 生成失败时自动重试，3 次失败用内置备用地图

### Phase 3：回合制战斗 + 敌人系统（~1000 行）

**目标**：遇到敌人触发回合制战斗，战斗结果影响探索。

1. `rpg_engine.js`：敌人 AI（巡逻范围 + 追逐半径）+ 遭遇检测
2. `game_engine.js` BattleEngine 改造：RPG 内嵌模式（不启动独立 game 窗口）
3. 战斗结束：经验 → 升级 / 道具掉落 → 背包
4. 敌人 spritesheet（史莱姆 × 2-3 种变体）

**验收**：
- 走近敌人巡逻区域 → 进入战斗
- 攻击/技能/防御/逃跑选项完整可用
- 胜利获得经验，失败回到地图起点
- 宠物在战斗中给出建议（AI 伙伴）

### Phase 4：存档 + HUD + 打磨（~600 行）

1. 存档系统（localStorage + Tauri fs 备份）
2. HUD 完善（小地图 / 背包界面 / 任务日志）
3. 音效（脚步 / 对话 / 战斗 / 升级）
4. 宠物伙伴跟随动画 + AI 建议气泡
5. 场景切换过渡动画

---

## 七、可复用的现有资产清单

| 现有模块 | 文件路径 | 复用方式 |
|---------|---------|---------|
| GameEngine 基类模式 | `app/frontend/js/game_engine.js` | RpgEngine 继承同一接口 |
| BattleEngine 回合制 | `app/frontend/js/game_engine.js` L900-1300 | 改造为 RPG 内嵌战斗 |
| chat_stream() 管线 | `core/src/agent.rs` | NPC 对话直接调用 |
| rig Extractor 模式 | `core/src/agent_reaction.rs` | NpcAction 结构化提取 |
| prompts.yml 加载 | `core/src/prompts.rs` | 扩展 rpg_npc / rpg_dungeon 段 |
| build_context() | `core/src/agent.rs` | 注入游戏状态替代聊天记忆 |
| sprite-loader.js v2 | `app/frontend/js/sprite-loader.js` | RPG 角色 spritesheet 加载 |
| manifest v2 格式 | `sprite-loader.js` + `__fixtures__/pets/*/manifest.json` | RPG 角色用同一格式 |
| 全屏 game 窗口 | `app/src/game.rs` | 复用窗口创建/关闭/焦点管理 |
| Tauri IPC 模式 | `app/src/commands.rs` | 新增 RPG 相关命令 |
| 浮动伤害数字 | `game_engine.js` drawFloaters | 战斗伤害显示 |
| 打字机文本渲染 | `app/frontend/js/bubble.js` | 对话框参考 |
| CSS 动画/粒子 | `app/frontend/css/game.css` + `particles.js` | 复用粒子系统 |

---

## 八、风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| AI 生成的地图 JSON 格式不稳定 | 关卡不可玩 | 后处理管道强制校验 + 内置备用地图 + 3 次重试 |
| AI NPC 动作解析失败 | 对话无效果 | Extractor 容错：解析失败时 NPC 仅说话不执行动作，不阻塞 |
| 全屏窗口遮挡桌宠 | 宠物不可见 | RPG 使用不透明沉浸模式时隐藏桌宠；后续加半透明模式恢复 |
| API 调用延迟影响对话体验 | 打字机卡顿 | 对话流式输出（复用 chat_stream），首 token <1s 时体感流畅 |
| 美术资产阻塞开发 | 无法测试 | Phase 1 用纯色块替代精灵，验证逻辑后再补美术 |
| exe 体积增长过大 | 分发困难 | RPG 精灵保持 32×32，总资产控制在 5MB 以内 |

---

## 九、exe 体积预算

| 资产类型 | 数量 | 单个大小 | 总计 |
|---------|------|---------|------|
| Tileset（32×32 像素块 spritesheet） | 3 个主题 × ~64 tiles | ~30 KB | ~90 KB |
| 玩家 spritesheet（48 帧 @ 32×32） | 1 | ~15 KB | ~15 KB |
| NPC spritesheet（48 帧 @ 32×32） | 3 | ~15 KB | ~45 KB |
| 敌人 spritesheet（16 帧 @ 32×32） | 3 | ~8 KB | ~24 KB |
| UI 资产（血条/背包/对话框） | 1 套 | ~20 KB | ~20 KB |
| 音效（mp3） | 8-10 个 | ~5 KB | ~50 KB |
| BGM（ogg） | 2-3 首 | ~500 KB | ~1.5 MB |
| **合计** | | | **~1.7 MB** |

当前 exe 22MB → 预计 **~24MB**，增量极小。如果后续加大量关卡美术才会到 28-30MB。
