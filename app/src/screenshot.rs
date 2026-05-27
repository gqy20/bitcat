//! 截图观察模块：定时截屏 → 去重 → Vision API 分析 → 气泡展示。
//!
//! 截图管线在独立线程上运行（`screenshot_loop`），使用单线程 tokio runtime
//! 驱动异步 Vision API 调用。手柄主循环（`gamepad_loop`）不参与截图流程。
//!
//! ## unsafe 安全不变量
//!
//! `capture_all_screens` 和 `enumerate_displays` 使用 `static mut` 指针
//! （`FRAMES_PTR` / `DISPLAYS_PTR`）在 `EnumDisplayMonitors` 回调中传递数据。
//! 调用约定：进入回调前赋值为栈上 `Mutex` 的引用，回调返回后立即清零。
//! **不得并发调用这两个函数**——所有截图入口必须先持有
//! `SCREENSHOT_PIPELINE_LOCK`，确保 BitBlt 捕获和 Vision 分析串行。
//!
//! 与 [`crate::bubble`] 模块交互：分析结果通过 `show_bubble` 显示；
//! 与 [`bitcat_core::vision`] 模块交互：构建 Vision API 请求并解析响应。

use bitcat_core::screenshot::{CapturedFrame, ScreenInfo, ScreenshotTarget};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use tracing::{debug, info, warn};

static SCREENSHOT_PIPELINE_LOCK: Mutex<()> = Mutex::new(());
static LAST_SCREENSHOT_FINISHED: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Debug)]
pub struct CapturedMonitorFrame {
    pub label: String,
    pub frame: CapturedFrame,
}

#[derive(Debug)]
struct MonitorAnalysisResult {
    monitor_label: String,
    description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameSkipReason {
    Empty,
    MostlyBlack {
        dark_samples: usize,
        total_samples: usize,
    },
}

fn mark_screenshot_finished() {
    *LAST_SCREENSHOT_FINISHED.lock().unwrap() = Some(Instant::now());
}

fn emit_screenshot_observing(app: &tauri::AppHandle) {
    let bus: tauri::State<'_, crate::pet_event_bus::SharedPetEventBus> = app.state();
    bus.emit(
        app,
        bitcat_core::pet_event::PetEvent::screenshot_observing(),
    );
}

fn screenshot_finished_recently(interval_sec: u64) -> bool {
    LAST_SCREENSHOT_FINISHED
        .lock()
        .unwrap()
        .is_some_and(|last| last.elapsed() < Duration::from_secs(interval_sec))
}

fn classify_frame_skip_reason(frame: &CapturedFrame) -> Option<FrameSkipReason> {
    let total_pixels = frame.pixels.len() / 4;
    if total_pixels == 0 || frame.width == 0 || frame.height == 0 {
        return Some(FrameSkipReason::Empty);
    }

    let sample_count = total_pixels.min(256);
    let dark_samples = (0..sample_count)
        .filter(|&i| {
            let pixel_index = if sample_count == 1 {
                0
            } else {
                i * (total_pixels - 1) / (sample_count - 1)
            };
            let idx = pixel_index * 4;
            let b = frame.pixels[idx];
            let g = frame.pixels[idx + 1];
            let r = frame.pixels[idx + 2];
            r <= 8 && g <= 8 && b <= 8
        })
        .count();

    if dark_samples * 100 >= sample_count * 95 {
        Some(FrameSkipReason::MostlyBlack {
            dark_samples,
            total_samples: sample_count,
        })
    } else {
        None
    }
}

fn classify_monitor_skip_reason(monitor: &CapturedMonitorFrame) -> Option<FrameSkipReason> {
    classify_frame_skip_reason(&monitor.frame)
}

// ---- Windows BitBlt 截图 ----

/// 根据截取目标（主屏 / 全部屏幕）执行 BitBlt 截屏。
///
/// 返回 BGRA 像素缓冲区及尺寸；非 Windows 平台直接返回错误。
#[cfg(target_os = "windows")]
pub fn capture_target(target: &ScreenshotTarget) -> Result<CapturedFrame, String> {
    match target {
        ScreenshotTarget::Primary => capture_primary(),
        ScreenshotTarget::All => capture_all_screens(),
    }
}

#[cfg(target_os = "windows")]
pub fn capture_target_frames(
    target: &ScreenshotTarget,
) -> Result<Vec<CapturedMonitorFrame>, String> {
    match target {
        ScreenshotTarget::Primary => Ok(vec![CapturedMonitorFrame {
            label: "primary".into(),
            frame: capture_primary()?,
        }]),
        ScreenshotTarget::All => capture_all_monitor_frames(),
    }
}

/// 截取主显示器画面（BitBlt + GetDIBits → BGRA 像素缓冲区）。
#[cfg(target_os = "windows")]
fn capture_primary() -> Result<CapturedFrame, String> {
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        if width <= 0 || height <= 0 {
            return Err("无法获取屏幕尺寸".into());
        }
        let w = width as u32;
        let h = height as u32;

        let hdc_screen = GetDC(std::ptr::null_mut());
        if hdc_screen.is_null() {
            return Err("GetDC 失败".into());
        }
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        if hdc_mem.is_null() {
            return Err("CreateCompatibleDC 失败".into());
        }
        let hbitmap = CreateCompatibleBitmap(hdc_screen, w as i32, h as i32);
        if hbitmap.is_null() {
            DeleteDC(hdc_mem);
            return Err("CreateCompatibleBitmap 失败".into());
        }

        let old = SelectObject(hdc_mem, hbitmap);
        let result = BitBlt(hdc_mem, 0, 0, w as i32, h as i32, hdc_screen, 0, 0, SRCCOPY);
        if result == 0 {
            SelectObject(hdc_mem, old);
            DeleteObject(hbitmap);
            DeleteDC(hdc_mem);
            return Err("BitBlt 失败".into());
        }

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w as i32;
        bmi.bmiHeader.biHeight = -(h as i32); // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        let row_size = w * 4;
        let buf_size = (row_size * h) as usize;
        let mut pixels = vec![0u8; buf_size];

        let scan_lines = GetDIBits(
            hdc_mem,
            hbitmap,
            0,
            h,
            pixels.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(hdc_mem, old);
        DeleteObject(hbitmap);
        DeleteDC(hdc_mem);

        if scan_lines == 0 {
            return Err("GetDIBits 失败".into());
        }

        Ok(CapturedFrame {
            pixels,
            width: w,
            height: h,
        })
    }
}

/// 截取所有显示器并水平拼接为一帧。
///
/// 使用 `static mut FRAMES_PTR` 将栈上 `Mutex` 传递给 `EnumDisplayMonitors` 回调。
/// 安全约束：赋值 → 调用 → 立即清零，且调用期间不可并发。
#[cfg(target_os = "windows")]
fn capture_all_screens() -> Result<CapturedFrame, String> {
    use bitcat_core::screenshot::stitch_horizontal;
    let frames = capture_all_monitor_frames()?;
    if frames.len() == 1 {
        return Ok(frames.into_iter().next().unwrap().frame);
    }
    let refs: Vec<&CapturedFrame> = frames.iter().map(|f| &f.frame).collect();
    Ok(stitch_horizontal(&refs))
}

#[cfg(target_os = "windows")]
fn capture_all_monitor_frames() -> Result<Vec<CapturedMonitorFrame>, String> {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
        EnumDisplayMonitors, GetDC, GetDIBits, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, HDC, HMONITOR, SRCCOPY,
    };

