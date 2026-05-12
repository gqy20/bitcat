# 结构化输出设计：舞蹈 & 游戏（方案 B — rig TypedPrompt）

> **决策**：舞蹈和游戏的内容生成采用 rig-core 的 `prompt_typed::<T>()` 路径，
> 让 LLM 直接输出符合 `DanceDef` / `GameDef` JSON Schema 约束的结构化数据。
>
> **核心优势**：AI 真正拥有创作权——动作序列的长度、节奏、组合全部由 LLM 决定，而非硬编码查表。

---

## 一、背景与动机

### 当前实现的缺陷

当前 `create_dance` 工具虽然注册在 AI Agent 上，但**实际编排逻辑是纯查表**：

```rust
// tools.rs:269 — execute_create_dance 的真实执行路径
let steps = crate::dance::choreograph(&args.mood);  // ← match 表，非 AI 生成
```

```
Roadmap 承诺:  AI 编排动作序列 → 生成 YML → 播放
当前实际:     AI 传 (name, mood) → Rust 侧 mood→固定模板 → 固定序列
```

`choreograph()` 是一个硬编码的 `match` 表（`dance.rs:122-158`），"happy" 永远返回同样的 5 步，"angry" 永远返回同样的 5 步。**AI 没有任何创作空间**。

### 为什么不用方案 A（Tool Call 扩展）

| 维度 | 方案 A（Tool Call 扩展） | **方案 B（TypedPrompt）** |
|------|--------------------------|--------------------------|
| 创作自由度 | 中等（受 tool arguments 大小限制 ~4KB） | 高（completion response 可达 max_tokens=256K） |
| 输出质量 | LLM 在 tool parameters 中填字段 | LLM 直接输出完整结构体，思维链更完整 |
| Schema 约束力 | 依赖模型遵循 tool definition | Provider 原生 strict mode 保证合规 |
| 与现有流程关系 | 改造现有 Tool Call | 新增独立调用入口，不影响 chat_stream |
| 复杂度 | 低（改 struct + if） | 中（新方法 + 意图检测 + 兜底） |

**结论**：舞蹈和游戏是本项目的**核心差异化功能**——"AI 动态生成可玩内容"。值得用更强的结构化输出方案来保证创作质量。

---

## 二、技术原理

### rig TypedPrompt API

从 rig-core 0.36.0 源码确认的完整调用链：

```rust
// API 入口（rig-core/src/agent/completion.rs:409-417）
fn prompt_typed<T>(&self, prompt: impl Into<Message> + WasmCompatSend)
    -> TypedPromptRequest<T, Standard, M, P>
where
    T: schemars::JsonSchema + serde::de::DeserializeOwned + WasmCompatSend
```

### 完整数据流

```
agent.prompt_typed::<DanceDef>("设计一段开心的舞蹈")
  │
  ├─ TypedPromptRequest::from_agent(agent, prompt)
  │   ├─ 克隆 Agent: preamble / model / tools / PermissionHook
  │   └─ inner.output_schema = Some(schema_for!(DanceDef))  // 编译期宏
  │
  ├─ .await (IntoFuture → send())
  │   ├─ inner.send() → build_completion_request()
  │   │   └─ CompletionRequest { output_schema: Some(schema) }
  │   │       └─ Provider 转换:
  │   │           └─ Anthropic: sanitize_schema() → output_config.format.json_schema
  │   │           └─ OpenAI: sanitize_schema() → response_format.json_schema
  │   │           └─ Gemini: response_mime_type + response_json_schema
  │   ├─ HTTP POST → LLM（受 Schema 约束输出 JSON 文本）
  │   ├─ response.is_empty()? → Err(EmptyResponse)
  │   └─ serde_json::from_str(&response)? → DanceDef  // 唯一解析点
  │
  └─ Result<DanceDef, StructuredOutputError>
```

### Schema 传递细节（以 Anthropic 为例）

Anthropic provider 对 schema 做 `sanitize_schema()` 清洗：

1. 所有 object 强制加 `"additionalProperties": false`
2. 所有 properties 自动加入 `"required"` 数组
3. 移除数值约束：`minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`
4. `oneOf` → `anyOf`（Anthropic 不支持 oneOf）
5. 递归处理 `$defs`, `properties`, `items`, `anyOf`, `allOf`

最终发给 Anthropic API 的请求体片段：

