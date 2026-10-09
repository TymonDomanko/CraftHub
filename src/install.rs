//! Installing, updating and uninstalling the Crafting Apps, and their menu entries.

use std::borrow::Cow;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;

use crate::apps::{self, APPS, App, OWNER};
use crate::github::{self, Release};
use crate::paths;
use crate::state::{self, Installed};

const MARKER: &str = "X-CraftHub=true";

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

/// Environment for launching an app.
pub fn app_env(app: &App, path: &str) -> Vec<(String, String)> {
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

pub fn exec_line(app: &App, path: &str) -> String {
    let mut env = app_env(app, path);
    env.sort();
    let env: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!("env {} {} %F", env.join(" "), exec_quote(path))
}

fn desktop_path(desktop_id: &str) -> PathBuf {
    paths::desktop_dir().join(format!("{desktop_id}.desktop"))
}

/// Ask the desktop to reload menus and icons.
pub fn refresh_menus() {
    let jobs: [(&str, Vec<String>); 2] = [
        (
            "update-desktop-database",
            vec!["-q".into(), paths::desktop_dir().display().to_string()],
        ),
        (
            "gtk-update-icon-cache",
            vec![
                "-q".into(),
                "-t".into(),
                "-f".into(),
                paths::hicolor_dir().display().to_string(),
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
pub fn write_menu_entry(app: &App, path: &str) -> std::io::Result<()> {
    fs::create_dir_all(paths::desktop_dir())?;
    fs::create_dir_all(paths::icon_dir())?;
    remove_legacy(app);
    fs::write(
        paths::icon_dir().join(format!("{}.png", app.desktop_id)),
        icon_bytes(app),
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

fn cached_icon_path(app: &App) -> PathBuf {
    paths::hub_dir()
        .join("icons")
        .join(format!("{}.png", app.id))
}

/// The app's icon: the downloaded one when it has an `icon_url`, else the bundled one.
pub fn icon_bytes(app: &App) -> Cow<'static, [u8]> {
    let cached = app
        .icon_url
        .and_then(|_| fs::read(cached_icon_path(app)).ok())
        .filter(|b| b.starts_with(b"\x89PNG"));
    match cached {
        Some(b) => Cow::Owned(b),
        None => Cow::Borrowed(app.icon_png),
    }
}

/// Download icons we don't redistribute (ArtCraft's logo) from the projects themselves,
/// at most once a week. Returns the apps whose icon changed.
pub fn refresh_remote_icons() -> Vec<&'static App> {
    let mut changed = Vec::new();
    for app in APPS {
        let Some(url) = app.icon_url else { continue };
        let path = cached_icon_path(app);
        let fresh = fs::metadata(&path)
            .and_then(|m| m.modified())
            .map(|t| t.elapsed().unwrap_or_default() < std::time::Duration::from_secs(7 * 86400))
            .unwrap_or(false);
        if fresh {
            continue;
        }
        let url = match std::env::var("CRAFTHUB_RAW") {
            Ok(base) => url.replace("https://raw.githubusercontent.com", &base),
            Err(_) => url.to_string(),
        };
        let Ok(mut resp) = github::agent().get(&url).call() else {
            continue;
        };
        let Ok(bytes) = resp.body_mut().with_config().limit(4 << 20).read_to_vec() else {
            continue;
        };
        if !bytes.starts_with(b"\x89PNG") {
            continue;
        }
        let old = fs::read(&path).ok();
        let _ = fs::create_dir_all(path.parent().unwrap_or(&path));
        if fs::write(&path, &bytes).is_ok() && old.as_deref() != Some(&bytes[..]) {
            if let Some(info) = state::installed(app.id) {
                let _ = write_menu_entry(app, &info.path);
            }
            changed.push(app);
        }
    }
    changed
}

/// Entries from Craft Hub 1.0, which used `crafthub-<id>` names.
fn remove_legacy(app: &App) {
    let _ = fs::remove_file(paths::desktop_dir().join(format!("crafthub-{}.desktop", app.id)));
    let _ = fs::remove_file(paths::icon_dir().join(format!("crafthub-{}.png", app.id)));
}

/// A menu entry we wrote, or one an app wrote for itself pointing into Craft Hub's
/// apps folder (PhotoCraft and EffectCraft do this on first launch).
fn ours_or_orphaned(dpath: &Path) -> bool {
    let Ok(text) = fs::read_to_string(dpath) else {
        return false;
    };
    text.contains(MARKER) || text.contains(&*paths::apps_dir().to_string_lossy())
}

pub fn remove_menu_entry(app: &App) {
    remove_legacy(app);
    let dpath = desktop_path(app.desktop_id);
    if ours_or_orphaned(&dpath) {
        let _ = fs::remove_file(&dpath);
        // Icons we (or the app) put in the user's icon folder, at every size.
        if let Ok(sizes) = fs::read_dir(paths::hicolor_dir()) {
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
pub fn cleanup_orphans() {
    let st = state::load();
    for app in APPS {
        if st.get(app.id).is_some_and(Installed::exists) {
            continue;
        }
        let stale = [
            desktop_path(app.desktop_id),
            paths::desktop_dir().join(format!("crafthub-{}.desktop", app.id)),
        ]
        .iter()
        .any(|p| p.exists() && ours_or_orphaned(p));
        if stale {
            remove_menu_entry(app);
        }
    }
}

/// Rewrite entries for installed apps that use an old name or are missing.
pub fn migrate_entries() {
    for (id, info) in state::load() {
        let Some(app) = apps::by_id(&id) else {
            continue;
        };
        if !info.exists() {
            continue;
        }
        let legacy = paths::desktop_dir().join(format!("crafthub-{id}.desktop"));
        if legacy.exists() || !desktop_path(app.desktop_id).exists() {
            let _ = write_menu_entry(app, &info.path);
        }
    }
}

/// The latest release of an app and the AppImage for this machine, if it has one.
#[derive(Clone)]
pub struct AppRelease {
    pub release: Release,
    pub appimage: Option<(String, String)>,
    pub sums_url: Option<String>,
}

impl AppRelease {
    pub fn tag(&self) -> &str {
        &self.release.tag
    }
}

pub fn latest_for(app: &App) -> Result<Option<AppRelease>, String> {
    let Some(release) = github::latest_release(OWNER, app.id)? else {
        return Ok(None);
    };
    let (appimage, sums_url) = if release.guessed {
        // Rate-limited: rebuild the URLs from the projects' naming scheme.
        if app.source_build {
            (None, None)
        } else {
            let ver = github::pretty_version(&release.tag);
            let base = format!(
                "{}/{OWNER}/{}/releases/download/{}",
                github::web_base(),
                app.id,
                release.tag
            );
            let name = format!("{}-{ver}-linux-{}.AppImage", app.id, github::arch_name());
            (
                Some((name.clone(), format!("{base}/{name}"))),
                Some(format!("{base}/SHA256SUMS.txt")),
            )
        }
    } else {
        let name = github::pick_appimage(release.assets.iter().map(|(n, _)| n.as_str()));
        let appimage = name.map(|n| {
            (
                n.to_string(),
                release.asset_url(n).unwrap_or_default().to_string(),
            )
        });
        (
            appimage,
            release.asset_url("SHA256SUMS.txt").map(str::to_string),
        )
    };
    Ok(Some(AppRelease {
        release,
        appimage,
        sums_url,
    }))
}

/// Download an app's AppImage and install it (also used for updates).
pub fn install_release(
    app: &App,
    rel: &AppRelease,
    progress: &dyn Fn(u64, u64),
) -> Result<(), String> {
    let (name, url) = rel
        .appimage
        .as_ref()
        .ok_or("The latest release has no Linux AppImage")?;
    let dest = paths::apps_dir().join(format!("{}.AppImage", app.id));
    let verified = github::download_verified(url, name, rel.sums_url.as_deref(), &dest, progress)?;
    let path = dest.to_string_lossy().to_string();
    write_menu_entry(app, &path).map_err(|e| e.to_string())?;
    state::update(
        app.id,
        Some(Installed {
            version: rel.tag().to_string(),
            path,
            asset: Some(name.clone()),
            verified,
            source: false,
            installed_at: state::now(),
        }),
    )
    .map_err(|e| e.to_string())
}

/// Install a binary built on this machine (ArtCraft from source).
pub fn register_local(app: &App, built: &Path, version: &str) -> Result<(), String> {
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
            asset: None,
            verified: false,
            source: true,
            installed_at: state::now(),
        }),
    )
    .map_err(|e| e.to_string())
}

pub fn uninstall(app: &App) -> Result<(), String> {
    let info = state::load().remove(app.id);
    let mut files = vec![
        paths::apps_dir().join(format!("{}.AppImage", app.id)),
        paths::apps_dir().join(app.id),
    ];
    if let Some(i) = &info {
        files.push(PathBuf::from(&i.path));
    }
    for f in files {
        if f.is_file() {
            fs::remove_file(&f).map_err(|e| format!("remove {}: {e}", f.display()))?;
        }
    }
    let src = paths::src_dir().join(app.id);
    if src.is_dir() {
        let _ = fs::remove_dir_all(src);
    }
    remove_menu_entry(app);
    state::update(app.id, None).map_err(|e| e.to_string())
}

pub fn launch(app: &App) -> Result<(), String> {
    let info = state::installed(app.id).ok_or("Not installed")?;
    let mut cmd = Command::new(&info.path);
    cmd.envs(app_env(app, &info.path))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
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

fn which(cmd: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|d| Path::new(d).join(cmd))
        .find(|p| p.is_file())
}

/// Run a script in a terminal window. gnome-terminal (Cinnamon's) is tried first
/// because `--wait` lets us notice when the window closes.
pub fn run_in_terminal(script: &Path, args: &[String]) -> Result<Child, String> {
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

const BUILD_SCRIPT: &str = include_str!("../scripts/build-from-source.sh");

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
