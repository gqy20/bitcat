//! pi 扩展事件解析。
//!
//! pi 通过本地扩展（`~/.pi/agent/extensions/*.ts`）把生命周期事件转发给
//! Agent Watch TCP monitor。扩展只做"采集 + 补上下文"（session id、cwd），
//! 原始事件对象放在 `data` 字段里，本模块负责映射到归一 `AgentSessionEvent`。
//! 事件 payload 样例见 docs/research/pi-opencode-agent-watch-protocol.md。

use crate::agent_session::{AgentSessionEvent, AgentSource, AgentStatus, AgentUsage, preview_text};
use crate::claude_code::preview_tool_input;
use serde_json::Value;

const PREVIEW_CHARS: usize = 160;

/// 解析 pi 扩展 envelope payload，返回归一事件。
///
/// 返回 `Ok(None)` 表示该事件对会话状态没有贡献（流式增量等），调用方应跳过。
pub fn parse_pi_payload(
    source: AgentSource,
    raw: &str,
    now_ms: u64,
    machine: Option<String>,
) -> Result<Option<AgentSessionEvent>, String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|e| format!("pi payload JSON 解析失败: {e}"))?;
    let event_name = value
        .get("event")
        .and_then(Value::as_str)
        .ok_or_else(|| "pi payload 缺少 event 字段".to_string())?;
    let session_id = string_field(&value, "session_id")
        .ok_or_else(|| format!("pi 事件 {event_name} 缺少 session_id"))?;
    let workspace = string_field(&value, "cwd").unwrap_or_default();
    let data = value.get("data").cloned().unwrap_or(Value::Null);

    let status = match event_name {
        "session_start" => Some(AgentStatus::Idle),
        "before_agent_start" | "agent_start" => Some(AgentStatus::Working),
        "agent_settled" => Some(AgentStatus::Done),
        "session_compact" => Some(AgentStatus::Compacting),
        "session_shutdown" => Some(AgentStatus::Idle),
        "tool_execution_start" => Some(AgentStatus::ToolRunning),
        "tool_execution_end" => {
            if data.get("isError").and_then(Value::as_bool) == Some(true) {
                Some(AgentStatus::Error)
            } else {
                Some(AgentStatus::Working)
            }
        }
        "message_end" => Some(AgentStatus::Working),
        // message_update 是 token 级流式增量，agent_end 之后 agent_settled 才是完成信号。
        _ => return Ok(None),
    };

    let Some(status) = status else {
        return Ok(None);
    };

    let tool_name = string_field(&data, "toolName");
    // pi 的 tool args 是 JSON 对象；对历史/边缘形态的 JSON 字符串也容忍。
    let tool_input = match data.get("args") {
        Some(Value::Object(_)) => data.get("args").cloned(),
        Some(Value::String(text)) => serde_json::from_str::<Value>(text)
            .ok()
            .or_else(|| Some(Value::String(text.clone()))),
        _ => None,
    };
    let user_prompt_preview = data
        .get("prompt")
        .and_then(Value::as_str)
        .and_then(|prompt| preview_text(prompt, PREVIEW_CHARS));
    let last_response_preview =
        assistant_text(&data).and_then(|text| preview_text(text, PREVIEW_CHARS));
    let show_tool_context = matches!(
        status,
        AgentStatus::ToolRunning | AgentStatus::Waiting | AgentStatus::Error
    );

    Ok(Some(AgentSessionEvent {
        session_id,
        source,
        workspace,
        parent_session_id: None,
        status,
        tool_name: show_tool_context.then_some(tool_name.clone()).flatten(),
        tool_input_preview: show_tool_context
            .then(|| {
                tool_input
                    .as_ref()
                    .and_then(|value| preview_tool_input(tool_name.as_deref(), value))
            })
            .flatten(),
        user_prompt_preview,
        last_response_preview,
        background: false,
        agent_id: None,
        agent_type: None,
        task_id: None,
        output_file: None,
        pid: None,
        machine,
        at_ms: now_ms,
        needs_user: status.needs_user(),
        usage: assistant_usage(&data),
        waiting_reason: None,
    }))
}

