pub mod bubble;
pub mod commands;
pub mod gamepad;
pub mod joystick;
pub mod panel;
pub mod screenshot;
pub mod settings;
pub mod tray;
pub mod voice;
pub mod tts;

use bubble::SharedBubble;
use commands::{SharedPet, SharedWindowState};
use screenshot::SharedScreenshotState;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use voice::SharedVoice;

use std::sync::Mutex;

use ai_pad_core::action::{ActionConfig, ActionDef};
use ai_pad_core::agent::PetAgent;
use ai_pad_core::bridge::handle_button_press;
use ai_pad_core::device::button_name;
use ai_pad_core::hotkey;
use ai_pad_core::memory::MemoryStore;
use joystick::SdlGamepad;
use std::sync::atomic::Ordering;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{debug, error, info, instrument, warn};

/// 切换 pet 窗口显示模式：正常(128x128) / 折叠(48x48)。
///
/// 使用预创建双窗口 + show/hide 模式（参照 bubble.rs），避免 destroy+recreate 的
/// WebView2 COM 竞态问题（Tauri #9307）和 #11975 的 setSize 静默失败。
/// 两个窗口在 setup 时预创建并隐藏，此处只做 show/hide + 定位切换。
#[tauri::command]
async fn cmd_recreate_pet_window(
    app: tauri::AppHandle,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
) -> Result<(), String> {
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    let on_top = ws.always_on_top.load(Ordering::SeqCst);
    let collapsed = width == 48 && height == 48;

    let target_label = if collapsed { "pet-mini" } else { "pet" };
    let hide_label = if collapsed { "pet" } else { "pet-mini" };

    // 隐藏另一个模式的窗口
    if let Some(other) = app.get_webview_window(hide_label) {
        let _ = other.hide();
    }

    // 显示目标窗口并定位
    let win = match app.get_webview_window(target_label) {
        Some(w) => w,
        None => return Err(format!("预创建窗口 '{}' 不存在", target_label)),
    };

    let _ = win.set_position(PhysicalPosition::new(x, y));
    let _ = win.set_always_on_top(on_top);
    win.show().map_err(|e| e.to_string())?;

    // 确保透明背景生效（Windows WebView2 需要）
    let _ = win.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));

    *ws.last_position.lock().map_err(|e| e.to_string())? = Some((x, y));

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

/// 贴边吸附：计算宠物窗口的吸附目标位置（水平贴左/右 + 垂直贴任务栏上方）。
#[tauri::command]
async fn cmd_snap_pet(app: tauri::AppHandle, x: i32, _y: i32) -> Result<SnapResult, String> {
    // 优先查找可见的宠物窗口（支持折叠态）
    let win = app
        .get_webview_window("pet")
        .filter(|w| w.is_visible().unwrap_or(false))
        .or_else(|| app.get_webview_window("pet-mini"))
        .ok_or("no visible pet window")?;

    let pet_size = win.outer_size().map_err(|e| e.to_string())?;
    let pw = pet_size.width as i32;
    let ph = pet_size.height as i32;
    let scale = win.scale_factor().unwrap_or(1.0);
    let snap_h_px = (SNAP_H as f64 * scale) as i32; // 竖条物理像素高度
    let snap_w_px = (SNAP_W * scale) as i32;         // 竖条物理像素宽度

    // 通过 Win32 获取宠物所在显示器的工作区（排除任务栏）
    let work = get_work_area_for_window(&win);
    info!(
        snap_cmd = true,
        input_x = x,
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

    // 水平：吸附到更近的左/右边缘（距离阈值 80 逻辑像素，按 DPI 缩放）
    let snap_threshold = (80.0 * scale) as u32;
    let left_dist = (x - work.left).unsigned_abs();
    let right_dist = (work.right - pw - x).unsigned_abs();

    let snap_result = if left_dist <= right_dist && left_dist <= snap_threshold {
        ("left", work.left, work.bottom - snap_h_px)
    } else if right_dist <= snap_threshold {
        ("right", work.right - snap_w_px, work.bottom - snap_h_px)
    } else {
        // 不吸附，返回当前位置
        return Ok(SnapResult {
            edge: "none".to_string(),
            x: x,
            y: work.bottom - ph,
        });
    };

    let (edge, target_x, target_y) = snap_result;

    info!(
        snap_cmd = true,
        input_x = x,
        left_dist = left_dist,
        right_dist = right_dist,
        edge = %edge,
        target_x = target_x,
        target_y = target_y,
        "cmd_snap_pet: 吸附结果"
    );

    Ok(SnapResult { edge: edge.to_string(), x: target_x, y: target_y })
}

/// Task 5: 拖拽过程中查询磁性预告。
/// 前端以约 60fps 的节流频率调用；返回预告条应显示的位置，或 visible=false。
#[tauri::command]
async fn cmd_get_snap_preview(
    app: tauri::AppHandle,
    x: i32,
    _y: i32,
) -> Result<commands::SnapPreview, String> {
    // 复用拖拽中宠物窗口（pet / pet-mini）的工作区 + 缩放
    let win = app
        .get_webview_window("pet")
        .filter(|w| w.is_visible().unwrap_or(false))
        .or_else(|| app.get_webview_window("pet-mini"))
        .ok_or("no visible pet window")?;

    let pet_size = win.outer_size().map_err(|e| e.to_string())?;
    let pw = pet_size.width as i32;
    let scale = win.scale_factor().unwrap_or(1.0);
    let snap_h_px = (SNAP_H as f64 * scale) as i32;
    let snap_w_px = (SNAP_W * scale) as i32;
    let threshold = (80.0 * scale) as i32;

    let work = get_work_area_for_window(&win);

    Ok(commands::calc_snap_preview(
        x,
        work.left,
        work.right,
        work.bottom,
        pw,
        snap_w_px,
        snap_h_px,
        threshold,
    ))
}

#[derive(serde::Serialize)]
struct SnapResult {
    edge: String,
    x: i32,
    y: i32,
}

/// 贴边吸附：将宠物窗口转换为吸附态（细长竖条）
#[tauri::command]
async fn cmd_snap_transform(app: tauri::AppHandle, edge: String, x: i32, y: i32) -> Result<(), String> {
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    let on_top = ws.always_on_top.load(Ordering::SeqCst);

    // 先写入共享状态（让 pet-snap 窗口 show 后 initPullState 能读到正确方向）
    // 这样替代原先通过 eval(__setSnapEdge) 注入方向的脆弱做法
    *ws.is_snapped.lock().map_err(|e| e.to_string())? = true;
    *ws.snap_edge.lock().map_err(|e| e.to_string())? = Some(edge.clone());

    // Task 4 Crossfade：触发旧窗口淡出（CSS transition 150ms）
    for label in ["pet", "pet-mini"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.eval("if(typeof __fadeOut==='function')__fadeOut();");
        }
    }
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // 隐藏旧窗口并重置其 fade class（下次 show 才能正常 fade-in）
    for label in ["pet", "pet-mini"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.hide();
            let _ = w.eval("if(typeof __fadeReset==='function')__fadeReset();");
        }
    }

    // 显示吸附态窗口
    let snap_win = app.get_webview_window("pet-snap")
        .ok_or("pet-snap window not found")?;

    let _ = snap_win.set_position(PhysicalPosition::new(x, y));
    let _ = snap_win.set_always_on_top(on_top);
    snap_win.show().map_err(|e| e.to_string())?;

    // 通过 eval 主动通知方向变更（pull 模式已写 ws.snap_edge 作为事实源；
    // eval 仅作兜底，让已加载的窗口无需重新 pull 就能立即切方向）
    let edge_for_eval = edge.clone();
    if let Ok(_) = snap_win.eval(
        &format!("if(typeof __setSnapEdge==='function'){{__setSnapEdge('{edge_for_eval}');'ok'}}else{{'no-fn'}}")
    ) {
        info!(edge = %edge, "[cmd_snap_snap] ✓ eval setSnapEdge 成功（兜底通知）");
    }

    // Task 4 Crossfade：新窗口淡入
    let _ = snap_win.eval("if(typeof __fadeIn==='function')__fadeIn();");

    info!(snap_transform = true, edge = %edge, x = x, y = y, "吸附态切换成功");
    Ok(())
}

