//! 屏幕观察门控：汇总显示电源、会话锁定与截图兜底状态。
//!
//! 截图线程不直接理解 Win32 消息，只读取本模块维护的状态并据此决定是否观察屏幕。
//! Windows 消息通过 pet 窗口的 subclass 注入，黑帧检测仍由截图模块作为最后兜底。

use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayPowerState {
    Unknown,
    On,
    Dimmed,
    Off,
}

impl DisplayPowerState {
    pub fn from_power_data(value: u32) -> Self {
        match value {
            0 => Self::Off,
            1 => Self::On,
            2 => Self::Dimmed,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "display_unknown",
            Self::On => "display_on",
            Self::Dimmed => "display_dimmed",
            Self::Off => "display_off",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Unknown,
    Unlocked,
    Locked,
}

impl SessionState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "session_unknown",
            Self::Unlocked => "session_unlocked",
            Self::Locked => "session_locked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationSkipReason {
    DisplayOff,
    DisplayDimmed,
    SessionLocked,
}

impl ObservationSkipReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::DisplayOff => "display_off",
            Self::DisplayDimmed => "display_dimmed",
            Self::SessionLocked => "session_locked",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ObservationGateSnapshot {
    display: DisplayPowerState,
    session: SessionState,
}

impl Default for ObservationGateSnapshot {
    fn default() -> Self {
        Self {
            display: DisplayPowerState::Unknown,
            session: SessionState::Unknown,
        }
    }
}

impl ObservationGateSnapshot {
    fn skip_reason(self) -> Option<ObservationSkipReason> {
        match (self.display, self.session) {
            (_, SessionState::Locked) => Some(ObservationSkipReason::SessionLocked),
            (DisplayPowerState::Off, _) => Some(ObservationSkipReason::DisplayOff),
            (DisplayPowerState::Dimmed, _) => Some(ObservationSkipReason::DisplayDimmed),
            _ => None,
        }
    }
}

pub struct SharedObservationGate {
    state: Mutex<ObservationGateSnapshot>,
}

impl Default for SharedObservationGate {
    fn default() -> Self {
        Self {
            state: Mutex::new(ObservationGateSnapshot::default()),
        }
    }
}

impl SharedObservationGate {
    pub fn skip_reason(&self) -> Option<ObservationSkipReason> {
        self.state.lock().unwrap().skip_reason()
    }

    pub fn set_display_state(&self, new_display: DisplayPowerState) {
        let mut state = self.state.lock().unwrap();
        if state.display != new_display {
            tracing::info!(
                from = state.display.label(),
                to = new_display.label(),
                "屏幕观察门控：显示状态更新"
            );
        }
        state.display = new_display;
    }

    pub fn set_session_state(&self, session: SessionState) {
        let mut state = self.state.lock().unwrap();
        if state.session != session {
            tracing::info!(
                from = state.session.label(),
                to = session.label(),
                "屏幕观察门控：会话状态更新"
            );
        }
        state.session = session;
    }
}

#[cfg(target_os = "windows")]
const OBSERVATION_GATE_SUBCLASS_ID: usize = 0x8B17_0001;

#[cfg(target_os = "windows")]
static OBSERVATION_GATE_PTR: std::sync::atomic::AtomicPtr<SharedObservationGate> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

#[cfg(target_os = "windows")]
pub fn install_windows_observation_hooks(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    use windows_sys::Win32::System::Power::RegisterPowerSettingNotification;
    use windows_sys::Win32::System::RemoteDesktop::{
        WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
    };
    use windows_sys::Win32::System::SystemServices::{
        GUID_CONSOLE_DISPLAY_STATE, GUID_SESSION_DISPLAY_STATUS,
    };
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;
    use windows_sys::Win32::UI::WindowsAndMessaging::DEVICE_NOTIFY_WINDOW_HANDLE;

    let gate: tauri::State<SharedObservationGate> = app.state();
    OBSERVATION_GATE_PTR.store(
        (&*gate as *const SharedObservationGate).cast_mut(),
        std::sync::atomic::Ordering::SeqCst,
    );

    let pet = app
        .get_webview_window("pet")
        .ok_or_else(|| "pet 窗口尚未创建，无法注册屏幕观察门控".to_string())?;
    let hwnd = pet
        .hwnd()
        .map_err(|e| format!("获取 pet HWND 失败: {e}"))?
        .0 as windows_sys::Win32::Foundation::HWND;

    unsafe {
        if SetWindowSubclass(
            hwnd,
            Some(observation_gate_subclass_proc),
            OBSERVATION_GATE_SUBCLASS_ID,
            0,
        ) == 0
        {
            return Err("安装屏幕观察门控 Win32 subclass 失败".into());
        }

        let console_notify = RegisterPowerSettingNotification(
            hwnd,
            &GUID_CONSOLE_DISPLAY_STATE,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        );
        if console_notify == 0 {
            tracing::warn!("注册 GUID_CONSOLE_DISPLAY_STATE 通知失败");
        }

        let session_display_notify = RegisterPowerSettingNotification(
            hwnd,
            &GUID_SESSION_DISPLAY_STATUS,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        );
        if session_display_notify == 0 {
            tracing::warn!("注册 GUID_SESSION_DISPLAY_STATUS 通知失败");
        }

        if WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) == 0 {
            tracing::warn!("注册 WTS 会话通知失败");
        }
    }

