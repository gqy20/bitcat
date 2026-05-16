# Agent Watch Hooks

Agent Watch observes Claude Code and Codex through read-only hooks. The hook scripts only forward local JSON events to the 8Bit Cat monitor on `127.0.0.1:5342`; they do not approve permissions, modify prompts, or control the external agent.

## Files

Claude Code:

- Settings: `~/.claude/settings.json`
- Script: `~/.claude/hooks/ai-pad-hook.ps1`
- Marker: `ai-pad-claude-code-watch`

Codex:

- Config: `$CODEX_HOME/config.toml`, or `~/.codex/config.toml`
- Script: `~/.codex/hooks/ai-pad-codex-hook.ps1`
- Marker: `ai-pad-codex-watch`

## Hook Doctor

The settings page buttons are repair operations. They are safe to click repeatedly.

On each repair, 8Bit Cat:

- rewrites its own PowerShell hook script if the generated content changed;
- backs up the existing settings/config file before saving;
- removes stale or duplicate hooks that contain the 8Bit Cat marker;
- removes old 8Bit Cat hooks from invalid event names, such as the removed `SubagentStopFailure` event;
- installs the current standard hook set.

The repair is marker-scoped. It only removes or replaces hooks containing `ai_pad_marker`. User hooks and hooks installed by other tools are preserved, including hooks that share the same event or matcher.

If the surrounding config shape is unsafe to edit, for example `hooks.PreToolUse` is not an array in Claude settings, repair stops with an error instead of rewriting the file.

## Current Event Sets

Claude Code hooks:

- `UserPromptSubmit`
- `SessionStart`
- `PreToolUse`
- `PostToolUse`
- `PostToolUseFailure`
- `PermissionRequest`
- `PermissionDenied`
- `PreCompact` with `auto` and `manual`
- `Stop`
- `StopFailure`
- `SubagentStop`
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

## Notes

Claude Code or VS Code may need a restart after repair because running agent processes may have already loaded their hook configuration.

Codex may additionally require trusting newly discovered hooks in the Codex UI or CLI before they execute. The repair command writes the hook config but does not bypass Codex's trust model.
