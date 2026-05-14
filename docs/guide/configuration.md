# 配置详解

所有配置文件在编译时通过 `include_str!` 嵌入 exe，单文件即可运行。用户可在 exe 同目录创建 `config/` 文件夹放入 yml 文件覆盖默认配置。

**查找顺序**：exe 同目录 `config/` → CWD `config/` → 内置默认值

## 设置窗口

右键托盘图标 → "设置..." 打开设置窗口（720×520）。设置窗口有 6 个标签页：

### AI 模型

| 配置项 | 说明 |
|--------|------|
| API Key | Anthropic API Key 或兼容接口 Key |
| Base URL | API 地址，默认 `https://api.anthropic.com` |
| 模型 | 模型名，默认 `claude-sonnet-4-20250514` |
| Max Tokens | 最大输出 token，默认 256000 |

每个配置项旁会显示**当前生效值**（只读），来自合并后的实际配置链。

写入目标为 `app_settings.json`，不会修改 `~/.claude/settings.json`。

### 用量统计

- 今日 Token 用量（按 Chat/Vision/ScreenSummary/MemoryAggregation 分类）
- 输入/输出/缓存读写 token 明细
- 最近 10 条会话记录
- 记录文件路径

### 按键绑定

- 选择默认终端（PowerShell / pwsh / cmd / Windows Terminal）
- 选择默认窗口模式（最大化 / 普通 / 最小化）
- 每个手柄按键可配置为 5 种动作类型之一

### Prompt

- Agent Preamble：AI 人设提示词
- Vision Prompt：截图分析提示词（单屏 / 多屏各一个）
- Memory：最大条目数（默认 20）、最大上下文字符（默认 1500）
- 屏幕摘要：聚合间隔（分钟）

### 外观行为

| 配置项 | 默认值 | 说明 |
|--------|--------|------|
| 默认置顶 | 开启 | 窗口始终在最前 |
| 启动时折叠 | 关闭 | 启动后直接以竖条形态显示 |
| TTS 语音 | 开启 | AI 回复后自动朗读 |
| 全局快捷键 | Ctrl+Alt+Space | 弹出面板的快捷键 |
| 截屏分析间隔 | 30 秒 | 范围 5~3600 秒 |

### 关于

显示版本号、配置文件路径、项目地址。

---

## 配置文件详解

### config/actions.yml — 按键动作绑定

定义每个手柄按键对应的动作。支持 6 种动作类型：

```yaml
defaults:
  terminal: powershell       # 默认终端：powershell / pwsh / cmd / wt
  window: maximized          # 默认窗口：maximized / normal / minimized

actions:
  # 启动程序
  Start:
    type: launch
    program: claude
    args: "--dangerously-skip-permissions"
    workdir: "D:\\C\\Desktop\\ai"
    terminal: true            # 在终端窗口中运行
    keyboard_shortcut: ""     # 可选：绑定全局键盘快捷键

  # 发送键盘组合键
  L1:
    type: hotkey
    trigger: ["alt", "tab"]  # Alt+Tab 切换窗口
    keyboard_shortcut: ""

  # 语音输入
  Y:
    type: voice
    voice:
      trigger: ["ctrl", "win"]   # 系统语音输入热键
      delay: 1.0                 # 延迟秒数
    keyboard_shortcut: ""

  # 执行脚本
  R2:
    type: script
    command: "python my_script.py"
    keyboard_shortcut: ""

  # 立即截图分析
  X:
    type: screenshot
    keyboard_shortcut: "CommandOrControl+Alt+S"

  # 未绑定
  R1:
    type: unbound
    keyboard_shortcut: ""
```

#### 动作类型说明

| 类型 | 参数 | 行为 |
|------|------|------|
| `launch` | program, args, workdir, terminal | 启动外部程序。terminal=true 时在 PowerShell 中运行 |
| `hotkey` | trigger | 发送键盘组合键（Win32 SendInput） |
| `voice` | voice.trigger, voice.delay | 按住触发语音输入，释放后取识别文本送 AI |
| `script` | command | 通过 PowerShell 执行命令 |
| `screenshot` | （无） | 立即截图并用 Vision 分析当前屏幕 |
| `unbound` | （无） | 该按键不执行任何动作 |

每个动作还可以配置 `keyboard_shortcut` 字段，为该动作绑定一个全局键盘快捷键（需重启生效）。

#### 注意事项

- 面板可见时，A/B/方向键由面板独占，不触发 actions.yml 绑定
- Home 键始终用于切换面板，不可重新绑定
- 保存前自动备份为 `.yml.bak`

