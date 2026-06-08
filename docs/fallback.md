# Fallback 机制盘点

本文件整理当前项目中已经存在的 fallback / 回退 / 兜底路径，便于后续做稳定性分析、产品决策和回归测试。这里的 fallback 指“主路径失败、缺失或不适用时，系统仍继续运行并切换到替代行为”；普通错误返回不单独列入。

## 总览

| 领域 | 触发条件 | 当前 fallback | 主要入口 |
|------|----------|---------------|----------|
| 配置文件 | exe 同目录或 CWD 下配置缺失 | 使用编译期嵌入的默认 YAML | `core/src/config.rs`, `core/src/action.rs`, `core/src/prompts.rs`, `core/src/user_profile.rs` |
| 设置覆盖层 | `app_settings.json` 缺失、读取失败或解析失败 | 使用 `AppSettings::default()` | `core/src/app_settings.rs` |
| AI 配置 | base URL / model / max_tokens 未配置 | 使用 Anthropic 默认 URL、默认模型、256K 上限 | `core/src/ai_config.rs` |
| AI 流式对话 | 部分 provider 的 SSE/JSON 流异常但已有文本 | 返回已累积回复，不让气泡空白失败 | `core/src/agent.rs`, `app/src/gamepad.rs` |
| StepFun 工具流 | StepFun streaming tool parse 缺 `input` 且无累积文本 | 切到非流式 `chat()` 重试一次 | `core/src/agent.rs` |
| 工具连续失败 | 同一工具连续失败 3 次 | 停止继续调用工具，生成明确失败文案 | `core/src/agent.rs`, `app/src/gamepad.rs` |
| AgentReaction | 结构化收尾失败、超时或 panic | `Idle` mood + 主回复截断为 speech，不写长期记忆 | `core/src/agent_reaction.rs`, `app/src/gamepad.rs` |
| Reminder Personalizer | AI 文案为空或字段过长 | 回退提醒原始标题/正文并截断 | `core/src/reminder_personalizer.rs` |
| 截图观察 | 分析结果为空 | 显示“观察完成，但看不太清内容”提示 | `app/src/screenshot.rs` |
| 屏幕观察门控 | Win32 电源/锁屏信号不可靠 | 截图黑帧检测作为最后兜底 | `app/src/observation_gate.rs`, `app/src/screenshot.rs` |
| Agent Watch 通知 | 顶部通知窗口显示失败 | 回退到 bubble toast | `app/src/agent_monitor.rs` |
| 宠物动画 | 瞬态动画播放完成或 Walk 超时 | 回到 `Idle` | `core/src/pet.rs`, `app/frontend/js/pet.js` |
| 宠物资源 | 未配置资源包 | 使用内置 `cat-tabby` fixture | `app/frontend/js/sprite-loader.js` |
| 小游戏投影 | 真实隐私数据投影尚未接入 | 使用确定性的 demo projection | `core/src/game_projection.rs`, `app/src/game.rs` |
| 窗口/吸附定位 | Win32 工作区查询失败 | 使用 Tauri monitor，再退到 `1920x1080` | `app/src/snap.rs` |
| 语音输入 | WebView eval 取值失败或输入法延迟 | 依赖 input 事件状态；空值时短暂重试 | `app/src/voice.rs` |
| Bubble 滚动 | 透明窗口 native scroll 不稳定 | 手动处理 wheel 和键盘滚动 | `app/frontend/js/bubble.js` |
| 积分系统 | 事件或聚合状态写入失败 | 只记 warn，不中断主流程 | `core/src/points.rs` |

## 配置与默认值

### YAML 配置加载

`core/src/config.rs` 提供统一优先级：

1. exe 同目录下的配置路径。
2. 当前工作目录下的配置路径。
3. 编译期 `include_str!` 嵌入的默认内容。

覆盖范围包括 `actions.yml`、`buttons.yml`、`panel_action.yml`、`prompts.yml`、`user.yml` 等。这个 fallback 保证便携包缺少外部配置时仍能启动，但也可能掩盖“发布包漏拷贝 config”的问题。需要区分“用户主动无配置”和“打包错误”时，应看启动日志和 `make build` / `xtask copy-config` 路径。

### prompts.yml 解析失败

`PromptsConfig::load()` 在文件存在但 YAML 解析失败时会 warn 并使用默认提示词。优点是 AI 功能不因 prompt 文件损坏整体崩掉；风险是用户以为配置已生效，实际没有生效。

建议后续在设置页暴露“当前 prompts 来源 / 解析错误”诊断。

