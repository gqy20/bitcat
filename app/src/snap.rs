use crate::commands::{self, SharedWindowState};
use ai_pad_core::app_settings::{AppSettings, WindowPosition};
use tauri::{LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};
use tracing::{info, warn};

use std::sync::atomic::Ordering;

pub const SNAP_W: f64 = 24.0;
pub const SNAP_H: i32 = 67;
const SNAP_BOTTOM_GAP_LP: f64 = 14.0;

/// 切换 pet 窗口显示模式：正常(128x128) / 折叠(48x48)。
#[tauri::command]
pub async fn cmd_recreate_pet_window(
    app: tauri::AppHandle,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
) -> Result<(), String> {
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    let on_top = ws.always_on_top.load(Ordering::SeqCst);
    let collapsed = width == 48 && height == 48;

    let _ = crate::bubble::hide_bubble_window(&app);

    let target_label = if collapsed { "pet-mini" } else { "pet" };
    let hide_label = if collapsed { "pet" } else { "pet-mini" };

    if let Some(other) = app.get_webview_window(hide_label) {
        let _ = other.hide();
    }

    let win = match app.get_webview_window(target_label) {
        Some(w) => w,
        None => return Err(format!("预创建窗口 '{}' 不存在", target_label)),
    };

    win.set_position(PhysicalPosition::new(x, y))
        .map_err(|e| format!("set {target_label} position failed: {e}"))?;
    if let Err(e) = win.set_always_on_top(on_top) {
        warn!(error = %e, target = target_label, "设置窗口置顶状态失败");
    }
    win.show().map_err(|e| e.to_string())?;
    let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));

    remember_pet_position(&ws, x, y);

    info!(
        width = width,
        height = height,
        x = x,
        y = y,
        collapsed = collapsed,
        target = target_label,
        "窗口切换"
    );
    Ok(())
}

#[derive(serde::Serialize)]
pub struct SnapResult {
    edge: String,
    x: i32,
    y: i32,
}

fn persist_pet_position(x: i32, y: i32) -> Result<(), String> {
    let mut settings = AppSettings::load();
    settings.appearance.pet_position = Some(WindowPosition { x, y });
    settings.save()
}

fn remember_pet_position(ws: &SharedWindowState, x: i32, y: i32) {
    if let Ok(mut pos) = ws.last_position.lock() {
        *pos = Some((x, y));
    }
    if let Err(e) = persist_pet_position(x, y) {
        warn!(error = %e, x, y, "保存桌宠位置失败");
    }
}

fn clamp_position_to_work_area(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    work: windows_sys::Win32::Foundation::RECT,
) -> (i32, i32) {
    let max_x = work.right - w;
    let max_y = work.bottom - h;
    (
        x.clamp(work.left, max_x.max(work.left)),
        y.clamp(work.top, max_y.max(work.top)),
    )
}

#[tauri::command]
pub async fn cmd_save_pet_position(app: tauri::AppHandle, x: i32, y: i32) -> Result<(), String> {
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    remember_pet_position(&ws, x, y);
    Ok(())
}

