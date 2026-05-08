use std::collections::HashMap;

/// 按键名 → Windows Virtual Key Code
pub fn parse_keys(keys: &[&str]) -> Vec<u16> {
    static MAP: &[(&str, u16)] = &[
        ("ctrl", 0x11), ("control", 0x11),
        ("win", 0x5B), ("windows", 0x5B),
        ("alt", 0x12),
        ("shift", 0x10),
        ("enter", 0x0D), ("return", 0x0D),
        ("tab", 0x09), ("esc", 0x1B), ("space", 0x20),
        ("backtick", 0xC0), ("`", 0xC0),
        ("a", 0x41), ("b", 0x42), ("c", 0x43), ("d", 0x44),
    ];
    let map: HashMap<&str, u16> = MAP.iter().cloned().collect();

    keys.iter().map(|k| {
        *map.get(k.to_lowercase().as_str()).unwrap_or(&0)
    }).collect()
}

/// 通过 Windows SendInput API 模拟按键组合
/// 先按下所有键，等待 hold 秒，再逆序释放
#[cfg(target_os = "windows")]
pub fn send_hotkey(vk_codes: &[u16], hold: f64) -> Result<(), String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_KEYBOARD,
        KEYEVENTF_KEYUP,
    };
    use std::mem::size_of;

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
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_KEYBOARD};
    use std::mem::size_of;

    let mut input: INPUT = unsafe { std::mem::zeroed() };
    input.r#type = INPUT_KEYBOARD;
    input.Anonymous.ki.wVk = vk;

    let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
    if sent == 0 { return Err("key_down failed".into()); }
    Ok(())
}

/// 释放单个按键
#[cfg(target_os = "windows")]
pub fn key_up(vk: u16) -> Result<(), String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP};
    use std::mem::size_of;

    let mut input: INPUT = unsafe { std::mem::zeroed() };
    input.r#type = INPUT_KEYBOARD;
    input.Anonymous.ki.wVk = vk;
    input.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

    let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
    if sent == 0 { return Err("key_up failed".into()); }
    Ok(())
}

/// 按键名解析为 VK code
pub fn parse_key(name: &str) -> Option<u16> {
    let codes = parse_keys(&[name]);
    codes.into_iter().next().filter(|&v| v != 0)
}

/// 模拟鼠标滚轮滚动
/// delta > 0 向上滚，delta < 0 向下滚，单次典型值 ±120
#[cfg(target_os = "windows")]
pub fn send_scroll(delta: i32) -> Result<(), String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT};
    use std::mem::size_of;

    const INPUT_MOUSE: u32 = 0;
    const MOUSEEVENTF_WHEEL: u32 = 0x0800;

    let mut input: INPUT = unsafe { std::mem::zeroed() };
    input.r#type = INPUT_MOUSE;
    input.Anonymous.mi.dwFlags = MOUSEEVENTF_WHEEL;
    input.Anonymous.mi.mouseData = delta as u32;

    let sent = unsafe {
        SendInput(1, &input, size_of::<INPUT>() as i32)
    };

    if sent == 0 {
        return Err("SendInput scroll failed".into());
    }

    Ok(())
}

/// 模拟鼠标水平滚轮滚动
/// delta > 0 向右滚，delta < 0 向左滚
#[cfg(target_os = "windows")]
pub fn send_scroll_h(delta: i32) -> Result<(), String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT};
    use std::mem::size_of;

    const INPUT_MOUSE: u32 = 0;
    const MOUSEEVENTF_HWHEEL: u32 = 0x1000;

    let mut input: INPUT = unsafe { std::mem::zeroed() };
    input.r#type = INPUT_MOUSE;
    input.Anonymous.mi.dwFlags = MOUSEEVENTF_HWHEEL;
    input.Anonymous.mi.mouseData = delta as u32;

    let sent = unsafe {
        SendInput(1, &input, size_of::<INPUT>() as i32)
    };

    if sent == 0 {
        return Err("SendInput hscroll failed".into());
    }

    Ok(())
}

/// 便捷函数：通过按键名触发热键
pub fn trigger_hotkey(key_names: &[&str], hold: f64) -> Result<(), String> {
    let vk_codes = parse_keys(key_names);
    if vk_codes.iter().any(|&v| v == 0) {
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
}
