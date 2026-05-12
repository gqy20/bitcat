use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, warn};
use windows_sys::Win32::System::Com::*;

static SPEAKING: AtomicBool = AtomicBool::new(false);

const CLSID_SP_VOICE: windows_sys::core::GUID = windows_sys::core::GUID {
    data1: 0x96749377,
    data2: 0x3391,
    data3: 0x11D2,
    data4: [0x9E, 0xE3, 0x00, 0xC0, 0x4F, 0x79, 0x73, 0x96],
};

const IID_ISP_VOICE: windows_sys::core::GUID = windows_sys::core::GUID {
    data1: 0x6C44DF74,
    data2: 0x72B9,
    data3: 0x4992,
    data4: [0xA1, 0xEC, 0xEF, 0x99, 0x6E, 0x04, 0x22, 0xD4],
};

/// ISpVoice vtable 布局
/// 继承链：IUnknown(3) → ISpNotifySource(7) → ISpEventSource(3) → ISpVoice
/// Speak 是 ISpVoice 的第 8 个方法，整体 vtable index = 3+7+3+7 = 20
#[repr(C)]
struct ISpVoiceVtbl {
    // IUnknown
    query_interface: usize,
    add_ref: usize,
    release: unsafe extern "system" fn(this: *mut ISpVoice) -> u32,
    // ISpNotifySource (7)
    set_notify_sink: usize,
    set_notify_window_message: usize,
    set_notify_callback_function: usize,
    set_notify_callback_interface: usize,
    set_notify_win32_event: usize,
    wait_for_notify_event: usize,
    get_notify_event_handle: usize,
    // ISpEventSource (3)
    set_interest: usize,
    get_events: usize,
    get_info: usize,
    // ISpVoice（Speak 前 7 个）
    set_output: usize,
    get_output_object_token: usize,
    get_output_stream: usize,
    pause: usize,
    resume: usize,
    set_voice: usize,
    get_voice: usize,
    // Speak (vtable index 20)
    speak: unsafe extern "system" fn(
        this: *mut ISpVoice,
        pwcs: *const u16,
        dwflags: u32,
        pulstream_number: *mut u32,
    ) -> i32,
}

#[repr(C)]
struct ISpVoice {
    lp_vtbl: *const ISpVoiceVtbl,
}

const SVSF_DEFAULT: u32 = 0;
const SVSF_PURGE_BEFORE_SPEAK: u32 = 2;
const S_FALSE: i32 = 1;

fn succeeded(hr: i32) -> bool {
    hr >= 0
}

pub fn speak(text: &str) {
    if text.is_empty() {
        return;
    }
    if SPEAKING.swap(true, Ordering::SeqCst) {
        debug!("TTS: 上一次朗读尚未结束，跳过");
        return;
    }

    let result = do_speak(text);
    SPEAKING.store(false, Ordering::SeqCst);
    if let Err(e) = result {
        warn!(error = %e, "TTS 播放失败");
    }
}

fn do_speak(text: &str) -> Result<(), String> {
    unsafe {
        let hr = CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32);
        if !succeeded(hr) && hr != S_FALSE {
            return Err(format!("CoInitializeEx 失败: 0x{hr:X}"));
        }

        let mut voice: *mut c_void = std::ptr::null_mut();
        let hr = CoCreateInstance(
            &CLSID_SP_VOICE,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_ISP_VOICE,
            &mut voice,
        );
        if !succeeded(hr) {
            CoUninitialize();
            return Err(format!("创建 ISpVoice 失败: 0x{hr:X}"));
        }

        let wide_text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let voice_ptr = voice as *mut ISpVoice;
        let hr = ((*(*voice_ptr).lp_vtbl).speak)(
            voice_ptr,
            wide_text.as_ptr(),
            SVSF_DEFAULT | SVSF_PURGE_BEFORE_SPEAK,
            std::ptr::null_mut(),
        );

        // IUnknown::Release (vtable index 2)
        let release_fn: unsafe extern "system" fn(this: *mut ISpVoice) -> u32 =
            std::mem::transmute((*(*voice_ptr).lp_vtbl).release);
        release_fn(voice_ptr);
        CoUninitialize();

        if succeeded(hr) {
            Ok(())
        } else {
            Err(format!("Speak 失败: 0x{hr:X}"))
        }
    }
}

pub fn stop() {
    SPEAKING.store(false, Ordering::SeqCst);
}

pub fn is_speaking() -> bool {
    SPEAKING.load(Ordering::SeqCst)
}