    static mut FRAMES_PTR: *const std::sync::Mutex<Vec<(i32, CapturedFrame)>> = std::ptr::null();

    unsafe extern "system" fn monitor_callback(
        _hmonitor: HMONITOR,
        _hdc: HDC,
        rect: *mut RECT,
        _lparam: LPARAM,
    ) -> i32 {
        let rect = &*rect;
        let w = (rect.right - rect.left) as u32;
        let h = (rect.bottom - rect.top) as u32;

        let hdc_screen = GetDC(std::ptr::null_mut());
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        let hbitmap = CreateCompatibleBitmap(hdc_screen, w as i32, h as i32);
        let old = SelectObject(hdc_mem, hbitmap);

        BitBlt(
            hdc_mem, 0, 0, w as i32, h as i32, hdc_screen, rect.left, rect.top, SRCCOPY,
        );

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w as i32;
        bmi.bmiHeader.biHeight = -(h as i32);
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        let mut pixels = vec![0u8; (w * h * 4) as usize];
        GetDIBits(
            hdc_mem,
            hbitmap,
            0,
            h,
            pixels.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(hdc_mem, old);
        DeleteObject(hbitmap);
        DeleteDC(hdc_mem);

        (*FRAMES_PTR).lock().unwrap().push((
            rect.left,
            CapturedFrame {
                pixels,
                width: w,
                height: h,
            },
        ));

        1
    }

    let frames: std::sync::Mutex<Vec<(i32, CapturedFrame)>> = std::sync::Mutex::new(Vec::new());

    unsafe {
        FRAMES_PTR = &frames;
        let result = EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            Some(monitor_callback),
            0,
        );
        FRAMES_PTR = std::ptr::null();

        if result == 0 {
            return Err("EnumDisplayMonitors 失败".into());
        }
    }