```json
{
  "output_config": {
    "format": {
      "type": "json_schema",
      "schema": { "<sanitized DanceDef schema>" }
    }
  }
}
```

### Trait 约束检查清单

类型 T 必须同时满足：

| Trait | 来源 | 用途 |
|-------|------|------|
| `schemars::JsonSchema` | rig-core 间接引入（已满足） | 编译期生成 JSON Schema |
| `serde::de::DeserializeOwned` | 已有 | 反序列化 LLM 输出 |
| `WasmCompatSend` | rig-core 提供 | 异步兼容 |

**`schemars` 无需新增依赖**——通过 `rig-core 0.36.0` 间接引入：

```
schemars v1.2.1
└── rig-core v0.36.0
    └── ai-pad-core v0.1.0
```

---

## 三、类型定义改造

### 3.1 DanceDef 加 JsonSchema derive

```rust
// core/src/dance.rs — 当前只有 Serialize/Deserialize

use schemars::JsonSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DanceAction {
    Jump,
    Spin,
    Wave,
    Shake,
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DanceStep {
    pub action: DanceAction,
    #[serde(rename = "duration_ms")]
    #[schemars(description = "该动作持续毫秒数", minimum = 50, maximum = 3000)]
    pub duration_ms: u32,
    #[serde(default = "default_repeat")]
    #[schemars(default = 1, minimum = 1, maximum = 10)]
    pub repeat: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DanceDef {
    #[schemars(title = "舞蹈名称", description = "用于文件名的英文标识，如 happy_twist")]
    pub name: String,
    #[serde(rename = "loop", default = "default_loop")]
    #[schemars(default = true, description = "是否循环播放")]
    pub loop_: bool,
    #[schemars(description = "按时间轴排列的动作步骤")]
    pub steps: Vec<DanceStep>,
}
```

`schemars` attributes 的作用：

- `description` / `title` → 写入 Schema，帮助 LLM 理解字段语义
- `minimum` / `maximum` → 数值范围约束（注意：Anthropic sanitize 会移除这些，但 OpenAI/Gemini 保留）
- `default` → 提示 LLM 可选字段的推荐值
- `enum` → 枚举值约束

### 3.2 GameDef（Phase 2 新建，core/src/minigame.rs）

```rust
use schemars::JsonSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MinigameType {
    Snake,
    Memory,
    Whack,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GameGrid {
    /// 贪吃蛇/打地鼠: width x height; 记忆翻牌: cols x rows
    pub width: u32,
    pub height: u32,
    #[schemars(default = 16, description = "每个格子像素大小")]
    pub cell_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlayerConfig {
    #[schemars(description = "起始位置 [x, y]")]
    pub start_position: [u32; 2],
    #[schemars(default = 120, description = "移动间隔毫秒")]
    pub speed_ms: Option<u32>,
    #[schemars(default = 3, description = "初始长度（蛇）")]
    pub initial_length: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct GameRules {
    #[schemars(default = true)]
    pub walls_kill: Option<bool>,
    #[schemars(default = true)]
    pub self_kill: Option<bool>,
    #[schemars(description = "翻牌后显示时间(ms)")]
    pub flip_time_ms: Option<u32>,
    #[schemars(description = "目标出现时间范围 ms")]
    pub show_time_range: Option<(u32, u32)>,
    #[schemars(description = "游戏时长(秒)")]
    pub duration_seconds: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DialogueConfig {
    #[schemars(description = "开始时的提示语")]
    pub start: String,
    #[schemars(description = "胜利时的对话，可用 {score} 占位符")]
    pub win: String,
    #[schemars(description = "失败时的对话")]
    pub lose: String,
}

/// 游戏完整定义 —— prompt_typed::<GameDef>() 的目标类型
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GameDef {
    #[schemars(title = "游戏类型")]
    pub game_type: MinigameType,
    #[schemars(title = "显示标题", description = "如「抓星星」「猫猫记忆」")]
    pub title: String,
    pub grid: GameGrid,
    #[serde(default)]
    pub player: Option<PlayerConfig>,
    #[serde(default)]
    pub rules: Option<GameRules>,
    pub dialogue: DialogueConfig,
}
```

---

## 四、Agent 层改造

### PetAgent 新增方法