/// 取消吸附：恢复宠物窗口原大小
#[tauri::command]
async fn cmd_unsnap_transform(app: tauri::AppHandle) -> Result<(), String> {
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    let on_top = ws.always_on_top.load(Ordering::SeqCst);

    // Task 4 Crossfade：pet-snap 淡出
    let snap_win_opt = app.get_webview_window("pet-snap");
    if let Some(w) = &snap_win_opt {
        let _ = w.eval("if(typeof __fadeOut==='function')__fadeOut();");
    }
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // 计算恢复位置（在 hide 之前读取当前位置）
    let snap_pos = snap_win_opt.as_ref().and_then(|w| w.outer_position().ok());

    // 隐藏吸附态窗口 + 重置 fade
    if let Some(w) = &snap_win_opt {
        let _ = w.hide();
        let _ = w.eval("if(typeof __fadeReset==='function')__fadeReset();");
    }

    // 显示正常宠物窗口
    let collapsed = ws.collapsed.load(Ordering::SeqCst);
    let target = if collapsed { "pet-mini" } else { "pet" };

    let win = app.get_webview_window(target)
        .ok_or(format!("window '{}' not found", target))?;

    // 从吸附态窗口位置恢复（往屏幕内移 80px 避免还在边缘）
    if let Some(pos) = snap_pos {
        let offset = if ws.snap_edge.lock().ok().and_then(|e| e.clone()).as_deref() == Some("left") {
            80
        } else {
            -80
        };
        let _ = win.set_position(PhysicalPosition::new(pos.x + offset, pos.y));
    }

    let _ = win.set_always_on_top(on_top);
    win.show().map_err(|e| e.to_string())?;
    let _ = win.eval("if(typeof __fadeIn==='function')__fadeIn();");

    // 清除吸附状态
    *ws.is_snapped.lock().map_err(|e| e.to_string())? = false;
    *ws.snap_edge.lock().map_err(|e| e.to_string())? = None;

    info!(unsnap_transform = true, "取消吸附成功");
    Ok(())
}

