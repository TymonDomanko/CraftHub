//! Where Craft Hub keeps things. Everything is per-user, under the XDG data dir.

use std::env;
use std::path::PathBuf;

pub fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// `$XDG_DATA_HOME`, or `~/.local/share`.
pub fn data_home() -> PathBuf {
    match env::var_os("XDG_DATA_HOME") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home().join(".local/share"),
    }
}

/// `~/.local/share/crafthub`
pub fn hub_dir() -> PathBuf {
    data_home().join("crafthub")
}

/// Installed AppImages and binaries.
pub fn apps_dir() -> PathBuf {
    hub_dir().join("apps")
}

/// Source checkouts for apps built from source.
pub fn src_dir() -> PathBuf {
    hub_dir().join("src")
}

pub fn state_file() -> PathBuf {
    hub_dir().join("state.json")
}

pub fn desktop_dir() -> PathBuf {
    data_home().join("applications")
}

pub fn hicolor_dir() -> PathBuf {
    data_home().join("icons/hicolor")
}

pub fn icon_dir() -> PathBuf {
    hicolor_dir().join("256x256/apps")
}

/// Where the `crafthub` command lives.
pub fn bin_dir() -> PathBuf {
    home().join(".local/bin")
}

pub fn installed_hub_binary() -> PathBuf {
    bin_dir().join("crafthub")
}
