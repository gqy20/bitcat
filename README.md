# BitCat

BitCat is a Windows desktop AI companion that lives at the edge of your screen. It combines streaming chat, deterministic reminders, Agent Watch, screen and camera observation, voice input, optional TTS, music-reactive performance, edge docking, quick actions, and mini games.

Built with Tauri 2.0 + Rust + SDL2 as a single exe with no Node.js runtime dependency. The project has 400+ Rust tests, 15 frontend test files, and cargo-husky hooks for fmt, clippy, and fast tests.

## 快速开始

```bash
# 开发运行（推荐；Makefile 会设置 Windows SDL2 所需环境变量）
make run

# Release 构建（体积优化：opt-level=z + LTO + strip）
make release

# 打包为版本化 ZIP
make dist

# 发布 0.1.6 时使用 tag 驱动产物命名
git tag v0.1.6
git push origin v0.1.6
```

启动后看到屏幕角落的小宠物即为成功。

## 功能概览

| 触发 | 行为 |
|------|------|
| 手柄 Start | AI 对话（流式回复，猫咪根据回复自动切换状态/气泡） |
| 手柄 Home / `Ctrl+Alt+Space` | 弹出/隐藏 Quicker 风格面板 |
| 手柄方向键（dpad） | 桌面滚动（隐藏时）/ 面板选择（弹出时） |
| 手柄 A / Enter | 面板确认（弹出时）；夸奖猫咪（隐藏时） |
| 手柄 B / Esc | 关闭面板（弹出时）；随机走动（隐藏时） |
| 手柄 Select | 切换睡眠/唤醒 |
| 手柄 Y（按住） | 语音输入 → 显示录音条 → 识别文字 → 送 AI |
| 手柄 Y（短按）/ 面板按钮 | 触发舞蹈播放（AI 编排或已保存舞步） |
| 手柄 L1/R1/L2 | 按 `config/actions.yml` 绑定执行（hotkey/launch/voice/script/screenshot） |
| 手柄 R2 / `Ctrl+Alt+S` | 立即截图并用 Vision 分析当前屏幕 |
| 拖拽宠物到边缘 | 贴边吸附，变精致发光竖条（弹簧缓动动画） |
| 点击猫咪嘴巴 | 打开聊天输入框，键盘输入文字送 AI |
| 双击猫咪左眼 | 立即截图并显示 Vision 分析结果 |
| 点击/拖拽宠物 | 播放语义短动作反馈（观察、确认、阻塞、拖拽），不打断当前工作状态 |
| AI 回复后 | 如果设置中开启 TTS，则自动朗读 |
| 系统托盘右键 | 截图/折叠/置顶/**设置窗口**/重载配置/退出 |
| 后台（每 30s） | 截图 + Vision API 分析 + 屏幕活动摘要 |
| 设置开启摄像头观察后 | 隐藏 WebView 低频采样摄像头帧，保存保守 Vision 观察 |
| AI 对话中 | 自动暂停截图管线（避免浪费 Vision API 调用） |

## 弹出面板

弹出面板由 `config/panel_action.yml` 驱动，默认 480×360、2×2 玻璃风格网格。窗口尺寸、网格行列、按钮数量、图标、排序、启用状态和动作类型都可以通过 YAML 修改。

默认按钮现在只保留 4 个小游戏入口：毛线球大作战 / 翻牌配对 / 接食物 / 飞机守护战。VSCode、浏览器、设置、聊天等非小游戏入口可按需手动加回 YAML。

- 弹出后窗口居中、置顶、跳过任务栏、失焦自动隐藏
- 选中项用蓝色光晕高亮，鼠标悬停或方向键移动都会同步选中
- 按下 A / Enter 执行动作后面板自动关闭
- 后端的 `panel-nav` / `panel-confirm` / `panel-close` 事件由 Rust 主循环根据手柄状态发送
- 前端通过 `cmd_get_panel_actions` 获取 ViewModel，不再硬编码按钮列表
- `type: launch` 启动程序，`type: script` 执行 PowerShell 命令，`type: builtin` 调用内置入口（dance/game/memory/catch/battle/settings/chat）

## AI 流式回复 & 气泡

按 Start 键触发 AI 对话时：

1. 后端调用 `agent.chat_stream()` 开始流式生成
2. 同时打开独立气泡窗口，定位在宠物上方
3. 每收到文本 chunk 通过 `bubble-chunk` 事件实时推送到前端渲染
4. 流结束后发送 `bubble-end` 事件，前端启动自动隐藏定时器；如果设置中开启 TTS，则异步朗读回复
5. 前端初始化时通过 `cmd_consume_bubble_text` invoke 拉取已有文本（解决 emit 早于 listen 的竞态）

