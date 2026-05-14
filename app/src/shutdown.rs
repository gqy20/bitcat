//! 应用关闭协调模块。
//!
//! 本模块把托盘退出、控制台 Ctrl+C 和后台轮询线程收敛到同一个关闭标志。
//! 这样做可以避免不同入口各自调用退出逻辑，导致重复保存窗口位置或线程在退出时继续访问窗口。
//! 它与 Tauri `AppHandle` 交互触发事件循环退出，并由 gamepad / bubble / screenshot 等后台循环轮询状态。

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter};
use tracing::{debug, info, warn};

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// 返回当前进程是否已经收到关闭请求。
pub fn is_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

/// 请求应用退出；多次调用只有第一次会执行实际退出动作。
pub fn request_exit(app: &AppHandle, reason: &'static str) {
    if SHUTDOWN_REQUESTED.swap(true, Ordering::SeqCst) {
        debug!(reason, "shutdown already requested");
        return;
    }

    info!(reason, "shutdown requested");
    crate::snap::save_visible_pet_position(app);
    let _ = app.emit("app-shutdown", reason);
    app.exit(0);
}

/// 为 `make run` / 控制台调试场景安装 Ctrl+C 处理器。
pub fn install_ctrlc_handler(app: AppHandle) {
    let result = ctrlc::set_handler(move || {
        request_exit(&app, "ctrl-c");
    });

    match result {
        Ok(()) => info!("Ctrl+C shutdown handler installed"),
        Err(e) => warn!(error = %e, "failed to install Ctrl+C shutdown handler"),
    }
}
