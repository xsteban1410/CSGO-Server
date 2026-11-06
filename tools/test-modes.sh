#!/usr/bin/env bash
# Runs modes and match types on the real server without humans and checks over rcon
# that game_type is the expected one, that bots move and that they respawn (or the round goes on).
# Modes with bots that stand still (aim) or without bots (duelo) only check the game type.
# Usage: tools/test-modes.sh [mode[:type] ...]   (the server must be running: csgo-legacy-server start ...)
set -u
B="${CSLS:-./target/debug/csgo-legacy-server}"
r() { "$B" rcon "$1" 2>/dev/null; }
val() { r "$1" | grep -m1 "^\"$1\"" | cut -d'"' -f4; }
pos() { r modo_estado | grep -o 'pos=.*' | sort; }
failures=0

check_case() {
    local case_name=$1 mode=${1%%:*} type=""
    [[ $case_name == *:* ]] && type=${case_name#*:}
    local args=("$mode")
    [[ -n $type ]] && args+=(-t "$type")
    if ! output=$("$B" mode "${args[@]}" --set modo_humanos=0 --set mp_warmuptime=5 --set bot_join_after_player=0 2>&1); then
        echo "FAILED $case_name: $output"; failures=$((failures + 1)); return
    fi
    sleep 15
    r "mp_warmup_end" >/dev/null
    sleep $(( $(val mp_freezetime | cut -d. -f1) + 9 ))
    local a b n moved still
    a=$(pos); n=$(wc -l <<<"$a"); sleep 6; b=$(pos)
    still=$(comm -12 <(echo "$a") <(echo "$b") | wc -l)
    moved=$((n - still))
    local gt rt; gt="$(val game_type)/$(val game_mode)"; rt=$(val mp_respawn_on_death_t)
    r "sv_cheats 1; bot_kill; sv_cheats 0" >/dev/null
    sleep 12
    local alive; alive=$(r modo_estado | grep -c 'pos=')
    local state=ok
    [[ $n -le 1 && -z $a && $mode != duelo ]] && state=NO_BOTS
    # in Guardian the bots wait still until a human is nearby; aim bots never move; retake bots hold the planted bomb
    [[ $n -gt 1 && $moved -eq 0 && $gt != 4/0 && $mode != aim && $mode != retake ]] && state=NOT_MOVING
    [[ $rt == 1 && $alive -eq 0 ]] && state=NO_RESPAWN
    [[ $state == ok ]] || failures=$((failures + 1))
    printf '%-24s %-14s gt=%s bots=%-2s moved=%-2s respawn=%s alive_after_kill=%-2s map=%s\n' "$case_name" "$state" "$gt" "$n" "$moved" "$rt" "$alive" "$(val host_map)"
}

if [[ $# -gt 0 ]]; then cases=("$@"); else
    mapfile -t cases < <("$B" modes | awk '{print $1}')
    cases+=(normal:short normal:casual normal:wingman normal:dm normal:ffa normal:tdm normal:armsrace normal:demolition normal:guardian normal:dz awp:dm)
fi
for c in "${cases[@]}"; do check_case "$c"; done
echo "Failures: $failures"
[[ $failures -eq 0 ]]
