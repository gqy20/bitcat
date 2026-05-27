# Logging Rules

All production logging uses the `tracing` crate. Do not use `println!` or `eprintln!`
outside tests.

## Levels

| Level | Purpose | Frequency |
| --- | --- | --- |
| `error!` | Unrecoverable failure that needs human attention | Almost never in a healthy run |
| `warn!` | Degraded success or recoverable anomaly | Occasional |
| `info!` | Product timeline event | Per user action or lifecycle event |
| `debug!` | Internal state snapshot for troubleshooting | Default-on diagnostics |
| `trace!` | High-frequency tick, frame, chunk, or loop detail | Default-off diagnostics |

## Large Text

Never write raw user text, AI output, prompts, shell commands, file contents, API
responses, or frontend forwarded messages directly to a log line. Use character
counts plus `bitcat_core::logging::log_preview()`.

```rust
let preview = log_preview(&reply, 80);
info!(
    reply_chars = reply.chars().count(),
    reply_preview = %preview,
    "ai response complete"
);
```

Use `*_chars` for counts and `*_preview` for shortened text. Count with
`.chars().count()`, not `.len()`, when the field describes text length.

## Structured Fields

- Prefer named fields over string interpolation.
- Keep the message body short and stable, for example `"chat complete"`.
- Put values in fields, not in the message body.
- Use `%` for readable display values and `?` for structured debug values.
- Keep high-cardinality or sensitive values as previews only.

## `#[instrument]`

- Always `skip` large parameters such as prompts, messages, replies, closures,
  and callback functions.
- Add summary fields explicitly with `fields(msg_chars = message.chars().count())`.
- Never let a multi-KB string become part of a span.

## Forbidden Patterns

- `info!("AI said: {reply}")`
- `info!(msg = %msg, "...")`
- `debug!(command = %command, "...")`
- `warn!(response = %api_response, "...")`
- `info!` for stream chunks, screenshot frames, gamepad ticks, or retry loops

## Checklist

Before adding a log line:

1. Is the level correct?
2. Is this high-frequency? If yes, use `trace!` or `debug!`.
3. Could any field contain user, AI, shell, file, prompt, or API text?
4. If yes, did you log `*_chars` and `*_preview` instead?
5. Is the message body stable enough to search across versions?
