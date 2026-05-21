//! Agent Watch 浮窗管理。
//!
//! 本模块创建一个透明置顶窗口，用来在宠物旁边展示 Claude Code 任务栈。
//! 它只负责窗口生命周期、跟随宠物定位和快照推送，不参与 hook 解析或提醒策略。
//! 会话状态由 `agent_monitor` 管理，前端通过 `agent-watch-update` 实时渲染。
use crate::agent_monitor::{AgentSessionsSnapshot, DEFAULT_AGENT_MONITOR_PORT};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tracing::warn;

const WINDOW_LABEL: &str = "agent-watch";
const WINDOW_W: f64 = 254.0;
const WINDOW_H: f64 = 300.0;
const FOLDED_WINDOW_H: f64 = 60.0;
const EDGE_MARGIN: i32 = 12;
static USER_PLACED: AtomicBool = AtomicBool::new(false);

/// 启动 Agent Watch 浮窗跟随线程。
pub fn spawn_agent_watch_follower(app: AppHandle) {
    std::thread::spawn(move || {
        let mut prev_pet_pos: Option<(i32, i32)> = None;
        while !crate::shutdown::is_requested() {
            let Some(watch) = app.get_webview_window(WINDOW_LABEL) else {
                std::thread::sleep(std::time::Duration::from_millis(250));
                continue;
            };
            if !watch.is_visible().unwrap_or(false) {
                std::thread::sleep(std::time::Duration::from_millis(250));
                continue;
            }
            let Some(pet) = app.get_webview_window("pet") else {
                std::thread::sleep(std::time::Duration::from_millis(250));
                continue;
            };
            if let Ok(pos) = pet.outer_position() {
                let key = (pos.x, pos.y);
                if Some(key) != prev_pet_pos && !USER_PLACED.load(Ordering::Relaxed) {
                    prev_pet_pos = Some(key);
                    position_near_pet(&app, &watch);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
    });
}

/// 确保浮窗存在，并把最新快照推给前端。
pub fn show_snapshot(app: &AppHandle, snapshot: &AgentSessionsSnapshot) {
    let Ok(window) = ensure_agent_watch_window(app) else {
        return;
    };
    if !USER_PLACED.load(Ordering::Relaxed) {
        position_near_pet(app, &window);
    }
    let has_sessions = !snapshot.sessions.is_empty();
    if has_sessions {
        let _ = window.show();
        let _ = window.set_always_on_top(true);
    } else {
        let _ = window.hide();
    }
    let _ = app.emit_to(WINDOW_LABEL, "agent-watch-update", snapshot);
    let _ = window.eval("window.__agentWatchRefresh && window.__agentWatchRefresh();");
}

fn ensure_agent_watch_window(app: &AppHandle) -> Result<WebviewWindow, tauri::Error> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        return Ok(window);
    }
    let window = WebviewWindowBuilder::new(
        app,
        WINDOW_LABEL,
        WebviewUrl::App("agent_watch.html".into()),
    )
    .title("8Bit Agent Watch")
    .inner_size(WINDOW_W, WINDOW_H)
    .min_inner_size(WINDOW_W, FOLDED_WINDOW_H)
    .max_inner_size(WINDOW_W, WINDOW_H)
    .decorations(false)
    .transparent(true)
    .background_color(tauri::webview::Color(0, 0, 0, 0))
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()?;
    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    Ok(window)
}

fn position_near_pet(app: &AppHandle, watch: &WebviewWindow) {
    let Some(pet) = app.get_webview_window("pet") else {
        return;
    };
    let Ok(pet_pos) = pet.outer_position() else {
        return;
    };
    let Some(monitor) = pet.current_monitor().ok().flatten() else {
        return;
    };
    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let scale = watch.scale_factor().unwrap_or(1.0).max(0.5);
    let watch_size = watch.inner_size().unwrap_or(PhysicalSize::new(
        (WINDOW_W * scale).round() as u32,
        (WINDOW_H * scale).round() as u32,
    ));
    let watch_w = watch_size.width as i32;
    let watch_h = watch_size.height as i32;

    let min_x = mon_pos.x + EDGE_MARGIN;
    let max_x = mon_pos.x + mon_size.width as i32 - watch_w - EDGE_MARGIN;
    let min_y = mon_pos.y + EDGE_MARGIN;
    let max_y = mon_pos.y + mon_size.height as i32 - watch_h - EDGE_MARGIN;

    let right_x = mon_pos.x + mon_size.width as i32 - watch_w - EDGE_MARGIN;
    let left_of_pet_x = pet_pos.x - watch_w - EDGE_MARGIN;
    let bubble_visible = app
        .get_webview_window("bubble")
        .is_some_and(|bubble| bubble.is_visible().unwrap_or(false));
    let x = if bubble_visible {
        right_x
    } else if left_of_pet_x >= min_x {
        left_of_pet_x
    } else {
        right_x.min(max_x)
    }
    .clamp(min_x, max_x.max(min_x));

    let preferred_y = mon_pos.y + EDGE_MARGIN;
    let pet_y = pet_pos.y - watch_h - EDGE_MARGIN;
    let y = if pet_y >= min_y && pet_pos.y < min_y + watch_h + EDGE_MARGIN {
        pet_y
    } else {
        preferred_y
    }
    .clamp(min_y, max_y.max(min_y));

    let _ = watch.set_position(PhysicalPosition::new(x, y));
}

fn set_size_preserving_anchor(
    window: &WebviewWindow,
    new_size: PhysicalSize<u32>,
) -> Result<(), tauri::Error> {
    let old_pos = window.outer_position().ok();
    let old_size = window.inner_size().ok();
    let monitor = window.current_monitor().ok().flatten();

    window.set_size(new_size)?;

    let (Some(old_pos), Some(old_size), Some(monitor)) = (old_pos, old_size, monitor) else {
        return Ok(());
    };

    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let left_gap = old_pos.x - mon_pos.x;
    let right_gap = mon_pos.x + mon_size.width as i32 - (old_pos.x + old_size.width as i32);
    let top_gap = old_pos.y - mon_pos.y;
    let bottom_gap = mon_pos.y + mon_size.height as i32 - (old_pos.y + old_size.height as i32);

    let min_x = mon_pos.x + EDGE_MARGIN;
    let max_x = mon_pos.x + mon_size.width as i32 - new_size.width as i32 - EDGE_MARGIN;
    let min_y = mon_pos.y + EDGE_MARGIN;
    let max_y = mon_pos.y + mon_size.height as i32 - new_size.height as i32 - EDGE_MARGIN;

    let x = if right_gap < left_gap {
        old_pos.x + old_size.width as i32 - new_size.width as i32
    } else {
        old_pos.x
    }
    .clamp(min_x, max_x.max(min_x));
    let y = if bottom_gap < top_gap {
        old_pos.y + old_size.height as i32 - new_size.height as i32
    } else {
        old_pos.y
    }
    .clamp(min_y, max_y.max(min_y));

    window.set_position(PhysicalPosition::new(x, y))
}

/// 前端请求隐藏浮窗。
#[tauri::command]
pub async fn cmd_agent_watch_hide(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 前端折叠/展开时调整窗口本体尺寸，避免透明区域遮挡其他浮窗。
#[tauri::command]
pub async fn cmd_agent_watch_set_folded(app: AppHandle, folded: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let scale = window.scale_factor().unwrap_or(1.0).max(0.5);
        let h = if folded { FOLDED_WINDOW_H } else { WINDOW_H };
        set_size_preserving_anchor(
            &window,
            PhysicalSize::new(
                (WINDOW_W * scale).round() as u32,
                (h * scale).round() as u32,
            ),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 记录用户已经手动摆放 Agent Watch，避免后续宠物跟随逻辑把它抢回去。
#[tauri::command]
pub async fn cmd_agent_watch_mark_user_placed() -> Result<(), String> {
    USER_PLACED.store(true, Ordering::Relaxed);
    Ok(())
}

/// 前端请求刷新浮窗快照。
#[tauri::command]
pub async fn cmd_agent_watch_refresh(app: AppHandle) -> Result<(), String> {
    let monitor: tauri::State<crate::agent_monitor::SharedAgentMonitor> = app.state();
    let snapshot = monitor.snapshot(crate::agent_monitor::now_ms())?;
    show_snapshot(&app, &snapshot);
    Ok(())
}

/// 返回浮窗监听端口，方便前端调试显示。
#[tauri::command]
pub async fn cmd_agent_watch_port() -> Result<u16, String> {
    Ok(DEFAULT_AGENT_MONITOR_PORT)
}

/// Frontend diagnostic bridge for the Agent Watch window.
#[tauri::command]
pub async fn cmd_agent_watch_log(msg: String) -> Result<(), String> {
    if !ai_pad_core::logging::frontend_log_allowed(
        "agent-watch",
        std::time::Duration::from_millis(120),
    ) {
        return Ok(());
    }
    let preview = ai_pad_core::logging::log_preview(&msg, 120);
    warn!(
        msg_chars = msg.chars().count(),
        msg_preview = %preview,
        "agent-watch frontend log"
    );
    Ok(())
}
