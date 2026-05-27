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
}
