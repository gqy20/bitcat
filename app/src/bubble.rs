//! 气泡窗口模块：流式 AI 文本渲染、动态高度调整与普通回应跟随宠物。
//!
//! 核心协议是三段式流式推送：`start_streaming_bubble` → `append_bubble_chunk`×N →
//! `finalize_bubble`。`bubble-start` 通知前端进入流式状态，轮询
//! `cmd_consume_bubble_text` 读取累积文本，`bubble-end` 携带 `{ text }` 最终快照通知生成结束。
//! 新窗口可能错过开始事件，初次拉取待消费文本作为兜底；空正文不代表正在生成。
//! 用户主动聊天或阅读时继续保护会话。
//! 用户请求的静态说明通过 `show_chat_message` 复用完整回复协议，普通通知继续避让会话。
//!
//! **动态高度**：气泡窗口默认 120px，前端根据文本量调整 CSS 高度后通过
//! `cmd_reposition_bubble` 通知 Rust 端重新计算窗口尺寸和位置，最大 680px。
//! 超长内容由前端内部滚轮翻阅（Win32 子类转发 `WM_MOUSEWHEEL`）。
//!
//! **follower 机制**：`spawn_bubble_follower` 启动独立线程，50ms 轮询宠物窗口位置，
//! 普通回应可见时自动对齐到宠物上方/下方（空间不足时翻边），与手柄循环解耦。
//! 主动聊天或阅读期间暂停持续跟随，打开会话与显式尺寸调整仍执行定位和屏幕边界修正。
//!
//! **chat 优先级**：生成中与用户聊天/阅读使用独立标记，任一有效时均避让后台观察。
//! 截图线程在发起 Vision API 前查询共享状态，普通通知也不会覆盖流式正文。

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

// 子类化只用于 Windows 上的滚轮转发；非 Windows 编译时保留定义不报警。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const BUBBLE_SUBCLASS_ID: usize = 100;

const BUBBLE_W: f64 = 300.0;
const BUBBLE_H: f64 = 120.0;
const EDGE_MARGIN_LP: f64 = 12.0;
const PET_GAP_LP: f64 = 4.0;
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

/// 对话保护状态，生成结束与用户收起会话各自释放对应的标记。
#[derive(Default)]
struct ChatActivity {
    generating: bool,
    interacting: bool,
}

impl ChatActivity {
    fn is_active(&self) -> bool {
        self.generating || self.interacting
    }
}

/// 气泡共享状态：待消费文本与独立的生成、交互保护标记。
///
/// 首次创建窗口时 emit 时机可能早于前端 listen 注册，
/// 因此把文本暂存于 `pending_text`，前端 init 时主动 invoke 拉取。
pub struct SharedBubble {
    /// 最近的完整累积正文；轮询读取保留内容，下一轮生成开始时重置。
    pub pending_text: Mutex<Option<String>>,
    activity: Mutex<ChatActivity>,
}

impl SharedBubble {
    /// 创建空闲状态，等待普通通知或新的用户会话。
    pub fn new() -> Self {
        Self {
            pending_text: Mutex::new(None),
            activity: Mutex::new(ChatActivity::default()),
        }
    }

    /// 生成中或用户正在聊天/阅读时避让观察；状态异常时也保持保护。
    pub fn is_chat_active(&self) -> bool {
        self.activity.lock().map_or(true, |g| g.is_active())
    }

    /// 用户正在聊天/阅读时保持窗口位置；状态异常时也暂停持续跟随。
    pub fn is_interacting(&self) -> bool {
        self.activity.lock().map_or(true, |g| g.interacting)
    }

    /// 更新 AI 生成状态，不改变用户聊天或阅读的保护。
    pub fn set_generation_active(&self, active: bool) {
        if let Ok(mut activity) = self.activity.lock() {
            activity.generating = active;
        }
    }

    /// 更新用户交互状态，收起会话时仍保留进行中的生成保护。
    pub fn set_interaction_active(&self, active: bool) {
        if let Ok(mut activity) = self.activity.lock() {
            activity.interacting = active;
        }
    }

    /// 新回复开始时清空旧正文，并在同一保护区内标记生成开始。
    fn begin_stream(&self) -> Result<(), String> {
        let mut activity = self.activity.lock().map_err(|e| e.to_string())?;
        let mut pending = self.pending_text.lock().map_err(|e| e.to_string())?;
        activity.generating = true;
        *pending = Some(String::new());
        Ok(())
    }

