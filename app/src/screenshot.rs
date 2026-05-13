use ai_pad_core::screenshot::{CapturedFrame, ScreenInfo, ScreenshotTarget};
use tauri::Manager;

// ---- Windows BitBlt 截图 ----

#[cfg(target_os = "windows")]
pub fn capture_target(target: &ScreenshotTarget) -> Result<CapturedFrame, String> {
    match target {
        ScreenshotTarget::Primary => capture_primary(),
        ScreenshotTarget::All => capture_all_screens(),
    }
}

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

#[cfg(target_os = "windows")]
fn capture_all_screens() -> Result<CapturedFrame, String> {
    use ai_pad_core::screenshot::stitch_horizontal;
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
        EnumDisplayMonitors, GetDC, GetDIBits, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, HDC, HMONITOR, SRCCOPY,
    };

    static mut FRAMES_PTR: *const std::sync::Mutex<Vec<CapturedFrame>> = std::ptr::null();

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

        (*FRAMES_PTR).lock().unwrap().push(CapturedFrame {
            pixels,
            width: w,
            height: h,
        });

        1
    }

    let frames: std::sync::Mutex<Vec<CapturedFrame>> = std::sync::Mutex::new(Vec::new());

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
    if frames.len() == 1 {
        return Ok(frames.remove(0));
    }

    let refs: Vec<&CapturedFrame> = frames.iter().collect();
    Ok(stitch_horizontal(&refs))
}

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
pub fn enumerate_displays() -> Vec<ScreenInfo> {
    vec![]
}

// ---- 截图线程 ----

use std::sync::Mutex;

pub struct SharedScreenshotState {
    pub last_hash: Mutex<u64>,
    pub enabled: Mutex<bool>,
}

impl Default for SharedScreenshotState {
    fn default() -> Self {
        Self {
            last_hash: Mutex::new(0),
            enabled: Mutex::new(true),
        }
    }
}

