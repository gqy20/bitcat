use std::collections::HashMap;
use std::sync::OnceLock;

/// 按键名 → Windows Virtual Key Code（完整映射表，OnceLock 初始化一次）
fn key_map() -> &'static HashMap<String, u16> {
    static MAP: OnceLock<HashMap<String, u16>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        // 修饰键
        m.insert("ctrl".into(), 0x11);
        m.insert("control".into(), 0x11);
        m.insert("win".into(), 0x5B);
        m.insert("windows".into(), 0x5B);
        m.insert("alt".into(), 0x12);
        m.insert("shift".into(), 0x10);
        // 功能键
        m.insert("enter".into(), 0x0D);
        m.insert("return".into(), 0x0D);
        m.insert("tab".into(), 0x09);
        m.insert("esc".into(), 0x1B);
        m.insert("escape".into(), 0x1B);
        m.insert("space".into(), 0x20);
        m.insert("backspace".into(), 0x08);
        m.insert("delete".into(), 0x2E);
        m.insert("del".into(), 0x2E);
        m.insert("insert".into(), 0x2D);
        m.insert("home".into(), 0x24);
        m.insert("end".into(), 0x23);
        m.insert("pageup".into(), 0x21);
        m.insert("page_up".into(), 0x21);
        m.insert("pagedown".into(), 0x22);
        m.insert("page_down".into(), 0x22);
        // 符号键
        m.insert("backtick".into(), 0xC0);
        m.insert("`".into(), 0xC0);
        m.insert("-".into(), 0xBD);
        m.insert("_".into(), 0xBD);
        m.insert("=".into(), 0xBB);
        m.insert("+".into(), 0xBB);
        m.insert("[".into(), 0xDB);
        m.insert("{".into(), 0xDB);
        m.insert("]".into(), 0xDD);
        m.insert("}".into(), 0xDD);
        m.insert("\\".into(), 0xDC);
        m.insert("|".into(), 0xDC);
        m.insert(";".into(), 0xBA);
        m.insert(":".into(), 0xBA);
        m.insert("'".into(), 0xDE);
        m.insert("\"".into(), 0xDE);
        m.insert(",".into(), 0xBC);
        m.insert("<".into(), 0xBC);
        m.insert(".".into(), 0xBE);
        m.insert(">".into(), 0xBE);
        m.insert("/".into(), 0xBF);
        m.insert("?".into(), 0xBF);
        // 字母 a-z
        for (i, c) in (b'a'..=b'z').enumerate() {
            let s = String::from(char::from(c));
            m.insert(s, 0x41 + i as u16);
        }
        // 数字键
        for (i, c) in (b'0'..=b'9').enumerate() {
            let s = String::from(char::from(c));
            m.insert(s, 0x30 + i as u16);
        }
        // F1-F12
        for i in 1u16..=12u16 {
            m.insert(format!("f{i}"), 0x6F + i);
        }
        m
    })
}

/// 按键名 → Windows Virtual Key Code
pub fn parse_keys(keys: &[&str]) -> Vec<u16> {
    let map = key_map();
    keys.iter()
        .map(|k| *map.get(&k.to_ascii_lowercase()).unwrap_or(&0))
        .collect()
}

/// 通过 Windows SendInput API 模拟按键组合
/// 先按下所有键，等待 hold 秒，再逆序释放
#[cfg(target_os = "windows")]
pub fn send_hotkey(vk_codes: &[u16], hold: f64) -> Result<(), String> {
    use std::mem::size_of;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, SendInput,
    };

    if vk_codes.is_empty() {
        return Ok(());
    }

    let mut inputs: Vec<INPUT> = Vec::with_capacity(vk_codes.len() * 2);

    // 按下所有键
    for &vk in vk_codes {
        let mut input: INPUT = unsafe { std::mem::zeroed() };
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous.ki.wVk = vk;
        input.Anonymous.ki.dwFlags = 0;
        inputs.push(input);
    }

    // 释放所有键（逆序）
    for &vk in vk_codes.iter().rev() {
        let mut input: INPUT = unsafe { std::mem::zeroed() };
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous.ki.wVk = vk;
        input.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
        inputs.push(input);
    }

    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };

    if sent == 0 {
        return Err("SendInput failed".into());
    }

    // 保持按键
    if hold > 0.0 {
        let dur = std::time::Duration::from_secs_f64(hold);
        std::thread::sleep(dur);
    }

    Ok(())
}

