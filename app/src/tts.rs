use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, warn};
use windows_sys::Win32::System::Com::*;

static SPEAKING: AtomicBool = AtomicBool::new(false);

const CLSID_SpVoice: windows_sys::core::GUID = windows_sys::core::GUID {
    data1: 0xC8BCA161,
    data2: 0x11D1,
    data3: 0x4B05,
    data4: [0xA5, 0x3B, 0x19, 0xD4, 0xF4, 0xE5, 0xA5, 0x22],
};

const IID_ISpVoice: windows_sys::core::GUID = windows_sys::core::GUID {
    data1: 0x6C44DF74,
    data2: 0x8620,
    data3: 0x4F7D,
    data4: [0x95, 0xAA, 0x00, 0x6F, 0xAB, 0x31, 0xD3, 0x3C],
};

#[repr(C)]
struct ISpVoiceVtbl {
    query_interface: usize,
    add_ref: usize,
    release: unsafe extern "system" fn(this: *mut ISpVoice) -> u32,
    _pad: [usize; 9],
    speak: unsafe extern "system" fn(
        this: *mut ISpVoice,
        pwcs: *const u16,
        dwflags: u32,
        pulstream_number: *mut u32,
    ) -> i32,
}

#[repr(C)]
struct ISpVoice {
    lpVtbl: *const ISpVoiceVtbl,
}

const SVSFDefault: u32 = 0;
const SVFSPurgeBeforeSpeak: u32 = 2;
const S_FALSE: i32 = 1;

fn succeeded(hr: i32) -> bool { hr >= 0 }

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
            &CLSID_SpVoice,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_ISpVoice,
            &mut voice,
        );
        if !succeeded(hr) {
            CoUninitialize();
            return Err(format!("创建 ISpVoice 失败: 0x{hr:X}"));
        }

        let wide_text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let voice_ptr = voice as *mut ISpVoice;
        let hr = ((*(*voice_ptr).lpVtbl).speak)(
            voice_ptr,
            wide_text.as_ptr(),
            SVSFDefault | SVFSPurgeBeforeSpeak,
            std::ptr::null_mut(),
        );

        // IUnknown::Release (vtable index 2)
        let release_fn: unsafe extern "system" fn(this: *mut ISpVoice) -> u32 =
            std::mem::transmute((*(*voice_ptr).lpVtbl).release);
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
