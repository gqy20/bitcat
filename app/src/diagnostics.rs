//! 诊断包导出：把设备上的运行日志打包成单个 zip，供开发机离线分析。
//!
//! 目标场景：其他设备（主力机 / 远程机器）出现异常后，用户点一下
//! 「导出诊断包」，把 zip 发给开发机，用 `xtask logs analyze <zip>`
//! 直接出报告——**分析永远在信息全的一侧做**，现场设备不需要 Rust
//! 工具链。
//!
//! 包结构：`manifest.json`（设备/版本/时间窗/文件清单）+ 按时间窗截取的
//! `*.jsonl` / `app.log` 副本。脱敏在导出端完成：密钥形态正则打码，
//! 用户路径可选替换为 `<user>`（源头处理，分析端拿到的就是安全的）。

use serde::Serialize;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tracing::{info, warn};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use tauri_plugin_opener::OpenerExt;

const DEFAULT_HOURS: u64 = 24;
/// 每个源文件最多截取的行数上限，防止异常巨大的文件撑爆内存。
const MAX_LINES_PER_SOURCE: usize = 50_000;

#[derive(Serialize)]
struct Manifest {
    exported_at: String,
    device: String,
    os: String,
    app_version: String,
    time_window_hours: u64,
    redacted_paths: bool,
    files: Vec<ManifestFile>,
}

#[derive(Serialize)]
struct ManifestFile {
    name: String,
    lines: usize,
    total_bytes: usize,
}

/// 导出诊断包，返回 zip 路径。
#[tauri::command]
pub async fn cmd_export_diagnostics(
    app: AppHandle,
    hours: Option<u64>,
    redact_paths: Option<bool>,
) -> Result<String, String> {
    let hours = hours.unwrap_or(DEFAULT_HOURS).clamp(1, 24 * 30);
    let redact_paths = redact_paths.unwrap_or(true);
    let dir = bitcat_core::logging::log_dir().map_err(|e| e.to_string())?;
    let out_dir = dir.join("diagnostics");
    fs::create_dir_all(&out_dir).map_err(|e| format!("创建诊断目录失败: {e}"))?;

    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let zip_path = out_dir.join(format!("bitcat-diagnostics-{stamp}.zip"));

    let manifest = write_bundle(&dir, &zip_path, hours, redact_paths)?;

    info!(
        zip = %zip_path.display(),
        files = manifest.files.len(),
        hours,
        "diagnostics bundle exported"
    );
    // 打开所在文件夹，让用户直接看到产物。
    if let Some(parent) = zip_path.parent() {
        let _ = app
            .opener()
            .open_path(parent.to_string_lossy().to_string(), None::<String>);
    }
    Ok(zip_path.to_string_lossy().to_string())
}

fn write_bundle(
    log_dir: &Path,
    zip_path: &Path,
    hours: u64,
    redact_paths: bool,
) -> Result<Manifest, String> {
    let cutoff = chrono::Local::now() - chrono::Duration::hours(hours as i64);
    if let Some(parent) = zip_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建诊断目录失败: {e}"))?;
    }
    let zip_file = File::create(zip_path).map_err(|e| format!("创建 zip 失败: {e}"))?;
    let mut zip = ZipWriter::new(zip_file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let mut files = Vec::new();
    let mut entries: Vec<PathBuf> = match fs::read_dir(log_dir) {
        Ok(iter) => iter
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && matches!(
                        path.extension().and_then(|ext| ext.to_str()),
                        Some("jsonl") | Some("log") | Some("json")
                    )
            })
            .collect(),
        Err(e) => return Err(format!("读取日志目录失败: {e}")),
    };
    entries.sort();

    for path in entries {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let content = match filter_and_redact(&path, &cutoff, redact_paths) {
            Ok(content) => content,
            Err(e) => {
                warn!(file = %name, error = %e, "diagnostics source skipped");
                continue;
            }
        };
        if content.line_count == 0 {
            continue;
        }
        let bytes = content.text.as_bytes();
        zip.start_file(name.clone(), options)
            .map_err(|e| format!("zip 写入 {name} 失败: {e}"))?;
        zip.write_all(bytes)
            .map_err(|e| format!("zip 写入 {name} 失败: {e}"))?;
        files.push(ManifestFile {
            name,
            lines: content.line_count,
            total_bytes: bytes.len(),
        });
    }

    let manifest = Manifest {
        exported_at: chrono::Local::now().to_rfc3339(),
        device: whoami_dev(),
        os: std::env::consts::OS.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        time_window_hours: hours,
        redacted_paths: redact_paths,
        files,
    };
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    zip.start_file("manifest.json", options)
        .map_err(|e| format!("写入 manifest 失败: {e}"))?;
    zip.write_all(manifest_json.as_bytes())
        .map_err(|e| format!("写入 manifest 失败: {e}"))?;
    zip.finish().map_err(|e| format!("zip 收尾失败: {e}"))?;
    Ok(manifest)
}

