fn main() {
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

    // 初始化 tracing 日志系统
    // RUST_LOG=info 默认 info 级别以上
    // RUST_LOG=ai_pad=debug,ai_pad_core=trace 调试时用
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ai_pad_app=info,ai_pad_core=debug".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    ai_pad_app_lib::run();
}