气泡支持 Markdown 渲染（marked.js）、毛玻璃效果、动画光标、聊天输入框模式、手动拖拽调整大小，并会在 AI 调用工具时显示计划/完成/失败/被阻止等运行状态。

## 语音输入

按住 Y 键（或 config/actions.yml 中配置的 voice 类型按键）：

1. 显示录音条窗口（280×40），移到屏幕中下方，强制前台化（`AttachThreadInput`）
2. 模拟用户配置的输入法语音热键（如 Ctrl+Win）
3. 松开按键后等待识别引擎注入文字到 textarea（700ms）
4. 通过前后端握手协议同步文本：
   - 后端 emit `voice-flush` → 前端 invoke `cmd_voice_update_text` → 前端 emit `voice-ready` → 后端 mpsc channel 收到继续（3s 超时兜底）
5. 取到的文本直接送 AI 流式对话，结果同样走气泡显示

## 截图观察

独立后台线程定时截图（默认 30s，可配置），流程：

1. BitBlt 捕获屏幕 + dHash 感知哈希去重 + 熄屏检测（`SM_MONITORISOFF` + 全黑帧采样）
2. 缩放 → JPEG 编码 → Vision API（Anthropic Messages）分析
3. 结果通过气泡显示，并保存到 `~/.bitcat/screenshots/`（7 天自动清理）
4. 屏幕活动摘要定时总结，默认取最近 30 条截图分析生成摘要，最多 20 条摘要注入 AI prompt
5. **聊天输入聚焦时自动暂停截图**（避免浪费 Vision API 调用）
6. 舞蹈播放期间同样暂停截图
7. 多显示器会按单个显示器独立分析和保存，多个可见显示器的 Vision API 请求会并行执行，再按显示器顺序汇总到气泡

手动截图入口：

- 系统托盘右键「立即截图」
- 手柄 R2（默认配置）
- 全局热键 `Ctrl+Alt+S`（来自 `actions.yml` 的 `keyboard_shortcut`）
- 双击宠物左眼

## 摄像头观察

摄像头观察默认关闭，可在设置页“外观与行为”中开启。开启后，应用会预创建隐藏的 `camera` WebView，由前端 `getUserMedia` 获取权限并按截图间隔低频采样，后端只接收 JPEG data URL、做节流和 Vision 分析。

- 提示词位于 `config/prompts.yml` 的 `camera.prompt`，严格要求不做人脸身份识别，不推断敏感属性。
- 观察记录保存到 `~/.bitcat/camera/YYYY-MM-DD/`；默认只保存分析 JSON，勾选“保存摄像头帧”后才保存帧图片。
- 采样会避让 AI 对话、舞蹈、游戏等忙碌状态，避免打断主要交互。
- 最近摄像头观察可作为上下文注入 AI，用于“我是不是离开座位了”这类低风险状态提醒。

## 舞蹈系统

AI 可通过 `perform_dance` Tool 直接提交完整舞蹈编排，前端实时播放宠物动画 + 窗口级大幅度位移。

### 编排（AI 侧）

1. AI 在普通对话中自行判断用户是否需要表演舞蹈
2. AI 调用 `perform_dance` Tool，提交完整 `DanceDef`（`jump/spin/wave/shake/idle` + 时长 + repeat）
3. 后端校验舞蹈名称、步数、单步时长和总时长后，YAML 持久化到 `~/.bitcat/dances/`
4. 项目内置默认舞蹈位于 `config/dances/`，用户/AI 生成舞蹈优先从 `~/.bitcat/dances/` 读取

### 播放（前端）

1. AI 调用 `perform_dance` / `play_dance`，或手柄 Y 短按/面板按钮触发 → 后端发送 `play-dance` 事件
2. 前端舞蹈播放器逐帧渲染：
   - **精灵内动画**：jump 抛物线上移、spin 快速翻转、wave 浮动、shake 抖动
   - **窗口级移动**：基于屏幕百分比计算偏移量（跳跃上移 ~14% 屏幕高度，弹性缓动；旋转离心抖动等）
   - **结束归位**：250ms 平滑缓动回到基准位置
3. 支持 `loop_`（循环播放）和 `max_duration_ms`（硬上限）
4. 舞蹈期间自动暂停截图管线

## 贴边吸附

