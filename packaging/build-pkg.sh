#!/usr/bin/env bash
# Crea el paquete de Arch con el código de un commit, sin descargar nada de GitHub (CI y uso local).
#   packaging/build-pkg.sh [carpeta de salida] [opciones de makepkg]
# REV=<commit> elige otro commit (por defecto HEAD). Las dependencias no se comprueban: multilib suele estar desactivado.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
out=$(mkdir -p "${1:-$root/dist}" && cd "${1:-$root/dist}" && pwd)
[[ $# -gt 0 ]] && shift
pkgbuild=$root/packaging/aur/csgo-legacy-server/PKGBUILD
ver=$(sed -n 's/^pkgver=//p' "$pkgbuild")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# makepkg usa el tarball si ya está junto al PKGBUILD
git -C "$root" archive --prefix="CSGO-Server-$ver/" -o "$work/csgo-legacy-server-$ver.tar.gz" "${REV:-HEAD}"
cp "$pkgbuild" "$work/"
(cd "$work" && makepkg --nodeps --noconfirm "$@")
cp "$work"/*.pkg.tar.zst "$out/"
ls -1 "$out"/*.pkg.tar.zst
