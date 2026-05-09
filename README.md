# ai-pad / 8Bit Cat

蓝牙手柄驱动的桌面工具：8 位像素桌宠 + AI 对话 + Quicker 风格弹出面板 + 系统级按键映射。

基于 Tauri 2.0 + SDL2，单 exe，无 Node.js 依赖。

## 快速开始

```bash
# 开发运行
cargo run -p ai-pad-app

# Release 构建（体积优化：opt-level=z + LTO + strip）
cargo build -p ai-pad-app --release
```

启动后看到屏幕角落的像素猫即为成功。

## 功能概览

| 触发 | 行为 |
|------|------|
| 手柄 Start | AI 对话（猫咪根据回复自动切换状态/气泡） |
| 手柄 Home / `Ctrl+Alt+Space` | 弹出 Quicker 风格面板 |
| 手柄方向键（dpad）/ 键盘 ↑↓←→ | 桌面滚动（隐藏时）/ 面板选择（弹出时） |
| 手柄 A / Enter | 面板确认 |
| 手柄 B / Esc | 关闭面板 |
| 其他按键 | 按 `actions.yml` 绑定执行（hotkey/launch/voice/script） |

## 弹出面板

3 × 2 玻璃风格网格，默认六个动作：VSCode / 浏览器 / 资源管理器 / PowerShell / 记事本 / 问 AI（待实现）。

- 弹出后窗口居中、置顶、跳过任务栏
- 失焦自动隐藏
- 选中项用蓝色光晕高亮，鼠标悬停或方向键移动都会同步选中
- 按下 A / Enter 启动对应程序后面板自动关闭

后端的 `panel-nav` / `panel-confirm` / `panel-close` 事件由 Rust 主循环根据手柄状态发送，前端 JS 通过 `window.__TAURI__.event.listen` 接收。

## 手柄配对

以 8BitDo Micro 为例：

1. 将手柄背面模式开关拨到 **D**（D-Input 模式）
2. 按住 Pair 按钮 1 秒，LED 快闪
3. Windows 蓝牙设置中搜索配对
4. 首次使用方向键需激活：**按住 Select + ↑ 五秒**

## 项目结构

```
8bit/
├── Cargo.toml              # workspace: members = ["core", "app"]
├── actions.yml             # 按键动作绑定（个性化配置）
├── buttons.yml             # 硬件按键映射（换手柄改这里）
├── core/                   # 纯逻辑库（无 UI 依赖，81 单测）
│   └── src/
│       ├── pet.rs          # 桌宠状态机（Idle/Walk/Sleep/Talk/Happy/Confused）
│       ├── bridge.rs       # 手柄按键 → 宠物状态/AI 调用
│       ├── agent.rs        # AI Agent (rig + DeepSeek/Kimi/etc)
│       ├── ai_config.rs    # AI 模型 / 256K 上下文配置
│       ├── action.rs       # 动作定义（hotkey/launch/voice/script）
│       ├── config.rs       # YAML 配置加载
│       ├── device.rs       # 按键编号 → 名称映射
│       ├── hotkey.rs       # Win32 SendInput 键鼠模拟
│       └── tools.rs        # AI tool calls
└── app/                    # Tauri 应用
    ├── tauri.conf.json     # 窗口、权限、withGlobalTauri
    ├── capabilities/
    ├── src/
    │   ├── lib.rs          # Tauri Builder + 全局热键 + 手柄循环
    │   ├── commands.rs     # 宠物状态命令（set_state / walk_to / show_bubble / get_status / tick）
    │   ├── panel.rs        # 弹出面板（动态创建窗口 + cmd_execute_panel_action）
    │   ├── tray.rs         # 系统托盘
    │   ├── gamepad.rs      # 按键事件 → 前端 emit
    │   └── joystick.rs     # SDL2 封装
    └── frontend/           # 静态 HTML（无 npm）
        ├── pet.html        # 宠物窗口（128×128 透明）
        ├── panel.html      # 面板窗口（480×320 玻璃风）
        ├── css/
        └── js/
            ├── sprite.js   # 16×16 像素精灵数据 + Canvas 绘制
            ├── pet.js      # 宠物状态机（前端）
            ├── app.js      # Tauri 事件监听
            └── panel.js    # 面板交互 + 方向键导航
```

## 配置说明

### actions.yml — 按键动作

支持四种动作类型：

```yaml
defaults:
  terminal: powershell

actions:
  # launch: 启动程序（可选在终端中）
  Start:
    type: launch
    program: claude
    args: "--dangerously-skip-permissions"
    workdir: "D:\\C\\Desktop\\ai"
    terminal: true

  # voice: 触发系统语音输入法快捷键
  Y:
    type: voice
    voice:
      trigger: ["ctrl", "win"]
      delay: 1.0

  # hotkey: 发送键盘组合键（支持 Alt/Ctrl+Tab 持续按住）
  L1:
    type: hotkey
    trigger: ["alt", "tab"]
  L2:
    type: hotkey
    trigger: ["ctrl", "tab"]
  R1:
    type: hotkey
    trigger: ["alt", "backtick"]

  # script: 执行 PowerShell 命令
  A:
    type: script
    command: "python my_script.py"
```

注意：当面板可见时，A / B / dpad 由面板独占，不再触发 actions.yml 绑定。

### buttons.yml — 硬件映射

按键编号到名称的映射，每种手柄不同。8BitDo Micro 已实测填好；换手柄需要校准。

### 方向键（面板隐藏时）

- 上/下 → 垂直滚动
- 左/右 → 水平滚动
- 长按持续滚动（80ms 间隔，3× 速）

## AI Agent

`core/src/ai_config.rs` 支持 `~/.claude/ai_config.toml` 配置（兼容 OpenAI 格式的国产模型）：

```toml
api_base = "https://api.deepseek.com/v1"
api_key = "sk-..."
model = "deepseek-chat"
# max_tokens 默认 256K，可用 AI_PAD_MAX_TOKENS 环境变量覆盖
```

按 Start 键触发对话，AI 回复中包含 "好"/"开心" 等关键词时，桌宠自动切到 Happy 状态显示气泡。

## 调试

```bash
# env var 控制：启动 2 秒后自动弹面板，并模拟方向键事件
AI_PAD_DEBUG=1 cargo run -p ai-pad-app
```

前端 `console.log` 通过 `cmd_panel_log` 命令转发到后端 stderr，方便无 DevTools 时排查。

## 技术栈

- **Tauri 2.0** — WebView 多窗口（pet + 动态创建的 panel），全局热键，托盘
- **SDL2 (bundled)** — 手柄输入读取（DirectInput）
- **rig** — AI Agent 抽象层（统一 OpenAI 兼容 API）
- **windows-sys** — SendInput 键鼠模拟
- **serde + serde_yaml** — 配置加载
- **静态 HTML** — 像素精灵用 Canvas 绘制，无 npm / 无打包工具

## License

MIT
