// voice.rs — 可见的语音输入条窗口,接收任意语音输入法注入的文字
//
// 流程:
// 1. App 启动时预创建 voice 窗口 (visible:true 但放在屏幕外, 用户看不到)
// 2. 用户按住手柄 voice 键 → 移到屏幕中下 + 强制前台化 + emit 'voice-clear'
//    → 模拟用户配置的输入法语音热键 → 输入法识别文字注入到 textarea
// 3. 用户松开手柄 voice 键 → 等识别引擎完成注入 → 取走 textarea 内容 → 送 AI
// 4. 取走文本后窗口移回屏幕外,等待下次使用
//
// 防残留机制: generation 计数器
//   open_voice_capture 时递增 generation 并清空文本
//   cmd_voice_update_text 写入时附带当前 generation
//   take_voice_text 只接受匹配当前 generation 的文本（拒绝旧会话残留）

//! 语音输入模块：通过手柄 voice 键触发系统输入法语音识别，将文字送入 AI 对话。
//!
//! 设计核心是 **generation 防残留机制**——每次按下 voice 键递增全局计数器，
//! `take_voice_text` 只接受与当前 generation 匹配的文本，旧会话残留会被丢弃。
//! 这解决了输入法异步注入跨越松键时序窗口的问题。
//!
//! 窗口采用"预创建 + 屏幕外隐藏"策略：启动时即创建 280×40 透明窗口
//! （visible: true 以便成为合法焦点目标），按下时移入屏幕，松开后归位。
//! 所有操作均在手柄主循环线程上同步执行。

use std::sync::Mutex;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindowBuilder,
};
use tracing::{info, warn};

const VOICE_W: u32 = 280;
const VOICE_H: u32 = 40;
const OFFSCREEN: i32 = -10000;

/// 带版本号的文本条目，generation 用于防残留校验。
///
/// 写入时附带当前 generation，取走时只接受与全局 generation 匹配的条目。
#[derive(Debug, Clone, Default)]
struct VoiceEntry {
    text: String,
    generation: u64,
}

/// 语音输入共享状态，由 `Mutex` 保护，在手柄循环和 Tauri 命令间传递。
pub struct SharedVoice {
    entry: Mutex<VoiceEntry>,
    /// 全局递增计数，每次 open_voice_capture 时 +1
    pub generation: Mutex<u64>,
}

impl SharedVoice {
    pub fn new() -> Self {
        Self {
            entry: Mutex::new(VoiceEntry {
                text: String::new(),
                generation: 0,
            }),
            generation: Mutex::new(0),
        }
    }
}

impl Default for SharedVoice {
    fn default() -> Self {
        Self::new()
    }
}

/// 启动时预创建 voice 窗口，放在屏幕外（visible: true 才能成为合法焦点目标）。
///
/// 避免首次按下 voice 键时创建窗口导致输入法抢不到焦点的时序竞态。
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
        .background_color(tauri::webview::Color(0, 0, 0, 0))
        .shadow(false)
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

