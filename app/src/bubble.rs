use std::sync::Mutex;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindowBuilder,
};

#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowExW, PostMessageW, WM_MOUSEWHEEL};

const BUBBLE_SUBCLASS_ID: usize = 100;

const BUBBLE_W: f64 = 280.0;
const BUBBLE_H: f64 = 140.0;

/// 后端待消费文本：首次创建窗口时 emit 时机早于前端 listen 注册，
/// 因此把文本存这里，前端 init 时主动 invoke 拉一次。
pub struct SharedBubble {
    pub pending_text: Mutex<Option<String>>,
}

impl SharedBubble {
    pub fn new() -> Self {
        Self {
            pending_text: Mutex::new(None),
        }
    }
}

impl Default for SharedBubble {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Serialize, Clone)]
struct BubblePayload {
    text: String,
}

#[derive(serde::Serialize, Clone)]
struct BubbleChunkPayload {
    chunk: String,
}

/// 显示气泡：按需创建窗口、定位到 pet 上方、写入待消费文本 + emit
pub fn show_bubble(app: &AppHandle, text: &str) -> Result<(), String> {
    // 写入 pending text
    let state: State<SharedBubble> = app.state();
    *state.pending_text.lock().map_err(|e| e.to_string())? = Some(text.to_string());

    // 取或创建窗口
    let window = match app.get_webview_window("bubble") {
        Some(w) => w,
        None => create_bubble_window(app).map_err(|e| e.to_string())?,
    };

    // 定位到 pet 上方
    position_above_pet(app, &window);

    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    let _ = window.show();
    // emit 兜底：窗口已存在时 listener 已注册，立刻刷新
    let _ = app.emit_to(
        "bubble",
        "bubble-update",
        BubblePayload {
            text: text.to_string(),
        },
    );

    Ok(())
}

/// 应用启动时预创建气泡窗口(hidden),让 JS 在启动时完成初始化
/// 避免首次流式时 emit 事件早于 listen 注册的竞态
pub fn precreate_bubble_window(app: &AppHandle) -> Result<(), tauri::Error> {
    if app.get_webview_window("bubble").is_some() {
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "bubble", WebviewUrl::App("bubble.html".into()))
        .title("8Bit Bubble")
        .inner_size(BUBBLE_W, BUBBLE_H)
        .decorations(false)
        .transparent(true)
        .background_color(tauri::webview::Color(0, 0, 0, 0))
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .build()?;
    // 运行时再设一次（Windows WebView2 需要）
    if let Some(w) = app.get_webview_window("bubble") {
        let _ = w.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
        // 安装 Win32 子类：转发 WM_MOUSEWHEEL 到 WebView2 子窗口
        // （Tao 的 WndProc 返回 LRESULT(0) 消费了滚轮消息，导致 WebView2 收不到）
        #[cfg(target_os = "windows")]
        {
            if let Ok(hwnd) = w.hwnd() {
                let raw_hwnd = hwnd.0 as windows_sys::Win32::Foundation::HWND;
                let installed = unsafe { install_wheel_subclass(raw_hwnd) };
                if !installed {
                    tracing::warn!("Failed to install wheel subclass on bubble window");
                }
            }
        }
    }
    Ok(())
}

/// 流式开始: 清空 pending、确保窗口显示
/// 注意: 不 emit 事件,因为 WebView2 首次 show() 后 JS 可能还没加载完
/// 前端 init 时通过 cmd_consume_bubble_text 拉取已有累积文本
pub fn start_streaming_bubble(app: &AppHandle) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    *state.pending_text.lock().map_err(|e| e.to_string())? = Some(String::new());

    let window = match app.get_webview_window("bubble") {
        Some(w) => w,
        None => create_bubble_window(app).map_err(|e| e.to_string())?,
    };
    position_above_pet(app, &window);
    // Windows WebView2: builder 的 background_color 可能不够，运行时再设一次确保透明
    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    let _ = window.show();
    Ok(())
}

/// 流式追加: 累加到 pending(给晚到的 listener 用)、emit "bubble-chunk"
pub fn append_bubble_chunk(app: &AppHandle, chunk: &str) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    if let Ok(mut g) = state.pending_text.lock() {
        if let Some(s) = g.as_mut() {
            s.push_str(chunk);
        } else {
            *g = Some(chunk.to_string());
        }
    }
    let _ = app.emit_to(
        "bubble",
        "bubble-chunk",
        BubbleChunkPayload {
            chunk: chunk.to_string(),
        },
    );
    Ok(())
}