拖拽宠物到屏幕边缘时：

1. 磁性预告条提示松手会吸附的位置
2. 松手后宠物变为精致发光竖条（多层发光 + hover 展宽 + 品牌色）
3. 点击竖条恢复宠物形态
4. 气泡自动跟随吸附态定位
5. 双窗口 Crossfade 过渡（150ms CSS 动画）
6. 支持上/下/左/右四边吸附
7. 吸附条方向由 Rust 状态注入；缺少方向时保持隐藏，避免 fallback 到错误边缘

拖拽结束且没有吸附时，桌宠当前位置会保存到 `app_settings.json` 的 `appearance.pet_position`。下次启动会恢复该位置，并按当前显示器工作区校正，避免窗口跑到屏幕外。

## 迷你游戏

面板可启动 4 个内置玩法：

- **毛线球大作战**：Snake 玩法，默认 48×32 加密网格，圆润连续身体渲染；前 20 个食物优先刷在中间区域，按住 A / Space / Enter 可加速，BOOST 吃到食物得分更高。
- **翻牌配对**：Memory 配对玩法，方向键移动光标，A / Enter 翻牌；记录 flips / moves / misses，最终得分包含匹配分、combo 和通关效率奖励，翻牌越少分数越高。
- **接食物**：Catch 玩法，半宽赛道、篮子造型和接住弹字反馈；连续接住会提高 combo 得分，漏掉会扣分、断 combo，累计失误 5 次失败。
- **飞机守护战**：原守护战已改为飞行射击玩法，按住方向可持续移动，A / Space 发射，X/Y 技能，L1 防护；击杀有 combo、爆炸粒子和目标进度，漏怪/碰撞会扣生命值并扣分。

1. 后端创建透明置顶 `game` 窗口并读取 `GameDef`
2. 前端 `game_engine.js` 按 `game_type` 创建 Snake / Memory / Catch / Battle 引擎，运行网格逻辑、键盘/手柄方向输入和胜负判定
3. 游戏期间宠物进入 `GamePlay` 状态，结束后按 win / lose / cancel 切换表现
4. `GameDef` 会校验网格尺寸、速度、胜利长度、主题枚举和各模式专用边界，防止异常配置导致越界或卡死

当前面板入口会通过 `ActionBus::PlayGameDefault` / `PlayMemoryDefault` / `PlayCatchDefault` / `PlayBattleDefault` 启动内置模式；底层 `start_game(GameDef)` / `cmd_start_game_with_def` 已能启动指定配置。AI 层 `perform_game` / `play_game` 工具还未注册，下一步会复用这条通道。

## Agent Watch

Agent Watch 是桌宠侧的只读任务看管面板，用于观察 Claude Code / Codex 的长任务状态，不直接执行危险操作：

- Claude Code 与 Codex hook 会把会话事件包装成统一 envelope，转发给本地 Agent Watch TCP monitor。
- 浮动任务栈展示最近会话、当前状态、运行中命令、hook 事件和离开提醒。
- Hook doctor 可以清理旧 BitCat hook、重写 PowerShell 转发脚本，并补齐缺失事件。
- 监控窗口与普通桌宠窗口解耦，游戏运行或桌宠表演不会阻塞会话事件记录。
- 设置页提供远程 Mac/Linux 一键安装命令和只读 `/watch` 地址；地址发现已抽象为远程 endpoint，支持 LAN、Tailscale/tailnet、VPN 和其他可达地址，UI 默认脱敏显示，复制命令时使用完整地址并支持多地址重试。远程安装脚本会做一次自检上报，且远程看板和安装脚本可以分别关闭。

## 程序化提醒

Agent 可以调用提醒工具创建确定性的本地任务，例如“3 分钟后提醒我喝水”或“每小时提醒我休息”。提醒不依赖模型继续在线，到期后由本地调度器触发统一通知窗口。

- 提醒存储在系统数据目录下的 `bitcat/reminders/reminders.json`，Windows 通常是 `%APPDATA%/bitcat/reminders/reminders.json`。
- store 使用当前版本 JSON 数组格式；读失败会在设置页暴露，并写入结构化诊断，不静默迁移旧格式或半写入文件。
- 写入使用临时文件 + 原子替换，避免程序退出或机器睡眠时留下空文件。
- 到期提醒、完成、稍后、取消、删除都会写入 `~/.bitcat/logs/reminder_events.jsonl`。
- 设置页“提醒”Tab 可以刷新、完成、10 分钟后、取消或用垃圾桶删除记录；通知窗口里的动作也会回写 store 并刷新设置页。
- `create_reminder` 创建失败时，工具结果会明确告诉 Agent 提醒没有创建成功，避免只在对话里口头承诺。

