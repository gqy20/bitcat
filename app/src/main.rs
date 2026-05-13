//! 应用程序入口点。
//!
//! 负责三件事：解析 `--debug` 标志决定是否分配控制台窗口、
//! 初始化 tracing 日志（stderr 带颜色 + 文件按日滚动）、
//! 调用 `ai_pad_app_lib::run()` 启动 Tauri 事件循环。
//!
//! ## unsafe 安全不变性
//!
//! `AllocConsole` / `FreeConsole` 仅在 Windows 平台条件编译下调用，
//! 属于 Win32 线程安全 API，不需要额外同步保护。

/// 二进制入口。按顺序完成：控制台分配/释放 → 日志初始化 → Tauri 启动。
fn main() {
    use tracing_subscriber::fmt;
    use tracing_subscriber::prelude::*;

    // ── 控制台分配 ──
    // --debug 时调用 AllocConsole 显示控制台窗口；
    // 非 debug release 模式下调用 FreeConsole 脱离父控制台。
    let debug = std::env::args().any(|a| a == "--debug");
    if debug {
        // 安全：AllocConsole 是线程安全的 Win32 API，仅影响调用线程。
        #[cfg(target_os = "windows")]
        unsafe {
            windows_sys::Win32::System::Console::AllocConsole();
        }
    } else {
        // 安全：FreeConsole 同上，仅在 release 构建时执行。
        #[cfg(all(target_os = "windows", not(debug_assertions)))]
        unsafe {
            windows_sys::Win32::System::Console::FreeConsole();
        }
    }

    // ── 日志双写初始化 ──
    // 文件层：~/.ai-pad/logs/app.log.YYYY-MM-DD，按日期滚动。
    // stderr 层：带颜色输出，方便终端实时查看。
    // 两层共享同一个 EnvFilter，默认级别 ai_pad_app=info, ai_pad_core=debug。
    let log_dir = std::env::var("USERPROFILE")
        .map(|p| std::path::PathBuf::from(p).join(".ai-pad").join("logs"))
        .unwrap_or_else(|_| std::path::PathBuf::from(".ai-pad-logs"));
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "app.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "ai_pad_app=info,ai_pad_core=debug".into());

    // 单次 .init()：stderr(带颜色) + 文件(无颜色)，统一本地时间。
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(true)
                .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
                .with_filter(filter.clone()),
        )
        .with(
            fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(true)
                .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
                .with_filter(filter),
        )
        .init();

    // ── 启动 Tauri ──
    ai_pad_app_lib::run();
}
