//! Craft Hub managing itself: installing into the menu, uninstalling and self-updating
//! from its own GitHub releases.

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::apps::APPS;
use crate::github::{self, parse_version};
use crate::install;
use crate::paths;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_REPO: &str = "TymonDomanko/crafthub";
const ICON_SVG: &[u8] = include_bytes!("../assets/icons/crafthub.svg");
pub const ICON_PNG: &[u8] = include_bytes!("../assets/icons/crafthub.png");

/// The GitHub repo Craft Hub updates itself from (`owner/name`).
pub fn repo() -> String {
    std::env::var("CRAFTHUB_REPO").unwrap_or_else(|_| DEFAULT_REPO.into())
}

pub fn releases_url() -> String {
    format!("https://github.com/{}/releases", repo())
}

/// Release asset name for this machine, e.g. `crafthub-linux-x86_64`.
pub fn asset_name() -> String {
    format!("crafthub-linux-{}", github::arch_name())
}

#[derive(Clone, Debug)]
pub struct HubRelease {
    pub tag: String,
    pub url: String,
    pub sums_url: Option<String>,
}

impl HubRelease {
    pub fn version(&self) -> String {
        github::pretty_version(&self.tag)
    }
    pub fn is_newer(&self) -> bool {
        parse_version(&self.tag) > parse_version(VERSION)
    }
}

/// The latest Craft Hub release that has a binary for this machine.
pub fn latest() -> Result<Option<HubRelease>, String> {
    let repo = repo();
    let (owner, name) = repo.split_once('/').ok_or("bad repo name")?;
    let Some(rel) = github::latest_release(owner, name)? else {
        return Ok(None);
    };
    let asset = asset_name();
    let base = format!(
        "{}/{repo}/releases/download/{}",
        github::web_base(),
        rel.tag
    );
    let (url, sums) = if rel.guessed {
        (
            Some(format!("{base}/{asset}")),
            Some(format!("{base}/SHA256SUMS.txt")),
        )
    } else {
        (
            rel.asset_url(&asset).map(str::to_string),
            rel.asset_url("SHA256SUMS.txt").map(str::to_string),
        )
    };
    Ok(url.map(|url| HubRelease {
        tag: rel.tag,
        url,
        sums_url: sums,
    }))
}

/// Download and install a new Craft Hub. Returns the path to restart into.
pub fn self_update(rel: &HubRelease, progress: &dyn Fn(u64, u64)) -> Result<PathBuf, String> {
    let target = paths::installed_hub_binary();
    remove_if_symlink(&target);
    github::download_verified(
        &rel.url,
        &asset_name(),
        rel.sums_url.as_deref(),
        &target,
        progress,
    )?;
    // Let the new version refresh its own menu entry and icon.
    let status = Command::new(&target)
        .arg("--install-self")
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("The new version failed to install itself".into());
    }
    Ok(target)
}

/// Replace this process with `path` (used after a self-update).
pub fn restart(path: &Path) -> String {
    Command::new(path).exec().to_string()
}

fn remove_if_symlink(p: &Path) {
    if fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        let _ = fs::remove_file(p);
    }
}

fn desktop_file() -> PathBuf {
    paths::desktop_dir().join("crafthub.desktop")
}

pub fn is_self_installed() -> bool {
    paths::installed_hub_binary().is_file() && desktop_file().is_file()
}

/// Copy this binary to `~/.local/bin/crafthub` and add Craft Hub to the menu.
/// Also replaces the earlier Python version if it's there.
pub fn install_self() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = paths::installed_hub_binary();
    fs::create_dir_all(paths::bin_dir()).map_err(|e| e.to_string())?;
    remove_if_symlink(&target); // the Python version symlinked crafthub.py here
    let same = fs::canonicalize(&exe).ok() == fs::canonicalize(&target).ok();
    if !same {
        let tmp = target.with_extension("part");
        fs::copy(&exe, &tmp).map_err(|e| e.to_string())?;
        github::set_executable(&tmp)?;
        fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
    }

    let scalable = paths::hicolor_dir().join("scalable/apps");
    fs::create_dir_all(&scalable).map_err(|e| e.to_string())?;
    fs::create_dir_all(paths::icon_dir()).map_err(|e| e.to_string())?;
    fs::write(scalable.join("crafthub.svg"), ICON_SVG).map_err(|e| e.to_string())?;
    fs::write(paths::icon_dir().join("crafthub.png"), ICON_PNG).map_err(|e| e.to_string())?;

    fs::create_dir_all(paths::desktop_dir()).map_err(|e| e.to_string())?;
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Craft Hub\n\
         Comment=Install, update and uninstall the Crafting Apps\nExec={}\nIcon=crafthub\n\
         Terminal=false\nStartupWMClass=crafthub\nCategories=Graphics;System;PackageManager;\n\
         Keywords=artcraft;photocraft;vectorcraft;filmcraft;lightcraft;pdfcraft;effectcraft;designcraft;adobe;\n",
        install::exec_quote(&target.to_string_lossy())
    );
    fs::write(desktop_file(), entry).map_err(|e| e.to_string())?;
    let _ = github::set_executable(&desktop_file());

    // Leftovers from the Python version.
    let _ = fs::remove_dir_all(paths::hub_dir().join("hub"));

    install::refresh_menus();
    Ok(target)
}

/// Remove Craft Hub; with `all`, also every app it installed.
pub fn uninstall_self(all: bool) -> Result<(), String> {
    if all {
        for app in APPS {
            if crate::state::load().contains_key(app.id) {
                install::uninstall(app)?;
            }
        }
        let _ = fs::remove_dir_all(paths::hub_dir());
    }
    let _ = fs::remove_file(desktop_file());
    let _ = fs::remove_file(paths::hicolor_dir().join("scalable/apps/crafthub.svg"));
    let _ = fs::remove_file(paths::icon_dir().join("crafthub.png"));
    let _ = fs::remove_file(paths::installed_hub_binary());
    install::refresh_menus();
    Ok(())
}
