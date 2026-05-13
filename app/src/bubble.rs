//! 气泡窗口模块：流式 AI 文本渲染 + 动态高度调整 + 实时跟随宠物。
//!
//! 核心协议是三段式流式推送：`start_streaming_bubble` → `append_bubble_chunk`×N →
//! `finalize_bubble`。前端通过 `bubble-chunk` / `bubble-end` 事件接收文本，
//! 并在 `bubble-end` 后启动自动隐藏定时器。
//!
//! **动态高度**：气泡窗口默认 120px，前端根据文本量调整 CSS 高度后通过
//! `cmd_reposition_bubble` 通知 Rust 端重新计算窗口尺寸和位置，最大 680px。
//! 超长内容由前端内部滚轮翻阅（Win32 子类转发 `WM_MOUSEWHEEL`）。
//!
//! **follower 机制**：`spawn_bubble_follower` 启动独立线程，50ms 轮询宠物窗口位置，
//! 气泡可见时自动对齐到宠物上方/下方（空间不足时翻边），与手柄循环解耦。
//!
//! **chat 优先级**：`chat_active` 标记阻止截图摘要覆盖正在进行的聊天。
//! 截图线程在发起 Vision API 前检查此标记，避免打断对话。

use std::sync::Mutex;
use tracing::{debug, info};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindowBuilder,
};

#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowExW, PostMessageW, WM_MOUSEWHEEL};

const BUBBLE_SUBCLASS_ID: usize = 100;

const BUBBLE_W: f64 = 260.0;
const BUBBLE_H: f64 = 120.0;
const EDGE_MARGIN_LP: f64 = 12.0;
const PET_GAP_LP: f64 = 8.0;
const ARROW_MARGIN_LP: f64 = 26.0;
const BUBBLE_INSET_X_LP: f64 = 8.0;

/// 整数矩形，用于屏幕坐标下的位置和碰撞计算。
#[derive(Clone, Copy, Debug)]
struct RectI {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl RectI {
    fn right(self) -> i32 {
        self.x + self.w
    }

    fn bottom(self) -> i32 {
        self.y + self.h
    }

    fn inset(self, margin: i32) -> Self {
        Self {
            x: self.x + margin,
            y: self.y + margin,
            w: (self.w - margin * 2).max(1),
            h: (self.h - margin * 2).max(1),
        }
    }

    #[cfg(test)]
    fn intersects(self, other: Self) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }
}

/// 气泡放置结果：位置、高度、箭头偏移、是否在宠物上方。
#[derive(Clone, Copy, Debug)]
struct BubblePlacement {
    x: i32,
    y: i32,
    h: i32,
    arrow_x: f64,
    above_pet: bool,
}

/// 将逻辑像素乘以 DPI 缩放因子，向下取整为整数像素。
fn scaled_px(value: f64, scale: f64) -> i32 {
    (value * scale.max(0.5)).round().max(1.0) as i32
}

/// 将整数限制在 [min, max] 范围内，min > max 时返回 min。
fn clamp_i32(value: i32, min: i32, max: i32) -> i32 {
    if min > max {
        min
    } else {
        value.clamp(min, max)
    }
}

