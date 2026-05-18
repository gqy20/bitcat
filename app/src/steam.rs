//! Steamworks startup probe for local SDK and AppID validation.
//! The module dynamically loads `steam_api64.dll` and calls the minimal C ABI.
//! App startup uses this for diagnostics; richer Steam features can grow here later.

use std::ffi::{c_char, CStr};
use std::path::PathBuf;
use std::sync::OnceLock;

use libloading::Library;
use tracing::{info, warn};

type SteamApiInitFlat = unsafe extern "C" fn(*mut c_char) -> i32;
type SteamApiShutdown = unsafe extern "C" fn();
type SteamApiIsSteamRunning = unsafe extern "C" fn() -> bool;
type SteamApiGetHSteamPipe = unsafe extern "C" fn() -> i32;
type SteamApiGetHSteamUser = unsafe extern "C" fn() -> i32;

static STEAM_RUNTIME: OnceLock<SteamRuntime> = OnceLock::new();

struct SteamRuntime {
    _library: Library,
    shutdown: SteamApiShutdown,
}

impl Drop for SteamRuntime {
    fn drop(&mut self) {
        unsafe {
            (self.shutdown)();
        }
    }
}

/// Initializes the Steamworks probe and writes a diagnostic log entry.
///
/// Failure is non-fatal so non-Steam builds and local dev sessions can still run.
pub fn init_probe() {
    match unsafe { init_runtime() } {
        Ok(report) => {
            info!(
                steam_running = report.steam_running,
                steam_user = report.steam_user,
                steam_pipe = report.steam_pipe,
                "Steamworks SDK initialized"
            );
        }
        Err(error) => {
            warn!(%error, "Steamworks SDK initialization skipped/failed");
        }
    }
}

struct SteamProbeReport {
    steam_running: bool,
    steam_user: i32,
    steam_pipe: i32,
}

unsafe fn init_runtime() -> Result<SteamProbeReport, String> {
    if STEAM_RUNTIME.get().is_some() {
        return Err("Steamworks runtime is already initialized".to_string());
    }

    let dll_path = steam_api_dll_path();
    if !dll_path.exists() {
        return Err(format!(
            "steam_api64.dll was not found next to the exe: {}",
            dll_path.display()
        ));
    }

    let library = unsafe { Library::new(&dll_path) }
        .map_err(|e| format!("failed to load {}: {e}", dll_path.display()))?;

    let init_flat: SteamApiInitFlat = unsafe {
        *library
            .get::<SteamApiInitFlat>(b"SteamAPI_InitFlat\0")
            .map_err(|e| format!("failed to find SteamAPI_InitFlat: {e}"))?
    };
    let shutdown: SteamApiShutdown = unsafe {
        *library
            .get::<SteamApiShutdown>(b"SteamAPI_Shutdown\0")
            .map_err(|e| format!("failed to find SteamAPI_Shutdown: {e}"))?
    };
    let is_steam_running: SteamApiIsSteamRunning = unsafe {
        *library
            .get::<SteamApiIsSteamRunning>(b"SteamAPI_IsSteamRunning\0")
            .map_err(|e| format!("failed to find SteamAPI_IsSteamRunning: {e}"))?
    };
    let get_hsteam_pipe: SteamApiGetHSteamPipe = unsafe {
        *library
            .get::<SteamApiGetHSteamPipe>(b"SteamAPI_GetHSteamPipe\0")
            .map_err(|e| format!("failed to find SteamAPI_GetHSteamPipe: {e}"))?
    };
    let get_hsteam_user: SteamApiGetHSteamUser = unsafe {
        *library
            .get::<SteamApiGetHSteamUser>(b"SteamAPI_GetHSteamUser\0")
            .map_err(|e| format!("failed to find SteamAPI_GetHSteamUser: {e}"))?
    };

    let steam_running = unsafe { is_steam_running() };
    let mut err_msg = [0 as c_char; 1024];
    let init_result = unsafe { init_flat(err_msg.as_mut_ptr()) };
    if init_result != 0 {
        let detail = unsafe { CStr::from_ptr(err_msg.as_ptr()) }
            .to_string_lossy()
            .trim()
            .to_string();
        return Err(format!(
            "SteamAPI_InitFlat returned {init_result}; steam_running={steam_running}; {detail}; ensure Steam is signed in and steam_appid.txt is next to the exe"
        ));
    }

    let steam_pipe = unsafe { get_hsteam_pipe() };
    let steam_user = unsafe { get_hsteam_user() };
    STEAM_RUNTIME
        .set(SteamRuntime {
            _library: library,
            shutdown,
        })
        .map_err(|_| "failed to store Steamworks runtime".to_string())?;

    Ok(SteamProbeReport {
        steam_running,
        steam_user,
        steam_pipe,
    })
}

fn steam_api_dll_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("steam_api64.dll")))
        .unwrap_or_else(|| PathBuf::from("steam_api64.dll"))
}
