//! Linux: AppImages in `~/.local/share/crafthub/apps`, freedesktop menu entries, and
//! Craft Hub installed as `~/.local/bin/crafthub`.

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;

use crate::apps::{self, APPS, App};
use crate::github;
use crate::hub;
use crate::install::{self, Kind};
use crate::paths;
use crate::state::{self, Installed};

pub const OS: &str = "linux";
pub const NAME: &str = "Linux";
pub const SOURCE_BUILDS: bool = true;

const MARKER: &str = "X-CraftHub=true";
const HUB_ICON_SVG: &[u8] = include_bytes!("../../assets/icons/crafthub.svg");

fn desktop_dir() -> PathBuf {
    paths::data_home().join("applications")
}
fn hicolor_dir() -> PathBuf {
    paths::data_home().join("icons/hicolor")
}
fn icon_dir() -> PathBuf {
    hicolor_dir().join("256x256/apps")
}
fn bin_dir() -> PathBuf {
    paths::home().join(".local/bin")
}
pub fn installed_hub_path() -> PathBuf {
    bin_dir().join("crafthub")
}
fn hub_desktop_file() -> PathBuf {
    desktop_dir().join("crafthub.desktop")
}

// ----------------------------------------------------------------------------- apps

/// Is FUSE 2 available? AppImages need it unless run in extract-and-run mode.
pub fn have_fuse2() -> bool {
    static FUSE: OnceLock<bool> = OnceLock::new();
    *FUSE.get_or_init(|| {
        ["/sbin/ldconfig", "/usr/sbin/ldconfig", "ldconfig"]
            .iter()
            .any(|ld| {
                Command::new(ld)
                    .arg("-p")
                    .stderr(Stdio::null())
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).contains("libfuse.so.2"))
                    .unwrap_or(false)
            })
    })
}

pub fn notice() -> Option<String> {
    (!have_fuse2())
        .then(|| "FUSE 2 isn't installed, so apps start a little slower. Fix: sudo apt install libfuse2t64".into())
}

/// Environment for launching an app.
fn app_env(app: &App, path: &str) -> Vec<(String, String)> {
    // PhotoCraft's AppImage adds its own menu entry on first run unless told not to.
    let mut env = vec![(
        format!("{}_NO_DESKTOP_INTEGRATION", app.id.to_uppercase()),
        "1".into(),
    )];
    if path.ends_with(".AppImage") && !have_fuse2() {
        env.push(("APPIMAGE_EXTRACT_AND_RUN".into(), "1".into()));
    }
    env
}

