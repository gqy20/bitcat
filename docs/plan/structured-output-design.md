# 结构化输出设计：舞蹈 & 游戏（Tool-native 结构化参数）

> **当前决策**：舞蹈内容生成采用普通对话中的 Tool Call，
> 让 LLM 在 `perform_dance` 的结构化参数中直接提交完整 `DanceDef`。
> 不做关键词意图匹配，也不额外发起一次 `prompt_typed` 分类/生成请求。
>
> **核心优势**：AI 真正拥有创作权——动作序列的长度、节奏、组合全部由 LLM 决定，而非硬编码查表。

## 当前实现状态（2026-05）

- ✅ `perform_dance` 已注册为主舞蹈工具：模型直接提交完整 `steps`，后端保存并立即播放。
- ✅ `create_dance(name, mood)` 已移除，不再向模型暴露 mood 查表工具。
- ✅ `choreograph()` 查表模板已移除，默认舞蹈改为 `config/dances/*.yaml` 内置预设。
- ✅ 跳舞期间 bubble 自动隐藏，截图管线跳过。
- ✅ `dance::validate_dance_def()` 统一限制名称、步数、单步时长、repeat 和总时长。

> 下文中关于 `prompt_typed::<DanceDef>()`、关键词意图分类、`choreograph()` 兜底的旧设计仅作为历史方案参考；当前 A1 主线以 `perform_dance` 为准。

---

## 一、背景与动机

### 旧实现的缺陷

旧版 `create_dance` 工具虽然注册在 AI Agent 上，但**实际编排逻辑是纯查表**：

```rust
// tools.rs:269 — execute_create_dance 的真实执行路径
let steps = crate::dance::choreograph(&args.mood);  // ← match 表，非 AI 生成
```

```
Roadmap 承诺:  AI 编排动作序列 → 生成 YML → 播放
旧版实际:     AI 传 (name, mood) → Rust 侧 mood→固定模板 → 固定序列
当前实际:     AI 调 perform_dance(name, steps...) → Rust 校验/保存/播放
```

`choreograph()` 是一个硬编码的 `match` 表（`dance.rs:122-158`），"happy" 永远返回同样的 5 步，"angry" 永远返回同样的 5 步。**AI 没有任何创作空间**。

### 为什么选择 Tool Call，而不是关键词分类 + TypedPrompt

| 维度 | Tool-native `perform_dance` | 关键词分类 + `prompt_typed` |
|------|---------------------------|---------------------------|
| 意图判断 | 交给模型原生工具选择 | 需要 Rust 侧匹配/分类 |
| API 调用次数 | 一次普通对话内完成 | 分类/生成可能额外调用 |
| 与现有流程关系 | 复用 `chat_stream` + Tool Call | 新增独立调用入口 |
| Schema 约束力 | Tool parameters 约束 + Rust 校验 | Provider output schema + Rust 校验 |
| 复杂度 | 低，贴合现有 agent | 中，需要双管线调度 |

**结论**：舞蹈请求属于模型擅长理解的简单任务，不应在 Rust 侧做关键词匹配。当前采用 `perform_dance`：模型负责判断和创作，Rust 负责强类型边界、校验、持久化和播放。

---

## 二、当前数据流