    /// 显式回复写入累积正文，用户聊天/阅读保护不会阻止本轮请求的反馈。
    fn append_stream_text(&self, chunk: &str) -> Result<(), String> {
        self.pending_text
            .lock()
            .map_err(|e| e.to_string())?
            .get_or_insert_with(String::new)
            .push_str(chunk);
        Ok(())
    }

    /// 释放生成保护前取得不可变的最终正文，后续通知不会污染回复结束事件。
    fn finish_stream(&self) -> Result<String, String> {
        let mut activity = self.activity.lock().map_err(|e| e.to_string())?;
        let text = self
            .pending_text
            .lock()
            .map_err(|e| e.to_string())
            .map(|pending| pending.clone().unwrap_or_default());
        activity.generating = false;
        text
    }

    /// 普通通知仅在会话空闲时写入，避免在途观察覆盖流式正文或阅读内容。
    fn set_notice_text(&self, text: &str) -> Result<bool, String> {
        let activity = self.activity.lock().map_err(|e| e.to_string())?;
        if activity.is_active() {
            return Ok(false);
        }
        *self.pending_text.lock().map_err(|e| e.to_string())? = Some(text.to_string());
        Ok(true)
    }
}

impl Default for SharedBubble {
    fn default() -> Self {
        Self::new()
    }
}

/// 回复结束时的正文快照；空串也是明确的最终结果。
#[derive(serde::Serialize, Clone)]
pub struct BubbleEndPayload {
    pub text: String,
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
    pub elapsed_ms: Option<u64>,
}

#[derive(serde::Serialize, Clone)]
pub struct AgentToastPayload {
    pub title: String,
    pub context: String,
    pub detail: String,
    pub tone: String,
}

/// 显示气泡：按需创建窗口、定位到宠物上方，并写入待消费文本。
///
/// 跳舞、生成或用户聊天/阅读期间跳过，不改写当前正文。
pub fn show_bubble(app: &AppHandle, text: &str) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();

    if bitcat_core::performance::is_performing() {
        debug!(
            text_len = text.chars().count(),
            "bubble skipped while performance is active"
        );
        return Ok(());
    }

    // chat 模式优先级：截图摘要不覆盖聊天内容
    if !state.set_notice_text(text)? {
        debug!(
            text_len = text.chars().count(),
            "bubble skipped while chat is active"
        );
        return Ok(());
    }

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

    let scale = window.scale_factor().unwrap_or(1.0).max(0.5);
    let _ = window.set_size(PhysicalSize::new(
        (BUBBLE_W * scale).round() as u32,
        (BUBBLE_H * scale).round() as u32,
    ));

    // 定位到 pet 上方
    position_above_pet(app, &window);

    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    let _ = window.show();
    debug!("bubble window show called");
    // eval 直接触发 JS 拉取 pending_text；若前端尚未加载，init 时也会主动拉取。
    let _ = window.eval("if(window.__bubble_onShow)window.__bubble_onShow();");
    debug!("bubble onShow eval called");
    info!(text_len = text.chars().count(), "bubble shown");

    Ok(())
}

pub fn show_agent_toast(app: &AppHandle, payload: AgentToastPayload) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    if state.is_chat_active() {
        debug!("agent toast skipped while chat is active");
        return Ok(());
    }

    let window = match app.get_webview_window("bubble") {
        Some(w) => w,
        None => create_bubble_window(app).map_err(|e| e.to_string())?,
    };

    let scale = window.scale_factor().unwrap_or(1.0).max(0.5);
    let _ = window.set_size(PhysicalSize::new(
        (BUBBLE_W * scale).round() as u32,
        (72.0 * scale).round() as u32,
    ));
    position_above_pet(app, &window);
    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    let _ = window.show();

    let json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    let _ = window.eval(format!(
        "if(window.__bubble_showAgentToast)window.__bubble_showAgentToast({json});"
    ));
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
        .title("BitCat Bubble")
        .inner_size(BUBBLE_W, BUBBLE_H)
        .min_inner_size(220.0, 104.0)
        .max_inner_size(480.0, 680.0)
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

