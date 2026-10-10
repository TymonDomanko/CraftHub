//! Craft Hub: a Creative-Cloud-style manager for the ArtCraft "Crafting Apps".
//! One-click install, update and uninstall for PhotoCraft, VectorCraft, DesignCraft,
//! LightCraft, FilmCraft, EffectCraft, PDFCraft and ArtCraft on Linux, Windows and macOS.

// A windowed program on Windows (no console window); command-line use still prints.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod apps;
mod github;
mod hub;
mod install;
mod paths;
mod platform;
mod state;
mod ui;

use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;

use eframe::egui;

const USAGE: &str = "\
Craft Hub: install, update and uninstall the Crafting Apps.

Usage:
  crafthub                          Open the window
  crafthub --install ID             Install an app (e.g. photocraft)
  crafthub --update ID|all          Update an app, or every installed app
  crafthub --uninstall ID           Uninstall an app
  crafthub --open ID                Open an installed app
  crafthub --list                   Show installed apps
  crafthub --check                  Show the latest release of each app
  crafthub --install-self           Install Craft Hub for this user
  crafthub --delete-data ID         Delete an uninstalled app's settings and saved data
  crafthub --uninstall-self [--all] [--purge]
                                    Remove Craft Hub; --all also uninstalls every app it
                                    installed, --purge also deletes their settings
  crafthub --version
";

fn fail(msg: impl std::fmt::Display) -> ExitCode {
    eprintln!("crafthub: {msg}");
    ExitCode::FAILURE
}

