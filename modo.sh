#!/bin/bash
# Cambia de modo con el servidor encendido. Ejemplos:
#   ./modo.sh awp                 empieza cuando haya 2 humanos
#   ./modo.sh retake alone -s a -d 2 -m de_inferno
#   ./modo.sh zombies 3 --set modo_t_vida=800
source "$(dirname "$0")/lib.sh"
[ $# -gt 0 ] || { uso; exit 1; }
parse_args "$@"

ACT=$(rcon host_map 2>/dev/null | sed -n 's/^"host_map" = "\(.*\)\.bsp".*/\1/p')
[ -n "$ACT" ] || fallo "el servidor no responde por rcon. ¿Está arrancado? (./arrancar.sh)"
pick_map "$ACT"

# El plugin aplica modo_base + modo_<modo> + modo_extra al cargar el mapa.
CAMBIO="changelevel $MAPA_FINAL"
[ -n "$WORKSHOP" ] && CAMBIO="host_workshop_map $WORKSHOP"
rcon "modo_nombre $MODO" "modo_humanos $H" "modo_extra \"$EXTRA\"" \
     "game_type $T" "game_mode $G" "$CAMBIO" >/dev/null
echo "Modo $MODO en $MAPA_FINAL; empieza con $H humano(s). Ajustes: $EXTRA"
