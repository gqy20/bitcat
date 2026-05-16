//! Claude Code hook 事件解析。
//!
//! Claude Code hook payload 可能随版本演进，因此这里接受宽松 JSON 字段，
//! 再转换为项目内部稳定的 `AgentSessionEvent`。本模块只提取 session、cwd、
//! hook 名称和短 preview，不保存完整工具输入或完整对话内容。

use crate::agent_session::{AgentSessionEvent, AgentSource, AgentStatus, preview_text};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const PREVIEW_CHARS: usize = 160;

/// 宽松的 Claude Code hook payload。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ClaudeHookEvent {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub hook_event_name: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_input: Option<Value>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub last_assistant_message: Option<String>,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

impl ClaudeHookEvent {
    /// 解析 hook JSON 并兼容少量常见别名字段。
    pub fn from_json(raw: &str) -> Result<Self, String> {
        let value: Value =
            serde_json::from_str(raw).map_err(|e| format!("Claude hook JSON 解析失败: {e}"))?;
        let mut event: Self = serde_json::from_value(value.clone())
            .map_err(|e| format!("Claude hook payload 反序列化失败: {e}"))?;
        event.fill_aliases(&value);
        Ok(event)
    }

    /// 转换为归一 session event。
    pub fn into_session_event(self, now_ms: u64) -> Result<AgentSessionEvent, String> {
        self.into_session_event_from(AgentSource::ClaudeCode, now_ms)
    }

    /// 转换为归一 session event，并允许调用方标记真实来源。
    pub fn into_session_event_from(
        self,
        source: AgentSource,
        now_ms: u64,
    ) -> Result<AgentSessionEvent, String> {
        let session_id_alias = string_field(&self.extra, "sessionId");
        let session_id_snake_alias = string_field(&self.extra, "session_id");
        let session_id = first_non_empty([
            self.session_id.as_deref(),
            session_id_alias.as_deref(),
            session_id_snake_alias.as_deref(),
        ])
        .ok_or_else(|| "Claude hook 缺少 session_id".to_string())?;

        let event_alias = string_field(&self.extra, "event");
        let hook_event_alias = string_field(&self.extra, "hookEventName");
        let hook_name = first_non_empty([
            self.hook_event_name.as_deref(),
            self.status.as_deref(),
            event_alias.as_deref(),
            hook_event_alias.as_deref(),
        ])
        .unwrap_or("unknown");

        let status = map_hook_status(hook_name);
        let tool_name = self
            .tool_name
            .or_else(|| string_field(&self.extra, "toolName"))
            .or_else(|| nested_string(&self.extra, &["tool", "name"]));
        let tool_input = self
            .tool_input
            .as_ref()
            .cloned()
            .or_else(|| value_field(&self.extra, "tool_input"))
            .or_else(|| value_field(&self.extra, "toolInput"));
        let tool_input_preview = tool_input
            .as_ref()
            .and_then(|value| preview_tool_input(tool_name.as_deref(), value));

        let workspace = first_non_empty([
            self.cwd.as_deref(),
            string_field(&self.extra, "workspace").as_deref(),
            string_field(&self.extra, "cwd").as_deref(),
        ])
        .unwrap_or("")
        .to_string();

        Ok(AgentSessionEvent {
            session_id: session_id.to_string(),
            source,
            workspace,
            status,
            tool_name,
            tool_input_preview,
            user_prompt_preview: self
                .prompt
                .as_deref()
                .and_then(|value| preview_text(value, PREVIEW_CHARS))
                .or_else(|| {
                    string_field(&self.extra, "user_prompt")
                        .as_deref()
                        .and_then(|value| preview_text(value, PREVIEW_CHARS))
                }),
            last_response_preview: self
                .last_assistant_message
                .as_deref()
                .and_then(|value| preview_text(value, PREVIEW_CHARS))
                .or_else(|| {
                    string_field(&self.extra, "lastAssistantMessage")
                        .as_deref()
                        .and_then(|value| preview_text(value, PREVIEW_CHARS))
                }),
            pid: self.pid.or_else(|| u32_field(&self.extra, "pid")),
            at_ms: now_ms,
            needs_user: status.needs_user(),
        })
    }

