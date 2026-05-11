fn main() {
    use tracing_subscriber::{prelude::*, Layer};

    // --debug 参数时分配控制台窗口用于查看日志
    let debug = std::env::args().any(|a| a == "--debug");
    if debug {
        #[cfg(target_os = "windows")]
        unsafe {
            windows_sys::Win32::System::Console::AllocConsole();
        }
    } else {
        // release 模式下隐藏控制台
        #[cfg(all(target_os = "windows", not(debug_assertions)))]
        unsafe {
            windows_sys::Win32::System::Console::FreeConsole();
        }
    }

    // 日志文件：~/.ai-pad/logs/，按日期滚动（每天一个文件）
    let log_dir = std::env::var("USERPROFILE")
        .map(|p| std::path::PathBuf::from(p).join(".ai-pad").join("logs"))
        .unwrap_or_else(|_| std::path::PathBuf::from(".ai-pad-logs"));
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "app.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "ai_pad_app=info,ai_pad_core=debug".into());

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer()
            .with_filter(filter.clone())
            .with_writer(std::io::stderr))
        .with(tracing_subscriber::fmt::layer()
            .with_filter(filter)
            .with_writer(non_blocking)
            .with_ansi(false)
            .with_target(true))
        .init();

    ai_pad_app_lib::run();
}
