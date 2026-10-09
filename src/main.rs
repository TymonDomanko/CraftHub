//! Craft Hub: a Creative-Cloud-style manager for the ArtCraft "Crafting Apps" on Linux.
//! One-click install, update and uninstall for PhotoCraft, VectorCraft, DesignCraft,
//! LightCraft, FilmCraft, EffectCraft, PDFCraft and ArtCraft.

mod apps;
mod github;
mod hub;
mod install;
mod paths;
mod state;
mod ui;

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use eframe::egui;

const USAGE: &str = "\
Craft Hub: install, update and uninstall the Crafting Apps.

Usage:
  crafthub                          Open the window
  crafthub --install-self           Install Craft Hub for this user (menu entry + ~/.local/bin/crafthub)
  crafthub --uninstall-self [--all] Remove Craft Hub (--all also removes every app it installed)
  crafthub --list                   Show installed apps
  crafthub --check                  Show the latest release of each app
  crafthub --register ID PATH VER   Install a locally built app (used by the ArtCraft source build)
  crafthub --version
";

fn fail(msg: impl std::fmt::Display) -> ExitCode {
    eprintln!("crafthub: {msg}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
        ["--install-self"] => match hub::install_self() {
            Ok(path) => {
                println!(
                    "Craft Hub {} is installed ({}).",
                    hub::VERSION,
                    path.display()
                );
                println!("Open it from your menu (search \"Craft Hub\") or run: crafthub");
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        },
        ["--uninstall-self", rest @ ..] => {
            let all = rest.contains(&"--all");
            match hub::uninstall_self(all) {
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
                        r.appimage
                            .as_ref()
                            .map_or("(no Linux AppImage)", |(n, _)| n.as_str())
                    ),
                    Ok(None) => println!("{:<12} no releases", app.name),
                    Err(e) => println!("{:<12} error: {e}", app.name),
                }
            }
            ExitCode::SUCCESS
        }
        ["--register", id, path, version] => {
            let Some(app) = apps::by_id(id) else {
                return fail(format!("unknown app {id}"));
            };
            match install::register_local(app, Path::new(path), version) {
                Ok(()) => {
                    println!("Installed {} {version} from {path}", app.name);
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        _ => {
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run_gui() -> ExitCode {
    // First run (or running a downloaded copy): put Craft Hub in the menu.
    let running_installed = std::env::current_exe()
        .ok()
        .and_then(|e| std::fs::canonicalize(e).ok())
        .is_some_and(|e| std::fs::canonicalize(paths::installed_hub_binary()).ok() == Some(e));
    if (!running_installed || !hub::is_self_installed())
        && let Err(e) = hub::install_self()
    {
        eprintln!("crafthub: couldn't add Craft Hub to the menu: {e}");
    }
    install::cleanup_orphans();
    install::migrate_entries();

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
