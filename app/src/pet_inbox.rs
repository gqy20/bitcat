//! 宠物待查看窗口：承接宠物角标点击后的轻量 Inbox。
//!
//! 这个窗口只负责生命周期和定位，具体内容由前端通过现有 IPC 拉取。
//! 它与 pet、Agent Watch、截图观察模块协作，但不拥有这些业务状态。

use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

const WINDOW_LABEL: &str = "pet-inbox";
const WINDOW_W: f64 = 280.0;
const WINDOW_H: f64 = 260.0;
const EDGE_MARGIN: i32 = 8;

#[tauri::command]
pub async fn cmd_show_pet_inbox(app: AppHandle) -> Result<(), String> {
    let window = ensure_window(&app).map_err(|e| e.to_string())?;
    position_near_pet(&app, &window);
    window.show().map_err(|e| e.to_string())?;
    window.set_always_on_top(true).map_err(|e| e.to_string())?;
    let _ = window.eval("window.__petInboxRefresh && window.__petInboxRefresh();");
    Ok(())
}

#[tauri::command]
pub async fn cmd_hide_pet_inbox(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn ensure_window(app: &AppHandle) -> Result<WebviewWindow, tauri::Error> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        return Ok(window);
    }
    let window =
        WebviewWindowBuilder::new(app, WINDOW_LABEL, WebviewUrl::App("pet_inbox.html".into()))
            .title("8Bit Inbox")
            .inner_size(WINDOW_W, WINDOW_H)
            .decorations(false)
            .transparent(true)
            .background_color(tauri::webview::Color(0, 0, 0, 0))
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .build()?;
    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    Ok(window)
}

fn position_near_pet(app: &AppHandle, inbox: &WebviewWindow) {
    let Some(pet) = app
        .get_webview_window("pet")
        .filter(|w| w.is_visible().unwrap_or(false))
        .or_else(|| {
            app.get_webview_window("pet-mini")
                .filter(|w| w.is_visible().unwrap_or(false))
        })
    else {
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
    let scale = inbox.scale_factor().unwrap_or(1.0).max(0.5);
    let size = inbox.inner_size().unwrap_or(PhysicalSize::new(
        (WINDOW_W * scale).round() as u32,
        (WINDOW_H * scale).round() as u32,
    ));
    let width = size.width as i32;
    let height = size.height as i32;
    let min_x = mon_pos.x + EDGE_MARGIN;
    let max_x = mon_pos.x + mon_size.width as i32 - width - EDGE_MARGIN;
    let min_y = mon_pos.y + EDGE_MARGIN;
    let max_y = mon_pos.y + mon_size.height as i32 - height - EDGE_MARGIN;

    let right_x = pet_pos.x + 28;
    let left_x = pet_pos.x - width + 100;
    let x = if right_x <= max_x { right_x } else { left_x }.clamp(min_x, max_x.max(min_x));
    let y = (pet_pos.y + 24).clamp(min_y, max_y.max(min_y));
    let _ = inbox.set_position(PhysicalPosition::new(x, y));
}
