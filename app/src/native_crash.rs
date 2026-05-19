//! Native crash diagnostics for Windows builds.
//!
//! Rust panics are handled by the normal panic hook, but SDL2, WebView2, GPU,
//! or other native code can terminate through a structured exception instead.
//! This module registers a top-level Windows exception filter that writes a
//! minidump and a small text log into the same directory as the app logs.

use std::path::PathBuf;

#[cfg(target_os = "windows")]
mod imp {
    use super::PathBuf;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};
    use windows_sys::Win32::Foundation::TRUE;
    use windows_sys::Win32::System::Diagnostics::Debug::{
        MiniDumpWithThreadInfo, MiniDumpWithUnloadedModules, MiniDumpWriteDump,
        SetUnhandledExceptionFilter, EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS,
        MINIDUMP_EXCEPTION_INFORMATION,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId,
    };

    static CRASH_LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

    pub fn install(log_dir: PathBuf) -> Result<(), String> {
        fs::create_dir_all(&log_dir).map_err(|e| format!("create crash log dir failed: {e}"))?;
        let _ = CRASH_LOG_DIR.set(log_dir);
        // SAFETY: `native_exception_filter` is an extern "system" function with
        // the signature required by SetUnhandledExceptionFilter and remains
        // valid for the entire process lifetime.
        unsafe {
            SetUnhandledExceptionFilter(Some(native_exception_filter));
        }
        Ok(())
    }

    unsafe extern "system" fn native_exception_filter(info: *const EXCEPTION_POINTERS) -> i32 {
        let _ = write_crash_artifacts(info);
        EXCEPTION_CONTINUE_SEARCH
    }

    fn write_crash_artifacts(info: *const EXCEPTION_POINTERS) -> Result<(), String> {
        let dir = CRASH_LOG_DIR
            .get()
            .cloned()
            .ok_or_else(|| "crash log dir not initialized".to_string())?;
        fs::create_dir_all(&dir).map_err(|e| format!("create crash log dir failed: {e}"))?;

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let process_id = unsafe { GetCurrentProcessId() };
        let thread_id = unsafe { GetCurrentThreadId() };
        let dump_path = dir.join(format!("crash-{now_secs}-{process_id}.dmp"));
        let dump_ok = write_minidump(&dump_path, info, thread_id);
        append_crash_log(
            &dir, now_secs, process_id, thread_id, info, &dump_path, dump_ok,
        )
    }

    fn write_minidump(
        path: &PathBuf,
        info: *const EXCEPTION_POINTERS,
        thread_id: u32,
    ) -> Result<(), String> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|e| format!("create minidump failed: {e}"))?;

        let mut exception_info = MINIDUMP_EXCEPTION_INFORMATION {
            ThreadId: thread_id,
            ExceptionPointers: info as *mut EXCEPTION_POINTERS,
            ClientPointers: TRUE,
        };
        let dump_type = MiniDumpWithThreadInfo | MiniDumpWithUnloadedModules;

        // SAFETY: the file handle stays alive during the call, process/thread ids
        // come from the current process, and Windows passes a valid exception
        // pointer to the top-level filter for the duration of the callback.
        let ok = unsafe {
            MiniDumpWriteDump(
                GetCurrentProcess(),
                GetCurrentProcessId(),
                file.as_raw_handle(),
                dump_type,
                &mut exception_info,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if ok == 0 {
            return Err(format!(
                "MiniDumpWriteDump failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    fn append_crash_log(
        dir: &PathBuf,
        now_secs: u64,
        process_id: u32,
        thread_id: u32,
        info: *const EXCEPTION_POINTERS,
        dump_path: &PathBuf,
        dump_result: Result<(), String>,
    ) -> Result<(), String> {
        let (exception_code, exception_address) = exception_details(info);
        let dump_status = match dump_result {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("failed: {e}"),
        };
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("native-crash.log"))
            .map_err(|e| format!("open native crash log failed: {e}"))?;
        writeln!(
            file,
            "time_unix={now_secs} pid={process_id} tid={thread_id} exception_code={exception_code} exception_address={exception_address} dump={} dump_status={dump_status}",
            dump_path.display()
        )
        .map_err(|e| format!("write native crash log failed: {e}"))
    }

    fn exception_details(info: *const EXCEPTION_POINTERS) -> (String, String) {
        if info.is_null() {
            return ("<null>".to_string(), "<null>".to_string());
        }
        let record = unsafe { (*info).ExceptionRecord };
        if record.is_null() {
            return ("<null>".to_string(), "<null>".to_string());
        }
        let code = unsafe { (*record).ExceptionCode as u32 };
        let address = unsafe { (*record).ExceptionAddress };
        (format!("0x{code:08X}"), format!("{address:p}"))
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::PathBuf;

    pub fn install(_log_dir: PathBuf) -> Result<(), String> {
        Ok(())
    }
}

/// Install the platform native crash handler.
pub fn install_native_crash_handler(log_dir: PathBuf) -> Result<(), String> {
    imp::install(log_dir)
}