### app_settings.json

`AppSettings::load()` 对三类情况返回默认值：

- 无法解析系统配置目录。
- 文件不存在。
- 文件读取或 JSON 解析失败。

默认权限偏保守：shell/read_file/clipboard/foreground/launch/hotkey 等工具默认关闭，截图观察允许，摄像头观察关闭。这个 fallback 对 Steam / 发布安全比较友好，但解析失败会丢失用户设置，应继续保留 warn 日志并考虑在 UI 中提示。

### AI 配置

`AiConfig::load()` 分字段合并来源：

1. 设置页覆盖层。
2. exe 旁 `.env`。
3. `~/.claude/settings.json`。
4. 系统环境变量。
5. 内置默认值。

`api_key` 没有默认值，缺失时直接失败；`base_url` 默认 `https://api.anthropic.com`；`model` 默认 `claude-sonnet-4-20250514`；`max_tokens` 没配时按当前模型返回 256000。

## AI 与工具链

### 流式对话可恢复错误

`PetAgent::chat_stream()` 会把 rig stream 错误分类为 `RecoverableStream` 或 `Fatal`。以下错误会进入可恢复路径：

- SSE `[DONE]` 或 `failed to parse json` + `data:`。
- StepFun streaming tool parse 缺少 `input`。
- 未知错误但已经累积超过 20 个字符。

如果可恢复错误发生时已有累积文本，则直接返回 `Ok(accumulated)`，app 层继续 finalize bubble，避免用户看到空白失败。每次错误仍写结构化日志，包含 session、错误类别、累积字符数、chunk 数和工具调用数。

风险点：`unknown_with_content` 是体验优先的启发式，可能把真实异常包装成半截回复。分析 AI 质量问题时要看日志里的 `error_reason`。

### StepFun 非流式重试

当 StepFun provider 的 streaming tool parse 出错且没有任何累积文本时，`chat_stream()` 会调用非流式 `chat()` 重试一次。成功后把完整文本作为普通文本事件送给 bubble。

这是 provider 兼容性 fallback，不适合泛化成所有 provider 自动重试，因为对话工具可能有副作用。

### 工具连续失败停止

同一个工具连续失败达到 `MAX_CONSECUTIVE_TOOL_FAILURES = 3` 时，agent 主循环返回 `tool_failure_stop` 类型错误，app 层生成用户可读文案：

- `create_reminder`：明确告诉用户“提醒没有创建成功”。
- 其他工具：明确说明工具没有完成并附最后错误。

这个 fallback 的目标是防止模型反复调用同一个失败工具，尤其避免提醒未创建但口头承诺成功。

### AgentReaction 收尾

对话结束后，app 层会用 8 秒 timeout 调 `extract_agent_reaction()` 来提取 mood、speech 和长期记忆候选。失败、超时或 panic 时走 `fallback_agent_reaction()`：

- mood 固定 `Idle`。
- speech 使用主回复截断。
- 不产生长期记忆候选。

这个设计符合“语义判断交给模型，但失败不阻塞主回复”。代价是收尾失败时不会记录长期记忆。

## 提醒与通知

### AI 提醒润色

`ReminderNotificationCopy::sanitized()` 会在模型输出为空时回退到 `ReminderRecord` 的原始标题和正文，并限制标题 48 字、正文 120 字。AI 润色本身默认关闭；即使开启，调度器可靠性也不依赖模型。

### Agent Watch 通知

Agent Watch nudge 优先走顶部 notification window。如果 `show_notification()` 失败，则回退 `bubble::show_agent_toast()`；如果 bubble toast 也失败，只记录 warn。

### 截图观察提示

截图分析结果为空时，仍会增加待查看计数，并在允许弹窗时显示“观察完成，但看不太清内容，已放入待查看”。这比静默失败更友好，但也可能让用户误以为有实际分析内容；查看 inbox 或日志时需要区分空描述。

## 观察系统

### 屏幕观察门控

`observation_gate.rs` 记录 Windows 电源状态和会话锁定状态，截图线程用它决定是否跳过观察。模块文档明确说明：Win32 消息只是主信号，截图模块的黑帧检测仍是最后兜底。

建议分析截图误触发时同时看：

- gate 的 display/session 日志。
- 截图黑帧/mostly black 过滤日志。
- 多显示器可见 monitor 列表。

### 摄像头观察

摄像头观察默认关闭，且默认只保存结构化 JSON，不保存原始 JPEG。虽然不是失败 fallback，但属于重要的安全默认值。AI 或 UI 分析时不要假设摄像头路径可用。

