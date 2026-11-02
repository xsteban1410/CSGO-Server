#!/usr/bin/env bash
# Publica (o actualiza) los paquetes en el AUR. Requisitos, una sola vez:
#   1. cuenta en https://aur.archlinux.org con tu clave SSH pública (ssh-keygen -t ed25519; pega ~/.ssh/id_ed25519.pub)
#   2. el código subido a GitHub y, para el paquete estable, el tag v<pkgver> publicado (git tag v1.0.0 && git push --tags)
# Uso: packaging/aur/publish.sh [csgo-legacy-server|csgo-legacy-server-git ...]   (sin argumentos: los dos)
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
    updpkgsums                      # sustituye el SKIP por el sha256 real del tarball del tag
    cp PKGBUILD "$here/$p/PKGBUILD" # deja el checksum también en el repo
  fi
  makepkg --printsrcinfo > .SRCINFO
  makepkg -f --noconfirm            # comprueba que compila antes de publicar
  git add PKGBUILD .SRCINFO
  git commit -m "Update to $(grep -m1 '^pkgver=' PKGBUILD | cut -d= -f2)"
  git push origin HEAD:master
  cd "$here"
done
