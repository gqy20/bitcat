# ai-pad / 8Bit Cat

蓝牙手柄驱动的桌面工具：8 位像素桌宠 + AI 对话（流式）+ 配置化弹出面板 + 语音输入 + 可选 TTS 朗读 + 截图视觉分析 + 贴边吸附 + 迷你游戏。

基于 Tauri 2.0 + SDL2，单 exe，无 Node.js 依赖。共 **300+ 个测试**（Rust workspace + Vitest 前端），接入 cargo-husky（pre-commit fmt / pre-push clippy+test）。

## 快速开始

```bash
# 开发运行（推荐；Makefile 会设置 Windows SDL2 所需环境变量）
make run

# Release 构建（体积优化：opt-level=z + LTO + strip）
make release

# 打包为版本化 ZIP
make dist
```

启动后看到屏幕角落的像素猫即为成功。

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
| AI 回复后 | 如果设置中开启 TTS，则自动朗读 |
| 系统托盘右键 | 截图/折叠/置顶/**设置窗口**/重载配置/退出 |
| 后台（每 30s） | 截图 + Vision API 分析 + 屏幕活动摘要 |
| AI 对话中 | 自动暂停截图管线（避免浪费 Vision API 调用） |

## 弹出面板

弹出面板由 `config/panel_action.yml` 驱动，默认 480×420、3×3 玻璃风格网格。窗口尺寸、网格行列、按钮数量、图标、排序、启用状态和动作类型都可以通过 YAML 修改。

默认按钮：VSCode / 浏览器 / 资源管理器 / PowerShell / 记事本 / 跳舞 / 游戏 / 设置 / 聊天。

- 弹出后窗口居中、置顶、跳过任务栏、失焦自动隐藏
- 选中项用蓝色光晕高亮，鼠标悬停或方向键移动都会同步选中
- 按下 A / Enter 执行动作后面板自动关闭
- 后端的 `panel-nav` / `panel-confirm` / `panel-close` 事件由 Rust 主循环根据手柄状态发送
- 前端通过 `cmd_get_panel_actions` 获取 ViewModel，不再硬编码按钮列表
- `type: launch` 启动程序，`type: script` 执行 PowerShell 命令，`type: builtin` 调用内置入口（dance/game/settings/chat）

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
3. 结果通过气泡显示，并保存到 `~/.ai-pad/screenshots/`（7 天自动清理）
4. 屏幕活动摘要定时总结，**最近 10 条截图原始分析记录注入 AI prompt**
5. **聊天输入聚焦时自动暂停截图**（避免浪费 Vision API 调用）
6. 舞蹈播放期间同样暂停截图
7. 多显示器会按单个显示器独立分析和保存，多个可见显示器的 Vision API 请求会并行执行，再按显示器顺序汇总到气泡

手动截图入口：

- 系统托盘右键「立即截图」
- 手柄 R2（默认配置）
- 全局热键 `Ctrl+Alt+S`（来自 `actions.yml` 的 `keyboard_shortcut`）
- 双击宠物左眼

## 舞蹈系统

AI 可通过 `perform_dance` Tool 直接提交完整舞蹈编排，前端实时播放像素动画 + 窗口级大幅度位移。

### 编排（AI 侧）

1. AI 在普通对话中自行判断用户是否需要表演舞蹈
2. AI 调用 `perform_dance` Tool，提交完整 `DanceDef`（`jump/spin/wave/shake/idle` + 时长 + repeat）
3. 后端校验舞蹈名称、步数、单步时长和总时长后，YAML 持久化到 `~/.ai-pad/dances/`
4. 项目内置默认舞蹈位于 `config/dances/`，用户/AI 生成舞蹈优先从 `~/.ai-pad/dances/` 读取

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

面板可启动内置 Snake 玩法「毛线球大作战」：

1. 后端创建透明置顶 `game` 窗口并读取 `GameDef`
2. 前端 `game_engine.js` 运行网格逻辑、键盘/手柄方向输入和胜负判定
3. 游戏期间宠物进入 `GamePlay` 状态，结束后按 win / lose / cancel 切换表现
4. `GameDef` 会校验网格尺寸、速度、胜利长度和主题枚举，防止异常配置导致越界或卡死

## 记忆系统（两层存储 + AI 画像）

### 第一层：短期对话记忆

- 滚动窗口（默认 20 条），每次 AI 对话后记录 user_msg + ai_reply（按字符截断）
- 持久化到 `~/.ai-pad/memory/chat_summary.json`
- 每次对话注入 prompt：`[最近对话记录]...[/最近对话记录]`

### 第二层：长期记忆

