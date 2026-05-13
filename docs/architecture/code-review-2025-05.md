# 代码质量审查报告 (2025-05)

全量代码审查，覆盖 core crate、app crate 和跨层架构。按优先级 P0–P6 分级。

---

## P0 — 确定性 Bug

### 1. `user_profile.rs:90` — language 字段误用 role 值

```rust
// 错误：language 非空时取的是 self.role.as_str()
(!self.language.is_empty()).then_some(self.role.as_str()),
```

应改为 `self.language.as_str()`。`parts` 中间变量与后续 `lines` 构建逻辑重复，可一并删除。

### 2. `screenshot.rs:50-53` — GetDC 泄漏 HDC

```rust
let hdc_screen = GetDC(std::ptr::null_mut());
// 从未调用 ReleaseDC(null_mut(), hdc_screen)
```

每次截图泄漏一个 GDI 对象。需在函数末尾补 `ReleaseDC`，或用 RAII 封装。

---

## P1 — 安全性 / Soundness

### 3. `screenshot.rs:125,220` — static mut 裸指针

```rust
static mut FRAMES_PTR: *const std::sync::Mutex<Vec<(i32, CapturedFrame)>> = std::ptr::null();
static mut DISPLAYS_PTR: *const std::sync::Mutex<Vec<ScreenInfo>> = std::ptr::null();
```

多线程访问为 UB。替换为安全替代：

```rust
static FRAMES_PTR: std::sync::OnceLock<std::sync::Mutex<Vec<(i32, CapturedFrame)>>> =
    std::sync::OnceLock::new();
```

### 4. `voice.rs:47-51` — generation/entry 竞态

`open_voice_capture` 先 drop generation 锁再获取 entry 锁，中间窗口期 `cmd_voice_update_text` 可用新 generation 写入旧文本。合并为单一 Mutex：

```rust
pub struct SharedVoice {
    inner: Mutex<VoiceInner>,
}
struct VoiceInner {
    entry: VoiceEntry,
    generation: u64,
}
```

### 5. `config/actions.yml` — 硬编码个人路径

已提交到 git 的配置包含 `D:\C\Desktop\ai` 和 `--dangerously-skip-permissions`。应改为 `config/actions.example.yml` 模板，原文件加入 `.gitignore`。

---

## P2 — 架构违规

### 6. core crate 依赖 windows-sys

core 定位为"纯逻辑，零 UI 依赖，可独立单测"，但 `hotkey.rs` 直接调用 `SendInput`。将平台相关的 SendInput 执行逻辑移到 app crate，core 只保留按键名 → VK Code 映射表（纯数据）。

### 7. `bridge.rs:179-211` — 关键词情绪匹配

`resolve_agent_response` 用硬编码中文关键词（"错误"、"哈哈"、"喵"）判断 AI 回复情绪。CLAUDE.md 明确禁止这种模式。应改为让 AI 通过结构化输出附带情绪标签。

### 8. workspace 缺少统一依赖管理

`serde`、`tokio`、`tracing`、`windows-sys` 等在各 crate 各自声明版本号。应添加 `[workspace.dependencies]` 统一版本，子 crate 用 `serde.workspace = true`。

### 9. edition 不一致

core 用 `edition = "2024"`，app 和 xtask 用 `edition = "2021"`。应统一。

---

## P3 — DRY 违反 / 重复代码

### 10. `memory.rs` 三份 load/save 重复

`MemoryStore`、`LongTermMemory`、`ProfileStore` 的 `load()` 和 `save()` 逻辑几乎相同（~90 行重复）。提取泛型 trait：

```rust
trait JsonStore: serde::de::DeserializeOwned + serde::Serialize + Default {
    fn file_path() -> Result<PathBuf, String>;
    fn load() -> Self { /* 通用加载 */ }
    fn save(&self) -> Result<(), String> { /* 通用保存 */ }
}
```

### 11. launch/script 动作执行三处重复

`panel.rs`、`gamepad.rs`、`action_bus.rs` 各有一份 launch/script match + spawn powershell 逻辑。统一到 `ActionBus::dispatch` 或专用 `ActionExecutor`。

### 12. bubble 窗口创建重复

`precreate_bubble_window` 和 `create_bubble_window` 配置几乎相同。前者应复用后者再添加 subclass。

### 13. 活跃 pet 窗口查找重复

`bubble.rs` 和 `panel.rs` 都有 pet/pet-mini/pet-snap 三层 fallback 查找，且 panel.rs 漏了 mini/snap。提取公共函数：

```rust
pub fn find_active_pet_window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    ["pet", "pet-mini", "pet-snap"]
        .iter()
        .find_map(|label| {
            app.get_webview_window(*label)
                .filter(|w| w.is_visible().unwrap_or(false))
        })
}
```

### 14. `prompts.rs` 重复 include_str