/// 流式结束: emit "bubble-end" → 前端启动自动隐藏定时器
pub fn finalize_bubble(app: &AppHandle) -> Result<(), String> {
    let _ = app.emit_to("bubble", "bubble-end", ());
    // 流结束后不再需要 pending 累积,但保留内容供迟到的 invoke 拉取
    Ok(())
}

/// 把 bubble 窗口对齐到 pet 窗口正上方，带屏幕边界检测
pub fn position_above_pet(app: &AppHandle, bubble: &tauri::WebviewWindow) {
    // 优先查找可见的宠物窗口（支持折叠态 + 吸附态）
    let pet = app
        .get_webview_window("pet")
        .filter(|w| w.is_visible().unwrap_or(false))
        .or_else(|| app.get_webview_window("pet-mini").filter(|w| w.is_visible().unwrap_or(false)))
        .or_else(|| app.get_webview_window("pet-snap").filter(|w| w.is_visible().unwrap_or(false)));
    let Some(pet) = pet else { return; };
    let (Ok(pet_pos), Ok(pet_size)) = (pet.outer_position(), pet.outer_size()) else {
        return;
    };

    let Some(monitor) = pet.current_monitor().ok().flatten() else { return; };
    let monitor_size = monitor.size();
    let monitor_pos = monitor.position();

    let scale = bubble.scale_factor().unwrap_or(1.0);
    let bubble_w_px = BUBBLE_W * scale;
    let bubble_h_px = BUBBLE_H * scale;

    // 默认：水平居中于 pet 上方
    let pet_center_x = pet_pos.x as f64 + pet_size.width as f64 / 2.0;
    let pet_top = pet_pos.y as f64;
    let mut bubble_x = (pet_center_x - bubble_w_px / 2.0) as i32;
    let mut bubble_y = (pet_top - bubble_h_px + 6.0 * scale) as i32;

    let screen_left = monitor_pos.x;
    let screen_right = monitor_pos.x + monitor_size.width as i32;
    let screen_top = monitor_pos.y;

    // 水平 clamp：不超出屏幕左右
    bubble_x = bubble_x.clamp(screen_left + 4, screen_right - bubble_w_px as i32 - 4);

    // 上方放不下 → 翻到 pet 下方
    if bubble_y < screen_top + 4 {
        bubble_y = (pet_top + pet_size.height as f64 + 6.0 * scale) as i32;
    }

    let _ = bubble.set_position(PhysicalPosition::new(bubble_x, bubble_y));
}

pub fn create_bubble_window(app: &AppHandle) -> Result<tauri::WebviewWindow, tauri::Error> {
    WebviewWindowBuilder::new(app, "bubble", WebviewUrl::App("bubble.html".into()))
        .title("8Bit Bubble")
        .inner_size(BUBBLE_W, BUBBLE_H)
        .decorations(false)
        .transparent(true)
        .background_color(tauri::webview::Color(0, 0, 0, 0))
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .build()
}

// ---- Win32 WM_MOUSEWHEEL 转发辅助 ----

/// 构建 WM_MOUSEWHEEL 的 wParam: HIWORD=signed delta, LOWORD=key flags
#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn build_wheel_wparam(delta: i32, key_flags: u16) -> usize {
    ((delta as i16 as u16) as usize) << 16 | (key_flags as usize)
}

/// 将屏幕坐标 (x, y) 打包为 LPARAM (MAKELPARAM 等价)
#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn build_lparam_from_point(x: i32, y: i32) -> isize {
    ((y as isize) << 16) | ((x as isize) & 0xFFFF)
}

// ---- Win32 Subclass 滚轮转发 ----

