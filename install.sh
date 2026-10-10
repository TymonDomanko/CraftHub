#!/usr/bin/env bash
# Install Craft Hub for the current user on Linux or macOS (no sudo needed):
#   curl -fsSL https://raw.githubusercontent.com/TymonDomanko/CraftHub/main/install.sh | bash
# On Windows, use install.ps1 instead.
set -euo pipefail

REPO="TymonDomanko/CraftHub"
BASE="https://github.com/$REPO/releases/latest/download"

case "$(uname -s)" in
  Linux)
    case "$(uname -m)" in
      x86_64 | amd64) ASSET=crafthub-linux-x86_64 ;;
      aarch64 | arm64) ASSET=crafthub-linux-aarch64 ;;
      *) echo "Sorry, there's no Craft Hub build for $(uname -m) yet." >&2; exit 1 ;;
    esac
    ;;
  Darwin) ASSET=crafthub-macos-universal.zip ;;
  *) echo "Sorry, this installer is for Linux and macOS. On Windows, use install.ps1." >&2; exit 1 ;;
esac

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Downloading Craft Hub..."
curl -fL --progress-bar "$BASE/$ASSET" -o "$TMP/$ASSET"
curl -fsSL "$BASE/SHA256SUMS.txt" -o "$TMP/SHA256SUMS.txt"
if command -v sha256sum >/dev/null; then sum="sha256sum"; else sum="shasum -a 256"; fi
(cd "$TMP" && grep " $ASSET\$" SHA256SUMS.txt | $sum -c --status) \
  || { echo "Checksum check failed; not installing." >&2; exit 1; }

if [ "$(uname -s)" = Darwin ]; then
  mkdir -p "$HOME/Applications"
  rm -rf "$TMP/unzipped" && mkdir "$TMP/unzipped"
  ditto -x -k "$TMP/$ASSET" "$TMP/unzipped"
  rm -rf "$HOME/Applications/Craft Hub.app"
  ditto "$TMP/unzipped/Craft Hub.app" "$HOME/Applications/Craft Hub.app"
  xattr -dr com.apple.quarantine "$HOME/Applications/Craft Hub.app" 2>/dev/null || true
  echo "Craft Hub is installed in ~/Applications. Opening it..."
  open "$HOME/Applications/Craft Hub.app"
else
  chmod +x "$TMP/$ASSET"
  "$TMP/$ASSET" --install-self
fi
