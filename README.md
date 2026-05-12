# ai-pad / 8Bit Cat

蓝牙手柄驱动的桌面工具：8 位像素桌宠 + AI 对话（流式）+ Quicker 风格弹出面板 + 语音输入 + TTS 朗读 + 截图视觉分析 + 贴边吸附。

基于 Tauri 2.0 + SDL2，单 exe，无 Node.js 依赖。共 **216 个测试**（Rust workspace + Vitest 前端）。

## 快速开始

```bash
# 开发运行（--debug 分配控制台窗口查看日志）
cargo run -p ai-pad-app -- --debug

# Release 构建（体积优化：opt-level=z + LTO + strip）
cargo build -p ai-pad-app --release

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
| 手柄 L1/R1/L2 | 按 `actions.yml` 绑定执行（hotkey/launch/voice/script） |
| 拖拽宠物到边缘 | 贴边吸附，变精致发光竖条（弹簧缓动动画） |
| 点击猫咪嘴巴 | 打开聊天输入框，键盘输入文字送 AI |
| AI 回复后 | 自动 TTS 朗读 |
| 系统托盘右键 | 截图/折叠/置顶/重载配置/退出 |
| 后台（每 30s） | 截图 + Vision API 分析 + 屏幕活动摘要 |

## 弹出面板

3 × 2 玻璃风格网格，默认六个动作：VSCode / 浏览器 / 资源管理器 / PowerShell / 记事本 / 问 AI（待实现）。

- 弹出后窗口居中、置顶、跳过任务栏、失焦自动隐藏
- 选中项用蓝色光晕高亮，鼠标悬停或方向键移动都会同步选中
- 按下 A / Enter 启动对应程序后面板自动关闭
- 后端的 `panel-nav` / `panel-confirm` / `panel-close` 事件由 Rust 主循环根据手柄状态发送

## AI 流式回复 & 气泡

按 Start 键触发 AI 对话时：

1. 后端调用 `agent.chat_stream()` 开始流式生成
2. 同时打开独立气泡窗口，定位在宠物上方
3. 每收到文本 chunk 通过 `bubble-chunk` 事件实时推送到前端渲染
4. 流结束后发送 `bubble-end` 事件，前端启动自动隐藏定时器
5. 前端初始化时通过 `cmd_consume_bubble_text` invoke 拉取已有文本（解决 emit 早于 listen 的竞态）

气泡支持 Markdown 渲染（marked.js）、毛玻璃效果、动画光标、聊天输入框模式。

## 语音输入

按住 Y 键（或 actions.yml 中配置的 voice 类型按键）：

1. 显示录音条窗口（280×40），移到屏幕中下方，强制前台化（`AttachThreadInput`）
2. 模拟用户配置的输入法语音热键（如 Ctrl+Win）
3. 松开按键后等待识别引擎注入文字到 textarea（700ms）
4. 通过前后端握手协议同步文本：
   - 后端 emit `voice-flush` → 前端 invoke `cmd_voice_update_text` → 前端 emit `voice-ready` → 后端 mpsc channel 收到继续（3s 超时兜底）
5. 取到的文本直接送 AI 流式对话，结果同样走气泡显示

## 截图观察

独立后台线程定时截图（默认 30s），流程：

1. BitBlt 捕获屏幕 + dHash 感知哈希去重 + 熄屏检测（`SM_MONITORISOFF` + 全黑帧采样）
2. 缩放 → JPEG 编码 → Vision API（Anthropic Messages）分析
3. 结果通过气泡显示，并保存到 `~/.ai-pad/screenshots/`（7 天自动清理）
4. 屏幕活动摘要定时总结，注入 AI 上下文
5. 支持多显示器水平拼接

## 贴边吸附

拖拽宠物到屏幕左右边缘时：

1. 磁性预告条提示松手会吸附的位置
2. 松手后宠物变为精致发光竖条（多层发光 + hover 展宽 + 品牌色）
3. 点击竖条恢复宠物形态
4. 气泡自动跟随吸附态定位
5. 双窗口 Crossfade 过渡（150ms CSS 动画）

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
├── actions.yml             # 按键动作绑定（示例配置，已嵌入 exe）
├── buttons.yml             # 硬件按键映射（示例配置，已嵌入 exe）
├── prompts.yml             # AI 提示词（示例配置，已嵌入 exe）
├── core/                   # 纯逻辑库（无 UI 依赖）
│   └── src/
│       ├── lib.rs          # 模块入口
│       ├── agent.rs        # AI Agent (rig-core + Anthropic SDK), 5 个 Tool, 流式 chat_stream
│       ├── bridge.rs       # 手柄→AI→宠物桥接层, PetCommand IPC 协议, 按键映射
│       ├── ai_config.rs    # 从 ~/.claude/settings.json 或环境变量读取 API 配置
│       ├── action.rs       # 动作定义与加载（hotkey/launch/voice/script），配置嵌入 + 多路径查找
│       ├── config.rs       # YAML 配置加载（buttons.yml），配置嵌入 + 多路径查找
│       ├── prompts.rs      # AI 提示词配置（agent/vision/memory/screen_summary），配置嵌入
│       ├── device.rs       # SDL2 按键编号 → 名称映射
│       ├── hotkey.rs       # Win32 SendInput 键鼠模拟 + force_foreground
│       ├── pet.rs          # 桌宠状态机（Idle/Walk/Sleep/Talk/Happy/Confused）
│       ├── memory.rs       # 对话记忆滚动窗口，JSON 持久化
│       ├── vision.rs       # Vision API 请求构建/响应解析
│       ├── screenshot.rs   # 截图类型定义、dHash、resize/JPEG、存储 + 7天清理
│       ├── screen_summary.rs # 屏幕活动摘要存储 + AI 上下文构建
│       └── tools.rs        # AI Tool 实现（launch/shell/read_file/get_time/recent_screenshots）
└── app/                    # Tauri 2.0 应用
    ├── tauri.conf.json     # 窗口、权限、withGlobalTauri
    ├── capabilities/
    ├── src/
    │   ├── main.rs         # 入口（--debug 控制台），日志双写初始化
    │   ├── lib.rs          # Tauri Builder + gamepad_loop（热插拔外层循环）
    │   ├── gamepad.rs      # PetEvent 序列化, PetCommand→前端事件转换
    │   ├── commands.rs     # 共享状态 + Tauri command（snap_preview/crossfade 等）
    │   ├── bubble.rs       # 独立气泡窗口, 流式 start/chunk/end 协议
    │   ├── voice.rs        # 语音输入窗口, 强制前台化, generation 防残留
    │   ├── panel.rs        # 弹出面板（方向键导航, 动作执行）
    │   ├── screenshot.rs   # 截图线程（BitBlt + 熄屏检测 + Vision API）
    │   ├── joystick.rs     # SDL2 手柄封装 + is_attached 热插拔检测
    │   ├── tts.rs          # Windows SAPI TTS 语音合成
    │   └── tray.rs         # 系统托盘（右键菜单 + 重载配置）
    └── frontend/           # 静态 HTML/JS/CSS + Vitest 前端测试
        ├── pet.html        # 宠物窗口（128×128 透明, Canvas 像素精灵 + 粒子）
        ├── bubble.html     # 气泡窗口（流式文本 + Markdown + 毛玻璃）
        ├── panel.html      # 面板窗口（480×320 玻璃风, 方向键导航）
        ├── voice.html      # 语音输入条（280×40, textarea 接收输入法注入）
        ├── glow.html       # 吸附竖条（发光动画）
        ├── css/            # pet.css / bubble.css / panel.css / glow.css
        ├── js/             # app.js / bubble.js / panel.js / voice.js / glow.js / particles.js / sprite.js / pet.js
        ├── __tests__/      # Vitest 单元测试（6 个测试文件）
        └── vitest.config.ts
```