## Steamworks 探针

应用启动时会非致命地尝试从 exe 同目录加载 `steam_api64.dll` 并调用 `SteamAPI_InitFlat`，用于验证 Steam 客户端、AppID 和 DLL 链路。缺少 DLL、未登录 Steam 或缺少 `steam_appid.txt` 只会写入 warn 日志，不影响普通非 Steam 构建运行；后续 Steam 成就、DLC 和商店能力会在这条诊断链路上扩展。

## 宠物资源包

桌宠渲染已进入 v2 manifest 资源包模式。默认内置 `piggy` 是 192×208 原始帧的高分辨率资源包，`cat` 也已升级为同规格的高分辨率 v2 资源包；设置页可切换 `status`、`core`、`stacky`、`bsod`、`null-signal` 等终端状态风资源，以及 `byte-bun`、`mossbot`、`moonbit`、`sparkle` 等角色资源包。

- 资源包位于 `app/frontend/__fixtures__/pets/<id>/manifest.json`
- `manifest.actions` 支持 timeline，可声明 `observe` / `nudge` / `acknowledge` / `blocked` / `dragging` 等语义短动作
- 设置页按“推荐 / 终端状态 / 角色 / 经典资源”分组展示资源包
- 配置了外部资源包时加载失败会直接暴露错误，不再静默回退到旧内置精灵

## 记忆系统（两层存储 + AI 画像）

### 第一层：短期对话记忆

- 滚动窗口默认不按条数淘汰（`memory.max_entries: 0`），由 `max_context_chars` 控制注入长度；每次 AI 对话后记录 user_msg + ai_reply（按字符截断）
- 持久化到 `~/.bitcat/memory/chat_summary.json`
- 每次对话注入 prompt：`[最近对话记录]...[/最近对话记录]`

### 第二层：长期记忆

- 对话结束后由 `AgentReaction.memory_candidates` 或 `remember` 工具写入长期记忆，Rust 只做 schema、长度、标签和重要度边界校验
- 支持按 text/tag/source/min_importance 做 grep-first 候选召回，最多返回 20 条交给模型判断语义相关性
- 持久化到 `~/.bitcat/memory/long_term.jsonl`，一行一条当前有效记录，包含稳定 `id` 和 `deleted` 软删除字段

### AI 聚合画像

- 定期调用 Anthropic API 将未聚合的长期记忆条目浓缩为用户画像
- 最大 400 字符，注入 prompt：`[关于主人]...[/关于主人]`
- 存储到 `~/.bitcat/memory/profile.json`
- 所有持久化使用原子写入（tempfile + rename）

## 设置窗口

系统托盘右键可打开独立设置窗口（1040×720），覆盖层配置机制：

- **AI 覆盖**：api_key / base_url / model / max_tokens → `app_settings.json`
- **动作绑定**：编辑 config/actions.yml 并实时重载
- **提示词配置**：编辑 config/prompts.yml 并实时重载
- **外观设置**：always_on_top / default_collapsed / tts_enabled / global_shortcut / screenshot_interval_sec / pet_position
- **摄像头观察**：camera_observation_enabled / camera_save_frames，采样间隔跟随截图间隔
- **Agent Watch**：本地/远程看板开关、远程安装脚本开关、离开提醒和 TTS 策略
- **提醒**：刷新、完成、10 分钟后、取消、删除本地提醒
- **Token 用量**：展示今日总量、最近 session、Chat/Vision/ScreenSummary/MemoryAggregation 分类统计
- 设计原则：`~/.claude/settings.json` 只读，所有用户修改通过覆盖层
- 支持按分类重置为默认值
- TTS 默认关闭；开启后 AI 回复完成时使用 Windows SAPI 本地朗读
- `app_settings.json` 保存使用唯一临时文件和进程内读写锁，避免高频位置保存时读到半写入文件

## 手柄配对

以 8BitDo Micro 为例：

1. 将手柄背面模式开关拨到 **D**（D-Input 模式）
2. 按住 Pair 按钮 1 秒，LED 快闪
3. Windows 蓝牙设置中搜索配对
4. 首次使用方向键需激活：**按住 Select + ↑ 五秒**

智能设备选择：自动过滤键鼠接收器等非手柄设备，优先识别 8BitDo/Xbox/DualSense 等已知手柄。支持热插拔自动重连。

