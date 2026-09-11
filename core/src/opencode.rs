//! opencode 插件事件解析。
//!
//! opencode 通过本地插件（`~/.config/opencode/plugins/*.js`）把事件转发给
//! Agent Watch TCP monitor。插件把 opencode 原生 event 原样放进 envelope，
//! 工具/权限事件（`tool.execute.*` 只存在于 hook 侧）由插件合成为同构 event。
//! 本模块只做"事件类型 → AgentStatus"映射和短 preview 提取，不保存完整
//! 消息。事件 payload 样例见 docs/research/pi-opencode-agent-watch-protocol.md。

use crate::agent_session::{
    AgentSessionEvent, AgentSource, AgentStatus, AgentUsage, WaitingReason, preview_text,
};
use crate::claude_code::preview_tool_input;
use serde_json::Value;

const PREVIEW_CHARS: usize = 160;

/// 解析 opencode 插件 envelope payload，返回归一事件。
///
/// 返回 `Ok(None)` 表示该事件对会话状态没有贡献（流式增量、user 消息 part 等），
/// 调用方应跳过而不当作错误。
pub fn parse_opencode_payload(
    source: AgentSource,
    raw: &str,
    now_ms: u64,
    machine: Option<String>,
) -> Result<Option<AgentSessionEvent>, String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|e| format!("opencode payload JSON 解析失败: {e}"))?;
    let event = value
        .get("event")
        .ok_or_else(|| "opencode payload 缺少 event 字段".to_string())?;
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| "opencode event 缺少 type 字段".to_string())?;
    if !is_watch_event(event_type) {
        // message.part.delta / plugin.added / catalog.updated 等事件与状态无关，
        // 也不保证携带 sessionID，直接跳过。
        return Ok(None);
    }
    let props = event.get("properties").cloned().unwrap_or(Value::Null);
    let directory = string_field(&value, "directory");

    let session_id = string_field(&props, "sessionID")
        .or_else(|| string_field(&props, "sessionId"))
        .ok_or_else(|| format!("opencode 事件 {event_type} 缺少 sessionID"))?;

    let mut builder = EventBuilder {
        session_id,
        source,
        workspace: directory.unwrap_or_default(),
        status: None,
        tool_name: None,
        tool_input: None,
        user_prompt_preview: None,
        last_response_preview: None,
        needs_user: false,
        usage: None,
        waiting_reason: None,
        machine,
        at_ms: now_ms,
    };

    match event_type {
        "session.created" => {
            builder.status = Some(AgentStatus::Idle);
            let info = props.get("info");
            builder.workspace = info
                .and_then(|info| info.get("directory").and_then(string_value))
                .unwrap_or_default();
            builder.usage = info.and_then(session_usage);
        }
        "session.updated" => {
            // title 形如 "Running <prompt>"，是 opencode 里最接近用户任务描述的字段。
            builder.status = Some(AgentStatus::Working);
            let info = props.get("info");
            builder.workspace = info
                .and_then(|info| info.get("directory").and_then(string_value))
                .unwrap_or_default();
            builder.user_prompt_preview = info
                .and_then(|info| info.get("title").and_then(string_value))
                .and_then(|title| strip_running_prefix(&title))
                .and_then(|title| preview_text(title, PREVIEW_CHARS));
            builder.usage = info.and_then(session_usage);
        }
        "session.status" => {
            let status = props
                .get("status")
                .and_then(|status| status.get("type"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            builder.status = match status {
                "busy" => Some(AgentStatus::Working),
                "idle" => Some(AgentStatus::Done),
                _ => None,
            };
        }
        "session.idle" => builder.status = Some(AgentStatus::Done),
        "session.error" => {
            builder.status = Some(AgentStatus::Error);
            builder.last_response_preview = props
                .get("error")
                .and_then(|error| error.get("message").and_then(string_value))
                .or_else(|| props.get("error").and_then(string_value))
                .and_then(|message| preview_text(message, PREVIEW_CHARS));
        }
        "session.deleted" => builder.status = Some(AgentStatus::Idle),
        "session.compacted" => builder.status = Some(AgentStatus::Working),
        "message.part.updated" => {
            // user part 没有 time 字段；assistant text part 完成时 time.end 非空。
            let part = props.get("part");
            let is_text = part
                .and_then(|part| part.get("type"))
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "text");
            let finished = part
                .and_then(|part| part.get("time"))
                .and_then(|time| time.get("end"))
                .and_then(Value::as_u64)
                .is_some();
            if !(is_text && finished) {
                return Ok(None);
            }
            builder.status = Some(AgentStatus::Working);
            builder.last_response_preview = part
                .and_then(|part| part.get("text").and_then(string_value))
                .and_then(|text| preview_text(text, PREVIEW_CHARS));
        }
        "permission.asked" => {
            builder.status = Some(AgentStatus::Waiting);
            builder.needs_user = true;
            builder.waiting_reason = Some(WaitingReason::Permission);
            builder.tool_name = string_field(&props, "tool");
            builder.tool_input = props.get("args").cloned();
        }
        "permission.replied" => builder.status = Some(AgentStatus::Working),
        "tool.execute.before" | "tool.execute.after" => {
            let tool = string_field(&props, "tool");
            let args = props.get("args").cloned();
            if event_type == "tool.execute.before" {
                builder.status = Some(AgentStatus::ToolRunning);
                builder.tool_name = tool;
                builder.tool_input = args;
            } else {
                builder.status = Some(AgentStatus::Working);
            }
        }
        _ => return Ok(None),
    }

    builder
        .status
        .map(|status| builder.build(status))
        .map_or(Ok(None), |event| Ok(Some(event)))
}

