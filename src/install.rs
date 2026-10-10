//! Installing, updating and uninstalling the Crafting Apps. This part is shared by every
//! platform; how a downloaded package is put in place lives in `platform`.

use std::borrow::Cow;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::apps::{self, APPS, App, Naming, OWNER};
use crate::github::{self, Release};
use crate::paths;
use crate::platform;
use crate::state::{self, Installed};

/// What kind of package an app was installed from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// Linux AppImage.
    AppImage,
    /// Windows portable zip, unpacked into Craft Hub's apps folder.
    PortableZip,
    /// Windows installer (`*-setup.exe`) run silently.
    Setup,
    /// macOS disk image; the `.app` is copied to `~/Applications`.
    Dmg,
    /// Built from source on this machine (Linux).
    Source,
}

/// The release file to install on this machine.
#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub kind: Kind,
}

/// The latest release of an app and the file for this machine, if it has one.
#[derive(Clone)]
pub struct AppRelease {
    pub release: Release,
    pub asset: Option<Asset>,
    pub sums_url: Option<String>,
}

impl AppRelease {
    pub fn tag(&self) -> &str {
        &self.release.tag
    }
}

fn has_arch(name: &str, arch: &str) -> bool {
    let low = name.to_lowercase();
    github::arch_tokens(arch).iter().any(|t| low.contains(t))
}

/// Which release file to install for `os` (`linux`/`windows`/`macos`) and `arch`
/// (`x86_64`/`aarch64`).
pub fn pick_asset<'a>(names: &[&'a str], os: &str, arch: &str) -> Option<(&'a str, Kind)> {
    let find =
        |pred: &dyn Fn(&str, &str) -> bool, a: &str| names.iter().copied().find(|n| pred(n, a));
    match os {
        "linux" => find(&|n, a| n.ends_with(".AppImage") && has_arch(n, a), arch)
            .map(|n| (n, Kind::AppImage)),
        "windows" => {
            // Windows on ARM runs x64 apps under emulation, so fall back to those.
            let arches: &[&str] = if arch == "aarch64" {
                &["aarch64", "x86_64"]
            } else {
                &[arch]
            };
            arches.iter().find_map(|a| {
                let zip = |n: &str, a: &str| {
                    let l = n.to_lowercase();
                    l.contains("windows") && l.ends_with("-portable.zip") && has_arch(n, a)
                };
                let setup =
                    |n: &str, a: &str| n.to_lowercase().ends_with("-setup.exe") && has_arch(n, a);
                find(&zip, a)
                    .map(|n| (n, Kind::PortableZip))
                    .or_else(|| find(&setup, a).map(|n| (n, Kind::Setup)))
            })
        }
        "macos" => find(
            &|n, a| {
                let l = n.to_lowercase();
                l.ends_with(".dmg") && (l.contains("universal") || has_arch(n, a))
            },
            arch,
        )
        .map(|n| (n, Kind::Dmg)),
        _ => None,
    }
}

/// The release file's name from the projects' naming scheme, for when GitHub's API is
/// rate-limited and we can't list the release's files.
pub fn guess_asset(app: &App, version: &str, os: &str, arch: &str) -> Option<(String, Kind)> {
    match (app.naming, os) {
        (Naming::Crafting, "linux") => Some((
            format!("{}-{version}-linux-{arch}.AppImage", app.id),
            Kind::AppImage,
        )),
        (Naming::Crafting, "windows") => {
            let a = if arch == "aarch64" { "arm64" } else { "x64" };
            Some((
                format!("{}-{version}-windows-{a}-portable.zip", app.id),
                Kind::PortableZip,
            ))
        }
        (Naming::Crafting, "macos") => Some((
            format!("{}-{version}-macos-universal.dmg", app.id),
            Kind::Dmg,
        )),
        (Naming::Tauri, "windows") => {
            Some((format!("{}_{version}_x64-setup.exe", app.name), Kind::Setup))
        }
        (Naming::Tauri, "macos") => {
            Some((format!("{}_{version}_universal.dmg", app.name), Kind::Dmg))
        }
        _ => None,
    }
}

