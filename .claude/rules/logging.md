# Logging Rules

All logging uses the `tracing` crate. Never use `eprintln!`/`println!` outside `#[test]`.

## Level Definitions

| Level | Purpose | Frequency | Example |
|-------|---------|-----------|---------|
| `error!` | Unrecoverable failure, needs human intervention | Almost never in healthy run | Agent init failed, thread crashed |
| `warn!` | Degraded success, auto-recoverable anomaly | Occasional | File parse failure → empty fallback, API non-200, lock poisoned |
| `info!` | Business lifecycle markers, one line per event | Per user action | App started, chat complete, screenshot cycle done |
| `debug!` | Internal state snapshot for troubleshooting | Default on for core crate | Memory entries loaded, context composition, persistence summary |
| `trace!` | High-frequency per-chunk/per-tick/per-frame | Default off (`RUST_LOG=trace`) | Stream chunk, gamepad tick, screenshot frame dHash |

## Large Text Rule

**Never put dynamic text longer than 80 chars directly into a log message body.** Always truncate to a preview:

```rust
// Correct
info!(
    chars = reply.chars().count(),
    preview = %reply.chars().take(60).collect::<String>(),
    "ai response complete"
);

// Wrong
info!(reply = %reply, "AI said: {reply}");
info!(msg = %msg, "user said: {msg}");
```

This applies to: AI replies, user messages, voice recognition text, file contents, any LLM output.

## Structured Fields

- Use named fields, not string interpolation: `info!(model = %m, chars = n, "...")`
- Message body is a short operation name, not a sentence template
- Keep message body under 80 chars
- Use `%` (Display) for readable types, `?` (Debug) for complex types, `?path` for PathBuf

## `#[instrument]` Rules

- Always `skip` large parameters (full prompts, reply text, closures)
- Add summary fields via `fields()`: `fields(msg_len = msg.chars().count())`
- Never let a multi-KB string become part of the span name

```rust
// Correct
#[instrument(skip(self, on_chunk, message), fields(msg_len = message.chars().count()))]

// Wrong — message pollutes span name
#[instrument(skip(self, on_chunk), fields(msg_len = message.chars().count()))]
```

## Forbidden Patterns

- `eprintln!` / `println!` in non-test code — use `debug!` or `trace!`
- Logging raw user input (PII risk) — use length + truncated preview
- Logging full API responses — use status code + char count
- `info!` for per-chunk/per-iteration events — use `trace!`

## Checklist (before adding a log line)

1. Is the level correct per the table above?
2. Is the message body under 80 chars?
3. Does any dynamic field exceed 80 chars? → truncate it
4. Is this a high-frequency call (>10x per second)? → should be `trace!`
5. Is this inside `#[test]`? → `eprintln!` is OK there
