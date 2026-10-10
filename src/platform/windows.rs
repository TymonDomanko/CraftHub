//! Windows: portable apps unpacked into `%LOCALAPPDATA%\CraftHub\apps` with Start Menu
//! shortcuts (no admin rights needed), installers run silently for apps that only ship one
//! (ArtCraft), and Craft Hub installed in `%LOCALAPPDATA%\Programs\CraftHub` with a Start
//! Menu shortcut and an "Apps & features" entry.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

use crate::apps::App;
use crate::github;
use crate::hub;
use crate::install::Kind;
use crate::paths;
use crate::state::Installed;

use super::replace_dir;

pub const OS: &str = "windows";
pub const NAME: &str = "Windows";
pub const SOURCE_BUILDS: bool = false;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const HUB_UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\CraftHub";
const HUB_ICON_ICO: &[u8] = include_bytes!("../../assets/icons/crafthub.ico");

fn start_menu_dir() -> PathBuf {
    let roaming = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| paths::home().join("AppData").join("Roaming"));
    roaming.join(r"Microsoft\Windows\Start Menu\Programs")
}

fn shortcut_path(name: &str) -> PathBuf {
    start_menu_dir().join(format!("{name}.lnk"))
}

/// A command that runs without flashing a console window.
fn hidden(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null());
    cmd
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Create a Start Menu shortcut (via the Windows Script Host COM object).
fn create_shortcut(
    lnk: &Path,
    target: &Path,
    icon: &Path,
    description: &str,
) -> Result<(), String> {
    if let Some(dir) = lnk.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let workdir = target.parent().unwrap_or(target);
    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut({lnk}); \
         $s.TargetPath = {target}; $s.WorkingDirectory = {dir}; $s.IconLocation = {icon}; \
         $s.Description = {desc}; $s.Save()",
        lnk = ps_quote(&lnk.to_string_lossy()),
        target = ps_quote(&target.to_string_lossy()),
        dir = ps_quote(&workdir.to_string_lossy()),
        icon = ps_quote(&format!("{},0", icon.to_string_lossy())),
        desc = ps_quote(description),
    );
    let out = hidden("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|e| format!("Couldn't create the Start Menu shortcut: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Couldn't create the Start Menu shortcut: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

// ----------------------------------------------------------------------------- apps

pub fn notice() -> Option<String> {
    None
}

/// winit reports the Windows theme itself.
pub fn prefers_dark() -> Option<bool> {
    None
}

pub fn install_package(
    app: &App,
    file: &Path,
    kind: Kind,
) -> Result<(String, Option<String>), String> {
    match kind {
        Kind::PortableZip => install_zip(app, file),
        Kind::Setup => install_setup(app, file),
        other => Err(format!("Can't install a {other:?} package on Windows")),
    }
}

/// The folder an archive was unpacked into: its single top-level folder if it has one.
fn unpacked_root(dir: &Path) -> PathBuf {
    let entries: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    match entries.as_slice() {
        [only] if only.is_dir() => only.clone(),
        _ => dir.to_path_buf(),
    }
}

/// Find the app's main `.exe` in an unpacked folder (e.g. `photocraft.exe`, not the CLI).
fn find_exe(root: &Path, app: &App) -> Option<PathBuf> {
    let mut exes = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() && depth < 2 {
                stack.push((p, depth + 1));
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
                exes.push(p);
            }
        }
    }
    let stem = |p: &PathBuf| {
        p.file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    };
    exes.iter()
        .find(|p| stem(p) == app.id || stem(p) == app.name.to_lowercase())
        .or_else(|| {
            exes.iter()
                .find(|p| !stem(p).contains("cli") && !stem(p).contains("unins"))
        })
        .cloned()
}

