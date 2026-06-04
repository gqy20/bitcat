# Provider Compatibility Notes

This note records provider-specific behavior that can break BitCat's AI chat
pipeline even when the API key, network, and model name are valid.

## StepFun step-3.7-flash Streaming Tools

Observed on 2026-06-02 with:

- Base URL: `https://api.stepfun.com/step_plan`
- Model: `step-3.7-flash`
- Client path: `rig` Anthropic provider, `stream_prompt().multi_turn(...)`

### Symptom

The bubble opens and the chat request starts, but no useful reply appears.
The app log contains an SSE parse error like:

```text
AI chat started model=step-3.7-flash
chat stream error ... error_reason=stepfun_stream_tool_parse
Failed to parse JSON: missing field `input`
Data: {"type":"content_block_start","content_block":{"type":"tool_use","name":"get_time"}}
```

This is not primarily a UI, API key, or network failure. The request reaches the
model and the model starts streaming.

### Probe Results

Manual probes against the Anthropic-compatible `/v1/messages` endpoint showed:

| Case | Result |
| --- | --- |
| Plain non-streaming text | Works |
| Plain streaming text | Works |
| Non-streaming tool call | Works; `tool_use.input` is present |
| Streaming tool call | Fails with current `rig` parser shape |

For non-streaming tool calls, StepFun returns a complete tool block:

```json
{
  "type": "tool_use",
  "name": "get_time",
  "input": {
    "format": "full"
  }
}
```

For streaming tool calls, StepFun first sends a `tool_use` start block without
`input`:

```json
{
  "type": "content_block_start",
  "index": 1,
  "content_block": {
    "type": "tool_use",
    "id": "chatcmpl-tool-...",
    "name": "get_time"
  }
}
```

The arguments arrive later as an input delta:

```json
{
  "type": "content_block_delta",
  "index": 1,
  "delta": {
    "type": "input_json_delta",
    "partial_json": "{\"format\": \"full\"}"
  }
}
```

Some runs emit `{}` as the partial JSON when the tool has no required arguments.

### Cause

BitCat currently consumes the main chat through `rig`'s Anthropic streaming
parser. That parser expects the streamed `content_block_start` for `tool_use` to
already contain an `input` field. StepFun sends the field later via
`input_json_delta`, so parsing aborts before the later delta can be accumulated.

In short: StepFun's non-streaming tool response is usable, but its streaming
tool event shape is not compatible with the current strict parser.

### Current Behavior

BitCat detects this exact StepFun streaming tool parse failure and retries the
same user turn with the existing non-streaming `prompt().max_turns(...)` path.
The retry reuses the same agent, tools, and permission hook, then emits the
complete reply as one text chunk so the bubble still receives visible content.

This fallback is scoped to StepFun config plus the `tool_use`/missing `input`
parse shape. Native Anthropic streaming and generic SSE parse recovery keep the
normal streaming behavior.

### Workarounds

- Use a provider/model that fully supports Anthropic Messages streaming tools.
- If the fallback also fails, use StepFun for plain text or non-streaming
  tool-call probes only while collecting logs.

### Possible Fixes

- Add a StepFun-specific compatibility layer before `rig` parses the stream:
  synthesize `input: {}` on `tool_use` start blocks and accumulate later
  `input_json_delta.partial_json` chunks.
- Patch or fork `rig` so Anthropic streamed `tool_use.input` is optional at
  `content_block_start` time.
- Replace the fallback with a first-class provider capability flag if more
  providers need split streaming/non-streaming behavior.

### Related Logs

Check:

```text
~/.bitcat/logs/app.log.YYYY-MM-DD
```

Search for:

```text
error_reason=sse_parse
error_reason=stepfun_stream_tool_parse
missing field `input`
content_block_start
tool_use
```
