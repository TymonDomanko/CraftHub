# Craft Hub

A Creative-Cloud-style manager for the [ArtCraft](https://getartcraft.com/) **Crafting Apps**
on Linux. One click to **Install**, **Update**, **Uninstall** or **Open** each app, with a short
description and links to its GitHub repo, docs, releases and website.

Built for Linux Mint, and works on other Ubuntu/Debian-based distros too. Written in Rust with
[egui](https://github.com/emilk/egui), the same UI toolkit several of the Crafting Apps use.

| App | Open-source alternative to |
|---|---|
| [PhotoCraft](https://github.com/storytold/photocraft) | Adobe Photoshop |
| [VectorCraft](https://github.com/storytold/vectorcraft) | Adobe Illustrator |
| [DesignCraft](https://github.com/storytold/designcraft) | Adobe InDesign |
| [LightCraft](https://github.com/storytold/lightcraft) | Adobe Lightroom |
| [FilmCraft](https://github.com/storytold/filmcraft) | Adobe Premiere Pro |
| [EffectCraft](https://github.com/storytold/effectcraft) | Adobe After Effects |
| [PDFCraft](https://github.com/storytold/pdfcraft) | Adobe Acrobat |
| [ArtCraft](https://github.com/storytold/artcraft) | (AI image & video studio) |

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/TymonDomanko/CraftHub/main/install.sh | bash
```

Or download `crafthub-linux-x86_64` from the [latest release](https://github.com/TymonDomanko/CraftHub/releases/latest),
make it executable (`chmod +x crafthub-linux-x86_64`) and run it. Either way, Craft Hub copies
itself to `~/.local/bin/crafthub` and adds itself to your menu. No password needed.

Needs a 64-bit PC with glibc 2.35 or newer (Linux Mint 21+, Ubuntu 22.04+).

If you used the earlier Python version of Craft Hub, the new one replaces it automatically and
keeps the apps you installed.

## How it works

- When it opens, Craft Hub checks each app's latest GitHub release (the refresh button checks again).
- **Install** downloads the app's Linux AppImage for your CPU, checks it against the release's
  `SHA256SUMS.txt`, and installs it **for your user only** in `~/.local/share/crafthub/apps/`.
  Each app gets a menu entry and icon.
- **Update** turns blue when a newer release is out. **Update all** updates everything at once.
  Apps with updates are sorted to the top, then your installed apps, then the rest.
- Menu entries use each app's own ID (e.g. `ai.storyteller.photocraft`), so there's one entry per
  app and the taskbar shows the right icon.
- **Uninstall** removes the app, its menu entry and its icons, including any entry the app created
  for itself. Your settings and documents are left alone.
- Craft Hub follows your light/dark appearance setting. Set `CRAFTHUB_THEME=light` or `dark` to
  override it.

### Updating Craft Hub itself

Craft Hub checks for a new version of itself too. When one is out, an **Update Craft Hub**
button appears in the bottom-right corner next to the version number. One click downloads it,
verifies it and restarts.

### ArtCraft

ArtCraft doesn't publish a Linux build yet (only Windows and macOS), so its **Install** button
builds it from source in a terminal window. That installs the build tools with apt (it asks for
your password once), plus Rust, Node.js and the Tauri CLI, then compiles. Expect 20–60 minutes the
first time and several GB of free space during the build; the build cache is deleted afterwards.
**Rebuild** pulls the latest source. If ArtCraft ever ships a Linux AppImage, Craft Hub switches
to it automatically.

### FUSE

AppImages start fastest with FUSE 2. If Craft Hub shows a notice about it, run
`sudo apt install libfuse2t64` (Mint 22) or `sudo apt install libfuse2` (Mint 21). Apps still run
without it, just a little slower to start.

## Command line

```
crafthub                          Open the window
crafthub --install-self           Install Craft Hub for this user
crafthub --uninstall-self [--all] Remove Craft Hub (--all also removes every app it installed)
crafthub --list                   Show installed apps
crafthub --check                  Show the latest release of each app
crafthub --version
```

## Building from source

Needs Rust 1.95 or newer.

```bash
cargo build --release
./target/release/crafthub
```

## Publishing a release

1. Bump `version` in `Cargo.toml` (e.g. `2.0.1`) and commit.
2. Tag and push:
   ```bash
   git tag v2.0.1
   git push origin main v2.0.1
   ```

The [Release workflow](.github/workflows/release.yml) builds the binary, writes `SHA256SUMS.txt`
and publishes the GitHub release. Everyone's Craft Hub then offers the update.

## Credits

The Crafting Apps are made by the [ArtCraft](https://getartcraft.com/) team. App icons are
credited in [`assets/icons/NOTICE.md`](assets/icons/NOTICE.md). ArtCraft's logo isn't included in
this repository: Craft Hub downloads it from the ArtCraft repo when it runs.

Craft Hub is an independent project and isn't affiliated with the ArtCraft team or Adobe Inc.
Adobe product names are used only to describe compatible workflows.

Licensed under the [MIT License](LICENSE).