struct FilteredContent {
    text: String,
    line_count: usize,
}

/// 按时间窗截取 + 脱敏。
/// - `.jsonl`：解析每行 timestamp 字段，窗口外丢弃；
/// - `.log` / 其他：行首 RFC3339 时间戳能解析则过滤，解析失败保留（宁多勿缺）；
/// - 窗口判断失败一律保留该行（分析端可再筛）。
fn filter_and_redact(
    path: &Path,
    cutoff: &chrono::DateTime<chrono::Local>,
    redact_paths: bool,
) -> Result<FilteredContent, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    // 整体 JSON 文件（非逐行 JSONL）没有可按行过滤的时间戳，整体保留。
    if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        let redacted = redact_line(&raw, redact_paths);
        let count = redacted.lines().count();
        return Ok(FilteredContent {
            text: redacted,
            line_count: count,
        });
    }
    let mut out = String::with_capacity(raw.len());
    let mut count = 0usize;
    for line in raw.lines() {
        if count >= MAX_LINES_PER_SOURCE {
            break;
        }
        if line_in_window(line, cutoff) {
            out.push_str(&redact_line(line, redact_paths));
            out.push('\n');
            count += 1;
        }
    }
    Ok(FilteredContent {
        text: out,
        line_count: count,
    })
}

fn line_in_window(line: &str, cutoff: &chrono::DateTime<chrono::Local>) -> bool {
    let timestamp = extract_timestamp(line);
    match timestamp.and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok()) {
        Some(time) => time.with_timezone(&chrono::Local) >= *cutoff,
        // 无时间戳（JSONL 损坏行 / 日志头部）：保留，交给分析端判断。
        None => true,
    }
}