/// 贴边吸附：计算宠物窗口的吸附目标位置
#[tauri::command]
pub async fn cmd_snap_pet(app: tauri::AppHandle, x: i32, y: i32) -> Result<SnapResult, String> {
    let win = app
        .get_webview_window("pet")
        .filter(|w| w.is_visible().unwrap_or(false))
        .or_else(|| app.get_webview_window("pet-mini"))
        .ok_or("no visible pet window")?;

    let pet_size = win.outer_size().map_err(|e| e.to_string())?;
    let pw = pet_size.width as i32;
    let ph = pet_size.height as i32;
    let scale = win.scale_factor().unwrap_or(1.0);
    let snap_h_px = (SNAP_H as f64 * scale) as i32;
    let snap_w_px = (SNAP_W * scale) as i32;
    let bottom_gap_px = (SNAP_BOTTOM_GAP_LP * scale).round() as i32;
    let horizontal_snap_w_px = snap_h_px;
    let horizontal_snap_h_px = snap_w_px;

    let work = get_work_area_for_window(&win);
    info!(
        snap_cmd = true,
        input_x = x,
        input_y = y,
        work_left = work.left,
        work_right = work.right,
        work_top = work.top,
        work_bottom = work.bottom,
        work_w = work.right - work.left,
        work_h = work.bottom - work.top,
        pet_w = pw,
        pet_h = ph,
        "cmd_snap_pet: 工作区信息"
    );

    let snap_threshold = (80.0 * scale) as i32;
    let left_dist = (x - work.left).max(0);
    let right_dist = (work.right - pw - x).max(0);
    let top_dist = (y - work.top).max(0);
    let bottom_dist = (work.bottom - ph - y).max(0);
    let candidates = [
        ("left", left_dist),
        ("right", right_dist),
        ("top", top_dist),
        ("bottom", bottom_dist),
    ];

    let (edge, dist) = candidates
        .iter()
        .min_by_key(|(_, dist)| *dist)
        .copied()
        .unwrap_or(("none", snap_threshold + 1));

    let side_snap_y =
        (work.bottom - snap_h_px - bottom_gap_px).clamp(work.top, work.bottom - snap_h_px);
    let snap_result = match edge {
        "left" if dist <= snap_threshold => ("left", work.left, side_snap_y),
        "right" if dist <= snap_threshold => ("right", work.right - snap_w_px, side_snap_y),
        "top" if dist <= snap_threshold => (
            "top",
            x.clamp(work.left, work.right - horizontal_snap_w_px),
            work.top,
        ),
        "bottom" if dist <= snap_threshold => (
            "bottom",
            x.clamp(work.left, work.right - horizontal_snap_w_px),
            work.bottom - horizontal_snap_h_px,
        ),
        _ => {
            return Ok(SnapResult {
                edge: "none".to_string(),
                x,
                y,
            });
        }
    };

    let (edge, target_x, target_y) = snap_result;

    info!(
        snap_cmd = true,
        input_x = x,
        input_y = y,
        left_dist = left_dist,
        right_dist = right_dist,
        top_dist = top_dist,
        bottom_dist = bottom_dist,
        edge = %edge,
        target_x = target_x,
        target_y = target_y,
        "cmd_snap_pet: 吸附结果"
    );

    Ok(SnapResult {
        edge: edge.to_string(),
        x: target_x,
        y: target_y,
    })
}

/// 拖拽过程中查询磁性预告
#[tauri::command]
pub async fn cmd_get_snap_preview(
    app: tauri::AppHandle,
    x: i32,
    y: i32,
) -> Result<commands::SnapPreview, String> {
    let win = app
        .get_webview_window("pet")
        .filter(|w| w.is_visible().unwrap_or(false))
        .or_else(|| app.get_webview_window("pet-mini"))
        .ok_or("no visible pet window")?;

    let pet_size = win.outer_size().map_err(|e| e.to_string())?;
    let pw = pet_size.width as i32;
    let ph = pet_size.height as i32;
    let scale = win.scale_factor().unwrap_or(1.0);
    let snap_h_px = (SNAP_H as f64 * scale) as i32;
    let snap_w_px = (SNAP_W * scale) as i32;
    let threshold = (80.0 * scale) as i32;

    let work = get_work_area_for_window(&win);

    Ok(commands::calc_snap_preview(
        x,
        y,
        work.left,
        work.top,
        work.right,
        work.bottom,
        pw,
        ph,
        snap_w_px,
        snap_h_px,
        threshold,
    ))
}

/// 贴边吸附：将宠物窗口转换为吸附态
#[tauri::command]
pub async fn cmd_snap_transform(
    app: tauri::AppHandle,
    edge: String,
    x: i32,
    y: i32,
) -> Result<(), String> {
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    let on_top = ws.always_on_top.load(Ordering::SeqCst);
    let (snap_width, snap_height) = match edge.as_str() {
        "left" | "right" => (SNAP_W, SNAP_H as f64),
        "top" | "bottom" => (SNAP_H as f64, SNAP_W),
        _ => return Err(format!("invalid snap edge: {edge}")),
    };

    *ws.is_snapped.lock().map_err(|e| e.to_string())? = true;
    *ws.snap_edge.lock().map_err(|e| e.to_string())? = Some(edge.clone());

    let _ = crate::bubble::hide_bubble_window(&app);

    for label in ["pet", "pet-mini"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.eval("if(typeof __fadeOut==='function')__fadeOut();");
        }
    }
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    for label in ["pet", "pet-mini"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.hide();
            let _ = w.eval("if(typeof __fadeReset==='function')__fadeReset();");
        }
    }

    let snap_win = app
        .get_webview_window("pet-snap")
        .ok_or("pet-snap window not found")?;

    snap_win
        .set_size(LogicalSize::new(snap_width, snap_height))
        .map_err(|e| format!("set snap size failed: {e}"))?;
    snap_win
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| format!("set snap position failed: {e}"))?;
    if let Err(e) = snap_win.set_always_on_top(on_top) {
        warn!(error = %e, "设置吸附窗口置顶状态失败");
    }
    snap_win.show().map_err(|e| e.to_string())?;

    let edge_for_eval = edge.clone();
    if snap_win
        .eval(format!(
            "if(typeof __setSnapMetrics==='function'){{__setSnapMetrics({},{})}};\
             if(typeof __setSnapEdge==='function'){{__setSnapEdge('{edge_for_eval}');'ok'}}else{{'no-fn'}}",
            SNAP_W, SNAP_H
        ))
        .map_err(|e| {
            warn!(error = %e, edge = %edge, "注入吸附方向失败");
            e
        })
        .is_ok()
    {
        info!(edge = %edge, "[cmd_snap_snap] ✓ eval setSnapEdge 成功（兜底通知）");
    }

    if let Err(e) = snap_win.eval("if(typeof __fadeIn==='function')__fadeIn();") {
        warn!(error = %e, "吸附窗口淡入失败");
    }

    info!(snap_transform = true, edge = %edge, x = x, y = y, "吸附态切换成功");
    Ok(())
}

