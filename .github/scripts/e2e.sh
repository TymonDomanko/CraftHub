#!/usr/bin/env bash
# End-to-end test against the real GitHub releases: install an app, check it's in place
# (with its menu entry / Start Menu shortcut / ~/Applications bundle), check for updates,
# uninstall it, and install/uninstall Craft Hub itself.
#   e2e.sh <crafthub binary> <app id> <app name>
set -euo pipefail

BIN="$1"
APP="$2"
NAME="$3"

say() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
die() { printf '\033[31mFAIL: %s\033[0m\n' "$*"; exit 1; }
native() { if [ "$RUNNER_OS" = "Windows" ]; then cygpath -u "$1"; else printf '%s' "$1"; fi; }

say "version and release check"
"$BIN" --version
"$BIN" --check

say "install $APP"
out=$("$BIN" --install "$APP")
echo "$out"
path=$(native "${out##*is installed: }")
[ -e "$path" ] || die "$path doesn't exist"
case "$RUNNER_OS" in
  Linux)
    test -x "$path" || die "AppImage isn't executable"
    test -f "$HOME/.local/share/applications/ai.storyteller.$APP.desktop" || die "no menu entry"
    ;;
  Windows)
    test -f "$path" || die "no .exe"
    test ! -f "$(dirname "$path")/portable.txt" || die "portable.txt should have been removed"
    test -f "$(native "$APPDATA")/Microsoft/Windows/Start Menu/Programs/$NAME.lnk" || die "no Start Menu shortcut"
    ;;
  macOS)
    case "$path" in "$HOME/Applications/"*.app) ;; *) die "not an .app in ~/Applications: $path" ;; esac
    test -f "$path/Contents/Info.plist" || die "bundle has no Info.plist"
    ;;
esac
"$BIN" --list

say "update check (should be up to date)"
"$BIN" --update "$APP" | tee /dev/stderr | grep -q "up to date" || die "expected up to date"

say "uninstall $APP"
"$BIN" --uninstall "$APP"
[ ! -e "$path" ] || die "$path is still there"
case "$RUNNER_OS" in
  Linux) test ! -f "$HOME/.local/share/applications/ai.storyteller.$APP.desktop" || die "menu entry left behind" ;;
  Windows)
    test ! -f "$(native "$APPDATA")/Microsoft/Windows/Start Menu/Programs/$NAME.lnk" || die "shortcut left behind"
    test ! -f "$(native "$USERPROFILE")/Desktop/$NAME.lnk" || die "Desktop shortcut left behind"
    ;;
esac

say "delete $APP's own settings (clean uninstall)"
case "$RUNNER_OS" in
  Linux) data="$HOME/.config/$APP" ;;
  Windows) data="$(native "$APPDATA")/$NAME" ;;
  macOS) data="$HOME/Library/Application Support/$NAME" ;;
esac
mkdir -p "$data/Presets"
echo '{}' > "$data/preferences.json"
"$BIN" --delete-data "$APP"
[ ! -e "$data" ] || die "settings left behind in $data"

say "install Craft Hub itself"
"$BIN" --install-self
case "$RUNNER_OS" in
  Linux)
    hub="$HOME/.local/bin/crafthub"
    test -f "$HOME/.local/share/applications/crafthub.desktop" || die "no Craft Hub menu entry"
    ;;
  Windows)
    hub="$(native "$LOCALAPPDATA")/Programs/CraftHub/crafthub.exe"
    test -f "$(native "$APPDATA")/Microsoft/Windows/Start Menu/Programs/Craft Hub.lnk" || die "no Craft Hub shortcut"
    reg query 'HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\CraftHub' //v DisplayVersion \
      || die "no Apps & features entry"
    ;;
  macOS)
    hub="$HOME/Applications/Craft Hub.app/Contents/MacOS/crafthub"
    ;;
esac
"$(native "$hub")" --version

say "uninstall Craft Hub itself, with the apps it installed"
"$BIN" --install "$APP" >/dev/null
"$BIN" --uninstall-self --all
test ! -e "$(native "$hub")" || die "Craft Hub is still installed"
case "$RUNNER_OS" in
  Linux) hubdir="$HOME/.local/share/crafthub" ;;
  Windows) hubdir="$(native "$LOCALAPPDATA")/CraftHub" ;;
  macOS) hubdir="$HOME/Library/Application Support/CraftHub" ;;
esac
[ ! -e "$hubdir" ] || die "Craft Hub's folder is still there: $hubdir"
[ ! -e "$path" ] || die "$APP is still installed"

say "all good"