    let mut frames = frames.into_inner().unwrap();
    if frames.is_empty() {
        return Err("未找到显示器".into());
    }

    frames.sort_by_key(|(left, _)| *left);
    let total = frames.len();
    Ok(frames
        .into_iter()
        .enumerate()
        .map(|(idx, (_left, frame))| CapturedMonitorFrame {
            label: if total == 1 {
                "primary".into()
            } else {
                format!("monitor{}", idx + 1)
            },
            frame,
        })
        .collect())
}

/// 枚举所有显示器的位置和尺寸信息。
///
/// 同样使用 `static mut DISPLAYS_PTR` + `EnumDisplayMonitors` 回调模式，
/// 调用约定与 `capture_all_screens` 一致。
#[cfg(target_os = "windows")]
pub fn enumerate_displays() -> Vec<ScreenInfo> {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};

    static mut DISPLAYS_PTR: *const std::sync::Mutex<Vec<ScreenInfo>> = std::ptr::null();

    unsafe extern "system" fn enum_callback(
        _hmonitor: HMONITOR,
        _hdc: HDC,
        rect: *mut RECT,
        _lparam: LPARAM,
    ) -> i32 {
        let rect = &*rect;
        (*DISPLAYS_PTR).lock().unwrap().push(ScreenInfo {
            left: rect.left,
            top: rect.top,
            width: (rect.right - rect.left) as u32,
            height: (rect.bottom - rect.top) as u32,
        });
        1
    }

    let displays: std::sync::Mutex<Vec<ScreenInfo>> = std::sync::Mutex::new(Vec::new());

    unsafe {
        DISPLAYS_PTR = &displays;
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            Some(enum_callback),
            0,
        );
        DISPLAYS_PTR = std::ptr::null();
    }

    displays.into_inner().unwrap()
}

// ---- 非 Windows 平台 stub ----

