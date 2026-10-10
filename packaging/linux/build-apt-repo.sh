#!/usr/bin/env bash
# Turn a folder of .deb files into a signed "flat" APT repository by adding Packages,
# Packages.gz, Release, InRelease and Release.gpg next to them. Signs with the default
# GPG key, so import the signing key first.
#   packaging/linux/build-apt-repo.sh <dir>
#
# The release's files are served from
#   https://github.com/TymonDomanko/CraftHub/releases/latest/download/
# which is the URI in crafthub.sources, so each new release updates the repository.
set -euo pipefail

cd "$1"
rm -f Packages Packages.gz Release InRelease Release.gpg
dpkg-scanpackages --multiversion . /dev/null 2>/dev/null | sed 's|^Filename: \./|Filename: |' > Packages
[ -s Packages ] || { echo "no .deb files in $1" >&2; exit 1; }
gzip -9 -k -n Packages

release="$(mktemp)"
apt-ftparchive \
  -o APT::FTPArchive::Release::Origin="Craft Hub" \
  -o APT::FTPArchive::Release::Label="Craft Hub" \
  -o APT::FTPArchive::Release::Architectures="amd64 arm64" \
  -o APT::FTPArchive::Release::Description="Craft Hub, the Crafting Apps manager" \
  release . > "$release"
mv "$release" Release

gpg --batch --yes --clearsign --output InRelease Release
gpg --batch --yes --armor --detach-sign --output Release.gpg Release
echo "Signed APT repository in $1:"
grep -E '^(Package|Version|Architecture|Filename):' Packages
