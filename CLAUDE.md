# CLAUDE.md

本文件为 Claude Code (claude.ai/code) 提供项目指导。

## 构建与测试命令（通过 Makefile）

```bash
make build          # cargo build + 复制 yml 配置到 target
make release        # cargo build --release（opt-level=z, LTO, strip）

# ───── 测试（nextest 必装：cargo install cargo-nextest --locked）─────
make test-fast      # 最快反馈：core + 跳过 proptest（PROPTEST_CASES=32）
make test-core      # 只跑 core crate（跳过 SDL2/Tauri 编译，~20s）
make test-app       # 只跑 app crate
make test           # 完整 workspace（core + app，app 首次编译较慢）
make nextest        # 同 make test

# 前端测试（Vitest + jsdom）
cd app/frontend && npx vitest run     # 3 个测试文件
cd app/frontend && npx vitest         # 监听模式

# 运行单个测试 / 模块
cargo nextest run -p ai-pad-core -E 'test(~pet::)'        # 按名字过滤
cargo nextest run -p ai-pad-core -E 'test(/test_walk_/)'  # 按正则过滤
cargo test -p ai-pad-app --features ipc-tests             # Tauri IPC 集成测试（需 WebView2）

# Insta 快照工作流
cargo insta test       # 运行测试，生成 .snap.new（未审查的快照）
cargo insta review     # 交互式审查，逐个接受/拒绝快照变更
cargo insta accept     # 一键接受所有新快照
```

**nextest 配置**：见 [.config/nextest.toml](.config/nextest.toml)。`default` profile 用于本地（安静输出 + slow-timeout 保护），`ci` profile 用于 GitHub Actions（JUnit 输出 + fail-fast）。`PROPTEST_CASES` 环境变量可覆盖 proptest 用例数（默认 256，CI 用 64）。

**Git hooks（cargo-husky）**：首次 `cargo test` 或 `make install-hooks` 会自动把 [.cargo-husky/hooks/](.cargo-husky/hooks/) 里的脚本写入 `.git/hooks/`：
- **pre-commit**：`cargo fmt --all -- --check`（仅当暂存区有 `.rs` 变更时跑，秒级）
- **pre-push**：fmt + `cargo clippy --workspace -- -D warnings` + `make test-fast`（约 30s-1min）

跳过单次：`git commit --no-verify` / `git push --no-verify`。完全禁用安装：`CARGO_HUSKY_DONT_INSTALL_HOOKS=true cargo test`。

**Windows SDL2 构建必须设置环境变量**（VS2026 + 新 CMake 兼容）：
```powershell
$env:CMAKE_POLICY_VERSION_MINIMUM="3.5"; make build
```

### 当前推荐开发习惯

`Makefile` 会导出 `CMAKE_POLICY_VERSION_MINIMUM=3.5`，所以日常优先走 `make`，不要直接手写零散的 `cargo build` + 复制配置 + 压缩命令。

- 快速改 core 逻辑：`make test-core` 或 `make test-fast`
- 改 app/Tauri/SDL2 相关逻辑：`make build`，提交前再跑 `make test-app` 或 `make test`
- 本地运行：`make run`；需要完整静态 SDL2 构建前，可先跑 `make build`
- 便携包：`make dist`，会先 `make release`，再通过 `xtask` 生成 `ai-pad-<version>-windows-x64-portable.zip`
- UPX 便携包：`make dist-upx`，只建议用于 portable zip；安装包保持 Tauri bundle 原样
- CI 发布：`.github/workflows/release.yml` 也调用同一个 `xtask package-portable`，本地和 CI 的 portable 清单保持一致

portable zip 标准内容：`ai-pad-app.exe` + `config/*.yml`。SDL2 通过 `sdl2 = { features = ["bundled", "static-link"] }` 静态链接进 exe，不再随包复制 `SDL2.dll`。如需直接使用 Cargo 命令绕过 Makefile，Windows 下仍需手动设置：

