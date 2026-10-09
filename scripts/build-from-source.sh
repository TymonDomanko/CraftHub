#!/usr/bin/env bash
# Craft Hub: build an app from source and register it with Craft Hub.
# Currently used for ArtCraft, which has no Linux release.
# Usage: build-from-source.sh artcraft [path-to-crafthub-binary]
set -uo pipefail

APP="${1:-artcraft}"
CRAFTHUB_BIN="${2:-$HOME/.local/bin/crafthub}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
SRC="$DATA_HOME/crafthub/src/$APP"
LOG="$DATA_HOME/crafthub/build-$APP.log"

bold() { printf '\n\033[1;36m==> %s\033[0m\n' "$*"; }
fail() {
  printf '\n\033[1;31mBuild failed:\033[0m %s\nFull log: %s\n' "$*" "$LOG"
  read -rp "Press Enter to close this window..." _
  exit 1
}
finish() {
  printf '\n\033[1;32m%s\033[0m\n' "$*"
  read -rp "Press Enter to close this window..." _
  exit 0
}

if [ "$APP" != "artcraft" ]; then
  fail "only ArtCraft is built from source (other apps ship Linux releases)"
fi

mkdir -p "$(dirname "$SRC")"
exec > >(tee -a "$LOG") 2>&1
echo "Craft Hub: building $APP ($(date))"
echo "This takes a while the first time (Rust + Node toolchains and a full compile)."

bold "1/5  System packages (needs your password once)"
PKGS="build-essential curl wget file git pkg-config libssl-dev libxdo-dev librsvg2-dev \
libayatana-appindicator3-dev libwebkit2gtk-4.1-dev libgtk-3-dev"
missing=""
for p in $PKGS; do
  dpkg-query -W -f='${Status}' "$p" 2>/dev/null | grep -q "install ok installed" || missing="$missing $p"
done
missing=$(echo "$missing" | xargs)
if [ -n "$missing" ]; then
  echo "Installing: $missing"
  # shellcheck disable=SC2086
  sudo apt-get update && sudo apt-get install -y $missing || fail "apt could not install: $missing"
else
  echo "All present."
fi

bold "2/5  Rust toolchain"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
if ! command -v cargo >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
    || fail "rustup install failed"
  . "$HOME/.cargo/env"
else
  command -v rustup >/dev/null && rustup update stable --no-self-update || true
fi
cargo --version

bold "3/5  Node.js (20+) and Tauri CLI"
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
node_major=$(node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0)
if [ "$node_major" -lt 20 ]; then
  if ! command -v nvm >/dev/null; then
    curl -fsSL https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash \
      || fail "nvm install failed"
    . "$NVM_DIR/nvm.sh"
  fi
  nvm install 24 || fail "Node install failed"
  nvm use 24
fi
node --version
if ! cargo tauri --version >/dev/null 2>&1; then
  cargo install tauri-cli --version "^2" --locked || fail "tauri-cli install failed"
fi
cargo tauri --version

bold "4/5  Fetching ArtCraft source"
if [ -d "$SRC/.git" ]; then
  git -C "$SRC" fetch --depth 1 origin main && git -C "$SRC" reset --hard origin/main \
    || fail "git update failed"
else
  rm -rf "$SRC"
  git clone --depth 1 https://github.com/storytold/artcraft "$SRC" || fail "git clone failed"
fi
REV=$(git -C "$SRC" rev-parse --short HEAD)
VER=$(grep -m1 '"version"' "$SRC/crates/desktop/artcraft/tauri.conf.json" | sed -E 's/.*"version" *: *"([^"]+)".*/\1/')
[ -n "$VER" ] || VER=dev
echo "Building ArtCraft $VER ($REV)"

bold "5/5  Compiling (this is the long part)"
cd "$SRC" || fail "missing source dir"
( cd frontend && npm install ) || fail "npm install failed"
export TAURI_FRONTEND_PATH="$SRC/frontend"
export TAURI_APP_PATH="$SRC/crates/desktop/artcraft"
export VITE_ENVIRONMENT_TYPE="production"
export SQLX_OFFLINE=true
# Same as the project's script/artcraft/unix_build.sh, but only the AppImage bundle.
cargo tauri build --config "$SRC/crates/desktop/artcraft/tauri.conf.json" --bundles appimage
status=$?

OUT=$(ls -t "$SRC"/target/release/bundle/appimage/*.AppImage 2>/dev/null | head -1)
if [ -z "$OUT" ] && [ -x "$SRC/target/release/artcraft" ]; then
  # AppImage bundling can fail on some systems; the plain binary still works.
  OUT="$SRC/target/release/artcraft"
fi
[ -n "$OUT" ] || fail "compile failed (exit $status)"

"$CRAFTHUB_BIN" --register artcraft "$OUT" "$VER-$REV" || fail "could not register the build"

# Free the large build cache (keeps the source for faster re-fetching).
rm -rf "$SRC/target" "$SRC/frontend/node_modules"

finish "ArtCraft $VER ($REV) is installed. It's in your menu and in Craft Hub."
