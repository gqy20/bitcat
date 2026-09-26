//! 查询原生拖拽期间的鼠标主键状态。
//! 原生窗口移动可能吞掉 WebView 的 pointerup，不能用位置静止代替松手信号。
//! 本模块向宠物前端提供短时按需查询，其他平台由前端指针事件处理。

/// Windows 上返回当前主鼠标键是否按住；不支持的平台返回 None。
/// 仅使用按下高位，并尊重系统左右键交换设置。
#[cfg(target_os = "windows")]
pub fn primary_button_down() -> Option<bool> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};

    // SAFETY: Both functions accept scalar values and retain no pointers.
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    let button = if swapped { VK_RBUTTON } else { VK_LBUTTON };
    let state = unsafe { GetAsyncKeyState(i32::from(button)) };
    Some((state as u16 & 0x8000) != 0)
}

/// 非 Windows 平台不把未知状态伪装成松手。
#[cfg(not(target_os = "windows"))]
pub fn primary_button_down() -> Option<bool> {
    None
}

/// 只在一次原生拖拽进行期间由前端轮询，空闲时不查询。
#[tauri::command]
pub fn cmd_pet_drag_button_down() -> Option<bool> {
    primary_button_down()
}

#[cfg(all(test, not(target_os = "windows")))]
mod tests {
    #[test]
    fn unsupported_platform_returns_unknown() {
        assert_eq!(super::primary_button_down(), None);
    }
}
