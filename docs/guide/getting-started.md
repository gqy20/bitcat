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

### 在非 Windows 开发机上开发（Linux / macOS）

`core` 是零 UI 依赖的纯逻辑 crate，`app` 里的 Win32 调用全部带 `#[cfg(not(windows))]` 回退，
`app/frontend` 是纯静态 HTML/JS/CSS，所以三层都能在非 Windows 机器上开发和测试：

```bash
cargo test -p bitcat-core          # 469 个测试，约 1 秒
cargo test -p bitcat-app --lib     # 160 个测试，app 层单测在 Linux 上同样能跑
cd app/frontend && npx vitest run  # 187 个测试，约 1.7 秒
python3 -m http.server 4178        # 在 app/frontend 下静态起页，在浏览器里调 UI
```

仍然不能做的事：`bitcat` **可执行文件**无法在非 Windows 上链接和运行（截图 BitBlt、
WASAPI、SendInput、TTS、托盘都是 Win32），`make dist` / UPX / Tauri bundle 同理。
`cargo test -p bitcat-app --lib` 能跑，是因为测试二进制只链接 lib，Win32 部分被 cfg 排除。

三个环境注意点：

- **Node 版本**：Node ≥ 22 的实验性全局 `localStorage` 会遮蔽 jsdom 实现（实测 Node 22
  容器里 `sprite-loader.test.js` 同样会红，**不是 Node 26 独有**）。`app/frontend/vitest.setup.js`
  已做兼容修补，因此本地任意 Node 版本都能跑通；CI 钉 Node 22 只为可复现。
- **时区**：前端测试已与运行环境时区解耦——`settings.test.js` 用 `localRfc3339()` 按本地
  时区构造 fixture，UTC / Asia-Shanghai / America-New_York / Pacific-Kiritimati 四个时区
  实测都是 187/187。CI 的 frontend job 故意钉 `TZ: UTC`，谁再引入时区相关断言就会立刻红；
  不要再把期望值写死成某个特定时区的墙上时间。
- **clippy 必须与 CI 同版本、同 target**：CI 用 `dtolnay/rust-toolchain@stable`，本地工具链
  落后就会出现“本地绿、CI 红”（实测踩过：本地 1.90 / CI 1.98，差一年，新 lint
  `chunks_exact_to_as_chunks` 在本地根本不存在）。而且 `audio_reactive.rs` 等处代码在
  `#[cfg(windows)]` 内，Linux 原生 clippy 看不到。本地要跑两条：

  ```bash
  rustup update stable                                    # 先对齐工具链版本
  cargo clippy --workspace --all-targets -- -D warnings   # 本机 target
  cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings
  ```

  交叉检查需要 `rustup target add x86_64-pc-windows-gnu` 和 mingw（Debian/Ubuntu：
  `sudo apt install mingw-w64`，SDL2 bundled 与 aws-lc-sys 的 build script 要跨编 C）。
  `check`/`clippy` 不链接，产物只有 `.rmeta`，整个 windows-gnu target 目录约 1GB。

### 不装 Windows 工具链也能拿到 exe

改完代码推到 GitHub，用 `dev-build` workflow 出一个 portable zip（约 5-10 分钟，
用 `[profile.dist]`：关 LTO、放开并行度，比 `release` 快很多），再下载到 Windows 上验收：

```bash
# 手动触发一次构建
gh workflow run dev-build.yml

# 取回最新一次的产物
gh run download "$(gh run list --workflow=dev-build.yml --limit=1 \
  --json databaseId --jq '.[0].databaseId')" \
  -n bitcat-dev-windows-x64 -D ./dist-dev
```

分工建议：编译正确性靠 push（`ci.yml` 在 windows-latest 上编译并测试整个 workspace，
同时有 ubuntu 的 core-only 护栏和前端 vitest job）；只有需要真机跑效果时才用
`dev-build` 取 exe。正式发布仍然走 tag 触发的 `release.yml`。

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