`EMBEDDED_YML` 和 `DEFAULT_PROMPTS_YML` 指向同一文件。删除 `DEFAULT_PROMPTS_YML`，统一使用 `EMBEDDED_YML`。

---

## P4 — 并发 / 性能

### 15. SharedChatCore 5 个独立 Mutex

`gamepad.rs:183-194`，聚合时同时持有 2-3 个锁。合并为单一 `Mutex<ChatCoreInner>` 或迁移 `parking_lot::Mutex`（不中毒、性能更好）。

### 16. chat_loop 80ms 空转轮询

`gamepad.rs:719-839`，无消息时纯空转。改为 `std::sync::mpsc::channel` + `recv_timeout`，新消息自动唤醒。

### 17. bubble_follower 50ms 空转

`bubble.rs:375-408`，气泡隐藏时也持续轮询。改为条件变量 / channel 唤醒，或隐藏时增大间隔到 500ms。

### 18. Vec::remove(0) 循环 — O(n²)

`memory.rs:150-152`，`Vec::remove(0)` 每次左移所有元素。改用 `Vec::drain`：

```rust
if self.entries.len() > config.max_entries {
    self.entries.drain(0..self.entries.len() - config.max_entries);
}
```

### 19. build_context chars().count() 循环 — O(n²)

`memory.rs:175-180`，每次迭代重新扫描全串。维护运行中的字符计数器：

```rust
let mut used_chars = result.chars().count();
for line in &lines {
    let line_chars = line.chars().count() + 1;
    if used_chars + line_chars > config.max_context_chars { break; }
    result.push_str(line);
    result.push('\n');
    used_chars += line_chars;
}
```

同样的问题存在于 `LongTermMemory::retrieve()` 和 `screenshot.rs` 的 `build_recent_analyses_context_with_base()` 中。

### 20. agent.rs 日志 len() 返回字节数

```rust
// 错误：len() 返回字节数，不是字符数
chars = res.response().len(),
// 应改为
chars = res.response().chars().count(),
```

违反项目 logging 规范。

---

## P5 — 错误处理

### 21. 全局 Result<_, String> 无类型化错误

core 所有模块用 `String` 作为错误类型，丢失错误链、无法按类型匹配。引入 `thiserror` 枚举：

```rust
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("序列化失败: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("AI 请求失败: {0}")]
    AiRequest(String),
}
```

可逐模块渐进替换。

### 22. Mutex 中毒处理不一致

三种策略并存：`unwrap()` panic、静默忽略、`return` 跳过。建议统一迁移 `parking_lot::Mutex`（不中毒），或至少统一使用 `unwrap_or_else(|e| e.into_inner())`。

---

## P6 — 可维护性

### 23. lib.rs setup 闭包 265 行

`lib.rs:110-375`，承担 .env 加载、托盘创建、窗口预创建、热键注册、后台线程 spawn 等十多项职责。拆分为 `load_env`、`spawn_dance_bridge`、`register_hotkeys`、`spawn_background_threads` 等独立函数。

### 24. gamepad_loop 内层 230 行 / 嵌套 5-6 层

`gamepad.rs:458-692`。将按钮处理、voice 检测、方向键处理提取为独立函数。

### 25. panel.js 按钮硬编码

`panel.js:2-9`，面板 6 个按钮完全硬编码，与 `actions.yml`/`buttons.yml` 不同步。应在 init 时从后端加载。

### 26. 日志中英文混用

同一模块内中英文日志交替出现。建议统一为一种语言（推荐中文，与团队和用户一致）。

### 27. Pet struct 字段过度公开

`pet.rs:65-81`，所有字段 `pub`，外部可绕过 `set_state()` 校验。将 `frame`、`frame_time_ms`、`state_time_ms` 改为私有，暴露只读访问器。

---

## 值得保持的设计

- **core/app 分离** — 纯逻辑层可独立编译测试（~20s）
- **RAII ChatActiveGuard** — panic 安全的截图/对话互斥
- **insta 快照 + rstest 参数化** — 测试风格一致，快照变更可审计
- **xtask 集中项目命令** — 打包、测试、配置复制统一入口
- **Pull 模式替代 Push** — 前端 init 时从 Rust 拉取状态，规避 Tauri 事件竞态

---

## 建议修复顺序

1. **P0 bug** — user_profile 字段引用 + screenshot HDC 泄漏（一行修复）
2. **P1 安全** — static mut → OnceLock、SharedVoice 合并 Mutex、actions.yml 脱敏
3. **P3 DRY** — memory.rs JsonStore trait（消除 ~90 行重复，收益最大）
4. **P4 并发** — SharedChatCore 合并 Mutex + chat_loop 改 channel
5. **P2 架构** — hotkey.rs 移到 app crate、bridge 情绪匹配改结构化输出
6. **P5 错误处理** — thiserror 类型化错误（可逐模块渐进）
7. **P6 可维护性** — lib.rs 拆分、gamepad_loop 提取子函数