/// voice 按下处理：递增 generation + 清空状态 + 窗口移到屏幕中下 +
/// `AttachThreadInput` 强制前台化 + 通知前端清空 textarea。
pub fn open_voice_capture(app: &AppHandle) -> Result<(), String> {
    let state: State<SharedVoice> = app.state();
    // 新会话: 递增 generation + 清空文本
    let mut gen = state.generation.lock().map_err(|e| e.to_string())?;
    *gen += 1;
    let new_gen = *gen;
    drop(gen);
    *state.entry.lock().map_err(|e| e.to_string())? = VoiceEntry {
        text: String::new(),
        generation: new_gen,
    };
    info!(generation = new_gen, "[voice] 新语音会话开始");

    let window = match app.get_webview_window("voice") {
        Some(w) => w,
        None => {
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
        warn!(error = %e, "[voice] emit voice-clear 失败");
    } else {
        info!("[voice] ✓ 已发送 voice-clear");
    }

    // 用 AttachThreadInput 强制前台化,绕过 Windows 安全限制
    #[cfg(target_os = "windows")]
    {
        if let Ok(hwnd) = window.hwnd() {
            let hwnd_raw = hwnd.0 as isize;
            if let Err(e) = ai_pad_core::hotkey::force_foreground(hwnd_raw) {
                warn!(error = %e, "[voice] force_foreground 失败");
            }
        }
    }

    Ok(())
}

/// voice 释放处理：eval 取值 + 等待注入完成 + generation 校验 + 取走文本 + 窗口归位。
///
/// 如果首次取值为空且 eval 成功，会短暂等待后重试一次，以应对输入法延迟注入。
/// generation 不匹配的旧文本会被丢弃并记录 warn 日志。
pub fn take_voice_text(app: &AppHandle) -> Result<String, String> {
    let mut eval_ok = true;

    // 当前活跃 generation — 只接受匹配此值的文本
    let state: State<SharedVoice> = app.state();
    let current_gen = *state.generation.lock().map_err(|e| e.to_string())?;

    // Step 1: eval — 在 WebView2 中原子地读取 textarea + 清空 + 上报
    if let Some(window) = app.get_webview_window("voice") {
        match window.eval(
            r#"(async()=>{const t=document.getElementById('vox');if(t){const v=t.value;t.value='';await window.__TAURI__.core.invoke('cmd_voice_update_text',{text:v});}})()"#
        ) {
            Ok(()) => {}
            Err(e) => {
                eval_ok = false;
                warn!(error = %e, "[voice] eval 失败（将依赖 input 事件兜底）");
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    } else {
        warn!("[voice] ⚠ voice 窗口不存在");
        eval_ok = false;
    }

    // Step 2: 取走文本，校验 generation
    let entry = std::mem::take(&mut *state.entry.lock().map_err(|e| e.to_string())?);

    if entry.generation != current_gen && !entry.text.is_empty() {
        warn!(
            entry_gen = entry.generation,
            current_gen,
            stale_text = %entry.text,
            "[voice] 拒绝残留旧文本 (generation 不匹配)"
        );
    }

    let text = if entry.generation == current_gen {
        entry.text
    } else {
        String::new()
    };

    // Step 3: 如果文本为空且 eval 成功了，再等一轮重试
    if text.is_empty() && eval_ok {
        if let Some(_window) = app.get_webview_window("voice") {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let retry_entry = std::mem::take(&mut *state.entry.lock().map_err(|e| e.to_string())?);
            if !retry_entry.text.is_empty() && retry_entry.generation == current_gen {
                info!(
                    chars = retry_entry.text.chars().count(),
                    "[voice] 重试取值成功"
                );
                return Ok(retry_entry.text);
            } else if !retry_entry.text.is_empty() {
                warn!(
                    entry_gen = retry_entry.generation,
                    current_gen, "[voice] 重试取到的也是旧文本，丢弃"
                );
            }
        }
    }

    // Step 4: 日志 + 归位
    info!(
        chars = text.chars().count(),
        eval_ok,
        entry_gen = entry.generation,
        current_gen,
        source = if text.is_empty() { "空" } else { "有内容" },
        fresh = entry.generation == current_gen,
        "[voice] 取走文本"
    );

    if let Some(window) = app.get_webview_window("voice") {
        let _ = window.set_position(PhysicalPosition::new(OFFSCREEN, OFFSCREEN));
    }

    // Step 5: 再次确保清空（防止残留）
    if let Some(window) = app.get_webview_window("voice") {
        let _ = window
            .eval(r#"if(document.getElementById('vox'))document.getElementById('vox').value=''"#);
    }

    Ok(text)
}

/// 前端通过 eval 注入调用：将当前输入法文本写入共享状态，附带当前 generation。
#[tauri::command]
pub async fn cmd_voice_update_text(
    state: State<'_, SharedVoice>,
    text: String,
) -> Result<(), String> {
    let gen = *state.generation.lock().map_err(|e| e.to_string())?;
    *state.entry.lock().map_err(|e| e.to_string())? = VoiceEntry {
        text,
        generation: gen,
    };
    Ok(())
}

/// 前端读取当前语音文本（不消费），调试用。
#[tauri::command]
pub async fn cmd_voice_get_text(state: State<'_, SharedVoice>) -> Result<String, String> {
    let entry = state.entry.lock().map_err(|e| e.to_string())?;
    Ok(entry.text.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_voice_default_empty() {
        let v = SharedVoice::new();
        assert!(v.entry.lock().unwrap().text.is_empty());
        assert_eq!(v.entry.lock().unwrap().generation, 0);
    }

    #[test]
    fn test_shared_voice_take_clears() {
        let v = SharedVoice::new();
        *v.entry.lock().unwrap() = VoiceEntry {
            text: "你好世界".into(),
            generation: 1,
        };
        let taken = std::mem::take(&mut *v.entry.lock().unwrap());
        assert_eq!(taken.text, "你好世界");
        assert!(v.entry.lock().unwrap().text.is_empty());
    }

    #[test]
    fn test_voice_constants_reasonable() {
        assert!(VOICE_W >= 200 && VOICE_W <= 500);
        assert!(VOICE_H >= 30 && VOICE_H <= 80);
    }

    #[test]
    fn test_offscreen_constant_far_enough() {
        assert!(OFFSCREEN < -5000);
    }

    #[test]
    fn test_shared_voice_overwrite() {
        let v = SharedVoice::new();
        *v.generation.lock().unwrap() = 1;
        *v.entry.lock().unwrap() = VoiceEntry {
            text: "第一次".into(),
            generation: 1,
        };
        *v.entry.lock().unwrap() = VoiceEntry {
            text: "第二次".into(),
            generation: 1,
        };
        let taken = std::mem::take(&mut *v.entry.lock().unwrap());
        assert_eq!(taken.text, "第二次");
        assert!(v.entry.lock().unwrap().text.is_empty());
    }

    #[test]
    fn test_shared_voice_empty_take_returns_empty() {
        let v = SharedVoice::new();
        let taken = std::mem::take(&mut *v.entry.lock().unwrap());
        assert!(taken.text.is_empty());
    }

    #[test]
    fn test_generation_isolation_rejects_stale_text() {
        // 模拟: gen=1 时写入的旧文本，在 gen=2 时应被拒绝
        let v = SharedVoice::new();
        *v.generation.lock().unwrap() = 1;
        *v.entry.lock().unwrap() = VoiceEntry {
            text: "旧文本".into(),
            generation: 1,
        };

        // 模拟新会话开始 (generation 递增)
        *v.generation.lock().unwrap() = 2;
        let current_gen = *v.generation.lock().unwrap();

        let entry = std::mem::take(&mut *v.entry.lock().unwrap());
        assert!(entry.generation != current_gen); // 旧文本 gen=1 ≠ current_gen=2

        // 模拟 take_voice_text 的过滤逻辑
        let accepted = if entry.generation == current_gen {
            entry.text
        } else {
            String::new()
        };
        assert!(accepted.is_empty(), "旧文本应被丢弃");
    }

    #[test]
    fn test_generation_accepts_fresh_text() {
        let v = SharedVoice::new();
        *v.generation.lock().unwrap() = 2;
        *v.entry.lock().unwrap() = VoiceEntry {
            text: "新文本".into(),
            generation: 2,
        };

        let current_gen = *v.generation.lock().unwrap();
        let entry = std::mem::take(&mut *v.entry.lock().unwrap());
        assert_eq!(entry.generation, current_gen);

        let accepted = if entry.generation == current_gen {
            entry.text
        } else {
            String::new()
        };
        assert_eq!(accepted, "新文本");
    }

    #[test]
    fn test_open_capture_increments_generation() {
        // 验证连续两次 open 会递增 generation
        let v = SharedVoice::new();
        assert_eq!(*v.generation.lock().unwrap(), 0);

        // 模拟 open 的效果
        let mut gen = v.generation.lock().unwrap();
        *gen += 1;
        let g1 = *gen;
        drop(gen);
        *v.entry.lock().unwrap() = VoiceEntry {
            text: String::new(),
            generation: g1,
        };
        assert_eq!(g1, 1);

        // 第二次 open
        let mut gen = v.generation.lock().unwrap();
        *gen += 1;
        let g2 = *gen;
        drop(gen);
        *v.entry.lock().unwrap() = VoiceEntry {
            text: String::new(),
            generation: g2,
        };
        assert_eq!(g2, 2);
        assert!(g2 > g1);
    }
}
