//! 气泡窗口模块：流式 AI 文本渲染、动态高度调整与普通回应跟随宠物。
//!
//! 核心协议是三段式流式推送：`start_streaming_bubble` → `append_bubble_chunk`×N →
//! `finalize_bubble`。开始、工具、结束事件携带请求编号，前端用
//! `cmd_get_bubble_snapshot` 读取对应的正文；普通通知继续使用独立的 consume 接口。
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

/// 用户请求来源，随请求排队和执行保持不变。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatSource {
    /// 宠物聊天输入框。
    Text,
    /// 语音识别得到的真实文本。
    Voice,
    /// 手柄或对应按键发起的对话。
    Gamepad,
}

/// 已接受的请求元数据，同时用于排队和开始事件。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatRequest {
    /// 提交时预留的单调编号，停止和事件匹配使用同一个值。
    pub request_id: u64,
    /// 本次请求原文，不包含注入的记忆或提示词。
    pub user_text: String,
    /// 请求入口，用于恢复语音、文字和手柄的正确问题。
    pub source: ChatSource,
}

/// 提交成功确认；排队消息尚未开始生成。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ChatSubmission {
    /// 本次已接受请求的编号。
    pub request_id: u64,
}

/// 停止操作覆盖的请求范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ChatCancelled {
    /// 已取消的编号上界，包含该编号；更晚提交的请求继续执行。
    pub request_id: u64,
}

/// 冷窗口恢复与轮询读取的请求快照，普通通知不会改写它。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BubbleSnapshot {
    /// 当前或最近完成的请求编号；从未开始请求时为空。
    pub request_id: Option<u64>,
    /// 对应请求的原文；没有请求时为空。
    pub user_text: Option<String>,
    /// 对应请求的来源；没有请求时为空。
    pub source: Option<ChatSource>,
    /// 累积正文，空串也是明确的最终结果。
    pub text: String,
    /// 仅表示这个请求仍在生成，后台收尾不占用此标记。
    pub streaming: bool,
}

/// 对话保护与正文所有权，旧请求不能追加文本或释放新请求的保护。
#[derive(Default)]
struct ChatActivity {
    generating: bool,
    interacting: bool,
    request: Option<ChatRequest>,
    text: String,
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
    /// 普通通知文本，保留旧 consume 接口；与对话正文分别保存。
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

    /// 仅释放匹配请求的生成保护，避免旧守卫影响下一轮。
    fn release_generation(&self, request_id: u64) {
        if let Ok(mut activity) = self.activity.lock() {
            if activity
                .request
                .as_ref()
                .is_some_and(|r| r.request_id == request_id)
            {
                activity.generating = false;
            }
        }
    }

    /// 更新用户交互状态，收起会话时仍保留进行中的生成保护。
    pub fn set_interaction_active(&self, active: bool) {
        if let Ok(mut activity) = self.activity.lock() {
            activity.interacting = active;
        }
    }

    /// 新回复开始时清空旧正文，并在同一保护区内标记生成开始。
    fn begin_stream(&self, request: &ChatRequest) -> Result<(), String> {
        let mut activity = self.activity.lock().map_err(|e| e.to_string())?;
        let mut pending = self.pending_text.lock().map_err(|e| e.to_string())?;
        if activity.generating {
            return Err("已有对话正在回复".into());
        }
        activity.generating = true;
        activity.request = Some(request.clone());
        activity.text.clear();
        *pending = None;
        Ok(())
    }

    /// 显式回复写入累积正文，用户聊天/阅读保护不会阻止本轮请求的反馈。
    fn append_stream_text(&self, request_id: u64, chunk: &str) -> Result<(), String> {
        let mut activity = self.activity.lock().map_err(|e| e.to_string())?;
        if !activity.generating
            || activity.request.as_ref().map(|r| r.request_id) != Some(request_id)
        {
            return Err("回复编号已经过期".into());
        }
        activity.text.push_str(chunk);
        Ok(())
    }

    /// 释放生成保护前取得不可变的最终正文，后续通知不会污染回复结束事件。
    fn finish_stream(&self, request_id: u64) -> Result<Option<BubbleEndPayload>, String> {
        let mut activity = self.activity.lock().map_err(|e| e.to_string())?;
        if !activity.generating
            || activity.request.as_ref().map(|r| r.request_id) != Some(request_id)
        {
            return Ok(None);
        }
        let final_text = BubbleEndPayload {
            request_id,
            text: activity.text.clone(),
        };
        activity.generating = false;
        Ok(Some(final_text))
    }