/// opencode 会话标题 "Running <prompt>" → "<prompt>"。
fn strip_running_prefix(title: &str) -> Option<String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .strip_prefix("Running ")
            .unwrap_or(trimmed)
            .to_string(),
    )
}

/// 从 session.info 提取会话级累计用量。
/// 真实样例：`{"cost":0,"tokens":{"input":0,"output":0,"cache":{"read":0,"write":0}}}`。
fn session_usage(info: &Value) -> Option<AgentUsage> {
    let tokens = info.get("tokens")?;
    let field = |name: &str| tokens.get(name).and_then(Value::as_u64).unwrap_or(0);
    let cost_usd_micros = info
        .get("cost")
        .and_then(Value::as_f64)
        .map(|cost| (cost * 1_000_000.0).round() as u64)
        .unwrap_or(0);
    Some(AgentUsage {
        tokens_in: field("input"),
        tokens_out: field("output"),
        cost_usd_micros,
        cumulative: true,
    })
}

/// Agent Watch 关心的 opencode 事件类型；其余类型（含未来新增）一律跳过。
fn is_watch_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "session.created"
            | "session.updated"
            | "session.status"
            | "session.idle"
            | "session.error"
            | "session.deleted"
            | "session.compacted"
            | "message.part.updated"
            | "permission.asked"
            | "permission.replied"
            | "tool.execute.before"
            | "tool.execute.after"
    )
}

struct EventBuilder {
    session_id: String,
    source: AgentSource,
    workspace: String,
    status: Option<AgentStatus>,
    tool_name: Option<String>,
    tool_input: Option<Value>,
    user_prompt_preview: Option<String>,
    last_response_preview: Option<String>,
    needs_user: bool,
    usage: Option<AgentUsage>,
    waiting_reason: Option<WaitingReason>,
    machine: Option<String>,
    at_ms: u64,
}