fn install_zip(app: &App, file: &Path) -> Result<(String, Option<String>), String> {
    let apps = paths::apps_dir();
    fs::create_dir_all(&apps).map_err(|e| e.to_string())?;
    let staging = apps.join(format!(".{}.new", app.id));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let unpack = || -> Result<(), String> {
        let f = fs::File::open(file).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipArchive::new(f).map_err(|e| format!("Bad download: {e}"))?;
        zip.extract(&staging)
            .map_err(|e| format!("Couldn't unpack {}: {e}", app.name))
    };
    if let Err(e) = unpack() {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    let root = unpacked_root(&staging);
    // The portable build keeps its settings in its own folder while portable.txt is there,
    // which an update would wipe. Without it, settings go to %APPDATA% as usual.
    let _ = fs::remove_file(root.join("portable.txt"));
    let Some(exe) = find_exe(&root, app) else {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!(
            "Couldn't find {}'s program file in the download",
            app.name
        ));
    };
    let rel = exe
        .strip_prefix(&root)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let dest = apps.join(app.id);
    let placed = replace_dir(&root, &dest, app.name);
    let _ = fs::remove_dir_all(&staging);
    placed?;
    let exe = dest.join(rel);
    create_shortcut(&shortcut_path(app.name), &exe, &exe, app.comment())?;
    Ok((exe.to_string_lossy().to_string(), None))
}

/// An installed program's entry in "Apps & features".
struct UninstallEntry {
    exe: Option<PathBuf>,
    uninstall: Option<String>,
}

fn find_uninstall_entry(display_name: &str) -> Option<UninstallEntry> {
    let wow = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";
    for (hive, path) in [
        (HKEY_CURRENT_USER, UNINSTALL_KEY),
        (HKEY_LOCAL_MACHINE, UNINSTALL_KEY),
        (HKEY_LOCAL_MACHINE, wow),
    ] {
        let Ok(root) = RegKey::predef(hive).open_subkey(path) else {
            continue;
        };
        for name in root.enum_keys().flatten() {
            let Ok(key) = root.open_subkey(&name) else {
                continue;
            };
            let Ok(dn) = key.get_value::<String, _>("DisplayName") else {
                continue;
            };
            if !dn.trim().eq_ignore_ascii_case(display_name) {
                continue;
            }
            let get = |v: &str| {
                key.get_value::<String, _>(v)
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            };
            let exe = get("DisplayIcon")
                .map(|icon| {
                    let icon = icon.trim().trim_matches('"');
                    let icon = icon.rsplit_once(',').map_or(icon, |(p, _)| p);
                    PathBuf::from(icon.trim_matches('"'))
                })
                .filter(|p| {
                    p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) && p.is_file()
                })
                .or_else(|| {
                    get("InstallLocation")
                        .map(|loc| {
                            PathBuf::from(loc.trim_matches('"')).join(format!("{display_name}.exe"))
                        })
                        .filter(|p| p.is_file())
                });
            let uninstall = get("QuietUninstallString")
                .or_else(|| get("UninstallString").map(|u| format!("{u} /S")));
            return Some(UninstallEntry { exe, uninstall });
        }
    }
    None
}

/// Run an installer `.exe` and wait. Asks for elevation only if the installer needs it.
fn run_installer(exe: &Path, args: &[&str]) -> Result<(), String> {
    match hidden(exe).args(args).status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!(
            "The installer stopped (exit code {})",
            s.code().unwrap_or(-1)
        )),
        // ERROR_ELEVATION_REQUIRED: the installer is per-machine; let Windows ask.
        Err(e) if e.raw_os_error() == Some(740) => {
            let script = format!(
                "$p = Start-Process -FilePath {} -ArgumentList {} -Verb RunAs -Wait -PassThru; exit $p.ExitCode",
                ps_quote(&exe.to_string_lossy()),
                ps_quote(&args.join(" "))
            );
            let s = hidden("powershell")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    &script,
                ])
                .status()
                .map_err(|e| e.to_string())?;
            if s.success() {
                Ok(())
            } else {
                Err("The installer was cancelled or failed".into())
            }
        }
        Err(e) => Err(format!("Couldn't run the installer: {e}")),
    }
}

