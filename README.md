# Craft Hub

A Creative-Cloud-style manager for the [ArtCraft](https://getartcraft.com/) **Crafting Apps** on
**Linux, Windows and macOS**, on Intel/AMD and ARM. One click to **Install**, **Update**,
**Uninstall** or **Open** each app, with a short description and links to its GitHub repo, docs,
releases and website.

Written in Rust with [egui](https://github.com/emilk/egui), the same UI toolkit several of the
Crafting Apps use.

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

Everything installs for your user only: no admin password needed.

**Linux and macOS**, in a terminal:

```bash
curl -fsSL https://raw.githubusercontent.com/TymonDomanko/CraftHub/main/install.sh | bash
```

**Windows**, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/TymonDomanko/CraftHub/main/install.ps1 | iex
```

Or download the file for your computer from the
[latest release](https://github.com/TymonDomanko/CraftHub/releases/latest) and open it:

| Computer | Download |
|---|---|
| Linux, Intel/AMD | `crafthub-linux-x86_64` (make it executable: `chmod +x crafthub-linux-x86_64`) |
| Linux, ARM64 | `crafthub-linux-aarch64` |
| Windows, Intel/AMD | `crafthub-windows-x86_64.exe` |
| Windows on ARM | `crafthub-windows-aarch64.exe` |
| Mac (Apple silicon and Intel) | `crafthub-macos-universal.zip` (unzip, move `Craft Hub.app` to Applications) |

The first time it opens, Craft Hub installs itself: on Linux to `~/.local/bin/crafthub` with a menu
entry; on Windows to `%LOCALAPPDATA%\Programs\CraftHub` with a Start Menu shortcut and an
"Apps & features" entry; on macOS it stays wherever you put `Craft Hub.app`.

Downloaded through a browser, the Windows and Mac versions show a warning the first time, because
Craft Hub isn't code-signed. On Windows choose **More info → Run anyway**. On a Mac, open
**System Settings → Privacy & Security** and choose **Open Anyway**. The install commands above
avoid this.

Linux builds need glibc 2.35 or newer (Linux Mint 21+, Ubuntu 22.04+). The Mac version needs
macOS 11 or newer. If you used the earlier Python version of Craft Hub, the new one replaces it
automatically and keeps the apps you installed.

## How it works

- When it opens, Craft Hub checks each app's latest GitHub release (the refresh button checks again).
- **Install** downloads the right build for your computer and checks it against the release's
  `SHA256SUMS.txt`:
  - **Linux:** the app's AppImage, in `~/.local/share/crafthub/apps/`, with a menu entry.
  - **Windows:** the app's portable build, unpacked into `%LOCALAPPDATA%\CraftHub\apps\`, with a
    Start Menu shortcut. Settings go in the usual `%APPDATA%` folder, so updates keep them.
  - **macOS:** the app from its disk image, copied to `~/Applications`.
  - **ARM:** ARM builds where they exist. Windows on ARM runs Intel-only apps (ArtCraft) under
    emulation. The Mac apps are universal.
- **Update** turns blue when a newer release is out. **Update all** updates everything at once.
  Apps with updates are sorted to the top, then your installed apps, then the rest.
- App icons are downloaded from each app's own GitHub repository and refreshed weekly; none are
  stored in this repository.
- Craft Hub follows your light/dark appearance setting. Set `CRAFTHUB_THEME=light` or `dark` to
  override it.

### Uninstalling cleanly

- **Uninstall** removes the app and everything Craft Hub created for it: its menu entry or Start
  Menu shortcut, icons, leftover downloads and update folders. On Linux it also removes menu
  entries the app created for itself.
- The app's own settings and saved data (preferences, presets, caches, recovery files) are kept
  in case you reinstall. Straight after uninstalling, the card offers **Delete them too**.
- **Uninstall Craft Hub…** (bottom right) removes Craft Hub and everything it keeps for itself.
  You can choose to also uninstall the apps it installed and delete their settings. On Windows,
  removing Craft Hub from **Apps & features** also uninstalls the apps it installed, since they
  live in its folder (their settings are kept).

### Updating Craft Hub itself

Craft Hub checks for a new version of itself too. When one is out, an **Update Craft Hub**
button appears in the bottom-right corner next to the version number. One click downloads it,
verifies it and restarts.

### ArtCraft

On Windows and macOS, ArtCraft installs from its own installer and disk image. It doesn't publish
a Linux build yet, so on Linux its **Install** button builds it from source in a terminal window.
That installs the build tools with apt (it asks for your password once), plus Rust, Node.js and the
Tauri CLI, then compiles. Expect 20–60 minutes the first time and several GB of free space during
the build; the build cache is deleted afterwards. **Rebuild** pulls the latest source. If ArtCraft
ever ships a Linux AppImage, Craft Hub switches to it automatically.

### FUSE (Linux)

AppImages start fastest with FUSE 2. If Craft Hub shows a notice about it, run
`sudo apt install libfuse2t64` (Mint 22) or `sudo apt install libfuse2` (Mint 21). Apps still run
without it, just a little slower to start.

## Command line

```
crafthub                          Open the window
crafthub --install ID             Install an app (e.g. photocraft)
crafthub --update ID|all          Update an app, or every installed app
crafthub --uninstall ID           Uninstall an app
crafthub --delete-data ID         Delete an uninstalled app's settings and saved data
crafthub --open ID                Open an installed app
crafthub --list                   Show installed apps
crafthub --check                  Show the latest release of each app
crafthub --install-self           Install Craft Hub for this user
crafthub --uninstall-self [--all] [--purge]
                                  Remove Craft Hub; --all also uninstalls every app it
                                  installed, --purge also deletes their settings
crafthub --version
```

## Building from source

Needs Rust 1.95 or newer.

```bash
cargo build --release
./target/release/crafthub
```

On macOS, `packaging/macos/bundle.sh dist` builds the universal `Craft Hub.app`.

## Publishing a release

Bump `version` in `Cargo.toml` (e.g. `2.1.1`), commit and push to `main`. That's it.

The [Release workflow](.github/workflows/release.yml) notices there's no release for that version
yet, builds Craft Hub for every platform, writes `SHA256SUMS.txt` and publishes the release
`v2.1.1`, usually within 15 minutes. Everyone's Craft Hub then offers the update. Pushes that don't
change the version don't make a release. (Pushing a `v2.1.1` tag or publishing a release on GitHub
works too.)

Every push is also tested by [CI](.github/workflows/ci.yml) on Linux, Windows and macOS (Intel/AMD
and ARM), including real installs and clean uninstalls of an app.

## Credits

The Crafting Apps are made by the [ArtCraft](https://getartcraft.com/) team. Their icons aren't
stored here: Craft Hub downloads each one from the app's own repository (see
[`assets/icons/NOTICE.md`](assets/icons/NOTICE.md)).

Craft Hub is an independent project and isn't affiliated with the ArtCraft team or Adobe Inc.
Adobe product names are used only to describe compatible workflows.

Licensed under the [MIT License](LICENSE).