    /// 原子读取请求元数据、正文和状态，避免把旧轮询结果当成新回复。
    fn snapshot(&self) -> Result<BubbleSnapshot, String> {
        let activity = self.activity.lock().map_err(|e| e.to_string())?;
        Ok(BubbleSnapshot {
            request_id: activity.request.as_ref().map(|r| r.request_id),
            user_text: activity.request.as_ref().map(|r| r.user_text.clone()),
            source: activity.request.as_ref().map(|r| r.source),
            text: activity.text.clone(),
            streaming: activity.generating,
        })
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
#[derive(Debug, PartialEq, Eq, serde::Serialize, Clone)]
pub struct BubbleEndPayload {
    /// 这条最终正文所属的请求。
    pub request_id: u64,
    /// 完整最终正文；空串不能回退到上一轮。
    pub text: String,
}

#[derive(serde::Serialize, Clone)]
pub struct BubbleToolPayload {
    /// 工具事件所属的请求，前端据此过滤迟到的状态。
    pub request_id: u64,
    /// 工具 schema 名称。
    pub tool_name: String,
    /// 工具的人类可读名称。
    pub label: String,
    /// 工具类别。
    pub kind: String,
    /// planned、blocked、finished 或 failed。
    pub phase: String,
    /// 模型服务提供的调用编号。
    pub call_id: Option<String>,
    /// rig 在当前调用内生成的编号。
    pub internal_call_id: String,
    /// 供诊断使用的简短结果。
    pub result_preview: Option<String>,
    /// 执行是否成功；准备阶段为空。
    pub success: Option<bool>,
    /// 调用耗时，单位为毫秒。
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
pub fn start_streaming_bubble(app: &AppHandle, request: &ChatRequest) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    state.begin_stream(request)?;

    let window = match app.get_webview_window("bubble") {
        Some(w) => w,
        None => create_bubble_window(app).map_err(|e| e.to_string())?,
    };
    position_above_pet(app, &window);
    // Windows WebView2: builder 的 background_color 可能不够，运行时再设一次确保透明
    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    let _ = window.show();
    let _ = app.emit_to("bubble", "bubble-start", request.clone());
    Ok(())
}

/// 流式追加：写入对应请求的正文，前端通过独立快照读取累计文本。
pub fn append_bubble_chunk(app: &AppHandle, request_id: u64, chunk: &str) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    state.append_stream_text(request_id, chunk)
}

/// 显示用户请求的静态反馈，复用开始、累积正文、结束的完整回复协议。
///
/// 与普通通知不同，它可以回应已经打开的会话；启动或写入失败时也尝试结束生成。
pub fn show_chat_message(app: &AppHandle, request: &ChatRequest, text: &str) -> Result<(), String> {
    let result = start_streaming_bubble(app, request)
        .and_then(|_| append_bubble_chunk(app, request.request_id, text));
    let finished = finalize_bubble(app, request.request_id);
    result.and(finished)
}

/// 发送工具运行时事件。工具状态独立于正文，不写入 pending_text。
pub fn emit_tool_event(app: &AppHandle, payload: BubbleToolPayload) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    let snapshot = state.snapshot()?;
    if snapshot.request_id != Some(payload.request_id) || !snapshot.streaming {
        return Ok(());
    }
    let _ = app.emit_to("bubble", "bubble-tool-event", payload);
    Ok(())
}

/// 流式结束：先取得正文快照再释放生成保护，保留用户聊天/阅读的保护。
///
/// `bubble-end` 携带请求编号与最终正文，避免旧请求或普通通知覆盖当前回复。
pub fn finalize_bubble(app: &AppHandle, request_id: u64) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    match state.finish_stream(request_id) {
        Ok(Some(payload)) => app
            .emit_to("bubble", "bubble-end", payload)
            .map_err(|e| e.to_string()),
        Ok(None) => Ok(()),
        Err(error) => {
            state.release_generation(request_id);
            let _ = app.emit_to(
                "bubble",
                "bubble-end",
                BubbleEndPayload {
                    request_id,
                    text: String::new(),
                },
            );
            Err(error)
        }
    }
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

