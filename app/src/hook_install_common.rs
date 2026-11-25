//! hook / 转发器安装公共工具。
//!
//! claude_hooks、codex_hooks、pi_hooks、opencode_hooks 都需要"原子写入脚本 +
//! 内容比对 + 写前备份"三件套。集中在这里实现，避免每个安装器各维护一份
//! 细微不同的文件操作逻辑。所有函数失败时返回带上下文的中文错误信息，
//! 不静默吞错。

use std::path::Path;
use tracing::info;

/// 内容一致时跳过写入，用于幂等安装。
pub fn file_content_matches(path: &Path, expected: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|actual| actual == expected)
        .unwrap_or(false)
}

/// 原子写入：先写同目录临时文件再 rename，避免半写入文件被宿主 agent 读到。
pub fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败: {e}"))?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| format!("替换文件失败: {e}"))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("保存文件失败: {e}"))
}

/// 覆盖用户配置前先备份；时间戳后缀由调用方决定扩展名风格。
pub fn backup_if_exists(path: &Path, backup_suffix: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let backup = path.with_file_name(format!("{backup_suffix}-{stamp}"));
    std::fs::copy(path, &backup).map_err(|e| format!("备份 {} 失败: {e}", path.display()))?;
    Ok(())
}

/// 清理 hooks 目录中已不再被任何配置引用的遗留脚本。
///
/// 只删除文件名匹配已知旧产品前缀的 `.ps1`（ai-pad-、ooclaw-），
/// 不碰当前 bitcat 脚本和用户自己的文件。返回删除数量。
pub fn cleanup_stale_scripts(hooks_dir: &Path) -> usize {
    const STALE_PREFIXES: &[&str] = &["ai-pad-", "ooclaw-"];
    let Ok(entries) = std::fs::read_dir(hooks_dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".ps1") {
            continue;
        }
        if STALE_PREFIXES.iter().any(|p| name.starts_with(p))
            && std::fs::remove_file(entry.path()).is_ok()
        {
            info!(file = %name, "removed stale hook script");
            removed += 1;
        }
    }
    removed
}