/// 计算气泡窗口在屏幕上的最佳放置位置。
///
/// 综合考虑宠物位置、DPI 缩放、安全边距，优先放在宠物上方，
/// 空间不足时翻到下方，同时计算箭头指示器的水平偏移。
fn compute_bubble_placement(
    monitor: RectI,
    pet: RectI,
    bubble_w: i32,
    bubble_h: i32,
    scale: f64,
) -> BubblePlacement {
    let edge_margin = scaled_px(EDGE_MARGIN_LP, scale);
    let pet_gap = scaled_px(PET_GAP_LP, scale);
    let arrow_margin = scaled_px(ARROW_MARGIN_LP, scale) as f64;
    let safe = monitor.inset(edge_margin);
    let pet_center_x = pet.x + pet.w / 2;

    let min_x = safe.x;
    let max_x = safe.right() - bubble_w;
    let centered_x = pet_center_x - bubble_w / 2;
    let x = clamp_i32(centered_x, min_x, max_x);

    let desired_h = bubble_h.min(safe.h).max(1);
    let space_above = (pet.y - pet_gap - safe.y).max(0);
    let space_below = (safe.bottom() - pet.bottom() - pet_gap).max(0);
    let fits_above = desired_h <= space_above;
    let fits_below = desired_h <= space_below;

    let (above_pet, available_h) = if fits_above || (!fits_below && space_above >= space_below) {
        (true, space_above)
    } else {
        (false, space_below)
    };
    let h = desired_h.min(available_h.max(1));
    let raw_y = if above_pet {
        pet.y - pet_gap - h
    } else {
        pet.bottom() + pet_gap
    };
    let y = clamp_i32(raw_y, safe.y, safe.bottom() - h);

    let arrow_x = (pet_center_x - x) as f64;
    let arrow_x = if bubble_w as f64 > arrow_margin * 2.0 {
        arrow_x.clamp(arrow_margin, bubble_w as f64 - arrow_margin)
    } else {
        bubble_w as f64 / 2.0
    };

    BubblePlacement {
        x,
        y,
        h,
        arrow_x,
        above_pet,
    }
}

/// 气泡共享状态：待消费文本 + chat 模式标记。
///
/// 首次创建窗口时 emit 时机可能早于前端 listen 注册，
/// 因此把文本暂存于 `pending_text`，前端 init 时主动 invoke 拉取。
pub struct SharedBubble {
    pub pending_text: Mutex<Option<String>>,
    /// chat 模式标记：输入框展开或正在流式回复时为 true
    /// 截图线程检查此标记，跳过 show_bubble 避免覆盖
    pub chat_active: Mutex<bool>,
}

impl SharedBubble {
    pub fn new() -> Self {
        Self {
            pending_text: Mutex::new(None),
            chat_active: Mutex::new(false),
        }
    }

    /// 检查是否处于 chat 模式（供截图线程查询）
    pub fn is_chat_active(&self) -> bool {
        self.chat_active.lock().is_ok_and(|g| *g)
    }