    tracing::info!("屏幕观察门控 Win32 hooks 已安装");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn install_windows_observation_hooks(_app: &tauri::AppHandle) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn observation_gate_subclass_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    umsg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
    _uidsubclass: usize,
    _dwrefdata: usize,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::System::Power::POWERBROADCAST_SETTING;
    use windows_sys::Win32::System::SystemServices::{
        GUID_CONSOLE_DISPLAY_STATE, GUID_SESSION_DISPLAY_STATUS,
    };
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        PBT_POWERSETTINGCHANGE, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK,
        WTS_SESSION_UNLOCK,
    };

    if let Some(gate) = observation_gate() {
        if umsg == WM_POWERBROADCAST && wparam as u32 == PBT_POWERSETTINGCHANGE {
            let setting = lparam as *const POWERBROADCAST_SETTING;
            if !setting.is_null() {
                let setting = &*setting;
                if setting.DataLength >= 4
                    && (guid_eq(&setting.PowerSetting, &GUID_CONSOLE_DISPLAY_STATE)
                        || guid_eq(&setting.PowerSetting, &GUID_SESSION_DISPLAY_STATUS))
                {
                    let value = std::ptr::read_unaligned(setting.Data.as_ptr() as *const u32);
                    gate.set_display_state(DisplayPowerState::from_power_data(value));
                }
            }
        } else if umsg == WM_WTSSESSION_CHANGE {
            match wparam as u32 {
                WTS_SESSION_LOCK => gate.set_session_state(SessionState::Locked),
                WTS_SESSION_UNLOCK => gate.set_session_state(SessionState::Unlocked),
                _ => {}
            }
        }
    }

    DefSubclassProc(hwnd, umsg, wparam, lparam)
}

#[cfg(target_os = "windows")]
fn observation_gate() -> Option<&'static SharedObservationGate> {
    let ptr = OBSERVATION_GATE_PTR.load(std::sync::atomic::Ordering::SeqCst);
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &*ptr })
    }
}

#[cfg(target_os = "windows")]
fn guid_eq(left: &windows_sys::core::GUID, right: &windows_sys::core::GUID) -> bool {
    left.data1 == right.data1
        && left.data2 == right.data2
        && left.data3 == right.data3
        && left.data4 == right.data4
}

#[cfg(test)]
mod tests {
    use super::{DisplayPowerState, ObservationGateSnapshot, ObservationSkipReason, SessionState};

    #[test]
    fn power_data_maps_to_display_state() {
        assert_eq!(
            DisplayPowerState::from_power_data(0),
            DisplayPowerState::Off
        );
        assert_eq!(DisplayPowerState::from_power_data(1), DisplayPowerState::On);
        assert_eq!(
            DisplayPowerState::from_power_data(2),
            DisplayPowerState::Dimmed
        );
        assert_eq!(
            DisplayPowerState::from_power_data(99),
            DisplayPowerState::Unknown
        );
    }

    #[test]
    fn locked_session_blocks_observation() {
        let snapshot = ObservationGateSnapshot {
            display: DisplayPowerState::On,
            session: SessionState::Locked,
        };
        assert_eq!(
            snapshot.skip_reason(),
            Some(ObservationSkipReason::SessionLocked)
        );
    }

    #[test]
    fn display_off_blocks_observation() {
        let snapshot = ObservationGateSnapshot {
            display: DisplayPowerState::Off,
            session: SessionState::Unlocked,
        };
        assert_eq!(
            snapshot.skip_reason(),
            Some(ObservationSkipReason::DisplayOff)
        );
    }

    #[test]
    fn active_unknown_states_allow_observation() {
        let snapshot = ObservationGateSnapshot {
            display: DisplayPowerState::Unknown,
            session: SessionState::Unknown,
        };
        assert_eq!(snapshot.skip_reason(), None);
    }
}
