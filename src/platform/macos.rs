//! macOS: apps copied from their disk images into `~/Applications` (no password needed),
//! and Craft Hub itself as `Craft Hub.app`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::apps::App;
use crate::hub;
use crate::install::Kind;
use crate::paths;
use crate::state::Installed;

use super::replace_dir;

pub const OS: &str = "macos";
pub const NAME: &str = "macOS";
pub const SOURCE_BUILDS: bool = false;

const HUB_BUNDLE: &str = "Craft Hub.app";
const INFO_PLIST: &str = include_str!("../../packaging/macos/Info.plist");
const HUB_ICNS: &[u8] = include_bytes!("../../assets/icons/crafthub.icns");

fn user_applications() -> PathBuf {
    paths::home().join("Applications")
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Copy a bundle with `ditto` (keeps permissions, symlinks and extended attributes).
fn ditto(src: &Path, dest: &Path) -> Result<(), String> {
    run(Command::new("ditto").arg(src).arg(dest))
}

fn clear_quarantine(path: &Path) {
    let _ = Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

// ----------------------------------------------------------------------------- apps

pub fn notice() -> Option<String> {
    None
}

/// winit reports the macOS appearance itself.
pub fn prefers_dark() -> Option<bool> {
    None
}

pub fn install_package(
    app: &App,
    file: &Path,
    kind: Kind,
) -> Result<(String, Option<String>), String> {
    if kind != Kind::Dmg {
        return Err(format!("Can't install a {kind:?} package on macOS"));
    }
    let mount = paths::hub_dir().join(format!("mnt-{}", app.id));
    let _ = run(Command::new("hdiutil")
        .arg("detach")
        .arg(&mount)
        .arg("-force"));
    fs::create_dir_all(&mount).map_err(|e| e.to_string())?;
    run(Command::new("hdiutil")
        .args([
            "attach",
            "-nobrowse",
            "-noautoopen",
            "-readonly",
            "-mountpoint",
        ])
        .arg(&mount)
        .arg(file))
    .map_err(|e| format!("Couldn't open {}'s disk image: {e}", app.name))?;

    let copy = || -> Result<PathBuf, String> {
        let bundle = fs::read_dir(&mount)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "app"))
            .ok_or_else(|| format!("No app found in {}'s disk image", app.name))?;
        let name = bundle
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let apps = user_applications();
        fs::create_dir_all(&apps).map_err(|e| e.to_string())?;
        let staging = apps.join(format!(".{name}.new"));
        let _ = fs::remove_dir_all(&staging);
        ditto(&bundle, &staging).map_err(|e| format!("Couldn't copy {}: {e}", app.name))?;
        let dest = apps.join(&name);
        if let Err(e) = replace_dir(&staging, &dest, app.name) {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
        clear_quarantine(&dest);
        Ok(dest)
    };
    let result = copy();
    let _ = run(Command::new("hdiutil")
        .arg("detach")
        .arg(&mount)
        .arg("-force"));
    let _ = fs::remove_dir(&mount);
    result.map(|dest| (dest.to_string_lossy().to_string(), None))
}

pub fn remove_package(app: &App, info: Option<&Installed>) -> Result<(), String> {
    let Some(info) = info else { return Ok(()) };
    let path = PathBuf::from(&info.path);
    // Only ever delete an .app bundle in ~/Applications that we installed.
    if path.starts_with(user_applications())
        && path.extension().is_some_and(|x| x == "app")
        && path.exists()
    {
        fs::remove_dir_all(&path).map_err(|e| format!("Couldn't remove {}: {e}", app.name))?;
    }
    Ok(())
}

pub fn launch(_app: &App, info: &Installed) -> Result<(), String> {
    Command::new("open")
        .arg(&info.path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Where an app may keep settings, caches and data in `~/Library`, under any of its names
/// (and its bundle id for the files macOS itself keeps per app).
pub fn data_locations(app: &App, names: &[String]) -> Vec<PathBuf> {
    let lib = paths::home().join("Library");
    let mut v: Vec<PathBuf> = ["Application Support", "Caches", "Logs"]
        .iter()
        .flat_map(|b| names.iter().map(move |n| lib.join(b).join(n)))
        .collect();
    let id = app.desktop_id;
    v.extend([
        lib.join("Preferences").join(format!("{id}.plist")),
        lib.join("Saved Application State")
            .join(format!("{id}.savedState")),
        lib.join("HTTPStorages").join(id),
        lib.join("WebKit").join(id),
    ]);
    v
}

/// Apps use their own icons in Finder and the Dock, so nothing to update.
pub fn icon_updated(_app: &App, _info: &Installed) {}

pub fn open_url(url: &str) {
    let _ = Command::new("open")
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

pub fn start_source_build(_app: &App) -> Result<Child, String> {
    Err("Building from source isn't available on macOS".into())
}

pub fn startup() {}

// ----------------------------------------------------------------------------- Craft Hub itself

pub fn hub_asset_name() -> String {
    "crafthub-macos-universal.zip".into()
}

/// The `.app` bundle this program is running from, if any.
fn current_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|x| x == "app"))
        .map(Path::to_path_buf)
}

pub fn installed_hub_path() -> PathBuf {
    current_bundle().unwrap_or_else(|| user_applications().join(HUB_BUNDLE))
}

/// On macOS Craft Hub stays wherever you put its `.app`.
pub fn running_installed_copy() -> bool {
    current_bundle().is_some()
}

pub fn is_self_installed() -> bool {
    current_bundle().is_some() || user_applications().join(HUB_BUNDLE).exists()
}

/// Running inside `Craft Hub.app` there's nothing to do. A bare binary (built from source)
/// wraps itself in `~/Applications/Craft Hub.app`.
pub fn install_self() -> Result<PathBuf, String> {
    if let Some(bundle) = current_bundle() {
        return Ok(bundle);
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bundle = user_applications().join(HUB_BUNDLE);
    let staging = user_applications().join(format!(".{HUB_BUNDLE}.new"));
    let _ = fs::remove_dir_all(&staging);
    let contents = staging.join("Contents");
    fs::create_dir_all(contents.join("MacOS")).map_err(|e| e.to_string())?;
    fs::create_dir_all(contents.join("Resources")).map_err(|e| e.to_string())?;
    fs::write(
        contents.join("Info.plist"),
        INFO_PLIST.replace("@VERSION@", hub::VERSION),
    )
    .map_err(|e| e.to_string())?;
    fs::write(contents.join("Resources/crafthub.icns"), HUB_ICNS).map_err(|e| e.to_string())?;
    let bin = contents.join("MacOS/crafthub");
    fs::copy(&exe, &bin).map_err(|e| e.to_string())?;
    crate::github::set_executable(&bin)?;
    replace_dir(&staging, &bundle, "Craft Hub")?;
    Ok(bundle)
}

/// Remove `Craft Hub.app`.
pub fn uninstall_self() -> Result<(), String> {
    let bundle = installed_hub_path();
    if bundle.extension().is_some_and(|x| x == "app") && bundle.exists() {
        fs::remove_dir_all(&bundle).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Unpack the downloaded `Craft Hub.app` over the current one. Returns the bundle.
pub fn apply_self_update(downloaded: &Path) -> Result<PathBuf, String> {
    let staging = paths::hub_dir().join("update");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    run(Command::new("ditto")
        .args(["-x", "-k"])
        .arg(downloaded)
        .arg(&staging))
    .map_err(|e| format!("Couldn't unpack the update: {e}"))?;
    let new = staging.join(HUB_BUNDLE);
    if !new.exists() {
        return Err("The update doesn't contain Craft Hub.app".into());
    }
    let target = installed_hub_path();
    let dir = target
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(user_applications);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Copy next to the current bundle first, so the swap is a rename on the same disk.
    let beside = dir.join(format!(".{HUB_BUNDLE}.new"));
    let _ = fs::remove_dir_all(&beside);
    let copied = ditto(&new, &beside);
    let _ = fs::remove_dir_all(&staging);
    copied.map_err(|e| format!("Couldn't copy the update: {e}"))?;
    replace_dir(&beside, &target, "Craft Hub")?;
    clear_quarantine(&target);
    Ok(target)
}

/// Open the (new) bundle and exit this process.
pub fn restart(target: &Path) -> String {
    match Command::new("open").arg("-n").arg(target).spawn() {
        Ok(_) => std::process::exit(0),
        Err(e) => e.to_string(),
    }
}
