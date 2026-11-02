#!/bin/bash
# Funciones compartidas por arrancar.sh y modo.sh. No se ejecuta solo.
DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
[ -f "$DIR/config.env" ] || { echo "Falta config.env: cp config.example.env config.env y edítalo" >&2; exit 1; }
# shellcheck source=config.example.env
source "$DIR/config.env"
CFG="$DIR/server/csgo/cfg"

rcon() { python3 -I "$DIR/rcon.py" "$@"; }

modos() { ls "$DIR"/cfg/modo_*.cfg | sed 's|.*/modo_||; s|\.cfg$||' | grep -vx base | tr '\n' ' '; }

uso() {
  cat <<EOF
Uso: $(basename "$0") <modo> [alone|N] [opciones]
  alone | N              humanos necesarios para empezar (por defecto $HUMANOS)
  -m, --mapa MAPA        mapa: de_dust2, de_inferno, de_mirage, de_nuke... (por defecto el actual)
  -t, --tipo TIPO        competitivo | casual | wingman (por defecto $TIPO)
  -d, --dificultad N     dificultad de los bots: 0 fácil ... 3 experto (por defecto $DIFICULTAD)
  -b, --bots N           cantidad de bots
  -r, --rondas N         rondas máximas
  -s, --sitio S          retake/defensa: a | b | random
  -p, --precision        precisión infinita: sin dispersión al moverse/saltar y sin retroceso (para todos)
  -w, --workshop ID      mapa del workshop por su ID (necesita WORKSHOP_AUTHKEY en config.env)
      --set cvar=valor   cualquier otra variable del servidor (repetible)
Modos: $(modos)
EOF
}

fallo() { echo "Error: $*" >&2; exit 1; }
numero() { [[ $1 =~ ^[0-9]+$ ]] || fallo "$2 tiene que ser un número (recibido: '$1')"; }

# Lee los argumentos y deja listos: MODO H T G MAPA_ARG EXTRA
parse_args() {
  local d=$DIFICULTAD tp=$TIPO
  H=$HUMANOS MAPA_ARG="" EXTRA="" WORKSHOP=""
  if [ $# -gt 0 ] && [[ $1 != -* ]]; then MODO=$1; shift; fi
  if [ $# -gt 0 ] && [[ $1 != -* ]]; then H=$1; shift; fi
  [ "$H" = alone ] && H=1
  while [ $# -gt 0 ]; do
    case $1 in
      -h|--help) uso; exit 0 ;;
      -p|--precision) EXTRA+="; weapon_accuracy_nospread 1; weapon_recoil_scale 0"; shift; continue ;;
    esac
    [ $# -ge 2 ] || fallo "falta el valor de $1"
    case $1 in
      -m|--mapa)       MAPA_ARG=$2 ;;
      -t|--tipo)       tp=$2 ;;
      -d|--dificultad) d=$2 ;;
      -b|--bots)       numero "$2" "--bots"; EXTRA+="; bot_quota $2" ;;
      -r|--rondas)     numero "$2" "--rondas"; EXTRA+="; mp_maxrounds $2" ;;
      -s|--sitio)      [[ $2 =~ ^(a|b|random)$ ]] || fallo "--sitio es a, b o random"; EXTRA+="; modo_sitio $2" ;;
      -w|--workshop)   numero "$2" "--workshop"
                       [ -n "${WORKSHOP_AUTHKEY:-}" ] || fallo "los mapas del workshop necesitan WORKSHOP_AUTHKEY en config.env (clave en https://steamcommunity.com/dev/apikey)"
                       WORKSHOP=$2 ;;
      --set)           [[ $2 =~ ^[a-z0-9_]+=[^\;\"]+$ ]] || fallo "--set espera cvar=valor sin ; ni comillas"
                       EXTRA+="; ${2%%=*} ${2#*=}" ;;
      *) fallo "opción desconocida: $1 (usa --help)" ;;
    esac
    shift 2
  done

  [ -f "$DIR/cfg/modo_$MODO.cfg" ] && [ "$MODO" != base ] || fallo "modo desconocido '$MODO'. Modos: $(modos)"
  numero "$H" "Humanos"
  [ "$H" -ge 1 ] || fallo "hace falta al menos 1 humano"
  [[ $d =~ ^[0-3]$ ]] || fallo "--dificultad va de 0 a 3"
  EXTRA="bot_difficulty $d$EXTRA"

  # game_type/game_mode de Valve
  case $MODO in
    armsrace)                     T=1 G=0 ;;
    demolicion)                   T=1 G=1 ;;
    dm|awpdm|dmcuchillo|dmcabeza) T=1 G=2 ;;
    guardian)                     T=4 G=0 ;;
    *) case $tp in
         competitivo) T=0 G=1 ;;
         casual)      T=0 G=0 ;;
         wingman)     T=0 G=2 ;;
         *) fallo "--tipo es competitivo, casual o wingman" ;;
       esac ;;
  esac
}

# Mapa por defecto de los modos que necesitan mapas propios (prefijo y mapa)
mapa_modo() {
  case $MODO in
    armsrace) echo "ar_ ar_shoots" ;;
    guardian) echo "gd_ gd_cbble" ;;
    rehenes)  echo "cs_ cs_office" ;;
    retake|defensa) echo "de_ $MAPA" ;;
  esac
}

# Elige el mapa (deja MAPA_FINAL). $1 = mapa actual, vacío al arrancar.
pick_map() {
  local pre def
  read -r pre def <<<"$(mapa_modo)"
  if [ -n "$WORKSHOP" ]; then MAPA_FINAL="workshop/$WORKSHOP"; return
  elif [ -n "$MAPA_ARG" ]; then MAPA_FINAL=$MAPA_ARG
  elif [ -n "$pre" ]; then [[ $1 == "$pre"* ]] && MAPA_FINAL=$1 || MAPA_FINAL=$def
  elif [ -n "$1" ] && [[ $1 != ar_* && $1 != gd_* && $1 != workshop/* ]]; then MAPA_FINAL=$1
  else MAPA_FINAL=$MAPA; fi
  [ -f "$DIR/server/csgo/maps/$MAPA_FINAL.bsp" ] || fallo "no existe el mapa '$MAPA_FINAL' en server/csgo/maps"
}
