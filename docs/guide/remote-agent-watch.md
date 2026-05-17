# Remote Agent Watch

Remote Agent Watch lets a Windows 8Bit Cat instance watch Claude Code and Codex sessions running on other macOS/Linux machines on the same LAN.

## Ports

- `5342`: write-only hook ingest. Remote machines send hook JSON envelopes here.
- `5344`: read-only LAN viewer. Other devices can open `/watch` or fetch JSON snapshots.

The viewer endpoints are:

- `http://<windows-ip>:5344/watch`
- `http://<windows-ip>:5344/agent-sessions`
- `http://<windows-ip>:5344/devices`
- `http://<windows-ip>:5344/health`

## Remote Install

On the Windows host, open Settings -> Agent Watch and copy the remote install command. It looks like:

```bash
bash scripts/remote-install.sh --host <windows-ip> --port 5342
```

Run that command on the remote macOS/Linux machine. The installer:

- writes `~/.ai-pad/hooks/sender.sh`;
- installs marker-scoped Claude Code hooks when `~/.claude` exists;
- installs marker-scoped Codex hooks when `~/.codex` exists;
- wraps hook payloads as `{ "source", "machine", "payload" }`;
- sends the envelope to the Windows monitor.

The sender is intentionally best-effort: it uses a short network timeout and exits successfully even when the Windows monitor is unreachable, so it does not block Claude Code or Codex.

## Source Selection

Install only one agent source when needed:

```bash
bash scripts/remote-install.sh --host <windows-ip> --source claude_code
bash scripts/remote-install.sh --host <windows-ip> --source codex
```

Remove 8Bit Cat remote hooks:

```bash
bash scripts/remote-install.sh --uninstall
```

## Display

Remote sessions appear in the same Agent Watch stack as local sessions. Cards include a device badge derived from the remote hostname. Settings -> Agent Watch also lists remote devices, session counts, active counts, and last update time.

## Notes

- Windows firewall must allow inbound TCP on `5342` for remote hook ingest and `5344` for the read-only viewer.
- The read-only viewer does not expose control actions; it only serves snapshots and a lightweight page.
- Remote permission approval and remote screenshots are intentionally out of scope.