pub fn latest_for(app: &App) -> Result<Option<AppRelease>, String> {
    let Some(release) = github::latest_release(OWNER, app.id)? else {
        return Ok(None);
    };
    let (os, arch) = (platform::OS, github::arch_name());
    let (asset, sums_url) = if release.guessed {
        let base = format!(
            "{}/{OWNER}/{}/releases/download/{}",
            github::web_base(),
            app.id,
            release.tag
        );
        let ver = github::pretty_version(&release.tag);
        let asset = guess_asset(app, &ver, os, arch).map(|(name, kind)| Asset {
            url: format!("{base}/{name}"),
            name,
            kind,
        });
        let sums = (app.naming == Naming::Crafting).then(|| format!("{base}/SHA256SUMS.txt"));
        (asset, sums)
    } else {
        let names: Vec<&str> = release.assets.iter().map(|(n, _)| n.as_str()).collect();
        let asset = pick_asset(&names, os, arch).map(|(n, kind)| Asset {
            name: n.to_string(),
            url: release.asset_url(n).unwrap_or_default().to_string(),
            kind,
        });
        (
            asset,
            release.asset_url("SHA256SUMS.txt").map(str::to_string),
        )
    };
    Ok(Some(AppRelease {
        release,
        asset,
        sums_url,
    }))
}

/// Download an app's package and install it (also used for updates).
pub fn install_release(
    app: &App,
    rel: &AppRelease,
    progress: &dyn Fn(u64, u64),
) -> Result<(), String> {
    let asset = rel
        .asset
        .as_ref()
        .ok_or_else(|| format!("The latest release has no build for {}", platform::NAME))?;
    ensure_icon(app);
    let file = paths::downloads_dir().join(&asset.name);
    let verified = github::download_verified(
        &asset.url,
        &asset.name,
        rel.sums_url.as_deref(),
        &file,
        progress,
    )?;
    let result = platform::install_package(app, &file, asset.kind);
    let _ = fs::remove_file(&file);
    let (path, uninstaller) = result?;
    state::update(
        app.id,
        Some(Installed {
            version: rel.tag().to_string(),
            path,
            kind: Some(asset.kind),
            uninstaller,
            asset: Some(asset.name.clone()),
            verified,
            source: false,
            installed_at: state::now(),
        }),
    )
    .map_err(|e| e.to_string())
}

/// Uninstall an app and everything Craft Hub created for it (menu entry or shortcut, icons,
/// leftover downloads and update folders). The app's own settings are left alone; see
/// [`app_data`] and [`delete_app_data`].
pub fn uninstall(app: &App) -> Result<(), String> {
    let info = state::load().remove(app.id);
    platform::remove_package(app, info.as_ref())?;
    remove_leftover_downloads(app);
    state::update(app.id, None).map_err(|e| e.to_string())
}

/// Half-finished or leftover downloads of this app's packages.
fn remove_leftover_downloads(app: &App) {
    let prefixes = [
        app.id.to_lowercase(),
        format!("{}_", app.name.to_lowercase()),
    ];
    for e in fs::read_dir(paths::downloads_dir())
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = e.file_name().to_string_lossy().to_lowercase();
        if prefixes.iter().any(|p| name.starts_with(p.as_str())) {
            let _ = fs::remove_file(e.path());
        }
    }
}

/// Names an app's settings folders may use: its id, name, desktop id and aliases.
pub fn data_names(app: &App) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for n in [app.id, app.name, app.desktop_id]
        .iter()
        .chain(app.data_aliases)
    {
        for v in [n.to_string(), n.to_lowercase()] {
            if !names.contains(&v) {
                names.push(v);
            }
        }
    }
    names
}

/// Where the app keeps its settings, caches and saved data on this computer (only the
/// ones that exist). These are the app's own files, not Craft Hub's.
pub fn app_data(app: &App) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut seen = Vec::new();
    for p in platform::data_locations(app, &data_names(app)) {
        if !p.exists() {
            continue;
        }
        // Folder names are case-insensitive on Windows and macOS: don't list one twice.
        let key = fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
        let is_hub = key
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("crafthub"));
        let key = key.to_string_lossy().to_lowercase();
        if !seen.contains(&key) && !is_hub {
            seen.push(key);
            found.push(p);
        }
    }
    found
}

/// Total size of files under `paths`, in bytes.
pub fn size_of(paths: &[PathBuf]) -> u64 {
    fn walk(p: &std::path::Path) -> u64 {
        match fs::symlink_metadata(p) {
            Ok(m) if m.is_dir() => fs::read_dir(p)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| walk(&e.path()))
                .sum(),
            Ok(m) => m.len(),
            Err(_) => 0,
        }
    }
    paths.iter().map(|p| walk(p)).sum()
}

