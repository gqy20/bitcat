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

    ai_pad_app_lib::run()
}