## 项目结构

```
bitcat/
├── Cargo.toml              # workspace: members = ["core", "app", "xtask"], release LTO+strip
├── config/                 # 运行时配置目录（编译时嵌入 exe，exe 同目录可覆盖）
│   ├── actions.yml         # 按键动作绑定
│   ├── buttons.yml         # 硬件按键映射
│   ├── panel_action.yml    # 弹出面板布局、按钮展示和动作
│   ├── prompts.yml         # AI 提示词配置
│   └── user.yml            # 用户画像（名字/角色/偏好/语言）
├── core/                   # 纯逻辑库（无 UI 依赖）
│   └── src/
│       ├── lib.rs          # 模块入口
│       ├── agent.rs        # AI Agent (rig-core + Anthropic SDK), 内置 Tool, 流式 chat_stream, tracing span
│       ├── bridge.rs       # 手柄→AI→宠物桥接层, PetCommand IPC 协议, 按键映射
│       ├── ai_config.rs    # 从 ~/.claude/settings.json 或环境变量读取 API 配置
│       ├── action.rs       # 动作定义与加载（hotkey/launch/voice/script），配置嵌入 + 多路径查找
│       ├── app_settings.rs # 设置覆盖层存储（app_settings.json），只读→可写桥接
│       ├── config.rs       # 通用配置路径解析 + buttons.yml 加载
│       ├── panel_action.rs # 面板布局、按钮 ViewModel 和动作配置
│       ├── dance.rs        # 舞蹈定义、校验、用户/内置 YAML 双层加载
│       ├── prompts.rs      # AI 提示词配置（agent/vision/memory/screen_summary），配置嵌入
│       ├── device.rs       # SDL2 按键编号 → 名称映射
│       ├── hotkey.rs       # Win32 SendInput 键鼠模拟 + force_foreground
│       ├── pet.rs          # 宠物状态机（Idle/Walk/Sleep/Talk/Happy/Confused/Dance）
│       ├── agent_session.rs # Agent Watch 会话归一模型（Claude Code / Codex）
│       ├── agent_nudge.rs # Agent Watch 离开/等待/完成提醒策略
│       ├── camera_observation.rs # 摄像头观察记录存储 + 最近上下文构建
│       ├── memory.rs       # 两层记忆系统：短期滚动窗口 + 长期 JSONL 候选召回 + AI 聚合画像
│       ├── reminder.rs     # 程序化提醒 store、原子写入、生命周期操作与 JSONL 事件日志
│       ├── vision.rs       # Vision API 请求构建/响应解析
│       ├── screenshot.rs   # 截图类型定义、dHash、resize/JPEG、存储 + 7天清理
│       ├── screen_summary.rs # 屏幕活动摘要存储 + 最近 N 条截图注入 prompt
│       ├── minigame.rs     # 迷你游戏 GameDef schema、内置模式与校验
│       ├── tool_events.rs  # 工具运行时事件审计日志
│       ├── token_tracker.rs # Token 用量 JSONL + session 聚合
│       └── tools.rs        # AI Tool 实现（launch/shell/read_file/get_time/recent_screenshots/search_memory/remember/reminder/hotkey/clipboard/foreground/dance）
└── app/                    # Tauri 2.0 应用
    ├── tauri.conf.json     # 窗口、权限、withGlobalTauri
    ├── capabilities/
    ├── src/
    │   ├── main.rs         # 入口（--debug 控制台），日志双写初始化
    │   ├── lib.rs          # Tauri Builder + 全局状态/命令注册 + 后台循环启动
    │   ├── action_bus.rs   # 手柄/热键/前端动作归一分发
    │   ├── gamepad.rs      # PetEvent 序列化, PetCommand→前端事件转换, chat_loop 解耦
    │   ├── game.rs         # 迷你游戏窗口与生命周期管理
    │   ├── agent_monitor.rs # Agent Watch 会话状态与 hook 事件监控
    │   ├── commands.rs     # 共享状态 + Tauri command（snap_preview/crossfade/play_dance 等）
    │   ├── bubble.rs       # 独立气泡窗口, 流式 start/chunk/end 协议, bubble_follower 线程
    │   ├── camera.rs       # 隐藏摄像头窗口 + 摄像头帧 Vision 观察
    │   ├── notification_window.rs # Agent Watch 与提醒共用的灵动岛式通知窗口
    │   ├── reminder_scheduler.rs # 到期提醒轮询调度
    │   ├── voice.rs        # 语音输入窗口, 强制前台化, generation 防残留
    │   ├── panel.rs        # 弹出面板（YAML 布局, 方向键导航, 动作执行）
    │   ├── settings.rs     # 设置窗口后端命令（读/写 app_settings + yml 重载）
    │   ├── pet_inbox.rs    # 宠物 Inbox 窗口
    │   ├── audio_reactive.rs # fake/WASAPI 音乐响应表演数据源
    │   ├── remote_endpoint.rs # Agent Watch 远程地址发现与安装命令生成
    │   ├── screenshot.rs   # 截图线程（BitBlt + 熄屏检测 + Vision API + 聊天/舞蹈暂停）
    │   ├── steam.rs        # Steamworks 本地 DLL/AppID 探针，失败只写诊断日志
    │   ├── joystick.rs     # SDL2 手柄封装 + is_attached 热插拔检测
    │   ├── tts.rs          # Windows SAPI TTS 语音合成
    │   └── tray.rs         # 系统托盘（右键菜单 + 设置入口 + 重载配置）
    └── frontend/           # 静态 HTML/JS/CSS + Vitest 前端测试
        ├── pet.html        # 宠物窗口（128×128 透明, Canvas 像素精灵 + 粒子 + 舞蹈播放器）
        ├── bubble.html     # 气泡窗口（流式文本 + Markdown + 毛玻璃）
        ├── game.html       # 迷你游戏窗口（Snake / Memory / Catch / Battle）
        ├── agent_watch.html # Agent Watch 浮动任务栈窗口
        ├── notification.html # Agent Watch 与提醒共用通知窗口
        ├── camera.html     # 隐藏摄像头采样窗口
        ├── pet_inbox.html  # 宠物 Inbox 窗口
        ├── panel.html      # 面板窗口（尺寸/网格/按钮来自 panel_action.yml）
        ├── voice.html      # 语音输入条（280×40, textarea 接收输入法注入）
        ├── glow.html       # 吸附竖条（发光动画）
        ├── settings.html   # 设置窗口（1040×720, 分类 Tab, 实时预览）
        ├── css/            # pet.css / bubble.css / game.css / panel.css / glow.css / settings.css
        ├── js/             # app.js / bubble.js / game_engine.js / panel.js / voice.js / glow.js / settings.js / particles.js / sprite.js / pet.js
        ├── __tests__/      # Vitest 单元测试（15 个测试文件）
        └── vitest.config.ts
```