```powershell
$env:CMAKE_POLICY_VERSION_MINIMUM="3.5"
cargo check -p ai-pad-app
```

### 打包工具约定

仓库使用 `xtask` 承载项目级维护命令，避免把发布逻辑散落在 shell/PowerShell 里。

```bash
cargo run -p xtask -- package-portable --version v0.1.0 --release-dir target/release --out-dir .
```

`make dist`、`make dist-upx` 和 GitHub Release workflow 都必须调用这条 Rust 工具链路径；不要新增第二套 zip 复制逻辑。

## 架构

Rust workspace（`core` + `app` + `xtask`），Tauri 2.0 多窗口桌面应用。无 npm 打包，前端是纯静态 HTML/JS/CSS。

### Workspace 结构

| Crate | 职责 | 关键依赖 |
|-------|------|----------|
| **core**（`ai-pad-core`） | 纯逻辑，零 UI 依赖，可独立单测 | rig-core(AI), serde_yaml, windows-sys |
| **app**（`ai-pad-app`） | Tauri 2.0 壳：窗口管理、手柄循环、IPC、托盘 | tauri 2, sdl2(bundled + static-link), tokio |
| **xtask** | 仓库维护工具：portable zip 打包等项目级命令 | zip |

### 核心数据流

```
SDL2 手柄输入 → gamepad_loop() [80ms tick, lib.rs]
  ├── bridge::handle_button_press() → PetCommand
  │     └── agent.chat_stream() → 流式 AI 回复 → bubble 窗口
  ├── voice 按住/释放 → voice.rs（generation 防残留）→ AI 对话
  ├── panel 导航 → panel.rs（方向键/A/B 独占）
  └── config/actions.yml 热键/启动 → hotkey.rs SendInput

截图观察线程 → screenshot_loop() [screenshot.rs, 独立线程]
  ├── BitBlt 截屏 → dHash 去重 → 缩放 → JPEG 编码
  ├── vision.rs → Vision API 分析 → 描述文本 → bubble 显示
  └── 存储 ~/.ai-pad/screenshots/YYYY-MM-DD/（7 天自动清理）
```

### 多窗口模型（4 个 WebView2 窗口）

- **pet** — 128×128 透明窗口，Canvas 像素精灵动画
- **bubble** — 动态高度（140→340px）透明窗口，AI 流式文本渲染。短文本固定 140px；长文本自动加高并上移避免遮挡宠物；超长文本（>340px）内部可滚轮翻阅（wheel 事件 JS 兜底 scrollTop，因 Tauri 透明窗口 native scroll 不稳定）
- **panel** — 480×320 玻璃面板，3×2 网格，方向键导航
- **voice** — 280×40 录音条，textarea 接收 IME 注入（预创建在屏幕外）

### Voice 输入防残留机制

`SharedVoice` 使用 `VoiceEntry { text, generation }` 结构。每次 `open_voice_capture()` 递增 generation 并清空；`cmd_voice_update_text()` 写入时附带当前 gen；`take_voice_text()` 只接受 `entry.generation == current_gen` 的文本，旧会话残留会被 warn 日志拒绝。

### IPC 通信模式

- Rust→JS：`app.emit("event-name", payload)` — JS 用 `window.__TAURI__.event.listen()` 接收
- JS→Rust：`window.__TAURI__.core.invoke("cmd_xxx", args)` — Rust 用 `#[tauri::command]` 注册
- 共享状态：`tauri::State<'_, SharedXxx>` + `Mutex<T>` 在命令间传递

### AI Agent

配置优先级：环境变量 > `~/.claude/settings.json` > `.env` > 默认值。4 个内置 Tool：launch_program / shell / read_file / get_time。max_tokens 固定 256K。

### 截图观察系统