## 配置说明

所有配置编译时通过 `include_str!` 嵌入 exe，单文件即可运行。用户放 yml 到 exe 同目录可覆盖默认配置。

查找顺序：**exe 同目录 → CWD → 内置默认值**

### actions.yml — 按键动作

支持四种动作类型：

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

  # script: 执行 PowerShell 命令
  # A:
  #   type: script
  #   command: "python my_script.py"
```

注意：当面板可见时，A / B / dpad 由面板独占，不再触发 actions.yml 绑定。

### buttons.yml — 硬件映射

按键编号到名称的映射，每种手柄不同。8BitDo Micro D-Input 已实测填好；换手柄需要校准。

### prompts.yml — AI 提示词

包含三段配置：
- `agent.preamble` — AI 人设（默认：8Bit 像素猫）
- `vision.prompt` / `vision.prompt_multi` — 截图分析提示词（强调反幻觉）
- `memory` — 记忆窗口大小和截断阈值

### 方向键（面板隐藏时）

- 上/下 → 垂直滚动
- 左/右 → 水平滚动
- 80ms 间隔持续触发，3× 滚动速度

## AI Agent

配置来源优先级：**环境变量 > `~/.claude/settings.json` > 默认值**

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
- 内置 5 个 Tool：`launch_program` / `shell` / `read_file` / `get_time` / `recent_screenshots`
- 按 Start 键触发对话，AI 回复关键词驱动桌宠状态切换（"哈哈"/"喵"→Happy, "错误"/"失败"→Confused）
- 对话记忆滚动窗口（默认 20 条），持久化到 `~/.ai-pad/memory/`

## 通信架构

```
Rust 后端                          JS 前端
─────────                          ───────
emit "pet-event"          ──────►  pet.js 监听 → Canvas 动画
emit "pet-toggle-collapse" ─────►  pet.js 折叠/展开切换
emit "bubble-update"      ──────►  bubble.js 全量刷新（bubble.rs）
emit "bubble-chunk"       ──────►  bubble.js 追加文本（bubble.rs）
emit "bubble-end"         ──────►  bubble.js 启动自动隐藏（bubble.rs）
emit "panel-nav"          ──────►  panel.js 方向键导航
emit "panel-confirm"      ──────►  panel.js 确认
emit "panel-close"        ──────►  panel.js 关闭
emit "voice-clear"        ──────►  voice.js 清空 textarea（voice.rs）
emit "voice-flush"        ──────►  voice.js 同步 textarea（voice.rs）
                              ◄───  invoke cmd_consume_bubble_text
                              ◄───  invoke cmd_voice_update_text
                              ◄───  emit "voice-ready" (mpsc 握手完成)
```

Voice 同步采用 **mpsc channel 握手**：后端发 flush → 前端 invoke 写入 SharedVoice → 前端发 ready → 后端 channel 收到继续（3s 超时兜底）。

## 调试

```bash
# env var 控制：启动 2 秒后自动弹面板，并模拟方向键事件
AI_PAD_DEBUG=1 cargo run -p ai-pad-app -- --debug
```

前端日志通过 `cmd_panel_log` 命令转发到后端 stderr，方便无 DevTools 时排查。

## 技术栈

- **Tauri 2.0** — WebView 多窗口（pet/bubble/panel/voice/glow），全局热键，托盘
- **SDL2 (bundled)** — 手柄输入读取（DirectInput），热插拔检测
- **rig-core** — AI Agent 抽象层（Anthropic SDK 兼容，streaming prompt）
- **tokio + futures** — 异步运行时 + 流式处理
- **windows-sys** — SendInput 键鼠模拟 + BitBlt 截图 + SAPI TTS + AttachThreadInput
- **serde + serde_yaml** — 配置加载（嵌入 + 外部覆盖）
- **Vitest + jsdom** — 前端单元测试（6 个测试文件）
- **Canvas + 粒子效果** — 像素精灵绘制，无打包工具

## License

MIT
