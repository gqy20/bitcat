# Remote Agent Watch

Remote Agent Watch lets a Windows BitCat instance watch Claude Code and Codex sessions running on reachable macOS/Linux machines. Reachability can be LAN, Tailscale/tailnet, VPN, or another manually reachable address.

## Ports

- `5342`: write-only hook ingest. Remote machines send hook JSON envelopes here.
- `5344`: read-only remote viewer. Other devices can open `/watch` or fetch JSON snapshots.

The viewer endpoints are:

- `http://<windows-ip>:5344/watch`
- `http://<windows-ip>:5344/agent-sessions`
- `http://<windows-ip>:5344/devices`
- `http://<windows-ip>:5344/health`
- `http://<windows-ip>:5344/remote-install.sh`

`/health` remains available as a lightweight reachability probe. The Settings -> Agent Watch page has separate safety switches for the remote viewer (`/watch`, `/agent-sessions`, `/devices`) and the remote install script (`/remote-install.sh`). Turning either switch off makes that surface return `403` instead of serving content.

## Remote Install

On the Windows host, open Settings -> Agent Watch and copy the remote install command. It looks like:

```bash
curl -fsSL http://<windows-ip>:5344/remote-install.sh | bash -s -- --host <windows-ip> --port 5342
```

Run that command on the remote macOS/Linux machine. It does not require the remote machine to have this repository checked out; the script is downloaded from the Windows BitCat viewer service.

When Windows has more than one plausible remote address, Settings generates a slightly longer command that tries each candidate and uses the first reachable one. This covers machines that can reach `192.168.x.x`, `10.x.x.x`, or a tailnet/CGNAT `100.64.0.0/10` address.

The generated address list is typed in the UI as LAN / Tailscale / Tailnet/CGNAT / Public / Virtual and prefers addresses in this order:

```text
10.x.x.x -> 192.168.x.x -> 172.16-31.x.x -> other private IPv4 -> Tailscale/CGNAT 100.64/10 -> public IPv4 -> 198.18/15 -> 169.254/16
```

For example, when Windows has both `198.18.0.1` and `10.0.0.20`, the Settings page should prefer `10.0.0.20` for the install command and `/watch` URL.
If Windows has `10.0.0.20`, `192.168.0.20`, and `100.64.0.10`, the generated command will try all three instead of forcing you to guess which network the remote server can see.

Settings shows redacted endpoint labels by default, such as `LAN 192.168.*.20` or `Tailscale 100.64.*.10`. The copied install command and copied watch URLs still contain the full addresses because the remote machine needs them to connect.

The installer:

- writes `~/.bitcat/hooks/sender.sh`;
- installs marker-scoped Claude Code hooks when `~/.claude` exists;
- installs marker-scoped Codex hooks when `~/.codex` exists;
- repairs previous BitCat hook installs by removing known stale markers before writing the current hook set;
- wraps hook payloads as `{ "source", "machine", "payload" }`;
- sends the envelope to the Windows monitor.

The sender is intentionally best-effort: it uses a short network timeout and exits successfully even when the Windows monitor is unreachable, so it does not block Claude Code or Codex. When the monitor is down, the sender records that state locally and skips network work until the next probe window. Defaults are `BITCAT_PROBE_INTERVAL_SEC=45` and `BITCAT_CONNECT_TIMEOUT_SEC=1`.

After installation, the script sends one self-test envelope to the Windows monitor. This should make the remote device appear in Agent Watch immediately when port `5342` is reachable. You can opt out with `--no-self-test`.

You do not need to restart BitCat after installing remote hooks. After the self-test, real remote sessions continue to update whenever Claude Code or Codex emits a hook event, such as a prompt submit, tool call, permission request, stop, or notification.

## Trust Codex Hooks

Codex CLI will not run newly installed hooks until you trust them. After installing the remote hooks, start Codex on the remote machine. If you see:

```text
⚠ 4 hooks need review before they can run. Open /hooks to review them.
```

open the hook review UI:

```text
/hooks
```

Then review each BitCat hook. Enter every hook item and press `T` to trust it. The commands should point at the installed sender, for example:

```bash
BITCAT_SOURCE=codex bash ~/.bitcat/hooks/sender.sh
```

After all hooks are trusted, return to Codex and submit another prompt or trigger a tool call. The Windows Agent Watch view should then show the remote Codex session. This trust step is required by Codex and is not bypassed by the installer.

## Source Selection

Install only one agent source when needed:

```bash
curl -fsSL http://<windows-ip>:5344/remote-install.sh | bash -s -- --host <windows-ip> --source claude_code
curl -fsSL http://<windows-ip>:5344/remote-install.sh | bash -s -- --host <windows-ip> --source codex
```

Remove BitCat remote hooks:

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

The installer normally sends a self-test event right after writing the hooks, so the Windows Settings -> Agent Watch page should show the remote device without restarting BitCat. Then trigger a Claude Code or Codex event on the remote machine to verify real hook traffic. The standalone `/watch` page is read-only and can be opened from any reachable device while the remote viewer switch is enabled.

If the script downloads but sessions never appear, the remote machine can probably reach `5344` but not `5342`. Allow inbound TCP `5342` through Windows Firewall and make sure the selected endpoint is reachable from that remote network.

## Mobile LAN Viewer

Open the read-only viewer from a phone or tablet on the same network:

```text
http://<windows-ip>:5344/watch
```

The mobile viewer uses the same session snapshot as the desktop Agent Watch window. It groups visible sessions by attention level:

- `Needs attention`: waiting or error sessions.
- `Running`: working, tool-running, or compacting sessions.
- `Recently done`: completed sessions that have not become quiet yet.

Idle sessions and quiet completed sessions are hidden so old history does not crowd the phone screen. Tap a card to expand its detail preview. The `Done` summary tile toggles the `Recently done` group locally in that browser; it does not change the underlying session state.

The page polls `/agent-sessions` every 2 seconds while visible and slows down while the browser tab is hidden. If the Windows BitCat process was already running before a new build, restart BitCat before checking the page so `5344` serves the current embedded HTML.

The viewer also exposes a small PWA shell:

- `/manifest.webmanifest`
- `/sw.js`
- `/agent-watch-icon-128.png`
- `/agent-watch-icon-256.png`

Browsers that support installing local or private-network web apps can add the viewer to the home screen as `Agent Watch`. Full service-worker behavior depends on the browser's secure-context rules. `localhost` usually works; plain `http://192.168.x.x:5344` may install as a shortcut but skip offline caching on stricter mobile browsers. Tailnet/VPN HTTPS or a trusted reverse proxy gives the most complete PWA behavior.

## Display

Remote sessions appear in the same Agent Watch stack as local sessions. Cards include a device badge derived from the remote hostname. Settings -> Agent Watch also lists remote devices, session counts, active counts, and last update time.

## Troubleshooting

- `http://<windows-ip>:5344/health` should return `{"ok":true}`. If it does not, BitCat may not be running, the wrong IP was used, or Windows Firewall is blocking TCP `5344`.
- If `/watch` opens but remote sessions never appear, check inbound TCP `5342`; remote hooks send events to `5342`, while the viewer itself is on `5344`.
- If the phone cannot open the page but the Windows machine can open `http://127.0.0.1:5344/watch`, make sure both devices are on the same LAN or VPN/tailnet and that the router does not isolate wireless clients.
- If the page still shows the older ungrouped list after updating code, restart BitCat. The viewer HTML is embedded in the running executable.
- If Codex sessions are missing after remote install, run `/hooks` in Codex and trust the BitCat hook entries.

## Notes

- Windows firewall must allow inbound TCP on `5342` for remote hook ingest and `5344` for the read-only viewer.
- Settings shows redacted endpoint labels by default; copied install commands and watch URLs still contain the full selected addresses.
- The remote viewer and remote installer can be disabled independently in Settings -> Agent Watch. `/health` stays available for diagnostics.
- The read-only viewer does not expose control actions; it only serves snapshots and a lightweight page.
- Remote permission approval and remote screenshots are intentionally out of scope.
- The endpoint discovery and install command generation live in `app/src/remote_endpoint.rs`; Agent Watch session ingest remains in `app/src/agent_monitor.rs`.
