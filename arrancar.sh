#!/bin/bash
# Arranca el servidor. Acepta las mismas opciones que modo.sh; sin argumentos usa config.env.
#   ./arrancar.sh
#   ./arrancar.sh zombies alone -m de_nuke
source "$(dirname "$0")/lib.sh"
parse_args "$@"
pick_map ""

if ss -lun 2>/dev/null | grep -q ":$PUERTO "; then
  fallo "el puerto $PUERTO está ocupado (¿servidor ya encendido o el juego usando ese puerto?)"
fi

cd "$DIR/server" || exit 1
# Cliente CS:GO Legacy = rama csgo_legacy de la app 730. steamcmd validate resetea estos ficheros.
echo -n 730 > steam_appid.txt
sed -i 's/^appID=.*/appID=730\r/' csgo/steam.inf

# Modo inicial: el plugin lo ejecuta una vez al cargar.
printf 'modo_nombre "%s"\nmodo_humanos %s\nmodo_extra "%s"\n' "$MODO" "$H" "$EXTRA" > csgo/cfg/modo_inicio.cfg

# Mapa: normal con +map, del workshop con +host_workshop_map (lo descarga con la clave de la Web API)
MAPA_CMD=(+map "$MAPA_FINAL")
AUTH=()
[ -n "${WORKSHOP_AUTHKEY:-}" ] && AUTH=(-authkey "$WORKSHOP_AUTHKEY")
[ -n "$WORKSHOP" ] && MAPA_CMD=(+host_workshop_map "$WORKSHOP")

IP=$(ip -4 route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i=="src"){print $(i+1); exit}}')  # hostname -I no existe en todas las distros
echo "Modo $MODO en $MAPA_FINAL. Conectar con: connect ${IP:-IP_DE_ESTE_PC}:$PUERTO"
export LD_LIBRARY_PATH=".:bin:$LD_LIBRARY_PATH"
exec ./srcds_run -game csgo -console -insecure -condebug -usercon -port "$PUERTO" "${AUTH[@]}" \
  -maxplayers_override "$MAX_JUGADORES" -tickrate "$TICKRATE" +sv_lan 0 \
  +game_type "$T" +game_mode "$G" +mapgroup mg_active "${MAPA_CMD[@]}" \
  +rcon_password "$RCON_PASSWORD" +exec server.cfg
