//! F2 功能使用审计：读取本地 `~/.bitcat/logs/` 埋点 JSONL，输出频次报告。
//!
//! 项目自带埋点、不上传（design-spec §8：本地 JSONL 是产品决策的事实来源）。
//! 本工具把"读日志出结论"固化成一条命令，避免每次手工分析：
//! `cargo run -p xtask -- audit-usage [--days N]`。
//! 缺失的文件标注"无数据"，本身即是一条审计结论（该功能未被使用）。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// 审计报告：按数据源分节的频次统计。
pub struct UsageAudit {
    pub days: u32,
    pub points: Option<Section>,
    pub tools: Option<Section>,
    pub tokens: TokenSection,
    pub reminders: Option<Section>,
}

/// 单个数据源的分节统计：键为事件/工具/生命周期名，值为次数与最近活跃日。
pub struct Section {
    pub total: usize,
    pub first_day: String,
    pub last_day: String,
    pub counts: Vec<(String, usize)>,
    pub top_extras: Vec<(String, usize)>,
}

pub struct TokenSection {
    pub records: Vec<TokenRecord>,
    pub first_day: String,
    pub last_day: String,
}

#[derive(Deserialize)]
struct PointsRecord {
    timestamp: String,
    #[serde(default)]
    event_kind: String,
    #[serde(default)]
    extra: Option<String>,
}

#[derive(Deserialize)]
struct ToolRecord {
    timestamp: String,
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    label: String,
}

#[derive(Deserialize, Clone)]
pub struct TokenRecord {
    pub timestamp: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

#[derive(Deserialize)]
struct ReminderRecord {
    timestamp: String,
    #[serde(default)]
    event: String,
    #[serde(default)]
    reminder_id: Option<String>,
}

pub fn run(days: u32) -> Result<()> {
    let audit = collect(days)?;
    print_report(&audit);
    Ok(())
}

fn log_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or("无法解析 home 目录")?;
    Ok(home.join(".bitcat").join("logs"))
}

fn collect(days: u32) -> Result<UsageAudit> {
    let dir = log_dir()?;
    let points = read_points(&dir, days);
    let tools = read_tools(&dir, days);
    let tokens = read_tokens(&dir, days);
    let reminders = read_reminders(&dir, days);
    Ok(UsageAudit {
        days,
        points,
        tools,
        tokens,
        reminders,
    })
}

/// 日期字符串比较是"YYYY-MM-DD"字典序 = 时间序；days 窗口用"年月日数值差"近似。
fn day_diff_later(timestamp: &str, base: &str, days: u32) -> bool {
    if days == 0 {
        return true;
    }
    let parse = |value: &str| -> Option<u32> {
        let parts: Vec<&str> = value.get(..10)?.split('-').collect();
        if parts.len() != 3 {
            return None;
        }
        let year: u32 = parts[0].parse().ok()?;
        let month: u32 = parts[1].parse().ok()?;
        let day: u32 = parts[2].parse().ok()?;
        Some(year * 10000 + month * 100 + day)
    };
    match (parse(timestamp), parse(base)) {
        (Some(a), Some(b)) => a + days > b,
        _ => true,
    }
}

fn day_of(timestamp: &str) -> &str {
    timestamp.get(..10).unwrap_or("?")
}

fn read_points(dir: &Path, days: u32) -> Option<Section> {
    let records: Vec<PointsRecord> = read_jsonl(&dir.join("points_events.jsonl"))?;
    let last_day = records
        .last()
        .map(|record| record.timestamp.clone())
        .unwrap_or_default();
    let filtered: Vec<&PointsRecord> = records
        .iter()
        .filter(|record| day_diff_later(&record.timestamp, &last_day, days))
        .collect();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut extras: BTreeMap<String, usize> = BTreeMap::new();
    for record in &filtered {
        *counts.entry(record.event_kind.clone()).or_default() += 1;
        if let Some(extra) = record.extra.as_deref().filter(|value| !value.is_empty()) {
            *extras.entry(extra.to_string()).or_default() += 1;
        }
    }
    Some(Section {
        total: filtered.len(),
        first_day: filtered
            .first()
            .map(|record| day_of(&record.timestamp).to_string())
            .unwrap_or_default(),
        last_day: day_of(&last_day).to_string(),
        counts: sorted_counts(counts),
        top_extras: sorted_counts(extras),
    })
}

fn read_tools(dir: &Path, days: u32) -> Option<Section> {
    let records: Vec<ToolRecord> = read_jsonl(&dir.join("tool_events.jsonl"))?;
    let last_day = records
        .last()
        .map(|record| record.timestamp.clone())
        .unwrap_or_default();
    let filtered: Vec<&ToolRecord> = records
        .iter()
        .filter(|record| day_diff_later(&record.timestamp, &last_day, days))
        .collect();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut labels: BTreeMap<String, usize> = BTreeMap::new();
    for record in &filtered {
        *counts.entry(record.tool_name.clone()).or_default() += 1;
        if !record.label.is_empty() {
            *labels.entry(record.label.clone()).or_default() += 1;
        }
    }
    Some(Section {
        total: filtered.len(),
        first_day: filtered
            .first()
            .map(|record| day_of(&record.timestamp).to_string())
            .unwrap_or_default(),
        last_day: day_of(&last_day).to_string(),
        counts: sorted_counts(counts),
        top_extras: sorted_counts(labels),
    })
}