/// 提取行内首个 RFC3339 时间戳。
/// - app.log：行首 tracing 输出 `YYYY-MM-DDTHH:MM:SS[.frac]±HH:MM INFO ...`，
///   截到第一个空白即可解析；
/// - jsonl：`"timestamp":"..."` 字段。
fn extract_timestamp(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("20") || trimmed.starts_with("19") {
        let candidate_end = trimmed
            .find(char::is_whitespace)
            .unwrap_or(trimmed.len())
            .min(40);
        let candidate = &trimmed[..candidate_end];
        if chrono::DateTime::parse_from_rfc3339(candidate).is_ok() {
            return Some(candidate);
        }
    }
    // JSONL timestamp 字段：从 `"timestamp"` 键之后开始，第一个引号是值的开头。
    if let Some(start) = line.find("\"timestamp\"") {
        let rest = &line[start + "\"timestamp\"".len()..];
        if let Some(q1) = rest.find('"') {
            let after = &rest[q1 + 1..];
            if let Some(q2) = after.find('"') {
                let value = &after[..q2];
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
    }
    None
}

/// 脱敏：密钥形态打码（永远启用）；用户路径替换（可选）。
fn redact_line(line: &str, redact_paths: bool) -> String {
    let mut out = line.to_string();
    // Bearer / API key / token 赋值形态。
    for marker in [
        "authorization:",
        "api_key:",
        "apikey:",
        "token:",
        "password:",
        "secret:",
    ] {
        if let Some(pos) = out.to_ascii_lowercase().find(marker) {
            let value_start = pos + marker.len();
            let value_end = out[value_start..]
                .find(|c: char| c.is_whitespace() || c == ',' || c == '"' || c == '\'')
                .map(|end| value_start + end)
                .unwrap_or(out.len());
            out.replace_range(value_start..value_end, "[REDACTED]");
        }
    }
    if let Some(pos) = out.find("Bearer ") {
        let value_start = pos + "Bearer ".len();
        let value_end = out[value_start..]
            .find(|c: char| c.is_whitespace())
            .map(|end| value_start + end)
            .unwrap_or(out.len());
        out.replace_range(value_start..value_end, "[REDACTED]");
    }
    // sk- 开头的 OpenAI 风格密钥（单次替换，避免重复命中）。
    if let Some(pos) = out.find("sk-") {
        let value_end = out[pos..]
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
            .map(|end| pos + end)
            .unwrap_or(out.len());
        if value_end - pos > 8 {
            out.replace_range(pos..value_end, "sk-[REDACTED]");
        }
    }
    if redact_paths {
        out = redact_user_paths(&out);
    }
    out
}

/// 把用户名从路径中替换掉：/home/<user>、/Users/<user>、C:\Users\<user>。
/// 逐次推进搜索起点，避免替换产物 `<user>` 再次命中前缀造成死循环。
fn redact_user_paths(line: &str) -> String {
    let mut out = line.to_string();
    for prefix in ["/home/", "/Users/"] {
        let mut search_from = 0usize;
        while let Some(rel) = out[search_from..].find(prefix) {
            let pos = search_from + rel;
            let name_start = pos + prefix.len();
            let name_end = out[name_start..]
                .find(|c: char| c == '/' || c.is_whitespace() || c == '"' || c == '\'')
                .map(|end| name_start + end)
                .unwrap_or(out.len());
            if name_end <= name_start {
                break;
            }
            out.replace_range(name_start..name_end, "<user>");
            search_from = name_start + "<user>".len();
        }
    }
    let mut search_from = 0usize;
    while let Some(rel) = out[search_from..].find("Users\\") {
        let pos = search_from + rel;
        let name_start = pos + "Users\\".len();
        let name_end = out[name_start..]
            .find(|c: char| c == '\\' || c.is_whitespace() || c == '"' || c == '\'')
            .map(|end| name_start + end)
            .unwrap_or(out.len());
        if name_end <= name_start {
            break;
        }
        out.replace_range(name_start..name_end, "<user>");
        search_from = name_start + "<user>".len();
    }
    out
}

fn whoami_dev() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_timestamp_handles_jsonl_and_log_lines() {
        let jsonl = r#"{"timestamp":"2026-09-12T10:00:00+08:00","event":"x"}"#;
        assert_eq!(extract_timestamp(jsonl), Some("2026-09-12T10:00:00+08:00"));
        let log = "2026-09-12T10:00:00.123+08:00 INFO bitcat_app: message";
        assert!(extract_timestamp(log).is_some());
        assert_eq!(extract_timestamp("no timestamp here"), None);
    }

    #[test]
    fn redact_line_masks_secrets_and_paths() {
        let line = r#"{"cmd":"curl -H \"Authorization: Bearer abc123\" sk-0123456789abcdef /home/qy113/x"}"#;
        let redacted = redact_line(line, true);
        assert!(redacted.contains("Bearer [REDACTED]"));
        assert!(redacted.contains("sk-[REDACTED]"));
        assert!(redacted.contains("/home/<user>/x"));
        assert!(!redacted.contains("abc123"));
        assert!(!redacted.contains("qy113"));
    }

    #[test]
    fn line_in_window_filters_old_lines() {
        let cutoff = chrono::Local::now() - chrono::Duration::hours(24);
        // 无时间戳行：保留（宁多勿缺）。
        assert!(line_in_window("no timestamp here", &cutoff));
        // 新事件：保留。
        let fresh = format!(
            "{{\"timestamp\":\"{}\"}}",
            chrono::Local::now().to_rfc3339()
        );
        assert!(line_in_window(&fresh, &cutoff));
        // 远古事件：过滤。
        let old = "{\"timestamp\":\"2020-01-01T00:00:00+08:00\"}";
        assert!(!line_in_window(old, &cutoff));
    }

    /// 显式运行（`cargo test -- --ignored`）时用真实 ~/.bitcat/logs 导出
    /// 一个诊断包，供 `xtask logs analyze` 端到端联调。
    #[test]
    #[ignore = "读写真实日志目录，仅手动联调时运行"]
    fn e2e_export_real_bundle() {
        let dir = bitcat_core::logging::log_dir().expect("log dir");
        let out = dir.join("diagnostics").join("e2e-test-bundle.zip");
        let manifest = write_bundle(&dir, &out, 72, true).expect("bundle");
        println!("bundle: {} (files={})", out.display(), manifest.files.len());
        for file in &manifest.files {
            println!("  {} lines={}", file.name, file.lines);
        }
        assert!(out.exists());
    }
}
