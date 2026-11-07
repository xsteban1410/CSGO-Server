#!/usr/bin/env bash
# Installs what the deb, the AppImage and the Flatpak carry into a prefix, from a finished build:
#   packaging/stage.sh <prefix> <folder with the binaries>
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
prefix=$1
bin=$2
id=io.github.xsteban1410.CsgoLegacyServer
install -Dm755 "$bin/csgo-legacy-server" "$bin/csgo-legacy-server-app" "$bin/csgo-legacy-server-agent" -t "$prefix/bin/"
share=$prefix/share/csgo-legacy-server
install -d "$share"
cp -r --no-preserve=ownership "$root"/game/{cfg,modes,plugin,data} "$share/"
find "$share" -type d -exec chmod 755 {} + && find "$share" -type f -exec chmod 644 {} +
install -Dm644 "$root/packaging/$id.desktop" -t "$prefix/share/applications/"
install -Dm644 "$root/packaging/$id.metainfo.xml" -t "$prefix/share/metainfo/"
install -Dm644 "$root/packaging/$id.svg" -t "$prefix/share/icons/hicolor/scalable/apps/"
for s in 32 48 64 128 256 512; do
    install -Dm644 "$root/packaging/icons/$s.png" "$prefix/share/icons/hicolor/${s}x${s}/apps/$id.png"
done
install -Dm644 "$root/README.md" "$root/README.es.md" "$root/CHANGELOG.md" -t "$prefix/share/doc/csgo-legacy-server/"
cli=$bin/csgo-legacy-server
"$cli" completions bash | install -Dm644 /dev/stdin "$prefix/share/bash-completion/completions/csgo-legacy-server"
"$cli" completions zsh | install -Dm644 /dev/stdin "$prefix/share/zsh/site-functions/_csgo-legacy-server"
"$cli" completions fish | install -Dm644 /dev/stdin "$prefix/share/fish/vendor_completions.d/csgo-legacy-server.fish"