/// Apps that only ship an installer (ArtCraft's Tauri/NSIS `*-setup.exe`): run it silently.
fn install_setup(app: &App, file: &Path) -> Result<(String, Option<String>), String> {
    // Windows only runs installers whose name ends in .exe.
    let exe_file = file.with_extension("exe");
    if exe_file != file {
        fs::rename(file, &exe_file).map_err(|e| e.to_string())?;
    }
    let result = run_installer(&exe_file, &["/S"]);
    let _ = fs::remove_file(&exe_file);
    result?;
    let entry = find_uninstall_entry(app.name).ok_or_else(|| {
        format!(
            "{} was installed, but Craft Hub couldn't find where",
            app.name
        )
    })?;
    let exe = entry.exe.ok_or_else(|| {
        format!(
            "{} was installed, but Craft Hub couldn't find its program file",
            app.name
        )
    })?;
    Ok((exe.to_string_lossy().to_string(), entry.uninstall))
}

/// Split an uninstall command line into program and arguments.
fn split_command(cmd: &str) -> (String, Vec<String>) {
    let cmd = cmd.trim();
    let (prog, rest) = if let Some(stripped) = cmd.strip_prefix('"') {
        match stripped.split_once('"') {
            Some((p, r)) => (p.to_string(), r),
            None => (stripped.to_string(), ""),
        }
    } else {
        // Unquoted: the program runs up to ".exe".
        match cmd.to_lowercase().find(".exe") {
            Some(i) => (cmd[..i + 4].to_string(), &cmd[i + 4..]),
            None => (cmd.to_string(), ""),
        }
    };
    (prog, rest.split_whitespace().map(str::to_string).collect())
}

pub fn remove_package(app: &App, info: Option<&Installed>) -> Result<(), String> {
    if let Some(i) = info.filter(|i| i.kind == Some(Kind::Setup)) {
        let cmd = i
            .uninstaller
            .clone()
            .or_else(|| find_uninstall_entry(app.name).and_then(|e| e.uninstall))
            .ok_or_else(|| format!("Couldn't find {}'s uninstaller", app.name))?;
        let (prog, args) = split_command(&cmd);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        run_installer(Path::new(&prog), &args)?;
        // NSIS uninstallers copy themselves and return early; wait for the files to go.
        for _ in 0..60 {
            if !Path::new(&i.path).exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        return Ok(());
    }
    let dir = paths::apps_dir().join(app.id);
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|_| format!("{} is in use. Close it and try again.", app.name))?;
    }
    let _ = fs::remove_file(shortcut_path(app.name));
    Ok(())
}

pub fn launch(_app: &App, info: &Installed) -> Result<(), String> {
    let exe = Path::new(&info.path);
    Command::new(exe)
        .current_dir(exe.parent().unwrap_or(exe))
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Shortcuts use the app's own icon, so nothing to update.
pub fn icon_updated(_app: &App, _info: &Installed) {}

pub fn open_url(url: &str) {
    let _ = hidden("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
}

pub fn start_source_build(_app: &App) -> Result<Child, String> {
    Err("Building from source isn't available on Windows".into())
}

// ----------------------------------------------------------------------------- Craft Hub itself

pub fn installed_hub_path() -> PathBuf {
    paths::local_app_data().join(r"Programs\CraftHub\crafthub.exe")
}

pub fn hub_asset_name() -> String {
    format!("crafthub-windows-{}.exe", github::arch_name())
}

fn old_copy(target: &Path) -> PathBuf {
    target.with_file_name("crafthub.exe.old")
}

pub fn running_installed_copy() -> bool {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|e| fs::canonicalize(e).ok());
    exe.is_some() && exe == fs::canonicalize(installed_hub_path()).ok()
}

pub fn is_self_installed() -> bool {
    installed_hub_path().is_file() && shortcut_path("Craft Hub").is_file()
}

/// Copy this program to `%LOCALAPPDATA%\Programs\CraftHub`, add a Start Menu shortcut and
/// an "Apps & features" entry.
pub fn install_self() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = installed_hub_path();
    let dir = target.parent().unwrap_or(&target).to_path_buf();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if fs::canonicalize(&exe).ok() != fs::canonicalize(&target).ok() {
        // A running copy can't be overwritten, but it can be moved aside.
        if target.exists() {
            let _ = fs::remove_file(old_copy(&target));
            let _ = fs::rename(&target, old_copy(&target));
        }
        fs::copy(&exe, &target).map_err(|e| e.to_string())?;
    }
    let ico = dir.join("crafthub.ico");
    let _ = fs::write(&ico, HUB_ICON_ICO);
    create_shortcut(
        &shortcut_path("Craft Hub"),
        &target,
        &target,
        "Install, update and uninstall the Crafting Apps",
    )?;

    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(HUB_UNINSTALL_KEY)
        .map_err(|e| e.to_string())?;
    let t = target.to_string_lossy().to_string();
    let values = [
        ("DisplayName", "Craft Hub".to_string()),
        ("DisplayVersion", hub::VERSION.to_string()),
        ("Publisher", "Tymon Domanko".to_string()),
        ("DisplayIcon", format!("\"{t}\",0")),
        ("InstallLocation", dir.to_string_lossy().to_string()),
        ("UninstallString", format!("\"{t}\" --uninstall-self")),
        ("QuietUninstallString", format!("\"{t}\" --uninstall-self")),
        (
            "URLInfoAbout",
            "https://github.com/TymonDomanko/CraftHub".to_string(),
        ),
    ];
    for (name, value) in values {
        key.set_value(name, &value).map_err(|e| e.to_string())?;
    }
    let size_kb = fs::metadata(&target)
        .map(|m| (m.len() / 1024) as u32)
        .unwrap_or(0);
    let _ = key.set_value("EstimatedSize", &size_kb);
    let _ = key.set_value("NoModify", &1u32);
    let _ = key.set_value("NoRepair", &1u32);
    Ok(target)
}

