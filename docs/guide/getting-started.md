# 快速入门

## 运行要求

- **操作系统**：Windows 10/11（截图、TTS、热键等使用 Windows API）
- **WebView2**：系统自带（Windows 11 已预装；Windows 10 可从微软官网安装）
- **AI API Key**：Anthropic Claude API Key 或兼容接口的 Key
- **蓝牙手柄**（可选）：8BitDo Micro 或其他 SDL2 兼容手柄

## 安装

### 方式一：下载便携包（推荐）

下载 `ai-pad-<version>-windows-x64-portable.zip`，解压后直接运行 `ai-pad-app.exe`，无需安装。

### 方式二：从源码构建

需要安装 [Rust](https://rustup.rs/) 和 [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（含 C++ 桌面开发工作负载）。

```powershell
git clone https://github.com/your-repo/8bit.git
cd 8bit
make build
```

构建完成后 exe 在 `target/debug/` 目录下。Release 构建：

```powershell
make release
```

## 配置 API Key

8Bit Cat 需要 AI API Key 才能使用对话功能。有三种配置方式（优先级从高到低）：

### 1. 系统环境变量（最高优先）

```powershell
set ANTHROPIC_API_KEY=sk-ant-...
set ANTHROPIC_BASE_URL=https://your-proxy.example.com   # 可选，默认 https://api.anthropic.com
set ANTHROPIC_MODEL=claude-sonnet-4-20250514             # 可选
```

### 2. 设置窗口

启动后右键系统托盘图标 → "设置..." → "AI 模型" 标签页，填入 API Key、Base URL 等。修改会保存到 `app_settings.json`，下次启动自动生效。

### 3. 复用 Claude Code 配置

如果你已经在使用 Claude Code，8Bit Cat 会自动读取 `~/.claude/settings.json` 中的 API 配置，无需重复填写。

## 首次运行

双击 `ai-pad-app.exe` 启动后，屏幕角落会出现一只 128x128 像素的猫咪。这就是你的桌宠。

### 基本操作

即使没有手柄，你也可以通过以下方式与猫咪互动：

| 操作 | 效果 |
|------|------|
| **点击猫咪嘴巴区域** | 打开聊天输入框，键入文字与 AI 对话 |
| **双击气泡** | 打开聊天输入框 |
| **拖拽猫咪** | 移动位置 |
| **拖拽到屏幕边缘** | 贴边吸附，变为精致竖条 |
| **点击竖条** | 恢复猫咪形态 |
| **Ctrl+Alt+Space** | 弹出面板（快捷启动程序） |
| **右键托盘图标** | 打开菜单（截图/折叠/置顶/设置/退出） |

### 手柄操作

如果连接了蓝牙手柄，操作方式更丰富：

| 按钮 | 效果 |
|------|------|
| **Start** | 与 AI 对话 |
| **A** | 夸奖猫咪（Happy 状态） |
| **B** | 随机走动 |
| **Y 短按** | 触发舞蹈 |
| **Y 按住** | 语音输入 |
| **Select** | 睡眠 / 唤醒 |
| **Home / Ctrl+Alt+Space** | 弹出 / 关闭面板 |
| **方向键** | 桌面滚动（面板关闭时）/ 面板导航（面板弹出时） |
| **L1 / R1 / L2** | 自定义动作（默认：Alt+Tab / Alt+\` / Ctrl+Tab） |

### AI 对话

按手柄 **Start** 键或**点击猫咪嘴巴**即可开始对话。AI 会以流式方式回复；如果已在设置中开启 TTS，回复完成后会自动朗读。猫咪会根据回复内容自动切换表情：

- 回复包含"哈哈"、"喵"等 → Happy 表情
- 回复包含"错误"、"失败"等 → Confused 表情
- 对话结束后 → 恢复 Idle

## 数据目录

所有用户数据存储在 `~/.ai-pad/` 目录下：

```
~/.ai-pad/
├── memory/
│   ├── chat_summary.json      # 短期对话记忆（最近 20 条）
│   ├── long_term.jsonl         # 长期记忆（一行一条，可 grep）
│   └── profile.json            # AI 聚合用户画像
├── screenshots/
│   └── 2026-05-13/             # 按日期存储的截图和分析
│       ├── 143022.jpg          # 截图文件
│       └── 143022_analysis.json # Vision API 分析结果
├── dances/                     # AI 生成的舞蹈定义
├── logs/                       # 运行日志（按日滚动）
├── token_usage.jsonl           # Token 使用记录
└── token_sessions.json         # 会话级 Token 汇总
```

截图文件 **7 天自动清理**，日志按日滚动。
