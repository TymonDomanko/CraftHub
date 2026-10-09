//! The Craft Hub window.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke, TextureHandle,
    TextureOptions, Theme, Vec2, load::SizedTexture,
};

use crate::apps::{self, APPS, App};
use crate::github::{self, pretty_version};
use crate::hub::{self, HubRelease};
use crate::install::{self, AppRelease};
use crate::state::{self, Installed};

// ----------------------------------------------------------------------------- state shared with workers

#[derive(Clone)]
enum Check {
    Pending,
    /// `None`: the project has no releases.
    Done(Option<AppRelease>),
    /// Couldn't reach GitHub; the error message.
    Failed(String),
}

#[derive(Clone)]
struct Busy {
    label: String,
    /// Download progress 0..1, or `None` for an indeterminate bar.
    frac: Option<f32>,
    bytes: Option<(u64, u64)>,
}

struct Build {
    child: Child,
    started: u64,
    started_at: Instant,
}

struct CardState {
    check: Check,
    busy: Option<Busy>,
    /// Message shown in the card, and whether it's an error.
    note: Option<(String, bool)>,
    build: Option<Build>,
}

struct Shared {
    cards: HashMap<&'static str, CardState>,
    pending_checks: usize,
    hub_release: Option<HubRelease>,
    hub_busy: Option<Busy>,
    hub_note: Option<(String, bool)>,
    restart_to: Option<PathBuf>,
    state_dirty: bool,
    /// Apps whose icon was just downloaded and needs reloading.
    new_icons: Vec<&'static str>,
}

type SharedRef = Arc<Mutex<Shared>>;

fn lock(s: &SharedRef) -> MutexGuard<'_, Shared> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

// ----------------------------------------------------------------------------- per-frame view of a card

struct CardView {
    status: String,
    note: Option<(String, bool)>,
    busy: Option<Busy>,
    installed: Option<Installed>,
    can_install: bool,
    can_update: bool,
    /// A newer release is out (not counting "rebuild" for source builds).
    has_update: bool,
    source: bool,
}

