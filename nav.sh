#!/bin/bash
# Genera la malla de navegación (.nav) del mapa actual para que los bots sepan moverse.
# Hace falta en mapas del workshop que no la traen. Tarda de segundos a minutos y luego recarga el mapa.
source "$(dirname "$0")/lib.sh"
rcon "sv_cheats 1" "nav_generate" >/dev/null || fallo "el servidor no responde por rcon. ¿Está arrancado?"
echo "Generando .nav; el mapa se recargará solo y se guardará para la próxima vez (sv_cheats vuelve a 0 con el modo)."
