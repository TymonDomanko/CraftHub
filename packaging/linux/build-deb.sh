#!/usr/bin/env bash
# Build the Craft Hub .deb (for Linux Mint, Ubuntu and Debian) from a compiled binary.
#   packaging/linux/build-deb.sh <crafthub binary> <amd64|arm64> <out-dir>
#     ->  <out-dir>/crafthub_<arch>.deb
#
# Besides the program, menu entry and icons, the package adds Craft Hub's software source
# (/etc/apt/sources.list.d/crafthub.sources and its key), so Craft Hub shows up in Software
# Manager and its updates arrive through Update Manager.
set -euo pipefail

BIN=$(realpath "$1")
ARCH="$2"
OUT=$(realpath -m "$3")
cd "$(dirname "$0")/../.."
VERSION=${DEB_VERSION:-$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)}  # DEB_VERSION: for tests
DATE=$(date -u +%Y-%m-%d)

ROOT="$(mktemp -d)"
trap 'rm -rf "$ROOT"' EXIT

install -Dm755 "$BIN" "$ROOT/usr/bin/crafthub"
install -Dm644 packaging/linux/crafthub.desktop "$ROOT/usr/share/applications/crafthub.desktop"
install -Dm644 assets/icons/crafthub.svg "$ROOT/usr/share/icons/hicolor/scalable/apps/crafthub.svg"
install -Dm644 assets/icons/crafthub.png "$ROOT/usr/share/icons/hicolor/256x256/apps/crafthub.png"
install -d "$ROOT/usr/share/metainfo"
sed -e "s/@VERSION@/$VERSION/" -e "s/@DATE@/$DATE/" packaging/linux/io.github.tymondomanko.CraftHub.metainfo.xml \
  > "$ROOT/usr/share/metainfo/io.github.tymondomanko.CraftHub.metainfo.xml"
install -Dm644 packaging/linux/copyright "$ROOT/usr/share/doc/crafthub/copyright"
# The software source and its signing key. Both are conffiles, so removing the package
# (as Software Manager does) keeps the source working for a reinstall; purging removes them.
install -Dm644 packaging/linux/crafthub.sources "$ROOT/etc/apt/sources.list.d/crafthub.sources"
install -d "$ROOT/etc/apt/keyrings"
gpg --dearmor < packaging/linux/crafthub-archive-keyring.asc > "$ROOT/etc/apt/keyrings/crafthub.gpg"
chmod 644 "$ROOT/etc/apt/keyrings/crafthub.gpg"
install -d "$ROOT/usr/share/man/man1"
gzip -9 -n -c packaging/linux/crafthub.1 > "$ROOT/usr/share/man/man1/crafthub.1.gz"
{
  echo "crafthub ($VERSION) stable; urgency=medium"
  echo
  echo "  * Craft Hub $VERSION. See the GitHub release for details."
  echo
  echo " -- Tymon Domanko <TymonDomanko@users.noreply.github.com>  $(date -u -R)"
} | gzip -9 -n > "$ROOT/usr/share/doc/crafthub/changelog.gz"

install -d "$ROOT/DEBIAN"
printf '%s\n' /etc/apt/keyrings/crafthub.gpg /etc/apt/sources.list.d/crafthub.sources > "$ROOT/DEBIAN/conffiles"
SIZE=$(du -sk "$ROOT/usr" "$ROOT/etc" | awk '{s += $1} END {print s}')
cat > "$ROOT/DEBIAN/control" <<EOF
Package: crafthub
Version: $VERSION
Architecture: $ARCH
Maintainer: Tymon Domanko <TymonDomanko@users.noreply.github.com>
Installed-Size: $SIZE
Depends: libc6 (>= 2.35), libgcc-s1, libgl1, libegl1, libx11-6, libxcursor1, libxrandr2, libxi6, libxkbcommon-x11-0, libwayland-client0, xdg-utils
Recommends: libfuse2t64 | libfuse2, gnome-terminal | x-terminal-emulator
Section: graphics
Priority: optional
Homepage: https://github.com/TymonDomanko/CraftHub
Description: Craft Hub: install, update and uninstall the Crafting Apps
 A Creative-Cloud-style manager for the ArtCraft Crafting Apps: PhotoCraft,
 VectorCraft, DesignCraft, LightCraft, FilmCraft, EffectCraft, PDFCraft and
 ArtCraft. One click to install, update, uninstall or open each app.
 .
 Apps are installed for your user only. Installing this package also adds
 Craft Hub's software source, so updates arrive through Update Manager.
EOF

mkdir -p "$OUT"
dpkg-deb --root-owner-group -Zxz --build "$ROOT" "$OUT/crafthub_${ARCH}.deb"
dpkg-deb --info "$OUT/crafthub_${ARCH}.deb" | sed -n '/Package:/,/Description:/p'
