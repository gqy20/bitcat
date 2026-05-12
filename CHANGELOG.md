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
