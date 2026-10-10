#!/usr/bin/env bash
# Build "Craft Hub.app" (a universal binary for Apple silicon and Intel) and zip it.
#   packaging/macos/bundle.sh <out-dir>   ->  <out-dir>/crafthub-macos-universal.zip
set -euo pipefail

OUT="${1:-dist}"
cd "$(dirname "$0")/../.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target x86_64-apple-darwin

APP="$OUT/Craft Hub.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/crafthub" \
  target/aarch64-apple-darwin/release/crafthub \
  target/x86_64-apple-darwin/release/crafthub
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
cp assets/icons/crafthub.icns "$APP/Contents/Resources/crafthub.icns"
# Ad-hoc signature: Apple silicon only runs signed code. (Not notarized.)
codesign --force --deep --sign - "$APP"
codesign --verify --verbose "$APP"
lipo -info "$APP/Contents/MacOS/crafthub"
"$APP/Contents/MacOS/crafthub" --version

(cd "$OUT" && rm -f crafthub-macos-universal.zip && ditto -c -k --keepParent "Craft Hub.app" crafthub-macos-universal.zip)
ls -la "$OUT/crafthub-macos-universal.zip"