fn read_tokens(dir: &Path, days: u32) -> TokenSection {
    let records: Vec<TokenRecord> = read_jsonl(&dir.join("token_usage.jsonl")).unwrap_or_default();
    let last_day = records
        .last()
        .map(|record| record.timestamp.clone())
        .unwrap_or_default();
    let filtered: Vec<TokenRecord> = records
        .into_iter()
        .filter(|record| day_diff_later(&record.timestamp, &last_day, days))
        .collect();
    TokenSection {
        first_day: filtered
            .first()
            .map(|record| day_of(&record.timestamp).to_string())
            .unwrap_or_default(),
        last_day: day_of(&last_day).to_string(),
        records: filtered,
    }
}

fn read_reminders(dir: &Path, days: u32) -> Option<Section> {
    let records: Vec<ReminderRecord> = read_jsonl(&dir.join("reminder_events.jsonl"))?;
    let last_day = records
        .last()
        .map(|record| record.timestamp.clone())
        .unwrap_or_default();
    let filtered: Vec<&ReminderRecord> = records
        .iter()
        .filter(|record| day_diff_later(&record.timestamp, &last_day, days))
        .collect();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    for record in &filtered {
        *counts.entry(record.event.clone()).or_default() += 1;
        if let Some(id) = record.reminder_id.as_deref() {
            *ids.entry(id.to_string()).or_default() += 1;
        }
    }
    Some(Section {
        total: filtered.len(),
        first_day: filtered
            .first()
            .map(|record| day_of(&record.timestamp).to_string())
            .unwrap_or_default(),
        last_day: day_of(&last_day).to_string(),
        counts: sorted_counts(counts),
        top_extras: sorted_counts(ids),
    })
}

fn read_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<Vec<T>> {
    let raw = fs::read_to_string(path).ok()?;
    let records = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    Some(records)
}

fn sorted_counts(map: BTreeMap<String, usize>) -> Vec<(String, usize)> {
    let mut entries: Vec<(String, usize)> = map.into_iter().collect();
    entries.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    entries
}

fn print_report(audit: &UsageAudit) {
    let window = if audit.days == 0 {
        "全部".to_string()
    } else {
        format!("最近 {} 天", audit.days)
    };
    println!("BitCat 功能使用审计（{window}）");
    println!("数据源：~/.bitcat/logs/（本地事实，不上传）");
    println!("{}", "=".repeat(64));

    println!("\n[宠物互动 / 玩法] points_events.jsonl");
    match &audit.points {
        Some(section) if section.total > 0 => {
            println!(
                "  窗口：{} → {}，共 {} 条",
                section.first_day, section.last_day, section.total
            );
            for (name, count) in &section.counts {
                println!("  {count:5}  {name}");
            }
            if !section.top_extras.is_empty() {
                println!("  —— 明细 top5:");
                for (name, count) in section.top_extras.iter().take(5) {
                    println!("  {count:5}  {name}");
                }
            }
        }
        Some(_) => println!("  （窗口内 0 条记录）"),
        None => println!("  （无数据文件：宠物互动/游戏/舞蹈从未发生）"),
    }

    println!("\n[AI 工具调用] tool_events.jsonl");
    match &audit.tools {
        Some(section) if section.total > 0 => {
            println!(
                "  窗口：{} → {}，共 {} 次调用",
                section.first_day, section.last_day, section.total
            );
            for (name, count) in &section.counts {
                println!("  {count:5}  {name}");
            }
        }
        Some(_) => println!("  （窗口内 0 条记录）"),
        None => println!("  （无数据文件：AI 对话工具从未被调用）"),
    }

    println!("\n[API 用量] token_usage.jsonl");
    if audit.tokens.records.is_empty() {
        println!("  （无记录）");
    } else {
        let total_in: u64 = audit.tokens.records.iter().map(|r| r.input_tokens).sum();
        let total_out: u64 = audit.tokens.records.iter().map(|r| r.output_tokens).sum();
        println!(
            "  窗口：{} → {}，共 {} 次调用",
            audit.tokens.first_day,
            audit.tokens.last_day,
            audit.tokens.records.len()
        );
        println!("  tokens: in={total_in} out={total_out}");
        let mut by_category: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
        let mut by_model: BTreeMap<&str, usize> = BTreeMap::new();
        for record in &audit.tokens.records {
            let entry = by_category.entry(record.category.as_str()).or_default();
            entry.0 += 1;
            entry.1 += record.input_tokens + record.output_tokens;
            *by_model.entry(record.model.as_str()).or_default() += 1;
        }
        println!("  —— 按链路:");
        let mut categories: Vec<_> = by_category.into_iter().collect();
        categories.sort_by_key(|(_, (calls, _))| std::cmp::Reverse(*calls));
        for (category, (calls, tokens)) in categories {
            println!("  {calls:5} 次 / {tokens:8} tok  {category}");
        }
        let mut models: Vec<_> = by_model.into_iter().collect();
        models.sort_by_key(|(_, calls)| std::cmp::Reverse(*calls));
        println!("  —— 按模型:");
        for (model, calls) in models.into_iter().take(5) {
            println!("  {calls:5} 次  {model}");
        }
    }

    println!("\n[提醒生命周期] reminder_events.jsonl");
    match &audit.reminders {
        Some(section) if section.total > 0 => {
            println!(
                "  窗口：{} → {}，共 {} 条",
                section.first_day, section.last_day, section.total
            );
            for (name, count) in &section.counts {
                println!("  {count:5}  {name}");
            }
        }
        Some(_) => println!("  （窗口内 0 条记录）"),
        None => println!("  （无数据文件：提醒功能从未使用）"),
    }

    println!("\n{}", "=".repeat(64));
    println!("提示：频次只是输入之一；砍/藏/留决策见 docs/roadmap.md 的 F2 节。");
}
