//! Where Craft Hub keeps things. Everything is per-user.
//!
//! - Linux:   `~/.local/share/crafthub` (or `$XDG_DATA_HOME/crafthub`)
//! - macOS:   `~/Library/Application Support/CraftHub`
//! - Windows: `%LOCALAPPDATA%\CraftHub`

use std::env;
use std::path::PathBuf;

pub fn home() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
}

/// `$XDG_DATA_HOME`, or `~/.local/share` (Linux).
#[cfg(target_os = "linux")]
pub fn data_home() -> PathBuf {
    match env::var_os("XDG_DATA_HOME") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home().join(".local/share"),
    }
}

/// Craft Hub's own folder: state, downloads, cached icons and (Linux/Windows) the apps.
pub fn hub_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    return data_home().join("crafthub");
    #[cfg(target_os = "macos")]
    return home().join("Library/Application Support/CraftHub");
    #[cfg(windows)]
    return local_app_data().join("CraftHub");
}

#[cfg(windows)]
pub fn local_app_data() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("AppData").join("Local"))
}

/// Installed apps (AppImages on Linux, unpacked folders on Windows).
/// On macOS apps go to `~/Applications` instead.
#[cfg(not(target_os = "macos"))]
pub fn apps_dir() -> PathBuf {
    hub_dir().join("apps")
}

/// Downloads in progress.
pub fn downloads_dir() -> PathBuf {
    hub_dir().join("downloads")
}

pub fn state_file() -> PathBuf {
    hub_dir().join("state.json")
}
