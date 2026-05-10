# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands (via Makefile)

```bash
make build          # cargo build + copy yml configs to target
make release        # cargo build --release (opt-level=z, LTO, strip)
make test           # cargo test (workspace, copies yml for core tests)
make read / make ctl # cargo run (debug, same binary: ai-pad-app)
make check          # cargo check
make clippy         # cargo clippy -- -W clippy::all
make clean          # cargo clean

# Frontend tests (Vitest + jsdom)
cd app/frontend && npx vitest run     # 56 tests in 3 files
cd app/frontend && npx vitest         # watch mode

# Run single test
cargo test -p ai-pad-core pet::tests::test_walk_moves   # core 单测
cargo test -p ai-pad-app voice::tests                    # app 单测
cargo test -p ai-pad-app --features ipc-tests            # Tauri IPC 集成测试(需 WebView2)
```

**Windows SDL2 构建必须设置环境变量**（VS2026 + 新 CMake 兼容）：
```powershell
$env:CMAKE_POLICY_VERSION_MINIMUM="3.5"; make build
```

## Architecture

Rust workspace (`core` + `app`)，Tauri 2.0 多窗口桌面应用。无 npm 打包，前端是纯静态 HTML/JS/CSS。

### Workspace 结构

| Crate | 职责 | 关键依赖 |
|-------|------|----------|
| **core** (`ai-pad-core`) | 纯逻辑，零 UI 依赖，可独立单测 | rig-core(AI), serde_yaml, windows-sys |
| **app** (`ai-pad-app`) | Tauri 2.0 壳：窗口管理、手柄循环、IPC、托盘 | tauri 2, sdl2(bundled), tokio |

### 核心数据流

```
SDL2 手柄输入 → gamepad_loop() [80ms tick, lib.rs]
  ├── bridge::handle_button_press() → PetCommand
  │     └── agent.chat_stream() → 流式 AI 回复 → bubble 窗口
  ├── voice 按住/释放 → voice.rs (generation 防残留) → AI 对话
  ├── panel 导航 → panel.rs (方向键/A/B 独占)
  └── actions.yml 热键/启动 → hotkey.rs SendInput
```

### 多窗口模型（4 个 WebView2 窗口）

- **pet** — 128×128 透明窗口，Canvas 像素精灵动画
- **bubble** — 动态高度（140→340px）透明窗口，AI 流式文本渲染。短文本固定 140px；长文本自动加高并上移避免遮挡宠物；超长文本（>340px）内部可滚轮翻阅（wheel 事件 JS 兜底 scrollTop，因 Tauri 透明窗口 native scroll 不稳定）
- **panel** — 480×320 玻璃面板，3×2 网格，方向键导航
- **voice** — 280×40 录音条，textarea 接收 IME 注入（预创建在屏幕外）

### Voice 输入防残留机制

`SharedVoice` 使用 `VoiceEntry { text, generation }` 结构。每次 `open_voice_capture()` 递增 generation 并清空；`cmd_voice_update_text()` 写入时附带当前 gen；`take_voice_text()` 只接受 `entry.generation == current_gen` 的文本，旧会话残留会被 warn 日志拒绝。

### IPC 通信模式

- Rust→JS: `app.emit("event-name", payload)` — JS 用 `window.__TAURI__.event.listen()` 接收
- JS→Rust: `window.__TAURI__.core.invoke("cmd_xxx", args)` — Rust 用 `#[tauri::command]` 注册
- 共享状态: `tauri::State<'_, SharedXxx>` + `Mutex<T>` 在命令间传递

### AI Agent

配置优先级: 环境变量 > `~/.claude/settings.json` > 默认值。4 个内置 Tool: launch_program / shell / read_file / get_time。max_tokens 固定 256K。

## Code Conventions

- **日志**: 统一用 `tracing` crate（info/warn/error/debug），不用 `eprintln!`
- **测试**: core 用 proptest 属性测试 + 单元测试；app 用纯函数测试 + 可选 feature 的 Tauri MockRuntime IPC 测试；前端用 Vitest (jsdom)
- **中文处理**: Rust 中字符串切片必须按字符边界（`.chars().take(n)`），不可用字节索引
- **前端**: 无框架，IIFE 模块，通过 `window.__TAURI__` API 与后端通信
- **配置**: `actions.yml` 和 `buttons.yml` 运行时从 exe 同目录加载，构建时需 cp 到 target/

## Key Files

- `app/src/lib.rs` — 主入口，gamepad_loop 全部逻辑（~500 行），state 管理，invoke_handler 注册
- `core/src/pet.rs` — 宠物状态机（6 状态，帧动画，proptest 属性测试）
- `core/src/agent.rs` — AI Agent 流式对话 + Tool 定义
- `core/src/bridge.rs` — 按键→命令映射，PetCommand 序列化
- `app/src/voice.rs` — 语音输入窗口 + generation 防残留
- `app/src/bubble.rs` — 独立气泡窗口 + 流式 chunk 协议
- `app/frontend/js/app.js` — 宠物窗口主逻辑（拖拽、状态同步、精灵渲染）
