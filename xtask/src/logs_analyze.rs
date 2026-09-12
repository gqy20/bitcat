//! 诊断包分析：读取导出端生成的 zip，输出合并诊断报告（L2 闭环的分析侧）。
//!
//! 使用：`cargo run -p xtask -- logs analyze <bitcat-diagnostics-xxx.zip>`
//! 设计原则：分析永远在信息全的一侧做——现场设备只负责导出（脱敏已完成），
//! 这里零修改地读包出结论，不需要目标设备安装任何工具链。
//!
//! 报告内容：manifest 摘要 → app.log 异常统计 → 各 JSONL 概况 →
//! chat 链路（session_id 聚合 tool/token）→ 资源曲线摘要。

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize)]
struct Manifest {
    exported_at: String,
    device: String,
    os: String,
    app_version: String,
    time_window_hours: u64,
    files: Vec<ManifestFile>,
}

#[derive(Deserialize)]
struct ManifestFile {
    name: String,
    lines: usize,
}

#[derive(Deserialize)]
struct ToolRecord {
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    success: Option<bool>,
}

#[derive(Deserialize)]
struct TokenRecord {
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

#[derive(Deserialize)]
struct ResourceRecord {
    #[serde(default)]
    process_memory_mb: f64,
    #[serde(default)]
    webview_windows: usize,
    #[serde(default)]
    timestamp: String,
}

pub fn run(zip_path: &Path) -> Result<()> {
    let sources = read_bundle(zip_path)?;
    print_report(zip_path, &sources);
    Ok(())
}

struct Bundle {
    manifest: Manifest,
    /// 文件名 → 全文行列表。
    files: BTreeMap<String, Vec<String>>,
}

fn read_bundle(zip_path: &Path) -> Result<Bundle> {
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut files = BTreeMap::new();
    let mut manifest: Option<Manifest> = None;

    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx)?;
        let name = entry.name().to_string();
        let mut text = String::new();
        entry.read_to_string(&mut text)?;
        if name == "manifest.json" {
            manifest = Some(serde_json::from_str(&text)?);
            continue;
        }
        let lines: Vec<String> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_string)
            .collect();
        files.insert(name, lines);
    }

    let manifest = manifest.ok_or("诊断包缺少 manifest.json")?;
    Ok(Bundle { manifest, files })
}

fn print_report(zip_path: &Path, bundle: &Bundle) {
    println!("BitCat 诊断包分析");
    println!("来源：{}", zip_path.display());
    println!("{}", "=".repeat(66));
    println!(
        "设备 {} · {} · app {} · 窗口 {}h · 导出于 {}",
        bundle.manifest.device,
        bundle.manifest.os,
        bundle.manifest.app_version,
        bundle.manifest.time_window_hours,
        bundle.manifest.exported_at
    );

    println!("\n[文件清单]");
    if bundle.manifest.files.is_empty() {
        println!("  （manifest 未记录文件——旧格式？直接读 zip 内容）");
    }
    for entry in &bundle.manifest.files {
        println!("  {:>7} 行  {}", entry.lines, entry.name);
    }

    // app.log：异常统计 + 最近错误。
    for (name, lines) in bundle.files.iter() {
        if name.ends_with(".log") {
            summarize_log(name, lines);
        }
    }

    // tool_events：工具调用概况。
    if let Some(lines) = bundle.files.get("tool_events.jsonl") {
        let records: Vec<ToolRecord> = lines
            .iter()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        if !records.is_empty() {
            println!("\n[工具调用] tool_events.jsonl（{} 条）", records.len());
            let mut by_tool: BTreeMap<&str, usize> = BTreeMap::new();
            let mut failures = 0usize;
            for record in &records {
                *by_tool.entry(record.tool_name.as_str()).or_default() += 1;
                if record.success == Some(false) {
                    failures += 1;
                }
            }
            println!("  失败 {} 次 / 共 {} 次", failures, records.len());
            let mut entries: Vec<_> = by_tool.into_iter().collect();
            entries.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            for (tool, n) in entries.iter().take(8) {
                println!("  {n:5}  {tool}");
            }
        }
    }

    // token_usage：链路用量。
    if let Some(lines) = bundle.files.get("token_usage.jsonl") {
        let records: Vec<TokenRecord> = lines
            .iter()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        if !records.is_empty() {
            println!("\n[API 用量] token_usage.jsonl（{} 条）", records.len());
            let mut by_cat: BTreeMap<&str, (u64, u64, u64)> = BTreeMap::new();
            for record in &records {
                let entry = by_cat.entry(record.category.as_str()).or_default();
                entry.0 += 1;
                entry.1 += record.input_tokens;
                entry.2 += record.output_tokens;
            }
            let mut entries: Vec<_> = by_cat.into_iter().collect();
            entries.sort_by_key(|(_, (n, _, _))| std::cmp::Reverse(*n));
            for (cat, (n, tin, tout)) in entries {
                println!("  {n:5} 次  in={tin} out={tout}  {cat}");
            }
        }
    }

    // chat 链路：session_id 聚合 tool + token。
    print_chat_chains(bundle);

    // 资源曲线。
    if let Some(lines) = bundle.files.get("resource_usage.jsonl") {
        let records: Vec<ResourceRecord> = lines
            .iter()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        if !records.is_empty() {
            println!(
                "\n[资源曲线] resource_usage.jsonl（{} 个采样）",
                records.len()
            );
            let memories: Vec<f64> = records.iter().map(|r| r.process_memory_mb).collect();
            let windows: Vec<usize> = records.iter().map(|r| r.webview_windows).collect();
            let max_mem = memories.iter().cloned().fold(0.0, f64::max);
            let avg_mem = memories.iter().sum::<f64>() / memories.len() as f64;
            let max_win = windows.iter().cloned().max().unwrap_or(0);
            println!(
                "  进程内存 avg={avg_mem:.0}MB max={max_mem:.0}MB · WebView 窗口峰值 {max_win}"
            );
            println!(
                "  首采样 {} → 末采样 {}",
                records.first().map(|r| r.timestamp.as_str()).unwrap_or("?"),
                records.last().map(|r| r.timestamp.as_str()).unwrap_or("?")
            );
        }
    }

    println!("\n{}", "=".repeat(66));
    println!("提示：chat 链路的 session_id 可在 app.log 中 grep 定位完整对话过程。");
}

