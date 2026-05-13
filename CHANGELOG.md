# 更新日志

项目使用带 `v` 前缀的语义化版本标签，格式参考 [Keep a Changelog](https://keepachangelog.com/)。

## [Unreleased]

### 新增

### 修复

### 变更

### 性能

### 移除

### 工具链

### 文档

---

## [0.1.1] - 2026-05-13

0.1.0 后的稳定性与体验更新，重点补齐用量观测、工具状态、手动截图、迷你游戏和 Windows 打包链路。

### 新增

**截图与观察**
- 新增手动截图入口：系统托盘、宠物左眼双击、`actions.yml` 的 `screenshot` 动作和 `keyboard_shortcut` 全局热键均可立即触发 Vision 分析。
- 默认 `R2` 绑定为立即截图，默认热键为 `CommandOrControl+Alt+S`。
- 截图记录升级为结构化存储，保存分析状态、摘要和上下文，方便后续检索与摘要注入。
- 多显示器截图按虚拟桌面坐标排序拼接，避免左右屏顺序错乱。

**小游戏**
- 新增可玩的 Snake 迷你游戏切片（`game` 窗口、`game_engine.js`、`GameDef` schema）。
- 面板可启动内置「毛线球大作战」，游戏中宠物进入 `GamePlay` 状态，结束后根据胜负切换表现。
- 新增小游戏配置校验，限制网格、速度、胜利长度和主题枚举，防止异常配置卡死或越界。

**Token 与工具观测**
- AI 调用开始记录 token 用量到 `~/.ai-pad/logs/token_usage.jsonl`，并聚合最近会话到 `token_sessions.json`。
- 设置窗口新增「用量」视图，展示今日汇总、最近 session、Chat/Vision/ScreenSummary/MemoryAggregation 分类统计。
- Agent 工具调用新增运行时事件：计划、完成、失败、被阻止等状态可推送到气泡窗口。
- 工具事件审计日志写入 `~/.ai-pad/logs/tool_events.jsonl`，便于排查工具链行为。

**气泡与交互**
- 气泡窗口支持手动拖拽调整大小，并保留用户手动尺寸偏好。
- 气泡可在 AI 调用工具期间展示工具状态，性能类工具（如舞蹈）会有阶段提示。
- 系统托盘右键菜单文案优化，截图、折叠、置顶、设置等入口更清晰。

**设置与外观**
- 设置窗口进行视觉打磨，新增本地字体资源，信息层级和表单布局更精致。
- 设置窗口动作编辑器支持 `screenshot` 动作类型和键盘热键字段。

### 修复

- 修复气泡手动 resize 后定位和尺寸偏好丢失的问题。
- 修复气泡在工具调用期间过早结束流式状态的问题。
- 修复气泡在多显示器环境中的放置和宠物遮挡问题。
- 修复贴边吸附只支持左右边的问题，扩展为四边吸附。
- 修复默认 TTS 行为：默认关闭，并尊重 `tts_enabled` 设置。
- 修复舞蹈 repeat 步骤未被正确播放的问题，并放大默认舞蹈动作幅度。
- 修复 Vision 结构化状态的 JsonSchema 生成问题，避免 `oneOf` 兼容性风险。
- 修复无 API key 的配置测试在 CI 中失败的问题，改为优雅跳过。
- 修复 pre-commit hook 对未暂存 Rust 文件也跑 fmt 的问题。

### 变更

- ActionBus 归一化三路输入，手柄、面板、快捷键等动作走统一分发路径。
- Agent 工具参数 schema 改为由类型派生，减少手写 JSON schema 样板。
- Vision、屏幕摘要、记忆画像改为走 rig extractor 的结构化输出路径，并移除旧 extractor fallback。
- 宠物动画引擎支持非均匀帧时长、瞬态 repeat 和 fallback 状态。
- 日志输出进一步规范化，降低聊天、截图、ActionBus 等高频路径噪声。
- `play_dance` / `perform_dance` 的 schema 文案收敛，减少 prompt token 占用。

### 工具链

- portable zip 打包统一迁移到 `xtask package-portable`，`make dist` / `make dist-upx` / Release workflow 共用同一条 Rust 路径。
- `make test-core` / `make test-fast` / `make test-app` / `make test` 统一委托 `xtask`，兼容 PowerShell/cmd/Git Bash。
- `make run` 标准化，日常运行优先通过 Makefile。
- 新增/更新 cargo-husky、nextest、提交信息规范和 Windows SDL2 构建说明。

### 文档

- 新增用户指南：AI 对话、配置、舞蹈、手柄、入门、截图、排障。
- 新增 grep-first 记忆检索取舍说明，明确不把 Embeddings / Vector RAG 作为当前主线。
- 新增 structured vision cleanup、rig capability roadmap、tool runtime、动画优化和小游戏计划文档。
- 新增 Steam 桌宠竞品分析、代码审查报告、临时产物目录约定。
- 为 core/app 源文件补齐模块文档和公共 API 注释。

---

## [0.1.0] - 2026-05-13

首个公开版本。

### 新增

**舞蹈系统**
- AI 工具调用生成舞蹈定义（`create_dance` tool）+ YML 持久化到 `~/.ai-pad/dances/`
- 前端 dancePlayer 播放器：按时间轴切换 sprite 动作帧（jump/spin/wave/shake/idle），支持循环和时长参数
- 手柄 Y 键 / Panel 按钮直接触发舞蹈，播放期间自动暂停截图管线
- 窗口级动画（screen-ratio 相对坐标，不再依赖绝对像素）

**记忆与画像**
- 两层存储记忆系统：短期滚动窗口（默认 20 条）+ 长期 AI 聚合画像（`~/.ai-pad/memory/chat_summary.json`）
- 用户画像配置 `config/user.yml`：显式声明 name/role/preferences，优先于自动聚合结果
- 截图原始分析记录注入 prompt（最近 10 条），屏幕活动摘要定时总结并注入上下文

**设置系统**
- 独立设置窗口（Tauri 多窗口）：按键绑定编辑、按钮目录展示、app_settings 运行时覆盖层
- 按键绑定支持未绑定状态可视化，button_catalog 展示完整手柄按钮集
- 配置文件嵌入 exe 二进制 + 多路径查找（exe 同目录 → CWD → 项目根目录）

**语音合成**
- Windows SAPI TTS 语音合成，AI 回复完成后自动朗读

**吸附增强**
- 吸附预览（拖近边缘时实时预览竖条形态）+ Crossfade 平滑过渡动画
- snap_edge 状态同步：吸附/展开/折叠状态在多窗口间一致
- 多层发光效果 + hover 热区扩展 + 品牌色统一

**AI 对话改进**
- 对话消费从手柄循环解耦为独立 `chat_loop` 线程，不再阻塞 80ms tick
- 输入焦点检测（`cmd_enter_chat` / `cmd_exit_chat`）：聊天期间自动锁定截图管线
- Agent max_turns=16 启用多轮工具调用

**工具系统扩展**
- 扩展至 9 个工具：新增 clipboard / foreground / recent_screenshots / create_dance / play_dance
- PermissionHook 安全加固：危险操作（shell/launch/hotkey）需用户确认

**可观测性**
- Agent tracing span：完整记录工具调用链路（输入参数 → 执行 → 输出结果）
- 结构化日志规范（`.claude/rules/logging.md`）：五级定义 + 大文本截断规则 + instrument 指南

### 修复

- **光标残留系列**：用 streaming 标志位彻底消除拖拽后光标残留（4 次递进修复）
- **气泡显示**：第二次不显示（eval 替代 emit_to）、首次创建文本丢失（init 先消费 pending_text）、自动隐藏延长至 15s
- **截图气泡链路**：首次显示文本丢失、WM_MOUSEWHEEL 转发（Win32 Subclass）、键盘滚动兜底
- **语音输入**：文本累积修复（generation 防残留机制）
- **拖拽坐标**：DPI 缩放修正
- **测试期望**：3 个本地测试用例断言值修正
- **Clippy**：全 workspace clippy warning 清零（unblock pre-push）

### 变更

- **配置重构**：yml 文件归入 `config/` 子目录；prompts 全部消除硬编码默认值，单一数据源来自 YAML
- **代码拆分**：lib.rs 和 screenshot.rs 大文件拆分（snap.rs / screenshot_tests.rs 独立）
- **气泡 UI**：CSS 精简 + JS 交互增强（Markdown 渲染 / 滚动 / 点击交互）

### 工具链

- **Git Hooks**：cargo-husky 接入 — pre-commit 做 fmt 检查，pre-push 做 clippy + test（~30s）
- **CI/CD**：GitHub Actions release 工作流 — workflow_dispatch 手动触发、并发控制、checksum、CHANGELOG 自动解析、Tauri 多平台打包
- **测试体系**：insta 快照 / rstest 参数化 / wiremock mock / proptest 属性测试 / Vitest 前端（293 tests passed）
- **Makefile**：重构为统一构建入口（build/release/test-fast/test-core/test-app）

### 文档

- **Roadmap**：重组为 Track A/B/C/D 四轨战略总览，覆盖所有 plan 文档方向
- **设计文档**：
  - 结构化输出设计（rig TypedPrompt 路径：DanceDef / GameDef）
  - Rig 能力升级路线图（Extractor / dynamic_tools / Embeddings RAG）
  - Token 全链路追踪方案
  - 日志体系规范化设计
  - 3D 体素架构方案（Three.js + 组合式部位模型）
  - Prompt Token Budget 分析
  - Vision API 设计文档
- **GDD 系列**：世界观、战斗系统、玩家角色、评分体系、数据架构、社区体系、Hub World 训练场
