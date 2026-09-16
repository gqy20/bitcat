# BitCat

[![CI](https://github.com/gqy20/bitcat/actions/workflows/ci.yml/badge.svg)](https://github.com/gqy20/bitcat/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/gqy20/bitcat)](https://github.com/gqy20/bitcat/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> 一只会陪你上班、帮你记事、偶尔把桌面变成小游戏战场的 Windows 桌面 AI 伙伴猫。

BitCat 常驻屏幕边缘：可以流式对话、创建确定性的本地提醒、在你亲手开启后观察屏幕并描述看到的内容、看管 Claude Code 等编码 Agent 的长任务，也会跳舞、陪你玩小游戏、攒积分长大。**所有数据只存在你自己的电脑上**，不内置任何遥测或分析服务。

## 它能做什么

- **AI 对话**：手柄 Start 或点击猫咪嘴巴开聊，流式回复 + 气泡，可选 TTS 朗读；中文优先
- **本地提醒**：「3 分钟后提醒我喝水」「每小时提醒我休息」——确定性任务，到期弹统一通知，不依赖 AI 在线
- **屏幕观察**：默认关闭；你亲手开启后定时截图并给出保守描述（不乱猜、不记敏感内容），聊天/游戏时自动暂停
- **摄像头观察**：默认关闭；开启后低频采样，只做保守描述，明确禁止身份识别
- **Agent Watch**：只读看管 Claude Code / Codex / pi / OpenCode 会话，完成或需要你时提醒；支持远程设备接入
- **小游戏**：8 种玩法，桌面保卫战主推，其余收进游戏库
- **舞蹈与音乐**：AI 现场编一支舞，或播放保存过的舞步；音乐响应舞动
- **积分与成长**：对话、提醒、游戏等日常互动攒积分、升级、解锁成就
- **手柄 + 键盘双通道**：全部操作都不强制手柄

## 安装

### 方式一：下载发布包（推荐）

到 [GitHub Releases](https://github.com/gqy20/bitcat/releases) 下载：

- `bitcat-<版本>-windows-x64-portable.zip` —— 解压即用，双击 `bitcat.exe`
- NSIS 安装包（`.exe`）—— 常规安装，自动建快捷方式

### 方式二：从源码构建

需要 Windows + Visual Studio + Rust。SDL2 静态链接进 exe，无需单独安装；命令统一走 Makefile（PowerShell 下也适用）：

```powershell
$env:CMAKE_POLICY_VERSION_MINIMUM="3.5"
make build      # 开发构建
make release    # 优化构建（opt-level=z + LTO + strip）
make dist       # 打版本化 portable zip
```

## 第一次启动

1. **3 步信任向导**：它是谁 → 它会什么、不会什么 → 随时可收回什么。观察类能力和高风险工具（shell、剪贴板、热键等）**默认全部关闭**，每一项都由你亲手开启，之后随时在设置页修改。
2. **配置 AI**（三种方式任选，优先级从高到低）：
   - 环境变量：`ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_BASE_URL` / `ANTHROPIC_MODEL`
   - 设置页填写（保存在本地 `~/.bitcat/app_settings.json`）
   - 什么都不填：自动复用 `~/.claude/settings.json`（只读，不会修改它）

   ```json
   // ~/.claude/settings.json 示例
   {
     "env": {
       "ANTHROPIC_AUTH_TOKEN": "sk-...",
       "ANTHROPIC_BASE_URL": "https://your-proxy.example.com",
       "ANTHROPIC_MODEL": "claude-sonnet-4-6"
     }
   }
   ```

3. **（可选）配对手柄**：8BitDo Micro 背面模式开关拨到 **D**（D-Input），按住 Pair 键 1 秒，LED 快闪后在 Windows 蓝牙设置中配对。首次使用方向键需 **按住 Select + ↑ 五秒** 激活。

## 快速上手

| 触发 | 行为 |
|------|------|
| 手柄 Start | AI 对话（流式回复，猫咪随状态切换表情） |
| 点击猫咪嘴巴 | 打开聊天输入框，键盘输入送 AI |
| 手柄 A（隐藏时） | 夸奖猫咪 |
| 手柄 Select | 睡眠 / 唤醒 |
| 手柄 Y（按住） | 语音输入 → 识别文字 → 送 AI |
| 手柄 Y（短按） | 跳一支舞 |
| 手柄 Home / `Ctrl+Alt+Space` | 弹出快捷面板（主推游戏 + 游戏库 + 自定义入口） |
| 面板弹出时 方向键/A/B | 导航 / 确认 / 关闭；隐藏时方向键滚动桌面 |
| 手柄 L1/R1/L2 | 按 `config/actions.yml` 绑定执行（热键/启动程序/语音等） |
| 拖拽宠物到屏幕边缘 | 贴边吸附成发光竖条，点击恢复 |
| 拖拽/点击宠物 | 语义小动作反馈（观察、确认、拖拽），不打断工作状态 |
| 系统托盘右键 | 设置 / 立即截图 / 折叠 / 置顶 / 重载配置 / 导出诊断包 / 退出 |
| AI 回复后 | 若开启 TTS 则自动朗读（默认关闭） |

## 小游戏

面板主网格是**桌面保卫战（Invasion）**——小怪试图偷走你的记忆碎片、提醒便签和 Agent 任务卡（都是安全投影，不会动真实数据），用方向键 + A 守住它们。其余玩法收进「游戏库」二级入口：

| 游戏 | 玩法 |
|------|------|
| 毛线球大作战 | Snake，48×32 网格，按住 A 加速 |
| 翻牌配对 | Memory，翻牌越少得分越高 |
| 接食物 | Catch，连续接住涨 combo，失误 5 次失败 |
| 飞机守护战 | 飞行射击，A 发射、X/Y 技能、L1 防护 |
| AI 五子棋 | 落子对弈，AI 给出候选点和思路讲解 |
| 猫猫擂台 | 3D 对战训练 |
| 拼豆 | 像素拼贴，调色/放置/撤销 |

AI 也能在对话里被要求开一局（「来玩个游戏」），或通过 `start_game` 直接启动。游戏用手柄或键盘都能玩，结束后宠物会根据胜负做出反应，并计入积分。

## 提醒

对 AI 说「三分钟后提醒我喝水」即可创建。提醒是**本地确定性任务**：存在你电脑上，到期由本地调度器触发通知，AI 不在线也照常工作。完成、10 分钟后、取消、删除都可以在通知窗口或设置页操作；创建失败时 AI 会明确告诉你没有成功，不会口头承诺。

## Agent Watch

只读看管你的 AI 编码工具：Claude Code / Codex 通过本地 hook 上报会话事件，pi / OpenCode 通过适配接入。浮动任务栈展示每个会话在跑什么、跑了多久；任务完成或等你输入时通过通知提醒你。支持远程 Mac/Linux 设备：设置页生成一键安装命令，远程设备上报事件，浏览器打开只读看板。**它只观察，不控制**。

## 它记住了我什么

- **短期记忆**：最近对话的滚动窗口，用于让 AI 记得上下文
- **长期记忆**：AI 觉得值得记的事（如「主人在做 bitcat 项目」），一行一条存在本地，可在设置页查看和删除
- **用户画像**：`config/user.yml` 里你自己填的名字、职业、偏好——优先级高于 AI 自动总结的画像

设置页「它记住了我什么」分区可以审查、导出或删除全部记忆；「它能做什么」分区是所有权限的总开关仪表盘，一键收回任何能力。

## 隐私与数据

照料一只高权限 AI 伴侣，你随时应该能回答四个疑问，设置页就是按这四个疑问组织的：

1. **它在偷看我的屏幕吗？** —— 截图/摄像头/shell/剪贴板/热键，每项一个显式开关，默认关闭
2. **它记住了我什么？** —— 记忆按人话展示，可查看 / 导出 / 删除
3. **这个月花了多少钱？** —— Token 用量按日/会话/链路统计
4. **它刚才干了什么？** —— 执行过的命令、通知原因、宠物反应决策都可查

所有数据都在本地 `~/.bitcat/` 下（记忆、截图、摄像头记录、日志、积分、提醒），设置页可定位并清理。诊断出问题时，托盘菜单可一键导出诊断包发给开发者。

## 手柄配对（8BitDo Micro）

1. 背面模式开关拨到 **D**（D-Input；S 和 K 模式无法被识别为手柄）
2. 按住 Pair 键 1 秒，LED 快闪
3. Windows 蓝牙设置中搜索配对
4. 首次使用方向键需激活：**按住 Select + ↑ 五秒**

其他手柄也能用：只要是 SDL2 兼容设备即可接入，按键编号映射在 `config/buttons.yml` 中校准。支持热插拔自动重连。

## 配置

配置编译时嵌入 exe，单文件即可运行；在 exe 同目录创建 `config/` 放入 yml 可覆盖默认值。查找顺序：**exe 同目录/config/ → CWD/config/ → 内置默认**。

| 文件 | 用途 |
|------|------|
| `actions.yml` | 手柄按键绑定（launch / hotkey / voice / script / screenshot） |
| `buttons.yml` | 手柄按键编号映射（换手柄时校准） |
| `panel_action.yml` | 面板尺寸、网格、按钮和游戏库分组 |
| `prompts.yml` | AI 人设与各链路提示词 |
| `user.yml` | 用户画像（名字/职业/偏好，你说了算） |

告诉 AI 你是谁（`user.yml`）：

```yaml
name: "小明"
role: "全栈工程师"
preferences:
  - "中文交流"
  - "简洁回答"
context: "正在开发 Rust 桌面应用"
language: "zh-CN"
```

## 常见问题

- **手柄没反应**：确认拨到 D-Input；方向键首次需 Select+↑ 五秒激活；看排障文档
- **对话没回复**：检查 API Key 配置（上方三种方式）；无 Key 时有友好提示，不会空白失败
- **截图观察什么时候跑**：只在设置里开启后才会跑；聊天、游戏、舞蹈时自动暂停
- **日志和数据在哪**：`~/.bitcat/`；设置页可直达并清理

更多文档见 [docs/guide/](docs/guide/)：[入门](docs/guide/getting-started.md) · [手柄](docs/guide/gamepad.md) · [配置](docs/guide/configuration.md) · [AI 对话](docs/guide/ai-chat.md) · [截图观察](docs/guide/screenshot.md) · [远程 Agent Watch](docs/guide/remote-agent-watch.md) · [排障](docs/guide/troubleshooting.md)

## 面向开发者

Rust workspace 三 crate：`core`（纯逻辑，无 UI 依赖，529+ 测试）+ `app`（Tauri 2.0 壳，SDL2 静态链接）+ `xtask`（构建/测试/打包工具链）。前端纯静态 HTML/JS/CSS，无框架无构建，Vitest 20 个测试文件。

```bash
make test-fast        # core 快速测试（~20s）
make test             # 完整 workspace
cd app/frontend && npx vitest run
```

- 架构与编码约定：[CLAUDE.md](CLAUDE.md) · [docs/roadmap.md](docs/roadmap.md) · [docs/architecture/design-tradeoffs.md](docs/architecture/design-tradeoffs.md)
- 产品验收标准：[docs/product/design-spec.md](docs/product/design-spec.md)
- 发布：打 `v*` tag 触发 Release workflow（测试 → Tauri 构建 → 产物 + checksum + 按 tag 从 CHANGELOG 抽取发布说明）

```bash
git tag v0.2.0
git push origin v0.2.0
```

## License

MIT