#[cfg(not(target_os = "windows"))]
pub fn capture_target(_target: &ScreenshotTarget) -> Result<CapturedFrame, String> {
    Err("截图仅支持 Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn capture_target_frames(
    _target: &ScreenshotTarget,
) -> Result<Vec<CapturedMonitorFrame>, String> {
    Err("截图仅支持 Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn enumerate_displays() -> Vec<ScreenInfo> {
    vec![]
}

// ---- 截图线程 ----

/// 截图线程共享状态：dHash 上次哈希值 + 启停开关。
pub struct SharedScreenshotState {
    pub last_hash: Mutex<u64>,
    pub enabled: Mutex<bool>,
    pub hidden_analysis_count: Mutex<u32>,
}

impl Default for SharedScreenshotState {
    fn default() -> Self {
        Self {
            last_hash: Mutex::new(0),
            enabled: Mutex::new(true),
            hidden_analysis_count: Mutex::new(0),
        }
    }
}

fn hidden_screenshot_count(state: &SharedScreenshotState) -> u32 {
    *state
        .hidden_analysis_count
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn emit_hidden_screenshot_count(app: &tauri::AppHandle, count: u32) {
    let _ = app.emit("screenshot-hidden-count-changed", count);
}

fn increment_hidden_screenshot_count(app: &tauri::AppHandle) {
    let state: tauri::State<'_, SharedScreenshotState> = app.state();
    let count = {
        let mut guard = state
            .hidden_analysis_count
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *guard = guard.saturating_add(1);
        *guard
    };
    emit_hidden_screenshot_count(app, count);
}

#[tauri::command]
pub fn cmd_get_hidden_screenshot_count(
    state: tauri::State<'_, SharedScreenshotState>,
) -> Result<u32, String> {
    Ok(hidden_screenshot_count(&state))
}

#[tauri::command]
pub fn cmd_clear_hidden_screenshot_count(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedScreenshotState>,
) -> Result<u32, String> {
    {
        let mut guard = state
            .hidden_analysis_count
            .lock()
            .map_err(|e| format!("hidden screenshot count lock poisoned: {e}"))?;
        *guard = 0;
    }
    emit_hidden_screenshot_count(&app, 0);
    Ok(0)
}

#[derive(serde::Serialize)]
pub struct RecentScreenshotAnalysisSummary {
    pub day: String,
    pub time_label: String,
    pub description: String,
    pub width: u32,
    pub height: u32,
}

#[tauri::command]
pub fn cmd_get_recent_screenshot_analyses(
    count: Option<usize>,
) -> Result<Vec<RecentScreenshotAnalysisSummary>, String> {
    let count = count.unwrap_or(3).clamp(1, 6);
    let base = bitcat_core::screenshot::screenshot_base_dir()?;
    Ok(
        bitcat_core::screenshot::list_recent_analyses_multi_day_named(&base, count)
            .into_iter()
            .map(|(day, file_name, record)| RecentScreenshotAnalysisSummary {
                time_label: screenshot_time_label(&file_name, &day),
                day,
                description: record.description().to_string(),
                width: record.width,
                height: record.height,
            })
            .collect(),
    )
}

fn screenshot_time_label(file_name: &str, day: &str) -> String {
    let digits: String = file_name
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.len() >= 6 {
        format!("{}:{}", &digits[0..2], &digits[2..4])
    } else {
        day.to_string()
    }
}

#[cfg(test)]
mod inbox_tests {
    use super::*;

    #[test]
    fn screenshot_time_label_uses_analysis_file_prefix() {
        assert_eq!(
            screenshot_time_label("132405_analysis.json", "2026-05-20"),
            "13:24"
        );
        assert_eq!(
            screenshot_time_label("132405_primary_analysis.json", "2026-05-20"),
            "13:24"
        );
    }

    #[test]
    fn screenshot_time_label_falls_back_to_day() {
        assert_eq!(
            screenshot_time_label("bad_analysis.json", "2026-05-20"),
            "2026-05-20"
        );
    }
}

/// 截图观察线程主循环（在独立线程上运行）。
///
/// 每轮流程：sleep → 检查启停/跳舞/熄屏/聊天状态 → BitBlt 截屏 →
/// 全黑帧采样 → 缩放 JPEG → Vision API 分析 → 保存文件 → 气泡展示 →
/// 定时生成屏幕活动摘要。使用单线程 tokio runtime 驱动异步 HTTP 调用。
pub fn screenshot_loop(app: &tauri::AppHandle) {
    use bitcat_core::screenshot::ScreenshotConfig;
    use tracing::{debug, error, info, trace, warn};

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "截图线程 Tokio 运行时创建失败");
            return;
        }
    };

    let config = {
        let cfg = ScreenshotConfig::from_env();
        match cfg.validate() {
            Ok(()) => cfg,
            Err(e) => {
                error!(error = %e, "截图配置无效");
                return;
            }
        }
    };

    // 从 app_settings.json 读取用户可调间隔；每轮循环都会 re-load
    let initial_interval = bitcat_core::app_settings::AppSettings::load()
        .appearance
        .screenshot_interval_sec
        .clamp(5, 3600);
    info!(
        interval_sec = initial_interval,
        max_width = config.max_width,
        "截图观察线程启动"
    );

    let mut cycle_count: u32 = 0;
    debug!("[screenshot] 进入主循环");
    loop {
        // 每轮重新读取间隔，用户前端调整后下一轮即生效
        if crate::shutdown::is_requested() {
            info!("[screenshot] shutdown requested, exiting");
            break;
        }
        let interval_sec = bitcat_core::app_settings::AppSettings::load()
            .appearance
            .screenshot_interval_sec
            .clamp(5, 3600);
        trace!(interval_sec, "[screenshot] sleep begin");
        std::thread::sleep(std::time::Duration::from_secs(interval_sec));
        if crate::shutdown::is_requested() {
            info!("[screenshot] shutdown requested after sleep, exiting");
            break;
        }
        cycle_count += 1;
        trace!(cycle = cycle_count, "[screenshot] sleep end");

        trace!("[screenshot] 获取 state");
        let state: tauri::State<SharedScreenshotState> = app.state();
        if !*state.enabled.lock().unwrap() {
            continue;
        }

        // 快速路径：刚完成过截图则跳过，避免无谓锁竞争
        if screenshot_finished_recently(interval_sec / 2) {
            trace!("[screenshot] 刚完成过截图，跳过本轮");
            continue;
        }

        let Ok(_pipeline_guard) = SCREENSHOT_PIPELINE_LOCK.try_lock() else {
            debug!("screenshot cycle skipped because another screenshot is running");
            continue;
        };

        // 表现会话期间跳过本轮：避免视觉分析回调打断舞蹈、音乐响应或游戏表演
        if bitcat_core::performance::blocks_screenshot_observation() {
            debug!(
                cycle = cycle_count,
                "[screenshot] performance active，跳过本轮"
            );
            continue;
        }

        if crate::game::is_game_busy(app) {
            info!(
                cycle = cycle_count,
                phase = crate::game::game_phase(app),
                "screenshot skipped while game is busy"
            );
            mark_screenshot_finished();
            continue;
        }

        {
            let gate: tauri::State<crate::observation_gate::SharedObservationGate> = app.state();
            if let Some(reason) = gate.skip_reason() {
                info!(
                    reason = reason.label(),
                    "screenshot skipped by observation gate"
                );
                mark_screenshot_finished();
                continue;
            }
        }

        // 对话优先级：用户正在聊天时跳过截图+Vision API（避免打断对话、浪费 token）
        {
            let bubble: tauri::State<crate::bubble::SharedBubble> = app.state();
            if bubble.is_chat_active() {
                debug!("screenshot cycle skipped while chat is active");
                continue;
            }
        }

        // 截图。多屏幕时每个显示器独立分析，避免横向拼接再压缩导致文字不可读。
        crate::camera::request_camera_capture(app);
        emit_screenshot_observing(app);
        trace!("[screenshot] 开始捕获");
        let monitor_frames = match capture_target_frames(&config.target) {
            Ok(frames) => {
                debug!(monitor_count = frames.len(), "screenshot captured");
                frames
            }
            Err(e) => {
                warn!(error = %e, "截图捕获失败");
                continue;
            }
        };

        let mut visible_monitors = Vec::new();
        for monitor in monitor_frames {
            match classify_monitor_skip_reason(&monitor) {
                Some(FrameSkipReason::Empty) => {
                    info!(monitor = %monitor.label, "截图帧为空，跳过视觉分析");
                }
                Some(FrameSkipReason::MostlyBlack {
                    dark_samples,
                    total_samples,
                }) => {
                    info!(
                        monitor = %monitor.label,
                        dark = dark_samples,
                        total = total_samples,
                        "检测到近黑帧，跳过视觉分析"
                    );
                }
                None => visible_monitors.push(monitor),
            }
        }

        if visible_monitors.is_empty() {
            debug!("all captured monitors were empty or mostly black; screenshot cycle skipped");
            mark_screenshot_finished();
            continue;
        }

        // 确定要尝试的分辨率列表
        let resolutions: Vec<u32> = if config.debug_resolutions.is_empty() {
            vec![config.max_width]
        } else {
            config.debug_resolutions.clone()
        };

        let ai_config = match bitcat_core::ai_config::AiConfig::load() {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "AI 配置加载失败");
                continue;
            }
        };

        let analysis_results = analyze_visible_monitors(
            app,
            visible_monitors,
            &ai_config,
            &resolutions,
            config.jpeg_quality,
        );
        let multi_monitor = analysis_results.len() > 1;
        let bubble_parts = bubble_parts_from_results(analysis_results, multi_monitor);

        let show_bubble = bitcat_core::app_settings::AppSettings::load()
            .appearance
            .screenshot_show_bubble;

        if bubble_parts.is_empty() {
            info!("[screenshot] 描述为空，显示兜底提示");
            increment_hidden_screenshot_count(app);
            if show_bubble {
                let _ = crate::bubble::show_bubble(app, "观察完成，但看不太清内容，已放入待查看");
            }
        } else {
            let text = bubble_parts.join("\n");
            increment_hidden_screenshot_count(app);
            if !show_bubble {
                debug!("[screenshot] 截屏分析弹窗已关闭，分析已收入待查看");
            } else {
                debug!(
                    chars = text.chars().count(),
                    "screenshot analysis stored in inbox"
                );
                match crate::bubble::show_bubble(app, "观察完成，已放入待查看") {
                    Ok(()) => debug!("screenshot completion notice shown"),
                    Err(e) => warn!(error = %e, "screenshot completion notice failed"),
                }
            }
        }

        // 清理 7 天前的截图
        if let Ok(removed) = bitcat_core::screenshot::cleanup_old_screenshots(7) {
            if removed > 0 {
                info!(removed = removed, "清理过期截图");
            }
        }

        // 定时 AI 摘要：每 interval_min 分钟触发一次
        {
            let prompts_cfg = bitcat_core::prompts::PromptsConfig::load();
            let summary_cfg = &prompts_cfg.screen_summary;
            let summary_cycles = (summary_cfg.interval_min as u64 * 60) / config.interval_sec;
            if summary_cycles > 0
                && (cycle_count as u64).is_multiple_of(summary_cycles)
                && cycle_count > 0
            {
                let now = chrono::Local::now();
                let end_time = now.format("%H:%M").to_string();
                let start_time = (now - chrono::Duration::minutes(summary_cfg.interval_min as i64))
                    .format("%H:%M")
                    .to_string();
                let time_range = format!("{start_time}-{end_time}");

                info!(time_range = %time_range, "开始生成屏幕活动摘要");

                if let Ok(today_dir) = bitcat_core::screenshot::ensure_today_dir() {
                    let records = bitcat_core::screenshot::list_recent_analyses(
                        &today_dir,
                        summary_cfg.max_recent_analyses,
                    );
                    let descriptions: Vec<String> = records
                        .into_iter()
                        .map(|r| r.context_text())
                        .filter(|d| !d.is_empty())
                        .collect();

                    if !descriptions.is_empty() {
                        match rt.block_on(bitcat_core::screen_summary::generate_summary(
                            &descriptions,
                            summary_cfg,
                            &ai_config,
                        )) {
                            Ok(summary) => {
                                let context_text = summary.to_context_text();
                                info!(
                                    chars = context_text.chars().count(),
                                    time_range = %time_range,
                                    "屏幕摘要生成完成"
                                );
                                let mut store =
                                    bitcat_core::screen_summary::ScreenSummaryStore::load();
                                store.record(&time_range, summary);
                                if let Err(e) = store.save() {
                                    warn!(error = %e, "保存屏幕摘要失败");
                                }
                            }
                            Err(e) => warn!(error = %e, "屏幕摘要生成失败"),
                        }
                    } else {
                        info!("没有可用的截图分析记录，跳过摘要");
                    }
                } else {
                    warn!("无法获取今日截图目录，跳过摘要");
                }
            }
        }

        mark_screenshot_finished();
    }
}

