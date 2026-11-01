#!/bin/bash
# Abre CS:GO Legacy o CS2 desde la misma instalación (CS2 con la beta csgo_legacy trae los dos).
#   ./jugar.sh csgo                  CS:GO Legacy
#   ./jugar.sh csgo 192.168.1.5:27030  CS:GO Legacy y conectar a un servidor
#   ./jugar.sh csgo local            CS:GO Legacy y conectar al servidor de este PC
#   ./jugar.sh cs2                   CS2 (lo que abre Steam con el botón Jugar)
set -e
uso() { sed -n '2,6p' "$0" | sed 's/^# \{0,1\}//'; exit 1; }

STEAM=
for d in "$HOME/.local/share/Steam" "$HOME/.steam/steam" "$HOME/.var/app/com.valvesoftware.Steam/.local/share/Steam"; do
  [ -d "$d/steamapps/common" ] && { STEAM=$d; break; }
done
[ -n "$STEAM" ] || { echo "No encuentro la carpeta de Steam" >&2; exit 1; }
C=$STEAM/steamapps/common
JUEGO="$C/Counter-Strike Global Offensive"
pgrep -x steam >/dev/null || { echo "Abre Steam primero" >&2; exit 1; }

case ${1:-} in
  cs2)
    steam steam://rungameid/730 >/dev/null 2>&1 &
    ;;
  csgo)
    [ -x "$JUEGO/csgo.sh" ] || { echo "No está CS:GO Legacy: pon la beta csgo_legacy en Steam > CS2 > Propiedades > Betas" >&2; exit 1; }
    # Steam abre siempre CS2; CS:GO se lanza con su csgo.sh dentro del Steam Linux Runtime, como haría Steam
    extra=()
    dest=${2:-}
    if [ "$dest" = local ]; then  # el servidor de este PC: su IP de red y el PUERTO de config.env
      p=$(sed -n 's/^PUERTO=//p' "$(dirname "$(readlink -f "$0")")/config.env" 2>/dev/null)
      dest="$(ip -4 route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i=="src"){print $(i+1); exit}}'):${p:-27030}"
    fi
    [ -n "$dest" ] && extra=(+connect "$dest")
    cd "$JUEGO"
    SteamAppId=730 SteamGameId=730 setsid "$C/SteamLinuxRuntime_soldier/_v2-entry-point" --verb=waitforexitandrun -- \
      "$C/SteamLinuxRuntime/scout-on-soldier-entry-point-v2" -- "$JUEGO/csgo.sh" -steam -console "${extra[@]}" \
      >/dev/null 2>&1 &
    ;;
  *) uso ;;
esac
echo "Abriendo ${1}..."