/// Delete the app's settings, caches and saved data. Only for apps that aren't installed.
pub fn delete_app_data(app: &App) -> Result<(), String> {
    if state::installed(app.id).is_some() {
        return Err(format!("Uninstall {} first", app.name));
    }
    let mut failed = Vec::new();
    for p in app_data(app) {
        let r = if p.is_dir() {
            fs::remove_dir_all(&p)
        } else {
            fs::remove_file(&p)
        };
        if r.is_err() {
            failed.push(p.display().to_string());
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Couldn't delete (is {} still open?): {}",
            app.name,
            failed.join(", ")
        ))
    }
}

/// Human-readable size, e.g. "2.4 MB".
pub fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} KB", b / 1e3)
    } else {
        format!("{bytes} bytes")
    }
}

pub fn launch(app: &App) -> Result<(), String> {
    let info = state::installed(app.id).ok_or("Not installed")?;
    platform::launch(app, &info)
}

fn cached_icon_path(app: &App) -> PathBuf {
    paths::hub_dir()
        .join("icons")
        .join(format!("{}.png", app.id))
}

/// The app's icon: the copy downloaded from its repo, or a stand-in until there is one.
pub fn icon_bytes(app: &App) -> Cow<'static, [u8]> {
    match fs::read(cached_icon_path(app)) {
        Ok(b) if b.starts_with(b"\x89PNG") => Cow::Owned(b),
        _ => Cow::Borrowed(apps::PLACEHOLDER_ICON),
    }
}

/// Download one app's icon from its repo if the cached copy is missing or a week old.
/// Returns true if the icon changed.
fn fetch_icon(app: &App) -> bool {
    let path = cached_icon_path(app);
    let fresh = fs::metadata(&path)
        .and_then(|m| m.modified())
        .map(|t| t.elapsed().unwrap_or_default() < Duration::from_secs(7 * 86400))
        .unwrap_or(false);
    if fresh {
        return false;
    }
    let url = match std::env::var("CRAFTHUB_RAW") {
        Ok(base) => app
            .icon_url
            .replace("https://raw.githubusercontent.com", &base),
        Err(_) => app.icon_url.to_string(),
    };
    let Ok(mut resp) = github::agent().get(&url).call() else {
        return false;
    };
    let Ok(bytes) = resp.body_mut().with_config().limit(4 << 20).read_to_vec() else {
        return false;
    };
    if !bytes.starts_with(b"\x89PNG") {
        return false;
    }
    let old = fs::read(&path).ok();
    let _ = fs::create_dir_all(path.parent().unwrap_or(&path));
    fs::write(&path, &bytes).is_ok() && old.as_deref() != Some(&bytes[..])
}

/// Make sure an app's icon is downloaded (before installing it, so its menu entry or
/// shortcut gets the real icon).
pub fn ensure_icon(app: &App) {
    if !cached_icon_path(app).exists() {
        fetch_icon(app);
    }
}