fn analyze_visible_monitors(
    app: &tauri::AppHandle,
    monitors: Vec<CapturedMonitorFrame>,
    ai_config: &bitcat_core::ai_config::AiConfig,
    resolutions: &[u32],
    jpeg_quality: u8,
) -> Vec<MonitorAnalysisResult> {
    if monitors.is_empty() {
        return Vec::new();
    }

    if monitors.len() == 1 {
        let monitor = monitors.into_iter().next().unwrap();
        return vec![analyze_monitor_with_guards(
            app,
            ai_config,
            monitor,
            resolutions,
            jpeg_quality,
        )];
    }

    let mut handles = Vec::with_capacity(monitors.len());
    for (index, monitor) in monitors.into_iter().enumerate() {
        if should_skip_vision_now(app) {
            break;
        }
        let ai_config = ai_config.clone();
        let resolutions = resolutions.to_vec();
        let handle = std::thread::spawn(move || {
            let label = monitor.label.clone();
            let description = match analyze_and_save_monitor_frame(
                &ai_config,
                &monitor,
                &resolutions,
                jpeg_quality,
            ) {
                Ok(description) => description,
                Err(e) => {
                    warn!(error = %e, monitor = %label, "显示器视觉分析失败");
                    None
                }
            };
            (
                index,
                MonitorAnalysisResult {
                    monitor_label: label,
                    description,
                },
            )
        });
        handles.push(handle);
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.join() {
            Ok(result) => results.push(result),
            Err(_) => warn!("显示器视觉分析线程 panic"),
        }
    }
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

fn analyze_monitor_with_guards(
    app: &tauri::AppHandle,
    ai_config: &bitcat_core::ai_config::AiConfig,
    monitor: CapturedMonitorFrame,
    resolutions: &[u32],
    jpeg_quality: u8,
) -> MonitorAnalysisResult {
    let label = monitor.label.clone();
    if should_skip_vision_now(app) {
        return MonitorAnalysisResult {
            monitor_label: label,
            description: None,
        };
    }
    let description =
        match analyze_and_save_monitor_frame(ai_config, &monitor, resolutions, jpeg_quality) {
            Ok(description) => description,
            Err(e) => {
                warn!(error = %e, monitor = %label, "显示器视觉分析失败");
                None
            }
        };
    MonitorAnalysisResult {
        monitor_label: label,
        description,
    }
}

fn should_skip_vision_now(app: &tauri::AppHandle) -> bool {
    {
        let bubble: tauri::State<crate::bubble::SharedBubble> = app.state();
        if bubble.is_chat_active() {
            debug!("vision skipped because chat became active");
            return true;
        }
    }
    if crate::game::is_game_busy(app) {
        info!(
            phase = crate::game::game_phase(app),
            "vision skipped because game became busy"
        );
        return true;
    }
    false
}

fn bubble_parts_from_results(
    results: Vec<MonitorAnalysisResult>,
    multi_monitor: bool,
) -> Vec<String> {
    results
        .into_iter()
        .filter_map(|result| {
            let description = result.description?;
            if description.is_empty() {
                None
            } else if multi_monitor {
                Some(format!("{}：{}", result.monitor_label, description))
            } else {
                Some(description)
            }
        })
        .collect()
}

fn analyze_and_save_monitor_frame(
    ai_config: &bitcat_core::ai_config::AiConfig,
    monitor: &CapturedMonitorFrame,
    resolutions: &[u32],
    jpeg_quality: u8,
) -> Result<Option<String>, String> {
    use base64::Engine;
    use bitcat_core::screenshot::{encode_jpeg, resize_bgra};
    use bitcat_core::vision::{self, VisionConfig};
    use tracing::{debug, info, warn};

    let prompt_cfg = bitcat_core::prompts::PromptsConfig::load().vision;
    let vision_model = ai_config.model.clone();
    let mut last_description = None;

    let dir = bitcat_core::screenshot::ensure_today_dir()?;
    let prefix = chrono::Local::now().format("%H%M%S").to_string();

    for (i, &res_w) in resolutions.iter().enumerate() {
        let (rgb, w, h) = match resize_bgra(
            &monitor.frame.pixels,
            monitor.frame.width,
            monitor.frame.height,
            res_w,
        ) {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, monitor = %monitor.label, width = res_w, "缩放失败，跳过");
                continue;
            }
        };

        let jpeg = match encode_jpeg(&rgb, w, h, jpeg_quality) {
            Ok(j) => j,
            Err(e) => {
                warn!(error = %e, monitor = %monitor.label, width = res_w, "JPEG 编码失败");
                continue;
            }
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);

        debug!(
            model = %vision_model,
            monitor = %monitor.label,
            width = w,
            height = h,
            "vision request started"
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("创建视觉运行时失败: {e}"))?;
        let analysis = match rt.block_on(vision::analyze_screenshot(
            ai_config,
            &VisionConfig::default(),
            &prompt_cfg,
            &b64,
            1,
        )) {
            Ok(analysis) => {
                info!(
                    model = %vision_model,
                    monitor = %monitor.label,
                    width = w,
                    height = h,
                    chars = analysis.description.chars().count(),
                    "[{}] 视觉分析完成",
                    if resolutions.len() > 1 { format!("{}px", res_w) } else { String::new() }
                );
                analysis
            }
            Err(e) => {
                warn!(error = %e, monitor = %monitor.label, width = res_w, "视觉分析失败");
                bitcat_core::vision::VisionAnalysis::default()
            }
        };

        let record = bitcat_core::screenshot::ScreenshotRecord {
            analysis: analysis.clone(),
            hash: 0,
            skipped: false,
            width: w,
            height: h,
            jpeg_size: jpeg.len(),
        };
        let suffix = if resolutions.len() > 1 {
            format!("_{}_{}px", monitor.label, res_w)
        } else {
            format!("_{}", monitor.label)
        };
        let jpg_path = dir.join(format!("{prefix}{suffix}.jpg"));
        if let Err(e) = std::fs::write(&jpg_path, &jpeg) {
            warn!(error = %e, path = ?jpg_path, "保存 JPEG 失败");
        }
        if let Err(e) = bitcat_core::screenshot::save_analysis_json(&dir, &prefix, &suffix, &record)
        {
            warn!(error = %e, "保存分析结果失败");
        }

        if i == resolutions.len() - 1 {
            last_description = Some(analysis.description);
        } else {
            info!(
                i,
                total = resolutions.len(),
                monitor = %monitor.label,
                "[screenshot] 非最后一个分辨率，跳过气泡"
            );
        }
    }

    Ok(last_description)
}