/// Quote a path for a desktop entry's Exec key.
pub fn exec_quote(path: &str) -> String {
    let mut s = String::from("\"");
    for c in path.chars() {
        match c {
            '"' | '`' | '$' => {
                s.push_str("\\\\");
                s.push(c);
            }
            '\\' => s.push_str("\\\\\\\\"),
            '%' => s.push_str("%%"),
            '\n' | '\r' => {}
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

fn exec_line(app: &App, path: &str) -> String {
    let mut env = app_env(app, path);
    env.sort();
    let env: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!("env {} {} %F", env.join(" "), exec_quote(path))
}

fn desktop_path(desktop_id: &str) -> PathBuf {
    desktop_dir().join(format!("{desktop_id}.desktop"))
}

/// Ask the desktop to reload menus and icons.
fn refresh_menus() {
    let jobs: [(&str, Vec<String>); 2] = [
        (
            "update-desktop-database",
            vec!["-q".into(), desktop_dir().display().to_string()],
        ),
        (
            "gtk-update-icon-cache",
            vec![
                "-q".into(),
                "-t".into(),
                "-f".into(),
                hicolor_dir().display().to_string(),
            ],
        ),
    ];
    for (cmd, args) in jobs {
        let _ = Command::new(cmd)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Write the app's menu entry and icon. Named after the app's own ID so there's one
/// entry per app, the taskbar shows the right icon, and apps don't add their own.
fn write_menu_entry(app: &App, path: &str) -> std::io::Result<()> {
    fs::create_dir_all(desktop_dir())?;
    fs::create_dir_all(icon_dir())?;
    remove_legacy(app);
    fs::write(
        icon_dir().join(format!("{}.png", app.desktop_id)),
        install::icon_bytes(app),
    )?;
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName={name}\nComment={comment}\nExec={exec}\n\
         Icon={id}\nTerminal=false\nStartupWMClass={id}\nCategories={cat}\n{MARKER}\n",
        name = app.name,
        comment = app.comment(),
        exec = exec_line(app, path),
        id = app.desktop_id,
        cat = app.category,
    );
    let dpath = desktop_path(app.desktop_id);
    fs::write(&dpath, entry)?;
    let _ = github::set_executable(&dpath);
    refresh_menus();
    Ok(())
}

/// Entries from Craft Hub 1.0, which used `crafthub-<id>` names.
fn remove_legacy(app: &App) {
    let _ = fs::remove_file(desktop_dir().join(format!("crafthub-{}.desktop", app.id)));
    let _ = fs::remove_file(icon_dir().join(format!("crafthub-{}.png", app.id)));
}

/// A menu entry we wrote, or one an app wrote for itself pointing into Craft Hub's
/// apps folder (PhotoCraft and EffectCraft do this on first launch).
fn ours_or_orphaned(dpath: &Path) -> bool {
    let Ok(text) = fs::read_to_string(dpath) else {
        return false;
    };
    text.contains(MARKER) || text.contains(&*paths::apps_dir().to_string_lossy())
}

fn remove_menu_entry(app: &App) {
    remove_legacy(app);
    let dpath = desktop_path(app.desktop_id);
    if ours_or_orphaned(&dpath) {
        let _ = fs::remove_file(&dpath);
        // Icons we (or the app) put in the user's icon folder, at every size.
        if let Ok(sizes) = fs::read_dir(hicolor_dir()) {
            for size in sizes.flatten() {
                let dir = size.path().join("apps");
                if dir.is_dir() {
                    for ext in ["png", "svg"] {
                        let _ = fs::remove_file(dir.join(format!("{}.{ext}", app.desktop_id)));
                    }
                }
            }
        }
    }
    refresh_menus();
}

/// Remove menu entries left behind for apps that aren't installed any more.
fn cleanup_orphans() {
    let st = state::load();
    for app in APPS {
        if st.get(app.id).is_some_and(Installed::exists) {
            continue;
        }
        let stale = [
            desktop_path(app.desktop_id),
            desktop_dir().join(format!("crafthub-{}.desktop", app.id)),
        ]
        .iter()
        .any(|p| p.exists() && ours_or_orphaned(p));
        if stale {
            remove_menu_entry(app);
        }
    }
}

/// Rewrite entries for installed apps that use an old name or are missing.
fn migrate_entries() {
    for (id, info) in state::load() {
        let Some(app) = apps::by_id(&id) else {
            continue;
        };
        if !info.exists() {
            continue;
        }
        let legacy = desktop_dir().join(format!("crafthub-{id}.desktop"));
        if legacy.exists() || !desktop_path(app.desktop_id).exists() {
            let _ = write_menu_entry(app, &info.path);
        }
    }
}

/// Housekeeping when the window opens.
pub fn startup() {
    cleanup_orphans();
    migrate_entries();
}

pub fn install_package(
    app: &App,
    file: &Path,
    kind: Kind,
) -> Result<(String, Option<String>), String> {
    if kind != Kind::AppImage {
        return Err(format!("Can't install a {kind:?} package on Linux"));
    }
    let dest = paths::apps_dir().join(format!("{}.AppImage", app.id));
    fs::create_dir_all(paths::apps_dir()).map_err(|e| e.to_string())?;
    fs::rename(file, &dest).map_err(|e| e.to_string())?;
    github::set_executable(&dest)?;
    let path = dest.to_string_lossy().to_string();
    write_menu_entry(app, &path).map_err(|e| e.to_string())?;
    Ok((path, None))
}

pub fn remove_package(app: &App, info: Option<&Installed>) -> Result<(), String> {
    let mut files = vec![
        paths::apps_dir().join(format!("{}.AppImage", app.id)),
        paths::apps_dir().join(app.id),
    ];
    if let Some(i) = info {
        files.push(PathBuf::from(&i.path));
    }
    for f in files {
        if f.is_file() {
            fs::remove_file(&f).map_err(|e| format!("remove {}: {e}", f.display()))?;
        }
    }
    let src = paths::hub_dir().join("src").join(app.id);
    if src.is_dir() {
        let _ = fs::remove_dir_all(src);
    }
    remove_menu_entry(app);
    Ok(())
}

pub fn launch(app: &App, info: &Installed) -> Result<(), String> {
    Command::new(&info.path)
        .envs(app_env(app, &info.path))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn icon_updated(app: &App, info: &Installed) {
    let _ = write_menu_entry(app, &info.path);
}

/// Open a URL in the default browser.
pub fn open_url(url: &str) {
    let _ = Command::new("xdg-open")
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
}

/// Light or dark from the desktop's settings. winit can't see the system theme on X11
/// (which Cinnamon uses), so read Mint/GNOME's settings directly.
pub fn prefers_dark() -> Option<bool> {
    let get = |schema: &str, key: &str| -> Option<String> {
        let out = Command::new("gsettings")
            .args(["get", schema, key])
            .stderr(Stdio::null())
            .output()
            .ok()?;
        out.status.success().then(|| {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .trim_matches('\'')
                .to_lowercase()
        })
    };
    // Explicit light/dark preference (Mint 21.2+ "Appearance", GNOME 42+).
    for (schema, key) in [
        ("org.x.apps.portal", "color-scheme"),
        ("org.gnome.desktop.interface", "color-scheme"),
    ] {
        match get(schema, key).as_deref() {
            Some("prefer-dark") => return Some(true),
            Some("prefer-light") => return Some(false),
            _ => {}
        }
    }
    // Otherwise go by the GTK theme's name (e.g. Mint-Y-Dark-Aqua vs Mint-Y-Aqua).
    [
        "org.cinnamon.desktop.interface",
        "org.gnome.desktop.interface",
    ]
    .iter()
    .find_map(|schema| get(schema, "gtk-theme").filter(|n| !n.is_empty()))
    .map(|name| name.contains("dark"))
}

// ----------------------------------------------------------------------------- source builds

fn which(cmd: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|d| Path::new(d).join(cmd))
        .find(|p| p.is_file())
}

/// Run a script in a terminal window. gnome-terminal (Cinnamon's) is tried first
/// because `--wait` lets us notice when the window closes.
fn run_in_terminal(script: &Path, args: &[String]) -> Result<Child, String> {
    let terms = [
        "gnome-terminal",
        "xfce4-terminal",
        "mate-terminal",
        "konsole",
        "tilix",
        "x-terminal-emulator",
        "xterm",
    ];
    let (name, term) = terms
        .iter()
        .find_map(|t| which(t).map(|p| (*t, p)))
        .ok_or("No terminal emulator found")?;
    let mut inner = vec!["bash".to_string(), script.to_string_lossy().to_string()];
    inner.extend(args.iter().cloned());
    let mut cmd = Command::new(term);
    match name {
        "gnome-terminal" => cmd.arg("--wait").arg("--").args(&inner),
        "xfce4-terminal" => cmd.arg("--disable-server").arg("-x").args(&inner),
        "konsole" | "tilix" => cmd.arg("-e").args(&inner),
        _ => {
            let quoted: Vec<String> = inner
                .iter()
                .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
                .collect();
            cmd.arg("-e").arg(quoted.join(" "))
        }
    };
    cmd.spawn().map_err(|e| e.to_string())
}

const BUILD_SCRIPT: &str = include_str!("../../scripts/build-from-source.sh");

/// Start a from-source build of `app` in a terminal window.
pub fn start_source_build(app: &App) -> Result<Child, String> {
    let script = paths::hub_dir().join("build-from-source.sh");
    fs::create_dir_all(paths::hub_dir()).map_err(|e| e.to_string())?;
    fs::write(&script, BUILD_SCRIPT).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    run_in_terminal(
        &script,
        &[app.id.to_string(), exe.to_string_lossy().to_string()],
    )
}

/// Install a binary built on this machine (ArtCraft from source).
pub fn register_local(app: &App, built: &Path, version: &str) -> Result<(), String> {
    install::ensure_icon(app);
    fs::create_dir_all(paths::apps_dir()).map_err(|e| e.to_string())?;
    let is_appimage = built.to_string_lossy().ends_with(".AppImage");
    let dest = paths::apps_dir().join(if is_appimage {
        format!("{}.AppImage", app.id)
    } else {
        app.id.to_string()
    });
    for old in [
        paths::apps_dir().join(app.id),
        paths::apps_dir().join(format!("{}.AppImage", app.id)),
    ] {
        if old != dest {
            let _ = fs::remove_file(old);
        }
    }
    let tmp = dest.with_extension("part");
    fs::copy(built, &tmp).map_err(|e| format!("copy {}: {e}", built.display()))?;
    github::set_executable(&tmp)?;
    fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    let path = dest.to_string_lossy().to_string();
    write_menu_entry(app, &path).map_err(|e| e.to_string())?;
    state::update(
        app.id,
        Some(Installed {
            version: version.to_string(),
            path,
            kind: Some(Kind::Source),
            uninstaller: None,
            asset: None,
            verified: false,
            source: true,
            installed_at: state::now(),
        }),
    )
    .map_err(|e| e.to_string())
}

// ----------------------------------------------------------------------------- Craft Hub itself

/// Release asset name for this machine, e.g. `crafthub-linux-x86_64`.
pub fn hub_asset_name() -> String {
    format!("crafthub-linux-{}", github::arch_name())
}

fn remove_if_symlink(p: &Path) {
    if fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        let _ = fs::remove_file(p);
    }
}

pub fn running_installed_copy() -> bool {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|e| fs::canonicalize(e).ok());
    exe.is_some() && exe == fs::canonicalize(installed_hub_path()).ok()
}

pub fn is_self_installed() -> bool {
    installed_hub_path().is_file() && hub_desktop_file().is_file()
}

/// Copy this binary to `~/.local/bin/crafthub` and add Craft Hub to the menu.
/// Also replaces the earlier Python version if it's there.
pub fn install_self() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = installed_hub_path();
    fs::create_dir_all(bin_dir()).map_err(|e| e.to_string())?;
    remove_if_symlink(&target); // the Python version symlinked crafthub.py here
    if fs::canonicalize(&exe).ok() != fs::canonicalize(&target).ok() {
        let tmp = target.with_extension("part");
        fs::copy(&exe, &tmp).map_err(|e| e.to_string())?;
        github::set_executable(&tmp)?;
        fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
    }

    let scalable = hicolor_dir().join("scalable/apps");
    fs::create_dir_all(&scalable).map_err(|e| e.to_string())?;
    fs::create_dir_all(icon_dir()).map_err(|e| e.to_string())?;
    fs::write(scalable.join("crafthub.svg"), HUB_ICON_SVG).map_err(|e| e.to_string())?;
    fs::write(icon_dir().join("crafthub.png"), hub::ICON_PNG).map_err(|e| e.to_string())?;

    fs::create_dir_all(desktop_dir()).map_err(|e| e.to_string())?;
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Craft Hub\n\
         Comment=Install, update and uninstall the Crafting Apps\nExec={}\nIcon=crafthub\n\
         Terminal=false\nStartupWMClass=crafthub\nCategories=Graphics;System;PackageManager;\n\
         Keywords=artcraft;photocraft;vectorcraft;filmcraft;lightcraft;pdfcraft;effectcraft;designcraft;adobe;\n",
        exec_quote(&target.to_string_lossy())
    );
    fs::write(hub_desktop_file(), entry).map_err(|e| e.to_string())?;
    let _ = github::set_executable(&hub_desktop_file());

    // Leftovers from the Python version.
    let _ = fs::remove_dir_all(paths::hub_dir().join("hub"));

    refresh_menus();
    Ok(target)
}

