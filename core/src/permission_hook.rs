//! AI 工具调用安全策略钩子
//!
//! 拦截 Agent 的 shell 工具调用，通过黑名单模式阻止危险命令（rm -rf、format、shutdown 等）。
//! 非 shell 工具直接放行。作为 rig PromptHook 注册到 Agent 流水线中。

use rig::agent::PromptHook;
use rig::agent::ToolCallHookAction;
use tracing::{info, warn};

use crate::logging::log_preview;

/// shell 工具被安全策略拦截时返回给模型的稳定原因。
pub const POLICY_BLOCK_REASON: &str = "此命令被安全策略阻止，可能造成数据丢失或系统损坏";
/// Tool calls disabled by the user's release-facing permission settings.
pub const PERMISSION_DISABLED_REASON: &str =
    "此能力已在 BitCat 权限设置中关闭，需要用户在设置页手动开启后才能使用";

/// 空结构体，实现 rig 的 PromptHook trait，在工具调用前进行安全检查
#[derive(Clone)]
pub struct PermissionHook;

impl<M: rig::completion::CompletionModel> PromptHook<M> for PermissionHook {
    fn on_tool_call(
        &self,
        tool_name: &str,
        _call_id: Option<String>,
        _internal_call_id: &str,
        args: &str,
    ) -> impl std::future::Future<Output = ToolCallHookAction> + Send {
        let cmd_lower = args.to_lowercase();
        async move {
            if let Some(reason) = disabled_by_settings(tool_name) {
                warn!(tool = %tool_name, reason = %reason, "tool call blocked by permission settings");
                return ToolCallHookAction::Skip { reason };
            }

            match tool_name {
                "shell" => {
                    let command_preview = log_preview(args, 120);
                    if is_dangerous_command(&cmd_lower) {
                        warn!(
                            command_chars = args.chars().count(),
                            command_preview = %command_preview,
                            "tool call blocked by policy"
                        );
                        ToolCallHookAction::Skip {
                            reason: POLICY_BLOCK_REASON.into(),
                        }
                    } else {
                        info!(
                            command_chars = args.chars().count(),
                            command_preview = %command_preview,
                            "shell tool call allowed"
                        );
                        ToolCallHookAction::Continue
                    }
                }
                _ => {
                    info!(tool = %tool_name, "非 shell 工具调用放行");
                    ToolCallHookAction::Continue
                }
            }
        }
    }
}

/// 检查命令是否包含危险操作
fn is_dangerous_command(cmd: &str) -> bool {
    let cmd_lower = cmd.to_lowercase();
    let dangerous = [
        // 文件删除
        "rm -rf",
        "del /s /q",
        "remove-item -recurse -force",
        // 磁盘格式化
        "format ",
        "format-volume",
        // 关机/重启
        "shutdown",
        "restart-computer",
        // 用户/组管理
        "net user",
        "net localgroup",
        // 注册表危险操作
        "reg delete",
        "remove-itemproperty -path hk",
        // 进程终止
        "taskkill /f",
        "stop-process -force",
        // 下载执行（远程代码）
        "invoke-webrequest -outfile",
        "iwr -o",
        // 清空磁盘
        "cipher /w",
        "sdelete",
    ];
    dangerous.iter().any(|pattern| cmd_lower.contains(pattern))
}

/// 判断工具结果是否来自 PermissionHook 的安全策略拦截。
pub fn is_policy_block_reason(text: &str) -> bool {
    text == POLICY_BLOCK_REASON || text == PERMISSION_DISABLED_REASON
}

fn disabled_by_settings(tool_name: &str) -> Option<String> {
    let permissions = crate::app_settings::AppSettings::load().permissions;
    disabled_by_permissions(tool_name, &permissions)
}

fn disabled_by_permissions(
    tool_name: &str,
    permissions: &crate::app_settings::PermissionSettings,
) -> Option<String> {
    let allowed = match tool_name {
        "shell" => permissions.allow_shell_tool,
        "read_file" => permissions.allow_read_file_tool,
        "read_clipboard" => permissions.allow_clipboard_tool,
        "force_foreground" => permissions.allow_foreground_tool,
        "launch_program" => permissions.allow_launch_program_tool,
        "send_hotkey" => permissions.allow_hotkey_tool,
        _ => return None,
    };
    if allowed {
        None
    } else {
        Some(PERMISSION_DISABLED_REASON.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_block_high_risk_tools() {
        let permissions = crate::app_settings::PermissionSettings::default();
        assert_eq!(
            disabled_by_permissions("shell", &permissions).as_deref(),
            Some(PERMISSION_DISABLED_REASON)
        );
        assert_eq!(
            disabled_by_permissions("read_file", &permissions).as_deref(),
            Some(PERMISSION_DISABLED_REASON)
        );
        assert!(disabled_by_permissions("get_time", &permissions).is_none());
    }
}