```rust
// core/src/agent.rs — 在 impl PetAgent 中新增

use crate::dance::DanceDef;
// Phase 2: use crate::minigame::GameDef;
use rig::completion::TypedPrompt;

impl PetAgent {
    // ---- 现有方法不变 ----
    // pub async fn chat(&self, message: &str) -> Result<String, String>
    // pub async fn chat_stream<F>(&self, message: &str, on_chunk: F) -> Result<String, String>

    /// 用结构化输出生成舞蹈定义。
    ///
    /// 走 rig 的 prompt_typed 路径：LLM 输出被 DanceDef 的 JSON Schema 约束，
    /// 反序列化后直接得到完整的 Rust 结构体。AI 自由决定：
    /// - 步骤数量（不受模板限制）
    /// - 每个 action 的 duration_ms（节奏感）
    /// - repeat 次数（强调某个动作）
    /// - 整体 loop 策略
    pub async fn generate_dance(&self, user_prompt: &str) -> Result<DanceDef, String> {
        let def: DanceDef = self
            .agent
            .prompt_typed(user_prompt)
            .await
            .map_err(|e| format!("生成舞蹈失败: {e}"))?;
        Ok(def)
    }

    // Phase 2:
    // pub async fn generate_game(&self, user_prompt: &str) -> Result<GameDef, String> { ... }
}
```

**关键点**：`self.agent` 类型是 `Agent<anthropic::CompletionModel, PermissionHook>`，已实现 `TypedPrompt` trait（rig-core 第367行），`.prompt_typed()` 直接可用，PermissionHook 自动生效。

---

## 五、意图检测与调度

### 核心问题

现有系统只有一条管道 `chat_stream()` → 流式文本到 bubble。方案 B 需要第二条管道 `prompt_typed::<T>()` → 结构体 → 存储播放。**必须决定何时走哪条路。**

### 推荐策略：前置关键词匹配（策略 1）

```rust
// app/src/gamepad.rs（或新建 app/src/intent.rs）

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    Chat,         // 默认：普通对话，走 chat_stream
    CreateDance,  // 舞蹈创作，走 prompt_typed::<DanceDef>
    CreateGame,   // 游戏创作，走 prompt_typed::<GameDef>（Phase 2）
}

/// 轻量级意图分类：零 token 消耗，零延迟
pub fn classify_intent(msg: &str) -> Intent {
    let lower = msg.to_lowercase();

    // 舞蹈关键词
    if lower.contains("跳舞") || lower.contains("舞蹈") || lower.contains("跳个")
        || lower.contains("扭一扭") || lower.contains("表演个")
    {
        return Intent::CreateDance;
    }

    // 游戏关键词（Phase 2 启用）
    if lower.contains("贪吃蛇") || lower.contains("记忆翻牌") || lower.contains("打地鼠")
        || lower.contains("来局") || lower.contains("玩个游戏") || lower.contains("小游戏")
    {
        return Intent::CreateGame;
    }

    Intent::Chat
}
```

### 后续升级路径：策略 2（两轮 AI 判断）

当关键词覆盖不足时，可以升级为先让 AI 分类再分发：

```
原始消息 → chat("用户想做什么？只回复: DANCE/GAME/CHAT")
         → 如果是 DANCE → prompt_typed::<DanceDef>(原始消息)
         → 如果是 GAME → prompt_typed::<GameDef>(原始消息)
         → 如果是 CHAT  → chat_stream(原始消息)
```

**优点**：表达方式无限灵活；**缺点**：多一轮 API 调用（额外延迟+费用）。建议作为 v2 迭代。

### 不推荐的策略 3（Tool 内部调 prompt_typed）

把 `create_dance` 工具内部改为调 `prompt_typed`。问题：
1. 当前 `execute_create_dance` 是同步签名 (`fn(&args) -> ToolResult`)，`prompt_typed` 是 async
2. Tool 执行上下文中没有 Agent 实例引用（只收到 args）
3. 需要重构整个 tool 执行模型，改动面最大

---

## 六、App 层调度函数

### run_dance_generation（和 run_ai_chat 平级的新函数）

