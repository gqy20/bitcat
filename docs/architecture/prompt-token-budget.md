# Prompt Token 预算分析

> 更新日期：2026-05-30
> 基于当前源码：`config/prompts.yml`、`core/src/agent.rs`、`core/src/memory.rs`

## 概览

当前主对话固定上下文主要来自四块：

| 模块 | 当前状态 | 估算 Token | 调用时机 |
|------|----------|------------|----------|
| Agent preamble | `config/prompts.yml` 的基础人格提示词 | 约 100-300 | 每次主对话 |
| Tool definitions | rig 原生 `ToolDefinition` + JSON schema，共 16 个工具 | 约 6.2K-9.7K | 每次主对话 |
| Tool policy | `build_tool_guide_prompt()` 只保留高风险/易误用规则 | 约 350-700 | 每次主对话 |
| Memory/profile/screen context | 用户画像、短期记忆、长期记忆候选、截图摘要按预算注入 | 0-20K+ | 有内容时注入 |

工具 schema 仍是固定开销的大头，但已经从“双写工具清单”调整为：

```text
普通能力说明 -> 写在工具 description/schema 中，由 rig 原生工具定义承载
额外行为政策 -> 写在工具政策 prompt 中，只保留高风险/容易误用规则
```

这样可以避免每个工具在 schema 和 prompt 里重复描述，也更贴合 rig 的设计：模型看工具时天然会看到工具名、description、参数 schema；prompt 只补足 schema 难以表达的运行时政策。

## 当前工具集

主 Agent 注册 16 个内置工具：

| 类别 | 工具 |
|------|------|
| 系统/文件 | `launch_program`、`shell`、`read_file`、`get_time` |
| 观察/上下文 | `recent_screenshots`、`search_memory`、`remember` |
| 提醒 | `create_reminder`、`list_reminders`、`cancel_reminder` |
| 桌面操作 | `send_hotkey`、`read_clipboard`、`force_foreground` |
| 表演/游戏 | `perform_dance`、`play_dance`、`start_game` |

`start_game` 是 2026-05-30 新增的低风险表演类工具，只接受内置枚举游戏类型，开销主要是一个小 schema，粗略约 300-600 tokens。

## 记忆预算

短期对话记忆由 `MemoryConfig` 控制：

| 字段 | 当前默认值 | 说明 |
|------|------------|------|
| `max_context_chars` | `20000` | 注入短期记忆上下文的总字符预算 |
| `max_user_chars` | `500` | 单条用户消息写入时截断 |
| `max_reply_chars` | `1000` | 单条 AI 回复写入时截断 |
| `max_entries` | `0` | 默认不按条数淘汰，由字符预算控制 |

这次把单条截断从 user 100 / reply 200 提高到 user 500 / reply 1000，能显著保留上下文细节，但也意味着短期记忆更容易吃满 `max_context_chars`。实际 token 取决于中文/英文比例和对话密度，满预算时大致可能达到数千到一万多 tokens。

长期记忆仍坚持 grep-first：`search_memory` 或上下文拼接先用 text/tags/source/importance 等可解释条件筛候选，再交给模型判断语义相关性，不引入 embedding/vector DB 作为主线。当前自动长期记忆注入预算默认 `retrieve_budget_chars = 8000`；`search_memory` 工具按需检索默认允许更高预算，上限 `12000` 字符，并可额外指定返回条数。

## 优化顺序

当前不建议默认动态裁剪工具。更稳的顺序是：

1. 继续用 token usage 日志和设置页统计观察真实消耗。
2. 保持工具 description/schema 是普通能力说明的单一来源。
3. 只把高风险规则、失败必须如实说明、提醒时间字段等放进工具政策 prompt。
4. 对特别胖的 schema 做结构化压缩，而不是把说明复制到 prompt。
5. 只有当真实统计显示固定工具开销成为主要瓶颈时，再评估能力包或 dynamic tools。

## 文件索引

| 内容 | 文件 |
|------|------|
| Agent preamble / memory config 默认 YAML | `config/prompts.yml` |
| Tool 注册与工具政策 prompt | `core/src/agent.rs` |
| Tool 参数与执行逻辑 | `core/src/tools.rs` |
| 短期/长期记忆预算与检索 | `core/src/memory.rs` |
| Token 统计持久化 | `core/src/token_usage.rs` |
