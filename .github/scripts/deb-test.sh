#!/usr/bin/env bash
# Test Craft Hub's .deb package and APT repository on Ubuntu: install the package and check
# its files, check it takes over from a per-user copy, upgrade it from a signed repository
# (served locally, signed with a throwaway key), check apt rejects a wrongly signed one,
# then remove (keeps the software source) and purge (removes it).
#   deb-test.sh <crafthub binary>
set -euo pipefail

BIN=$(realpath "$1")
cd "$(dirname "$0")/../.."
ARCH=$(dpkg --print-architecture)
SUDO=; [ "$(id -u)" = 0 ] || SUDO=sudo
work=$(mktemp -d)
chmod 755 "$work"  # apt reads the test key as its own (unprivileged) user
server=
cleanup() { [ -z "$server" ] || kill "$server" 2>/dev/null || true; }
trap cleanup EXIT

say() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
die() { printf '\033[31mFAIL: %s\033[0m\n' "$*"; exit 1; }

say "build the package"
bash packaging/linux/build-deb.sh "$BIN" "$ARCH" "$work/v1"
deb="$work/v1/crafthub_$ARCH.deb"
version=$(dpkg-deb -f "$deb" Version)

say "install it"
dpkg-deb -c "$deb" | grep -q '/usr/share/man/man1/crafthub.1.gz$' || die "no man page in the package"
$SUDO apt-get install -y -q --no-install-recommends "$deb"
/usr/bin/crafthub --version | grep -qF "$version" || die "/usr/bin/crafthub isn't version $version"
for f in /usr/share/applications/crafthub.desktop \
         /usr/share/icons/hicolor/scalable/apps/crafthub.svg \
         /usr/share/icons/hicolor/256x256/apps/crafthub.png \
         /usr/share/metainfo/io.github.tymondomanko.CraftHub.metainfo.xml \
         /usr/share/doc/crafthub/copyright \
         /etc/apt/sources.list.d/crafthub.sources \
         /etc/apt/keyrings/crafthub.gpg; do
  [ -f "$f" ] || die "$f is missing"
done
if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate /usr/share/applications/crafthub.desktop
fi
# (AppStream before 1.0, as on Ubuntu 22.04, doesn't know some of the tags used.)
if command -v appstreamcli >/dev/null && appstreamcli --version | grep -qE 'version: *[1-9]'; then
  appstreamcli validate --no-net /usr/share/metainfo/io.github.tymondomanko.CraftHub.metainfo.xml
fi

say "the packaged Craft Hub takes over from a per-user copy"
export HOME="$work/home" XDG_DATA_HOME="$work/home/.local/share"
mkdir -p "$HOME"
"$BIN" --install-self
[ -f "$HOME/.local/bin/crafthub" ] || die "per-user copy wasn't installed"
[ -f "$XDG_DATA_HOME/applications/crafthub.desktop" ] || die "per-user menu entry wasn't added"
/usr/bin/crafthub --install-self
[ ! -e "$HOME/.local/bin/crafthub" ] || die "per-user copy is still there"
[ ! -e "$XDG_DATA_HOME/applications/crafthub.desktop" ] || die "per-user menu entry is still there"

say "upgrade from a signed repository"
export GNUPGHOME="$work/gnupg"
mkdir -m 700 "$GNUPGHOME"
gpg --batch --passphrase '' --quick-gen-key "Craft Hub test <test@example.invalid>" ed25519 sign never
gpg --export > "$work/test.gpg" && chmod 644 "$work/test.gpg"
newer="$version+test1"
DEB_VERSION="$newer" bash packaging/linux/build-deb.sh "$BIN" "$ARCH" "$work/repo"
bash packaging/linux/build-apt-repo.sh "$work/repo"
python3 -m http.server --bind 127.0.0.1 --directory "$work/repo" 18765 >/dev/null 2>&1 &
server=$!
sleep 1

# Use only the test repository, so the real Craft Hub source (and every other) is left alone.
apt_test() {
  local key="$1"; shift
  mkdir -p "$work/parts"
  printf 'Types: deb\nURIs: http://127.0.0.1:18765/\nSuites: ./\nSigned-By: %s\n' "$key" \
    > "$work/parts/test.sources"
  $SUDO apt-get -q -o Dir::Etc::sourcelist=/dev/null -o Dir::Etc::sourceparts="$work/parts" \
    -o APT::Get::List-Cleanup=0 "$@"
}
apt_test "$work/test.gpg" update 2>&1 | tee "$work/update.log"
! grep -qE '^(W|E):' "$work/update.log" || die "apt update reported problems"
apt-cache -o Dir::Etc::sourcelist=/dev/null -o Dir::Etc::sourceparts="$work/parts" policy crafthub \
  | tee "$work/policy"
grep -qF "Candidate: $newer" "$work/policy" || die "apt doesn't offer $newer"
apt_test "$work/test.gpg" install -y crafthub
[ "$(dpkg-query -W -f '${Version}' crafthub)" = "$newer" ] || die "crafthub wasn't upgraded"
[ -x /usr/bin/crafthub ] || die "/usr/bin/crafthub is missing after the upgrade"

say "apt rejects a repository signed with another key"
if apt_test /etc/apt/keyrings/crafthub.gpg update 2>&1 | tee "$work/bad.log" \
   && ! grep -qE "NO_PUBKEY|not signed|signatures (couldn't|were invalid)|is not signed" "$work/bad.log"; then
  die "apt accepted a repository signed with the wrong key"
fi

say "remove keeps the software source, purge removes it"
$SUDO apt-get remove -y -q crafthub
[ ! -e /usr/bin/crafthub ] || die "/usr/bin/crafthub is still there after remove"
[ -f /etc/apt/sources.list.d/crafthub.sources ] || die "remove deleted the software source"
[ -f /etc/apt/keyrings/crafthub.gpg ] || die "remove deleted the key"
$SUDO apt-get purge -y -q crafthub
[ ! -e /etc/apt/sources.list.d/crafthub.sources ] || die "purge left the software source"
[ ! -e /etc/apt/keyrings/crafthub.gpg ] || die "purge left the key"
[ ! -e /usr/share/applications/crafthub.desktop ] || die "purge left the menu entry"

say "all good"