## 配置说明

所有配置编译时通过 `include_str!` 嵌入 exe，单文件即可运行。用户在 exe 同目录创建 `config/` 文件夹放入 yml 可覆盖默认配置。

查找顺序：**exe 同目录/config/ → CWD/config/ → 内置默认值**

### config/actions.yml — 按键动作

支持五种动作类型：

```yaml
defaults:
  terminal: powershell
  window: maximized

actions:
  # launch: 启动程序（可选在终端中）
  Start:
    type: launch
    program: claude
    args: "--dangerously-skip-permissions"
    workdir: "D:\\C\\Desktop\\ai"
    terminal: true

  # voice: 按住触发系统语音输入法快捷键
  Y:
    type: voice
    voice:
      trigger: ["ctrl", "win"]   # 输入法语音热键
      delay: 1.0

  # hotkey: 发送键盘组合键（支持 Alt/Ctrl+Tab 持续按住）
  L1:
    type: hotkey
    trigger: ["alt", "tab"]
  R1:
    type: hotkey
    trigger: ["alt", "backtick"]
  L2:
    type: hotkey
    trigger: ["ctrl", "tab"]

  # screenshot: 立即截图并用 Vision 分析当前屏幕
  R2:
    type: screenshot
    keyboard_shortcut: "CommandOrControl+Alt+S"

  # script: 执行 PowerShell 命令
  # A:
  #   type: script
  #   command: "python my_script.py"
```

注意：当面板可见时，A / B / dpad 由面板独占，不再触发 actions.yml 绑定。

### config/buttons.yml — 硬件映射

按键编号到名称的映射，每种手柄不同。8BitDo Micro D-Input 已实测填好；换手柄需要校准。

### config/panel_action.yml — 面板布局与按钮

面板配置同时描述展示和执行行为：

```yaml
defaults:
  terminal: powershell
  width: 480
  height: 520
  columns: 3
  rows: 4

actions:
  vscode:
    label: VSCode
    icon: "💻"
    order: 10
    type: launch
    program: code
    workdir: "D:\\C\\Desktop\\ai"
    terminal: false

  chat:
    label: 聊天
    icon: "💬"
    order: 90
    type: builtin
    command: chat
```