/// On Windows, a windowed program has no console: borrow the one it was started from
/// (if any) so command-line output shows up.
#[cfg(windows)]
fn attach_console() {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE,
    };
    // SAFETY: plain Win32 calls with constant arguments.
    unsafe {
        let out = GetStdHandle(STD_OUTPUT_HANDLE);
        if out.is_null() || out == INVALID_HANDLE_VALUE {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

fn app_arg(id: &str) -> Result<&'static apps::App, ExitCode> {
    apps::by_id(&id.to_lowercase()).ok_or_else(|| {
        let ids: Vec<&str> = apps::APPS.iter().map(|a| a.id).collect();
        fail(format!(
            "unknown app \"{id}\" (choose from: {})",
            ids.join(", ")
        ))
    })
}

fn cli_progress(done: u64, total: u64) {
    if total > 0 {
        eprint!(
            "\r  {:>5.1} / {:.1} MB",
            done as f64 / 1e6,
            total as f64 / 1e6
        );
        let _ = std::io::stderr().flush();
    }
}

/// Install or update one app from the command line.
fn cli_install(app: &apps::App, update_only: bool) -> Result<String, String> {
    let installed = state::installed(app.id);
    if update_only && installed.is_none() {
        return Ok(format!("{} isn't installed", app.name));
    }
    let rel = install::latest_for(app)?.ok_or("No releases yet")?;
    if rel.asset.is_none() {
        return Err(format!(
            "{}'s latest release has no build for {}",
            app.name,
            platform::NAME
        ));
    }
    if let Some(i) = &installed
        && !i.source
        && i.version == rel.tag()
    {
        return Ok(format!("{} is up to date ({})", app.name, rel.tag()));
    }
    eprintln!(
        "{} {} {}…",
        if installed.is_some() {
            "Updating"
        } else {
            "Installing"
        },
        app.name,
        rel.tag()
    );
    let result = install::install_release(app, &rel, &cli_progress);
    eprintln!();
    result?;
    let path = state::installed(app.id).map(|i| i.path).unwrap_or_default();
    Ok(format!("{} {} is installed: {path}", app.name, rel.tag()))
}

fn report(result: Result<String, String>) -> ExitCode {
    match result {
        Ok(msg) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    if !args.is_empty() {
        attach_console();
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] => run_gui(),
        ["--version" | "-V"] => {
            println!("Craft Hub {}", hub::VERSION);
            ExitCode::SUCCESS
        }
        ["--help" | "-h"] => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        ["--install", id] => match app_arg(id) {
            Ok(app) => report(cli_install(app, false)),
            Err(code) => code,
        },
        ["--update", "all"] => {
            let mut code = ExitCode::SUCCESS;
            for app in apps::APPS {
                if state::installed(app.id).is_some()
                    && let Err(e) = cli_install(app, true).map(|m| println!("{m}"))
                {
                    code = fail(format!("{}: {e}", app.name));
                }
            }
            code
        }
        ["--update", id] => match app_arg(id) {
            Ok(app) => report(cli_install(app, true)),
            Err(code) => code,
        },
        ["--uninstall", id] => match app_arg(id) {
            Ok(app) if state::load().contains_key(app.id) => {
                report(install::uninstall(app).map(|_| format!("Uninstalled {}", app.name)))
            }
            Ok(app) => report(Ok(format!("{} isn't installed", app.name))),
            Err(code) => code,
        },
        ["--open", id] => match app_arg(id) {
            Ok(app) => report(install::launch(app).map(|_| format!("Opened {}", app.name))),
            Err(code) => code,
        },
        ["--install-self"] => match hub::install_self() {
            Ok(path) => {
                println!(
                    "Craft Hub {} is installed ({}).",
                    hub::VERSION,
                    path.display()
                );
                println!("Open it from your menu, Start Menu or Applications folder.");
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        },
        ["--delete-data", id] => match app_arg(id) {
            Ok(app) => {
                let found = install::app_data(app);
                if found.is_empty() {
                    return report(Ok(format!(
                        "{} has no settings or saved data here",
                        app.name
                    )));
                }
                let size = install::human_size(install::size_of(&found));
                report(install::delete_app_data(app).map(|_| {
                    let list: Vec<String> =
                        found.iter().map(|p| format!("  {}", p.display())).collect();
                    format!(
                        "Deleted {}'s settings and data ({size}):\n{}",
                        app.name,
                        list.join("\n")
                    )
                }))
            }
            Err(code) => code,
        },
        ["--uninstall-self", rest @ ..] => {
            let purge = rest.contains(&"--purge");
            let all = purge || rest.contains(&"--all");
            match hub::uninstall_self(all, purge) {
                Ok(()) if purge => {
                    println!("Removed Craft Hub, every app it installed, and their settings.");
                    ExitCode::SUCCESS
                }
                Ok(()) if all => {
                    println!("Removed Craft Hub and every app it installed.");
                    ExitCode::SUCCESS
                }
                Ok(()) => {
                    println!(
                        "Removed Craft Hub. Apps it installed are kept (use --all to remove them too)."
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        ["--list"] => {
            let st = state::load();
            for app in apps::APPS {
                let v = st
                    .get(app.id)
                    .filter(|i| i.exists())
                    .map_or("-".to_string(), |i| i.version.clone());
                println!("{:<12} {v}", app.name);
            }
            ExitCode::SUCCESS
        }
        ["--check"] => {
            for app in apps::APPS {
                match install::latest_for(app) {
                    Ok(Some(r)) => println!(
                        "{:<12} {:<20} {}",
                        app.name,
                        r.tag(),
                        r.asset.as_ref().map_or_else(
                            || format!("(no build for {} {})", platform::NAME, github::arch_name()),
                            |a| a.name.clone()
                        )
                    ),
                    Ok(None) => println!("{:<12} no releases", app.name),
                    Err(e) => println!("{:<12} error: {e}", app.name),
                }
            }
            ExitCode::SUCCESS
        }
        #[cfg(target_os = "linux")]
        ["--register", id, path, version] => match app_arg(id) {
            Ok(app) => report(
                platform::register_local(app, std::path::Path::new(path), version)
                    .map(|_| format!("Installed {} {version} from {path}", app.name)),
            ),
            Err(code) => code,
        },
        _ => {
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run_gui() -> ExitCode {
    // Craft Hub is installed as a package (from Software Manager), but this is an older
    // per-user copy or a downloaded file: open the packaged one instead.
    if let Some(packaged) = platform::packaged_copy() {
        let err = platform::restart(&packaged);
        eprintln!("crafthub: couldn't start {}: {err}", packaged.display());
    }
    // First run (or running a downloaded copy): install Craft Hub for this user.
    if (!platform::running_installed_copy() || !platform::is_self_installed())
        && let Err(e) = hub::install_self()
    {
        eprintln!("crafthub: couldn't install Craft Hub: {e}");
    }
    platform::startup();

    let (rgba, width, height) = ui::decode_png(hub::ICON_PNG);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Craft Hub")
            .with_app_id("crafthub")
            .with_inner_size([1080.0, 860.0])
            .with_min_inner_size([480.0, 400.0])
            .with_icon(Arc::new(egui::IconData {
                rgba,
                width,
                height,
            })),
        ..Default::default()
    };
    match eframe::run_native(
        "Craft Hub",
        options,
        Box::new(|cc| Ok(Box::new(ui::HubApp::new(cc)))),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(format!("couldn't open the window: {e}")),
    }
}
