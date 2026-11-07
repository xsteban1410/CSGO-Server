#!/usr/bin/env bash
# Builds the .deb and the AppImage. They run on systems at least as new as the one that builds them, so CI uses Debian 12:
#   packaging/build-bundles.sh [output folder]
# Needs cargo, go, curl, mksquashfs and the -dev packages listed in .github/workflows/ci.yml.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
out=$(mkdir -p "${1:-$root/dist}" && cd "${1:-$root/dist}" && pwd)
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
target=${CARGO_TARGET_DIR:-$root/target}
id=io.github.xsteban1410.CsgoLegacyServer
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

nfpm_version=2.48.0
nfpm_sha=6d7f9a28b2f397934403950fcbba83f598b08d02edbe79a0815b3b4897341750
runtime_tag=20251108
runtime_sha=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d

fetch() {
    curl -fsSL --retry 3 -o "$3" "$1"
    echo "$2  $3" | sha256sum -c --quiet -
}

# Bundled libraries are found through the binary, not LD_LIBRARY_PATH, which the game and Steam would inherit
cd "$root"
# shellcheck disable=SC2016 # $ORIGIN is for the dynamic linker
RUSTFLAGS='-C link-arg=-Wl,--disable-new-dtags,-rpath,$ORIGIN/../lib' cargo build --release --locked -p csgo-legacy-server -p csgo-legacy-server-app
(cd go && CGO_ENABLED=0 go build -trimpath -o "$target/release/csgo-legacy-server-agent" ./cmd/agent)
packaging/stage.sh "$work/usr" "$target/release"

fetch "https://github.com/goreleaser/nfpm/releases/download/v$nfpm_version/nfpm_${nfpm_version}_Linux_x86_64.tar.gz" "$nfpm_sha" "$work/nfpm.tar.gz"
tar -xzf "$work/nfpm.tar.gz" -C "$work" nfpm
(cd "$work" && VERSION=$version ./nfpm package --config "$root/packaging/deb/nfpm.yaml" --packager deb --target "$out/")

appdir=$work/AppDir
mkdir "$appdir"
cp -r "$work/usr" "$appdir/usr"
# Only what a desktop may lack: fontconfig, freetype and graphics must come from the system
install -d "$appdir/usr/lib"
for lib in libsqlite3.so.0 libxkbcommon.so.0; do
    cp -L "/usr/lib/x86_64-linux-gnu/$lib" "$appdir/usr/lib/"
done
install -m755 packaging/appimage/AppRun "$appdir/AppRun"
cp "$appdir/usr/share/applications/$id.desktop" "$appdir/"
cp "packaging/$id.svg" "$appdir/"
fetch "https://github.com/AppImage/type2-runtime/releases/download/$runtime_tag/runtime-x86_64" "$runtime_sha" "$work/runtime"
mksquashfs "$appdir" "$work/app.squashfs" -root-owned -noappend -comp zstd -quiet
cat "$work/runtime" "$work/app.squashfs" > "$out/csgo-legacy-server-$version-x86_64.AppImage"
chmod 755 "$out/csgo-legacy-server-$version-x86_64.AppImage"
ls -1 "$out"
