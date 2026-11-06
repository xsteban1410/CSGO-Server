#!/usr/bin/env bash
# Versión del proyecto, repartida entre Cargo.toml, Cargo.lock, los PKGBUILD, el metainfo y el changelog.
#   tools/version.sh bump X.Y.Z    la cambia en todos y convierte "Unreleased" del changelog en esa versión
#   tools/version.sh check [vX.Y.Z]  falla si no coinciden entre sí o con el tag
#   tools/version.sh notes X.Y.Z   imprime la sección de esa versión del changelog
set -euo pipefail
cd "$(dirname "$0")/.."

meta=(packaging/*.metainfo.xml)
stable=packaging/aur/csgo-legacy-server/PKGBUILD
git_pkg=packaging/aur/csgo-legacy-server-git/PKGBUILD

current() { sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1; }

notes() {
    awk -v v="$1" '
        $0 ~ "^## \\[" v "\\]" { on = 1; next }
        on && /^## \[/ { exit }
        on && /^\[[^]]+\]: / { exit }
        on { print }
    ' CHANGELOG.md | sed -e '/./,$!d' -e :a -e '/^\n*$/{$d;N;ba' -e '}'
}

case "${1:-}" in
bump)
    new=${2:?uso: version.sh bump X.Y.Z}
    [[ $new =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "versión inválida: $new" >&2; exit 1; }
    old=$(current)
    [[ $new != "$old" ]] || { echo "ya está en $new" >&2; exit 1; }
    [[ -n $(notes Unreleased) ]] || { echo "CHANGELOG.md: escribe primero los cambios en ## [Unreleased]" >&2; exit 1; }
    repo=https://github.com/xsteban1410/CSGO-Server
    sed -i -e "s/^## \[Unreleased\]\$/&\n\n## [$new] - $(date +%F)/" \
        -e "s|^\[Unreleased\]: .*|[Unreleased]: $repo/compare/v$new...HEAD\n[$new]: $repo/compare/v$old...v$new|" CHANGELOG.md
    sed -i "0,/^version = \".*\"/s//version = \"$new\"/" Cargo.toml
    cargo update --workspace --offline --quiet
    sed -i -e "s/^pkgver=.*/pkgver=$new/" -e 's/^pkgrel=.*/pkgrel=1/' "$stable"
    sed -i "s/^pkgver=.*/pkgver=$new.r0.g0000000/" "$git_pkg"
    if ! grep -q "<release version=\"$new\"" "${meta[0]}"; then
        sed -i "s|^  <releases>|  <releases>\n    <release version=\"$new\" date=\"$(date +%F)\"/>|" "${meta[0]}"
    fi
    echo "Versión $new lista. Haz commit y push: GitHub crea el tag y la release."
    ;;
check)
    v=$(current)
    fail=0
    bad() { echo "ERROR: $1" >&2; fail=1; }
    [[ -z ${2:-} || ${2#v} == "$v" ]] || bad "el tag $2 no coincide con Cargo.toml ($v)"
    [[ $(grep -c "^version = \"$v\"" Cargo.lock) -ge 3 ]] || bad "Cargo.lock no está en $v"
    [[ $(sed -n 's/^pkgver=//p' "$stable") == "$v" ]] || bad "$stable no está en $v"
    grep -q "<release version=\"$v\"" "${meta[0]}" || bad "${meta[0]} no tiene la versión $v"
    [[ -n $(notes "$v") ]] || bad "CHANGELOG.md no tiene la sección de $v"
    [[ $fail -eq 0 ]] && echo "Versión $v coherente."
    exit $fail
    ;;
notes)
    notes "${2:?uso: version.sh notes X.Y.Z}"
    ;;
*)
    sed -n '2,5p' "$0" >&2
    exit 2
    ;;
esac