- `width` / `height` / `columns` / `rows` 控制窗口大小和网格布局
- `order` 控制排序；超出 `columns * rows` 的按钮不会显示
- `enabled: false` 可临时隐藏可执行状态（按钮保留但禁用）
- `type: builtin` 支持 `dance` / `game` / `memory` / `catch` / `battle` / `settings` / `chat`
- `type: launch` 和 `type: script` 用于外部程序和脚本快捷入口

### config/prompts.yml — AI 提示词

包含多段配置：
- `agent.preamble` — AI 人设（默认：BitCat 桌面 AI 伙伴）
- `vision.prompt` / `vision.prompt_multi` — 截图分析提示词（强调反幻觉）
- `camera.prompt` — 摄像头观察提示词（保守描述，不做人脸身份或敏感属性推断）
- `memory` / `memory_v2` — 短期记忆窗口、长期记忆检索和聚合参数
- `screen_summary` — 截图摘要聚合和注入条数（默认取 30 条原始分析，注入 20 条摘要）
- `reminder_personalizer` — 到期提醒通知文案润色
- `aggregation` — 长期记忆聚合画像提示词

### config/user.yml — 用户画像

告诉 AI 你是谁，**优先级高于自动聚合画像**（长期记忆系统慢慢猜出的画像）：

```yaml
name: "小明"              # 名字/昵称
role: "全栈工程师"         # 职业/角色
preferences:             # 偏好列表
  - "中文交流"
  - "简洁回答"
context: "正在开发 Rust 桌面应用"  # 自由描述
language: "zh-CN"         # 首选语言（空则自动判断）
```

全空时回退到自动聚合画像；有内容时直接使用，跳过聚合。

### 方向键（面板隐藏时）

- 上/下 → 垂直滚动
- 左/右 → 水平滚动
- 80ms 间隔持续触发，3× 滚动速度

## AI Agent

配置来源优先级：**环境变量 > `app_settings.json` 覆盖层 > `~/.claude/settings.json` > 默认值**

```json
// ~/.claude/settings.json
{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-...",
    "ANTHROPIC_BASE_URL": "https://your-proxy.example.com",
    "ANTHROPIC_MODEL": "glm-5v-turbo"
  }
}
```

- max_tokens 统一 **256K**，可用 `ANTHROPIC_MAX_TOKENS` 环境变量覆盖
- Agent 人设："BitCat" — 一个住在屏幕上的桌面 AI 伙伴，活泼好奇，用中文交流
- 内置 **15 个 Tool**：

| Tool | 功能 |
|------|------|
| `launch_program` | 启动外部程序（可选终端） |
| `shell` | 执行 Shell 命令 |
| `read_file` | 读取文件内容 |
| `get_time` | 获取当前时间（支持格式/时区） |
| `recent_screenshots` | 获取最近 N 条截图分析记录 |
| `hotkey` | 发送键盘组合键（SendInput） |
| `clipboard` | 读取剪贴板文本 |
| `foreground` | 按标题聚焦窗口 |
| `perform_dance` | AI 直接提交完整 DanceDef，保存并立即播放 |
| `play_dance` | 播放已保存的舞蹈 |
| `create_reminder` | 创建一次性或重复提醒，写入本地提醒 store |
| `list_reminders` | 查看当前提醒任务 |
| `cancel_reminder` | 取消提醒 |

- 按 Start 键触发对话，流式回复、工具生命周期和最终 `AgentReaction` 会通过 tagged `PetEvent` 驱动宠物状态
- 完成、稍后和删除提醒目前由通知窗口与设置页提供，不暴露为 Agent Tool
- 对话记忆**两层存储**：短期滚动窗口（默认不限条数，由字符预算控制注入）+ 长期 JSONL grep-first 候选召回 + AI 聚合画像
- 所有持久化到 `~/.bitcat/memory/`
- Agent 方法带 `#[instrument]` tracing span，完整记录工具调用链路
- Token 用量写入 `~/.bitcat/logs/token_usage.jsonl`，最近会话聚合写入 `~/.bitcat/logs/token_sessions.json`
- 工具运行时审计写入 `~/.bitcat/logs/tool_events.jsonl`
- 提醒生命周期写入 `~/.bitcat/logs/reminder_events.jsonl`，包含创建失败、触发、完成、稍后、取消、删除和 store 读写异常

## 通信架构