/// 读取当前请求的原文、来源、正文和生成状态，普通通知走旧 consume 接口。
#[tauri::command]
pub async fn cmd_get_bubble_snapshot(
    state: State<'_, SharedBubble>,
) -> Result<BubbleSnapshot, String> {
    state.snapshot()
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
            request_id: 1,
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

    // ---- 请求所有权、交互保护与独立通知 ----

    fn chat_request(request_id: u64, user_text: &str, source: ChatSource) -> ChatRequest {
        ChatRequest {
            request_id,
            user_text: user_text.into(),
            source,
        }
    }

    #[test]
    fn test_chat_active_default_false() {
        let b = SharedBubble::new();
        assert!(!b.is_chat_active());
        assert!(!b.is_interacting());
        assert_eq!(
            b.snapshot().unwrap(),
            BubbleSnapshot {
                request_id: None,
                user_text: None,
                source: None,
                text: String::new(),
                streaming: false,
            }
        );
    }

    #[test]
    fn test_generation_completion_preserves_reading_and_reply() {
        let b = SharedBubble::new();
        b.set_interaction_active(true);
        b.begin_stream(&chat_request(1, "请回答", ChatSource::Text))
            .unwrap();
        b.append_stream_text(1, "正在阅读的回复").unwrap();
        let end = b.finish_stream(1).unwrap().unwrap();
        assert!(b.is_chat_active());
        assert!(b.is_interacting());
        assert!(!b.set_notice_text("后台屏幕观察").unwrap());
        assert_eq!(
            end,
            BubbleEndPayload {
                request_id: 1,
                text: "正在阅读的回复".into()
            }
        );
        assert_eq!(b.snapshot().unwrap().text, end.text);
        b.set_interaction_active(false);
        assert!(!b.is_chat_active());
        assert!(b.set_notice_text("新的轻提示").unwrap());
        assert_eq!(b.snapshot().unwrap().text, "正在阅读的回复");
    }

    #[test]
    fn test_collapsing_chat_preserves_in_progress_generation() {
        let b = SharedBubble::new();
        b.set_interaction_active(true);
        b.begin_stream(&chat_request(1, "请回答", ChatSource::Text))
            .unwrap();
        b.append_stream_text(1, "尚未说完").unwrap();
        b.set_interaction_active(false);
        assert!(b.is_chat_active());
        assert!(!b.is_interacting());
        assert!(!b.set_notice_text("后台摄像头观察").unwrap());
        assert_eq!(b.snapshot().unwrap().text, "尚未说完");
        b.finish_stream(1).unwrap();
        assert!(!b.is_chat_active());
        assert_eq!(b.snapshot().unwrap().text, "尚未说完");
    }

    #[test]
    fn test_new_reply_clears_idle_notice_and_keeps_true_voice_question() {
        let b = SharedBubble::new();
        assert!(b.set_notice_text("之前的轻提示").unwrap());
        b.begin_stream(&chat_request(7, "这是语音识别的原话", ChatSource::Voice))
            .unwrap();
        assert!(b.is_chat_active());
        assert!(!b.is_interacting());
        assert_eq!(b.pending_text.lock().unwrap().as_deref(), None);
        assert!(!b.set_notice_text("刚返回的屏幕观察").unwrap());
        assert_eq!(
            b.snapshot().unwrap(),
            BubbleSnapshot {
                request_id: Some(7),
                user_text: Some("这是语音识别的原话".into()),
                source: Some(ChatSource::Voice),
                text: String::new(),
                streaming: true,
            }
        );
    }

    #[test]
    fn test_static_chat_feedback_completes_once_with_reading_protected() {
        let b = SharedBubble::new();
        b.set_interaction_active(true);
        b.begin_stream(&chat_request(1, "原始请求", ChatSource::Text))
            .unwrap();
        b.append_stream_text(1, "对话暂时不可用，请检查设置后再试。")
            .unwrap();
        let end = b.finish_stream(1).unwrap().unwrap();
        assert_eq!(end.request_id, 1);
        assert!(!b.snapshot().unwrap().streaming);
        assert!(b.is_interacting());
        assert!(!b.set_notice_text("后返回的后台观察").unwrap());
        assert_eq!(b.finish_stream(1).unwrap(), None);
    }

    #[test]
    fn test_final_reply_snapshot_survives_notice_after_collapse() {
        let b = SharedBubble::new();
        b.begin_stream(&chat_request(1, "原始请求", ChatSource::Gamepad))
            .unwrap();
        b.append_stream_text(1, "收起时仍在生成的最终回复").unwrap();
        b.set_interaction_active(false);
        let end = b.finish_stream(1).unwrap().unwrap();
        assert!(b.set_notice_text("后到的普通通知").unwrap());
        assert_eq!(end.text, "收起时仍在生成的最终回复");
        assert_eq!(b.snapshot().unwrap().text, end.text);
        assert_eq!(
            b.pending_text.lock().unwrap().as_deref(),
            Some("后到的普通通知")
        );
    }

    #[test]
    fn test_old_finish_and_guard_cannot_touch_new_request() {
        let b = SharedBubble::new();
        b.begin_stream(&chat_request(1, "上一句", ChatSource::Text))
            .unwrap();
        b.append_stream_text(1, "上一轮回复").unwrap();
        b.finish_stream(1).unwrap();
        b.begin_stream(&chat_request(2, "下一句", ChatSource::Voice))
            .unwrap();
        b.append_stream_text(2, "新回复").unwrap();
        b.release_generation(1);
        assert_eq!(b.finish_stream(1).unwrap(), None);
        assert!(b.append_stream_text(1, "迟到的旧字").is_err());
        assert_eq!(
            b.snapshot().unwrap(),
            BubbleSnapshot {
                request_id: Some(2),
                user_text: Some("下一句".into()),
                source: Some(ChatSource::Voice),
                text: "新回复".into(),
                streaming: true,
            }
        );
    }

    #[test]
    fn test_pending_accumulates_only_for_the_owner() {
        let b = SharedBubble::new();
        b.begin_stream(&chat_request(1, "原始请求", ChatSource::Text))
            .unwrap();
        b.append_stream_text(1, "Hello").unwrap();
        b.append_stream_text(1, " World").unwrap();
        assert!(b
            .begin_stream(&chat_request(2, "意外并发", ChatSource::Voice))
            .is_err());
        assert_eq!(b.snapshot().unwrap().text, "Hello World");
        assert_eq!(b.pending_text.lock().unwrap().as_deref(), None);
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