enum Action {
    Install(&'static str),
    Update(&'static str),
    Uninstall(&'static str),
    Open(&'static str),
    Url(String),
    Refresh,
    UpdateAll,
    UpdateHub,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Primary,
    Danger,
    Normal,
}

struct Palette {
    card: Color32,
    card_stroke: Color32,
    bar: Color32,
    muted: Color32,
    accent: Color32,
    danger: Color32,
    ok_bg: Color32,
    ok_fg: Color32,
    warn_bg: Color32,
    warn_fg: Color32,
    banner: Color32,
    error: Color32,
}

fn palette(dark: bool) -> Palette {
    let rgb = Color32::from_rgb;
    if dark {
        Palette {
            card: rgb(0x2a, 0x2b, 0x31),
            card_stroke: rgb(0x3a, 0x3c, 0x44),
            bar: rgb(0x24, 0x25, 0x2a),
            muted: rgb(0xa4, 0xa7, 0xb0),
            accent: rgb(0x35, 0x84, 0xe4),
            danger: rgb(0xc0, 0x1c, 0x28),
            ok_bg: rgb(0x1f, 0x3a, 0x5c),
            ok_fg: rgb(0x99, 0xc1, 0xf1),
            warn_bg: rgb(0x4a, 0x3a, 0x12),
            warn_fg: rgb(0xf8, 0xd0, 0x6a),
            banner: rgb(0x22, 0x33, 0x4a),
            error: rgb(0xff, 0x7b, 0x72),
        }
    } else {
        Palette {
            card: Color32::WHITE,
            card_stroke: rgb(0xdd, 0xde, 0xe3),
            bar: rgb(0xfa, 0xfa, 0xfb),
            muted: rgb(0x5e, 0x61, 0x6b),
            accent: rgb(0x1c, 0x71, 0xd8),
            danger: rgb(0xc0, 0x1c, 0x28),
            ok_bg: rgb(0xdc, 0xe9, 0xfb),
            ok_fg: rgb(0x1a, 0x5f, 0xb4),
            warn_bg: rgb(0xfb, 0xee, 0xc8),
            warn_fg: rgb(0x8a, 0x5a, 0x00),
            banner: rgb(0xe3, 0xee, 0xfb),
            error: rgb(0xc0, 0x1c, 0x28),
        }
    }
}

// ----------------------------------------------------------------------------- the app

pub struct HubApp {
    shared: SharedRef,
    ctx: egui::Context,
    icons: HashMap<&'static str, TextureHandle>,
    hub_icon: TextureHandle,
    installed: BTreeMap<String, Installed>,
    last_state_load: Instant,
    order: Vec<&'static str>,
    /// Measured height of each card's content (without buttons), for lining up rows.
    content_heights: HashMap<&'static str, f32>,
    fuse: bool,
}

pub fn decode_png(bytes: &[u8]) -> (Vec<u8>, u32, u32) {
    match image::load_from_memory_with_format(bytes, image::ImageFormat::Png) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            (rgba.into_raw(), w, h)
        }
        Err(_) => (vec![0; 4], 1, 1),
    }
}

fn texture(ctx: &egui::Context, name: &str, png: &[u8]) -> TextureHandle {
    let (rgba, w, h) = decode_png(png);
    let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
    ctx.load_texture(name, img, TextureOptions::LINEAR)
}

/// Light or dark, from `CRAFTHUB_THEME` or the desktop's settings. winit can't see the
/// system theme on X11 (which Cinnamon uses), so read Mint/GNOME's settings directly.
fn desktop_theme() -> Option<Theme> {
    match std::env::var("CRAFTHUB_THEME").ok().as_deref() {
        Some("light") => return Some(Theme::Light),
        Some("dark") => return Some(Theme::Dark),
        _ => {}
    }
    let get = |schema: &str, key: &str| -> Option<String> {
        let out = std::process::Command::new("gsettings")
            .args(["get", schema, key])
            .stderr(std::process::Stdio::null())
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
            Some("prefer-dark") => return Some(Theme::Dark),
            Some("prefer-light") => return Some(Theme::Light),
            _ => {}
        }
    }
    // Otherwise go by the GTK theme's name (e.g. Mint-Y-Dark-Aqua vs Mint-Y-Aqua).
    for schema in [
        "org.cinnamon.desktop.interface",
        "org.gnome.desktop.interface",
    ] {
        if let Some(name) = get(schema, "gtk-theme").filter(|n| !n.is_empty()) {
            return Some(if name.contains("dark") {
                Theme::Dark
            } else {
                Theme::Light
            });
        }
    }
    None
}

fn setup_style(ctx: &egui::Context) {
    for theme in [Theme::Light, Theme::Dark] {
        ctx.style_mut_of(theme, |s| {
            use egui::{FontId, TextStyle};
            s.text_styles
                .insert(TextStyle::Body, FontId::proportional(14.5));
            s.text_styles
                .insert(TextStyle::Button, FontId::proportional(14.0));
            s.text_styles
                .insert(TextStyle::Small, FontId::proportional(12.0));
            s.text_styles
                .insert(TextStyle::Heading, FontId::proportional(20.0));
            s.spacing.item_spacing = Vec2::new(8.0, 6.0);
            s.spacing.button_padding = Vec2::new(12.0, 5.0);
            s.spacing.interact_size.y = 30.0;
            let dark = theme == Theme::Dark;
            s.visuals.panel_fill = if dark {
                Color32::from_rgb(0x1b, 0x1c, 0x21)
            } else {
                Color32::from_rgb(0xf0, 0xf1, 0xf4)
            };
            s.visuals.hyperlink_color = palette(dark).accent;
            for w in [
                &mut s.visuals.widgets.inactive,
                &mut s.visuals.widgets.hovered,
                &mut s.visuals.widgets.active,
                &mut s.visuals.widgets.noninteractive,
            ] {
                w.corner_radius = CornerRadius::same(6);
            }
        });
    }
}

impl HubApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();
        setup_style(&ctx);
        if let Some(theme) = desktop_theme() {
            ctx.set_theme(theme);
        }
        let icons = APPS
            .iter()
            .map(|a| (a.id, texture(&ctx, a.id, &install::icon_bytes(a))))
            .collect();
        let hub_icon = texture(&ctx, "crafthub", hub::ICON_PNG);
        let cards = APPS
            .iter()
            .map(|a| {
                (
                    a.id,
                    CardState {
                        check: Check::Pending,
                        busy: None,
                        note: None,
                        build: None,
                    },
                )
            })
            .collect();
        let shared = Arc::new(Mutex::new(Shared {
            cards,
            pending_checks: 0,
            hub_release: None,
            hub_busy: None,
            hub_note: None,
            restart_to: None,
            state_dirty: true,
            new_icons: Vec::new(),
        }));
        let mut app = Self {
            shared,
            ctx,
            icons,
            hub_icon,
            installed: BTreeMap::new(),
            last_state_load: Instant::now(),
            order: APPS.iter().map(|a| a.id).collect(),
            content_heights: HashMap::new(),
            fuse: install::have_fuse2(),
        };
        app.reload_state();
        app.check_all();
        let (shared, ctx) = (app.shared.clone(), app.ctx.clone());
        thread::spawn(move || {
            let changed = install::refresh_remote_icons();
            if !changed.is_empty() {
                lock(&shared).new_icons.extend(changed.iter().map(|a| a.id));
                ctx.request_repaint();
            }
        });
        app
    }

    fn reload_state(&mut self) {
        self.installed = state::load()
            .into_iter()
            .filter(|(_, i)| i.exists())
            .collect();
        self.last_state_load = Instant::now();
    }

    /// Check every app (and Craft Hub itself) for its latest release.
    fn check_all(&self) {
        {
            let mut s = lock(&self.shared);
            if s.pending_checks > 0 {
                return;
            }
            s.pending_checks = APPS.len();
            for c in s.cards.values_mut() {
                c.check = Check::Pending;
            }
        }
        for app in APPS {
            let (shared, ctx) = (self.shared.clone(), self.ctx.clone());
            thread::spawn(move || {
                let check = match install::latest_for(app) {
                    Ok(r) => Check::Done(r),
                    Err(e) => Check::Failed(e),
                };
                let mut s = lock(&shared);
                if let Some(c) = s.cards.get_mut(app.id) {
                    c.check = check;
                }
                s.pending_checks = s.pending_checks.saturating_sub(1);
                drop(s);
                ctx.request_repaint();
            });
        }
        let (shared, ctx) = (self.shared.clone(), self.ctx.clone());
        thread::spawn(move || {
            let rel = hub::latest().ok().flatten();
            lock(&shared).hub_release = rel;
            ctx.request_repaint();
        });
    }

    fn view(&self, s: &Shared, app: &'static App) -> CardView {
        let c = &s.cards[app.id];
        let inst = self.installed.get(app.id).cloned();
        let source = inst.as_ref().is_some_and(|i| i.source);
        let rel = match &c.check {
            Check::Done(Some(r)) => Some(r),
            _ => None,
        };
        let has_appimage = rel.is_some_and(|r| r.appimage.is_some());
        let has_update = match (&inst, rel) {
            (Some(i), Some(r)) if !i.source => has_appimage && r.tag() != i.version,
            _ => false,
        };
        let can_update = inst.is_some() && (source || has_update);
        let can_install = inst.is_none() && (has_appimage || app.source_build);

        let mut parts = Vec::new();
        if let Some(i) = &inst {
            parts.push(if i.source {
                format!("Installed: source build {}", i.version)
            } else {
                format!("Installed: v{}", pretty_version(&i.version))
            });
        }
        parts.push(match &c.check {
            Check::Pending => "Checking for updates…".to_string(),
            Check::Failed(e) => {
                let short: String = e.chars().take(90).collect();
                format!("Couldn't check for updates ({short})")
            }
            Check::Done(None) => "No releases yet".to_string(),
            Check::Done(Some(r)) if r.appimage.is_some() => {
                if inst
                    .as_ref()
                    .is_some_and(|i| !i.source && i.version == r.tag())
                {
                    "Up to date".to_string()
                } else {
                    format!("Latest: v{}", pretty_version(r.tag()))
                }
            }
            Check::Done(Some(_)) if app.source_build && source => {
                "Rebuild pulls the latest source and recompiles".to_string()
            }
            Check::Done(Some(_)) if app.source_build => {
                "No Linux release yet, so Install builds it from \
                source (opens a terminal, takes 20–60 min the first time)"
                    .to_string()
            }
            Check::Done(Some(_)) => format!(
                "Latest release has no Linux {} AppImage",
                github::arch_name()
            ),
        });
        CardView {
            status: parts.join("  ·  "),
            note: c.note.clone(),
            busy: c.busy.clone(),
            installed: inst,
            can_install,
            can_update,
            has_update,
            source,
        }
    }

    /// Updates first, then installed apps, then the rest; catalog order within each group.
    fn sort_order(&self, views: &HashMap<&'static str, CardView>) -> Vec<&'static str> {
        let mut ids: Vec<(usize, &'static str)> = APPS.iter().map(|a| a.id).enumerate().collect();
        ids.sort_by_key(|(i, id)| {
            let v = &views[id];
            let group = if v.has_update {
                0
            } else if v.installed.is_some() {
                1
            } else {
                2
            };
            (group, *i)
        });
        ids.into_iter().map(|(_, id)| id).collect()
    }

    // ------------------------------------------------------------------------- background work

    fn poll(&mut self) {
        let mut s = lock(&self.shared);
        if let Some(path) = s.restart_to.take() {
            drop(s);
            let err = hub::restart(&path);
            let mut s = lock(&self.shared);
            s.hub_note = Some((
                format!("Updated, but couldn't restart: {err}. Please reopen Craft Hub."),
                true,
            ));
            return;
        }
        let mut building = false;
        let mut finished_build = false;
        let st = if s.cards.values().any(|c| c.build.is_some()) {
            Some(state::load())
        } else {
            None
        };
        for (id, c) in s.cards.iter_mut() {
            let Some(b) = c.build.as_mut() else { continue };
            building = true;
            let done = st
                .as_ref()
                .and_then(|st| st.get(*id))
                .is_some_and(|i| i.installed_at + 1 >= b.started);
            let exited = matches!(b.child.try_wait(), Ok(Some(_)));
            let (note, end) = if done {
                (Some(("Built and installed.".to_string(), false)), true)
            } else if exited && b.started_at.elapsed() > Duration::from_secs(5) {
                (
                    Some(("Build window closed before finishing.".to_string(), true)),
                    true,
                )
            } else if b.started_at.elapsed() > Duration::from_secs(6 * 3600) {
                (
                    Some(("Timed out waiting for the build.".to_string(), true)),
                    true,
                )
            } else {
                (None, false)
            };
            if end {
                c.build = None;
                c.busy = None;
                c.note = note;
                finished_build = true;
            }
        }
        let dirty = std::mem::take(&mut s.state_dirty) || finished_build;
        let new_icons = std::mem::take(&mut s.new_icons);
        drop(s);
        for id in new_icons {
            if let Some(app) = apps::by_id(id) {
                self.icons
                    .insert(id, texture(&self.ctx, id, &install::icon_bytes(app)));
            }
        }
        if dirty || self.last_state_load.elapsed() > Duration::from_secs(3) {
            self.reload_state();
        }
        if building {
            self.ctx.request_repaint_after(Duration::from_millis(500));
        } else {
            self.ctx.request_repaint_after(Duration::from_secs(3));
        }
    }

    fn set_busy(&self, id: &'static str, label: String) {
        let mut s = lock(&self.shared);
        let c = s.cards.get_mut(id).expect("known app");
        c.busy = Some(Busy {
            label,
            frac: None,
            bytes: None,
        });
        c.note = None;
    }

    fn finish(
        shared: &SharedRef,
        ctx: &egui::Context,
        id: &'static str,
        result: Result<String, String>,
    ) {
        let mut s = lock(shared);
        if let Some(c) = s.cards.get_mut(id) {
            c.busy = None;
            c.note = Some(match result {
                Ok(msg) => (msg, false),
                Err(e) => (format!("Error: {e}"), true),
            });
        }
        s.state_dirty = true;
        drop(s);
        ctx.request_repaint();
    }

    fn start_download(&self, app: &'static App, verb: &'static str) {
        let rel = match &lock(&self.shared).cards[app.id].check {
            Check::Done(Some(r)) if r.appimage.is_some() => r.clone(),
            _ => return,
        };
        self.set_busy(app.id, format!("{verb} v{}…", pretty_version(rel.tag())));
        let (shared, ctx) = (self.shared.clone(), self.ctx.clone());
        thread::spawn(move || {
            let progress = |done: u64, total: u64| {
                if let Some(b) = lock(&shared)
                    .cards
                    .get_mut(app.id)
                    .and_then(|c| c.busy.as_mut())
                {
                    b.frac = (total > 0).then(|| done as f32 / total as f32);
                    b.bytes = Some((done, total));
                }
                ctx.request_repaint();
            };
            let result = install::install_release(app, &rel, &progress).map(|_| {
                if verb == "Installing" {
                    "Installed."
                } else {
                    "Updated."
                }
                .to_string()
            });
            Self::finish(&shared, &ctx, app.id, result);
        });
    }

    fn start_build(&self, app: &'static App) {
        match install::start_source_build(app) {
            Ok(child) => {
                let mut s = lock(&self.shared);
                let c = s.cards.get_mut(app.id).expect("known app");
                c.busy = Some(Busy {
                    label: "Building in the terminal window… this card updates when it finishes."
                        .into(),
                    frac: None,
                    bytes: None,
                });
                c.note = None;
                c.build = Some(Build {
                    child,
                    started: state::now(),
                    started_at: Instant::now(),
                });
            }
            Err(e) => {
                lock(&self.shared)
                    .cards
                    .get_mut(app.id)
                    .expect("known app")
                    .note = Some((format!("Error: {e}"), true));
            }
        }
    }

    fn run(&mut self, action: Action, views: &HashMap<&'static str, CardView>) {
        match action {
            Action::Install(id) | Action::Update(id) => {
                let app = apps::by_id(id).expect("known app");
                let v = &views[id];
                let installing = matches!(action, Action::Install(_));
                if v.busy.is_some() {
                    return;
                }
                let rel_has_appimage = matches!(&lock(&self.shared).cards[id].check, Check::Done(Some(r)) if r.appimage.is_some());
                if app.source_build && !rel_has_appimage {
                    self.start_build(app);
                } else {
                    self.start_download(app, if installing { "Installing" } else { "Updating" });
                }
            }
            Action::Uninstall(id) => {
                let app = apps::by_id(id).expect("known app");
                self.set_busy(id, "Uninstalling…".into());
                let (shared, ctx) = (self.shared.clone(), self.ctx.clone());
                thread::spawn(move || {
                    let result = install::uninstall(app).map(|_| "Uninstalled.".to_string());
                    Self::finish(&shared, &ctx, id, result);
                });
            }
            Action::Open(id) => {
                if let Err(e) = install::launch(apps::by_id(id).expect("known app")) {
                    lock(&self.shared)
                        .cards
                        .get_mut(id)
                        .expect("known app")
                        .note = Some((format!("Couldn't open: {e}"), true));
                }
            }
            Action::Url(u) => install::open_url(&u),
            Action::Refresh => self.check_all(),
            Action::UpdateAll => {
                for (id, v) in views {
                    if v.has_update && v.busy.is_none() && !v.source {
                        self.start_download(apps::by_id(id).expect("known app"), "Updating");
                    }
                }
            }
            Action::UpdateHub => self.update_hub(),
        }
    }

    fn update_hub(&self) {
        let rel = {
            let mut s = lock(&self.shared);
            match &s.hub_release {
                Some(r) if r.is_newer() && s.hub_busy.is_none() => {
                    let r = r.clone();
                    s.hub_busy = Some(Busy {
                        label: format!("Updating Craft Hub to v{}…", r.version()),
                        frac: None,
                        bytes: None,
                    });
                    s.hub_note = None;
                    r
                }
                _ => return,
            }
        };
        let (shared, ctx) = (self.shared.clone(), self.ctx.clone());
        thread::spawn(move || {
            let progress = |done: u64, total: u64| {
                if let Some(b) = lock(&shared).hub_busy.as_mut() {
                    b.frac = (total > 0).then(|| done as f32 / total as f32);
                }
                ctx.request_repaint();
            };
            let result = hub::self_update(&rel, &progress);
            let mut s = lock(&shared);
            s.hub_busy = None;
            match result {
                Ok(path) => {
                    s.hub_note = Some(("Updated. Restarting…".into(), false));
                    s.restart_to = Some(path);
                }
                Err(e) => s.hub_note = Some((format!("Craft Hub update failed: {e}"), true)),
            }
            drop(s);
            ctx.request_repaint();
        });
    }

    // ------------------------------------------------------------------------- drawing

    fn header(
        &self,
        ui: &mut egui::Ui,
        pal: &Palette,
        checking: bool,
        n_updates: usize,
        actions: &mut Vec<Action>,
    ) {
        ui.horizontal(|ui| {
            ui.add(
                egui::Image::new(SizedTexture::from_handle(&self.hub_icon))
                    .fit_to_exact_size(Vec2::splat(36.0)),
            );
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("Craft Hub").size(19.0).strong());
                ui.label(
                    RichText::new("The Crafting Apps, open-source creative suite")
                        .size(12.0)
                        .color(pal.muted),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if n_updates > 0 {
                    format!("Update all ({n_updates})")
                } else {
                    "Update all".into()
                };
                if button(
                    ui,
                    pal,
                    &label,
                    n_updates > 0,
                    if n_updates > 0 {
                        Kind::Primary
                    } else {
                        Kind::Normal
                    },
                ) {
                    actions.push(Action::UpdateAll);
                }
                if checking {
                    ui.add(egui::Spinner::new());
                    ui.label(RichText::new("Checking…").color(pal.muted));
                } else if button(ui, pal, "🔄 Check for updates", true, Kind::Normal) {
                    actions.push(Action::Refresh);
                }
            });
        });
    }

    fn footer(&self, ui: &mut egui::Ui, pal: &Palette, s: &Shared, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.set_min_height(28.0);
            if let Some(b) = &s.hub_busy {
                ui.add(egui::Spinner::new());
                ui.label(RichText::new(&b.label).size(12.5));
                if let Some(f) = b.frac {
                    ui.add(
                        egui::ProgressBar::new(f)
                            .desired_width(160.0)
                            .desired_height(8.0),
                    );
                }
            } else if let Some((note, is_err)) = &s.hub_note {
                ui.label(RichText::new(note).size(12.5).color(if *is_err {
                    pal.error
                } else {
                    pal.muted
                }));
            } else if s.hub_release.as_ref().is_some_and(|r| r.is_newer()) {
                ui.label(
                    RichText::new("A new version of Craft Hub is available.")
                        .size(12.5)
                        .color(pal.muted),
                );
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Version, bottom right.
                let ver = ui
                    .link(
                        RichText::new(format!("Craft Hub v{}", hub::VERSION))
                            .size(12.0)
                            .color(pal.muted),
                    )
                    .on_hover_text("Release notes on GitHub");
                if ver.clicked() {
                    actions.push(Action::Url(hub::releases_url()));
                }
                if let Some(r) = s.hub_release.as_ref().filter(|r| r.is_newer())
                    && button(
                        ui,
                        pal,
                        &format!("Update Craft Hub to v{}", r.version()),
                        s.hub_busy.is_none(),
                        Kind::Primary,
                    )
                {
                    actions.push(Action::UpdateHub);
                }
            });
        });
    }

    fn card(
        &mut self,
        ui: &mut egui::Ui,
        pal: &Palette,
        app: &'static App,
        v: &CardView,
        pad: f32,
        actions: &mut Vec<Action>,
    ) {
        let frame = Frame::new()
            .fill(pal.card)
            .stroke(Stroke::new(1.0, pal.card_stroke))
            .corner_radius(12)
            .inner_margin(16);
        frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            let top = ui.min_rect().top();
            ui.horizontal(|ui| {
                let img = egui::Image::new(SizedTexture::from_handle(&self.icons[app.id]))
                    .fit_to_exact_size(Vec2::splat(64.0))
                    .corner_radius(14);
                ui.add(img);
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(app.name).size(20.0).strong());
                        if v.has_update {
                            badge(ui, "UPDATE", pal.warn_fg, pal.warn_bg);
                        } else if v.installed.is_some() {
                            badge(ui, "INSTALLED", pal.ok_fg, pal.ok_bg);
                        }
                    });
                    ui.label(RichText::new(app.tagline()).size(12.5).color(pal.muted));
                });
            });
            ui.add_space(4.0);
            ui.label(app.blurb);
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                for (label, url) in [
                    ("GitHub", app.repo_url()),
                    ("Wiki & Docs", app.docs_url()),
                    ("Releases", app.releases_url()),
                    ("Website", app.website_url()),
                ] {
                    if ui.link(label).on_hover_text(&url).clicked() {
                        actions.push(Action::Url(url));
                    }
                }
            });
            ui.add_space(2.0);
            if let Some((note, is_err)) = &v.note {
                ui.label(RichText::new(note).size(12.5).color(if *is_err {
                    pal.error
                } else {
                    pal.muted
                }));
            }
            match &v.busy {
                Some(b) => {
                    ui.label(RichText::new(&b.label).size(12.5).color(pal.muted));
                    let mut bar = egui::ProgressBar::new(b.frac.unwrap_or(0.0))
                        .desired_height(10.0)
                        .fill(pal.accent);
                    if let Some((done, total)) = b.bytes {
                        bar = bar.text(
                            RichText::new(format!(
                                "{:.0} / {:.0} MB",
                                done as f64 / 1e6,
                                total as f64 / 1e6
                            ))
                            .size(10.5),
                        );
                    }
                    if b.frac.is_none() {
                        bar = bar.animate(true);
                    }
                    ui.add(bar);
                }
                None => {
                    ui.label(RichText::new(&v.status).size(12.5).color(pal.muted));
                }
            }
            let content_h = ui.min_rect().bottom() - top;
            self.content_heights.insert(app.id, content_h);
            if pad > 0.5 {
                ui.add_space(pad);
            }
            ui.add_space(4.0);
            let busy = v.busy.is_some();
            ui.horizontal(|ui| {
                let inst = v.installed.is_some();
                if button(
                    ui,
                    pal,
                    "Install",
                    !busy && v.can_install,
                    if v.can_install {
                        Kind::Primary
                    } else {
                        Kind::Normal
                    },
                ) {
                    actions.push(Action::Install(app.id));
                }
                let upd = if v.source { "Rebuild" } else { "Update" };
                if button(
                    ui,
                    pal,
                    upd,
                    !busy && v.can_update,
                    if v.has_update {
                        Kind::Primary
                    } else {
                        Kind::Normal
                    },
                ) {
                    actions.push(Action::Update(app.id));
                }
                if button(ui, pal, "Uninstall", !busy && inst, Kind::Danger) {
                    actions.push(Action::Uninstall(app.id));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if button(ui, pal, "Open", !busy && inst, Kind::Normal) {
                        actions.push(Action::Open(app.id));
                    }
                });
            });
        });
    }
}

