# Prompt Token 预算分析

> 统计日期：2026-05-13 | 基于源码默认值（`config/prompts.yml` + `core/src/agent.rs`）

## 概览

| 模块 | 字符数 | 估算 Token | 占比 | 调用时机 |
|------|--------|-----------|------|---------|
| Agent Preamble (系统提示词) | 158 | ~45 | 4.8% | 每次对话必带 |
| Vision Prompt (截图分析) | 220 | ~63 | 6.7% | 截图观察时 |
| Vision Multi-Monitor 后缀 | 54 | ~15 | 1.6% | 多屏拼接时追加 |
| Screen Summary Prompt (屏幕摘要) | 173 | ~49 | 5.3% | 摘要聚合时 |
| **Tools (x10)** (工具定义) | 待重新统计 | 待重新统计 | 主要开销 | 每次对话必带 |
| **总计** | **3,291** | **~940** | 100% | — |

> 注：舞蹈工具已从 `create_dance(name, mood)` 改为 `perform_dance(name, steps...)`，下方旧统计仅作历史参考，实际 token 预算需重新跑脚本统计。

## 每次对话必带开销

**Preamble + Tools = 2,844 字符 ≈ 813 tokens**，占 max_tokens(256K) 的 **0.32%**

这是每次 AI 对话的固定开销，与用户消息长度无关。

## Tool 定义明细

| 工具名 | 字符数 | 估算 Token | 说明 |
|--------|--------|-----------|------|
| `launch_program` | 358 | ~102 | 4 个参数：program/args/workdir/terminal |
| `shell` | 222 | ~63 | 1 个参数：command |
| `read_file` | 192 | ~55 | 1 个参数：path |
| `get_time` | 185 | ~53 | 1 个可选参数：format (enum 3 值) |
| `recent_screenshots` | 187 | ~53 | 1 个可选参数：count |
| `send_hotkey` | 322 | ~92 | 2 个参数：keys(数组) + hold |
| `read_clipboard` | 140 | ~40 | 无参数 |
| `force_foreground` | 201 | ~57 | 1 个参数：hwnd |
| `perform_dance` | 待重新统计 | 待重新统计 | 完整 steps schema，当前最胖工具之一 |
| `play_dance` | **438** | **~125** | 3 个参数，description 最长 |

### 最胖的工具 Top 3

1. **perform_dance** — description + steps schema 较长，是下一轮工具裁剪重点
2. **play_dance** (125 tok) — description 详细解释了 loops/duration_ms 语义
3. **launch_program** (102 tok) — 4 个参数字段，schema 较宽

## 各模块原始内容

### 1. Agent Preamble (`prompts.rs::DEFAULT_AGENT_PREAMBLE`)

```
你是 8Bit，一只住在电脑屏幕上的像素风小猫助手。

性格特点：
- 活泼好奇，喜欢用 emoji
- 偶尔调皮，但做事靠谱
- 回答简洁，不说废话
- 用中文交流

你通过手柄和用户交互，可以帮用户：
- 启动程序、执行命令
- 查时间、读文件
- 闲聊、讲笑话、提醒事项

回答时保持角色感，像一只懂技术的猫。
```

**评价**：非常精简（158 字符 / 45 tok），人设清晰但不冗余。如果要增强角色感可以适当扩充，当前体量完全不是瓶颈。

### 2. Vision Prompt (`prompts.rs::DEFAULT_VISION_PROMPT`)

```
你是 8Bit，一只住在电脑屏幕上的像素风小猫助手。你刚刚看了一眼主人的屏幕。

严格遵守以下规则：
1. 如果你无法看清文字、标签、文件名，必须说"看不清"，绝对不要猜测或编造
2. 对于模糊的图标，只描述颜色和形状，用"看起来像是"而非"就是"
3. 不要编造任何具体的名称、数字、文字内容
4. 与其编造细节，不如诚实说"这个太小了喵~我看不太清"
5. 回复控制在 80 字以内，语气活泼可爱，像猫的视角

请描述你看到的屏幕内容。
```

**评价**：反幻觉规则完善（220 字符 / 63 tok），独立 API 调用不占用对话 context。

### 3. Screen Summary Prompt (`screen_summary.rs::DEFAULT_SCREEN_SUMMARY_PROMPT`)

```
你是 8Bit 的观察模块。以下是一段时间内对主人屏幕的多次 AI 观察记录。

请将它们整理为结构化的活动日志：
- 按活动类型分组（编程、浏览、通讯、娱乐、文档等）
- 每组列出时间段和具体活动
- 合并重复的观察（如连续多次看到同一应用，合并为时间段范围）
- 保留关键细节（项目名、文件名、网站等能看清的信息）
- 控制在 300 字以内
```

**评价**：结构化聚合指令清晰（173 字符 / 49 tok），独立 API 调用。

## 优化建议

### 高优先级（省 token 明显）

| 方案 | 预估节省 | 难度 |
|------|---------|------|
| 精简 `perform_dance` description / schema 文案 | 待测 | 低 |
| 精简 `play_dance` description（语义移到参数 description） | ~25 tok | 低 |
| 合并 `force_foreground` 到 `send_hotkey` 或 `shell`（hwnd 可通过 shell 获取） | ~57 tok | 中 |

### 中优先级（架构改进）

| 方案 | 预估节省 | 难度 |
|------|---------|------|
| 按场景动态注册工具子集（闲聊模式只注册 4-5 个） | ~200-400 tok | 高 |
| 工具 description 中英双语改纯中文（当前已是中文，无需改动） | 0 | — |
| `launch_program` 的 workdir/terminal 合并为一个 options 字符串 | ~20 tok | 低 |

### 不建议优化

- **Preamble**（45 tok）— 已经很精简，扩充反而能提升回复质量
- **Vision Prompt**（63 tok）— 反幻觉规则每条都有价值，删减可能降低分析质量
- **read_clipboard**（40 tok）— 已经是最小工具，无法再精简

## 文件索引

| 配置项 | 源文件 | 常量/字段 |
|--------|--------|----------|
| Agent Preamble | `core/src/prompts.rs` | `DEFAULT_AGENT_PREAMBLE` |
| Vision Prompt | `core/src/prompts.rs` | `DEFAULT_VISION_PROMPT` |
| Vision Multi | `core/src/prompts.rs` | `DEFAULT_VISION_PROMPT_MULTI` |
| Screen Summary | `core/src/screen_summary.rs` | `DEFAULT_SCREEN_SUMMARY_PROMPT` |
| Tool Definitions | `core/src/agent.rs` | `define_tool_sync!` / `define_tool_async!` 宏调用 |
| Memory Config | `core/src/memory.rs` | `MemoryConfig`（数值配置，无提示词文本） |