/// Win32 子类回调：拦截 WM_MOUSEWHEEL 并转发到 WebView2 子窗口。
/// 安装在 Tao 的 subclass 之后（ID=100），LIFO 链中先于 Tao 执行。
#[cfg(target_os = "windows")]
unsafe extern "system" fn bubble_wheel_subclass_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    umsg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
    _uidsubclass: usize,
    _dwrefdata: usize,
) -> windows_sys::Win32::Foundation::LRESULT {
    if umsg == WM_MOUSEWHEEL {
        let webview = FindWindowExW(
            hwnd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        if !webview.is_null() {
            let _ = PostMessageW(webview, WM_MOUSEWHEEL, wparam, lparam);
        }
    }
    DefSubclassProc(hwnd, umsg, wparam, lparam)
}

/// 在 bubble 窗口 HWND 上安装滚轮转发子类。
/// 必须在窗口创建后调用（WebView2 子窗口已存在）。
#[cfg(target_os = "windows")]
unsafe fn install_wheel_subclass(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    SetWindowSubclass(
        hwnd,
        Some(bubble_wheel_subclass_proc),
        BUBBLE_SUBCLASS_ID,
        0,
    ) != 0
}

/// 前端 init 时调用：读取当前累积文本（不清空）
#[tauri::command]
pub async fn cmd_consume_bubble_text(
    state: State<'_, SharedBubble>,
) -> Result<Option<String>, String> {
    let t = state.pending_text.lock().map_err(|e| e.to_string())?;
    Ok(t.clone())
}

/// 前端自动隐藏定时到了 → 调用此 cmd 隐藏窗口
#[tauri::command]
pub async fn cmd_hide_bubble(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("bubble") {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_bubble_default_empty() {
        let b = SharedBubble::new();
        let g = b.pending_text.lock().unwrap();
        assert!(g.is_none());
    }

    #[test]
    fn test_shared_bubble_take_clears() {
        let b = SharedBubble::new();
        *b.pending_text.lock().unwrap() = Some("hello".into());
        let taken = b.pending_text.lock().unwrap().take();
        assert_eq!(taken, Some("hello".to_string()));
        // 二次取应为 None
        let again = b.pending_text.lock().unwrap().take();
        assert_eq!(again, None);
    }

    #[test]
    fn test_bubble_constants_reasonable() {
        // 280x140 适合阅读 4-6 行 13px 字体
        assert!(BUBBLE_W >= 240.0 && BUBBLE_W <= 360.0);
        assert!(BUBBLE_H >= 100.0 && BUBBLE_H <= 200.0);
    }

    #[test]
    fn test_payload_serializes() {
        let p = BubblePayload {
            text: "你好".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("你好") || json.contains("\\u"));
    }

    #[test]
    fn test_chunk_payload_serializes() {
        let p = BubbleChunkPayload {
            chunk: "片段".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("chunk"));
        assert!(json.contains("片段") || json.contains("\\u"));
    }

    #[test]
    fn test_pending_accumulates() {
        let b = SharedBubble::new();
        *b.pending_text.lock().unwrap() = Some(String::new());
        if let Some(s) = b.pending_text.lock().unwrap().as_mut() {
            s.push_str("Hello");
        }
        if let Some(s) = b.pending_text.lock().unwrap().as_mut() {
            s.push_str(" World");
        }
        let taken = b.pending_text.lock().unwrap().take();
        assert_eq!(taken, Some("Hello World".to_string()));
    }

    // ---- Cycle 1: WM_MOUSEWHEEL 参数构建 ----

    #[cfg(target_os = "windows")]
    #[test]
    fn test_build_wheel_wparam_positive_delta() {
        let w = build_wheel_wparam(120, 0);
        assert_eq!((w >> 16) as i16, 120);
        assert_eq!(w & 0xFFFF, 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_build_wheel_wparam_negative_delta() {
        let w = build_wheel_wparam(-120, 0);
        assert_eq!((w >> 16) as i16, -120);
        assert_eq!(w & 0xFFFF, 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_build_wheel_wparam_with_keys() {
        let w = build_wheel_wparam(240, 0x0004); // MK_SHIFT
        assert_eq!((w >> 16) as i16, 240);
        assert_eq!(w & 0xFFFF, 0x0004);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_build_lparam_from_point() {
        let lp = build_lparam_from_point(100, 200);
        assert_eq!((lp & 0xFFFF) as i16, 100);
        assert_eq!(((lp >> 16) & 0xFFFF) as i16, 200);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_build_lparam_negative_coords() {
        let lp = build_lparam_from_point(-50, -100);
        assert_eq!((lp & 0xFFFF) as i16, -50);
        assert_eq!(((lp >> 16) & 0xFFFF) as i16, -100);
    }

    // ---- Cycle 2: Subclass 类型编译检查 ----

    #[cfg(target_os = "windows")]
    #[test]
    fn test_subclass_proc_type_matches() {
        fn _assert(
            _f: unsafe extern "system" fn(
                windows_sys::Win32::Foundation::HWND,
                u32,
                windows_sys::Win32::Foundation::WPARAM,
                windows_sys::Win32::Foundation::LPARAM,
                usize,
                usize,
            ) -> windows_sys::Win32::Foundation::LRESULT,
        ) {
        }
        _assert(bubble_wheel_subclass_proc);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_install_wheel_subclass_signature() {
        let _: unsafe fn(windows_sys::Win32::Foundation::HWND) -> bool = install_wheel_subclass;
    }
}