fn button(ui: &mut egui::Ui, pal: &Palette, label: &str, enabled: bool, kind: Kind) -> bool {
    let fill = match kind {
        Kind::Primary => Some(pal.accent),
        Kind::Danger => Some(pal.danger),
        Kind::Normal => None,
    };
    let text = match (fill, enabled) {
        (Some(_), true) => RichText::new(label).color(Color32::WHITE),
        _ => RichText::new(label),
    };
    let mut b = egui::Button::new(text)
        .min_size(Vec2::new(0.0, 30.0))
        .corner_radius(6);
    if let (Some(f), true) = (fill, enabled) {
        b = b.fill(f);
    }
    ui.add_enabled(enabled, b).clicked()
}

fn badge(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
    Frame::new()
        .fill(bg)
        .corner_radius(8)
        .inner_margin(Margin::symmetric(7, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.0).strong().color(fg));
        });
}

impl eframe::App for HubApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll();
        let pal = palette(ui.visuals().dark_mode);
        let mut actions = Vec::new();

        // Snapshot shared state for this frame.
        let (views, checking, n_updates) = {
            let s = lock(&self.shared);
            let views: HashMap<&'static str, CardView> =
                APPS.iter().map(|a| (a.id, self.view(&s, a))).collect();
            let n = views.values().filter(|v| v.has_update).count();
            (views, s.pending_checks > 0, n)
        };
        if !checking {
            self.order = self.sort_order(&views);
        }

        egui::Panel::top("header")
            .frame(
                Frame::new()
                    .fill(pal.bar)
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| {
                self.header(ui, &pal, checking, n_updates, &mut actions)
            });

        if !self.fuse {
            egui::Panel::top("fuse")
                .frame(Frame::new().fill(pal.banner).inner_margin(Margin::symmetric(16, 8)))
                .show_separator_line(false)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(
                            "FUSE 2 isn't installed, so apps start a little slower. Fix: sudo apt install libfuse2t64",
                        )
                        .size(12.5),
                    );
                });
        }

        {
            let s = lock(&self.shared);
            let shared_view = Shared {
                cards: HashMap::new(),
                pending_checks: s.pending_checks,
                hub_release: s.hub_release.clone(),
                hub_busy: s.hub_busy.clone(),
                hub_note: s.hub_note.clone(),
                restart_to: None,
                state_dirty: false,
                new_icons: Vec::new(),
            };
            drop(s);
            egui::Panel::bottom("footer")
                .frame(
                    Frame::new()
                        .fill(pal.bar)
                        .inner_margin(Margin::symmetric(16, 6)),
                )
                .show(ui, |ui| self.footer(ui, &pal, &shared_view, &mut actions));
        }

        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(Margin::symmetric(16, 14)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let cols = if ui.available_width() > 820.0 { 2 } else { 1 };
                        let order = self.order.clone();
                        for row in order.chunks(cols) {
                            let tallest = row
                                .iter()
                                .map(|id| self.content_heights.get(id).copied().unwrap_or(0.0))
                                .fold(0.0, f32::max);
                            ui.columns(cols, |columns| {
                                for (col, id) in columns.iter_mut().zip(row) {
                                    let app = apps::by_id(id).expect("known app");
                                    let own =
                                        self.content_heights.get(id).copied().unwrap_or(tallest);
                                    // Plain top-down layout: columns are "justified",
                                    // which would also justify the description text.
                                    col.with_layout(Layout::top_down(Align::Min), |col| {
                                        self.card(
                                            col,
                                            &pal,
                                            app,
                                            &views[id],
                                            tallest - own,
                                            &mut actions,
                                        );
                                    });
                                }
                            });
                            ui.add_space(8.0);
                        }
                    });
            });

        for a in actions {
            self.run(a, &views);
        }
    }
}
