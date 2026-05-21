#!/usr/bin/env sh
# Install 8Bit Cat remote Agent Watch hooks on macOS/Linux.
# Reads Claude Code/Codex hook JSON locally and forwards it to the Windows monitor.
set -eu

MARKER="ai-pad-remote-watch"
KNOWN_MARKERS="ai-pad-remote-watch,ai-pad-claude-code-watch,ai-pad-codex-watch"
HOST=""
HOSTS=""
PORT="5342"
SOURCE="all"
UNINSTALL="0"
SELF_TEST="1"

usage() {
  cat <<'EOF'
Usage: remote-install.sh --host <windows-ip> [--hosts "ip1 ip2"] [--port 5342] [--source claude_code|codex|all] [--no-self-test] [--uninstall]

Examples:
  curl -fsSL http://192.0.2.10:5344/remote-install.sh | bash -s -- --host 192.0.2.10
  curl -fsSL http://192.0.2.10:5344/remote-install.sh | bash -s -- --hosts "192.0.2.10 100.64.0.10"
  curl -fsSL http://192.0.2.10:5344/remote-install.sh | bash -s -- --host 192.0.2.10 --source claude_code
  curl -fsSL http://192.0.2.10:5344/remote-install.sh | bash -s -- --uninstall

Direct local checkout form, useful only when this repository exists on the remote machine:
  bash scripts/remote-install.sh --host 192.0.2.10
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --host)
      HOST="${2:-}"
      shift 2
      ;;
    --hosts)
      HOSTS="${2:-}"
      shift 2
      ;;
    --port)
      PORT="${2:-5342}"
      shift 2
      ;;
    --source)
      SOURCE="${2:-all}"
      shift 2
      ;;
    --uninstall)
      UNINSTALL="1"
      shift
      ;;
    --no-self-test)
      SELF_TEST="0"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

case "$SOURCE" in
  claude_code|codex|all) ;;
  *)
    echo "--source must be claude_code, codex, or all" >&2
    exit 2
    ;;
esac

if [ -z "$HOSTS" ] && [ -n "$HOST" ]; then
  HOSTS="$HOST"
fi

if [ "$UNINSTALL" = "0" ] && [ -z "$HOSTS" ]; then
  echo "--host or --hosts is required" >&2
  usage
  exit 2
fi

if command -v hostname >/dev/null 2>&1; then
  MACHINE="$(hostname -s 2>/dev/null || hostname)"
else
  MACHINE="remote"
fi

HOOK_DIR="${HOME}/.ai-pad/hooks"
SENDER="${HOOK_DIR}/sender.sh"