impl EventBuilder {
    fn build(self, status: AgentStatus) -> AgentSessionEvent {
        let show_tool_context = matches!(
            status,
            AgentStatus::ToolRunning | AgentStatus::Waiting | AgentStatus::Error
        );
        let tool_name = self.tool_name.as_deref();
        AgentSessionEvent {
            session_id: self.session_id,
            source: self.source,
            workspace: self.workspace,
            parent_session_id: None,
            status,
            tool_name: show_tool_context
                .then(|| tool_name.map(str::to_string))
                .flatten(),
            tool_input_preview: show_tool_context
                .then(|| {
                    self.tool_input
                        .as_ref()
                        .and_then(|value| preview_tool_input(tool_name, value))
                })
                .flatten(),
            user_prompt_preview: self.user_prompt_preview,
            last_response_preview: self.last_response_preview,
            background: false,
            agent_id: None,
            agent_type: None,
            task_id: None,
            output_file: None,
            pid: None,
            machine: self.machine,
            at_ms: self.at_ms,
            needs_user: status.needs_user() || self.needs_user,
            usage: self.usage,
            waiting_reason: match status {
                AgentStatus::Waiting => Some(self.waiting_reason.unwrap_or(WaitingReason::Input)),
                _ => None,
            },
        }
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(string_value)
}

fn string_value(value: &Value) -> Option<String> {
    value
        .as_str()
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
        parse_opencode_payload(AgentSource::OpenCode, raw, 42, Some("dev-box".into())).unwrap()
    }

    // ---- 以下 payload 全部来自 2026-09-11 本机 opencode 1.18.30 真实运行探针 ----

    #[test]
    fn session_created_builds_idle_session_with_directory() {
        let raw = r#"{"schema":"bitcat-opencode-watch/1","event":{"id":"evt_1","type":"session.created","properties":{"sessionID":"ses_abc","info":{"id":"ses_abc","directory":"/home/u/proj","title":"New session - 2026-09-11T09:56:16.622Z","time":{"created":1789120576622,"updated":1789120576622}}}}}"#;
        let event = parse(raw).unwrap();
        assert_eq!(event.session_id, "ses_abc");
        assert_eq!(event.status, AgentStatus::Idle);
        assert_eq!(event.workspace, "/home/u/proj");
        assert_eq!(event.machine.as_deref(), Some("dev-box"));
        assert_eq!(event.source, AgentSource::OpenCode);
    }

    #[test]
    fn session_status_busy_and_idle_map_states() {
        let busy = r#"{"event":{"type":"session.status","properties":{"sessionID":"ses_abc","status":{"type":"busy"}}}}"#;
        assert_eq!(parse(busy).unwrap().status, AgentStatus::Working);

        let idle = r#"{"event":{"type":"session.status","properties":{"sessionID":"ses_abc","status":{"type":"idle"}}}}"#;
        assert_eq!(parse(idle).unwrap().status, AgentStatus::Done);
    }

    #[test]
    fn session_idle_is_done() {
        let raw = r#"{"event":{"type":"session.idle","properties":{"sessionID":"ses_abc"}}}"#;
        assert_eq!(parse(raw).unwrap().status, AgentStatus::Done);
    }

    #[test]
    fn session_updated_extracts_prompt_from_title() {
        let raw = r#"{"event":{"type":"session.updated","properties":{"sessionID":"ses_abc","info":{"directory":"/home/u/proj","title":"Running echo bitcat-probe-123"}}}}"#;
        let event = parse(raw).unwrap();
        assert_eq!(event.status, AgentStatus::Working);
        assert_eq!(event.workspace, "/home/u/proj");
        assert_eq!(
            event.user_prompt_preview.as_deref(),
            Some("echo bitcat-probe-123")
        );
    }

    #[test]
    fn tool_execute_before_is_tool_running_with_args() {
        let raw = r#"{"event":{"id":"hook-1","type":"tool.execute.before","properties":{"sessionID":"ses_abc","tool":"bash","args":{"command":"echo bitcat-probe-123"}}},"directory":"/home/u/proj"}"#;
        let event = parse(raw).unwrap();
        assert_eq!(event.status, AgentStatus::ToolRunning);
        assert_eq!(event.tool_name.as_deref(), Some("bash"));
        assert!(
            event
                .tool_input_preview
                .as_deref()
                .unwrap_or_default()
                .contains("echo bitcat-probe-123")
        );
        assert_eq!(event.workspace, "/home/u/proj");
    }

    #[test]
    fn tool_execute_after_clears_tool_context() {
        let raw = r#"{"event":{"type":"tool.execute.after","properties":{"sessionID":"ses_abc","tool":"bash","args":{"command":"echo bitcat-probe-123"}}}}"#;
        let event = parse(raw).unwrap();
        assert_eq!(event.status, AgentStatus::Working);
        assert!(event.tool_name.is_none());
        assert!(event.tool_input_preview.is_none());
    }