/// 取消吸附：恢复宠物窗口原大小
#[tauri::command]
pub async fn cmd_unsnap_transform(app: tauri::AppHandle) -> Result<(), String> {
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    let on_top = ws.always_on_top.load(Ordering::SeqCst);

    let _ = crate::bubble::hide_bubble_window(&app);

    let snap_win_opt = app.get_webview_window("pet-snap");
    if let Some(w) = &snap_win_opt {
        let _ = w.eval("if(typeof __fadeOut==='function')__fadeOut();");
    }
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let snap_pos = snap_win_opt.as_ref().and_then(|w| w.outer_position().ok());

    if let Some(w) = &snap_win_opt {
        let _ = w.hide();
        let _ = w.eval("if(typeof __fadeReset==='function')__fadeReset();");
    }

    let collapsed = ws.collapsed.load(Ordering::SeqCst);
    let target = if collapsed { "pet-mini" } else { "pet" };

    let win = app
        .get_webview_window(target)
        .ok_or(format!("window '{}' not found", target))?;

    if let Some(pos) = snap_pos {
        let edge = ws.snap_edge.lock().ok().and_then(|e| e.clone());
        let (x, y) = match edge.as_deref() {
            Some("left") => (pos.x + 80, pos.y),
            Some("right") => (pos.x - 80, pos.y),
            Some("top") => (pos.x, pos.y + 80),
            Some("bottom") => (pos.x, pos.y - 80),
            _ => (pos.x, pos.y),
        };
        win.set_position(PhysicalPosition::new(x, y))
            .map_err(|e| format!("set {target} position failed: {e}"))?;
        remember_pet_position(&ws, x, y);
    }

    if let Err(e) = win.set_always_on_top(on_top) {
        warn!(error = %e, target = target, "设置窗口置顶状态失败");
    }
    win.show().map_err(|e| e.to_string())?;
    if let Err(e) = win.eval("if(typeof __fadeIn==='function')__fadeIn();") {
        warn!(error = %e, target = target, "窗口淡入失败");
    }

    *ws.is_snapped.lock().map_err(|e| e.to_string())? = false;
    *ws.snap_edge.lock().map_err(|e| e.to_string())? = None;

    info!(unsnap_transform = true, "取消吸附成功");
    Ok(())
}

/// 获取窗口所在显示器的工作区（Win32）
#[cfg(target_os = "windows")]
pub fn get_work_area_for_window(
    win: &tauri::WebviewWindow,
) -> windows_sys::Win32::Foundation::RECT {
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };

    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..unsafe { std::mem::zeroed() }
    };

    if let Ok(hwnd) = win.hwnd() {
        let raw_hwnd = hwnd.0 as windows_sys::Win32::Foundation::HWND;
        let hmon_val: isize;
        let ok;
        unsafe {
            hmon_val = MonitorFromWindow(raw_hwnd, MONITOR_DEFAULTTONEAREST) as isize;
            ok = GetMonitorInfoW(hmon_val as _, &mut mi) != 0;
        }
        if !ok {
            warn!(
                hmon = hmon_val,
                "get_work_area_for_window: Win32 GetMonitorInfoW 失败，使用 Tauri monitor fallback"
            );
            return fallback_work_area_for_window(win);
        }
        if mi.rcWork.right <= mi.rcWork.left || mi.rcWork.bottom <= mi.rcWork.top {
            warn!("get_work_area_for_window: Win32 工作区为空，使用 Tauri monitor fallback");
            return fallback_work_area_for_window(win);
        }
        info!(
            work_area = true,
            hmon = hmon_val,
            rcWork_left = mi.rcWork.left,
            rcWork_top = mi.rcWork.top,
            rcWork_right = mi.rcWork.right,
            rcWork_bottom = mi.rcWork.bottom,
            rcWork_w = mi.rcWork.right - mi.rcWork.left,
            rcWork_h = mi.rcWork.bottom - mi.rcWork.top,
            "get_work_area_for_window: Win32 GetMonitorInfoW 结果"
        );
    } else {
        warn!("get_work_area_for_window: 无法获取 HWND");
    }

    if mi.rcWork.right <= mi.rcWork.left || mi.rcWork.bottom <= mi.rcWork.top {
        return fallback_work_area_for_window(win);
    }
    mi.rcWork
}

