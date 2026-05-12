use rig::agent::PromptHook;
use rig::agent::ToolCallHookAction;
use tracing::warn;

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
            match tool_name {
                "shell" => {
                    if is_dangerous_command(&cmd_lower) {
                        warn!(command = %args, "工具调用被安全策略拦截");
                        ToolCallHookAction::Skip {
                            reason: "此命令被安全策略阻止，可能造成数据丢失或系统损坏".into(),
                        }
                    } else {
                        ToolCallHookAction::Continue
                    }
                }
                _ => ToolCallHookAction::Continue,
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