/// 流式回复开始：清空 pending、标记生成开始、显示窗口并发送 `bubble-start`。
///
/// 新窗口的 WebView2 可能尚未注册监听，前端初次拉取已有累积文本作为兜底。
/// 已有窗口使用开始事件重新启动轮询，覆盖语音等未经过输入框提交的对话。
pub fn start_streaming_bubble(app: &AppHandle) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    state.begin_stream()?;

    let window = match app.get_webview_window("bubble") {
        Some(w) => w,
        None => create_bubble_window(app).map_err(|e| e.to_string())?,
    };
    position_above_pet(app, &window);
    // Windows WebView2: builder 的 background_color 可能不够，运行时再设一次确保透明
    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    let _ = window.show();
    let _ = app.emit_to("bubble", "bubble-start", ());
    Ok(())
}

/// 流式追加：累加到 `pending_text`，前端轮询读取完整累积文本。
pub fn append_bubble_chunk(app: &AppHandle, chunk: &str) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    state.append_stream_text(chunk)
}

/// 显示用户请求的静态反馈，复用开始、累积正文、结束的完整回复协议。
///
/// 与普通通知不同，它可以回应已经打开的会话；启动或写入失败时也尝试结束生成。
pub fn show_chat_message(app: &AppHandle, text: &str) -> Result<(), String> {
    let result = start_streaming_bubble(app).and_then(|_| append_bubble_chunk(app, text));
    let finished = finalize_bubble(app);
    result.and(finished)
}

/// 发送工具运行时事件。工具状态独立于正文，不写入 pending_text。
pub fn emit_tool_event(app: &AppHandle, payload: BubbleToolPayload) -> Result<(), String> {
    let _ = app.emit_to("bubble", "bubble-tool-event", payload);
    Ok(())
}

/// 流式结束：先取得正文快照再释放生成保护，保留用户聊天/阅读的保护。
///
/// `bubble-end` 携带最终 `{ text }`，避免收起会话后的普通通知覆盖前端待读取的回复。
pub fn finalize_bubble(app: &AppHandle) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    let text = state.finish_stream();
    // 即使正文读取失败也通知前端结束等待，原始错误由调用方写入诊断日志。
    let ended = app
        .emit_to(
            "bubble",
            "bubble-end",
            BubbleEndPayload {
                text: text.as_ref().cloned().unwrap_or_default(),
            },
        )
        .map_err(|e| e.to_string());
    text.map(|_| ()).and(ended)
}