/// 按下单个按键（不释放）
#[cfg(target_os = "windows")]
pub fn key_down(vk: u16) -> Result<(), String> {
    use std::mem::size_of;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{INPUT, INPUT_KEYBOARD, SendInput};

    let mut input: INPUT = unsafe { std::mem::zeroed() };
    input.r#type = INPUT_KEYBOARD;
    input.Anonymous.ki.wVk = vk;

    let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
    if sent == 0 {
        return Err("key_down failed".into());
    }
    Ok(())
}

/// 释放单个按键
#[cfg(target_os = "windows")]
pub fn key_up(vk: u16) -> Result<(), String> {
    use std::mem::size_of;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, SendInput,
    };

    let mut input: INPUT = unsafe { std::mem::zeroed() };
    input.r#type = INPUT_KEYBOARD;
    input.Anonymous.ki.wVk = vk;
    input.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

    let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
    if sent == 0 {
        return Err("key_up failed".into());
    }
    Ok(())
}

/// 按键名解析为 VK code
pub fn parse_key(name: &str) -> Option<u16> {
    let codes = parse_keys(&[name]);
    codes.into_iter().next().filter(|&v| v != 0)
}

/// 读取系统剪贴板文本内容 (Windows)
#[cfg(target_os = "windows")]
pub fn read_clipboard() -> Option<String> {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};

    const CF_UNICODETEXT: u32 = 13;

    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
            return None;
        }
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_null() {
            CloseClipboard();
            return None;
        }
        let ptr = GlobalLock(handle);
        if ptr.is_null() {
            CloseClipboard();
            return None;
        }
        let text = {
            // 从宽字符指针构造 String
            let mut len = 0usize;
            let mut p = ptr as *const u16;
            while *p != 0 {
                len += 1;
                p = p.add(1);
            }
            let slice = std::slice::from_raw_parts(ptr as *const u16, len);
            String::from_utf16(slice).ok()
        };
        GlobalUnlock(handle);
        CloseClipboard();
        text
    }
}

#[cfg(not(target_os = "windows"))]
pub fn read_clipboard() -> Option<String> {
    None
}

/// 模拟鼠标滚轮滚动
/// delta > 0 向上滚，delta < 0 向下滚，单次典型值 ±120
#[cfg(target_os = "windows")]
pub fn send_scroll(delta: i32) -> Result<(), String> {
    use std::mem::size_of;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{INPUT, SendInput};

    const INPUT_MOUSE: u32 = 0;
    const MOUSEEVENTF_WHEEL: u32 = 0x0800;

    let mut input: INPUT = unsafe { std::mem::zeroed() };
    input.r#type = INPUT_MOUSE;
    input.Anonymous.mi.dwFlags = MOUSEEVENTF_WHEEL;
    input.Anonymous.mi.mouseData = delta as u32;

    let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };

    if sent == 0 {
        return Err("SendInput scroll failed".into());
    }

    Ok(())
}

/// 模拟鼠标水平滚轮滚动
/// delta > 0 向右滚，delta < 0 向左滚
#[cfg(target_os = "windows")]
pub fn send_scroll_h(delta: i32) -> Result<(), String> {
    use std::mem::size_of;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{INPUT, SendInput};

    const INPUT_MOUSE: u32 = 0;
    const MOUSEEVENTF_HWHEEL: u32 = 0x1000;

    let mut input: INPUT = unsafe { std::mem::zeroed() };
    input.r#type = INPUT_MOUSE;
    input.Anonymous.mi.dwFlags = MOUSEEVENTF_HWHEEL;
    input.Anonymous.mi.mouseData = delta as u32;

    let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };

    if sent == 0 {
        return Err("SendInput hscroll failed".into());
    }

    Ok(())
}

/// 便捷函数：通过按键名触发热键
pub fn trigger_hotkey(key_names: &[&str], hold: f64) -> Result<(), String> {
    let vk_codes = parse_keys(key_names);
    if vk_codes.contains(&0) {
        return Err(format!("包含未知按键: {key_names:?}"));
    }

    #[cfg(target_os = "windows")]
    {
        send_hotkey(&vk_codes, hold)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (&vk_codes, hold);
        Err("SendInput 仅支持 Windows".into())
    }
}