```rust
// app/src/gamepad.rs

/// 用结构化输出生成并播放舞蹈
///
/// 与 run_ai_chat 平级：chat 走流式文本管道，dance 走结构化输出管道。
pub fn run_dance_generation(
    rt: &tokio::runtime::Runtime,
    agent: &PetAgent,
    app: &tauri::AppHandle,
    msg: &str,
    memory: &mut MemoryStore,
    memory_config: &MemoryConfig,
) {
    info!(msg = %msg, "→ 舞蹈生成开始");

    // 1. 显示等待状态（非流式，一次性提示）
    let _ = bubble::show_static_bubble(app, "正在编排舞蹈...");

    // 2. 构建带上下文的 prompt
    let ctx = memory.build_context(memory_config);
    let enriched_msg = if ctx.is_empty() {
        msg.to_string()
    } else {
        format!("{ctx}\n用户说: {msg}")
    };

    // 3. 调用 prompt_typed
    let result = rt.block_on(agent.generate_dance(&enriched_msg));

    match result {
        Ok(def) => {
            let total_ms = def.total_duration_ms();
            info!(name = %def.name, steps = def.steps.len(), duration_ms = total_ms,
                "← 舞蹈生成成功");

            // 4. 持久化
            match crate::dance::save_dance(&def) {
                Ok(path) => {
                    info!(path = %path.display(), "舞蹈已保存");

                    // 5. 通知前端播放
                    let _ = app.emit("play-dance", &def);

                    // 6. 显示完成提示
                    let _ = bubble::show_static_bubble(
                        app,
                        &format!("「{}」编好了！({}步, {}ms)", def.name, def.steps.len(), total_ms),
                    );
                }
                Err(e) => {
                    warn!(error = %e, "保存舞蹈失败");
                    let _ = bubble::show_static_bubble(app, &format!("保存失败: {e}"));
                }
            }

            // 7. 记录到短期记忆
            memory.record_conversation(msg, &format!("生成了舞蹈「{}」", def.name), memory_config);
        }
        Err(e) => {
            warn!(error = %e, "舞蹈生成失败，回退到查表模板");

            // 兜底：使用 choreograph 查表
            let fallback_def = crate::dance::DanceDef {
                name: "fallback_dance".into(),
                loop_: true,
                steps: crate::dance::choreograph("happy"),
            };
            let _ = app.emit("play-dance", &fallback_def);
            let _ = bubble::show_static_bubble(app, "用了预设舞蹈~");
        }
    }
}
```

### gamepad_loop 中的调度点

```rust
// app/src/gamepad.rs — gamepad_loop() 中现有的 AI 对话触发处

// 当前代码（约第409行）：
if let (Some(msg), Some(ag)) = (&agent_msg, get_agent(&agent)) {
    // ... 构建 enriched_msg ...
    run_ai_chat(rt, ag, app, &enriched_msg, "", &mut memory, ...);
}

// 改为：
if let (Some(msg), Some(ag)) = (&agent_msg, get_agent(&agent)) {
    match classify_intent(&msg) {
        Intent::Chat => {
            run_ai_chat(rt, ag, app, &enriched_msg, "", &mut memory, ...);
        }
        Intent::CreateDance => {
            run_dance_generation(rt, ag, app, &msg, &mut memory, &memory_config);
        }
        Intent::CreateGame => {
            // Phase 2: run_game_generation(...)
            run_ai_chat(rt, ag, app, &enriched_msg, "", &mut memory, ...); // 暂时 fallback 到 chat
        }
    }
}
```

---

## 七、错误处理与兜底链

```
prompt_typed::<DanceDef>() 调用
  │
  ├─ 成功 → DanceDef → save_dance() → emit("play-dance") → 前端播放
  │
  ├─ StructuredOutputError::EmptyResponse
  │   → warn 日志
  │   → 兜底: choreograph("happy") → 仍能跳舞
  │
  ├─ StructuredOutputError::DeserializationError
  │   → LLM 返回了不符合 Schema 的 JSON（理论上 strict mode 不应发生）
  │   → warn 日志 + 打印原始响应
  │   → 兜底: choreograph(从 msg 提取 mood 关键词)
  │
  └─ PromptError (网络超时 / API Key 无效 / rate limit)
      → 兜底: choreograph("happy")  // 离线也能跳舞
```

**兜底的意义**：即使 AI 服务完全不可用，用户按 A 键触发"跳舞"时至少能看到一段预设动画，而不是什么都不发生。

### 重试策略（可选增强）