## UI 与窗口兼容

### 宠物窗口状态 pull

前端启动时通过 `cmd_get_window_state` 拉取 collapsed / always-on-top / snap edge，替代脆弱的 Rust push emit。拉取失败时：

- 普通宠物窗口：`collapsed=false`，`alwaysOnTop=true`。
- 吸附竖条：默认无方向或 left 方向提示，仍可通过运行时 `window.__setSnapEdge` 修正。

这类 fallback 是为了解决 Tauri WebView 初始化时序问题。

### 吸附工作区

`snap.rs` 先尝试 Win32 `GetMonitorInfoW` 获取工作区；失败或工作区为空时回退 Tauri `current_monitor()`；再失败则使用 `(0,0,1920,1080)`。

风险点：最后一级固定尺寸在多屏、高 DPI 或非 16:9 环境可能定位不准。分析吸附错位时优先看 `get_work_area_for_window` 和 `get_work_area_for_position` 日志。

### 语音输入

释放语音输入时，后端先 eval WebView 读取 textarea 并清空。如果 eval 失败，则依赖前端 input 事件此前写入的共享状态。若 eval 成功但初次取值为空，会等待 300ms 再取一次，以兼容输入法延迟注入。generation 不匹配的旧文本会被丢弃。

### Bubble 滚动

透明 Tauri 窗口的 native scroll 不稳定，`bubble.js` 手动监听 wheel、方向键、PageUp/PageDown/Home/End 来控制 `scrollTop`。这属于交互兼容 fallback，出问题时看 DOM 是否拿到 `contentEl`，以及 wheel listener 是否挂载。

## 动画与资源

### 宠物状态 fallback

`core/src/pet.rs` 和前端 `pet.js` 都采用循环态与瞬态两类：

- `Talk` / `Happy` / `Confused` / `GameWin` / `GameLose` 播放指定 repeat 次数后回到 `Idle`。
- `Walk` 超过 3000ms 自动回到 `Idle`。
- 时间轴扫描异常时使用最后一帧或 0 帧。

这是避免瞬态动画卡死的核心兜底。

### 宠物资源包

`sprite-loader.js` 默认资源是 `/__fixtures__/pets/cat-tabby`。资源来源优先级大致为：

1. `window.__PET_ASSET_URL__`。
2. URL query `petAsset`。
3. sessionStorage / localStorage。
4. Tauri 设置页 `appearance.pet_asset_url`。
5. 默认 `cat-tabby`。

注意：当前加载指定资源包失败时，`initSpriteRenderer()` 没有显式再 fallback 到 `cat-tabby` 重试；它主要是在“未配置资源”时使用默认包。资源包损坏会更早暴露为加载错误，这是合理的 fail-fast。

## 游戏与数据投影

`cmd_get_game_projection()` 当前直接返回 `GameProjection::fallback()`，即一组确定性的 demo target：energy treat、memory shard、reminder note、agent task、focus snack。

这避免前端小游戏直接读取隐私数据，也让离线/demo 可玩。后续接入真实投影时要保持“先脱敏、限量、再给游戏”的边界，不要让小游戏拿到原始记忆、提醒、截图或 Agent 会话正文。

## Fire-and-forget 降级

部分附属系统故障不会打断主流程：

- 积分事件 JSONL 写入失败：只记 warn。
- 积分聚合状态更新失败：只记 warn。
- Agent Watch / reminder 等通知音或 toast 二级失败：只记 warn。
- 截图过期清理失败：当前主循环不因此失败。

这类 fallback 适合“奖励、诊断、清理、提示音”等非关键路径；不适合提醒创建、文件读取、shell 执行这类用户明确期待成功/失败反馈的路径。

## 分析建议

1. 把 fallback 分成“体验保护”和“错误掩盖”两类。AI 半截回复、bubble toast、动画回 idle 属于体验保护；配置解析失败用默认值则可能掩盖问题。
2. 对用户承诺型操作保持 fail-loud。提醒、shell、读文件、启动程序、热键这类操作失败时必须让模型或 UI 明确说未完成。
3. 对可恢复 AI 流错误保留结构化日志。后续比较 provider 稳定性时，重点看 `error_reason`、`accumulated_chars` 和是否进入 StepFun retry。
4. 对默认配置 fallback 增加来源可见性。设置页可以显示“当前配置来自 exe/config、CWD/config 还是内嵌默认”。
5. 对 UI/window fallback 保留环境信息。多屏、DPI、monitor rect、window label、location.href 是复盘吸附/渲染问题的关键上下文。
