//! The catalog of Crafting Apps that Craft Hub manages.

/// GitHub owner of all the Crafting Apps.
pub const OWNER: &str = "storytold";

/// How a project names its release files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Naming {
    /// `photocraft-0.5.0-windows-x64-portable.zip` and friends.
    Crafting,
    /// Tauri's `ArtCraft_0.32.0_x64-setup.exe` style.
    Tauri,
}

pub struct App {
    /// Repository name and internal ID, e.g. `photocraft`.
    pub id: &'static str,
    pub name: &'static str,
    /// The Adobe app it is an alternative to, if any.
    pub like: Option<&'static str>,
    /// freedesktop menu categories.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub category: &'static str,
    pub blurb: &'static str,
    /// Path of the documentation inside the repo (the projects have no GitHub wiki).
    pub docs_path: &'static str,
    pub website: Option<&'static str>,
    /// No Linux release: on Linux, Install builds it from source.
    pub source_build: bool,
    pub naming: Naming,
    /// The app's own desktop ID / Wayland app_id.
    pub desktop_id: &'static str,
    /// The app's icon, downloaded from the project's own repo (none are redistributed here).
    pub icon_url: &'static str,
    /// Other names the app's settings folders may use (besides its id, name and desktop id).
    pub data_aliases: &'static [&'static str],
}

/// Shown until an app's own icon has been downloaded (e.g. on first run while offline).
pub const PLACEHOLDER_ICON: &[u8] = include_bytes!("../assets/icons/app-placeholder.png");

impl App {
    pub fn repo_url(&self) -> String {
        format!("https://github.com/{OWNER}/{}", self.id)
    }
    pub fn docs_url(&self) -> String {
        format!(
            "https://github.com/{OWNER}/{}/tree/main/{}",
            self.id, self.docs_path
        )
    }
    pub fn releases_url(&self) -> String {
        format!("{}/releases", self.repo_url())
    }
    pub fn website_url(&self) -> String {
        match self.website {
            Some(w) => w.to_string(),
            None => format!("https://getartcraft.com/apps/{}", self.id),
        }
    }
    /// One-line description for the menu entry.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn comment(&self) -> &'static str {
        let end = self.blurb.find(['.', ':']).unwrap_or(self.blurb.len());
        &self.blurb[..end]
    }
    pub fn tagline(&self) -> String {
        match self.like {
            Some(like) => format!("Open-source alternative to Adobe {like}"),
            None => "AI image & video studio".to_string(),
        }
    }
}

/// Each Crafting App's 256 px icon in its own repo.
macro_rules! crafting_icon {
    ($id:literal) => {
        concat!(
            "https://raw.githubusercontent.com/storytold/",
            $id,
            "/main/assets/app-icon/hicolor/256x256/apps/ai.storyteller.",
            $id,
            ".png"
        )
    };
}

