//! Craft Hub managing itself: installing, uninstalling and self-updating from its own
//! GitHub releases. The platform-specific parts are in `platform`.

use std::path::{Path, PathBuf};

use crate::apps::APPS;
use crate::github::{self, parse_version};
use crate::install;
use crate::paths;
use crate::platform;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_REPO: &str = "TymonDomanko/CraftHub";
pub const ICON_PNG: &[u8] = include_bytes!("../assets/icons/crafthub.png");

/// The GitHub repo Craft Hub updates itself from (`owner/name`).
pub fn repo() -> String {
    std::env::var("CRAFTHUB_REPO").unwrap_or_else(|_| DEFAULT_REPO.into())
}

pub fn releases_url() -> String {
    format!("https://github.com/{}/releases", repo())
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

/// The latest Craft Hub release that has a build for this machine.
pub fn latest() -> Result<Option<HubRelease>, String> {
    let repo = repo();
    let (owner, name) = repo.split_once('/').ok_or("bad repo name")?;
    let Some(rel) = github::latest_release(owner, name)? else {
        return Ok(None);
    };
    let asset = platform::hub_asset_name();
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

/// Download and install a new Craft Hub. Returns what to restart into.
pub fn self_update(rel: &HubRelease, progress: &dyn Fn(u64, u64)) -> Result<PathBuf, String> {
    let asset = platform::hub_asset_name();
    let file = paths::downloads_dir().join(&asset);
    github::download_verified(&rel.url, &asset, rel.sums_url.as_deref(), &file, progress)?;
    let result = platform::apply_self_update(&file);
    let _ = std::fs::remove_file(&file);
    result
}

/// Start the new version and exit. Only returns (with the error) if that fails.
pub fn restart(path: &Path) -> String {
    platform::restart(path)
}

pub fn install_self() -> Result<PathBuf, String> {
    platform::install_self()
}

/// Remove Craft Hub and everything it keeps for itself. With `all`, also every app it
/// installed; with `purge`, also those apps' settings and saved data.
pub fn uninstall_self(all: bool, purge: bool) -> Result<(), String> {
    // Removing the package asks for the password first, so cancelling changes nothing.
    let packaged = platform::packaged();
    if packaged {
        platform::uninstall_self()?;
    }
    if all || purge {
        for app in APPS {
            if crate::state::load().contains_key(app.id) {
                install::uninstall(app)?;
                if purge {
                    install::delete_app_data(app)?;
                }
            }
        }
    }
    if !packaged {
        platform::uninstall_self()?;
    }
    let dir = paths::hub_dir();
    if all || purge || crate::state::load().is_empty() {
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        // Keep the installed apps and the record of them; remove everything else.
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name == "apps" || name == "state.json" {
                continue;
            }
            let p = e.path();
            let _ = if p.is_dir() {
                std::fs::remove_dir_all(&p)
            } else {
                std::fs::remove_file(&p)
            };
        }
    }
    Ok(())
}