独立线程定时截图（默认 30s），流程：BitBlt 捕获 → 熄屏检测（`SM_MONITORISOFF` + 全黑帧采样）→ dHash 去重 → 缩放到 max_width → JPEG 编码 → Vision API（Anthropic Messages）分析 → 结果通过 bubble 显示并保存到 `~/.ai-pad/screenshots/`。支持多显示器水平拼接 + 调试多分辨率对比。配置在 `config/prompts.yml` 的 `screen_summary` 段。

### 记忆系统

`MemoryStore` 维护滚动窗口对话记忆（默认 20 条），持久化到 `~/.ai-pad/memory/chat_summary.json`。每次 AI 对话后记录 user_msg + ai_reply（按字符截断），下次对话时通过 `build_context()` 注入 prompt。配置在 `config/prompts.yml` 的 `memory` 段。

### 用户画像

`UserProfile` 从 `config/user.yml` 加载用户显式声明的身份信息（name/role/preferences/context/language），通过 `build_context()` 生成 `[关于主人]...[/关于主人]` 注入 prompt。**优先级高于** `ProfileStore` 的自动聚合画像：user.yml 有内容时直接使用，全空时才回退到聚合结果。设置窗口可编辑，支持重置为默认。

### Prompts 配置

`config/prompts.yml` 统一管理四段提示词：`agent.preamble`（AI 人设）、`vision.prompt`/`vision.prompt_multi`（截图分析提示词，强调反幻觉）、`memory`（记忆窗口大小和截断阈值）、`screen_summary`（截图摘要注入配置）。所有字段有编译时默认值，YAML 可选覆盖。运行时从 exe 同目录/config/ 加载，构建时需 cp 到 target/config/。

### 日志与 .env

日志双写：stderr（带颜色）+ 文件 `~/.ai-pad/logs/`（按日滚动，默认 `ai_pad_app=info,ai_pad_core=debug`）。`.env` 多级加载：exe 同目录 → CWD → 项目根目录（兜底 target/debug 向上两级）。

## 编码规范

- **日志**：统一用 `tracing` crate（info/warn/error/debug），不用 `eprintln!`
- **中文处理**：Rust 中字符串切片必须按字符边界（`.chars().take(n)`），不可用字节索引
- **前端**：无框架，IIFE 模块，通过 `window.__TAURI__` API 与后端通信
- **配置**：`config/actions.yml`、`config/buttons.yml`、`config/prompts.yml` 运行时从 exe 同目录/config/ 加载，构建时需 cp 到 target/config/

## 测试规范

### 测试框架

| 框架 | 用途 | 所在 crate |
|------|------|-----------|
| `insta`（yaml+redactions） | 快照测试：serde 序列化、API 请求体、配置解析 | core |
| `rstest` | 参数化测试 + 测试夹具，替代重复的 test 函数 | core |
| `wiremock` | HTTP mock：模拟 Anthropic API 响应 | core |
| `mockall` | Trait mock（预留，暂未使用） | core |
| `proptest` | 属性测试：状态机、边界条件 | core |
| `tauri::test` | MockRuntime IPC 测试，需 `ipc-tests` feature | app |
| `vitest` + `jsdom` | 前端单元测试 | frontend |

### 写测试的规则

1. **序列化/反序列化测试用 insta 快照**，不要手写 `assert!(json.contains(...))`。快照自动捕获完整结构，字段增删一目了然。
   ```rust
   // 正确：快照捕获完整结构
   insta::assert_yaml_snapshot!(record);
   // 正确：动态字段用 redaction 替换
   insta::assert_yaml_snapshot!(body, { ".messages[0].content[0].text" => "[prompt]" });
   // 错误：手动逐字段断言，新增字段时不会报警
   assert_eq!(body["model"], "claude-sonnet-4-20250514");
   ```

