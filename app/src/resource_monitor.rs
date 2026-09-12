//! 常驻资源自监控（L3）。
//!
//! 每 60 秒把进程内存/CPU、系统内存和 WebView 窗口数写入
//! `~/.bitcat/logs/resource_usage.jsonl`，为"优化前后对比"和常驻健康度
//! 提供自动数据积累——吃狗粮期间无需人工观测，事后 `xtask` 或脚本直接
//! 出资源曲线。进程内存/CPU 采集仅 Windows 实现（与设置页诊断共用）；
//! 其他平台记录窗口数与时间戳，保持文件结构一致。

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tracing::{debug, info};

const SAMPLE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Serialize)]
struct ResourceSample {
    timestamp: String,
    process_memory_mb: f64,
    process_cpu_percent: f64,
    system_memory_used_mb: f64,
    system_memory_total_mb: f64,
    system_memory_percent: f64,
    webview_windows: usize,
}

/// 启动资源采样线程；失败采样只记日志，不影响后续周期。
pub fn spawn_resource_monitor(app: AppHandle) {
    std::thread::spawn(move || {
        while !crate::shutdown::is_requested() {
            std::thread::sleep(SAMPLE_INTERVAL);
            if crate::shutdown::is_requested() {
                break;
            }
            if let Err(e) = sample_once(&app) {
                debug!(error = %e, "resource sample skipped");
            }
        }
        info!("resource monitor stopped");
    });
}

fn sample_once(app: &AppHandle) -> Result<(), String> {
    let (memory_mb, cpu_percent, sys_used, sys_total, sys_percent) = collect_platform_stats()?;
    let sample = ResourceSample {
        timestamp: chrono::Local::now().to_rfc3339(),
        process_memory_mb: memory_mb,
        process_cpu_percent: cpu_percent,
        system_memory_used_mb: sys_used,
        system_memory_total_mb: sys_total,
        system_memory_percent: sys_percent,
        webview_windows: app.webview_windows().len(),
    };
    bitcat_core::logging::append_jsonl("resource_usage.jsonl", &sample).map(|_| ())
}

#[cfg(windows)]
fn collect_platform_stats() -> Result<(f64, f64, f64, f64, f64), String> {
    use crate::settings::{current_process_memory_mb, process_cpu_percent, system_memory_stats};
    let (sys_used, sys_total, sys_percent) = system_memory_stats()?;
    Ok((
        current_process_memory_mb()?,
        process_cpu_percent()?,
        sys_used,
        sys_total,
        sys_percent,
    ))
}

#[cfg(not(windows))]
fn collect_platform_stats() -> Result<(f64, f64, f64, f64, f64), String> {
    // 非 Windows 无进程级采集实现（与设置页诊断一致）；记录占位 0，
    // 文件结构保持一致便于统一解析。
    Ok((0.0, 0.0, 0.0, 0.0, 0.0))
}
