#!/bin/bash
# Compila los plugins y copia las configs al servidor. Si está encendido, recarga los plugins.
set -e
cd "$(dirname "$0")"
SM=server/csgo/addons/sourcemod
SPCOMP="$SM/scripting/spcomp"

"$SPCOMP" -i "$SM/scripting/include" plugin/modos.sp -o "$SM/plugins/modos.smx" | grep -i "error\|warning" || true
"$SPCOMP" -i "$SM/scripting/include" plugin/nolobbyreservation/nolobbyreservation.sp \
  -o "$SM/plugins/nolobbyreservation.smx" | grep -i "error\|warning" || true
"$SPCOMP" -i "$SM/scripting/include" plugin/skins.sp -o "$SM/plugins/skins.smx" | grep -i "error\|warning" || true
cp plugin/nolobbyreservation/nolobbyreservation.games.txt "$SM/gamedata/"
cp data/skins.cfg "$SM/configs/skins.cfg"
# Permite dar cuchillos especiales con GivePlayerItem (lo usa skins.sp)
sed -i 's/"FollowCSGOServerGuidelines"\s*"yes"/"FollowCSGOServerGuidelines"\t"no"/' "$SM/configs/core.cfg"
cp cfg/*.cfg server/csgo/cfg/

if python3 -I rcon.py "sm plugins reload nolobbyreservation" "sm plugins reload modos" "sm plugins load skins" "sm plugins reload skins" >/dev/null 2>&1; then
  echo "Desplegado y recargado. Usa ./modo.sh para aplicar el modo con las configs nuevas."
else
  echo "Desplegado (servidor apagado: se aplicará al arrancar)."
fi