/// 手动触发截图分析（同步，内部自行创建 tokio runtime）。
pub fn do_screenshot_now(app: &tauri::AppHandle) -> Result<String, String> {
    use bitcat_core::screenshot::ScreenshotConfig;

    let _pipeline_guard = SCREENSHOT_PIPELINE_LOCK
        .try_lock()
        .map_err(|_| "已有截图分析正在进行中，请稍后再试".to_string())?;

    if crate::game::is_game_busy(app) {
        let description = format!(
            "screen_observation_paused:game_{}",
            crate::game::game_phase(app)
        );
        tracing::info!(
            phase = crate::game::game_phase(app),
            "manual screenshot skipped while game is busy"
        );
        return Ok(description);
    }

    let config = ScreenshotConfig::default();
    let ai_config = bitcat_core::ai_config::AiConfig::load()?;
    emit_screenshot_observing(app);

    {
        let gate: tauri::State<crate::observation_gate::SharedObservationGate> = app.state();
        if let Some(reason) = gate.skip_reason() {
            let description = format!("屏幕观察已暂停：{}", reason.label());
            let _ = crate::bubble::show_bubble(app, &description);
            mark_screenshot_finished();
            return Ok(description);
        }
    }

    if crate::game::is_game_busy(app) {
        let description = format!("屏幕观察已暂停：game_{}", crate::game::game_phase(app));
        tracing::info!(
            phase = crate::game::game_phase(app),
            "manual screenshot skipped while game is busy"
        );
        return Ok(description);
    }

    crate::camera::request_camera_capture(app);

    let monitor_frames = capture_target_frames(&config.target)?;
    let mut visible_monitors = Vec::new();
    for monitor in monitor_frames {
        match classify_monitor_skip_reason(&monitor) {
            Some(FrameSkipReason::Empty) => {
                tracing::info!(monitor = %monitor.label, "手动截图帧为空，跳过视觉分析");
            }
            Some(FrameSkipReason::MostlyBlack {
                dark_samples,
                total_samples,
            }) => {
                tracing::info!(
                    monitor = %monitor.label,
                    dark = dark_samples,
                    total = total_samples,
                    "手动截图检测到近黑帧，跳过视觉分析"
                );
            }
            None => visible_monitors.push(monitor),
        }
    }

    if visible_monitors.is_empty() {
        let description = "屏幕似乎已关闭或是黑屏，已跳过视觉分析".to_string();
        let _ = crate::bubble::show_bubble(app, &description);
        mark_screenshot_finished();
        return Ok(description);
    }

    let analysis_results = analyze_visible_monitors(
        app,
        visible_monitors,
        &ai_config,
        &[config.max_width],
        config.jpeg_quality,
    );
    let multi_monitor = analysis_results.len() > 1;
    let parts = bubble_parts_from_results(analysis_results, multi_monitor);

    let description = if parts.is_empty() {
        "喵~ 看不太清屏幕内容，可能需要检查 API 配置".to_string()
    } else {
        parts.join("\n")
    };
    let _ = crate::bubble::show_bubble(app, &description);
    mark_screenshot_finished();
    Ok(description)
}