```
Rust 后端                          JS 前端
─────────                          ───────
emit "pet-event"          ──────►  pet.js 监听 → Canvas 动画
emit "pet-toggle-collapse" ─────►  pet.js 折叠/展开切换
emit "bubble-update"      ──────►  bubble.js 全量刷新（bubble.rs）
emit "bubble-chunk"       ──────►  bubble.js 追加文本（bubble.rs）
emit "bubble-tool"        ──────►  bubble.js 工具运行状态（计划/完成/失败/被阻止）
emit "bubble-end"         ──────►  bubble.js 启动自动隐藏（bubble.rs）
emit "panel-nav"          ──────►  panel.js 方向键导航
emit "panel-confirm"      ──────►  panel.js 确认
emit "panel-close"        ──────►  panel.js 关闭
emit "play-dance"          ──────►  app.js 舞蹈播放器（窗口移动+精灵动画）
emit "game-input"         ──────►  game_engine.js 小游戏输入
emit "camera-observation-refresh" ─► camera.js 读取设置并启动/停止采样
emit "camera-observation-capture" ─► camera.js 采样一帧并 invoke 后端
emit "notification-show"  ──────►  notification.js 展示提醒/Agent Watch 通知
emit "agent-watch-update" ──────►  agent_watch.js 渲染任务栈
emit "voice-clear"        ──────►  voice.js 清空 textarea（voice.rs）
emit "voice-flush"        ──────►  voice.js 同步 textarea（voice.rs）
                              ◄───  invoke cmd_consume_bubble_text
                              ◄───  invoke cmd_voice_update_text
                              ◄───  invoke cmd_play_dance / cmd_settings_* / cmd_camera_frame
                              ◄───  invoke cmd_start_game / cmd_start_memory / cmd_start_catch / cmd_start_battle / cmd_screenshot_now
                              ◄───  emit "voice-ready" (mpsc 握手完成)
```

### 线程模型（解耦后）

```
主线程: Tauri event loop + window management
  ├── gamepad_loop (OS thread)     — SDL2 轮询 80ms tick, 按键→PetCommand
  ├── chat_loop (OS thread)       — 气泡输入消费 + 长期记忆聚合（独立于手柄）
  ├── screenshot_loop (OS thread) — 定时截图 + Vision API（聊天/舞蹈时暂停）
  ├── camera window              — 隐藏 WebView getUserMedia + 低频 Vision 观察
  ├── bubble_follower (OS thread) — 气泡跟随宠物窗口定位
  ├── reminder_scheduler (OS thread) — 每 5 秒扫描到期提醒
  ├── dance_bridge (async task)   — mpsc channel 消费 play_dance 指令
  ├── agent_monitor (async task)  — Claude Code / Codex hook 事件看管
  ├── agent_view_server (async task) — 只读 /watch 远程看板
  └── game window                 — 独立 WebView 运行迷你游戏前端逻辑
```

Voice 同步采用 **mpsc channel 握手**：后端发 flush → 前端 invoke 写入 SharedVoice → 前端发 ready → 后端 channel 收到继续（3s 超时兜底）。

## 调试

```bash
# env var 控制：启动 2 秒后自动弹面板，并模拟方向键事件
BITCAT_DEBUG=1 cargo run -p bitcat-app -- --debug
```

前端日志通过 `cmd_panel_log` 命令转发到后端 stderr，方便无 DevTools 时排查。

## 技术栈

- **Tauri 2.0** — WebView 多窗口（pet/bubble/panel/voice/glow/settings/game/agent-watch/notification/camera/pet-inbox），全局热键，托盘
- **SDL2 (bundled)** — 手柄输入读取（DirectInput），热插拔检测
- **rig-core** — AI Agent 抽象层（Anthropic SDK 兼容，streaming prompt + Tool 定义）
- **tokio + futures** — 异步运行时 + 流式处理 + 多线程解耦
- **tracing** — 结构化日志 + `#[instrument]` span 可观测性
- **windows-sys / windows** — SendInput 键鼠模拟 + BitBlt 截图 + SAPI TTS + AttachThreadInput + WASAPI 音频采样
- **serde + serde_yaml** — 配置加载（嵌入 + 外部覆盖）
- **cargo-husky** — Git hooks：pre-commit fmt / pre-push clippy+test
- **Vitest + jsdom** — 前端单元测试（15 个测试文件）
- **Canvas + 粒子效果** — 宠物精灵绘制 + 舞蹈窗口级动画，无打包工具

## License

MIT