/// 启动独立的气泡跟随线程，普通回应随宠物移动，主动聊天与阅读时保持位置。
///
/// 与手柄循环解耦，无手柄时仍能跟随；结束交互后按宠物的最新位置恢复定位。
/// 打开会话与显式尺寸调整调用的定位不受此跟随暂停影响。
pub fn spawn_bubble_follower(app: AppHandle) {
    std::thread::spawn(move || {
        let mut prev_pet_pos: Option<(i32, i32)> = None;
        loop {
            if crate::shutdown::is_requested() {
                tracing::debug!("[bubble-follower] shutdown requested, exiting");
                break;
            }
            if let Some(bubble_win) = app.get_webview_window("bubble") {
                let state: State<SharedBubble> = app.state();
                if bubble_win.is_visible().unwrap_or(false) && !state.is_interacting() {
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
    let _ = bubble.eval(format!(
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
        .title("BitCat Bubble")
        .inner_size(BUBBLE_W, BUBBLE_H)
        .min_inner_size(220.0, 104.0)
        .max_inner_size(480.0, 680.0)
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

/// 前端收起或普通通知自动隐藏时调用，结束交互并隐藏窗口。
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

/// 隐藏气泡窗口并结束用户交互，生成中的回复继续避让后台观察。
pub fn hide_bubble_window(app: &AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("bubble") {
        w.hide().map_err(|e| e.to_string())?;
    }
    let state: State<SharedBubble> = app.state();
    state.set_interaction_active(false);
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
    // 常量范围哨兵：编译期常量的断言在 clippy 看来恒为真，但这里要的
    // 正是“有人把尺寸改到不合理区间就让测试失败”。
    #[allow(clippy::assertions_on_constants)]
    fn test_bubble_constants_reasonable() {
        // 300x120 keeps the default bubble compact while leaving room for Chinese text.
        assert!(BUBBLE_W >= 240.0 && BUBBLE_W <= 320.0);
        assert!(BUBBLE_H >= 100.0 && BUBBLE_H <= 200.0);
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
            elapsed_ms: None,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("perform_dance"));
        assert!(json.contains("performance"));
        assert!(json.contains("planned"));
        assert!(json.contains("rig-call"));
    }

    #[test]
    fn test_agent_toast_payload_serializes() {
        let p = AgentToastPayload {
            title: "data 正在等你".into(),
            context: "Claude Code".into(),
            detail: "需要确认下一步".into(),
            tone: "needs_user".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("needs_user"));
        assert!(json.contains("Claude Code"));
        assert!(json.contains("data"));
    }

    // ---- 生成与用户交互的独立保护 ----

    #[test]
    fn test_chat_active_default_false() {
        let b = SharedBubble::new();
        assert!(!b.is_chat_active());
        assert!(!b.is_interacting());
    }

    #[test]
    fn test_generation_completion_preserves_reading_and_reply() {
        let b = SharedBubble::new();
        b.set_interaction_active(true);
        b.begin_stream().unwrap();
        *b.pending_text.lock().unwrap() = Some("正在阅读的回复".into());

        // 生成完成或停止只释放生成保护，阅读中的正文仍不能被观察结果覆盖。
        b.set_generation_active(false);
        assert!(b.is_chat_active());
        assert!(b.is_interacting());
        assert!(!b.set_notice_text("后台屏幕观察").unwrap());
        assert_eq!(
            b.pending_text.lock().unwrap().as_deref(),
            Some("正在阅读的回复")
        );

        b.set_interaction_active(false);
        assert!(!b.is_chat_active());
        assert!(!b.is_interacting());
        assert!(b.set_notice_text("新的轻提示").unwrap());
    }

    #[test]
    fn test_collapsing_chat_preserves_in_progress_generation() {
        let b = SharedBubble::new();
        b.set_interaction_active(true);
        b.begin_stream().unwrap();
        *b.pending_text.lock().unwrap() = Some("尚未说完".into());

        // 隐藏窗口或退出输入不代表当前请求已经结束。
        b.set_interaction_active(false);
        assert!(b.is_chat_active());
        assert!(!b.is_interacting());
        assert!(!b.set_notice_text("后台摄像头观察").unwrap());
        assert_eq!(b.pending_text.lock().unwrap().as_deref(), Some("尚未说完"));

        b.set_generation_active(false);
        assert!(!b.is_chat_active());
        assert_eq!(b.pending_text.lock().unwrap().as_deref(), Some("尚未说完"));
    }

    #[test]
    fn test_new_reply_replaces_idle_notice_and_blocks_new_notice() {
        let b = SharedBubble::new();
        assert!(b.set_notice_text("之前的轻提示").unwrap());
        b.begin_stream().unwrap();
        assert!(b.is_chat_active());
        assert!(!b.is_interacting());
        assert_eq!(b.pending_text.lock().unwrap().as_deref(), Some(""));
        assert!(!b.set_notice_text("刚返回的屏幕观察").unwrap());
        assert_eq!(b.pending_text.lock().unwrap().as_deref(), Some(""));
    }

    #[test]
    fn test_static_chat_feedback_completes_with_reading_protected() {
        let b = SharedBubble::new();
        b.set_interaction_active(true);
        assert!(!b.set_notice_text("后台观察").unwrap());

        b.begin_stream().unwrap();
        b.append_stream_text("对话暂时不可用，请检查设置后再试。")
            .unwrap();
        b.set_generation_active(false);

        assert!(!b.activity.lock().unwrap().generating);
        assert!(b.is_interacting());
        assert!(!b.set_notice_text("后返回的后台观察").unwrap());
        assert_eq!(
            b.pending_text.lock().unwrap().as_deref(),
            Some("对话暂时不可用，请检查设置后再试。")
        );
    }

    #[test]
    fn test_final_reply_snapshot_survives_notice_after_collapse() {
        let b = SharedBubble::new();
        b.set_interaction_active(true);
        b.begin_stream().unwrap();
        b.append_stream_text("收起时仍在生成的最终回复").unwrap();
        b.set_interaction_active(false);

        let final_text = b.finish_stream().unwrap();
        assert!(!b.is_chat_active());
        assert!(b.set_notice_text("后到的普通通知").unwrap());

        assert_eq!(final_text, "收起时仍在生成的最终回复");
        assert_eq!(
            b.pending_text.lock().unwrap().as_deref(),
            Some("后到的普通通知")
        );
    }

    #[test]
    fn test_pending_accumulates() {
        let b = SharedBubble::new();
        b.begin_stream().unwrap();
        b.append_stream_text("Hello").unwrap();
        b.append_stream_text(" World").unwrap();
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
