// voice.rs — 可见的语音输入条窗口,接收任意语音输入法注入的文字
//
// 流程:
// 1. App 启动时预创建 voice 窗口 (visible:true 但放在屏幕外, 用户看不到)
// 2. 用户按住手柄 voice 键 → 移到屏幕中下 + 强制前台化 + emit 'voice-clear'
//    → 模拟用户配置的输入法语音热键 → 输入法识别文字注入到 textarea
// 3. 用户松开手柄 voice 键 → 等识别引擎完成注入 → 取走 textarea 内容 → 送 AI
// 4. 取走文本后窗口移回屏幕外,等待下次使用

use std::sync::{Mutex, mpsc};

use tauri::{AppHandle, Emitter, Listener, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder};

const VOICE_W: u32 = 280;
const VOICE_H: u32 = 40;
const OFFSCREEN: i32 = -10000;

pub struct SharedVoice {
    pub text: Mutex<String>,
}

impl SharedVoice {
    pub fn new() -> Self {
        Self { text: Mutex::new(String::new()) }
    }
}

impl Default for SharedVoice {
    fn default() -> Self { Self::new() }
}

/// 启动时预创建 voice 窗口,放在屏幕外 (visible:true 才能成为合法焦点目标)
pub fn precreate_voice_window(app: &AppHandle) -> Result<(), tauri::Error> {
    if app.get_webview_window("voice").is_some() {
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, "voice", WebviewUrl::App("voice.html".into()))
        .title("Voice Input")
        .inner_size(VOICE_W as f64, VOICE_H as f64)
        .position(OFFSCREEN as f64, OFFSCREEN as f64)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(true)
        .build()?;
    let _ = window.set_size(PhysicalSize::new(VOICE_W, VOICE_H));
    let _ = window.set_position(PhysicalPosition::new(OFFSCREEN, OFFSCREEN));
    Ok(())
}

/// voice 按下: 清空状态 + 移到屏幕中下 + 强制前台化 + 通知前端清空 textarea
pub fn open_voice_capture(app: &AppHandle) -> Result<(), String> {
    let state: State<SharedVoice> = app.state();
    state.text.lock().map_err(|e| e.to_string())?.clear();

    let window = match app.get_webview_window("voice") {
        Some(w) => w,
        None => {
            // 兜底: 启动时预创建失败,这里再试
            precreate_voice_window(app).map_err(|e| e.to_string())?;
            app.get_webview_window("voice")
                .ok_or_else(|| "voice 窗口创建失败".to_string())?
        }
    };

    // 移到当前屏幕中下方,用户能看到"正在录音"
    if let Ok(Some(monitor)) = window.current_monitor() {
        let m_size = monitor.size();
        let m_pos = monitor.position();
        let x = m_pos.x + (m_size.width as i32 - VOICE_W as i32) / 2;
        let y = m_pos.y + m_size.height as i32 - VOICE_H as i32 - 120;
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }

    // 确保可见 + 抢焦点
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit_to("voice", "voice-clear", ());

    // 用 AttachThreadInput 强制前台化,绕过 Windows 安全限制
    // 否则输入法语音注入会跑到我们之前的前台窗口（终端/桌面/浏览器…）
    #[cfg(target_os = "windows")]
    {
        if let Ok(hwnd) = window.hwnd() {
            let hwnd_raw = hwnd.0 as isize;
            if let Err(e) = ai_pad_core::hotkey::force_foreground(hwnd_raw) {
                // 失败不致命,焦点可能已经够用,但日志要记
                eprintln!("[voice] force_foreground 失败: {e}");
            }
        }
    }

    Ok(())
}

/// voice 释放: 等前端确认文本已同步 → 取走 → 移回屏幕外
///
/// 不再用固定 sleep 猜 IPC 延迟,而是:
/// 1. emit voice-flush → 前端 invoke(cmd_voice_update_text) 写入 SharedVoice.text
/// 2. 前端 invoke 完成后 emit voice-ready
/// 3. 后端阻塞等 voice-ready 到达 → 此时 SharedVoice.text 保证是最新的
pub fn take_voice_text(app: &AppHandle) -> Result<String, String> {
    let (tx, rx) = mpsc::channel::<()>();

    // 注册一次性监听: 前端 invoke 完成后发 voice-ready,我们收到就继续
    let _listener_id = app.listen("voice-ready", move |_event| {
        let _ = tx.send(());
    });

    // 通知前端: 把当前 ta.value 通过 invoke 同步到后端
    let _ = app.emit_to("voice", "voice-flush", ());

    // 阻塞等待前端确认 (最多 3 秒超时,防止前端挂死导致永远卡住)
    match rx.recv_timeout(std::time::Duration::from_secs(3)) {
        Ok(()) => {}
        Err(mpsc::RecvTimeoutError::Timeout) => {
            eprintln!("[voice] ⚠ voice-ready 超时 (3s),将读取可能过期的文本");
        }
        Err(e) => return Err(format!("channel error: {e}")),
    }

    // 此时 SharedVoice.text 已被前端 invoke 更新为最新值
    let state: State<SharedVoice> = app.state();
    let text = std::mem::take(&mut *state.text.lock().map_err(|e| e.to_string())?);

    // 清理 + 归位
    let _ = app.emit_to("voice", "voice-clear", ());
    if let Some(window) = app.get_webview_window("voice") {
        let _ = window.set_position(PhysicalPosition::new(OFFSCREEN, OFFSCREEN));
    }

    Ok(text)
}

#[tauri::command]
pub async fn cmd_voice_update_text(
    state: State<'_, SharedVoice>,
    text: String,
) -> Result<(), String> {
    *state.text.lock().map_err(|e| e.to_string())? = text;
    Ok(())
}

#[tauri::command]
pub async fn cmd_voice_get_text(state: State<'_, SharedVoice>) -> Result<String, String> {
    Ok(state.text.lock().map_err(|e| e.to_string())?.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_voice_default_empty() {
        let v = SharedVoice::new();
        assert!(v.text.lock().unwrap().is_empty());
    }

    #[test]
    fn test_shared_voice_take_clears() {
        let v = SharedVoice::new();
        *v.text.lock().unwrap() = "你好世界".into();
        let taken = std::mem::take(&mut *v.text.lock().unwrap());
        assert_eq!(taken, "你好世界");
        assert!(v.text.lock().unwrap().is_empty());
    }

    #[test]
    fn test_voice_constants_reasonable() {
        // 280x40 是用户能看到的小录音条尺寸
        assert!(VOICE_W >= 200 && VOICE_W <= 500);
        assert!(VOICE_H >= 30 && VOICE_H <= 80);
    }
}