/// Download (or refresh, weekly) every app's icon from its own repo, in parallel.
/// Returns the apps whose icon changed, after updating their menu entries.
pub fn refresh_icons() -> Vec<&'static App> {
    let changed: Vec<&'static App> = std::thread::scope(|scope| {
        let handles: Vec<_> = APPS
            .iter()
            .map(|app| (app, scope.spawn(|| fetch_icon(app))))
            .collect();
        handles
            .into_iter()
            .filter_map(|(app, h)| h.join().unwrap_or(false).then_some(app))
            .collect()
    });
    for app in &changed {
        if let Some(info) = state::installed(app.id) {
            platform::icon_updated(app, &info);
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::by_id;

    /// File names as the Crafting Apps and ArtCraft actually publish them.
    fn crafting_assets() -> Vec<String> {
        let mut v = Vec::new();
        for a in ["aarch64", "x86_64"] {
            for e in [
                "AppImage",
                "AppImage.zsync",
                "deb",
                "rpm",
                "tar.gz",
                "flatpak",
            ] {
                v.push(format!("photocraft-0.5.0-linux-{a}.{e}"));
            }
        }
        for a in ["arm64", "x64", "x86"] {
            v.push(format!("photocraft-0.5.0-windows-{a}.msi"));
            v.push(format!("photocraft-0.5.0-windows-{a}-portable.zip"));
        }
        v.push("photocraft-0.5.0-macos-universal.dmg".into());
        v.push("photocraft-cli-0.5.0-macos-universal.zip".into());
        v.push("SHA256SUMS.txt".into());
        v
    }

    fn pick(names: &[String], os: &str, arch: &str) -> Option<(String, Kind)> {
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        pick_asset(&refs, os, arch).map(|(n, k)| (n.to_string(), k))
    }

    #[test]
    fn picks_the_right_file_everywhere() {
        let a = crafting_assets();
        let p = |os, arch| pick(&a, os, arch).unwrap();
        assert_eq!(
            p("linux", "x86_64"),
            (
                "photocraft-0.5.0-linux-x86_64.AppImage".into(),
                Kind::AppImage
            )
        );
        assert_eq!(
            p("linux", "aarch64"),
            (
                "photocraft-0.5.0-linux-aarch64.AppImage".into(),
                Kind::AppImage
            )
        );
        assert_eq!(
            p("windows", "x86_64"),
            (
                "photocraft-0.5.0-windows-x64-portable.zip".into(),
                Kind::PortableZip
            )
        );
        assert_eq!(
            p("windows", "aarch64"),
            (
                "photocraft-0.5.0-windows-arm64-portable.zip".into(),
                Kind::PortableZip
            )
        );
        assert_eq!(
            p("macos", "aarch64"),
            ("photocraft-0.5.0-macos-universal.dmg".into(), Kind::Dmg)
        );
        assert_eq!(
            p("macos", "x86_64"),
            ("photocraft-0.5.0-macos-universal.dmg".into(), Kind::Dmg)
        );
    }

    #[test]
    fn artcraft_files() {
        let a: Vec<String> = [
            "ArtCraft_0.32.0_universal.dmg",
            "ArtCraft_0.32.0_x64-setup.exe",
            "ArtCraft_0.32.0_x64_en-US.msi",
            "ArtCraft_universal.app.tar.gz",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(pick(&a, "linux", "x86_64"), None);
        assert_eq!(
            pick(&a, "windows", "x86_64").unwrap(),
            ("ArtCraft_0.32.0_x64-setup.exe".into(), Kind::Setup)
        );
        // Windows on ARM falls back to the x64 installer (runs under emulation).
        assert_eq!(
            pick(&a, "windows", "aarch64").unwrap(),
            ("ArtCraft_0.32.0_x64-setup.exe".into(), Kind::Setup)
        );
        assert_eq!(
            pick(&a, "macos", "aarch64").unwrap(),
            ("ArtCraft_0.32.0_universal.dmg".into(), Kind::Dmg)
        );
    }

    #[test]
    fn guessed_names_match_real_ones() {
        let a = crafting_assets();
        let pc = by_id("photocraft").unwrap();
        for (os, arch) in [
            ("linux", "x86_64"),
            ("linux", "aarch64"),
            ("windows", "x86_64"),
            ("windows", "aarch64"),
            ("macos", "aarch64"),
        ] {
            assert_eq!(
                guess_asset(pc, "0.5.0", os, arch),
                pick(&a, os, arch),
                "{os} {arch}"
            );
        }
        let ac = by_id("artcraft").unwrap();
        assert_eq!(
            guess_asset(ac, "0.32.0", "windows", "x86_64").unwrap().0,
            "ArtCraft_0.32.0_x64-setup.exe"
        );
        assert_eq!(
            guess_asset(ac, "0.32.0", "macos", "aarch64").unwrap().0,
            "ArtCraft_0.32.0_universal.dmg"
        );
        assert_eq!(guess_asset(ac, "0.32.0", "linux", "x86_64"), None);
    }

    #[test]
    fn data_folder_names() {
        let names = data_names(by_id("pdfcraft").unwrap());
        for n in [
            "pdfcraft",
            "PDFCraft",
            "ai.storyteller.pdfcraft",
            "PrintCraft",
            "printcraft",
        ] {
            assert!(names.contains(&n.to_string()), "{n} missing from {names:?}");
        }
        assert_eq!(human_size(2_400_000), "2.4 MB");
        assert_eq!(human_size(512), "512 bytes");
    }

    #[test]
    fn kind_is_stored_as_text() {
        assert_eq!(
            serde_json::to_string(&Kind::PortableZip).unwrap(),
            "\"portable-zip\""
        );
        let old: Installed = serde_json::from_str(r#"{"version":"v1","path":"/x"}"#).unwrap();
        assert_eq!(old.kind, None);
    }
}
