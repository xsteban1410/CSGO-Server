#!/usr/bin/env bash
# Builds the Arch package from the code of a commit, downloading nothing from GitHub (CI and local use).
#   packaging/build-pkg.sh [output folder] [makepkg options]
# REV=<commit> picks another commit (HEAD by default). Dependencies are not checked: multilib is usually disabled.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
out=$(mkdir -p "${1:-$root/dist}" && cd "${1:-$root/dist}" && pwd)
[[ $# -gt 0 ]] && shift
pkgbuild=$root/packaging/aur/csgo-legacy-server/PKGBUILD
ver=$(sed -n 's/^pkgver=//p' "$pkgbuild")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# makepkg uses the tarball if it is already next to the PKGBUILD
git -C "$root" archive --prefix="CSGO-Server-$ver/" -o "$work/csgo-legacy-server-$ver.tar.gz" "${REV:-HEAD}"
cp "$pkgbuild" "$work/"
(cd "$work" && makepkg --nodeps --noconfirm "$@")
cp "$work"/*.pkg.tar.zst "$out/"
ls -1 "$out"/*.pkg.tar.zst