    /// 进入 chat 模式（chat 输入 / 流式回复开始）
    pub fn set_chat_active(&self, active: bool) {
        if let Ok(mut g) = self.chat_active.lock() {
            *g = active;
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

#[derive(serde::Serialize, Clone)]
pub struct BubbleToolPayload {
    pub tool_name: String,
    pub label: String,
    pub kind: String,
    pub phase: String,
    pub call_id: Option<String>,
    pub internal_call_id: String,
    pub result_preview: Option<String>,
    pub success: Option<bool>,
}

/// 显示气泡：按需创建窗口、定位到宠物上方、写入待消费文本 + emit。
///
/// 跳过条件：跳舞中或 `chat_active` 为 true 时仅更新 pending 文本不显示。
pub fn show_bubble(app: &AppHandle, text: &str) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();

    if ai_pad_core::dance::is_dancing() {
        debug!(
            text_len = text.chars().count(),
            "bubble skipped while dancing"
        );
        return Ok(());
    }

    // chat 模式优先级：截图摘要不覆盖聊天内容
    if state.is_chat_active() {
        debug!(
            text_len = text.chars().count(),
            "bubble deferred while chat is active"
        );
        // 只更新 pending 文本，不显示/不 emit（等 chat 结束后自然消费）
        *state.pending_text.lock().map_err(|e| e.to_string())? = Some(text.to_string());
        return Ok(());
    }

    // 写入 pending text
    *state.pending_text.lock().map_err(|e| e.to_string())? = Some(text.to_string());

    // 取或创建窗口
    let window = match app.get_webview_window("bubble") {
        Some(w) => {
            debug!("reuse bubble window");
            w
        }
        None => {
            debug!("create bubble window");
            create_bubble_window(app).map_err(|e| e.to_string())?
        }
    };

    // 定位到 pet 上方
    position_above_pet(app, &window);

    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    let _ = window.show();
    debug!("bubble window show called");
    // eval 直接触发 JS 拉取 pending_text：emit_to 对 hide→show 窗口不可靠
    let _ = window.eval("if(window.__bubble_onShow)window.__bubble_onShow();");
    debug!("bubble onShow eval called");
    // emit 兜底：窗口首次创建或未被 hide 过时可能仍有效
    let _ = app.emit_to(
        "bubble",
        "bubble-update",
        BubblePayload {
            text: text.to_string(),
        },
    );
    info!(text_len = text.chars().count(), "bubble shown");

    Ok(())
}

/// 应用启动时预创建气泡窗口（hidden），让 JS 在启动时完成初始化。
///
/// 避免首次流式回复时 emit 事件早于前端 listen 注册的竞态。
/// 同时安装 Win32 子类以转发 `WM_MOUSEWHEEL` 到 WebView2 子窗口。
pub fn precreate_bubble_window(app: &AppHandle) -> Result<(), tauri::Error> {
    if app.get_webview_window("bubble").is_some() {
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "bubble", WebviewUrl::App("bubble.html".into()))
        .title("8Bit Bubble")
        .inner_size(BUBBLE_W, BUBBLE_H)
        .min_inner_size(220.0, 104.0)
        .max_inner_size(420.0, 680.0)
        .decorations(false)
        .transparent(true)
        .background_color(tauri::webview::Color(0, 0, 0, 0))
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(true)
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

/// 流式回复开始：清空 pending、设置 `chat_active`、确保窗口可见并定位。
///
/// 不 emit 事件（WebView2 首次 show 后 JS 可能未加载完），
/// 前端 init 时通过 `cmd_consume_bubble_text` 拉取已有累积文本。
pub fn start_streaming_bubble(app: &AppHandle) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    *state.pending_text.lock().map_err(|e| e.to_string())? = Some(String::new());
    state.set_chat_active(true); // 流式回复中，截图不应覆盖

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

/// 流式追加：累加到 `pending_text`（给晚到的 listener 用）+ emit `bubble-chunk`。
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

/// 发送工具运行时事件。工具状态独立于正文，不写入 pending_text。
pub fn emit_tool_event(app: &AppHandle, payload: BubbleToolPayload) -> Result<(), String> {
    let _ = app.emit_to("bubble", "bubble-tool-event", payload);
    Ok(())
}

/// 流式结束：emit `bubble-end` → 前端启动自动隐藏定时器 + 退出 chat 模式。
pub fn finalize_bubble(app: &AppHandle) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    state.set_chat_active(false); // 回复截图写 bubble
    let _ = app.emit_to("bubble", "bubble-end", ());
    Ok(())
}

/// 启动一个独立的气泡跟随线程：轮询宠物窗口位置变化，
/// 实时把气泡窗口对齐到宠物正上方。与手柄循环彻底解耦，
/// 确保"无手柄"场景下气泡仍能实时跟随。
pub fn spawn_bubble_follower(app: AppHandle) {
    std::thread::spawn(move || {
        let mut prev_pet_pos: Option<(i32, i32)> = None;
        loop {
            if let Some(bubble_win) = app.get_webview_window("bubble") {
                if bubble_win.is_visible().unwrap_or(false) {
                    let pet = app
                        .get_webview_window("pet")
                        .filter(|w| w.is_visible().unwrap_or(false))
                        .or_else(|| {
                            app.get_webview_window("pet-mini")
                                .filter(|w| w.is_visible().unwrap_or(false))
                        })
                        .or_else(|| {
                            app.get_webview_window("pet-snap")
                                .filter(|w| w.is_visible().unwrap_or(false))
                        });
                    if let Some(p) = pet {
                        if let Ok(pos) = p.outer_position() {
                            let key = (pos.x, pos.y);
                            if Some(key) != prev_pet_pos {
                                prev_pet_pos = Some(key);
                                position_above_pet(&app, &bubble_win);
                            }
                        }
                    }
                } else {
                    prev_pet_pos = None;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });
}

/// 将气泡窗口对齐到宠物窗口上方（空间不足时翻到下方）。
///
/// 支持折叠态（`pet-mini`）和吸附态（`pet-snap`）宠物窗口，
/// 计算时考虑 DPI 缩放和屏幕安全边距。
pub fn position_above_pet(app: &AppHandle, bubble: &tauri::WebviewWindow) {
    // 优先查找可见的宠物窗口（支持折叠态 + 吸附态）
    let pet = app
        .get_webview_window("pet")
        .filter(|w| w.is_visible().unwrap_or(false))
        .or_else(|| {
            app.get_webview_window("pet-mini")
                .filter(|w| w.is_visible().unwrap_or(false))
        })
        .or_else(|| {
            app.get_webview_window("pet-snap")
                .filter(|w| w.is_visible().unwrap_or(false))
        });
    let Some(pet) = pet else {
        return;
    };
    let (Ok(pet_pos), Ok(pet_size)) = (pet.outer_position(), pet.outer_size()) else {
        return;
    };

    let Some(monitor) = pet.current_monitor().ok().flatten() else {
        return;
    };
    let monitor_size = monitor.size();
    let monitor_pos = monitor.position();

    let scale = bubble.scale_factor().unwrap_or(1.0);
    let bubble_size = bubble.inner_size().unwrap_or(tauri::PhysicalSize::new(
        (BUBBLE_W * scale) as u32,
        (BUBBLE_H * scale) as u32,
    ));

    let placement = compute_bubble_placement(
        RectI {
            x: monitor_pos.x,
            y: monitor_pos.y,
            w: monitor_size.width as i32,
            h: monitor_size.height as i32,
        },
        RectI {
            x: pet_pos.x,
            y: pet_pos.y,
            w: pet_size.width as i32,
            h: pet_size.height as i32,
        },
        bubble_size.width as i32,
        bubble_size.height as i32,
        scale,
    );

    if placement.h != bubble_size.height as i32 {
        let _ = bubble.set_size(PhysicalSize::new(bubble_size.width, placement.h as u32));
    }
    let _ = bubble.set_position(PhysicalPosition::new(placement.x, placement.y));
    let arrow_side = if placement.above_pet { "bottom" } else { "top" };
    let arrow_css_x = (placement.arrow_x / scale.max(0.5)) - BUBBLE_INSET_X_LP;
    let _ = bubble.eval(&format!(
        "document.documentElement.style.setProperty('--bubble-arrow-x','{}px');\
         document.documentElement.classList.toggle('bubble-arrow-top', {});\
         document.documentElement.classList.toggle('bubble-arrow-bottom', {});",
        arrow_css_x.round(),
        arrow_side == "top",
        arrow_side == "bottom"
    ));
}

/// 按需创建气泡窗口（不可见、置顶、透明），用于首次 show 前的懒初始化。
pub fn create_bubble_window(app: &AppHandle) -> Result<tauri::WebviewWindow, tauri::Error> {
    WebviewWindowBuilder::new(app, "bubble", WebviewUrl::App("bubble.html".into()))
        .title("8Bit Bubble")
        .inner_size(BUBBLE_W, BUBBLE_H)
        .min_inner_size(220.0, 104.0)
        .max_inner_size(420.0, 680.0)
        .decorations(false)
        .transparent(true)
        .background_color(tauri::webview::Color(0, 0, 0, 0))
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(true)
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
    hide_bubble_window(&app)
}

/// 前端调整自身尺寸后调用，重新计算气泡窗口位置和高度。
#[tauri::command]
pub async fn cmd_reposition_bubble(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("bubble") {
        position_above_pet(&app, &w);
    }
    Ok(())
}

/// 隐藏气泡窗口（前端自动隐藏定时器到期时调用）。
pub fn hide_bubble_window(app: &AppHandle) -> Result<(), String> {
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
        // 260x120 keeps the default bubble compact while leaving room for 13px text.
        assert!(BUBBLE_W >= 240.0 && BUBBLE_W <= 320.0);
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
    fn test_tool_payload_serializes() {
        let p = BubbleToolPayload {
            tool_name: "perform_dance".into(),
            label: "编排舞蹈".into(),
            kind: "performance".into(),
            phase: "planned".into(),
            call_id: Some("provider-call".into()),
            internal_call_id: "rig-call".into(),
            result_preview: None,
            success: None,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("perform_dance"));
        assert!(json.contains("performance"));
        assert!(json.contains("planned"));
        assert!(json.contains("rig-call"));
    }

    // ---- chat_active 状态 ----

    #[test]
    fn test_chat_active_default_false() {
        let b = SharedBubble::new();
        assert!(!b.is_chat_active());
    }

    #[test]
    fn test_set_chat_active_true() {
        let b = SharedBubble::new();
        b.set_chat_active(true);
        assert!(b.is_chat_active());
    }

    #[test]
    fn test_set_chat_active_false() {
        let b = SharedBubble::new();
        b.set_chat_active(true);
        b.set_chat_active(false);
        assert!(!b.is_chat_active());
    }

    #[test]
    fn test_chat_active_isolated() {
        let b = SharedBubble::new();
        b.set_chat_active(true);
        // pending_text 不受影响
        *b.pending_text.lock().unwrap() = Some("test".into());
        assert!(b.is_chat_active());
        b.set_chat_active(false);
        assert!(!b.is_chat_active());
        assert_eq!(b.pending_text.lock().unwrap().as_deref(), Some("test"));
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

    #[test]
    fn test_bubble_placement_keeps_negative_monitor_bounds() {
        let placement = compute_bubble_placement(
            RectI {
                x: -1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
            RectI {
                x: -80,
                y: 500,
                w: 128,
                h: 128,
            },
            280,
            140,
            1.0,
        );

        assert!(placement.x <= -12);
        assert!(placement.x >= -1920 + 12);
        assert!(placement.y >= 12);
        assert!(placement.y + 140 <= 1080 - 12);
    }

    #[test]
    fn test_bubble_placement_flips_below_near_top() {
        let placement = compute_bubble_placement(
            RectI {
                x: 0,
                y: 0,
                w: 1536,
                h: 960,
            },
            RectI {
                x: 700,
                y: 20,
                w: 128,
                h: 128,
            },
            280,
            140,
            1.25,
        );

        assert!(!placement.above_pet);
        assert!(placement.y >= 20 + 128);
        assert!(placement.y + 140 <= 960 - 15);
    }

    #[test]
    fn test_bubble_placement_clamps_bottom_for_tall_bubble() {
        let placement = compute_bubble_placement(
            RectI {
                x: 0,
                y: 0,
                w: 1536,
                h: 960,
            },
            RectI {
                x: 700,
                y: 200,
                w: 128,
                h: 128,
            },
            420,
            680,
            1.25,
        );

        assert!(placement.y >= 15);
        let pet = RectI {
            x: 700,
            y: 200,
            w: 128,
            h: 128,
        };
        let bubble = RectI {
            x: placement.x,
            y: placement.y,
            w: 420,
            h: placement.h,
        };

        assert!(placement.h < 680);
        assert!(placement.y >= 15);
        assert!(placement.y + placement.h <= 960 - 15);
        assert!(!bubble.intersects(pet));
    }

    #[test]
    fn test_bubble_placement_avoids_pet_when_both_sides_are_tight() {
        let pet = RectI {
            x: 700,
            y: 400,
            w: 128,
            h: 128,
        };
        let placement = compute_bubble_placement(
            RectI {
                x: 0,
                y: 0,
                w: 1536,
                h: 960,
            },
            pet,
            420,
            680,
            1.25,
        );
        let bubble = RectI {
            x: placement.x,
            y: placement.y,
            w: 420,
            h: placement.h,
        };

        assert!(placement.h < 680);
        assert!(!bubble.intersects(pet));
    }

    #[test]
    fn test_bubble_arrow_tracks_pet_when_window_is_clamped() {
        let placement = compute_bubble_placement(
            RectI {
                x: 0,
                y: 0,
                w: 1536,
                h: 960,
            },
            RectI {
                x: 4,
                y: 500,
                w: 128,
                h: 128,
            },
            280,
            140,
            1.0,
        );

        assert_eq!(placement.x, 12);
        assert!(placement.arrow_x > 26.0);
        assert!(placement.arrow_x < 140.0);
    }

    #[test]
    fn test_bubble_arrow_css_position_accounts_for_scale_and_inset() {
        let arrow_css_x = (150.0 / 1.25_f64.max(0.5)) - BUBBLE_INSET_X_LP;
        assert_eq!(arrow_css_x.round() as i32, 112);
    }

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