```rust
const MAX_RETRIES: u32 = 1;

async fn generate_with_retry(agent: &PetAgent, prompt: &str) -> Result<DanceDef, String> {
    let mut last_err = String::new();
    for attempt in 0..=MAX_RETRIES {
        match agent.generate_dance(prompt).await {
            Ok(def) => return Ok(def),
            Err(e) => {
                last_err = e;
                if attempt < MAX_RETRIES {
                    debug!(attempt, "重试舞蹈生成...");
                    // 可选：修改 prompt 加强约束
                }
            }
        }
    }
    Err(last_err)
}
```

---

## 八、与现有系统的交互

| 组件 | 是否受影响 | 说明 |
|------|-----------|------|
| **PermissionHook** | 自动生效 | 同一个 Agent 实例，hook 拦截危险工具调用。但设置了 `output_schema` 后 LLM 倾向于直接输出 JSON 而非调工具，实际触发概率低 |
| **MemoryStore** | 需手动注入 | `prompt_typed` 不自动携带上下文记忆。需在构造 prompt 时手动拼接 `memory.build_context()` |
| **config/prompts.yml (preamble)** | 自动生效 | Agent 的 preamble 作为系统提示词发送，AI 知道自己是 8Bit Cat |
| **create_dance Tool** | 降级/移除 | 方案 B 下不再需要通过 Tool Call 创建舞蹈。可保留给简单模式（只传 mood 走查表），或完全替换 |
| **choreograph() 查表** | 降级为兜底 | 从主路径降级为 `prompt_typed` 失败时的 fallback |
| **前端 dancePlayer** | 不变 | 无论 DanceDef 来源（AI 生成 or 查表），前端播放机制相同 |
| **chat_stream 流式管道** | 不变 | 普通对话完全不受影响，两条管道并行 |

---

## 九、实施步骤

### Phase 1：舞蹈（预计 1-2 天）

| 步骤 | 文件 | 改动 | 行数 |
|------|------|------|------|
| 1 | `core/src/dance.rs` | `DanceDef` / `DanceStep` / `DanceAction` 加 `JsonSchema` derive + schemars attributes | ~15 |
| 2 | `core/Cargo.toml` | 确认无需新增依赖（schemars 通过 rig-core 引入） | 0 |
| 3 | `core/src/agent.rs` | 新增 `generate_dance()` 方法 + import `TypedPrompt` + `DanceDef` | ~15 |
| 4 | `app/src/intent.rs`（新建） | `Intent` enum + `classify_intent()` 函数 | ~30 |
| 5 | `app/src/gamepad.rs` | 新增 `run_dance_generation()` 函数 + `gamepad_loop()` 中加 dispatch 分支 | ~70 |
| 6 | `app/src/bubble.rs` | 新增 `show_static_bubble()` 辅助函数（非流式一次性显示） | ~15 |
| 7 | `app/frontend/js/app.js` | dancePlayer 变量 + `updateDance(dt)` + 监听 `play-dance` 事件 + `loop()` 分支 | ~40 |
| 8 | `app/frontend/js/sprite.js` | 4 个新动作帧 (jump/spin/wave/shake) 加入 SPRITES 字典 | ~30 |
| 9 | 测试 | schema 合法性测试 + wiremock 集成测试 + 兜底路径测试 + intent 分类测试 | ~100 |

**Phase 1 小计：~315 行新/改代码，零新依赖**

### Phase 2：游戏（预计 2-3 天，复用同样模式）

| 步骤 | 文件 | 改动 | 行数 |
|------|------|------|------|
| 1 | `core/src/minigame.rs`（新建） | `GameDef` + 全部子类型，全部加 `JsonSchema` derive | ~120 |
| 2 | `core/src/lib.rs` | `pub mod minigame;` | 1 |
| 3 | `core/src/agent.rs` | 新增 `generate_game()` 方法 | ~12 |
| 4 | `app/src/intent.rs` | `Intent::CreateGame` 分支的关键词扩展 | ~5 |
| 5 | `app/src/gamepad.rs` | 新增 `run_game_generation()` + dispatch 分支 | ~60 |
| 6 | `app/src/commands.rs` | `cmd_start_game` + `cmd_game_input` IPC 命令 | ~30 |
| 7 | `app/frontend/js/game_engine.js`（新建） | GameEngine 类：Snake/Memory/Whack 三种实现 | ~250 |
| 8 | `app/frontend/panel.html` | `<canvas id="game-canvas">`（默认 hidden） | ~3 |
| 9 | `app/frontend/js/panel.js` | 游戏模式切换 + 输入转发 + game-end 回调 | ~20 |
| 10 | 测试 | GameDef schema 测试 + 各游戏类型单元测试 | ~80 |

