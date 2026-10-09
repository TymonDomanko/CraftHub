#!/usr/bin/env bash
# Install Craft Hub for the current user (no sudo needed):
#   curl -fsSL https://raw.githubusercontent.com/TymonDomanko/crafthub/main/install.sh | bash
set -euo pipefail

REPO="TymonDomanko/crafthub"
case "$(uname -m)" in
  x86_64 | amd64) ARCH=x86_64 ;;
  *) echo "Sorry, there's no Craft Hub build for $(uname -m) yet." >&2; exit 1 ;;
esac

BASE="https://github.com/$REPO/releases/latest/download"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Downloading Craft Hub..."
curl -fL --progress-bar "$BASE/crafthub-linux-$ARCH" -o "$TMP/crafthub-linux-$ARCH"
curl -fsSL "$BASE/SHA256SUMS.txt" -o "$TMP/SHA256SUMS.txt"
(cd "$TMP" && grep " crafthub-linux-$ARCH\$" SHA256SUMS.txt | sha256sum --check --status) \
  || { echo "Checksum check failed; not installing." >&2; exit 1; }

chmod +x "$TMP/crafthub-linux-$ARCH"
"$TMP/crafthub-linux-$ARCH" --install-self
