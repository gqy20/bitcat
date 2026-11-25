//! 编程助手连接器：统一的状态探测与修复入口。
//!
//! Claude Code / Codex / pi / opencode 各有独立安装器（claude_hooks、codex_hooks、
//! pi_hooks、opencode_hooks），本模块把"是否已接入"与"修复"归一成两个 Tauri 命令，
//! 设置页「编程助手看管」的连接列表据此渲染；新增受看管的 CLI 时在 `CONNECTORS`
//! 表加一行、在 `repair` 加一个分支即可，前端 `js/agent_sources.js` 同步一份展示名。

use crate::{claude_hooks, codex_hooks, opencode_hooks, pi_hooks, zcode_hooks};
use bitcat_core::agent_session::AgentSource;
use serde::Serialize;

/// 单个连接器的当前状态，设置页连接列表的一行。
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorStatus {
    /// snake_case 来源 id，与 AgentWatch 会话的 `source` 字段同词。
    pub source: String,
    pub installed: bool,
    /// 已安装脚本是否与当前代码生成的版本一致；未安装时为 false。
    pub script_current: bool,
}

/// 安装探测与幂等修复的函数指针签名（各安装模块同名约定）。
type ProbeFn = fn() -> bool;
type InstallFn = fn() -> Result<String, String>;

/// 受支持的连接器表：`(来源, 安装探测, 脚本新鲜度探测, 幂等修复)`。
type ConnectorSpec = (AgentSource, ProbeFn, ProbeFn, InstallFn);
const CONNECTORS: &[ConnectorSpec] = &[
    (
        AgentSource::ClaudeCode,
        claude_hooks::is_installed,
        claude_hooks::is_script_current,
        claude_hooks::install_claude_code_hooks,
    ),
    (
        AgentSource::Codex,
        codex_hooks::is_installed,
        codex_hooks::is_script_current,
        codex_hooks::install_codex_hooks,
    ),
    (
        AgentSource::ZCode,
        zcode_hooks::is_installed,
        zcode_hooks::is_script_current,
        zcode_hooks::install_zcode_hooks,
    ),
    (
        AgentSource::Pi,
        pi_hooks::is_installed,
        pi_hooks::is_script_current,
        pi_hooks::install_pi_extension,
    ),
    (
        AgentSource::OpenCode,
        opencode_hooks::is_installed,
        opencode_hooks::is_script_current,
        opencode_hooks::install_opencode_plugin,
    ),
];

/// 按给定探测结果组装状态行（测试注入用，避免触真实文件系统）。
fn connector_statuses_with(installed: &[bool], current: &[bool]) -> Vec<ConnectorStatus> {
    CONNECTORS
        .iter()
        .zip(installed.iter().zip(current.iter()))
        .map(|(source, (&ok, &fresh))| ConnectorStatus {
            source: source.0.as_str().to_string(),
            installed: ok,
            script_current: ok && fresh,
        })
        .collect()
}

/// 全部连接器的实时安装状态（每次调用都重新探测，文件操作开销可忽略）。
pub fn connector_statuses() -> Vec<ConnectorStatus> {
    let probes: Vec<_> = CONNECTORS.iter().map(|(_, probe, _, _)| probe()).collect();
    let freshness: Vec<_> = CONNECTORS.iter().map(|(_, _, fresh, _)| fresh()).collect();
    connector_statuses_with(&probes, &freshness)
}

/// 幂等修复单个来源：重新写入其连接脚本，成功时返回人话消息。
fn repair(source: AgentSource) -> Result<String, String> {
    for (candidate, _, _, install) in CONNECTORS.iter() {
        if *candidate == source {
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
        let rows = connector_statuses_with(
            &[true, false, false, true, false],
            &[true, false, false, false, false],
        );
        let sources: Vec<&str> = rows.iter().map(|row| row.source.as_str()).collect();
        assert_eq!(
            sources,
            vec![
                AgentSource::ClaudeCode.as_str(),
                AgentSource::Codex.as_str(),
                AgentSource::ZCode.as_str(),
                AgentSource::Pi.as_str(),
                AgentSource::OpenCode.as_str()
            ]
        );
        assert!(!rows[1].installed);
        assert!(!rows[2].installed);
        assert!(!rows[4].installed);
        assert!(rows[0].script_current);
        assert!(!rows[3].script_current);
    }

    #[test]
    fn connector_table_round_trips_core_enum() {
        // 表必须恰好覆盖全部 AgentSource 变体（repair 靠它兜底），且每个来源
        // 都能用 as_str 被 from_envelope 认回，保证前端传回的 source id 与
        // 会话流里的 id 同一套词表。
        let all = [
            AgentSource::ClaudeCode,
            AgentSource::Codex,
            AgentSource::ZCode,
            AgentSource::Pi,
            AgentSource::OpenCode,
        ];
        for source in all {
            let count = CONNECTORS
                .iter()
                .filter(|(candidate, _, _, _)| *candidate == source)
                .count();
            assert_eq!(count, 1, "{source:?} 在 CONNECTORS 中应恰好出现一次");
            assert_eq!(AgentSource::from_envelope(source.as_str()), Some(source));
        }
    }
}