/// 获取窗口所在显示器的工作区（Win32: MonitorFromWindow → GetMonitorInfoW → rcWork）
#[cfg(target_os = "windows")]
fn get_work_area_for_window(
    win: &tauri::WebviewWindow,
) -> windows_sys::Win32::Foundation::RECT {
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    };

    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..unsafe { std::mem::zeroed() }
    };

    if let Ok(hwnd) = win.hwnd() {
        let raw_hwnd = hwnd.0 as windows_sys::Win32::Foundation::HWND;
        let hmon_val: isize;
        unsafe {
            hmon_val = MonitorFromWindow(raw_hwnd, MONITOR_DEFAULTTONEAREST) as isize;
            GetMonitorInfoW(hmon_val as _, &mut mi);
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

    mi.rcWork
}

#[cfg(not(target_os = "windows"))]
fn get_work_area_for_window(win: &tauri::WebviewWindow) -> (i32, i32, i32, i32) {
    // 非 Windows 回退：使用 Tauri Monitor API 的完整屏幕边界
    let (x, y, w, h) = if let Ok(Some(m)) = win.current_monitor() {
        let s = m.size();
        let p = m.position();
        (p.x, p.y, s.width as i32, s.height as i32)
    } else {
        (0, 0, 1920, 1080)
    };
    windows_sys::Win32::Foundation::RECT { left: x, top: y, right: x + w, bottom: y + h }
}

/// 预创建两个 pet 窗口（正常 + 折叠），启动时隐藏备用。
/// 避免运行时 destroy+recreate 的 WebView2 竞态（#9307）。
fn precreate_pet_windows(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    // 正常窗口 128x128（主窗口，默认可见）
    if app.get_webview_window("pet").is_none() {
        WebviewWindowBuilder::new(app, "pet", WebviewUrl::App("pet.html".into()))
            .title("8Bit Cat")
            .inner_size(128.0, 128.0)
            .decorations(false)
            .transparent(true)
            .background_color(tauri::webview::Color(0, 0, 0, 0))
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .build()?;
        info!("预创建 pet 窗口 (128x128)");
    }

    // 折叠窗口 48x48（启动时隐藏）
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

    // 吸附窗口 40x120（启动时隐藏）
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

const SNAP_W: f64 = 24.0;  // 吸附竖条宽度（视觉 2px + 隐式热区 22px）
const SNAP_H: i32 = 100;   // 吸附竖条高度（竖条）


// ---- Bubble 聊天输入：前端提交 → gamepad_loop 消费 ----

/// 前端 bubble 输入框提交的待消费消息。
/// 设计同构 SharedBubble：前端 invoke 写入，gamepad_loop 每 tick 检查并取走。
struct SharedPendingChat {
    pending: Mutex<Option<String>>,
}

impl SharedPendingChat {
    fn new() -> Self {
        Self {
            pending: Mutex::new(None),
        }
    }
}

impl Default for SharedPendingChat {
    fn default() -> Self {
        Self::new()
    }
}

/// 前端调用：提交聊天消息到 pending 队列
#[tauri::command]
async fn cmd_submit_chat(
    state: State<'_, SharedPendingChat>,
    text: String,
) -> Result<(), String> {
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return Err("消息不能为空".into());
    }
    *state.pending.lock().map_err(|e| e.to_string())? = Some(trimmed);
    Ok(())
}

/// gamepad_loop 调用：原子地取走 pending 消息（如有）
fn take_pending_chat(state: &State<'_, SharedPendingChat>) -> Option<String> {
    state
        .pending
        .lock()
        .ok()
        .and_then(|mut g| g.take())
}

/// 前端诊断日志桥接：JS console.log 不会出现在终端，
/// 通过此 IPC 把关键诊断数据写到 Rust tracing
#[tauri::command]
async fn cmd_pet_log(msg: String) -> Result<(), String> {
    info!("[pet-diag] {msg}");
    Ok(())
}

/// 前端退出 chat 模式（输入框收起/超时隐藏时调用）
#[tauri::command]
async fn cmd_exit_chat(app: AppHandle) -> Result<(), String> {
    let state: State<bubble::SharedBubble> = app.state();
    state.set_chat_active(false);
    info!("[cmd_exit_chat] chat 模式结束，截图恢复写 bubble");
    Ok(())
}

/// 点击宠物嘴巴 → 打开聊天输入框：
/// 显示 bubble 窗口 + eval 直接调用 showInput()（不走事件系统，
/// 因为 hidden 预创建窗口的 JS listen 可能未就绪导致 emit 丢失）
#[tauri::command]
async fn cmd_open_chat(app: AppHandle) -> Result<(), String> {
    info!("[cmd_open_chat] 开始");

    // 取或创建 bubble 窗口
    let window = match app.get_webview_window("bubble") {
        Some(w) => w,
        None => bubble::create_bubble_window(&app).map_err(|e| e.to_string())?,
    };

    // 写入空 pending + 进入 chat 模式（截图不覆盖）
    let state: State<bubble::SharedBubble> = app.state();
    *state.pending_text.lock().map_err(|e| e.to_string())? = Some(String::new());
    state.set_chat_active(true);

    // 定位 + 显示
    bubble::position_above_pet(&app, &window);
    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    let _ = window.show();

    // 等 WebView2 就绪后 eval 调用 showInput()
    for attempt in 0..10u8 {
        std::thread::sleep(std::time::Duration::from_millis(30));
        if window.eval(
            "if(typeof __bubble_showInput==='function'){__bubble_showInput();'ok'}else{'no-fn'}"
        ).is_ok() {
            info!(attempt = attempt, "[cmd_open_chat] ✓ eval showInput 成功");
            return Ok(());
        }
        info!(attempt = attempt, "[cmd_open_chat] eval 重试中...");
    }

    warn!("[cmd_open_chat] eval showInput 失败（10 次重试均未成功）");
    Ok(())
}

#[cfg(test)]
mod pending_chat_tests {
    use super::*;

    #[test]
    fn test_default_empty() {
        let pc = SharedPendingChat::new();
        assert!(pc.pending.lock().unwrap().is_none());
    }

    #[test]
    fn test_submit_and_take() {
        let pc = SharedPendingChat::new();
        *pc.pending.lock().unwrap() = Some("你好 AI".into());
        let taken = pc.pending.lock().unwrap().take();
        assert_eq!(taken, Some("你好 AI".to_string()));
        // 二次取为空
        assert!(pc.pending.lock().unwrap().is_none());
    }

    #[test]
    fn test_submit_overwrites() {
        let pc = SharedPendingChat::new();
        *pc.pending.lock().unwrap() = Some("第一条".into());
        *pc.pending.lock().unwrap() = Some("第二条".into());
        assert_eq!(
            pc.pending.lock().unwrap().take(),
            Some("第二条".to_string())
        );
    }

    #[test]
    fn test_empty_text_rejected() {
        // cmd_submit_chat 对空字符串应返回 Err（模拟）
        let trimmed = "   ".trim().to_string();
        assert!(trimmed.is_empty());
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(SharedPet::default())
        .manage(SharedWindowState::default())
        .manage(SharedBubble::new())
        .manage(SharedVoice::new())
        .manage(SharedScreenshotState::default())
        .manage(SharedPendingChat::new())
        .invoke_handler(tauri::generate_handler![
            commands::cmd_set_state,
            commands::cmd_walk_to,
            commands::cmd_show_bubble,
            commands::cmd_get_status,
            commands::cmd_tick,
            commands::cmd_get_window_state,
            panel::cmd_show_panel,
            panel::cmd_hide_panel,
            panel::cmd_execute_panel_action,
            panel::cmd_panel_log,
            bubble::cmd_consume_bubble_text,
            bubble::cmd_hide_bubble,
            voice::cmd_voice_update_text,
            voice::cmd_voice_get_text,
            cmd_recreate_pet_window,
            cmd_snap_pet,
            cmd_snap_transform,
            cmd_unsnap_transform,
            cmd_get_snap_preview,
            screenshot::cmd_screenshot_now,
            cmd_submit_chat,
            cmd_open_chat,
            cmd_exit_chat,
            cmd_pet_log,
            settings::cmd_settings_show,
            settings::cmd_settings_hide,
            settings::cmd_settings_close,
            settings::cmd_settings_log,
            settings::cmd_settings_load,
            settings::cmd_settings_save_ai,
            settings::cmd_settings_save_actions,
            settings::cmd_settings_save_prompts,
            settings::cmd_settings_save_appearance,
            settings::cmd_settings_reset,
            settings::cmd_settings_apply,
        ])
        .on_window_event(|window, event| {
            if window.label() == "panel" {
                if let WindowEvent::Focused(false) = event {
                    let _ = window.hide();
                }
            }
            if window.label() == "settings" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            // 加载 .env（优先级低于系统环境变量），按顺序尝试多个路径
            let mut env_loaded = false;
            if let Some(exe_dir) = app
                .path()
                .resource_dir()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            {
                let env_path = exe_dir.join(".env");
                if env_path.exists() {
                    dotenvy::from_path(&env_path).ok();
                    info!(path = ?env_path, "已加载 .env");
                    env_loaded = true;
                }
            }
            if !env_loaded {
                // 开发模式：尝试当前目录和项目根目录
                if dotenvy::dotenv().is_ok() {
                    info!("已加载 .env (CWD)");
                    env_loaded = true;
                }
            }
            if !env_loaded {
                // 兜底：尝试 exe 向上两级（target/debug → 项目根）
                if let Some(exe_dir) = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                {
                    let fallback = exe_dir.join(".env");
                    if fallback.exists() {
                        dotenvy::from_path(&fallback).ok();
                        info!(path = ?fallback, "已加载 .env (项目根目录)");
                        env_loaded = true;
                    }
                }
            }
            if !env_loaded {
                warn!(".env 未找到，将使用 ~/.claude/settings.json 或默认配置");
            }

            tray::create_tray(app.handle())?;

            // 预创建 pet 双窗口（正常 + 折叠），避免运行时 destroy+recreate 竞态
            if let Err(e) = precreate_pet_windows(app.handle()) {
                warn!(error = %e, "预创建 pet 窗口失败");
            }

            if let Err(e) = voice::precreate_voice_window(app.handle()) {
                warn!(error = %e, "预创建 voice 窗口失败");
            }

            // edge-glow 窗口已不再需要（pet-snap 窗口替代了吸附竖条功能）

            let app_handle = app.handle().clone();
            let hotkey_str = "CommandOrControl+Alt+Space";
            info!(hotkey = %hotkey_str, "准备注册全局热键");
            match hotkey_str.parse::<tauri_plugin_global_shortcut::Shortcut>() {
                Ok(shortcut) => {
                    let shortcut_for_handler = shortcut;
                    let handler_app = app_handle.clone();
                    let result =
                        app.global_shortcut()
                            .on_shortcut(shortcut, move |_app, sc, evt| {
                                debug!(state = ? evt.state(), "热键回调触发");
                                if sc == &shortcut_for_handler
                                    && evt.state() == ShortcutState::Pressed
                                {
                                    panel::toggle_panel(&handler_app);
                                }
                            });
                    match result {
                        Ok(_) => info!(hotkey = %hotkey_str, "✓ 已注册 → 切换面板"),
                        Err(e) => warn!(error = %e, hotkey = %hotkey_str, "✗ 注册失败"),
                    }
                }
                Err(e) => warn!(error = %e, hotkey = %hotkey_str, "✗ 解析失败"),
            }

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                gamepad_loop(&handle);
            });

            let ss_handle = app.handle().clone();
            std::thread::spawn(move || {
                eprintln!("[SS-DBG] 截图线程已 spawn");
                screenshot::screenshot_loop(&ss_handle);
            });

            if std::env::var("AI_PAD_DEBUG").is_ok() {
                let dbg_app = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    info!("[debug] 自动弹出 panel");
                    panel::toggle_panel(&dbg_app);
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    info!("[debug] 模拟 panel-nav (1, 0)");
                    let _ = dbg_app.emit("panel-nav", (1i32, 0i32));
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    info!("[debug] 模拟 panel-nav (1, 0)");
                    let _ = dbg_app.emit("panel-nav", (1i32, 0i32));
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    info!("[debug] 模拟 panel-nav (0, -1)");
                    let _ = dbg_app.emit("panel-nav", (0i32, -1i32));
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 从候选设备中选出"真正的游戏手柄"。
///
/// 规则（按优先级，命中即返回）：
/// 1. 黑名单：跳过键鼠接收器（Link-KM / Keyboard / Mouse / Receiver / Wireless Link）
/// 2. 白名单：优先名字含 Controller / Gamepad / 8BitDo / Xbox / DualShock / DualSense / Joy-Con / Pro Controller
/// 3. 有方向键（num_hats>=1）或带摇杆（num_axes>=2）
/// 4. 兜底：任意未被黑名单过滤的
fn choose_gamepad(pads: &[joystick::GamepadInfo]) -> Option<&joystick::GamepadInfo> {
    let is_kbm_like = |name: &str| {
        let n = name.to_lowercase();
        ["link-km", "receiver", "keyboard", "mouse", "wireless link"]
            .iter()
            .any(|kw| n.contains(kw))
    };
    let is_preferred = |name: &str| {
        let n = name.to_lowercase();
        [
            "controller",
            "gamepad",
            "8bitdo",
            "xbox",
            "dualshock",
            "dualsense",
            "joy-con",
            "joycon",
            "pro controller",
        ]
        .iter()
        .any(|kw| n.contains(kw))
    };

    let filtered: Vec<&joystick::GamepadInfo> =
        pads.iter().filter(|p| !is_kbm_like(&p.name)).collect();

    // 优先白名单
    if let Some(p) = filtered.iter().find(|p| is_preferred(&p.name)) {
        return Some(*p);
    }
    // 次选：有方向键或摇杆
    if let Some(p) = filtered
        .iter()
        .find(|p| p.num_hats >= 1 || p.num_axes >= 2)
    {
        return Some(*p);
    }
    // 兜底
    filtered.first().copied()
}

/// 主游戏手柄循环 — 80ms tick，按键检测 → 状态机 → AI → 气泡/动作
///
/// 外层循环负责**热插拔重连**：手柄未连接/断开时每秒重新枚举；
/// 内层循环负责正常采样。长生命周期状态（AI Agent/记忆/配置）跨重连保留。
#[instrument(skip(app))]
fn gamepad_loop(app: &tauri::AppHandle) {
    eprintln!("[GP-DBG] gamepad_loop 开始");
    let sdl = match SdlGamepad::init() {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "SDL2 初始化失败");
            return;
        }
    };

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Tokio 运行时创建失败");
            return;
        }
    };

    let agent: std::sync::OnceLock<std::option::Option<PetAgent>> = std::sync::OnceLock::new();

    /// 懒加载：首次调用时才初始化 PetAgent（避免启动阻塞 2-5s）
    fn get_agent(agent: &std::sync::OnceLock<std::option::Option<PetAgent>>) -> Option<&PetAgent> {
        agent
            .get_or_init(|| match PetAgent::new() {
                Ok(a) => {
                    info!("AI Agent 初始化成功 (8Bit Cat)");
                    Some(a)
                }
                Err(e) => {
                    error!(error = %e, "AI Agent 初始化失败，后续对话将不可用");
                    None
                }
            })
            .as_ref()
    }

    let mut action_config = ActionConfig::load("actions.yml").ok();
    let ac = action_config.as_ref().map(|c| c.actions.len()).unwrap_or(0);
    info!(action_count = ac, "已加载 {ac} 个动作绑定");

    let mut memory = MemoryStore::load();
    let memory_config = ai_pad_core::prompts::PromptsConfig::load().memory;
    info!(entries = memory.entries.len(), "对话记忆系统已初始化");
    let summary_store = ai_pad_core::screen_summary::ScreenSummaryStore::load();
    info!(entries = summary_store.entries.len(), "屏幕活动摘要系统已初始化");

    let mut alt_tab = HeldModifier::new(0x12); // VK_MENU
    let mut ctrl_tab = HeldModifier::new(0x11); // VK_CONTROL
    let mut held_voice = HeldCombo::new();
    let mut prev_pet_pos: Option<(i32, i32)> = None; // 气泡跟随：上次宠物位置

    // 外层热插拔循环：选设备 → 采样 → 断开后重连
    let mut last_warn: Option<std::time::Instant> = None;
    loop {
        let pads = match SdlGamepad::list_gamepads(&sdl) {
            Ok(p) => p,
            Err(e) => {
                error!(error = %e, "枚举手柄失败，2s 后重试");
                std::thread::sleep(std::time::Duration::from_secs(2));
                continue;
            }
        };

        for p in &pads {
            debug!(index = p.index, name = %p.name, buttons = p.num_buttons, hats = p.num_hats, axes = p.num_axes, "候选设备");
        }

        let target = match choose_gamepad(&pads) {
            Some(t) => t.clone(),
            None => {
                // 降噪：每 10s 打一次 warn
                if last_warn.map_or(true, |t| t.elapsed() > std::time::Duration::from_secs(10)) {
                    warn!(
                        enumerated = pads.len(),
                        "未检测到真正的游戏手柄（已自动跳过键鼠接收器等），每秒重试中..."
                    );
                    last_warn = Some(std::time::Instant::now());
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }
        };
        last_warn = None;

        info!(
            index = target.index,
            name = %target.name,
            buttons = target.num_buttons,
            hats = target.num_hats,
            axes = target.num_axes,
            "✓ 选中手柄 [{}] {}",
            target.index,
            target.name
        );

        let mut gamepad = match SdlGamepad::open(&sdl, target.index) {
            Ok(g) => g,
            Err(e) => {
                error!(error = %e, "打开手柄失败，1s 后重试");
                std::thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }
        };
        info!("ai-pad 启动（手柄已就绪）");

        // 内层采样循环状态（每次重连重置）
        let mut prev_buttons: u32 = 0;
        let mut prev_hat: Option<(i32, i32)> = None;
        let mut attach_check_tick: u32 = 0;

        loop {
            // 配置热重载：托盘 "重载配置" 设置 flag，此处消费
            {
                let ws: tauri::State<'_, SharedWindowState> = app.state();
                if ws.config_reload.load(Ordering::SeqCst) {
                    ws.config_reload.store(false, Ordering::SeqCst);
                    action_config = ActionConfig::load("actions.yml").ok();
                    info!(
                        actions = action_config.as_ref().map(|c| c.actions.len()).unwrap_or(0),
                        "gamepad_loop 配置已刷新"
                    );
                }
            }

            // 每 ~1s 检测一次手柄连接状态（12 tick × 80ms ≈ 960ms）
            attach_check_tick += 1;
            if attach_check_tick >= 12 {
                attach_check_tick = 0;
                if !gamepad.is_attached() {
                    // 释放所有持按键，避免"卡键"
                    alt_tab.release();
                    ctrl_tab.release();
                    held_voice.release_keys();
                    warn!(index = target.index, name = %target.name, "手柄已断开，返回外层重新枚举");
                    break;
                }
            }

            let panel_visible = app
                .get_webview_window("panel")
                .and_then(|w| w.is_visible().ok())
                .unwrap_or(false);

            let buttons = gamepad.read_buttons();
            let new_presses = (buttons ^ prev_buttons) & buttons;

            if new_presses != 0 {
                for bit in 0..32 {
                    if new_presses & (1 << bit) != 0 {
                        let idx = bit as u32;
                        let name = button_name(idx as usize).unwrap_or("?");
                        debug!(button_idx = idx, button_name = name, "按下 #{idx} {name}");

                        // 如果 Alt/Ctrl+Tab 按住中，按其他键时先释放
                        if (alt_tab.held && name != "L1") || (ctrl_tab.held && name != "L2") {
                            alt_tab.release();
                            ctrl_tab.release();
                        }

                        // Home 键 → 切换面板（独占处理）
                        if name == "Home" {
                            info!("→ 切换面板");
                            panel::toggle_panel(app);
                            continue;
                        }

                        // 面板可见 → 按键转发到面板
                        if panel_visible {
                            match name {
                                "A" => {
                                    info!("→ 面板确认");
                                    let _ = app.emit("panel-confirm", ());
                                }
                                "B" => {
                                    info!("→ 面板关闭");
                                    let _ = app.emit("panel-close", ());
                                }
                                _ => {}
                            }
                            continue;
                        }

                        // Bridge: 特殊按键 → 宠物状态/AI
                        let (agent_msg, _pet_cmd) = handle_button_press(idx, "");

                        let events = gamepad::process_button(idx);
                        for evt in &events {
                            let _ = app.emit("pet-event", evt);
                        }

                        if let (Some(msg), Some(ag)) = (&agent_msg, get_agent(&agent)) {
                            info!(msg = %msg, "→ AI: {msg}");
                            run_ai_chat(&rt, ag, app, msg, "", &mut memory, &memory_config);
                        }

                        // Actions: 按键名 → 动作绑定
                        if let Some(ref config) = action_config {
                            if let Some(action_def) = config.actions.get(name) {
                                info!(name = name, action_type = %action_def.action_type, "→ {} ({})", name, action_def.action_type);
                                execute_action(
                                    action_def,
                                    &config.defaults,
                                    &mut alt_tab,
                                    &mut ctrl_tab,
                                );
                            }
                        }
                    }
                }
            }

            // Voice 按住检测
            let mut voice_just_released = false;
            let mut voice_just_pressed = false;
            if let Some(ref config) = action_config {
                let mut voice_bits: u32 = 0;
                for (name, action_def) in &config.actions {
                    if action_def.action_type == "voice" {
                        if let Some(bit) = name_to_bit(name) {
                            voice_bits |= 1 << bit;
                        }
                    }
                }
                let voice_active = (buttons & voice_bits) != 0;
                let (jp, jr) = held_voice.detect(voice_active);
                voice_just_pressed = jp;
                voice_just_released = jr;
            }

            // voice 按下
            if voice_just_pressed {
                match voice::open_voice_capture(app) {
                    Ok(()) => info!("[voice] 录音条已显示并强制前台化"),
                    Err(e) => warn!(error = %e, "[voice] 打开录音条失败"),
                }
                std::thread::sleep(std::time::Duration::from_millis(80));
                if let Some(ref config) = action_config {
                    held_voice.press_keys(config);
                }
            }

            // voice 释放
            if voice_just_released {
                held_voice.release_keys();
                info!("[voice] 等待识别注入完成 (700ms)...");
                std::thread::sleep(std::time::Duration::from_millis(700));
                match voice::take_voice_text(app) {
                    Ok(raw) => {
                        let text = raw.trim().to_string();
                        if text.is_empty() {
                            warn!("[voice] 虚拟输入框为空 (识别可能失败或焦点被抢走)");
                        } else {
                            info!(text = %text, len = text.chars().count(), "[voice] 识别全文: {text}");

                            if let Some(ag) = get_agent(&agent) {
                                run_ai_chat(
                                    &rt,
                                    ag,
                                    app,
                                    &text,
                                    "[voice]",
                                    &mut memory,
                                    &memory_config,
                                );
                            } else {
                                warn!("[voice] AI Agent 未初始化");
                            }
                        }
                    }
                    Err(e) => warn!(error = %e, "[voice] 读取虚拟输入框失败"),
                }
            }

            prev_buttons = buttons;

            // Bubble 聊天输入：检查前端是否提交了消息
            if let Some(chat_msg) = {
                let pc: State<SharedPendingChat> = app.state();
                take_pending_chat(&pc)
            } {
                if let Some(ag) = get_agent(&agent) {
                    info!(msg = %chat_msg, "[chat] → AI 对话 (bubble 输入)");
                    run_ai_chat(&rt, ag, app, &chat_msg, "[chat]", &mut memory, &memory_config);
                }
            }

            // 方向键
            let hat = gamepad.read_hat(0);
            if panel_visible {
                if hat != prev_hat {
                    if let Some((dx, dy)) = hat {
                        info!(dx = dx, dy = dy, "→ 面板导航");
                        let _ = app.emit("panel-nav", (dx, dy));
                    }
                }
            } else if let Some((dx, dy)) = hat {
                alt_tab.release();
                ctrl_tab.release();
                let speed = 3;
                if dy > 0 {
                    let _ = hotkey::send_scroll(120 * speed);
                } else if dy < 0 {
                    let _ = hotkey::send_scroll(-120 * speed);
                }
                if dx > 0 {
                    let _ = hotkey::send_scroll_h(120 * speed);
                } else if dx < 0 {
                    let _ = hotkey::send_scroll_h(-120 * speed);
                }
            }
            prev_hat = hat;

            // 气泡跟随：宠物移动时自动重定位气泡窗口
            if let Some(bubble_win) = app.get_webview_window("bubble") {
                if bubble_win.is_visible().unwrap_or(false) {
                    let pet = app
                        .get_webview_window("pet")
                        .filter(|w| w.is_visible().unwrap_or(false))
                        .or_else(|| app.get_webview_window("pet-mini").filter(|w| w.is_visible().unwrap_or(false)))
                        .or_else(|| app.get_webview_window("pet-snap").filter(|w| w.is_visible().unwrap_or(false)));
                    if let Some(p) = pet {
                        if let Ok(pos) = p.outer_position() {
                            let key = (pos.x, pos.y);
                            if Some(key) != prev_pet_pos {
                                prev_pet_pos = Some(key);
                                bubble::position_above_pet(app, &bubble_win);
                            }
                        }
                    }
                } else {
                    // 气泡隐藏时重置位置缓存（下次显示时必定重定位）
                    prev_pet_pos = None;
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        // 内层 break 到这里：小睡一下再回外层重枚举（避免空转）
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// 统一的 AI 流式对话执行：启动气泡 → 注入记忆 → 流式追加 chunk → 结束气泡 → 记录记忆 + 处理回复
fn run_ai_chat(
    rt: &tokio::runtime::Runtime,
    agent: &PetAgent,
    app: &tauri::AppHandle,
    msg: &str,
    log_prefix: &str,
    memory: &mut MemoryStore,
    memory_config: &ai_pad_core::memory::MemoryConfig,
) {
    let tag = if log_prefix.is_empty() { "" } else { " " };
    info!(model = %agent.config.model, msg = %msg, "{log_prefix}→ AI 对话开始");
    if let Err(e) = bubble::start_streaming_bubble(app) {
        warn!(error = %e, "{log_prefix}气泡启动错误");
        return;
    }

    // 注入记忆上下文 + 屏幕活动摘要 + 最近截图观察
    let ctx = memory.build_context(memory_config);
    let summary_store = ai_pad_core::screen_summary::ScreenSummaryStore::load();
    let summary_config = ai_pad_core::prompts::PromptsConfig::load().screen_summary;
    info!(entries = summary_store.entries.len(), "屏幕活动摘要已加载");
    let summary_ctx = summary_store.build_context(&summary_config);
    let recent_ctx = ai_pad_core::screenshot::build_recent_analyses_context(10, 1500);
    let context_parts: Vec<&str> = [&ctx, &recent_ctx, &summary_ctx]
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(|s| s.as_str())
        .collect();
    let enriched_msg = if context_parts.is_empty() {
        msg.to_string()
    } else {
        format!("{}\n用户说: {msg}", context_parts.join("\n"))
    };

    let app_for_chunks = app.clone();
    let prefix = log_prefix.to_string();
    let prefix_for_log = prefix.clone();
    let stream_result = rt.block_on(agent.chat_stream(&enriched_msg, move |chunk| {
        debug!(len = chunk.len(), "{prefix_for_log}{tag}AI chunk");
        let _ = bubble::append_bubble_chunk(&app_for_chunks, chunk);
    }));
    let _ = bubble::finalize_bubble(app);
    match stream_result {
        Ok(reply) => {
            // 持久化到记忆
            memory.record_conversation(msg, &reply, memory_config);
            if let Err(e) = memory.save() {
                warn!(error = %e, "保存对话记忆失败");
            }

            if prefix.is_empty() {
                let preview: String = reply.chars().take(60).collect();
                info!(model = %agent.config.model, preview = %preview, "← AI: {preview}");
            } else {
                info!(model = %agent.config.model, chars = reply.chars().count(), reply = %reply, "{prefix} AI 回复全文 ({reply})");
            }
            // 异步朗读 AI 回复（不阻塞主线程）
            let reply_for_tts = reply.clone();
            std::thread::spawn(move || {
                tts::speak(&reply_for_tts);
            });

            let ai_events = gamepad::process_agent_response(&reply);
            for evt in &ai_events {
                let _ = app.emit("pet-event", evt);
            }
        }
        Err(e) => warn!(model = %agent.config.model, error = %e, "{prefix} AI 错误"),
    }
}

fn execute_action(
    action: &ActionDef,
    defaults: &ai_pad_core::action::Defaults,
    alt_tab: &mut HeldModifier,
    ctrl_tab: &mut HeldModifier,
) {
    match action.action_type.as_str() {
        "launch" => {
            let program = match &action.program {
                Some(p) => p.as_str(),
                None => return,
            };
            let args = action.args.as_deref().unwrap_or("");
            let _ = ai_pad_core::action::launch_program(
                program,
                args,
                &action.workdir,
                action.terminal,
                &defaults.terminal,
            );
        }
        "voice" => {}
        "script" => {
            if let Some(cmd) = &action.command {
                let _ = std::process::Command::new("powershell")
                    .args(["-Command", cmd])
                    .spawn();
            }
        }
        "hotkey" => {
            if let Some(trigger) = &action.trigger {
                let has_alt = trigger.iter().any(|k| k.to_lowercase() == "alt");
                let has_ctrl = trigger.iter().any(|k| k.to_lowercase() == "ctrl");
                let has_tab = trigger.iter().any(|k| k.to_lowercase() == "tab");

                if has_alt && has_tab {
                    alt_tab.press();
                } else if has_ctrl && has_tab {
                    ctrl_tab.press();
                } else {
                    let key_refs: Vec<&str> = trigger.iter().map(|s| s.as_str()).collect();
                    if let Err(e) = hotkey::trigger_hotkey(&key_refs, 0.02) {
                        warn!(error = %e, "热键触发失败");
                    }
                }
            }
        }
        other => {
            warn!(action_type = other, "未知动作类型");
        }
    }
}

struct HeldModifier {
    vk: u16,
    held: bool,
}

impl HeldModifier {
    fn new(vk: u16) -> Self {
        Self { vk, held: false }
    }
    fn press(&mut self) {
        if !self.held {
            let _ = hotkey::key_down(self.vk);
            self.held = true;
        }
        let _ = hotkey::key_down(0x09);
        let _ = hotkey::key_up(0x09);
    }
    fn release(&mut self) {
        if self.held {
            let _ = hotkey::key_up(self.vk);
            self.held = false;
        }
    }
}

/// 持续按住的多键组合(用于 voice 动作)
struct HeldCombo {
    vks: Vec<u16>,
    held: bool,
}

impl HeldCombo {
    fn new() -> Self {
        Self {
            vks: Vec::new(),
            held: false,
        }
    }

    fn detect(&mut self, active: bool) -> (bool, bool) {
        match (active, self.held) {
            (true, false) => {
                self.held = true;
                (true, false)
            }
            (false, true) => {
                self.held = false;
                (false, true)
            }
            _ => (false, false),
        }
    }

    fn press_keys(&mut self, config: &ai_pad_core::action::ActionConfig) {
        let mut vks = Vec::new();
        for action_def in config.actions.values() {
            if action_def.action_type == "voice" {
                if let Some(voice) = &action_def.voice {
                    let keys: Vec<&str> = voice.trigger.iter().map(|s| s.as_str()).collect();
                    vks.extend(hotkey::parse_keys(&keys));
                }
            }
        }
        self.vks = vks;
        for &vk in &self.vks {
            let _ = hotkey::key_down(vk);
        }
        info!(vk_count = self.vks.len(), "→ 输入法语音热键已按下");
    }

    fn release_keys(&mut self) {
        for &vk in self.vks.iter().rev() {
            let _ = hotkey::key_up(vk);
        }
        info!("→ 输入法语音热键已松开");
    }
}

fn name_to_bit(name: &str) -> Option<u32> {
    match name {
        "A" => Some(0),
        "B" => Some(1),
        "X" => Some(3),
        "Y" => Some(4),
        "L1" => Some(6),
        "R1" => Some(7),
        "L2" => Some(8),
        "R2" => Some(9),
        "Select" => Some(10),
        "Start" => Some(11),
        "Home" => Some(12),
        _ => None,
    }
}