pub static APPS: &[App] = &[
    App {
        id: "photocraft",
        name: "PhotoCraft",
        like: Some("Photoshop"),
        category: "Graphics;RasterGraphics;2DGraphics;Photography;",
        blurb: "Image editing with layers, masks, adjustment layers, layer styles, type, vectors \
                and brushes. Opens and saves real layered PSD files, runs on a GPU compositor and \
                works fully offline.",
        docs_path: "book/src",
        website: None,
        source_build: false,
        naming: Naming::Crafting,
        desktop_id: "ai.storyteller.photocraft",
        icon_url: crafting_icon!("photocraft"),
        data_aliases: &[],
    },
    App {
        id: "vectorcraft",
        name: "VectorCraft",
        like: Some("Illustrator"),
        category: "Graphics;VectorGraphics;2DGraphics;",
        blurb: "Vector illustration with the Pen and Direct Selection tools, Pathfinder, live \
                blends, clipping masks and live appearance effects like Outer Glow.",
        docs_path: "docs",
        website: None,
        source_build: false,
        naming: Naming::Crafting,
        desktop_id: "ai.storyteller.vectorcraft",
        icon_url: crafting_icon!("vectorcraft"),
        data_aliases: &[],
    },
    App {
        id: "designcraft",
        name: "DesignCraft",
        like: Some("InDesign"),
        category: "Graphics;Publishing;Office;",
        blurb: "Page layout and publishing: threaded multi-column text frames, parent pages, text \
                wrap and its own paragraph composer. IDML import/export and PNG export, with PDF \
                export on the roadmap.",
        docs_path: "docs",
        website: None,
        source_build: false,
        naming: Naming::Crafting,
        desktop_id: "ai.storyteller.designcraft",
        icon_url: crafting_icon!("designcraft"),
        data_aliases: &[],
    },
    App {
        id: "lightcraft",
        name: "LightCraft",
        like: Some("Lightroom"),
        category: "Graphics;Photography;RasterGraphics;",
        blurb: "Photo library and non-destructive raw development in a 32-bit float, wide-gamut \
                pipeline, with masking, presets, profiles, a colour mixer and cinematic colour \
                grading.",
        docs_path: "docs",
        website: None,
        source_build: false,
        naming: Naming::Crafting,
        desktop_id: "ai.storyteller.lightcraft",
        icon_url: crafting_icon!("lightcraft"),
        data_aliases: &[],
    },
    App {
        id: "filmcraft",
        name: "FilmCraft",
        like: Some("Premiere Pro"),
        category: "AudioVideo;Video;AudioVideoEditing;",
        blurb: "Non-linear video editor with Premiere's panels, workspaces and shortcuts: colour \
                grading, effects and keyframes, audio with loudness meters, titles and captions.",
        docs_path: "docs",
        website: None,
        source_build: false,
        naming: Naming::Crafting,
        desktop_id: "ai.storyteller.filmcraft",
        icon_url: crafting_icon!("filmcraft"),
        data_aliases: &[],
    },
    App {
        id: "effectcraft",
        name: "EffectCraft",
        like: Some("After Effects"),
        category: "AudioVideo;Video;Graphics;",
        blurb: "Motion graphics and compositing: compositions, layers, keyframes, 300+ effects, \
                layer styles, expressions, 3D cameras and lights, a render queue and built-in \
                Lottie import/export.",
        docs_path: "docs",
        website: None,
        source_build: false,
        naming: Naming::Crafting,
        desktop_id: "ai.storyteller.effectcraft",
        icon_url: crafting_icon!("effectcraft"),
        data_aliases: &[],
    },
    App {
        id: "pdfcraft",
        name: "PDFCraft",
        like: Some("Acrobat"),
        category: "Office;Viewer;Graphics;",
        blurb: "The PDF workbench: read, search, organise, combine, split and secure PDFs, with \
                comments, forms, layers and attachments. No account and no telemetry.",
        docs_path: "docs",
        website: None,
        source_build: false,
        naming: Naming::Crafting,
        desktop_id: "ai.storyteller.pdfcraft",
        icon_url: crafting_icon!("pdfcraft"),
        data_aliases: &["PrintCraft"],
    },
    App {
        id: "artcraft",
        name: "ArtCraft",
        like: None,
        category: "Graphics;2DGraphics;3DGraphics;",
        blurb: "The IDE for artists: AI image and video creation where you compose in 2D, stage \
                scenes in 3D, and choose the models you want, for precise, repeatable results.",
        docs_path: "_docs",
        website: Some("https://getartcraft.com/"),
        source_build: true,
        naming: Naming::Tauri,
        desktop_id: "ai.artcraft.app",
        icon_url: "https://raw.githubusercontent.com/storytold/artcraft/main/crates/desktop/artcraft/icons/128x128@2x.png",
        data_aliases: &[],
    },
];

pub fn by_id(id: &str) -> Option<&'static App> {
    APPS.iter().find(|a| a.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        assert_eq!(APPS.len(), 8);
        for a in APPS {
            assert!(by_id(a.id).is_some());
            assert!(!a.comment().is_empty() && a.comment().len() < a.blurb.len());
            assert!(
                a.icon_url
                    .starts_with("https://raw.githubusercontent.com/storytold/")
            );
            assert!(a.icon_url.contains(a.id) && a.icon_url.ends_with(".png"));
        }
        assert_eq!(by_id("pdfcraft").unwrap().comment(), "The PDF workbench");
        assert_eq!(
            by_id("photocraft").unwrap().icon_url,
            "https://raw.githubusercontent.com/storytold/photocraft/main/assets/app-icon/hicolor/256x256/apps/ai.storyteller.photocraft.png"
        );
        assert!(PLACEHOLDER_ICON.starts_with(b"\x89PNG"));
    }
}
