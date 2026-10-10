//! Platform-specific parts: how packages are installed, menu/Start Menu integration,
//! launching, and Craft Hub installing and updating itself.
//!
//! Each platform module provides the same set of functions:
//!
//! - `OS`, `NAME`, `SOURCE_BUILDS`
//! - `install_package`, `remove_package`, `launch`, `open_url`, `icon_updated`
//! - `startup`, `notice`, `prefers_dark`, `start_source_build`
//! - `hub_asset_name`, `running_installed_copy`, `is_self_installed`, `install_self`,
//!   `uninstall_self`, `apply_self_update`, `restart`

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

use std::fs;
use std::path::Path;

/// Put `new` in place of `dest` (both folders), keeping the old one until the new one is
/// in. Fails with a friendly message if the old one is in use (Windows).
#[allow(dead_code)]
pub(crate) fn replace_dir(new: &Path, dest: &Path, app_name: &str) -> Result<(), String> {
    if !dest.exists() {
        return fs::rename(new, dest).map_err(|e| format!("Couldn't install {app_name}: {e}"));
    }
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let old = dest.with_file_name(format!(".{name}.old"));
    let _ = fs::remove_dir_all(&old);
    fs::rename(dest, &old).map_err(|_| format!("{app_name} is in use. Close it and try again."))?;
    if let Err(e) = fs::rename(new, dest) {
        let _ = fs::rename(&old, dest);
        return Err(format!("Couldn't install {app_name}: {e}"));
    }
    let _ = fs::remove_dir_all(&old);
    Ok(())
}