write_sender() {
  mkdir -p "$HOOK_DIR"
  cat > "$SENDER" <<EOF
#!/usr/bin/env bash
# ${MARKER}
set -euo pipefail

MACHINE="${MACHINE}"
HOSTS="${HOSTS}"
PORT="${PORT}"
SOURCE="\${AI_PAD_SOURCE:-claude_code}"
STATE_DIR="${HOOK_DIR}/state"
STATE_FILE="\${STATE_DIR}/monitor-state"
LOG_DIR="${HOOK_DIR}/logs"
LOG_FILE="\${LOG_DIR}/agent-hook-bridge.jsonl"
PROBE_INTERVAL_SEC="\${AI_PAD_PROBE_INTERVAL_SEC:-45}"
CONNECT_TIMEOUT_SEC="\${AI_PAD_CONNECT_TIMEOUT_SEC:-1}"
LOG_MAX_BYTES="\${AI_PAD_HOOK_LOG_MAX_BYTES:-1048576}"

raw=\$(cat || true)
if [ -z "\$raw" ]; then exit 0; fi

now_epoch() {
  date +%s 2>/dev/null || printf '0'
}

state_field() {
  key="\$1"
  [ -f "\$STATE_FILE" ] || return 0
  awk -F= -v key="\$key" '\$1 == key { print \$2; exit }' "\$STATE_FILE" 2>/dev/null || true
}

write_state() {
  status="\$1"
  host="\${2:-}"
  mkdir -p "\$STATE_DIR"
  {
    printf 'status=%s\n' "\$status"
    printf 'host=%s\n' "\$host"
    printf 'checked_at=%s\n' "\$(now_epoch)"
  } > "\$STATE_FILE"
}

json_escape() {
  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import json, sys; print(json.dumps(sys.stdin.read())[1:-1])'
  else
    sed 's/\\/\\\\/g; s/"/\\"/g'
  fi
}

payload_field() {
  key="\$1"
  if command -v python3 >/dev/null 2>&1; then
    python3 -c '
import json, sys
key = sys.argv[1]
try:
    payload = json.load(sys.stdin)
    value = payload.get(key)
    print("" if value is None else str(value))
except Exception:
    pass
' "\$key" 2>/dev/null <<FIELD_EOF
\$raw
FIELD_EOF
  fi
}

log_event() {
  status="\$1"
  detail="\${2:-}"
  mkdir -p "\$LOG_DIR" 2>/dev/null || return 0
  if [ -f "\$LOG_FILE" ]; then
    size=\$(wc -c < "\$LOG_FILE" 2>/dev/null || printf '0')
    case "\$size" in ''|*[!0-9]*) size=0 ;; esac
    if [ "\$size" -gt "\$LOG_MAX_BYTES" ]; then
      mv -f "\$LOG_FILE" "\$LOG_FILE.1" 2>/dev/null || true
    fi
  fi
  ts=\$(date -u +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null || date)
  hook=\$(payload_field hook_event_name)
  [ -n "\$hook" ] || hook=\$(payload_field hookEventName)
  session_id=\$(payload_field session_id)
  [ -n "\$session_id" ] || session_id=\$(payload_field sessionId)
  tool=\$(payload_field tool_name)
  [ -n "\$tool" ] || tool=\$(payload_field toolName)
  bytes=\$(printf '%s' "\$raw" | wc -c | tr -d ' ')
  printf '{"ts":"%s","source":"%s","machine":"%s","status":"%s","detail":"%s","hook":"%s","session_id":"%s","tool":"%s","bytes":%s}\n' \
    "\$(printf '%s' "\$ts" | json_escape)" \
    "\$(printf '%s' "\$SOURCE" | json_escape)" \
    "\$(printf '%s' "\$MACHINE" | json_escape)" \
    "\$(printf '%s' "\$status" | json_escape)" \
    "\$(printf '%s' "\$detail" | json_escape)" \
    "\$(printf '%s' "\$hook" | json_escape)" \
    "\$(printf '%s' "\$session_id" | json_escape)" \
    "\$(printf '%s' "\$tool" | json_escape)" \
    "\$bytes" >> "\$LOG_FILE" 2>/dev/null || true
}

can_attempt_network() {
  last_status="\$(state_field status)"
  checked_at="\$(state_field checked_at)"
  now="\$(now_epoch)"
  case "\$checked_at" in
    ''|*[!0-9]*) return 0 ;;
  esac
  case "\$PROBE_INTERVAL_SEC" in
    ''|*[!0-9]*) PROBE_INTERVAL_SEC=45 ;;
  esac
  if [ "\$last_status" = "down" ] && [ \$((now - checked_at)) -lt "\$PROBE_INTERVAL_SEC" ]; then
    return 1
  fi
  return 0
}

send_to_host() {
  host="\$1"
  if command -v nc >/dev/null 2>&1; then
    printf '%s' "\$envelope" | nc -w "\$CONNECT_TIMEOUT_SEC" "\$host" "\$PORT" >/dev/null 2>&1
    return \$?
  fi
  if command -v bash >/dev/null 2>&1; then
    bash -c 'cat > /dev/tcp/"\$0"/"\$1"' "\$host" "\$PORT" <<TCP_EOF >/dev/null 2>&1
\$envelope
TCP_EOF
    return \$?
  fi
  return 1
}

can_attempt_network || exit 0

