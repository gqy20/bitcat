# 更新日志

项目使用带 `v` 前缀的语义化版本标签，格式参考 [Keep a Changelog](https://keepachangelog.com/)。

## [Unreleased]

---

## [0.1.4] - 2026-05-16

Agent Watch 接入、小游戏模式扩展、宠物外部资产加载和发布前工具链修复，准备作为 0.1.3 后的桌宠协作与可玩性增强版本发布。

### 新增

- **Agent Watch 会话模型**：新增 `AgentSession`、`AgentNudge`、Claude Code 事件解析和 app settings 监控配置，为桌宠看管长任务提供核心状态层。
- **Claude Code hook 监控**：新增 Claude Code 只读 hook 安装、事件转发和设置页入口，hook payload 会进入本地 Agent Watch TCP monitor。
- **Agent Watch 浮动任务栈**：新增独立 `agent-watch` 窗口、前端 HUD、capabilities 权限和窗口生命周期管理，能够展示当前任务状态、hook 事件、运行中命令和离开提醒。
- **Codex hook 监控**：新增 Codex hook 安装与事件转发，和 Claude Code hook 共用 Agent Watch TCP monitor 与浮动任务栈。
- **Hook doctor 修复**：新增 hook 配置诊断与修复逻辑，可清理旧的 ai-pad hook、重建脚本并恢复缺失事件。
- **Agent Watch 设置页集成**：设置窗口新增 Claude / Codex hook 状态、安装/修复控制和 Agent Watch 打开入口。
- **宠物外部资产 fixture loader**：新增 spritesheet fixture loader、默认猫 fixture、导出脚本和前端测试，支持通过 manifest + sprites.png 载入宠物资源并覆盖默认帧。
- **小游戏 Memory 模式**：新增翻牌配对模式，复用 `GameDef`、`game` 窗口、ActionBus 和手柄/键盘输入通道。
- **小游戏 Catch 模式**：新增接食物模式，复用同一套小游戏生命周期，并在 HUD 中显示接取进度和失误次数。
- **面板小游戏入口扩展**：弹出面板扩展到 3×4，新增“翻牌”和“接食物”入口，默认面板可直接启动 Snake / Memory / Catch / Battle。

### 修复

- 修复 Agent Watch HUD 控件和 hook 事件展示细节，使会话状态与只读 hook payload 更稳定。
- 修复 Claude hook schema 兼容问题，避免监控事件解析失败。
- 修复 Agent Watch hook 事件中 source、session 和命令字段的兼容展示问题。
- 修复小游戏默认 Snake 太快结束的问题，默认胜利长度从 20 提高到 80。
- 修复新版 clippy 在发布前检查中暴露的 workspace warning，包括 `Path` 参数、冷却判断和整数倍数判断。

### 变更

- Agent Watch 会话看管从规划文档进入可运行路径，设置页、浮窗和 hook 安装器共享后端状态。
- Claude / Codex hook 脚本均采用 ai-pad marker 管理，便于重复安装、修复和人工排查。
- Snake `rules.win_length` 校验上限从 200 提高到 500，允许更长的小游戏局。
- `MinigameType` 扩展为 `snake` / `memory` / `catch` / `battle`，前端 `createEngine` 改为注册式分发，后续新增模式更容易复用同一窗口生命周期。
- 面板配置默认尺寸从 480×420 调整为 480×520，以容纳更多内置入口。
- pre-commit hook 改为检查整个 workspace 的 Rust 格式，减少发布前才发现格式漂移的概率。
- Release workflow 不再保存测试 cache，避免发布构建上传不必要缓存。

### 工具链

- 更新 core / app / xtask / Tauri 版本号到 0.1.4。
- 新增 Agent Watch、Claude hook、Codex hook 和 pet asset loader 相关前端/后端测试覆盖。
- 更新面板动作测试，覆盖新增 `memory` / `catch` 入口和 3×4 布局。
- pre-push 验证通过 `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings` 和 `cargo nextest run`。

### 文档

- 新增 Agent Watch hook 用户指南，说明 Claude Code / Codex hook 安装、修复和排障。
- 新增 Codex monitoring plan、Bongo Cat 竞品分析、Steam 评论采集脚本和评论 JSONL 研究材料。
- 更新 Claude Code Agent Watch 计划、pet spritesheet manifest 计划、roadmap 和配置指南，反映 0.1.4 的实际落地范围。
- 更新 README 的面板、小游戏、通信架构、线程模型、技术栈和发布说明，反映 0.1.4 的 Agent Watch 与小游戏扩展。
- 更新 CHANGELOG 供 GitHub Release workflow 按 `v0.1.4` 自动抽取发布说明。

---

## [0.1.3] - 2026-05-15

AI 语义事件、长期记忆、音乐响应舞动、守护战斗模式和宠物动画表现更新，准备作为 0.1.2 后的桌宠智能表现版本发布。

### 新增

- **宠物语义事件**：新增 tagged `PetEvent` 协议，主链路从旧的裸视觉 state 迁移到 `Notify` / `React` / `SetMode` / `WalkTo` / `ShowBubble` / `PlayDance` 等语义事件。
- **Rig 生命周期状态**：对话流新增 `AiWriting`、`ToolPreparing`、`ToolRunning`、`ToolBlocked`、`ToolFailed` 等通知，宠物动画可跟随模型写作和工具执行状态变化。
- **AgentReaction 收尾**：对话结束后用结构化输出生成最终情绪、speech 和长期记忆候选，不再依赖回复文本关键词猜测。
- **长期记忆工具**：新增 `search_memory` 与 `remember` 工具，长期记忆支持按文本、标签、来源、重要度进行 grep-first 检索。
- **长期记忆 JSONL 存储**：长期记忆主文件切换为 `~/.ai-pad/memory/long_term.jsonl`，一行一条记录，包含稳定 id、标签、重要度、来源和软删除字段。
- **宠物事件总线**：新增 `PetEventBus` 和 `MoodPolicy`，集中处理事件去重、低优先级节流、情绪 TTL、覆盖规则和最近事件日志。
- **事件诊断面板**：设置页新增宠物事件诊断，可查看最近 50 条事件的发送、去重、节流和失败原因。
- **记忆审查面板**：设置页新增长期记忆审查和删除入口，便于人工查看、清理和校正 AI 记住的内容。
- **资源诊断**：设置页新增进程内存和系统内存使用情况。
- **音乐响应舞动**：新增 fake source 与 WASAPI loopback 音乐舞动数据源，后端发送 energy / bass / onset / silence 帧，前端以 sprite-only 方式驱动宠物动作。
- **音乐舞动控制**：设置页、托盘菜单和宠物右键菜单新增音乐舞动启动/停止入口与诊断显示。
- **统一表演管线**：新增 performance session / timeline player / performer host，舞蹈和音乐响应动作复用统一播放宿主。
- **宠物右键菜单**：新增宠物窗口上下文菜单，并补充停止舞动、设置等常用入口。
- **小游戏守护战斗模式**：新增 Guardian Battle 模式、战斗输入热点、战斗事件结构化和战斗事件到宠物反应的映射。
- **面板游戏入口增强**：面板配置加入守护战斗入口，小游戏窗口和手柄输入支持战斗状态下的专用处理。
- **专注与准备动画帧**：前端新增 `focused` 和 `preparing` 精灵帧，用于专注观察和工具准备状态。
- **宠物 idle ambient variants**：前端 idle 状态新增耳朵动和左右看的偶发动画，长时间待机不再只循环基础眨眼。
- **宠物外部资产草案**：新增 spritesheet manifest 设计草案，规划 `manifest.json` + `sprites.png` 的外部宠物资产格式。

### 修复

- 修复面板 `launch` 入口在 Windows 上无法可靠启动 URL、目录或 shell 关联项的问题，改为走 Windows shell fallback。
- 修复音乐响应舞动停止与切换时的稳定性问题，避免旧 session 残留继续推送帧。
- 修复小游戏战斗模式下宠物窗口被错误覆盖或丢失状态的问题。
- 修复守护战斗输入热点捕获不稳定的问题。
- 修复战斗事件和宠物反应之间的映射遗漏，使胜负、受击和阶段变化能正确驱动宠物表现。
- 修复表演自然结束或被宠物硬事件中断后恢复语义状态不够即时的问题，恢复时会立刻重绘当前宠物帧。

### 变更

- AI 主链路移除旧 `SetState` 视觉事件路径，宠物前端统一消费 tagged 语义事件。
- 删除 `resolve_agent_response()` 关键词情绪推断和 `should_store()` 关键词记忆规则，相关判断改由结构化 `AgentReaction` 与 `memory_candidates` 驱动。
- `perform_dance` / `play_dance` 纳入统一 performance session，舞蹈播放期间的状态、停止和诊断更加一致。
- 设置页重构为更密集的控制台式布局，记忆、用量、资源和音乐诊断信息集中呈现。
- 气泡、宠物和设置页的工具/表演状态文案收敛为更稳定的生命周期表现。
- 前端宠物测试入口改用语义事件样例；`pet.js`、`sprite.js` 开始导出模块，`pet.html` / `test.html` 改用 module script。
- 宠物表演期间的事件优先级明确化：工具阻塞、工具失败、睡眠和退出会中断表演，普通 AI 写作/工具运行/情绪事件作为后台语义状态等待恢复。

### 性能

- 音乐响应舞动默认只做 canvas 内 sprite 动作，不高频移动真实桌面窗口，降低 WebView、设置页和右键菜单卡顿风险。
- 宠物事件总线对重复通知和低优先级情绪做去重/节流，减少前端无意义状态刷新。
- 设置页音乐诊断做低频刷新，避免音频帧高频重绘 DOM。

### 移除

- 移除旧的裸 `SetState` IPC 主路径和对应快照，保留明确动作类事件。
- 移除长期记忆关键词式 `should_store` 规则。
- 移除舞蹈播放中分散的旧动作会话逻辑，改由统一 performance 管线承接。

### 工具链

- 新增前端性能/舞动相关 Vitest 覆盖，验证 timeline dance、music reactive player 和宠物事件状态机。
- 前端宠物和精灵测试改为直接导入真实 `pet.js` / `sprite.js` 实现，避免测试复制旧状态机导致失真。
- 增加 `sysinfo` 等运行诊断依赖，用于设置页资源统计。

### 文档

- 新增宠物语义事件架构文档，说明 `PetEvent`、`PetEventBus`、`MoodPolicy`、AgentReaction 和前端状态优先级。
- 新增 Claude Code 桌宠看管计划，规划只读 hook、会话状态和后续权限控制路线。
- 新增音乐响应舞动调研与实现计划，梳理 WASAPI、fake source、舞感状态机和窗口摆动边界。
- 新增宠物动画视觉路线图和研究补充。
- 新增宠物 spritesheet manifest 计划，梳理外部宠物资产布局、schema、loader fallback、校验规则和迁移阶段。
- 重写 `docs/guide` 用户指南，使入门、配置、AI 对话、手柄、截图、舞蹈/音乐/小游戏和排障与当前代码一致。
- 梳理 `docs/plan`：新增计划索引，将 token tracking、日志规范、宠物语义事件、结构化输出和 rig capability roadmap 等已完成方案移入 `docs/plan/archive/`。
- 更新 README、roadmap、AGENTS.md、CLAUDE.md 和记忆取舍文档，补充长期记忆 JSONL、语义事件、音乐舞动和计划归档说明。

---

## [0.1.2] - 2026-05-14

面板配置化、多屏观察性能和桌宠位置/设置稳定性更新，准备作为 0.1.1 后的桌面体验修复版本发布。

### 新增

**弹出面板**
- 新增 `config/panel_action.yml`，弹出面板的窗口尺寸、网格行列、按钮数量、按钮图标、排序和启用状态均可通过 YAML 配置。
- 面板新增后端 ViewModel：前端通过 `cmd_get_panel_actions` 动态渲染按钮，不再硬编码 3×3 按钮列表。
- 面板动作支持 `launch`、`script` 和 `builtin` 三类，默认内置 VSCode、浏览器、资源管理器、PowerShell、记事本、跳舞、游戏、设置、聊天入口。

**截图观察**
- 多显示器截图分析改为按显示器独立分析和保存，bubble 会按显示器标签汇总结果。
- 多显示器 Vision 分析支持并行请求：多个可见显示器会并发调用 Vision API，再按原显示器顺序汇总。

### 修复

- 修复吸附条在恢复时缺少 `snap_edge` 会默认显示到左侧的问题；现在缺少方向时保持隐藏，等待 Rust 注入真实方向。
- 修复桌宠拖拽位置重启后不保留的问题；位置会写入 `app_settings.json`，启动时按当前显示器工作区校正后恢复。
- 修复 Win32 工作区获取失败时可能返回空矩形的问题，改为回退到 Tauri monitor 信息。
- 修复面板首次创建和再次显示大小不一致的问题；重设已有面板窗口尺寸时使用逻辑像素，避免高 DPI 屏幕下被压成半高。
- 修复 `app_settings.json` 并发保存时可能抢占同一个临时文件的问题，临时文件名改为进程/线程/时间戳唯一。
- 修复 `app_settings.json` 读写竞争导致偶发读到空文件或半写入文件的问题，读写路径统一加进程内锁。
- 修复 TTS 默认行为文档与代码不一致的问题；TTS 默认关闭，仅在设置中开启后朗读。
- 修复游戏窗口启动时焦点、桌宠位置和输入占用状态不稳定的问题。
- 修复截图观察在显示器空闲/游戏忙碌等状态下仍可能误触发的问题。

### 变更

- 面板按钮执行统一由后端处理，前端只负责渲染 ViewModel 和提交动作 id。
- `config/actions.yml` 回归只管理手柄/键盘动作，面板快捷入口独立到 `config/panel_action.yml`。
- 配置文件查找/保存路径解析抽到 `core::config`，`actions.yml`、`buttons.yml`、`panel_action.yml` 复用同一套规则。
- 截图观察从旧的多屏拼接说明调整为逐显示器分析存储，减少多屏拼接导致的小字不可读问题。

### 性能

- 多屏 Vision API 请求由串行改为并发，双屏场景下等待时间接近最慢单屏请求，而不是两个请求耗时相加。

### 移除

### 工具链

### 文档

- 更新 README：同步 0.1.2 的配置化面板、多屏并行截图、默认关闭 TTS、项目结构和配置说明。
- 更新配置和手柄指南：补充 `panel_action.yml`、面板按钮来源、TTS 默认关闭和 app settings 保存行为。
- 更新 AGENTS.md / CLAUDE.md，补充 `config/panel_action.yml` 为运行时配置文件。

---

## [0.1.1] - 2026-05-13

0.1.0 后的稳定性与体验更新，重点补齐用量观测、工具状态、手动截图、迷你游戏和 Windows 打包链路。

### 新增

**截图与观察**
- 新增手动截图入口：系统托盘、宠物左眼双击、`actions.yml` 的 `screenshot` 动作和 `keyboard_shortcut` 全局热键均可立即触发 Vision 分析。
- 默认 `R2` 绑定为立即截图，默认热键为 `CommandOrControl+Alt+S`。
- 截图记录升级为结构化存储，保存分析状态、摘要和上下文，方便后续检索与摘要注入。
- 多显示器截图按虚拟桌面坐标排序拼接，避免左右屏顺序错乱。

**小游戏**
- 新增可玩的 Snake 迷你游戏切片（`game` 窗口、`game_engine.js`、`GameDef` schema）。
- 面板可启动内置「毛线球大作战」，游戏中宠物进入 `GamePlay` 状态，结束后根据胜负切换表现。
- 新增小游戏配置校验，限制网格、速度、胜利长度和主题枚举，防止异常配置卡死或越界。

**Token 与工具观测**
- AI 调用开始记录 token 用量到 `~/.ai-pad/logs/token_usage.jsonl`，并聚合最近会话到 `token_sessions.json`。
- 设置窗口新增「用量」视图，展示今日汇总、最近 session、Chat/Vision/ScreenSummary/MemoryAggregation 分类统计。
- Agent 工具调用新增运行时事件：计划、完成、失败、被阻止等状态可推送到气泡窗口。
- 工具事件审计日志写入 `~/.ai-pad/logs/tool_events.jsonl`，便于排查工具链行为。

**气泡与交互**
- 气泡窗口支持手动拖拽调整大小，并保留用户手动尺寸偏好。
- 气泡可在 AI 调用工具期间展示工具状态，性能类工具（如舞蹈）会有阶段提示。
- 系统托盘右键菜单文案优化，截图、折叠、置顶、设置等入口更清晰。

**设置与外观**
- 设置窗口进行视觉打磨，新增本地字体资源，信息层级和表单布局更精致。
- 设置窗口动作编辑器支持 `screenshot` 动作类型和键盘热键字段。

### 修复

- 修复气泡手动 resize 后定位和尺寸偏好丢失的问题。
- 修复气泡在工具调用期间过早结束流式状态的问题。
- 修复气泡在多显示器环境中的放置和宠物遮挡问题。
- 修复贴边吸附只支持左右边的问题，扩展为四边吸附。
- 修复默认 TTS 行为：默认关闭，并尊重 `tts_enabled` 设置。
- 修复舞蹈 repeat 步骤未被正确播放的问题，并放大默认舞蹈动作幅度。
- 修复 Vision 结构化状态的 JsonSchema 生成问题，避免 `oneOf` 兼容性风险。
- 修复无 API key 的配置测试在 CI 中失败的问题，改为优雅跳过。
- 修复 pre-commit hook 对未暂存 Rust 文件也跑 fmt 的问题。

### 变更

- ActionBus 归一化三路输入，手柄、面板、快捷键等动作走统一分发路径。
- Agent 工具参数 schema 改为由类型派生，减少手写 JSON schema 样板。
- Vision、屏幕摘要、记忆画像改为走 rig extractor 的结构化输出路径，并移除旧 extractor fallback。
- 宠物动画引擎支持非均匀帧时长、瞬态 repeat 和 fallback 状态。
- 日志输出进一步规范化，降低聊天、截图、ActionBus 等高频路径噪声。
- `play_dance` / `perform_dance` 的 schema 文案收敛，减少 prompt token 占用。

### 工具链

- portable zip 打包统一迁移到 `xtask package-portable`，`make dist` / `make dist-upx` / Release workflow 共用同一条 Rust 路径。
- `make test-core` / `make test-fast` / `make test-app` / `make test` 统一委托 `xtask`，兼容 PowerShell/cmd/Git Bash。
- `make run` 标准化，日常运行优先通过 Makefile。
- 新增/更新 cargo-husky、nextest、提交信息规范和 Windows SDL2 构建说明。

### 文档

- 新增用户指南：AI 对话、配置、舞蹈、手柄、入门、截图、排障。
- 新增 grep-first 记忆检索取舍说明，明确不把 Embeddings / Vector RAG 作为当前主线。
- 新增 structured vision cleanup、rig capability roadmap、tool runtime、动画优化和小游戏计划文档。
- 新增 Steam 桌宠竞品分析、代码审查报告、临时产物目录约定。
- 为 core/app 源文件补齐模块文档和公共 API 注释。

---

## [0.1.0] - 2026-05-13

首个公开版本。

### 新增

**舞蹈系统**
- AI 工具调用生成舞蹈定义（`create_dance` tool）+ YML 持久化到 `~/.ai-pad/dances/`
- 前端 dancePlayer 播放器：按时间轴切换 sprite 动作帧（jump/spin/wave/shake/idle），支持循环和时长参数
- 手柄 Y 键 / Panel 按钮直接触发舞蹈，播放期间自动暂停截图管线
- 窗口级动画（screen-ratio 相对坐标，不再依赖绝对像素）

**记忆与画像**
- 两层存储记忆系统：短期滚动窗口（默认 20 条）+ 长期 AI 聚合画像（`~/.ai-pad/memory/chat_summary.json`）
- 用户画像配置 `config/user.yml`：显式声明 name/role/preferences，优先于自动聚合结果
- 截图原始分析记录注入 prompt（最近 10 条），屏幕活动摘要定时总结并注入上下文

**设置系统**
- 独立设置窗口（Tauri 多窗口）：按键绑定编辑、按钮目录展示、app_settings 运行时覆盖层
- 按键绑定支持未绑定状态可视化，button_catalog 展示完整手柄按钮集
- 配置文件嵌入 exe 二进制 + 多路径查找（exe 同目录 → CWD → 项目根目录）

**语音合成**
- Windows SAPI TTS 语音合成，AI 回复完成后自动朗读

**吸附增强**
- 吸附预览（拖近边缘时实时预览竖条形态）+ Crossfade 平滑过渡动画
- snap_edge 状态同步：吸附/展开/折叠状态在多窗口间一致
- 多层发光效果 + hover 热区扩展 + 品牌色统一

**AI 对话改进**
- 对话消费从手柄循环解耦为独立 `chat_loop` 线程，不再阻塞 80ms tick
- 输入焦点检测（`cmd_enter_chat` / `cmd_exit_chat`）：聊天期间自动锁定截图管线
- Agent max_turns=16 启用多轮工具调用

**工具系统扩展**
- 扩展至 9 个工具：新增 clipboard / foreground / recent_screenshots / create_dance / play_dance
- PermissionHook 安全加固：危险操作（shell/launch/hotkey）需用户确认

**可观测性**
- Agent tracing span：完整记录工具调用链路（输入参数 → 执行 → 输出结果）
- 结构化日志规范（`.claude/rules/logging.md`）：五级定义 + 大文本截断规则 + instrument 指南

### 修复

- **光标残留系列**：用 streaming 标志位彻底消除拖拽后光标残留（4 次递进修复）
- **气泡显示**：第二次不显示（eval 替代 emit_to）、首次创建文本丢失（init 先消费 pending_text）、自动隐藏延长至 15s
- **截图气泡链路**：首次显示文本丢失、WM_MOUSEWHEEL 转发（Win32 Subclass）、键盘滚动兜底
- **语音输入**：文本累积修复（generation 防残留机制）
- **拖拽坐标**：DPI 缩放修正
- **测试期望**：3 个本地测试用例断言值修正
- **Clippy**：全 workspace clippy warning 清零（unblock pre-push）

### 变更

- **配置重构**：yml 文件归入 `config/` 子目录；prompts 全部消除硬编码默认值，单一数据源来自 YAML
- **代码拆分**：lib.rs 和 screenshot.rs 大文件拆分（snap.rs / screenshot_tests.rs 独立）
- **气泡 UI**：CSS 精简 + JS 交互增强（Markdown 渲染 / 滚动 / 点击交互）

### 工具链

- **Git Hooks**：cargo-husky 接入 — pre-commit 做 fmt 检查，pre-push 做 clippy + test（~30s）
- **CI/CD**：GitHub Actions release 工作流 — workflow_dispatch 手动触发、并发控制、checksum、CHANGELOG 自动解析、Tauri 多平台打包
- **测试体系**：insta 快照 / rstest 参数化 / wiremock mock / proptest 属性测试 / Vitest 前端（293 tests passed）
- **Makefile**：重构为统一构建入口（build/release/test-fast/test-core/test-app）

### 文档

- **Roadmap**：重组为 Track A/B/C/D 四轨战略总览，覆盖所有 plan 文档方向
- **设计文档**：
  - 结构化输出设计（rig TypedPrompt 路径：DanceDef / GameDef）
  - Rig 能力升级路线图（Extractor / dynamic_tools / Embeddings RAG）
  - Token 全链路追踪方案
  - 日志体系规范化设计
  - 3D 体素架构方案（Three.js + 组合式部位模型）
  - Prompt Token Budget 分析
  - Vision API 设计文档
- **GDD 系列**：世界观、战斗系统、玩家角色、评分体系、数据架构、社区体系、Hub World 训练场
