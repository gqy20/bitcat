# 配置详解

BitCat 的配置分两类：

- 设置页保存到 `~/.bitcat/app_settings.json`，适合 API、外观、截图间隔、TTS 等用户偏好。
- `config/*.yml` 放在 exe 同目录或项目 `config/` 下，适合按键、面板、提示词和用户画像。

配置读取顺序：

```text
exe 同目录/config > 当前工作目录/config > 内置默认配置
```

设置页保存后大多数配置会立即生效。全局快捷键变更需要重启。

## 设置页

右键托盘图标 → “设置...” 打开设置窗口。

### AI 与对话

| 项 | 说明 |
|----|------|
| API Key | Anthropic API Key 或兼容接口 Key |
| Base URL | API 地址 |
| Model | 对话、Vision、Extractor 使用的模型名 |
| Max Tokens | 最大输出 token |

设置页会显示合并后的当前有效值。写入目标是 `~/.bitcat/app_settings.json`，不会修改 `~/.claude/settings.json`。

### 记忆与画像

可以填写显式用户画像：

```yaml
name: ""
role: ""
preferences: []
context: ""
language: ""
```

显式画像优先级高于 AI 自动聚合画像。只要 `user.yml` 有内容，AI 就会优先看到 `[关于主人]...[/关于主人]` 中的显式信息；全空时才回退到自动聚合画像。

同一页还能审查长期记忆。长期记忆来自 `AgentReaction.memory_candidates` 或 `remember` 工具，保存在 `~/.bitcat/memory/long_term.jsonl`，支持按条删除，删除使用软删除字段 `deleted: true`。

### 按键与操作

可以编辑 `config/actions.yml` 的常见字段：

- 默认终端：`powershell` / `pwsh` / `cmd` / `wt`
- 默认窗口模式：`maximized` / `normal` / `minimized`
- 每个手柄按键的动作类型：`launch`、`hotkey`、`script`、`voice`、`screenshot`、`unbound`

保存前会自动备份为 `.bak`。

### 提示词

对应 `config/prompts.yml`：

- `agent.preamble`：主对话人设和能力说明
- `vision.prompt` / `vision.prompt_multi`：截图分析提示词
- `memory` / `memory_v2`：短期和长期记忆参数
- `screen_summary`：屏幕活动摘要聚合参数
- `aggregation`：自动用户画像聚合提示词

### 外观与行为

| 项 | 说明 |
|----|------|
| 默认置顶 | 控制宠物窗口是否默认 always-on-top |
| 启动时折叠 | 启动后显示为屏幕边缘竖条 |
| TTS 语音 | AI 回复完成后用 Windows SAPI 本地朗读 |
| 全局快捷键 | 默认 `Ctrl+Alt+Space` 打开面板 |
| 截屏分析间隔 | 默认 30 秒，范围 5 到 3600 秒 |
| 摄像头观察 | 默认关闭；开启后使用隐藏 WebView 低频采样摄像头帧 |
| 保存摄像头帧 | 默认关闭；关闭时只保存摄像头观察 JSON |

猫咪被拖拽后的物理坐标会写入 `appearance.pet_position`。保存外观设置时会保留这个位置，不会因为改置顶、折叠、TTS 或截图间隔而清空。

### 用量统计

用量页展示：

- 今日 Token：chat / vision / screen_summary / memory_aggregation
- 最近会话 token 汇总
- 进程内存和系统内存
- 最近 50 条宠物事件决策：`sent` / `deduplicated` / `throttled` / `emit_failed`
- 音乐舞动诊断：模拟 / WASAPI / 停止，显示能量、低频、onset、silence 等状态

## Agent Watch hooks

Agent 看管页可以修复 Claude Code / Codex hook。修复操作会写入 BitCat 自己的 PowerShell hook 脚本，并合并到对应的用户配置：

- Claude Code：`~/.claude/settings.json`
- Codex：`$CODEX_HOME/config.toml` 或 `~/.codex/config.toml`

修复是可重复执行的，只清理带 BitCat `bitcat_marker` 的 hook，不会改动用户或其他工具写入的 hook。详细规则见 [Agent Watch Hooks](agent-watch-hooks.md)。

## config/actions.yml

默认示例：

```yaml
defaults:
  terminal: powershell
  window: maximized

actions:
  Start:
    type: launch
    program: claude
    args: "--dangerously-skip-permissions"
    workdir: "D:\\C\\Desktop\\ai"
    terminal: true

  Y:
    type: voice
    voice:
      trigger: ["ctrl", "win"]
      delay: 1.0

  L1:
    type: hotkey
    trigger: ["alt", "tab"]

  R2:
    type: screenshot
    keyboard_shortcut: "CommandOrControl+Alt+S"
```

动作类型：

| 类型 | 行为 |
|------|------|
| `launch` | 启动外部程序，可选择终端、工作目录和参数 |
| `hotkey` | 使用 Win32 SendInput 发送组合键 |
| `script` | 通过默认终端执行命令 |
| `voice` | 按住触发系统语音输入，松开后把识别文本发给 AI |
| `screenshot` | 立即截图并用 Vision 分析 |
| `unbound` | 不执行任何动作 |

注意：

- 面板可见时，A/B/方向键由面板独占。
- 游戏窗口激活时，方向键/A/B/Start 由游戏独占。
- Home 始终用于切换面板。
- `keyboard_shortcut` 是全局快捷键，修改后需要重启程序。

## config/panel_action.yml

快捷面板布局和按钮来自该文件。当前默认 480x360、2x2，四个入口都是内置小游戏：

```yaml
defaults:
  width: 480
  height: 360
  columns: 2
  rows: 2

actions:
  game:
    label: 毛线球大作战
    icon: "🎮"
    order: 10
    type: builtin
    command: game
```

支持：

- `type: launch`：启动程序
- `type: script`：执行脚本
- `type: builtin`：内置入口，目前支持 `dance`、`game`、`memory`、`catch`、`battle`、`settings`、`chat`

## config/prompts.yml

提示词字段都有编译时默认值。运行时可以覆盖其中任意字段：

```yaml
memory:
  max_entries: 20
  max_context_chars: 1500

memory_v2:
  long_term_max_entries: 200
  retrieve_budget_chars: 10000

screen_summary:
  interval_min: 15
  max_recent_analyses: 30
  max_context_entries: 20
  max_context_chars: 2000
```

还包括 `camera.prompt`、`reminder_personalizer.preamble` 和 `aggregation.prompt`。屏幕、摄像头、提醒润色和摘要都使用结构化输出，文档、日志和设置页都以结构字段为准，不再依赖自由文本解析。

## 热重载

支持运行中生效：

- `actions.yml`：托盘“重载配置”或设置页保存后生效
- `panel_action.yml`：重新打开面板时读取
- `prompts.yml`：托盘“重载配置”或设置页保存后生效
- `app_settings.json`：设置页保存后生效
- 截图间隔：截图线程下一轮读取新值

全局快捷键注册依赖系统 API，修改后请重启。