    fn fill_aliases(&mut self, value: &Value) {
        if self.hook_event_name.is_none() {
            self.hook_event_name = value
                .get("hookEventName")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if self.session_id.is_none() {
            self.session_id = value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if self.tool_name.is_none() {
            self.tool_name = value
                .get("toolName")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if self.tool_input.is_none() {
            self.tool_input = value
                .get("toolInput")
                .cloned()
                .or_else(|| value.get("tool_input").cloned());
        }
        if self.last_assistant_message.is_none() {
            self.last_assistant_message = value
                .get("lastAssistantMessage")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
    }
}

/// Claude hook 名称到内部状态的映射。
pub fn map_hook_status(name: &str) -> AgentStatus {
    match normalize_hook_name(name).as_str() {
        "userpromptsubmit" => AgentStatus::Working,
        "sessionstart" => AgentStatus::Idle,
        "pretooluse" => AgentStatus::ToolRunning,
        "posttooluse" => AgentStatus::Working,
        "permissionrequest" => AgentStatus::Waiting,
        "precompact" => AgentStatus::Compacting,
        "stop" => AgentStatus::Done,
        "stopfailure" => AgentStatus::Error,
        "subagentstop" => AgentStatus::Working,
        "subagentstopfailure" => AgentStatus::Working,
        "sessionend" => AgentStatus::Idle,
        "notification" => AgentStatus::Waiting,
        "permissiondenied" => AgentStatus::Waiting,
        "posttoolusefailure" => AgentStatus::Working,
        "error" => AgentStatus::Error,
        "interrupted" => AgentStatus::Interrupted,
        _ => AgentStatus::Working,
    }
}

fn normalize_hook_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn first_non_empty<'a>(values: impl IntoIterator<Item = Option<&'a str>>) -> Option<&'a str> {
    values
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
}

fn string_field(map: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn u32_field(map: &serde_json::Map<String, Value>, key: &str) -> Option<u32> {
    map.get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn value_field(map: &serde_json::Map<String, Value>, key: &str) -> Option<Value> {
    map.get(key).cloned()
}

fn nested_string(map: &serde_json::Map<String, Value>, path: &[&str]) -> Option<String> {
    let mut current = Value::Object(map.clone());
    for key in path {
        current = current.get(*key)?.clone();
    }
    current.as_str().map(str::to_string)
}

fn preview_json(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => preview_text(text, PREVIEW_CHARS),
        other => serde_json::to_string(other)
            .ok()
            .and_then(|text| preview_text(text, PREVIEW_CHARS)),
    }
}

fn preview_tool_input(tool_name: Option<&str>, value: &Value) -> Option<String> {
    let Value::Object(input) = value else {
        return preview_json(value);
    };
    let tool = tool_name
        .unwrap_or_default()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    let keys: &[&str] = match tool.as_str() {
        "bash" | "powershell" => &["description", "command", "timeout"],
        "read" => &["file_path", "offset", "limit"],
        "write" => &["file_path"],
        "edit" | "multiedit" => &["file_path", "replace_all"],
        "grep" => &[
            "pattern",
            "path",
            "glob",
            "output_mode",
            "-n",
            "-i",
            "head_limit",
        ],
        "glob" => &["pattern", "path"],
        "agent" | "task" => &["description", "subagent_type"],
        "taskoutput" | "taskstop" => &["task_id", "block", "timeout"],
        "skill" => &["skill", "args"],
        _ => &[
            "description",
            "file_path",
            "path",
            "pattern",
            "command",
            "url",
            "task_id",
        ],
    };

    let mut output = serde_json::Map::new();
    for key in keys {
        if let Some(value) = input.get(*key).and_then(preview_value) {
            output.insert((*key).to_string(), value);
        }
    }
    if output.is_empty() {
        return None;
    }
    preview_json(&Value::Object(output))
}

fn preview_value(value: &Value) -> Option<Value> {
    match value {
        Value::Null => None,
        Value::String(text) => preview_text(text, PREVIEW_CHARS).map(Value::String),
        Value::Bool(_) | Value::Number(_) => Some(value.clone()),
        Value::Array(items) => {
            let values = items
                .iter()
                .filter_map(preview_value)
                .take(4)
                .collect::<Vec<_>>();
            (!values.is_empty()).then_some(Value::Array(values))
        }
        Value::Object(_) => serde_json::to_string(value)
            .ok()
            .and_then(|text| preview_text(text, PREVIEW_CHARS))
            .map(Value::String),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_hook_names() {
        assert_eq!(map_hook_status("UserPromptSubmit"), AgentStatus::Working);
        assert_eq!(map_hook_status("SessionStart"), AgentStatus::Idle);
        assert_eq!(map_hook_status("PreToolUse"), AgentStatus::ToolRunning);
        assert_eq!(map_hook_status("PostToolUse"), AgentStatus::Working);
        assert_eq!(map_hook_status("PermissionRequest"), AgentStatus::Waiting);
        assert_eq!(map_hook_status("PreCompact"), AgentStatus::Compacting);
        assert_eq!(map_hook_status("Stop"), AgentStatus::Done);
        assert_eq!(map_hook_status("StopFailure"), AgentStatus::Error);
        assert_eq!(map_hook_status("SubagentStop"), AgentStatus::Working);
        assert_eq!(map_hook_status("SubagentStopFailure"), AgentStatus::Working);
        assert_eq!(map_hook_status("PermissionDenied"), AgentStatus::Waiting);
        assert_eq!(map_hook_status("PostToolUseFailure"), AgentStatus::Working);
        assert_eq!(map_hook_status("Notification"), AgentStatus::Waiting);
        assert_eq!(map_hook_status("SessionEnd"), AgentStatus::Idle);
    }

    #[test]
    fn parses_snake_case_payload() {
        let raw = r#"{
            "session_id": "s1",
            "hook_event_name": "PreToolUse",
            "cwd": "D:\\C\\Desktop\\ai\\8bit",
            "tool_name": "Bash",
            "tool_input": {"command": "cargo test"},
            "pid": 123
        }"#;
        let event = ClaudeHookEvent::from_json(raw)
            .unwrap()
            .into_session_event(42)
            .unwrap();
        assert_eq!(event.session_id, "s1");
        assert_eq!(event.status, AgentStatus::ToolRunning);
        assert_eq!(event.tool_name.as_deref(), Some("Bash"));
        assert_eq!(event.pid, Some(123));
        assert!(event.tool_input_preview.unwrap().contains("cargo test"));
    }

    #[test]
    fn parses_camel_case_payload() {
        let raw = r#"{
            "sessionId": "s2",
            "hookEventName": "PermissionRequest",
            "cwd": "D:\\repo",
            "toolName": "Edit",
            "toolInput": {"file_path": "src/main.rs"}
        }"#;
        let event = ClaudeHookEvent::from_json(raw)
            .unwrap()
            .into_session_event(100)
            .unwrap();
        assert_eq!(event.session_id, "s2");
        assert_eq!(event.status, AgentStatus::Waiting);
        assert!(event.needs_user);
        assert_eq!(event.tool_name.as_deref(), Some("Edit"));
    }

    #[test]
    fn write_preview_keeps_target_path_not_content() {
        let raw = r#"{
            "session_id": "s3",
            "hook_event_name": "PreToolUse",
            "cwd": "D:\\repo",
            "tool_name": "Write",
            "tool_input": {
                "file_path": "D:\\repo\\reviews.py",
                "content": "Steam 评测爬取 + 分析一体化脚本，包含很多很多正文内容"
            }
        }"#;
        let event = ClaudeHookEvent::from_json(raw)
            .unwrap()
            .into_session_event(100)
            .unwrap();
        let preview = event.tool_input_preview.unwrap();
        assert!(preview.contains("reviews.py"));
        assert!(!preview.contains("content"));
        assert!(!preview.contains("Steam 评测"));
    }

    #[test]
    fn agent_preview_omits_large_prompt() {
        let raw = r#"{
            "session_id": "s4",
            "hook_event_name": "PreToolUse",
            "cwd": "D:\\repo",
            "tool_name": "Agent",
            "tool_input": {
                "description": "Review UI overlap",
                "subagent_type": "explorer",
                "prompt": "A very long delegated prompt that should not be shown in the tray"
            }
        }"#;
        let event = ClaudeHookEvent::from_json(raw)
            .unwrap()
            .into_session_event(100)
            .unwrap();
        let preview = event.tool_input_preview.unwrap();
        assert!(preview.contains("Review UI overlap"));
        assert!(preview.contains("explorer"));
        assert!(!preview.contains("delegated prompt"));
    }

    #[test]
    fn missing_session_id_is_error() {
        let raw = r#"{"hook_event_name":"Stop"}"#;
        let err = ClaudeHookEvent::from_json(raw)
            .unwrap()
            .into_session_event(0)
            .unwrap_err();
        assert!(err.contains("session_id"));
    }
}
