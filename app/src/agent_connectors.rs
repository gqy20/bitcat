//! 编程助手连接器：统一的状态探测与修复入口。
//!
//! Claude Code / Codex / pi / opencode 各有独立安装器（claude_hooks、codex_hooks、
//! pi_hooks、opencode_hooks），本模块把"是否已接入"与"修复"归一成两个 Tauri 命令，
//! 设置页「编程助手看管」的连接列表据此渲染；新增受看管的 CLI 时在 `CONNECTORS`
//! 表加一行、在 `repair` 加一个分支即可，前端 `js/agent_sources.js` 同步一份展示名。

use crate::{claude_hooks, codex_hooks, opencode_hooks, pi_hooks};
use bitcat_core::agent_session::AgentSource;
use serde::Serialize;

/// 单个连接器的当前状态，设置页连接列表的一行。
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorStatus {
    /// snake_case 来源 id，与 AgentWatch 会话的 `source` 字段同词。
    pub source: String,
    pub installed: bool,
}

/// 安装探测与幂等修复的函数指针签名（各安装模块同名约定）。
type ProbeFn = fn() -> bool;
type InstallFn = fn() -> Result<String, String>;

/// 受支持的连接器表：`(来源, 安装探测, 幂等修复)`，顺序即设置页展示顺序。
const CONNECTORS: [(AgentSource, ProbeFn, InstallFn); 4] = [
    (
        AgentSource::ClaudeCode,
        claude_hooks::is_installed,
        claude_hooks::install_claude_code_hooks,
    ),
    (
        AgentSource::Codex,
        codex_hooks::is_installed,
        codex_hooks::install_codex_hooks,
    ),
    (
        AgentSource::Pi,
        pi_hooks::is_installed,
        pi_hooks::install_pi_extension,
    ),
    (
        AgentSource::OpenCode,
        opencode_hooks::is_installed,
        opencode_hooks::install_opencode_plugin,
    ),
];

/// 按给定探测结果组装状态行（测试注入用，避免触真实文件系统）。
fn connector_statuses_with(installed: &[bool; 4]) -> Vec<ConnectorStatus> {
    CONNECTORS
        .iter()
        .zip(installed)
        .map(|(source, &ok)| ConnectorStatus {
            source: source.0.as_str().to_string(),
            installed: ok,
        })
        .collect()
}

/// 全部连接器的实时安装状态（每次调用都重新探测，文件操作开销可忽略）。
pub fn connector_statuses() -> Vec<ConnectorStatus> {
    let probes = CONNECTORS.map(|(_, probe, _)| probe());
    connector_statuses_with(&probes)
}

/// 幂等修复单个来源：重新写入其连接脚本，成功时返回人话消息。
fn repair(source: AgentSource) -> Result<String, String> {
    for (candidate, _, install) in CONNECTORS {
        if candidate == source {
            return install();
        }
    }
    unreachable!("CONNECTORS 必须覆盖全部 AgentSource 变体")
}

/// 设置页连接列表的状态探测。
#[tauri::command]
pub async fn cmd_agent_connectors_status() -> Result<Vec<ConnectorStatus>, String> {
    Ok(connector_statuses())
}

/// 设置页"修复"动作：`source` 是会话同款 snake_case id。
#[tauri::command]
pub async fn cmd_repair_connector(source: String) -> Result<String, String> {
    let parsed = AgentSource::from_envelope(&source)
        .ok_or_else(|| format!("未知的编程助手来源：{source}"))?;
    repair(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_cover_all_sources_in_display_order() {
        let rows = connector_statuses_with(&[true, false, true, false]);
        let sources: Vec<&str> = rows.iter().map(|row| row.source.as_str()).collect();
        assert_eq!(
            sources,
            vec![
                AgentSource::ClaudeCode.as_str(),
                AgentSource::Codex.as_str(),
                AgentSource::Pi.as_str(),
                AgentSource::OpenCode.as_str()
            ]
        );
        assert_eq!(rows[1].installed, false);
        assert_eq!(rows[3].installed, false);
    }

    #[test]
    fn connector_table_round_trips_core_enum() {
        // 表必须恰好覆盖全部 AgentSource 变体（repair 靠它兜底），且每个来源
        // 都能用 as_str 被 from_envelope 认回，保证前端传回的 source id 与
        // 会话流里的 id 同一套词表。
        let all = [
            AgentSource::ClaudeCode,
            AgentSource::Codex,
            AgentSource::Pi,
            AgentSource::OpenCode,
        ];
        for source in all {
            let count = CONNECTORS
                .iter()
                .filter(|(candidate, _, _)| *candidate == source)
                .count();
            assert_eq!(count, 1, "{source:?} 在 CONNECTORS 中应恰好出现一次");
            assert_eq!(AgentSource::from_envelope(source.as_str()), Some(source));
        }
    }
}
