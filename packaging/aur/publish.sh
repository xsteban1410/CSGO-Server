#!/usr/bin/env bash
# Publishes (or updates) the packages on the AUR. Requirements, just once:
#   1. account on https://aur.archlinux.org with your public SSH key (ssh-keygen -t ed25519; paste ~/.ssh/id_ed25519.pub)
#   2. the code on GitHub and, for the stable package, the v<pkgver> tag published (git tag v1.0.0 && git push --tags)
# Usage: packaging/aur/publish.sh [csgo-legacy-server|csgo-legacy-server-git ...]   (no arguments: both)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
work=${TMPDIR:-/tmp}/csls-aur
pkgs=("$@")
[ ${#pkgs[@]} -eq 0 ] && pkgs=(csgo-legacy-server csgo-legacy-server-git)
mkdir -p "$work"
for p in "${pkgs[@]}"; do
  echo "==> $p"
  rm -rf "${work:?}/$p"
  git clone "ssh://aur@aur.archlinux.org/$p.git" "$work/$p"
  cp "$here/$p/PKGBUILD" "$work/$p/"
  cd "$work/$p"
  if [ "$p" = csgo-legacy-server ]; then
    updpkgsums                      # replaces SKIP with the tag tarball's real sha256
  fi
  makepkg --printsrcinfo > .SRCINFO
  makepkg -f --noconfirm            # checks it builds before publishing
  git add PKGBUILD .SRCINFO
  git commit -m "Update to $(grep -m1 '^pkgver=' PKGBUILD | cut -d= -f2)"
  git push origin HEAD:master
  cd "$here"
done