if command -v python3 >/dev/null 2>&1; then
  envelope=\$(SOURCE="\$SOURCE" MACHINE="\$MACHINE" python3 -c '
import json, os, sys
payload = json.loads(sys.stdin.read())
print(json.dumps({
  "schema": "ai-pad.agent-hook.v1",
  "source": os.environ.get("SOURCE", "claude_code"),
  "machine": os.environ.get("MACHINE", "remote"),
  "payload": payload,
}, separators=(",", ":")))
' <<< "\$raw" 2>/dev/null || true)
else
  envelope=""
fi

if [ -z "\$envelope" ]; then
  envelope=\$(printf '{"schema":"ai-pad.agent-hook.v1","source":"%s","machine":"%s","payload":%s}' "\$SOURCE" "\$MACHINE" "\$raw")
fi

for host in \$(printf '%s' "\$HOSTS" | tr ',' ' '); do
  if send_to_host "\$host"; then
    write_state up "\$host"
    log_event sent "\$host:\$PORT"
    exit 0
  fi
done
write_state down ""
log_event failed "all-targets-unreachable"
exit 0
EOF
  chmod +x "$SENDER"
}

backup_file() {
  path="$1"
  if [ -f "$path" ]; then
    cp "$path" "${path}.ai-pad-backup.$(date +%Y%m%d-%H%M%S)"
  fi
}

install_claude() {
  settings="${HOME}/.claude/settings.json"
  [ -d "${HOME}/.claude" ] || {
    echo "skip Claude Code: ~/.claude not found"
    return 0
  }
  mkdir -p "$(dirname "$settings")"
  [ -f "$settings" ] || printf '{}\n' > "$settings"
  backup_file "$settings"
  SETTINGS="$settings" SENDER="$SENDER" MARKER="$MARKER" KNOWN_MARKERS="$KNOWN_MARKERS" python3 - <<'PY'
import json, os
path = os.environ["SETTINGS"]
sender = os.environ["SENDER"]
marker = os.environ["MARKER"]
known_markers = {m for m in os.environ.get("KNOWN_MARKERS", marker).split(",") if m}
events = [
  ("UserPromptSubmit", None), ("SessionStart", None),
  ("PreToolUse", "*"), ("PostToolUse", "*"), ("PostToolUseFailure", "*"),
  ("PostToolBatch", None), ("PermissionRequest", "*"), ("PermissionDenied", "*"),
  ("PreCompact", "auto"), ("PreCompact", "manual"), ("Stop", None),
  ("StopFailure", None), ("SubagentStart", None), ("SubagentStop", None),
  ("TaskCreated", None), ("TaskCompleted", None), ("SessionEnd", None),
  ("Notification", None),
]
try:
    with open(path, "r", encoding="utf-8") as f:
        root = json.load(f)
except Exception:
    root = {}
hooks = root.setdefault("hooks", {})
hook = {"type": "command", "command": f'AI_PAD_SOURCE=claude_code bash "{sender}"', "ai_pad_marker": marker}
removed = 0
for event, groups in list(hooks.items()):
    if not isinstance(groups, list):
        continue
    before = len(groups)
    groups[:] = [g for g in groups if not (isinstance(g, dict) and g.get("ai_pad_marker") in known_markers)]
    removed += before - len(groups)
    for group in groups:
        if isinstance(group, dict) and isinstance(group.get("hooks"), list):
            before_hooks = len(group["hooks"])
            group["hooks"] = [h for h in group["hooks"] if not (isinstance(h, dict) and h.get("ai_pad_marker") in known_markers)]
            removed += before_hooks - len(group["hooks"])
    hooks[event] = [g for g in groups if not (isinstance(g, dict) and g.get("hooks") == [])]
    if hooks[event] == []:
        hooks.pop(event, None)
for event, matcher in events:
    groups = hooks.setdefault(event, [])
    group = next((g for g in groups if isinstance(g, dict) and g.get("matcher") == matcher and isinstance(g.get("hooks"), list)), None)
    if group is None:
        group = {"hooks": []}
        if matcher is not None:
            group["matcher"] = matcher
        groups.append(group)
    group["hooks"].append(dict(hook))
with open(path, "w", encoding="utf-8") as f:
    json.dump(root, f, indent=2, ensure_ascii=False)
    f.write("\n")
print(f"repaired Claude Code hooks: removed {removed} stale 8Bit Cat hook(s)")
PY
  echo "installed Claude Code remote hooks"
}