/// 从 assistant `message_end` 的 usage 字段提取单条消息用量（增量）。
/// 真实样例：`{"input":9568,"output":19,"cost":{"total":0}}`。
fn assistant_usage(data: &Value) -> Option<AgentUsage> {
    let message = data.get("message")?;
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let usage = message.get("usage")?;
    let field = |name: &str| usage.get(name).and_then(Value::as_u64).unwrap_or(0);
    let cost_usd_micros = usage
        .get("cost")
        .and_then(|cost| cost.get("total"))
        .and_then(Value::as_f64)
        .map(|total| (total * 1_000_000.0).round() as u64)
        .unwrap_or(0);
    Some(AgentUsage {
        tokens_in: field("input"),
        tokens_out: field("output"),
        cost_usd_micros,
        cumulative: false,
    })
}

/// 从 message_end 事件里取 assistant 回复的 text 部分作为 last_response_preview。
fn assistant_text(data: &Value) -> Option<&str> {
    let message = data.get("message")?;
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    message
        .get("content")?
        .as_array()?
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .find(|text| !text.trim().is_empty())
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_session::{AgentSource, AgentStatus};
    use rstest::rstest;

    fn parse(raw: &str) -> Option<AgentSessionEvent> {
        parse_pi_payload(AgentSource::Pi, raw, 42, Some("dev-box".into())).unwrap()
    }

    // ---- 以下 payload 全部来自 2026-09-11 本机 pi 0.85.1 真实运行探针 ----

    #[test]
    fn session_start_creates_idle_session() {
        let raw = r#"{"schema":"bitcat-pi-watch/1","event":"session_start","session_id":"01a08fe5-4a2b-729d-8455-7c2e69c683bb","cwd":"/home/u/proj","data":{"type":"session_start","reason":"startup"}}"#;
        let event = parse(raw).unwrap();
        assert_eq!(event.session_id, "01a08fe5-4a2b-729d-8455-7c2e69c683bb");
        assert_eq!(event.status, AgentStatus::Idle);
        assert_eq!(event.workspace, "/home/u/proj");
        assert_eq!(event.machine.as_deref(), Some("dev-box"));
        assert_eq!(event.source, AgentSource::Pi);
    }

    #[test]
    fn before_agent_start_keeps_user_prompt() {
        let raw = r#"{"event":"before_agent_start","session_id":"s1","cwd":"/home/u/proj","data":{"type":"before_agent_start","prompt":"use the bash tool to run exactly: echo bitcat-probe-123"}}"#;
        let event = parse(raw).unwrap();
        assert_eq!(event.status, AgentStatus::Working);
        assert_eq!(
            event.user_prompt_preview.as_deref(),
            Some("use the bash tool to run exactly: echo bitcat-probe-123")
        );
    }

    #[test]
    fn tool_execution_start_parses_object_args() {
        // payload 来自 2026-09-11 pi 0.85.1 真实运行（args 是 JSON 对象）。
        let raw = r#"{"event":"tool_execution_start","session_id":"s1","cwd":"/home/u/proj","data":{"type":"tool_execution_start","toolCallId":"call_45ae","toolName":"bash","args":{"command":"echo pi-e2e-ok"}}}"#;
        let event = parse(raw).unwrap();
        assert_eq!(event.status, AgentStatus::ToolRunning);
        assert_eq!(event.tool_name.as_deref(), Some("bash"));
        assert!(
            event
                .tool_input_preview
                .as_deref()
                .unwrap_or_default()
                .contains("echo pi-e2e-ok")
        );
    }

    #[test]
    fn tool_execution_start_tolerates_string_args() {
        let raw = r#"{"event":"tool_execution_start","session_id":"s1","cwd":"/home/u/proj","data":{"toolCallId":"call_bfdc","toolName":"bash","args":"{\"command\": \"echo bitcat-probe-123\"}"}}"#;
        let event = parse(raw).unwrap();
        assert!(
            event
                .tool_input_preview
                .as_deref()
                .unwrap_or_default()
                .contains("echo bitcat-probe-123")
        );
    }

    #[test]
    fn tool_execution_end_returns_to_working() {
        let raw = r#"{"event":"tool_execution_end","session_id":"s1","cwd":"/home/u/proj","data":{"type":"tool_execution_end","toolCallId":"call_bfdc","toolName":"bash","result":"...","isError":false}}"#;
        let event = parse(raw).unwrap();
        assert_eq!(event.status, AgentStatus::Working);
        assert!(event.tool_name.is_none());
    }

    #[test]
    fn tool_execution_end_with_error_maps_error() {
        let raw = r#"{"event":"tool_execution_end","session_id":"s1","cwd":"/home/u/proj","data":{"toolCallId":"c1","toolName":"bash","isError":true}}"#;
        assert_eq!(parse(raw).unwrap().status, AgentStatus::Error);
    }

    #[test]
    fn agent_settled_is_done() {
        let raw = r#"{"event":"agent_settled","session_id":"s1","cwd":"/home/u/proj","data":{"type":"agent_settled"}}"#;
        assert_eq!(parse(raw).unwrap().status, AgentStatus::Done);
    }

    #[test]
    fn assistant_message_end_updates_response_preview() {
        let raw = r#"{"event":"message_end","session_id":"s1","cwd":"/home/u/proj","data":{"type":"message_end","message":{"role":"assistant","content":[{"type":"thinking","thinking":"..."},{"type":"text","text":"ok"}],"stopReason":"stop"}}}"#;
        let event = parse(raw).unwrap();
        assert_eq!(event.status, AgentStatus::Working);
        assert_eq!(event.last_response_preview.as_deref(), Some("ok"));
    }

    #[test]
    fn assistant_message_end_carries_incremental_usage() {
        // usage 来自 2026-09-11 真实运行：input 9568 / output 19 / cost.total 0。
        let raw = r#"{"event":"message_end","session_id":"s1","cwd":"/home/u/proj","data":{"message":{"role":"assistant","content":[{"type":"text","text":"ok"}],"usage":{"input":9568,"output":19,"cacheRead":2048,"reasoning":15,"totalTokens":11635,"cost":{"input":0,"output":0,"total":0}}}}}"#;
        let event = parse(raw).unwrap();
        let usage = event.usage.unwrap();
        assert_eq!(usage.tokens_in, 9568);
        assert_eq!(usage.tokens_out, 19);
        assert_eq!(usage.cost_usd_micros, 0);
        assert!(!usage.cumulative);
    }

    #[test]
    fn user_message_end_has_no_usage() {
        let raw = r#"{"event":"message_end","session_id":"s1","cwd":"/p","data":{"message":{"role":"user","content":[{"type":"text","text":"hi"}],"usage":{"input":1,"output":2}}}}"#;
        assert!(parse(raw).unwrap().usage.is_none());
    }

    #[test]
    fn user_message_end_keeps_response_preview_empty() {
        let raw = r#"{"event":"message_end","session_id":"s1","cwd":"/home/u/proj","data":{"type":"message_end","message":{"role":"user","content":[{"type":"text","text":"reply with just: ok"}]}}}"#;
        let event = parse(raw).unwrap();
        assert!(event.last_response_preview.is_none());
    }

    #[rstest]
    #[case(
        "message_update",
        r#"{"message":{"role":"assistant"},"assistantMessageEvent":{}}"#,
        "流式增量"
    )]
    #[case("turn_start", r#"{"turnIndex":0}"#, "turn 开始")]
    #[case("agent_end", r#"{"messages":[]}"#, "低层 run 结束，可能自动重试")]
    #[case(
        "tool_execution_update",
        r#"{"toolCallId":"c1","partialResult":"{}"}"#,
        "工具部分结果"
    )]
    fn noise_events_are_skipped(#[case] event_name: &str, #[case] data: &str, #[case] _why: &str) {
        let raw =
            format!(r#"{{"event":"{event_name}","session_id":"s1","cwd":"/p","data":{data}}}"#);
        assert!(parse(&raw).is_none());
    }

    #[test]
    fn session_shutdown_is_idle() {
        let raw = r#"{"event":"session_shutdown","session_id":"s1","cwd":"/home/u/proj","data":{"reason":"quit"}}"#;
        assert_eq!(parse(raw).unwrap().status, AgentStatus::Idle);
    }

    #[test]
    fn missing_session_id_is_error() {
        let raw = r#"{"event":"agent_start","cwd":"/p","data":{}}"#;
        assert!(parse_pi_payload(AgentSource::Pi, raw, 0, None).is_err());
    }

    #[test]
    fn tool_args_are_sanitized_like_claude_hooks() {
        let raw = r#"{"event":"tool_execution_start","session_id":"s1","cwd":"/p","data":{"toolName":"bash","args":"{\"command\": \"curl -H \\\"Authorization: Bearer abc123\\\" https://x.test\"}"}}"#;
        let event = parse(raw).unwrap();
        let preview = event.tool_input_preview.unwrap();
        assert!(preview.contains("[redacted]"));
        assert!(!preview.contains("abc123"));
    }
}