/// 手动触发截图分析的 Tauri 命令。
///
/// 前端目前未消费返回值（仅 tray 直接调 `do_screenshot_now`），
/// 故此处归一走 ActionBus，返回描述由 Bus 内部自行处理（通过 bubble 展示）。
#[tauri::command]
pub async fn cmd_screenshot_now(app: tauri::AppHandle) -> Result<String, String> {
    crate::action_bus::ActionBus::dispatch(
        &app,
        crate::action_bus::Action::ScreenshotNow,
        crate::action_bus::ActionSource::Frontend {
            cmd: "cmd_screenshot_now".into(),
        },
    );
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use bitcat_core::screenshot::{CapturedFrame, ScreenInfo, ScreenshotConfig};

    use super::{classify_frame_skip_reason, FrameSkipReason};

    fn repeated_bgra(pixel: [u8; 4], count: usize) -> Vec<u8> {
        let mut pixels = Vec::with_capacity(count * 4);
        for _ in 0..count {
            pixels.extend_from_slice(&pixel);
        }
        pixels
    }

    #[test]
    fn test_captured_frame_type_compiles() {
        let frame = CapturedFrame {
            pixels: vec![0; 4],
            width: 1,
            height: 1,
        };
        assert_eq!(frame.width, 1);
    }

    #[test]
    fn test_screen_info_type_compiles() {
        let info = ScreenInfo {
            left: 0,
            top: 0,
            width: 1920,
            height: 1080,
        };
        assert_eq!(info.width, 1920);
    }

    #[test]
    fn test_config_validate_ok() {
        let cfg = ScreenshotConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_empty_frame_is_skipped() {
        let frame = CapturedFrame {
            pixels: vec![],
            width: 0,
            height: 0,
        };
        assert_eq!(
            classify_frame_skip_reason(&frame),
            Some(FrameSkipReason::Empty)
        );
    }

    #[test]
    fn test_mostly_black_frame_is_skipped() {
        let frame = CapturedFrame {
            pixels: repeated_bgra([0, 0, 0, 255], 300),
            width: 30,
            height: 10,
        };
        assert!(matches!(
            classify_frame_skip_reason(&frame),
            Some(FrameSkipReason::MostlyBlack {
                dark_samples: 256,
                total_samples: 256
            })
        ));
    }

    #[test]
    fn test_near_black_frame_is_skipped() {
        let frame = CapturedFrame {
            pixels: repeated_bgra([8, 7, 6, 255], 256),
            width: 16,
            height: 16,
        };
        assert!(matches!(
            classify_frame_skip_reason(&frame),
            Some(FrameSkipReason::MostlyBlack { .. })
        ));
    }

    #[test]
    fn test_visible_frame_is_not_skipped() {
        let mut pixels = repeated_bgra([0, 0, 0, 255], 256);
        for chunk in pixels.chunks_exact_mut(4).take(32) {
            chunk[0] = 80;
            chunk[1] = 120;
            chunk[2] = 200;
        }
        let frame = CapturedFrame {
            pixels,
            width: 16,
            height: 16,
        };
        assert_eq!(classify_frame_skip_reason(&frame), None);
    }
}
