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
# A server with no humans hibernates and adds no bots.
r "sv_hibernate_when_empty 0" >/dev/null

check_case() {
    local case_name=$1 mode=${1%%:*} type=""
    [[ $case_name == *:* ]] && type=${case_name#*:}
    local args=("$mode")
    [[ -n $type ]] && args+=(-t "$type")
    if ! output=$("$B" mode "${args[@]}" --set modo_humanos=0 --set mp_warmuptime=5 --set mp_freezetime=0 --set bot_join_after_player=0 2>&1); then
        echo "FAILED $case_name: $output"; failures=$((failures + 1)); return
    fi
    # Give changelevel time to start, or the bots seen below are the old map's.
    sleep 2
    local a b n moved=0 still
    [[ $mode == duelo ]] || for _ in {1..25}; do [[ -n $(pos) ]] && break; sleep 1; done
    r "mp_warmup_end" >/dev/null
    # The round restart teleports everyone, which would look like moving.
    sleep $(( $(val mp_freezetime | cut -d. -f1) + 4 ))
    local gt rt; gt="$(val game_type)/$(val game_mode)"; rt=$(val mp_respawn_on_death_t)
    a=$(pos); n=$(wc -l <<<"$a")
    # Stop at the first bot that moves; these never do: Guardian waits for a human, aim stands still, retake holds the bomb.
    local expect_still=0
    [[ $gt == 4/0 || $mode == aim || $mode == retake || $mode == duelo ]] && expect_still=1
    if [[ $expect_still == 0 ]]; then
        for _ in {1..10}; do
            sleep 1; b=$(pos)
            still=$(comm -12 <(echo "$a") <(echo "$b") | wc -l)
            moved=$((n - still))
            [[ $moved -gt 0 ]] && break
        done
    fi
    # Only modes that respawn have something to check after killing the bots.
    local alive=-
    if [[ $rt == 1 ]]; then
        r "sv_cheats 1; bot_kill; sv_cheats 0" >/dev/null
        sleep 2
        for _ in {1..10}; do
            alive=$(r modo_estado | grep -c 'pos=')
            [[ $alive -gt 0 ]] && break
            sleep 1
        done
    fi
    local state=ok
    [[ $n -le 1 && -z $a && $mode != duelo ]] && state=NO_BOTS
    [[ $n -gt 1 && $moved -eq 0 && $expect_still == 0 ]] && state=NOT_MOVING
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