/// Remove Craft Hub's own program, shortcut and "Apps & features" entry.
pub fn uninstall_self() -> Result<(), String> {
    let _ = fs::remove_file(shortcut_path("Craft Hub"));
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(HUB_UNINSTALL_KEY);
    let target = installed_hub_path();
    let dir = target.parent().unwrap_or(&target).to_path_buf();
    if running_installed_copy() {
        // A running program can't delete itself; remove the folder once we've exited.
        let cmd = format!(
            "ping -n 3 127.0.0.1 >NUL & rmdir /S /Q \"{}\"",
            dir.to_string_lossy()
        );
        let _ = hidden("cmd")
            .args(["/C", &cmd])
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn();
    } else {
        let _ = fs::remove_dir_all(&dir);
    }
    Ok(())
}

/// Put a downloaded new version in place. Returns what to restart into.
pub fn apply_self_update(downloaded: &Path) -> Result<PathBuf, String> {
    let target = installed_hub_path();
    fs::create_dir_all(target.parent().unwrap_or(&target)).map_err(|e| e.to_string())?;
    if target.exists() {
        let _ = fs::remove_file(old_copy(&target));
        fs::rename(&target, old_copy(&target)).map_err(|e| e.to_string())?;
    }
    if let Err(e) =
        fs::rename(downloaded, &target).or_else(|_| fs::copy(downloaded, &target).map(|_| ()))
    {
        let _ = fs::rename(old_copy(&target), &target);
        return Err(e.to_string());
    }
    // Let the new version refresh its shortcut and "Apps & features" entry.
    let _ = hidden(&target)
        .arg("--install-self")
        .stdout(Stdio::null())
        .status();
    Ok(target)
}

/// Start `target` and exit this process.
pub fn restart(target: &Path) -> String {
    match Command::new(target)
        .creation_flags(DETACHED_PROCESS)
        .spawn()
    {
        Ok(_) => std::process::exit(0),
        Err(e) => e.to_string(),
    }
}

/// Housekeeping when the window opens: remove copies left behind by updates.
pub fn startup() {
    let _ = fs::remove_file(old_copy(&installed_hub_path()));
    for e in fs::read_dir(paths::apps_dir())
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && (name.ends_with(".old") || name.ends_with(".new")) {
            let _ = fs::remove_dir_all(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_uninstall_commands() {
        let (p, a) = split_command(r#""C:\Users\me\AppData\Local\ArtCraft\uninstall.exe" /S"#);
        assert_eq!(p, r"C:\Users\me\AppData\Local\ArtCraft\uninstall.exe");
        assert_eq!(a, vec!["/S"]);
        let (p, a) = split_command(r"C:\Program Files\X\unins000.exe /SILENT");
        assert_eq!(p, r"C:\Program Files\X\unins000.exe");
        assert_eq!(a, vec!["/SILENT"]);
    }
}
