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
    let log_dir = ai_pad_core::logging::log_dir()
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

    install_panic_hook(log_dir);
    log_startup_diagnostics(debug);

    // ── 启动 Tauri ──
    ai_pad_app_lib::run();
}

fn log_startup_diagnostics(debug_console: bool) {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("<unavailable: {e}>"));
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("<unavailable: {e}>"));
    let log_dir = ai_pad_core::logging::log_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("<unavailable: {e}>"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let cleanup = ai_pad_core::logging::log_dir().and_then(|dir| {
        ai_pad_core::logging::cleanup_old_logs(&dir, std::time::Duration::from_secs(14 * 24 * 3600))
    });
    match cleanup {
        Ok(removed) if removed > 0 => tracing::info!(removed, "old log cleanup completed"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "old log cleanup failed"),
    }
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        profile,
        debug_console,
        exe = %exe,
        cwd = %cwd,
        log_dir = %log_dir,
        "ai-pad startup diagnostics"
    );
}

fn install_panic_hook(log_dir: std::path::PathBuf) {
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed");
        let location = info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        let backtrace = std::backtrace::Backtrace::force_capture();
        let message = format!(
            "\n===== panic =====\ntime={}\nthread={}\nlocation={}\npayload={}\nbacktrace={:?}\n",
            chrono::Local::now().to_rfc3339(),
            thread_name,
            location,
            payload,
            backtrace
        );
        let path = log_dir.join("panic.log");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, message.as_bytes()));
        tracing::error!(thread = thread_name, location = %location, payload = %payload, "panic captured");
    }));
}