```
用户："跳个开心一点的舞"
  │
  ├─ PetAgent::chat_stream()
  │   └─ AgentBuilder 注册 perform_dance / play_dance 等工具
  │
  ├─ LLM 自行选择 perform_dance
  │   └─ args: { name, loop_, steps: [{ action, duration_ms, repeat }], loops?, duration_ms? }
  │
  ├─ tools::execute_perform_dance()
  │   ├─ DanceDef 组装
  │   ├─ dance::validate_dance_def()
  │   ├─ dance::save_dance() → ~/.ai-pad/dances/{name}.yaml
  │   └─ dance::request_play_dance()
  │
  └─ app dance bridge
      ├─ load_dance(): 用户目录优先，config/dances 内置预设兜底
      ├─ emit("play-dance")
      └─ 前端 dancePlayer 播放
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

### 3.2 GameDef（A2 Phase 1 已新建，core/src/minigame.rs）

> 当前状态（2026-05-13）：`core/src/minigame.rs` 已落地 Phase 1 版本，支持 `MinigameType::Snake`、`GameDef::default_snake()` 与 `validate_game_def()`。下方早期草案保留为结构化输出扩展参考；后续接 AI 工具时应以当前源码为准，而不是重新新建 `GameDef`。

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

> **当前结论：本节旧方案废弃。**
>
> 舞蹈请求不做 Rust 侧关键词匹配，也不额外调用模型做分类。普通对话 Agent 已注册 `perform_dance`，由模型自行决定何时调用工具。

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

> **当前结论：`choreograph()` 兜底已删除。**
>
> - AI 即兴舞蹈：`perform_dance` 参数无效时返回工具错误，让模型自行修正或解释。
> - 已保存舞蹈：`play_dance` 只播放用户目录或 `config/dances/` 中存在的 YAML。
> - 默认/手柄舞蹈：依赖 `config/dances/happy_twist.yaml` 等内置预设。

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
| **MemoryStore** | 自动沿用 | `perform_dance` 发生在普通对话工具调用中，沿用现有 `chat_stream` 上下文 |
| **config/prompts.yml (preamble)** | 自动生效 | Agent 的 preamble 作为系统提示词发送，AI 知道自己是 8Bit Cat |
| **create_dance Tool** | 已移除 | 不再保留 mood 查表兼容工具 |
| **choreograph() 查表** | 已移除 | 内置预设改为 `config/dances/*.yaml` |
| **前端 dancePlayer** | 不变 | 无论 DanceDef 来源（AI 生成 or 查表），前端播放机制相同 |
| **chat_stream 流式管道** | 不变 | 普通对话完全不受影响，两条管道并行 |

---

## 九、实施步骤

### Phase 1：舞蹈（已完成）

| 步骤 | 文件 | 改动 | 行数 |
|------|------|------|------|
| 1 | `core/src/tools.rs` | 新增 `perform_dance` 参数结构和执行函数 | 完成 |
| 2 | `core/src/agent.rs` | 注册 `PerformDanceTool`，移除 `CreateDanceTool` | 完成 |
| 3 | `core/src/dance.rs` | 新增 `validate_dance_def()`，删除 `choreograph()` | 完成 |
| 4 | `config/dances/` | 内置默认舞蹈 YAML | 完成 |
| 5 | `app/src/lib.rs` / `bubble.rs` | 跳舞期间隐藏气泡并阻止气泡重新显示 | 完成 |
| 6 | 前端 | `dancePlayer` + `jump/spin/wave/shake` 动作帧 | 完成 |
| 7 | 测试 | `perform_dance`、加载内置 YAML、舞蹈校验 | 完成 |

**当前状态：A1 已进入可用状态，后续只需继续调 prompt/schema 文案和真实交互体验。**

### Phase 2：游戏（部分完成，继续复用同样模式）

> 已完成：`core/src/minigame.rs`、`app/src/game.rs`、独立 `game` 窗口、Snake 前端、3×3 面板入口、手柄输入独占和宠物结果状态联动。
> 待完成：AI 工具、Memory/Catch、持久化游戏配置与分数。

| 步骤 | 文件 | 改动 | 行数 |
|------|------|------|------|
| 1 | `core/src/minigame.rs` | `GameDef` + Snake bounds 校验 | 已完成 |
| 2 | `core/src/lib.rs` | `pub mod minigame;` | 已完成 |
| 3 | `app/src/game.rs` | 动态 `game` 窗口 + IPC + 生命周期 | 已完成 |
| 4 | `app/src/gamepad.rs` | 游戏激活时 D-pad/A/B/Start 独占转发 | 已完成 |
| 5 | `app/frontend/js/game_engine.js` | Snake 引擎 + Canvas 2D 渲染 | 已完成 |
| 6 | `app/frontend/js/panel.js` / `panel.css` | 3×3 面板 + 游戏入口 | 已完成 |
| 7 | `core/src/agent.rs` / `tools.rs` | 新增 `perform_game` / `play_game` 工具 | 待做 |
| 8 | `game_engine.js` | Memory + Catch 引擎注册 | 待做 |
| 9 | `config/minigames.yml` | 默认配置与难度预设 | 待做 |
| 10 | 测试 | AI 工具、Memory/Catch、持久化分数 | 待做 |

**当前 A2 Phase 1：已提交 `a2105ff`；下一步是 AI 工具与多游戏体系。**

### Phase 3：后续增强

- **工具参数体验**：继续优化 `perform_dance` description，让模型更稳定地产生短而有节奏的舞蹈
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
| LLM 输出非法 steps | `perform_dance` 返回工具错误 | `validate_dance_def()` 给出明确错误，让模型可自我修正 |
| 模型没有调用舞蹈工具 | 舞蹈请求走了普通对话 | 优化 preamble 和 tool description，不做关键词匹配 |
| Token 消耗增加 | 每次舞蹈生成是一次独立 completion | 舞蹈不是高频操作；可考虑缓存热门结果 |
| Memory 上下文不自动注入 | AI 不知道之前的对话 | 手动拼接 `build_context()` 到 prompt |