    #[test]
    fn permission_asked_waits_for_user() {
        let raw = r#"{"event":{"type":"permission.asked","properties":{"sessionID":"ses_abc","tool":"bash","args":{"command":"rm -rf /"}}}}"#;
        let event = parse(raw).unwrap();
        assert_eq!(event.status, AgentStatus::Waiting);
        assert!(event.needs_user);
        assert_eq!(event.tool_name.as_deref(), Some("bash"));
        assert_eq!(event.waiting_reason, Some(WaitingReason::Permission));
    }

    #[test]
    fn session_updated_carries_cumulative_usage() {
        let raw = r#"{"event":{"type":"session.updated","properties":{"sessionID":"ses_abc","info":{"directory":"/home/u/proj","title":"Running echo x","cost":0.042,"tokens":{"input":12000,"output":340,"reasoning":10,"cache":{"read":0,"write":0}}}}}}"#;
        let event = parse(raw).unwrap();
        let usage = event.usage.unwrap();
        assert_eq!(usage.tokens_in, 12000);
        assert_eq!(usage.tokens_out, 340);
        assert_eq!(usage.cost_usd_micros, 42_000);
        assert!(usage.cumulative);
    }

    #[test]
    fn assistant_text_part_updates_response_preview() {
        let raw = r#"{"event":{"type":"message.part.updated","properties":{"sessionID":"ses_abc","part":{"type":"text","text":"`bitcat-probe-123`","time":{"start":1789120740650,"end":1789120741262}}}}}"#;
        let event = parse(raw).unwrap();
        assert_eq!(event.status, AgentStatus::Working);
        assert_eq!(
            event.last_response_preview.as_deref(),
            Some("`bitcat-probe-123`")
        );
    }

    #[rstest]
    #[case(r#"{"event":{"type":"message.part.updated","properties":{"sessionID":"ses_abc","part":{"type":"text","text":"user prompt no time"}}}}"#, "user part 无 time")]
    #[case(r#"{"event":{"type":"message.part.updated","properties":{"sessionID":"ses_abc","part":{"type":"text","text":"streaming","time":{"start":1789120740650}}}}}"#, "未完成的 assistant part")]
    #[case(r#"{"event":{"type":"message.part.delta","properties":{"sessionID":"ses_abc","delta":"x"}}}"#, "流式 delta")]
    #[case(
        r#"{"event":{"type":"plugin.added","properties":{"id":"p1"}}}"#,
        "无关插件事件"
    )]
    #[case(r#"{"event":{"type":"catalog.updated","properties":{}}}"#, "目录更新")]
    fn noise_events_are_skipped(#[case] raw: &str, #[case] _why: &str) {
        assert!(parse(raw).is_none());
    }

    #[test]
    fn session_error_maps_error_with_message() {
        let raw = r#"{"event":{"type":"session.error","properties":{"sessionID":"ses_abc","error":{"message":"provider quota exceeded"}}}}"#;
        let event = parse(raw).unwrap();
        assert_eq!(event.status, AgentStatus::Error);
        assert_eq!(
            event.last_response_preview.as_deref(),
            Some("provider quota exceeded")
        );
    }

    #[test]
    fn missing_session_id_is_error() {
        let raw = r#"{"event":{"type":"session.idle","properties":{}}}"#;
        assert!(parse_opencode_payload(AgentSource::OpenCode, raw, 0, None).is_err());
    }

    #[test]
    fn tool_args_are_sanitized_like_claude_hooks() {
        let raw = r#"{"event":{"type":"tool.execute.before","properties":{"sessionID":"ses_abc","tool":"bash","args":{"command":"curl -H \"Authorization: Bearer abc123\" https://example.test?token=secret"}}}}"#;
        let event = parse(raw).unwrap();
        let preview = event.tool_input_preview.unwrap();
        assert!(preview.contains("[redacted]"));
        assert!(!preview.contains("abc123"));
    }
}