install_codex() {
  config="${HOME}/.codex/config.toml"
  [ -d "${HOME}/.codex" ] || {
    echo "skip Codex: ~/.codex not found"
    return 0
  }
  mkdir -p "$(dirname "$config")"
  [ -f "$config" ] || : > "$config"
  backup_file "$config"
  tmp="${config}.tmp.$$"
  awk -v markers="$KNOWN_MARKERS" '
    BEGIN {
      n = split(markers, parts, ",")
      for (i = 1; i <= n; i++) known[parts[i]] = 1
    }
    {
      for (marker in known) {
        if ($0 ~ ("# " marker " begin")) { skip=1; removed=1; next }
        if ($0 ~ ("# " marker " end")) { skip=0; next }
      }
      if (!skip) print
    }
    END {
      if (removed) print "repaired Codex hooks: removed stale 8Bit Cat block(s)" > "/dev/stderr"
    }
  ' "$config" > "$tmp"
  cat >> "$tmp" <<EOF

# ${MARKER} begin
[[hooks.UserPromptSubmit]]
[[hooks.UserPromptSubmit.hooks]]
type = "command"
command = "AI_PAD_SOURCE=codex bash ${SENDER}"
commandLinux = "AI_PAD_SOURCE=codex bash ${SENDER}"
timeout = 5
ai_pad_marker = "${MARKER}"

[[hooks.PreToolUse]]
matcher = "*"
[[hooks.PreToolUse.hooks]]
type = "command"
command = "AI_PAD_SOURCE=codex bash ${SENDER}"
commandLinux = "AI_PAD_SOURCE=codex bash ${SENDER}"
timeout = 5
ai_pad_marker = "${MARKER}"

[[hooks.PostToolUse]]
matcher = "*"
[[hooks.PostToolUse.hooks]]
type = "command"
command = "AI_PAD_SOURCE=codex bash ${SENDER}"
commandLinux = "AI_PAD_SOURCE=codex bash ${SENDER}"
timeout = 5
ai_pad_marker = "${MARKER}"

[[hooks.Stop]]
[[hooks.Stop.hooks]]
type = "command"
command = "AI_PAD_SOURCE=codex bash ${SENDER}"
commandLinux = "AI_PAD_SOURCE=codex bash ${SENDER}"
timeout = 5
ai_pad_marker = "${MARKER}"

# ${MARKER} end
EOF
  mv "$tmp" "$config"
  echo "installed Codex remote hooks"
}

uninstall_all() {
  if [ -f "${HOME}/.claude/settings.json" ] && command -v python3 >/dev/null 2>&1; then
    SETTINGS="${HOME}/.claude/settings.json" MARKER="$MARKER" KNOWN_MARKERS="$KNOWN_MARKERS" python3 - <<'PY'
import json, os
path = os.environ["SETTINGS"]
marker = os.environ["MARKER"]
known_markers = {m for m in os.environ.get("KNOWN_MARKERS", marker).split(",") if m}
with open(path, "r", encoding="utf-8") as f:
    root = json.load(f)
hooks = root.get("hooks", {})
for event, groups in list(hooks.items()):
    if not isinstance(groups, list):
        continue
    groups[:] = [g for g in groups if not (isinstance(g, dict) and g.get("ai_pad_marker") in known_markers)]
    for group in groups:
        if isinstance(group, dict) and isinstance(group.get("hooks"), list):
            group["hooks"] = [h for h in group["hooks"] if not (isinstance(h, dict) and h.get("ai_pad_marker") in known_markers)]
    hooks[event] = [g for g in groups if not (isinstance(g, dict) and g.get("hooks") == [])]
with open(path, "w", encoding="utf-8") as f:
    json.dump(root, f, indent=2, ensure_ascii=False)
    f.write("\n")
PY
  fi
  if [ -f "${HOME}/.codex/config.toml" ]; then
    tmp="${HOME}/.codex/config.toml.tmp.$$"
    awk -v markers="$KNOWN_MARKERS" '
      BEGIN {
        n = split(markers, parts, ",")
        for (i = 1; i <= n; i++) known[parts[i]] = 1
      }
      {
        for (marker in known) {
          if ($0 ~ ("# " marker " begin")) { skip=1; next }
          if ($0 ~ ("# " marker " end")) { skip=0; next }
        }
        if (!skip) print
      }
    ' "${HOME}/.codex/config.toml" > "$tmp"
    mv "$tmp" "${HOME}/.codex/config.toml"
  fi
  rm -f "$SENDER"
  echo "removed 8Bit Cat remote hooks"
}

