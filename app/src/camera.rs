//! 摄像头观察模块：接收前端摄像头帧并做低频 Vision 分析。
//!
//! 摄像头权限和采样由隐藏 WebView 中的 `getUserMedia` 负责，本模块只处理 IPC 传入的
//! JPEG data URL、节流、业务避让、Vision 调用和独立目录持久化。
//! 这样可以保持 app 层平台集成清晰，并复用 core 的结构化视觉分析与记录格式。

use ai_pad_core::app_settings::AppSettings;
use ai_pad_core::camera_observation::CameraObservationRecord;
use ai_pad_core::prompts::{PromptsConfig, VisionPromptConfig};
use ai_pad_core::vision::{self, VisionConfig};
use base64::Engine;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tracing::{debug, info, warn};

static LAST_CAMERA_ANALYSIS: Mutex<Option<Instant>> = Mutex::new(None);
static CAMERA_ANALYSIS_RUNNING: AtomicBool = AtomicBool::new(false);
static CAMERA_WINDOW_AUTHORIZED: AtomicBool = AtomicBool::new(false);

/// 预创建摄像头观察窗口。窗口默认隐藏，只承载 getUserMedia 采样脚本。
pub fn precreate_camera_window(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("camera").is_some() {
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, "camera", WebviewUrl::App("camera.html".into()))
        .title("Camera Observation")
        .inner_size(360.0, 240.0)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .skip_taskbar(true)
        .visible(false)
        .build()?;
    info!("camera observation window precreated");
    let _ = window.hide();
    Ok(())
}

/// 通知摄像头窗口重新读取设置并按需开始/停止采样。
pub fn refresh_camera_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("camera") {
        let settings = AppSettings::load();
        if settings.appearance.camera_observation_enabled {
            if !CAMERA_WINDOW_AUTHORIZED.load(Ordering::SeqCst) {
                match window.show() {
                    Ok(()) => debug!("camera window shown for permission/capture"),
                    Err(e) => warn!(error = %e, "failed to show camera window"),
                }
                let _ = window.set_focus();
            } else if let Err(e) = window.hide() {
                warn!(error = %e, "failed to hide authorized camera window");
            }
        } else if let Err(e) = window.hide() {
            warn!(error = %e, "failed to hide disabled camera window");
        }
        match window.emit("camera-observation-refresh", ()) {
            Ok(()) => info!(
                enabled = settings.appearance.camera_observation_enabled,
                interval_sec = settings.appearance.camera_observation_interval_sec,
                save_frames = settings.appearance.camera_save_frames,
                "camera observation refresh emitted"
            ),
            Err(e) => warn!(error = %e, "failed to emit camera observation refresh"),
        }
    } else {
        warn!("camera observation refresh skipped because window is missing");
    }
}

/// 请求摄像头窗口在当前周期采样一次。
pub fn request_camera_capture(app: &AppHandle) {
    if !AppSettings::load().appearance.camera_observation_enabled {
        return;
    }
    if let Some(window) = app.get_webview_window("camera") {
        match window.emit("camera-observation-capture", ()) {
            Ok(()) => debug!("camera observation capture emitted"),
            Err(e) => warn!(error = %e, "failed to emit camera observation capture"),
        }
    } else {
        warn!("camera observation capture skipped because window is missing");
    }
}

#[tauri::command]
pub fn cmd_camera_ready(app: AppHandle) -> Result<(), String> {
    CAMERA_WINDOW_AUTHORIZED.store(true, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("camera") {
        window.hide().map_err(|e| e.to_string())?;
        info!("camera observation window hidden after stream became ready");
    }
    Ok(())
}

#[tauri::command]
pub fn cmd_camera_log(message: String) {
    info!(message = %message, "[camera/frontend]");
}

#[tauri::command]
pub async fn cmd_camera_frame(
    app: AppHandle,
    data_url: String,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let settings = AppSettings::load();
    if !settings.appearance.camera_observation_enabled {
        return Ok(());
    }
    if let Some(window) = app.get_webview_window("camera") {
        let _ = window.hide();
    }
    info!(
        width,
        height,
        bytes = data_url.len(),
        "camera frame received"
    );

    let interval = settings.appearance.screenshot_interval_sec.clamp(5, 3600);
    if camera_finished_recently(interval / 2) {
        debug!("camera frame skipped because analysis finished recently");
        return Ok(());
    }

    if CAMERA_ANALYSIS_RUNNING.swap(true, Ordering::SeqCst) {
        debug!("camera frame skipped because another analysis is running");
        return Ok(());
    }
    let result = analyze_camera_frame(app, data_url, width, height, settings).await;
    CAMERA_ANALYSIS_RUNNING.store(false, Ordering::SeqCst);
    result
}

async fn analyze_camera_frame(
    app: AppHandle,
    data_url: String,
    width: u32,
    height: u32,
    settings: AppSettings,
) -> Result<(), String> {
    if ai_pad_core::performance::blocks_screenshot_observation() {
        debug!("camera observation skipped while performance is active");
        return Ok(());
    }
    if crate::game::is_game_busy(&app) {
        debug!("camera observation skipped while game is busy");
        return Ok(());
    }
    {
        let bubble: tauri::State<crate::bubble::SharedBubble> = app.state();
        if bubble.is_chat_active() {
            debug!("camera observation skipped while chat is active");
            return Ok(());
        }
    }

    let jpeg = decode_jpeg_data_url(&data_url)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
    let ai_config = ai_pad_core::ai_config::AiConfig::load()?;
    let prompts = PromptsConfig::load();
    let prompt_cfg = VisionPromptConfig {
        prompt: prompts.camera.prompt,
        prompt_multi: String::new(),
    };

    let analysis =
        vision::analyze_screenshot(&ai_config, &VisionConfig::default(), &prompt_cfg, &b64, 1)
            .await?;

    let record = CameraObservationRecord {
        analysis: analysis.clone(),
        width,
        height,
        jpeg_size: jpeg.len(),
        saved_frame: settings.appearance.camera_save_frames,
    };
    let path = ai_pad_core::camera_observation::save_camera_observation(&jpeg, &record)?;
    info!(
        path = ?path,
        width,
        height,
        chars = analysis.description.chars().count(),
        saved_frame = record.saved_frame,
        "camera observation saved"
    );
    *LAST_CAMERA_ANALYSIS.lock().unwrap() = Some(Instant::now());
    Ok(())
}

fn camera_finished_recently(interval_sec: u64) -> bool {
    LAST_CAMERA_ANALYSIS
        .lock()
        .unwrap()
        .is_some_and(|last| last.elapsed() < Duration::from_secs(interval_sec))
}

fn decode_jpeg_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    let Some((header, payload)) = data_url.split_once(',') else {
        return Err("摄像头帧 data URL 格式无效".into());
    };
    if !header.starts_with("data:image/jpeg") || !header.ends_with(";base64") {
        return Err("摄像头帧必须是 JPEG base64 data URL".into());
    }
    base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| format!("解码摄像头帧失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_jpeg_data_url_rejects_non_jpeg() {
        let err = decode_jpeg_data_url("data:image/png;base64,AAAA").unwrap_err();
        assert!(err.contains("JPEG"));
    }

    #[test]
    fn decode_jpeg_data_url_decodes_payload() {
        let data = decode_jpeg_data_url("data:image/jpeg;base64,AQID").unwrap();
        assert_eq!(data, vec![1, 2, 3]);
    }
}
