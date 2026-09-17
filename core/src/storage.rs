//! Local storage path resolution for BitCat runtime data.
//!
//! The app keeps the settings file in the platform config directory so it can
//! always be found before user preferences are loaded. Runtime data paths are
//! resolved from `AppSettings.storage`, falling back to the current BitCat
//! defaults without probing or importing directories from the old project name.

use std::path::PathBuf;

use crate::app_settings::AppSettings;

/// Storage roots exposed to the settings UI.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoragePaths {
    pub data_dir: String,
    pub app_data_dir: String,
    pub default_data_dir: String,
    pub default_app_data_dir: String,
}

/// Return the high-volume runtime data root.
///
/// This contains logs, memory, screenshots, and camera observations.
pub fn data_dir() -> Result<PathBuf, String> {
    let settings = AppSettings::load();
    if let Some(path) = non_empty_path(settings.storage.data_dir.as_deref()) {
        return Ok(path);
    }
    default_data_dir()
}

/// Return the smaller application data root.
///
/// This contains user-authored app data such as reminders and dances. The
/// settings file itself intentionally remains in `app_settings::settings_path`.
pub fn app_data_dir() -> Result<PathBuf, String> {
    let settings = AppSettings::load();
    if let Some(path) = non_empty_path(settings.storage.app_data_dir.as_deref()) {
        return Ok(path);
    }
    default_app_data_dir()
}

/// Return paths suitable for rendering in the settings UI.
pub fn storage_paths() -> Result<StoragePaths, String> {
    Ok(StoragePaths {
        data_dir: data_dir()?.to_string_lossy().into_owned(),
        app_data_dir: app_data_dir()?.to_string_lossy().into_owned(),
        default_data_dir: default_data_dir()?.to_string_lossy().into_owned(),
        default_app_data_dir: default_app_data_dir()?.to_string_lossy().into_owned(),
    })
}

/// Return the default high-volume runtime data root, currently `~/.bitcat`.
pub fn default_data_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("USERPROFILE")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .or_else(dirs::home_dir)
        .ok_or_else(|| "unable to resolve home directory".to_string())?;
    Ok(home.join(".bitcat"))
}

/// Return the default app data root, currently the platform data dir + `bitcat`.
pub fn default_app_data_dir() -> Result<PathBuf, String> {
    dirs::data_dir()
        .ok_or_else(|| "unable to determine user data directory".to_string())
        .map(|dir| dir.join("bitcat"))
}

fn non_empty_path(value: Option<&str>) -> Option<PathBuf> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// 临时文件 + 原子替换写入：先写同目录唯一临时文件并刷盘，再替换目标。
///
/// Windows 下通过 `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)` 保证半写入
/// 文件不会出现在目标路径上。store 类小文件（reminders / screen_time 等）
/// 统一走这里，避免每个模块复制一份替换逻辑。
pub fn write_file_atomically(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent).map_err(|e| format!("create parent dir failed: {e}"))?;
    let temp_path = parent.join(format!(
        ".{}.tmp-{}-{:08x}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file"),
        std::process::id(),
        rand::random::<u32>()
    ));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temp_path)
            .map_err(|e| format!("create temp file failed: {e}"))?;
        file.write_all(bytes)
            .map_err(|e| format!("write temp file failed: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("sync temp file failed: {e}"))?;
    }
    replace_file(&temp_path, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp_path);
    })
}

#[cfg(target_os = "windows")]
fn replace_file(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    fn wide(path: &std::path::Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    let from_w = wide(from);
    let to_w = wide(to);
    let ok = unsafe {
        MoveFileExW(
            from_w.as_ptr(),
            to_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(format!(
            "replace file failed: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
fn replace_file(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    std::fs::rename(from, to).map_err(|e| format!("replace file failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_data_dir_uses_bitcat_folder() {
        let path = default_data_dir().unwrap();
        assert_eq!(path.file_name().and_then(|s| s.to_str()), Some(".bitcat"));
    }

    #[test]
    fn default_app_data_dir_uses_bitcat_folder() {
        let path = default_app_data_dir().unwrap();
        assert_eq!(path.file_name().and_then(|s| s.to_str()), Some("bitcat"));
    }

    #[test]
    fn write_file_atomically_replaces_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        write_file_atomically(&path, b"first").unwrap();
        write_file_atomically(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
    }
}