### config/buttons.yml — 硬件按键映射

SDL2 按键编号到名称的映射表。针对 8BitDo Micro D-Input 模式实测校准。

如果更换手柄型号，需要根据实际按键编号修改此文件。你可以通过程序日志（`--debug` 模式）查看按键按下时的编号。

### config/panel_action.yml — 弹出面板布局与快捷入口

定义弹出面板的窗口尺寸、网格行列、按钮展示和按钮动作。修改后重新打开面板即可按新布局渲染；如果面板已存在，下一次显示时会同步窗口大小。

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

  explorer:
    label: 资源管理器
    icon: "📁"
    order: 30
    type: launch
    program: explorer
    args: "D:\\C\\Desktop\\ai"
    terminal: false

  chat:
    label: 聊天
    icon: "💬"
    order: 90
    type: builtin
    command: chat
```

`type: launch` 和 `type: script` 用于外部程序/脚本；`type: builtin` 用于内置入口，目前支持 `dance`、`game`、`settings`、`chat`。前端不再硬编码按钮列表，而是从后端读取 `panel_action.yml` 生成的 ViewModel。

### config/prompts.yml — AI 提示词

```yaml
agent:
  preamble: |
    你是 "8Bit"，一只住在屏幕上的像素风小猫助手...

vision:
  prompt: "请用中文简洁描述屏幕内容..."
  prompt_multi: "请用中文简洁描述多屏幕内容..."

memory:
  max_entries: 20           # 短期记忆条目数
  max_context_chars: 1500   # 注入 prompt 的字符上限
  max_user_chars: 100       # 单条用户消息截断长度
  max_reply_chars: 200      # 单条 AI 回复截断长度

screen_summary:
  max_entries: 10           # 注入 prompt 的截图分析条数
```

所有字段都有编译时默认值，YAML 可选覆盖。修改后通过托盘菜单"重载配置"或设置窗口即时生效。

### config/user.yml — 用户画像

```yaml
name: ""                     # 名字/昵称
role: ""                     # 职业/角色
preferences: []              # 偏好列表
context: ""                  # 自由描述
language: ""                 # 首选语言（空则自动判断）
```

填写的个人信息会以 `[关于主人]...[/关于主人]` 的格式注入 AI prompt。**显式画像优先级高于 AI 聚合画像**。全空时回退到 AI 自动从长期记忆中聚合的画像。

可在设置窗口中编辑，也可直接修改文件。

---

## AI 配置优先级

API Key、Base URL、模型名的配置来源按优先级排列：

```
系统环境变量 > app_settings.json > ~/.claude/settings.json > 内置默认值
```

| 来源 | 环境变量 | 说明 |
|------|---------|------|
| 系统环境变量 | `ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` | 最高优先 |
| 系统环境变量 | `ANTHROPIC_BASE_URL` | API 地址 |
| 系统环境变量 | `ANTHROPIC_MODEL` | 模型名 |
| 系统环境变量 | `ANTHROPIC_MAX_TOKENS` | 最大 token |
| app_settings.json | `ai.api_key` / `ai.base_url` / ... | 设置窗口修改 |
| ~/.claude/settings.json | `env.ANTHROPIC_API_KEY` / ... | 只读，不修改 |

这样设计可以复用 Claude Code 的配置，也支持通过环境变量或设置窗口覆盖。

---

## 配置热重载

以下配置支持修改后即时生效，无需重启程序：

- `config/actions.yml` — 通过托盘"重载配置"或设置窗口保存
- `config/panel_action.yml` — 通过托盘"重载配置"重新校验，面板点击时实时读取
- `config/prompts.yml` — 同上
- `app_settings.json` — 设置窗口保存后即时生效（gamepad_loop 下一个 tick 刷新）
- 截屏分析间隔 — 截图线程每轮重新读取，即时生效

配置重载通过原子标志位 `config_reload` 协调：设置窗口保存后设标志，gamepad_loop 在下一个 80ms tick 检测到标志后重新加载配置文件。
## 桌宠位置持久化

桌宠拖拽结束且没有进入吸附态时，会自动把物理坐标写入 `app_settings.json` 的 `appearance.pet_position`。应用下次启动会读取该字段，并按当前显示器工作区校正位置，避免窗口出现在屏幕外。

设置窗口保存外观行为时会保留 `pet_position`，不会因为修改置顶、折叠、TTS 或截图间隔而清空桌宠位置。
