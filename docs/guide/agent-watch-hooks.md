# Agent Watch Hooks

Agent Watch observes Claude Code, Codex, and ZCode through read-only hooks. The hook scripts only forward local JSON events to the BitCat monitor on `127.0.0.1:5342`; they do not approve permissions, modify prompts, or control the external agent.

For reachable macOS/Linux machines watched over LAN, Tailscale/tailnet, VPN, or another routable address, see [Remote Agent Watch](remote-agent-watch.md). Remote hooks send the same event shape to the Windows monitor with an additional `machine` field.

## Files

Claude Code:

- Settings: `~/.claude/settings.json`
- Script: `~/.claude/hooks/bitcat-hook.ps1`
- Marker: `bitcat-claude-code-watch`

Codex:

- Config: `$CODEX_HOME/config.toml`, or `~/.codex/config.toml`
- Script: `~/.codex/hooks/bitcat-codex-hook.ps1`
- Marker: `bitcat-codex-watch`

ZCode:

- Config: `~/.zcode/cli/config.json`
- Script: `~/.zcode/hooks/bitcat-zcode-hook.ps1`
- Marker: `bitcat-zcode-watch`
- Scope: user-level hooks; no project files or workspace trust changes

## Hook Doctor

The settings page buttons are repair operations. They are safe to click repeatedly.

On each repair, BitCat:

- rewrites its own PowerShell hook script if the generated content changed;
- backs up the existing settings/config file before saving;
- removes stale or duplicate hooks that contain the BitCat marker;
- removes old BitCat hooks from invalid event names, such as the removed `SubagentStopFailure` event;
- installs the current standard hook set.

The repair is marker-scoped. It only removes or replaces hooks containing `bitcat_marker`. User hooks and hooks installed by other tools are preserved, including hooks that share the same event or matcher.

If the surrounding config shape is unsafe to edit, for example `hooks.PreToolUse` is not an array in Claude settings, repair stops with an error instead of rewriting the file.

## Current Event Sets

Claude Code hooks:

- `UserPromptSubmit`
- `SessionStart`
- `PreToolUse`
- `PostToolUse`
- `PostToolUseFailure`
- `PostToolBatch`
- `PermissionRequest`
- `PermissionDenied`
- `PreCompact` with `auto` and `manual`
- `Stop`
- `StopFailure`
- `SubagentStart`
- `SubagentStop`
- `TaskCreated`
- `TaskCompleted`
- `SessionEnd`
- `Notification`

Codex hooks:

- `UserPromptSubmit`
- `SessionStart`
- `PreToolUse`
- `PermissionRequest`
- `PostToolUse`
- `PreCompact`
- `PostCompact`
- `Stop`

ZCode hooks:

- `SessionStart`
- `UserPromptSubmit`
- `PreToolUse`
- `PermissionRequest`
- `PostToolUse`
- `PostToolUseFailure`
- `Stop`

ZCode currently does not expose session-end, compaction, subagent, task, notification, or token/cost hook events. A finished turn is reported by `Stop`; the session card then follows the normal quiet-period cleanup.

## Notes

Claude Code or VS Code may need a restart after repair because running agent processes may have already loaded their hook configuration.

Codex may additionally require trusting newly discovered hooks in the Codex UI or CLI before they execute. If Codex shows `hooks need review`, run `/hooks`, enter each BitCat hook item, and press `T` to trust it. The repair and remote install commands write the hook config but do not bypass Codex's trust model.

ZCode reads user hook configuration once per session. After repair, start a new ZCode session or send `/reload`. If the user explicitly disabled all hooks in ZCode, BitCat stops without changing that choice and asks the user to enable hooks in ZCode Settings first. ZCode hooks can execute outside the sandbox, so the BitCat runner uses an absolute PowerShell path, a direct argv `process` runner, a 2-second timeout, and `onError: allow`.