- 通过启发式规则（关键词匹配 + 长度阈值）筛选值得长期保存的对话
- 支持按关键词相关性评分检索
- 持久化到 `~/.ai-pad/memory/long_term.json`

### AI 聚合画像

- 定期调用 Anthropic API 将未聚合的长期记忆条目浓缩为用户画像
- 最大 400 字符，注入 prompt：`[关于主人]...[/关于主人]`
- 存储到 `~/.ai-pad/memory/profile.json`
- 所有持久化使用原子写入（tempfile + rename）

## 设置窗口

系统托盘右键可打开独立设置窗口（720×520），覆盖层配置机制：

- **AI 覆盖**：api_key / base_url / model / max_tokens → `app_settings.json`
- **动作绑定**：编辑 config/actions.yml 并实时重载
- **提示词配置**：编辑 config/prompts.yml 并实时重载
- **外观设置**：always_on_top / default_collapsed / tts_enabled / global_shortcut / screenshot_interval_sec / pet_position
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
8bit/
├── Cargo.toml              # workspace: members = ["core", "app"], release LTO+strip
├── config/                 # 运行时配置目录（编译时嵌入 exe，exe 同目录可覆盖）
│   ├── actions.yml         # 按键动作绑定
│   ├── buttons.yml         # 硬件按键映射
│   ├── panel_action.yml    # 弹出面板布局、按钮展示和动作
│   ├── prompts.yml         # AI 提示词配置
│   └── user.yml            # 用户画像（名字/角色/偏好/语言）
├── core/                   # 纯逻辑库（无 UI 依赖）
│   └── src/
│       ├── lib.rs          # 模块入口
│       ├── agent.rs        # AI Agent (rig-core + Anthropic SDK), 8+ Tool, 流式 chat_stream, tracing span
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
│       ├── pet.rs          # 桌宠状态机（Idle/Walk/Sleep/Talk/Happy/Confused/Dance）
│       ├── memory.rs       # 两层记忆系统：短期滚动窗口 + 长期关键词检索 + AI 聚合画像
│       ├── vision.rs       # Vision API 请求构建/响应解析
│       ├── screenshot.rs   # 截图类型定义、dHash、resize/JPEG、存储 + 7天清理
│       ├── screen_summary.rs # 屏幕活动摘要存储 + 最近 N 条截图注入 prompt
│       ├── minigame.rs     # 迷你游戏 GameDef schema 与校验
│       ├── tool_events.rs  # 工具运行时事件审计日志
│       ├── token_tracker.rs # Token 用量 JSONL + session 聚合
│       └── tools.rs        # AI Tool 实现（launch/shell/read_file/get_time/recent_screenshots/hotkey/clipboard/foreground/perform_dance/play_dance）
└── app/                    # Tauri 2.0 应用
    ├── tauri.conf.json     # 窗口、权限、withGlobalTauri
    ├── capabilities/
    ├── src/
    │   ├── main.rs         # 入口（--debug 控制台），日志双写初始化
    │   ├── lib.rs          # Tauri Builder + gamepad_loop + chat_loop(独立线程) + bubble_follower(独立线程)
    │   ├── gamepad.rs      # PetEvent 序列化, PetCommand→前端事件转换, chat_loop 解耦
    │   ├── game.rs         # 迷你游戏窗口与生命周期管理
    │   ├── commands.rs     # 共享状态 + Tauri command（snap_preview/crossfade/play_dance 等）
    │   ├── bubble.rs       # 独立气泡窗口, 流式 start/chunk/end 协议, bubble_follower 线程
    │   ├── voice.rs        # 语音输入窗口, 强制前台化, generation 防残留
    │   ├── panel.rs        # 弹出面板（YAML 布局, 方向键导航, 动作执行）
    │   ├── settings.rs     # 设置窗口后端命令（读/写 app_settings + yml 重载）
    │   ├── screenshot.rs   # 截图线程（BitBlt + 熄屏检测 + Vision API + 聊天/舞蹈暂停）
    │   ├── joystick.rs     # SDL2 手柄封装 + is_attached 热插拔检测
    │   ├── tts.rs          # Windows SAPI TTS 语音合成
    │   └── tray.rs         # 系统托盘（右键菜单 + 设置入口 + 重载配置）
    └── frontend/           # 静态 HTML/JS/CSS + Vitest 前端测试
        ├── pet.html        # 宠物窗口（128×128 透明, Canvas 像素精灵 + 粒子 + 舞蹈播放器）
        ├── bubble.html     # 气泡窗口（流式文本 + Markdown + 毛玻璃）
        ├── game.html       # 迷你游戏窗口（Snake Phase 1）
        ├── panel.html      # 面板窗口（尺寸/网格/按钮来自 panel_action.yml）
        ├── voice.html      # 语音输入条（280×40, textarea 接收输入法注入）
        ├── glow.html       # 吸附竖条（发光动画）
        ├── settings.html   # 设置窗口（720×520, 分类 Tab, 实时预览）
        ├── css/            # pet.css / bubble.css / game.css / panel.css / glow.css / settings.css
        ├── js/             # app.js / bubble.js / game_engine.js / panel.js / voice.js / glow.js / settings.js / particles.js / sprite.js / pet.js
        ├── __tests__/      # Vitest 单元测试（7 个测试文件）
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
  height: 420
  columns: 3
  rows: 3

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
- `type: builtin` 支持 `dance` / `game` / `settings` / `chat`
- `type: launch` 和 `type: script` 用于外部程序和脚本快捷入口

