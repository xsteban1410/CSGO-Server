#!/usr/bin/env bash
# Recorre modos y tipos de partida en el servidor real sin humanos y comprueba por rcon
# que el game_type es el esperado, que los bots se mueven y que reaparecen (o que la ronda sigue).
# Uso: tools/probar-modos.sh [modo[:tipo] ...]   (necesita el servidor encendido: csgo-legacy-server start ...)
set -u
B="${CSLS:-./target/debug/csgo-legacy-server}"
r() { "$B" rcon "$1" 2>/dev/null; }
val() { r "$1" | grep -m1 "^\"$1\"" | cut -d'"' -f4; }
pos() { r modo_estado | grep -o 'pos=.*' | sort; }
fallos=0

probar() {
    local caso=$1 modo=${1%%:*} tipo=""
    [[ $caso == *:* ]] && tipo=${caso#*:}
    local args=("$modo")
    [[ -n $tipo ]] && args+=(-t "$tipo")
    if ! salida=$("$B" mode "${args[@]}" --set modo_humanos=0 --set mp_warmuptime=5 --set bot_join_after_player=0 2>&1); then
        echo "FALLO $caso: $salida"; fallos=$((fallos + 1)); return
    fi
    sleep 15
    r "mp_warmup_end" >/dev/null
    sleep $(( $(val mp_freezetime | cut -d. -f1) + 9 ))
    local a b n movidos quietos
    a=$(pos); n=$(wc -l <<<"$a"); sleep 6; b=$(pos)
    quietos=$(comm -12 <(echo "$a") <(echo "$b") | wc -l)
    movidos=$((n - quietos))
    local gt rt; gt="$(val game_type)/$(val game_mode)"; rt=$(val mp_respawn_on_death_t)
    r "sv_cheats 1; bot_kill; sv_cheats 0" >/dev/null
    sleep 12
    local vivos; vivos=$(r modo_estado | grep -c 'pos=')
    local estado=ok
    [[ $n -le 1 && -z $a ]] && estado=SIN_BOTS
    # en Guardián los bots esperan quietos hasta que hay un humano cerca
    [[ $n -gt 1 && $movidos -eq 0 && $gt != 4/0 ]] && estado=QUIETOS
    [[ $rt == 1 && $vivos -eq 0 ]] && estado=NO_REAPARECEN
    [[ $estado == ok ]] || fallos=$((fallos + 1))
    printf '%-24s %-14s gt=%s bots=%-2s movidos=%-2s respawn=%s vivos_tras_matar=%-2s mapa=%s\n' "$caso" "$estado" "$gt" "$n" "$movidos" "$rt" "$vivos" "$(val host_map)"
}

if [[ $# -gt 0 ]]; then casos=("$@"); else
    mapfile -t casos < <("$B" modes | awk '{print $1}')
    casos+=(normal:short normal:casual normal:wingman normal:dm normal:ffa normal:tdm normal:armsrace normal:demolition normal:guardian normal:dz awp:dm)
fi
for c in "${casos[@]}"; do probar "$c"; done
echo "Fallos: $fallos"
[[ $fallos -eq 0 ]]
