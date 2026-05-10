// voice.rs — 可见的语音输入条窗口,接收任意语音输入法注入的文字
//
// 流程:
// 1. App 启动时预创建 voice 窗口 (visible:true 但放在屏幕外, 用户看不到)
// 2. 用户按住手柄 voice 键 → 移到屏幕中下 + 强制前台化 + emit 'voice-clear'
//    → 模拟用户配置的输入法语音热键 → 输入法识别文字注入到 textarea
// 3. 用户松开手柄 voice 键 → 等识别引擎完成注入 → 取走 textarea 内容 → 送 AI
// 4. 取走文本后窗口移回屏幕外,等待下次使用

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder};

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
    if let Err(e) = app.emit_to("voice", "voice-clear", ()) {
        eprintln!("[voice] emit voice-clear 失败: {e}");
    } else {
        eprintln!("[voice] ✓ 已发送 voice-clear");
    }

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

/// voice 释放: eval 取值+清空 → 等待完成 → 取走 → 归位
///
/// 核心改进: 捕获 eval 错误 + 增加等待时间 + 双重保险清空
pub fn take_voice_text(app: &AppHandle) -> Result<String, String> {
    let mut eval_ok = true;

    // Step 1: eval — 在 WebView2 中原子地读取 textarea + 清空 + 上报
    if let Some(window) = app.get_webview_window("voice") {
        match window.eval(
            r#"(async()=>{const t=document.getElementById('vox');if(t){const v=t.value;t.value='';await window.__TAURI__.core.invoke('cmd_voice_update_text',{text:v});}})()"#
        ) {
            Ok(()) => {}
            Err(e) => {
                eval_ok = false;
                eprintln!("[voice] eval 失败（将依赖 input 事件兜底）: {e}");
            }
        }

        // 给 ExecuteScriptAsync + 内部 invoke IPC 足够时间完成
        // ExecuteScriptAsync 本身通常 < 50ms，但 invoke IPC 需要主线程调度
        // 在 gamepad 线程上调用时主线程可能繁忙，给充裕时间
        std::thread::sleep(std::time::Duration::from_millis(500));
    } else {
        eprintln!("[voice] ⚠ voice 窗口不存在");
        eval_ok = false;
    }

    // Step 2: 取走文本（来源: input 事件实时上报 或 eval invoke）
    let state: State<SharedVoice> = app.state();
    let text = std::mem::take(&mut *state.text.lock().map_err(|e| e.to_string())?);

    // Step 3: 如果文本为空且 eval 成功了，说明可能时序问题导致 invoke 还没写回
    // 再等一轮并重试取值
    if text.is_empty() && eval_ok {
        if let Some(_window) = app.get_webview_window("voice") {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let retry = std::mem::take(&mut *state.text.lock().map_err(|e| e.to_string())?);
            if !retry.is_empty() {
                eprintln!("[voice] 重试取值成功: {} 字符", retry.chars().count());
                return Ok(retry);
            }
        }
    }

    // Step 4: 日志 + 归位
    eprintln!(
        "[voice] 取走文本: {} 字符 (eval={}, 来源={})",
        text.chars().count(),
        eval_ok,
        if text.is_empty() { "空" } else { "有内容" }
    );

    if let Some(window) = app.get_webview_window("voice") {
        let _ = window.set_position(PhysicalPosition::new(OFFSCREEN, OFFSCREEN));
    }

    // Step 5: 再次确保清空（防止残留）
    if let Some(window) = app.get_webview_window("voice") {
        let _ = window.eval(r#"if(document.getElementById('vox'))document.getElementById('vox').value=''"#);
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

    #[test]
    fn test_offscreen_constant_far_enough() {
        // -10000 应该在任何屏幕配置下都不可见
        assert!(OFFSCREEN < -5000);
    }

    #[test]
    fn test_shared_voice_overwrite() {
        // 模拟连续两次写入，验证后一次覆盖前一次
        let v = SharedVoice::new();
        *v.text.lock().unwrap() = "第一次".into();
        *v.text.lock().unwrap() = "第二次".into();
        let taken = std::mem::take(&mut *v.text.lock().unwrap());
        assert_eq!(taken, "第二次");
        assert!(v.text.lock().unwrap().is_empty());
    }

    #[test]
    fn test_shared_voice_empty_take_returns_empty() {
        // 取空文本不应 panic
        let v = SharedVoice::new();
        let taken = std::mem::take(&mut *v.text.lock().unwrap());
        assert!(taken.is_empty());
    }
}
