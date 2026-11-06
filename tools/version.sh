#!/usr/bin/env bash
# Project version, spread over Cargo.toml, Cargo.lock, the PKGBUILDs, the metainfo and the changelog.
#   tools/version.sh bump X.Y.Z    changes it everywhere and turns "Unreleased" in the changelog into that version
#   tools/version.sh check [vX.Y.Z]  fails if they differ from each other or from the tag
#   tools/version.sh notes X.Y.Z   prints that version's section of the changelog
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
    [[ $new =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "invalid version: $new" >&2; exit 1; }
    old=$(current)
    [[ $new != "$old" ]] || { echo "already at $new" >&2; exit 1; }
    [[ -n $(notes Unreleased) ]] || { echo "CHANGELOG.md: first write the changes under ## [Unreleased]" >&2; exit 1; }
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
    echo "Version $new ready. Commit and push: GitHub creates the tag and the release."
    ;;
check)
    v=$(current)
    fail=0
    bad() { echo "ERROR: $1" >&2; fail=1; }
    [[ -z ${2:-} || ${2#v} == "$v" ]] || bad "tag $2 does not match Cargo.toml ($v)"
    [[ $(grep -c "^version = \"$v\"" Cargo.lock) -ge 3 ]] || bad "Cargo.lock is not at $v"
    [[ $(sed -n 's/^pkgver=//p' "$stable") == "$v" ]] || bad "$stable is not at $v"
    grep -q "<release version=\"$v\"" "${meta[0]}" || bad "${meta[0]} does not have version $v"
    [[ -n $(notes "$v") ]] || bad "CHANGELOG.md has no section for $v"
    [[ $fail -eq 0 ]] && echo "Version $v is consistent."
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
