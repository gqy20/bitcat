# ZCode Agent Watch 接入协议调研

> 2026-11-25 核对本机 ZCode 桌面端 `4.0.0.10870`、内置 CLI `0.18.0`，并交叉验证官方 Hooks / Plugin 文档与安装包实现。结论：ZCode 原生用户 Hook 可以稳定覆盖 Agent Watch 的核心状态循环，无需逆向 `app-server` 或扫描日志/SQLite。

## 一、安装与配置

- 桌面端：`%LOCALAPPDATA%\Programs\ZCode\ZCode.exe`
- CLI：`resources/glm/zcode.cjs`（`zcode 0.18.0`）
- 用户配置：`~/.zcode/cli/config.json`
- BitCat 脚本：`~/.zcode/hooks/bitcat-zcode-hook.ps1`
- 标记：`bitcat-zcode-watch`

ZCode 配置支持用户级、项目级、插件和 Legacy 四类 Hook。本接入只使用用户级：

- 不需要改项目文件，不触发 workspace trust；
- 用户级 Hook 对所有项目生效；
- 配置每个会话读取一次，安装后需要新开会话或 `/reload`；
- `.claude/settings.json` 中的旧 Hook 属于 Legacy，只展示、不执行，必须导入，因此现有 Claude Code 连接器不能自动覆盖 ZCode。

## 二、事件与状态映射

ZCode 当前支持 7 个事件，stdin 同时提供 camelCase 和 Claude 兼容 snake_case 字段：

| 事件 | 主要字段 | BitCat 状态 |
|---|---|---|
| `SessionStart` | `session_id` / `sessionId`、`cwd`、`source` | Idle |
| `UserPromptSubmit` | `prompt` | Working |
| `PreToolUse` | `tool_name` / `toolName`、`tool_input` / `toolInput`、`tool_use_id` | ToolRunning |
| `PermissionRequest` | 工具字段、`permission_suggestions` | Waiting(Permission) |
| `PostToolUse` | `tool_response` / `toolResponse` | Working |
| `PostToolUseFailure` | `error`、`error_details`、`is_interrupt` | Error（沿用现有 Claude 解析器语义） |
| `Stop` | `last_assistant_message` / `lastAssistantMessage` | Done |

所有事件都有 `session_id`、`transcript_path`、`cwd`、`permission_mode`、`hook_event_name`、`agent_type`。环境变量提供 `ZCODE_PROJECT_DIR` / `ZCODE_SESSION_ID` 和 Claude 兼容别名。

`transcript_path` 是本次运行期间的临时文件，结束后删除。BitCat 不读取它，避免把完整对话写入 Agent Watch。

## 三、配置形态

```json
{
  "hooks": {
    "enabled": true,
    "timeoutMs": 5000,
    "maxOutputBytes": 32768,
    "events": {
      "SessionStart": [{ "hooks": [{ "type": "process", "command": "<powershell>", "args": ["-File", "<script>"] }] }],
      "PreToolUse": [{ "hooks": [{ "type": "process", "command": "<powershell>", "args": ["-File", "<script>"] }] }]
    }
  }
}
```

- matcher 缺省或 `*` 匹配全部；工具事件也可写具体工具名。
- `process` runner 直接传 argv，不经过 shell，适合 Windows 路径和参数。
- `timeoutMs` 优先于兼容字段 `timeout`（秒）。
- `async: true` 只用于 shell runner；异步输出不能阻断或修改事件。
- `onError` 默认 `allow`；BitCat 显式使用 `allow`，监控不可用时绝不阻断 ZCode。
- runner schema 允许额外字段（`.passthrough()`），因此可以写 `bitcat_marker` 做幂等修复。

## 四、转发 envelope

脚本读取 stdin JSON，包成现有 Agent Watch envelope：

```json
{
  "schema": "bitcat.agent-hook.v1",
  "source": "zcode",
  "machine": "<COMPUTERNAME>",
  "payload": { "<ZCode 原始 hook payload>": "..." }
}
```

只发到 `127.0.0.1:5342`。脚本：

- 单次连接，250ms 超时；
- 监控未运行时用 `~/.bitcat/hooks/state/monitor-state` 把探测降到 45 秒一次；
- 不输出协议 JSON，始终 `exit 0`；
- 日志只记录事件名、session id、工具名和字节数，不记录 prompt/tool input 正文。

## 五、限制与后续方向

ZCode Hook 目前不提供：

- `SessionEnd`：会话关闭后卡片按 Done 的 30 分钟安静窗口淘汰；
- `PreCompact` / `PostCompact`；
- `SubagentStart` / `SubagentStop`；
- `TaskCreated` / `TaskCompleted`；
- `Notification` / `PermissionDenied` / `StopFailure`；
- token / cost 用量。

ZCode 还有 `app-server --stdio` 协议，安装包里能看到 `task_start` / `streaming` / `permission_request` / `task_complete` / `task_error` 等内部事件，但没有公开协议文档。第一版不依赖它；未来若 Hook 事件不足，再单独评估 app-server 适配。

ZCode 桌面端还能托管 Claude Code / Codex / opencode / Gemini。第一版只接管 ZCode 原生 runtime；若真实会话出现重复卡片，再按 `agent_type` 或 session id 去重。

## 六、安全边界

ZCode 官方明确提醒 Hook 可以在 sandbox 外执行本地命令。因此本接入：

- 只写用户级 Hook，不写 workspace Hook；
- 使用绝对 PowerShell 路径和 `process` runner，避免 PATH 劫持与 shell 引号问题；
- 合并配置时保留其他 Hook，写前备份、原子替换；
- 用户显式 `hooks.enabled: false` 时不擅自开启；
- 不读取 transcript，不保存原始 payload，不修改提示词或权限决策。

## 七、官方来源

- <https://zcode.z.ai/en/docs/hooks>
- <https://zcode.z.ai/cn/docs/plugin>
