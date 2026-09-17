//! 陪伴时长统计的 app 侧：电源通知接收与 IPC。
//!
//! Windows 上起一个 message-only 窗口注册 `GUID_CONSOLE_DISPLAY_STATE`
//! 电源通知，显示器亮/灭事件由 core 的 `screen_time` 模块落盘；非 Windows
//! 只做启动清理，不记录。设置页通过 `cmd_screen_time_summary` 读取聚合。
//!
//! 设计取舍：message-only 窗口收不到广播消息（PBT_APMSUSPEND 等），因此
//! 只依赖直接投递的显示器状态通知——现代 Windows 挂起/恢复同样会触发
//! 显示器 off/on，事件流天然覆盖；不引入常驻轮询。

use serde::Serialize;
use tracing::warn;

/// 陪伴时长事件保留天数。
const KEEP_DAYS: u32 = 30;

/// 设置页 ④ 区卡片与未来 earnings 联动消费的聚合视图。
#[derive(Debug, Clone, Serialize)]
pub struct ScreenTimeSummary {
    pub enabled: bool,
    /// 今天亮屏分钟数（向下取整）。
    pub today_minutes: u64,
    /// 最近 7 天（含今天）亮屏分钟数合计。
    pub week_minutes: u64,
}

/// 启动陪伴时长统计：清理过期事件 + 注册电源通知。
pub fn init() {
    if let Err(e) = bitcat_core::screen_time::cleanup_old_events(KEEP_DAYS) {
        warn!(error = %e, "screen_time cleanup failed");
    }
    #[cfg(target_os = "windows")]
    {
        std::thread::Builder::new()
            .name("bitcat-screen-time".to_string())
            .spawn(power::power_message_loop)
            .expect("failed to spawn screen_time thread");
    }
}

/// 读取当前开关 + 聚合视图（今天 + 最近 7 天）。
pub fn summary() -> ScreenTimeSummary {
    let enabled = bitcat_core::app_settings::AppSettings::load()
        .appearance
        .screen_time_enabled;
    let now = chrono::Local::now();
    let today = now.date_naive();
    let week_ago = today - chrono::Duration::days(6);
    let (events, _) = bitcat_core::screen_time::load_events().unwrap_or_default();
    let days = bitcat_core::screen_time::aggregate_range(&events, week_ago, today, now);
    let today_minutes = days.last().map(|day| day.on_minutes()).unwrap_or(0);
    let week_minutes = days.iter().map(|day| day.on_minutes()).sum();
    ScreenTimeSummary {
        enabled,
        today_minutes,
        week_minutes,
    }
}

#[tauri::command]
pub fn cmd_screen_time_summary() -> ScreenTimeSummary {
    summary()
}

#[cfg(target_os = "windows")]
mod power {
    use bitcat_core::app_settings::AppSettings;
    use bitcat_core::screen_time::{record_event, ScreenPowerEvent};
    use tracing::{info, warn};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::Power::{
        RegisterPowerSettingNotification, POWERBROADCAST_SETTING,
    };
    use windows_sys::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
        TranslateMessage, DEVICE_NOTIFY_WINDOW_HANDLE, HWND_MESSAGE, MSG, WNDCLASSW,
    };

    // windows-sys 未导出的两个常量
    const WM_POWERBROADCAST: u32 = 0x0218;
    const PBT_POWERSETTINGCHANGE: u32 = 0x8013;

    // 显示器状态值（POWERBROADCAST_SETTING.Data 的 DWORD）；2=Dim 按"亮"处理不记录
    const MONITOR_OFF: u32 = 0;
    const MONITOR_ON: u32 = 1;

    // "BitCatScreenTime" 的 UTF-16 字面量（windows-sys 的 PCWSTR 是 *const u16）
    const CLASS_NAME: &[u16] = &[
        0x42, 0x69, 0x74, 0x43, 0x61, 0x74, 0x53, 0x63, 0x72, 0x65, 0x65, 0x6E, 0x54, 0x69, 0x6D,
        0x65, 0,
    ];

    /// 消息循环线程：建 message-only 窗口 → 注册电源通知 → 收消息记事件。
    pub(super) fn power_message_loop() {
        unsafe {
            let instance =
                windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
            let class = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                lpszClassName: CLASS_NAME.as_ptr(),
                hInstance: instance,
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                warn!(
                    error = std::io::Error::last_os_error().to_string(),
                    "screen_time RegisterClassW failed"
                );
                return;
            }
            // message-only 窗口：不可见、不进任务栏、只收直接投递的消息
            let hwnd = CreateWindowExW(
                0,
                CLASS_NAME.as_ptr(),
                CLASS_NAME.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                warn!(
                    error = std::io::Error::last_os_error().to_string(),
                    "screen_time CreateWindowExW failed"
                );
                return;
            }
            let notify = RegisterPowerSettingNotification(
                hwnd,
                &GUID_CONSOLE_DISPLAY_STATE,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            );
            if notify == 0 {
                warn!(
                    error = std::io::Error::last_os_error().to_string(),
                    "screen_time RegisterPowerSettingNotification failed"
                );
                return;
            }
            info!("screen_time power notification registered");

            // 启动时视为亮屏：用户正在启动应用（近似，见模块文档）
            record_if_enabled(ScreenPowerEvent::ScreenOn);

            let mut msg = MSG::default();
            loop {
                let ret = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
                if ret <= 0 {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_POWERBROADCAST && wparam as u32 == PBT_POWERSETTINGCHANGE {
            unsafe {
                let setting = &*(lparam as *const POWERBROADCAST_SETTING);
                if guid_eq(&setting.PowerSetting, &GUID_CONSOLE_DISPLAY_STATE)
                    && setting.DataLength >= 4
                {
                    // Data 是 [u8;1] 柔性数组声明，实际载荷跟在结构体后——
                    // 直接按 Data 起址读 u32，避免字节级索引被 clippy 判越界
                    let state =
                        std::ptr::read_unaligned(std::ptr::addr_of!(setting.Data).cast::<u32>());
                    let event = if state == MONITOR_ON {
                        Some(ScreenPowerEvent::ScreenOn)
                    } else if state == MONITOR_OFF {
                        Some(ScreenPowerEvent::ScreenOff)
                    } else {
                        None
                    };
                    if let Some(event) = event {
                        record_if_enabled(event);
                    }
                }
            }
            return 1;
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn record_if_enabled(event: ScreenPowerEvent) {
        if !AppSettings::load().appearance.screen_time_enabled {
            return;
        }
        if let Err(e) = record_event(event) {
            warn!(error = %e, "screen_time record failed");
        }
    }

    /// windows-sys 的 GUID 未实现 PartialEq，按字段比较。
    fn guid_eq(a: &windows_sys::core::GUID, b: &windows_sys::core::GUID) -> bool {
        (a.data1, a.data2, a.data3, a.data4) == (b.data1, b.data2, b.data3, b.data4)
    }
}