### config/prompts.yml — AI 提示词

包含四段配置：
- `agent.preamble` — AI 人设（默认：8Bit 像素猫）
- `vision.prompt` / `vision.prompt_multi` — 截图分析提示词（强调反幻觉）
- `memory` — 短期记忆窗口大小和截断阈值
- `screen_summary` — 截图摘要注入条数（默认 10 条）

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
- Agent 人设："8Bit" — 一只住在屏幕上的像素风小猫助手，活泼好奇，用中文交流
- 内置 **8+ 个 Tool**：

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

- 按 Start 键触发对话，AI 回复关键词驱动桌宠状态切换（"哈哈"/"喵"→Happy, "错误"/"失败"→Confused, 舞蹈相关→Dance）
- 对话记忆**两层存储**：短期滚动窗口（默认 20 条）+ 长期关键词检索 + AI 聚合画像
- 所有持久化到 `~/.ai-pad/memory/`
- Agent 方法带 `#[instrument]` tracing span，完整记录工具调用链路
- Token 用量写入 `~/.ai-pad/logs/token_usage.jsonl`，最近会话聚合写入 `~/.ai-pad/logs/token_sessions.json`
- 工具运行时审计写入 `~/.ai-pad/logs/tool_events.jsonl`

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
emit "voice-clear"        ──────►  voice.js 清空 textarea（voice.rs）
emit "voice-flush"        ──────►  voice.js 同步 textarea（voice.rs）
                              ◄───  invoke cmd_consume_bubble_text
                              ◄───  invoke cmd_voice_update_text
                              ◄───  invoke cmd_play_dance / cmd_settings_*
                              ◄───  invoke cmd_start_game / cmd_screenshot_now
                              ◄───  emit "voice-ready" (mpsc 握手完成)
```

### 线程模型（解耦后）

```
主线程: Tauri event loop + window management
  ├── gamepad_loop (OS thread)     — SDL2 轮询 80ms tick, 按键→PetCommand
  ├── chat_loop (OS thread)       — 气泡输入消费 + 长期记忆聚合（独立于手柄）
  ├── screenshot_loop (OS thread) — 定时截图 + Vision API（聊天/舞蹈时暂停）
  ├── bubble_follower (OS thread) — 气泡跟随宠物窗口定位
  ├── dance_bridge (async task)   — mpsc channel 消费 play_dance 指令
  └── game window                 — 独立 WebView 运行迷你游戏前端逻辑
```

Voice 同步采用 **mpsc channel 握手**：后端发 flush → 前端 invoke 写入 SharedVoice → 前端发 ready → 后端 channel 收到继续（3s 超时兜底）。

## 调试

```bash
# env var 控制：启动 2 秒后自动弹面板，并模拟方向键事件
AI_PAD_DEBUG=1 cargo run -p ai-pad-app -- --debug
```

前端日志通过 `cmd_panel_log` 命令转发到后端 stderr，方便无 DevTools 时排查。

## 技术栈

- **Tauri 2.0** — WebView 多窗口（pet/bubble/panel/voice/glow/settings/game），全局热键，托盘
- **SDL2 (bundled)** — 手柄输入读取（DirectInput），热插拔检测
- **rig-core** — AI Agent 抽象层（Anthropic SDK 兼容，streaming prompt + Tool 定义）
- **tokio + futures** — 异步运行时 + 流式处理 + 多线程解耦
- **tracing** — 结构化日志 + `#[instrument]` span 可观测性
- **windows-sys** — SendInput 键鼠模拟 + BitBlt 截图 + SAPI TTS + AttachThreadInput
- **serde + serde_yaml** — 配置加载（嵌入 + 外部覆盖）
- **cargo-husky** — Git hooks：pre-commit fmt / pre-push clippy+test
- **Vitest + jsdom** — 前端单元测试（7 个测试文件）
- **Canvas + 粒子效果** — 像素精灵绘制 + 舞蹈窗口级动画，无打包工具

## License

MIT