**Phase 2 小计：~581 行新代码，零新依赖**

### Phase 3：后续增强

- **策略 2 升级**：关键词分类 → AI 两轮分类（更灵活的意图识别）
- **重试机制**：`generate_with_retry()` 带指数退避
- **prompt 工程**：针对舞蹈/游戏的专用 system prompt 注入（在 preamble 基础上追加领域指令）
- **Steam Workshop**：分享/订阅 AI 生成的 DanceDef 和 GameDef YAML

---

## 十、测试策略

### 10.1 Schema 合法性测试

```rust
#[test]
fn dance_def_json_schema_has_required_fields() {
    let schema = schemars::schema_for!(DanceDef);
    let json = serde_json::to_value(schema).unwrap();
    let props = json["properties"].as_object().unwrap();
    assert!(props.contains_key("name"));
    assert!(props.contains_key("steps"));
    assert!(props.contains_key("loop"));

    // 验证 steps.items 包含 action 和 duration_ms
    let steps_props = props["steps"]
        .get("items")
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(steps_props.contains_key("action"));
    assert!(steps_props.contains_key("duration_ms"));
}
```

### 10.2 反序列化边界测试

```rust
#[test]
fn dance_def_from_llm_output_ignores_extra_fields() {
    // LLM 可能输出 Schema 未定义的字段，serde 应忽略
    let json = r#"{
        "name":"test","loop":true,
        "steps":[{"action":"jump","duration_ms":300}],
        "mood":"happy","composer":"claude"
    }"#;
    let def: DanceDef = serde_json::from_str(json).unwrap();
    assert_eq!(def.name, "test");
    assert_eq!(def.steps.len(), 1);
}
```

### 10.3 Wiremock 集成测试（模拟 Anthropic API）

```rust
#[tokio::test]
async fn test_generate_dance_returns_valid_struct() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    let server = MockServer::start().await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();

    // Mock Anthropic API 返回符合 DanceDef schema 的 JSON
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body(r#"{
            "content":[{"type":"text","text":"{\"name\":\"ai_dance\",\"loop\":true,\"steps\":[{\"action\":\"jump\",\"duration_ms\":300},{action\":\"shake\",\"duration_ms\":400}]}"}],
            "usage":{"input_tokens":100,"output_tokens":50}
        }"#))
        .mount(&server)
        .await;

    // 验证 PetAgent::generate_dance() 正确反序列化
    // （需要构建指向 mock server 的 agent）
}
```

### 10.4 意图分类测试

```rstest
#[case("跳个舞！", CreateDance)]
#[case("给我表演个舞蹈", CreateDance)]
#[case("来局贪吃蛇", CreateGame)]
#[case("现在几点了", Chat)]
#[case("你好", Chat)]
fn test_classify_intent(#[case] msg: &str, #[case] expected: Intent) {
    assert_eq!(classify_intent(msg), expected);
}
```

### 10.5 兜底路径测试

```rust
#[test]
fn fallback_choreograph_produces_valid_dance_def() {
    for mood in &["happy", "sleepy", "angry", "cute", "unknown_xyz"] {
        let steps = choreograph(mood);
        assert!(!steps.is_empty(), "mood={mood} 应产生非空步骤");
        // 验证每个 step 都能正确序列化为 YAML
        let def = DanceDef {
            name: format!("test_{mood}"),
            loop_: true,
            steps: steps.clone(),
        };
        let yaml = serde_yaml::to_string(&def).unwrap();
        let roundtrip: DanceDef = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(roundtrip.steps.len(), steps.len());
    }
}
```

---

## 十一、风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| LLM 输出不符合 Schema | `DeserializationError` | Anthropic strict mode 概率极低；兜底查表保证不崩溃 |
| `prompt_typed` 不支持流式 | 用户看不到"思考过程" | 用 `show_static_bubble("正在编排...")` 显示等待状态 |
| 意图分类漏判 | 舞蹈请求走了普通对话 | 关键词列表持续积累；v2 升级到 AI 分类 |
| Token 消耗增加 | 每次舞蹈生成是一次独立 completion | 舞蹈不是高频操作；可考虑缓存热门结果 |
| Memory 上下文不自动注入 | AI 不知道之前的对话 | 手动拼接 `build_context()` 到 prompt |