/// 把指定窗口强制提到前台。
///
/// 通过 AttachThreadInput + AllowSetForegroundWindow + SetForegroundWindow 三件套
/// 绕过 Windows 对非用户驱动进程的前台化限制。
/// `hwnd` 由 Tauri 的 `window.hwnd()` 取出后传入（isize 形式）。
#[cfg(target_os = "windows")]
pub fn force_foreground(hwnd: isize) -> Result<(), String> {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SetActiveWindow, SetFocus};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AllowSetForegroundWindow, BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId,
        SetForegroundWindow,
    };

    if hwnd == 0 {
        return Err("hwnd is null".into());
    }
    let target = hwnd as *mut core::ffi::c_void;

    unsafe {
        let fg = GetForegroundWindow();
        let mut fg_pid: u32 = 0;
        let fg_thread = if !fg.is_null() {
            GetWindowThreadProcessId(fg, &mut fg_pid)
        } else {
            0
        };
        let our_thread = GetCurrentThreadId();

        // 把我们的线程附加到前台线程的输入队列，绕过前台化限制
        let attached = if fg_thread != 0 && fg_thread != our_thread {
            AttachThreadInput(our_thread, fg_thread, 1) != 0
        } else {
            false
        };

        // ASFW_ANY = 0xFFFFFFFF：放开任何进程前台化的限制
        let _ = AllowSetForegroundWindow(0xFFFFFFFF);
        let _ = BringWindowToTop(target);
        let _ = SetActiveWindow(target);
        let ok = SetForegroundWindow(target);
        let _ = SetFocus(target);

        if attached {
            let _ = AttachThreadInput(our_thread, fg_thread, 0);
        }

        if ok == 0 {
            Err("SetForegroundWindow failed".into())
        } else {
            Ok(())
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn force_foreground(_hwnd: isize) -> Result<(), String> {
    Err("force_foreground 仅支持 Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ctrl_win() {
        let codes = parse_keys(&["ctrl", "win"]);
        assert_eq!(codes, vec![0x11, 0x5B]);
    }

    #[test]
    fn test_parse_single_key() {
        assert_eq!(parse_keys(&["enter"]), vec![0x0D]);
        assert_eq!(parse_keys(&["space"]), vec![0x20]);
    }

    #[test]
    fn test_parse_case_insensitive() {
        assert_eq!(parse_keys(&["Ctrl"]), vec![0x11]);
        assert_eq!(parse_keys(&["WIN"]), vec![0x5B]);
    }

    #[test]
    fn test_parse_unknown_key() {
        assert_eq!(parse_keys(&["unknown"]), vec![0]);
    }

    #[test]
    fn test_parse_empty() {
        let codes = parse_keys(&[]);
        assert!(codes.is_empty());
    }

    #[test]
    fn test_trigger_hotkey_empty() {
        let result = trigger_hotkey(&[], 0.05);
        assert!(result.is_ok());
    }

    #[test]
    fn test_trigger_hotkey_unknown_key() {
        let result = trigger_hotkey(&["nonexistent"], 0.05);
        assert!(result.is_err());
    }

    #[test]
    fn test_trigger_hotkey_valid_keys() {
        // 不实际发送，只验证解析逻辑
        let codes = parse_keys(&["ctrl", "win"]);
        assert_eq!(codes, vec![0x11, 0x5B]);
        assert!(!codes.iter().any(|&v| v == 0));
    }

    #[test]
    fn test_parse_full_alphabet() {
        // a-d 原来就有，现在验证 e-z 也能解析
        let codes = parse_keys(&["e", "z"]);
        assert_eq!(codes, vec![0x45, 0x5A]);
    }

    #[test]
    fn test_parse_fkeys() {
        // 大小写不敏感，内部统一小写存储
        assert_eq!(parse_keys(&["F1"])[0], 0x70);
        assert_eq!(parse_keys(&["f12"])[0], 0x7B);
        assert_eq!(parse_keys(&["f1"])[0], 0x70);
    }

    #[test]
    fn test_parse_digit_keys() {
        assert_eq!(parse_keys(&["0"])[0], 0x30);
        assert_eq!(parse_keys(&["9"])[0], 0x39);
    }

    #[test]
    fn test_parse_navigation_keys() {
        assert_eq!(parse_keys(&["home"])[0], 0x24);
        assert_eq!(parse_keys(&["end"])[0], 0x23);
        assert_eq!(parse_keys(&["pageup"])[0], 0x21);
        assert_eq!(parse_keys(&["delete"])[0], 0x2E);
    }

    #[test]
    fn test_parse_symbol_keys() {
        assert_eq!(parse_keys(&["-"])[0], 0xBD);
        assert_eq!(parse_keys(&["="])[0], 0xBB);
        assert_eq!(parse_keys(&["["])[0], 0xDB);
    }
}