2. **同函数多输入的测试用 rstest 参数化**，不要写 N 个 `#[test] fn test_parse_X`。
   ```rust
   #[rstest]
   #[case(&["ctrl", "win"], vec![0x11, 0x5B])]
   #[case(&["enter"], vec![0x0D])]
   fn test_parse_keys(#[case] keys: &[&str], #[case] expected: Vec<u16>) { ... }
   ```

3. **外部 API 调用用 wiremock mock**，不要跳过测试或依赖真实网络。wiremock 测试用 `#[tokio::test]`，client 需加 `.no_proxy()` 避免 Windows 系统代理干扰 localhost。
   ```rust
   let server = MockServer::start().await;
   let client = reqwest::Client::builder().no_proxy().build().unwrap();
   ```

4. **状态机/边界测试用 proptest 属性测试**，已有 `pet::prop_tests` 模块。适用于"对任意合法输入都不 panic"的场景。

5. **insta 快照文件（`*.snap`）必须提交到 git**，它们是测试的基线。`.snap.new` 文件不应提交（已在 `.gitignore`）。修改序列化格式后运行 `cargo insta review` 审查变更。

6. **测试内联在源文件底部**（`#[cfg(test)] mod tests`），不单独建 `tests/` 目录。快照文件在 `core/src/snapshots/`。

### 测试性能最佳实践

1. **日常只跑 core**：`make test-core`（~20s），app crate 依赖 SDL2/Tauri 编译慢，提交前再跑 `make test`。
2. **async 测试用 `#[tokio::test]`**，不要手动 `Runtime::new().unwrap().block_on(...)`——每个测试新建 runtime 有额外开销，代码也更冗长。
3. **nextest fail-fast 默认开启**：本地调试失败时加 `--no-fail-fast` 看全部失败，别误以为"只有一个测试失败"。
4. **proptest 用例数可配**：本地 `PROPTEST_CASES=32 make test-fast`，CI 默认 64，完整回归 256（默认）。
5. **改了 `wiremock` 相关测试记得 `.no_proxy()`**：Windows 系统代理会劫持 `localhost`，不加会 hang。
6. **环境变量相关测试自动进串行组**：[.config/nextest.toml](.config/nextest.toml) 的 `serial-env` test-group 已经把 `*from_env*` / `*env_overrides*` 的测试串行化，新增此类测试会自动受益，无需手动加 `#[serial]`。

## 关键文件

- `app/src/lib.rs` — 主入口，gamepad_loop（~520 行）+ 右键菜单（Win32 TrackPopupMenu）+ .env 加载 + 全局热键注册
- `app/src/main.rs` — --debug 控制台分配 + 日志双写（stderr + `~/.ai-pad/logs/` 按日滚动）
- `app/src/screenshot.rs` — 截图观察线程：BitBlt 截屏 + 熄屏检测 + 缩放 JPEG + Vision API 调用 + 存储/清理
- `core/src/pet.rs` — 宠物状态机（6 状态，帧动画，proptest 属性测试）
- `core/src/agent.rs` — AI Agent 流式对话 + Tool 定义
- `core/src/bridge.rs` — 按键→命令映射，PetCommand 序列化
- `core/src/screenshot.rs` — 截图类型定义、dHash 感知哈希、resize/JPEG 编码、截图存储 + 清理
- `core/src/vision.rs` — Vision API 请求构建/响应解析（Anthropic Messages 图片分析）
- `core/src/memory.rs` — 滚动窗口对话记忆，`~/.ai-pad/memory/` 持久化
- `core/src/prompts.rs` — 统一提示词配置加载（agent/vision/memory），prompts.yml 解析
- `core/src/user_profile.rs` — 用户画像配置（name/role/preferences），user.yml 解析，优先于自动聚合画像
- `app/src/voice.rs` — 语音输入窗口 + generation 防残留
- `app/src/bubble.rs` — 独立气泡窗口 + 流式 chunk 协议
- `app/frontend/js/app.js` — 宠物窗口主逻辑（拖拽、状态同步、精灵渲染）