send_self_test() {
  [ "$SELF_TEST" = "1" ] || return 0
  test_source="$SOURCE"
  if [ "$test_source" = "all" ]; then
    if [ -d "${HOME}/.codex" ]; then
      test_source="codex"
    elif [ -d "${HOME}/.claude" ]; then
      test_source="claude_code"
    else
      test_source="codex"
    fi
  fi
  cwd="$(pwd 2>/dev/null || printf '%s' "$HOME")"
  if command -v python3 >/dev/null 2>&1; then
    envelope="$(AI_PAD_SOURCE="$test_source" MACHINE="$MACHINE" CWD="$cwd" python3 - <<'PY' 2>/dev/null || true
import json, os
source = os.environ.get("AI_PAD_SOURCE", "codex")
machine = os.environ.get("MACHINE", "remote")
cwd = os.environ.get("CWD", "")
print(json.dumps({
  "schema": "ai-pad.agent-hook.v1",
  "source": source,
  "machine": machine,
  "payload": {
    "session_id": "ai-pad-remote-self-test-" + machine,
    "hook_event_name": "UserPromptSubmit",
    "cwd": cwd,
    "prompt": "8Bit Cat remote hook self-test"
  }
}, separators=(",", ":")))
PY
)"
  else
    envelope="$(printf '{"schema":"ai-pad.agent-hook.v1","source":"%s","machine":"%s","payload":{"session_id":"ai-pad-remote-self-test-%s","hook_event_name":"UserPromptSubmit","cwd":"%s","prompt":"8Bit Cat remote hook self-test"}}' "$test_source" "$MACHINE" "$MACHINE" "$cwd")"
  fi
  [ -n "$envelope" ] || {
    echo "remote self-test: skipped, could not build payload" >&2
    return 0
  }
  for host in $(printf '%s' "$HOSTS" | tr ',' ' '); do
    if command -v nc >/dev/null 2>&1; then
      printf '%s' "$envelope" | nc -w 2 "$host" "$PORT" >/dev/null 2>&1 && {
        echo "remote self-test: sent to $host:$PORT"
        return 0
      }
    elif command -v bash >/dev/null 2>&1; then
      bash -c 'cat > /dev/tcp/"$0"/"$1"' "$host" "$PORT" <<TCP_EOF >/dev/null 2>&1 && {
$envelope
TCP_EOF
        echo "remote self-test: sent to $host:$PORT"
        return 0
      }
    fi
  done
  echo "remote self-test: could not reach any target on port $PORT" >&2
  return 0
}

if [ "$UNINSTALL" = "1" ]; then
  uninstall_all
  exit 0
fi

write_sender
case "$SOURCE" in
  claude_code) install_claude ;;
  codex) install_codex ;;
  all)
    install_claude
    install_codex
    ;;
esac

send_self_test
echo "remote watch sender: $SENDER"
echo "targets: $HOSTS -> $PORT"
echo "machine: $MACHINE"