#[cfg(not(target_os = "windows"))]
pub fn get_work_area_for_window(
    win: &tauri::WebviewWindow,
) -> windows_sys::Win32::Foundation::RECT {
    fallback_work_area_for_window(win)
}

fn fallback_work_area_for_window(
    win: &tauri::WebviewWindow,
) -> windows_sys::Win32::Foundation::RECT {
    let (x, y, w, h) = if let Ok(Some(m)) = win.current_monitor() {
        let s = m.size();
        let p = m.position();
        (p.x, p.y, s.width as i32, s.height as i32)
    } else {
        (0, 0, 1920, 1080)
    };
    windows_sys::Win32::Foundation::RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

/// 预创建两个 pet 窗口（正常 + 折叠 + 吸附），启动时隐藏备用
pub fn precreate_pet_windows(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    // 正常窗口 128x128
    let saved_position = AppSettings::load().appearance.pet_position;

    if app.get_webview_window("pet").is_none() {
        let mut builder = WebviewWindowBuilder::new(app, "pet", WebviewUrl::App("pet.html".into()))
            .title("8Bit Cat")
            .inner_size(128.0, 128.0)
            .decorations(false)
            .transparent(true)
            .background_color(tauri::webview::Color(0, 0, 0, 0))
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false);
        if let Some(pos) = saved_position {
            builder = builder.position(pos.x as f64, pos.y as f64);
        }
        builder.build()?;
        if let (Some(w), Some(pos)) = (app.get_webview_window("pet"), saved_position) {
            match (w.outer_size(), w.scale_factor()) {
                (Ok(size), Ok(scale)) => {
                    let work = get_work_area_for_window(&w);
                    let width = ((size.width as f64) / scale).round() as i32;
                    let height = ((size.height as f64) / scale).round() as i32;
                    let (x, y) = clamp_position_to_work_area(pos.x, pos.y, width, height, work);
                    if let Err(e) = w.set_position(PhysicalPosition::new(x, y)) {
                        warn!(error = %e, x, y, "恢复桌宠位置失败");
                    } else {
                        let ws: tauri::State<'_, SharedWindowState> = app.state();
                        remember_pet_position(&ws, x, y);
                    }
                }
                (Err(e), _) => warn!(error = %e, "读取桌宠窗口尺寸失败，跳过位置校正"),
                (_, Err(e)) => warn!(error = %e, "读取桌宠窗口缩放失败，跳过位置校正"),
            }
        }
        info!("预创建 pet 窗口 (128x128)");
    }

    // 折叠窗口 48x48
    if app.get_webview_window("pet-mini").is_none() {
        WebviewWindowBuilder::new(app, "pet-mini", WebviewUrl::App("pet.html".into()))
            .title("8Bit Cat Mini")
            .inner_size(48.0, 48.0)
            .decorations(false)
            .transparent(true)
            .background_color(tauri::webview::Color(0, 0, 0, 0))
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .build()?;
        if let Some(w) = app.get_webview_window("pet-mini") {
            let _ = w.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
        }
        info!("预创建 pet-mini 窗口 (48x48, hidden)");
    }

    // 吸附窗口 40x120
    if app.get_webview_window("pet-snap").is_none() {
        WebviewWindowBuilder::new(app, "pet-snap", WebviewUrl::App("pet.html".into()))
            .title("8Bit Cat Snap")
            .inner_size(SNAP_W, SNAP_H as f64)
            .decorations(false)
            .transparent(true)
            .background_color(tauri::webview::Color(0, 0, 0, 0))
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .build()?;
        if let Some(w) = app.get_webview_window("pet-snap") {
            let _ = w.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
        }
        info!("预创建 pet-snap 窗口 ({}x{}, hidden)", SNAP_W, SNAP_H);
    }

    Ok(())
}
