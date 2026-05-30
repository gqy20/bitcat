# 快速入门

BitCat 是一个 Windows 桌面 AI 伙伴：它以小宠物的形式常驻屏幕边角，可以聊天、看屏幕、记住偏好、跳舞、启动工具，也能用手柄控制。

## 运行要求

- Windows 10/11。截图、热键、语音、TTS、WASAPI 音乐舞动都依赖 Windows API。
- WebView2。Windows 11 通常已内置；Windows 10 如缺失请安装 Microsoft WebView2 Runtime。
- AI API Key。支持 Anthropic Claude API 或兼容 Anthropic Messages 协议的代理。
- 蓝牙手柄可选。推荐 8BitDo Micro D-Input 模式；其他 SDL2 兼容手柄也可使用。

## 安装方式

### 便携包

下载 `bitcat-<version>-windows-x64-portable.zip`，解压后运行 `bitcat.exe`。便携包内包含：

```text
bitcat.exe
config/*.yml
```

### 源码构建

需要 Rust、Visual Studio Build Tools 和 `cargo-nextest`。

```powershell
git clone https://github.com/gqy20/bitcat.git
cd bitcat
make build
```

日常建议通过 Makefile 构建和测试。Windows 下 `make build` 已设置 SDL2 所需的 `CMAKE_POLICY_VERSION_MINIMUM=3.5`。

## 配置 AI

配置优先级从高到低：

```text
环境变量 > ~/.bitcat/app_settings.json > ~/.claude/settings.json > 内置默认值
```

推荐第一次启动后右键系统托盘图标，打开“设置...”，在“AI 与对话”里填写：

- API Key
- Base URL，默认 `https://api.anthropic.com`
- Model，默认 `claude-sonnet-4-20250514`
- Max Tokens

也可以使用环境变量：

```powershell
$env:ANTHROPIC_API_KEY="sk-ant-..."
$env:ANTHROPIC_BASE_URL="https://api.anthropic.com"
$env:ANTHROPIC_MODEL="claude-sonnet-4-20250514"
```

如果你已经使用 Claude Code，应用会只读读取 `~/.claude/settings.json` 中的 Anthropic 配置，不会改写它。

## 首次运行

双击 `bitcat.exe` 后，屏幕角落会出现 128x128 的小宠物窗口。右键托盘图标可以打开菜单：立即截图、停止舞动、置顶、折叠、重载配置、设置、退出。

常用入口：

| 操作 | 效果 |
|------|------|
| 点击猫咪嘴巴 | 打开聊天输入 |
| 双击气泡 | 打开聊天输入 |
| 拖拽猫咪 | 移动位置，位置会保存到 `app_settings.json` |
| 拖到屏幕边缘 | 折叠成边缘竖条 |
| 点击竖条 | 恢复猫咪 |
| `Ctrl+Alt+Space` | 打开快捷面板 |
| 右键托盘图标 | 打开设置和运行菜单 |

## 手柄默认操作

| 按键 | 默认效果 |
|------|------|
| Start | 启动 `config/actions.yml` 中的 `Start` 动作，默认打开 Claude CLI |
| A | 夸奖猫咪 |
| B | 随机走动 |
| Y 短按 | 播放默认舞蹈 |
| Y 按住 | 语音输入 |
| Select | 睡眠 / 唤醒 |
| Home | 打开 / 关闭快捷面板 |
| R2 | 立即截图分析 |
| L1 / R1 / L2 | 默认发送 Alt+Tab / Alt+` / Ctrl+Tab |
| 方向键 | 面板可见时导航；否则滚动当前桌面窗口 |

快捷面板默认是 2x2 网格，来自 `config/panel_action.yml`：毛线球大作战、翻牌配对、接食物、飞机守护战。VSCode、浏览器、设置、聊天等入口可以按需手动加回 YAML。

## 数据目录

用户数据位于 `~/.bitcat/`：

```text
~/.bitcat/
├── app_settings.json
├── memory/
│   ├── chat_summary.json
│   ├── long_term.jsonl
│   ├── long_term.md
│   └── profile.json
├── screenshots/YYYY-MM-DD/
│   ├── HHMMSS.jpg
│   └── HHMMSS_analysis.json
├── camera/YYYY-MM-DD/
│   └── HHMMSS_analysis.json
├── dances/
├── logs/
│   ├── token_usage.jsonl
│   ├── token_sessions.json
│   ├── tool_events.jsonl
│   ├── reminder_events.jsonl
│   ├── agent_watch_events.jsonl
│   └── agent_watch_sessions.jsonl
└── scores/                 # 后续游戏分数使用
```

截图目录会自动清理 7 天前的数据。摄像头观察默认关闭，开启后默认只保存分析 JSON，勾选保存帧后才会留下图片。长期记忆是 JSONL，一行一条，可用 `rg` 检索，也可在设置页审查和删除。
