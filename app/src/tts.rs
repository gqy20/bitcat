//! Windows SAPI 文本转语音模块。
//!
//! 通过 COM 互操作调用系统自带的 `ISpVoice` 接口实现朗读。
//! `speak()` 是同步阻塞调用——会等到 SAPI 朗读完毕才返回，
//! 因此必须在独立线程（而非 async runtime 线程）上调用。
//! 模块内部用 `AtomicBool` 防止并发朗读，调用方无需额外加锁。
//! 仅与 `bridge`（按键→命令分发）交互：收到 TTS 请求时调用 `speak()`。

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, warn};
use windows_sys::Win32::System::Com::*;

/// 全局朗读锁：`true` 表示正在朗读，后续请求直接跳过。
/// 使用 `SeqCst` 排序保证与 COM 操作的可见性。
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

/// `ISpVoice` vtable 布局（手写，绕过 windows-sys 未暴露的 COM 接口）。
///
/// 继承链：`IUnknown`(3) → `ISpNotifySource`(7) → `ISpEventSource`(3) → `ISpVoice`。
/// `Speak` 是 `ISpVoice` 的第 8 个方法，整体 vtable index = 3+7+3+7 = 20。
///
/// # Safety
///
/// 字段顺序必须严格匹配 Windows COM vtable 布局；任何偏移都会导致
/// 调用错误的虚函数指针，引发未定义行为或进程崩溃。
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

/// COM `ISpVoice` 对象的 Rust 投影：仅包含指向 vtable 的指针。
#[repr(C)]
struct ISpVoice {
    lp_vtbl: *const ISpVoiceVtbl,
}

const SVSF_DEFAULT: u32 = 0;
const SVSF_PURGE_BEFORE_SPEAK: u32 = 2;
const S_FALSE: i32 = 1;

/// 判断 COM `HRESULT` 是否表示成功（`>= 0`）。
fn succeeded(hr: i32) -> bool {
    hr >= 0
}

/// 朗读指定文本；空文本静默返回，已有朗读进行中则跳过。
///
/// **阻塞**：内部调用 SAPI `Speak` 会等待朗读完毕才返回，
/// 不得在 Tokio async runtime 线程上直接调用，应 `std::thread::spawn` 后使用。
/// `SVSF_PURGE_BEFORE_SPEAK` 会打断上一次未完成的朗读。
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

/// 底层 SAPI 调用：初始化 COM → 创建 `ISpVoice` → 同步 `Speak` → 释放。
///
/// # Safety
///
/// - `CoInitializeEx` / `CoUninitialize` 必须成对调用；当前使用 `COINIT_MULTITHREADED`，
///   因此不能在已初始化为单线程单元（STA）的线程上调用，否则会返回冲突错误。
/// - vtable 指针解引用前已通过 `CoCreateInstance` 验证 `voice` 非空；
///   `release` 函数指针取自 vtable 固定偏移（index 2），布局由 `ISpVoiceVtbl` 保证。
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
            (*(*voice_ptr).lp_vtbl).release;
        release_fn(voice_ptr);
        CoUninitialize();

        if succeeded(hr) {
            Ok(())
        } else {
            Err(format!("Speak 失败: 0x{hr:X}"))
        }
    }
}

/// 强制清除朗读状态标志，使后续 `speak()` 调用不会被"上一次未结束"拦截。
/// 注意：这只是清除标志，不会中断正在执行的 SAPI `Speak` 调用。
pub fn stop() {
    SPEAKING.store(false, Ordering::SeqCst);
}

/// 查询当前是否正在朗读。
pub fn is_speaking() -> bool {
    SPEAKING.load(Ordering::SeqCst)
}