/// Remove Craft Hub's own binary and menu entry.
pub fn uninstall_self() -> Result<(), String> {
    let _ = fs::remove_file(hub_desktop_file());
    let _ = fs::remove_file(hicolor_dir().join("scalable/apps/crafthub.svg"));
    let _ = fs::remove_file(icon_dir().join("crafthub.png"));
    let _ = fs::remove_file(installed_hub_path());
    refresh_menus();
    Ok(())
}

/// Put a downloaded new version in place. Returns what to restart into.
pub fn apply_self_update(downloaded: &Path) -> Result<PathBuf, String> {
    let target = installed_hub_path();
    fs::create_dir_all(bin_dir()).map_err(|e| e.to_string())?;
    remove_if_symlink(&target);
    github::set_executable(downloaded)?;
    fs::rename(downloaded, &target).map_err(|e| e.to_string())?;
    // Let the new version refresh its own menu entry and icon.
    let ok = Command::new(&target)
        .arg("--install-self")
        .stdout(Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    if !ok.success() {
        return Err("The new version failed to install itself".into());
    }
    Ok(target)
}

/// Replace this process with `target`. Only returns (with the error) if that fails.
pub fn restart(target: &Path) -> String {
    Command::new(target).exec().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_quoting() {
        assert_eq!(
            exec_quote("/home/a b/x.AppImage"),
            "\"/home/a b/x.AppImage\""
        );
        assert_eq!(exec_quote("/x/100%"), "\"/x/100%%\"");
        assert_eq!(exec_quote("/x/$y"), "\"/x/\\\\$y\"");
    }
}