fn summarize_log(name: &str, lines: &[String]) {
    let mut warn_count = 0usize;
    let mut error_count = 0usize;
    let mut last_errors: Vec<&String> = Vec::new();
    for line in lines {
        // tracing 默认输出：LEVEL 在时间戳之后，如 `2026-.. INFO target: msg`。
        let upper = line.to_ascii_uppercase();
        if upper.contains(" ERROR ") {
            error_count += 1;
            last_errors.push(line);
            if last_errors.len() > 5 {
                last_errors.remove(0);
            }
        } else if upper.contains(" WARN ") {
            warn_count += 1;
        }
    }
    println!("\n[运行日志] {name}（{} 行）", lines.len());
    println!("  ERROR {error_count} · WARN {warn_count}");
    if !last_errors.is_empty() {
        println!("  —— 最近错误（最多 5 条）:");
        for line in last_errors {
            let preview: String = line.chars().take(160).collect();
            println!("  | {preview}");
        }
    }
}

/// 按 session_id 聚合 tool_events + token_usage，还原每次对话的链路。
fn print_chat_chains(bundle: &Bundle) {
    let tools: Vec<ToolRecord> = bundle
        .files
        .get("tool_events.jsonl")
        .map(|lines| {
            lines
                .iter()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        })
        .unwrap_or_default();
    let tokens: Vec<TokenRecord> = bundle
        .files
        .get("token_usage.jsonl")
        .map(|lines| {
            lines
                .iter()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        })
        .unwrap_or_default();
    if tools.is_empty() && tokens.is_empty() {
        return;
    }

    let mut chains: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut chain_tokens: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    for tool in &tools {
        if tool.session_id.is_empty() {
            continue;
        }
        chains
            .entry(tool.session_id.as_str())
            .or_default()
            .push(tool.tool_name.as_str());
    }
    for token in &tokens {
        if token.session_id.is_empty() {
            continue;
        }
        let entry = chain_tokens.entry(token.session_id.as_str()).or_default();
        entry.0 += token.input_tokens;
        entry.1 += token.output_tokens;
    }

    println!(
        "\n[对话链路] 共 {} 条（session_id → 工具序列 → 用量）",
        chains.len()
    );
    let mut sorted: Vec<_> = chains.into_iter().collect();
    sorted.reverse(); // 最近链路大概率在末尾生成，倒序先展示。
    for (session_id, tool_names) in sorted.iter().take(10) {
        let (tin, tout) = chain_tokens.get(*session_id).copied().unwrap_or((0, 0));
        let tools_preview = if tool_names.len() > 6 {
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for name in tool_names {
                *counts.entry(name).or_default() += 1;
            }
            format!("{}（{} 次调用）", tool_names.len(), counts.len())
        } else {
            tool_names.join(" → ")
        };
        println!("  {session_id}");
        println!("    工具: {tools_preview} · tokens in={tin} out={tout}");
    }
    if sorted.len() > 10 {
        println!("  … 其余 {} 条略", sorted.len() - 10);
    }
}

/// 解析 `logs analyze <path>` 参数。
pub fn parse_args(mut args: Vec<String>) -> Result<PathBuf> {
    match args.pop().as_deref() {
        Some(path) if Path::new(path).exists() => Ok(PathBuf::from(path)),
        Some(path) => Err(format!("诊断包不存在: {path}").into()),
        None => Err("用法: xtask logs analyze <bitcat-diagnostics-xxx.zip>".into()),
    }
}