/// 截图观察线程主循环。
pub fn screenshot_loop(app: &tauri::AppHandle) {
    use ai_pad_core::screenshot::{encode_jpeg, resize_bgra, ScreenshotConfig};
    use ai_pad_core::vision::{self, VisionConfig};
    use base64::Engine;
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
    let initial_interval = ai_pad_core::app_settings::AppSettings::load()
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
        let interval_sec = ai_pad_core::app_settings::AppSettings::load()
            .appearance
            .screenshot_interval_sec
            .clamp(5, 3600);
        trace!(interval_sec, "[screenshot] sleep begin");
        std::thread::sleep(std::time::Duration::from_secs(interval_sec));
        cycle_count += 1;
        trace!(cycle = cycle_count, "[screenshot] sleep end");

        trace!("[screenshot] 获取 state");
        let state: tauri::State<SharedScreenshotState> = app.state();
        if !*state.enabled.lock().unwrap() {
            continue;
        }

        // 跳舞期间跳过本轮：避免视觉分析回调打断舞蹈表演
        if ai_pad_core::dance::is_dancing() {
            debug!(
                cycle = cycle_count,
                "[screenshot] is_dancing=true，跳过本轮"
            );
            continue;
        }

        // 熄屏检测
        trace!("[screenshot] 检查熄屏");
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics;
            if unsafe { GetSystemMetrics(0x8000) } != 0 {
                debug!("[screenshot] 显示器关闭，跳过");
                continue;
            }
        }

        // 对话优先级：用户正在聊天时跳过截图+Vision API（避免打断对话、浪费 token）
        {
            let bubble: tauri::State<crate::bubble::SharedBubble> = app.state();
            if bubble.is_chat_active() {
                info!("[screenshot] 对话进行中，跳过本轮截图（chat_active=true)");
                continue;
            }
        }

        // 截图
        trace!("[screenshot] 开始捕获");
        let frame = match capture_target(&config.target) {
            Ok(f) => {
                info!("截图周期: 捕获成功 {}x{}", f.width, f.height);
                // 全黑帧检测（覆盖屏保/锁屏等 SM_MONITORISOFF 未覆盖的场景）
                let sample_count = 256;
                let step = (f.pixels.len() / 4).max(1) / sample_count.max(1);
                let black_pixels = (0..sample_count)
                    .filter(|&i| {
                        let idx = i * step * 4;
                        idx + 3 < f.pixels.len()
                            && f.pixels[idx] == 0
                            && f.pixels[idx + 1] == 0
                            && f.pixels[idx + 2] == 0
                    })
                    .count();
                if black_pixels > sample_count * 95 / 100 {
                    info!(
                        black = black_pixels,
                        total = sample_count,
                        "检测到全黑帧，跳过"
                    );
                    continue;
                }
                f
            }
            Err(e) => {
                warn!(error = %e, "截图捕获失败");
                continue;
            }
        };

        // 确定要尝试的分辨率列表
        let resolutions: Vec<u32> = if config.debug_resolutions.is_empty() {
            vec![config.max_width]
        } else {
            config.debug_resolutions.clone()
        };

        let (first_rgb, first_w, first_h) =
            match resize_bgra(&frame.pixels, frame.width, frame.height, resolutions[0]) {
                Ok(r) => r,
                Err(e) => {
                    warn!(error = %e, "截图缩放失败");
                    continue;
                }
            };

        // 逐分辨率处理
        let ai_config = match ai_pad_core::ai_config::AiConfig::load() {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "AI 配置加载失败");
                continue;
            }
        };

        for (i, &res_w) in resolutions.iter().enumerate() {
            let is_last = i == resolutions.len() - 1;

            let (rgb, w, h) = if res_w == resolutions[0] {
                (first_rgb.clone(), first_w, first_h)
            } else {
                match resize_bgra(&frame.pixels, frame.width, frame.height, res_w) {
                    Ok(r) => r,
                    Err(e) => {
                        warn!(error = %e, width = res_w, "缩放失败，跳过");
                        continue;
                    }
                }
            };

            let jpeg = match encode_jpeg(&rgb, w, h, config.jpeg_quality) {
                Ok(j) => j,
                Err(e) => {
                    warn!(error = %e, width = res_w, "JPEG 编码失败");
                    continue;
                }
            };
            let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);

            // Vision API
            let prompt_cfg = ai_pad_core::prompts::PromptsConfig::load().vision;
            let vision_model = ai_config.model.clone();

            // 二次检查 chat_active：截图捕获/缩放期间用户可能刚开始聊天，
            // 此时不应再发起 Vision API 请求（浪费 token + 可能打断对话）
            {
                let bubble: tauri::State<crate::bubble::SharedBubble> = app.state();
                if bubble.is_chat_active() {
                    info!("[screenshot] Vision 前发现 chat_active=true，本轮放弃分析");
                    break;
                }
            }

            info!(model = %vision_model, "视觉分析: 开始请求");
            let description = match rt.block_on(vision::analyze_screenshot(
                &ai_config,
                &VisionConfig::default(),
                &prompt_cfg,
                &b64,
                1,
            )) {
                Ok(desc) => {
                    info!(
                        model = %vision_model,
                        width = w,
                        height = h,
                        chars = desc.chars().count(),
                        "[{}] 视觉分析完成",
                        if resolutions.len() > 1 { format!("{}px", res_w) } else { String::new() }
                    );
                    desc
                }
                Err(e) => {
                    warn!(error = %e, width = res_w, "视觉分析失败");
                    String::new()
                }
            };

            // 保存（文件名带分辨率后缀）
            let record = ai_pad_core::screenshot::ScreenshotRecord {
                description: description.clone(),
                hash: 0,
                skipped: false,
                width: w,
                height: h,
                jpeg_size: jpeg.len(),
            };
            let suffix = if resolutions.len() > 1 {
                format!("_{}px", res_w)
            } else {
                String::new()
            };
            let dir = match ai_pad_core::screenshot::ensure_today_dir() {
                Ok(d) => d,
                Err(e) => {
                    warn!(error = %e, "创建目录失败");
                    continue;
                }
            };
            let prefix = chrono::Local::now().format("%H%M%S").to_string();
            let jpg_path = dir.join(format!("{prefix}{suffix}.jpg"));
            if let Err(e) = std::fs::write(&jpg_path, &jpeg) {
                warn!(error = %e, path = ?jpg_path, "保存 JPEG 失败");
            }
            if let Err(e) =
                ai_pad_core::screenshot::save_analysis_json(&dir, &prefix, &suffix, &record)
            {
                warn!(error = %e, "保存分析结果失败");
            }

            // 只在最后一个分辨率显示气泡
            if is_last {
                if description.is_empty() {
                    info!("[screenshot] 描述为空，显示兜底提示");
                    let _ = crate::bubble::show_bubble(
                        app,
                        "喵~ 看不太清屏幕内容，可能需要检查 API 配置",
                    );
                } else {
                    info!(
                        chars = description.chars().count(),
                        "[screenshot] 调用 show_bubble"
                    );
                    match crate::bubble::show_bubble(app, &description) {
                        Ok(()) => info!("[screenshot] show_bubble 成功"),
                        Err(e) => warn!(error = %e, "[screenshot] show_bubble 失败"),
                    }
                }
            } else {
                info!(
                    i,
                    total = resolutions.len(),
                    "[screenshot] 非最后一个分辨率，跳过气泡"
                );
            }
        }

        // 清理 7 天前的截图
        if let Ok(removed) = ai_pad_core::screenshot::cleanup_old_screenshots(7) {
            if removed > 0 {
                info!(removed = removed, "清理过期截图");
            }
        }

        // 定时 AI 摘要：每 interval_min 分钟触发一次
        {
            let prompts_cfg = ai_pad_core::prompts::PromptsConfig::load();
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

                if let Ok(today_dir) = ai_pad_core::screenshot::ensure_today_dir() {
                    let records = ai_pad_core::screenshot::list_recent_analyses(
                        &today_dir,
                        summary_cfg.max_recent_analyses,
                    );
                    let descriptions: Vec<String> = records
                        .into_iter()
                        .map(|r| r.description)
                        .filter(|d| !d.is_empty())
                        .collect();

                    if !descriptions.is_empty() {
                        match rt.block_on(ai_pad_core::screen_summary::generate_summary(
                            &descriptions,
                            summary_cfg,
                            &ai_config,
                        )) {
                            Ok(summary) => {
                                info!(
                                    chars = summary.chars().count(),
                                    time_range = %time_range,
                                    "屏幕摘要生成完成"
                                );
                                let mut store =
                                    ai_pad_core::screen_summary::ScreenSummaryStore::load();
                                store.record(&time_range, &summary, summary_cfg);
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
    }
}

/// 手动触发截图分析（同步，内部自行创建 tokio runtime）。
pub fn do_screenshot_now(app: &tauri::AppHandle) -> Result<String, String> {
    use ai_pad_core::screenshot::{encode_jpeg, resize_bgra, ScreenshotConfig};
    use ai_pad_core::vision::{self, VisionConfig};
    use base64::Engine;

    let config = ScreenshotConfig::default();
    let frame = capture_target(&config.target)?;
    let (rgb, w, h) = resize_bgra(&frame.pixels, frame.width, frame.height, config.max_width)?;
    let jpeg = encode_jpeg(&rgb, w, h, config.jpeg_quality)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);

    let ai_config = ai_pad_core::ai_config::AiConfig::load()?;
    let prompt_cfg = ai_pad_core::prompts::PromptsConfig::load().vision;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("创建运行时失败: {e}"))?;

    let description = rt.block_on(vision::analyze_screenshot(
        &ai_config,
        &VisionConfig::default(),
        &prompt_cfg,
        &b64,
        enumerate_displays().len(),
    ))?;

    let record = ai_pad_core::screenshot::ScreenshotRecord {
        description: description.clone(),
        hash: 0,
        skipped: false,
        width: w,
        height: h,
        jpeg_size: jpeg.len(),
    };
    if let Err(e) = ai_pad_core::screenshot::save_screenshot(&jpeg, &record) {
        tracing::warn!(error = %e, "保存截图失败");
    }

    let _ = crate::bubble::show_bubble(app, &description);
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
    use ai_pad_core::screenshot::{CapturedFrame, ScreenInfo, ScreenshotConfig};

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
}
