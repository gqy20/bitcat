# pi / opencode Agent Watch 接入协议调研

> 2026-09-11 通过本机真实运行验证（pi 0.85.1 / opencode 1.18.30，各跑一次纯文本会话和一次 bash 工具会话，探针已清理）。
> 结论：两者都支持**本地插件文件自动发现**，安装器无需改用户配置文件，比 Claude Code / Codex 的 hook 合并更干净。

## 一、pi（@earendil-works/pi-coding-agent）

### 扩展机制
- 全局扩展目录：`~/.pi/agent/extensions/*.ts`（或子目录 `*/index.ts`），启动时自动发现加载，**无需改 settings.json**。
- 项目级：`.pi/extensions/*.ts`（需项目 trust，不适用 BitCat）。
- 文档：npm 包内 `docs/extensions.md`（`@earendil-works/pi-coding-agent`）。
- 硬约束：extension factory 里**不能启动后台资源**（socket/timer/watcher），必须延迟到 `session_start`；`session_shutdown` 时关闭。转发器用"每个事件一次性 connect"最简单，规避该约束。

### 事件协议（真实 payload 验证）
- `session_start` `{reason: startup|reload|new|resume|fork}`；session id 从 `ctx.sessionManager.getSessionId()`，cwd 从 `ctx.cwd`。
- `before_agent_start` `{prompt, systemPrompt, ...}` → user_prompt_preview。
- `agent_start` → Working；`agent_end`（低层 run 结束，可能自动重试）；`agent_settled`（确定不再自动继续）→ **Done 用这个**。
- `tool_execution_start` `{toolCallId, toolName, args}`（args 是 JSON **字符串**）→ ToolRunning。
- `tool_execution_update` `{partialResult}` → 忽略（噪音）。
- `tool_execution_end` `{toolCallId, toolName, result, isError}` → Working / Error。
- `message_end` `{message: {role, content: [{type: "text"|"thinking"|"toolCall", ...}]}}`；role=assistant 时取 text 部分 → last_response_preview。
- `session_compact` → Compacting。
- `session_shutdown` `{reason: quit}` → Idle（会话结束，与 Claude Code SessionEnd→Idle 对齐）。
- `pi -p`（print 模式）每次完整走 start/shutdown，会话生命周期短。

## 二、opencode（sst/opencode 1.18.x）

### 插件机制
- 全局插件目录：`~/.config/opencode/plugins/*.js`，启动时自动加载，**无需改 opencode.json**（旧版 `experimental.hook` 已从 config schema 移除，勿再用）。
- 项目级：`.opencode/plugins/`。
- 插件签名：`export const X = async ({project, client, $, directory, worktree}) => ({ hooks })`，运行在 Bun。
- 事件钩子 `event: async ({event}) => {}` 收全量事件 `{id, type, properties}`。
- 工具钩子 `tool.execute.before(input, output)` / `tool.execute.after(input)` 是**独立 hook 不进 event 流**，必须由插件合成转发；`before` 的 args 在 `output.args`（input 只有 `{tool, sessionID, callID}`）。
- 插件是长驻的：需维护 `sessionID → directory` 缓存（来自 `session.created` 的 `info.directory`），tool hook 里没有 directory。

### 事件协议（真实 payload 验证）
- `session.created` `{sessionID, info: {id, title, directory, time}}` → 建会话，workspace=info.directory。
- `session.updated` `{sessionID, info}` → title 变化（"Running <prompt>"），映射 Working + user_prompt_preview（去 "Running " 前缀）。
- `session.status` `{sessionID, status: {type: "busy"|"idle"}}` → Working / Done（权威状态源）。
- `session.idle` `{sessionID}` → Done（最终完成）。
- `session.error` → Error（未实测，宽松解析）。
- `message.part.updated` `{sessionID, part: {type, text, time}}`：**user part 没有 time 字段；assistant text part 有 `time: {start, end}`**——`time.end` 非空 ⟹ assistant 文本完成 → last_response_preview。天然区分 user/assistant。
- `message.part.delta` 流式增量 → 忽略。
- `permission.asked` / `permission.replied` 在 event 流中 → Waiting(needs_user) / Working。
- 插件初始化 `{directory}` 是启动目录，不等于会话目录，以 session.created 的 info.directory 为准。

## 三、转发 envelope（复用现有 Agent Watch TCP :5342）

```json
{"schema": "bitcat-agent-watch/1", "source": "pi"|"opencode", "machine": "<hostname>", "payload": {...}}
```

- pi payload：`{"schema": "bitcat-pi-watch/1", "event": "<事件名>", "session_id": "...", "cwd": "...", "data": {<原始事件对象>}}`
- opencode payload：`{"schema": "bitcat-opencode-watch/1", "event": {<原始 event 对象 或 插件合成的 tool/permission 事件>}, "directory": "..."}`

## 四、安装器要点

| | Claude Code | Codex | pi | opencode |
|---|---|---|---|---|
| 写脚本 | `~/.claude/hooks/bitcat-hook.ps1` | `~/.codex/hooks/bitcat-codex-hook.ps1` | `~/.pi/agent/extensions/bitcat-watch.ts` | `~/.config/opencode/plugins/bitcat-watch.js` |
| 改配置 | settings.json 合并 hook 项 | config.toml 合并 hook 表 | **无** | **无** |
| 风险 | BOM/损坏防护已有 | TOML 解析防护已有 | 文件名冲突即可 | 文件名冲突即可 |

- opencode 目录解析：`$XDG_CONFIG_HOME/opencode` → `$HOME/.config/opencode`（Windows 下 `$USERPROFILE/.config/opencode`）。
- pi 目录解析：`$HOME/.pi/agent/extensions`。
- 两者脚本都是跨平台 JS/TS（跑在宿主的 Bun/Node），不像 Claude/Codex 依赖 PowerShell。
