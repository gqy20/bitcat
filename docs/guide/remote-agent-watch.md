# Remote Agent Watch

Remote Agent Watch lets a Windows 8Bit Cat instance watch Claude Code and Codex sessions running on reachable macOS/Linux machines. Reachability can be LAN, Tailscale/tailnet, VPN, or another manually reachable address.

## Ports

- `5342`: write-only hook ingest. Remote machines send hook JSON envelopes here.
- `5344`: read-only remote viewer. Other devices can open `/watch` or fetch JSON snapshots.

The viewer endpoints are:

- `http://<windows-ip>:5344/watch`
- `http://<windows-ip>:5344/agent-sessions`
- `http://<windows-ip>:5344/devices`
- `http://<windows-ip>:5344/health`
- `http://<windows-ip>:5344/remote-install.sh`

## Remote Install

On the Windows host, open Settings -> Agent Watch and copy the remote install command. It looks like:

```bash
curl -fsSL http://<windows-ip>:5344/remote-install.sh | bash -s -- --host <windows-ip> --port 5342
```

Run that command on the remote macOS/Linux machine. It does not require the remote machine to have this repository checked out; the script is downloaded from the Windows 8Bit Cat viewer service.

When Windows has more than one plausible remote address, Settings generates a slightly longer command that tries each candidate and uses the first reachable one. This covers machines that can reach `192.168.x.x`, `10.x.x.x`, or a tailnet/CGNAT `100.64.0.0/10` address.

The generated address list is typed in the UI as LAN / Tailscale / Tailnet/CGNAT / Public / Virtual and prefers addresses in this order:

```text
10.x.x.x -> 192.168.x.x -> 172.16-31.x.x -> other private IPv4 -> Tailscale/CGNAT 100.64/10 -> public IPv4 -> 198.18/15 -> 169.254/16
```

For example, when Windows has both `198.18.0.1` and `10.0.0.20`, the Settings page should prefer `10.0.0.20` for the install command and `/watch` URL.
If Windows has `10.0.0.20`, `192.168.0.20`, and `100.64.0.10`, the generated command will try all three instead of forcing you to guess which network the remote server can see.

Settings shows redacted endpoint labels by default, such as `LAN 192.168.*.20` or `Tailscale 100.64.*.10`. The copied install command and copied watch URLs still contain the full addresses because the remote machine needs them to connect.

The installer:

- writes `~/.ai-pad/hooks/sender.sh`;
- installs marker-scoped Claude Code hooks when `~/.claude` exists;
- installs marker-scoped Codex hooks when `~/.codex` exists;
- wraps hook payloads as `{ "source", "machine", "payload" }`;
- sends the envelope to the Windows monitor.

The sender is intentionally best-effort: it uses a short network timeout and exits successfully even when the Windows monitor is unreachable, so it does not block Claude Code or Codex.

You do not need to restart 8Bit Cat after installing remote hooks. New remote sessions appear after the remote Claude Code or Codex process emits its next hook event, such as a prompt submit, tool call, permission request, stop, or notification.

## Trust Codex Hooks

Codex CLI will not run newly installed hooks until you trust them. After installing the remote hooks, start Codex on the remote machine. If you see:

```text
⚠ 4 hooks need review before they can run. Open /hooks to review them.
```

open the hook review UI:

```text
/hooks
```

Then review each 8Bit Cat hook. Enter every hook item and press `T` to trust it. The commands should point at the installed sender, for example:

```bash
AI_PAD_SOURCE=codex bash ~/.ai-pad/hooks/sender.sh
```

After all hooks are trusted, return to Codex and submit another prompt or trigger a tool call. The Windows Agent Watch view should then show the remote Codex session. This trust step is required by Codex and is not bypassed by the installer.

## Source Selection

Install only one agent source when needed:

```bash
curl -fsSL http://<windows-ip>:5344/remote-install.sh | bash -s -- --host <windows-ip> --source claude_code
curl -fsSL http://<windows-ip>:5344/remote-install.sh | bash -s -- --host <windows-ip> --source codex
```

Remove 8Bit Cat remote hooks:

```bash
curl -fsSL http://<windows-ip>:5344/remote-install.sh | bash -s -- --uninstall
```

## Verify

From the remote macOS/Linux machine, first check that the Windows viewer is reachable:

```bash
curl http://<windows-ip>:5344/health
```

It should return:

```json
{"ok":true}
```

Then trigger a Claude Code or Codex event on the remote machine. The Windows Settings -> Agent Watch page should show the remote device and session after the hook sends its first envelope. The standalone `/watch` page is read-only and can be opened from any reachable device.

If the script downloads but sessions never appear, the remote machine can probably reach `5344` but not `5342`. Allow inbound TCP `5342` through Windows Firewall and make sure the selected endpoint is reachable from that remote network.

## Display

Remote sessions appear in the same Agent Watch stack as local sessions. Cards include a device badge derived from the remote hostname. Settings -> Agent Watch also lists remote devices, session counts, active counts, and last update time.

## Notes

- Windows firewall must allow inbound TCP on `5342` for remote hook ingest and `5344` for the read-only viewer.
- Settings shows redacted endpoint labels by default; copied install commands and watch URLs still contain the full selected addresses.
- The read-only viewer does not expose control actions; it only serves snapshots and a lightweight page.
- Remote permission approval and remote screenshots are intentionally out of scope.
- The endpoint discovery and install command generation live in `app/src/remote_endpoint.rs`; Agent Watch session ingest remains in `app/src/agent_monitor.rs`.
