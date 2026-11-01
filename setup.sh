#!/bin/bash
# Instala y configura todo el servidor CS:GO Legacy en este directorio. Se puede repetir: salta lo que ya está hecho.
#   ./setup.sh               instalar
#   ./setup.sh --actualizar  además, actualizar/validar los archivos del servidor con SteamCMD
set -euo pipefail
cd "$(dirname "$0")"
DIR=$PWD
ACTUALIZAR=0
[ "${1:-}" = --actualizar ] && ACTUALIZAR=1

paso() { printf '\n\033[1;32m==> %s\033[0m\n' "$*"; }
fallo() { printf '\033[1;31mError:\033[0m %s\n' "$*" >&2; exit 1; }

# --- 1. dependencias (no se instala nada con sudo: solo se dice qué falta) ---------
paso "Comprobando dependencias"
falta=()
for c in curl tar python3 ss gzip; do command -v "$c" >/dev/null || falta+=("$c"); done
# srcds y steamcmd son de 32 bits
[ -e /lib/ld-linux.so.2 ] || [ -e /lib32/ld-linux.so.2 ] || [ -e /usr/lib32/ld-linux.so.2 ] || falta+=("libc 32 bits")
if [ ${#falta[@]} -gt 0 ]; then
  echo "Falta: ${falta[*]}"
  echo "  Debian/Ubuntu: sudo dpkg --add-architecture i386 && sudo apt update && sudo apt install curl tar python3 iproute2 lib32gcc-s1 lib32stdc++6 libc6-i386"
  echo "  Arch:          sudo pacman -S curl tar python iproute2 lib32-gcc-libs lib32-glibc   (con [multilib] activado)"
  echo "  Fedora:        sudo dnf install curl tar python3 iproute glibc.i686 libstdc++.i686"
  exit 1
fi

# --- 2. SteamCMD + servidor dedicado (app 740) -------------------------------------
if [ ! -x steamcmd/steamcmd.sh ]; then
  paso "Descargando SteamCMD"
  mkdir -p steamcmd
  curl -fsSL https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz | tar -xz -C steamcmd
fi

if [ ! -x server/srcds_run ] || [ $ACTUALIZAR = 1 ]; then
  paso "Instalando el servidor dedicado de CS:GO (app 740, ~33 GB). Tarda; se puede cortar y volver a lanzar."
  for intento in 1 2 3 4 5; do
    # SteamCMD falla a menudo a la primera (se autoactualiza, timeouts): se reintenta
    if steamcmd/steamcmd.sh +force_install_dir "$DIR/server" +login anonymous +app_update 740 validate +quit; then
      [ -x server/srcds_run ] && break
    fi
    echo "SteamCMD no terminó (intento $intento/5), reintentando..."
    sleep 5
  done
  [ -x server/srcds_run ] || fallo "no se pudo instalar la app 740. Vuelve a lanzar ./setup.sh"
fi

# --- 3. arreglos del servidor (validate los deshace, por eso se repiten siempre) ----
paso "Ajustando el servidor"
# La libgcc que trae Valve es vieja y cuelga srcds al arrancar en distros modernas
rm -f server/bin/libgcc_s.so.1

# --- 4. Metamod:Source y SourceMod 1.12 --------------------------------------------
mkdir -p dl
bajar() { # $1 = url del directorio, $2 = prefijo; deja en $ARCHIVO la ruta descargada
  local nombre
  nombre=$(curl -fsSL "$1/$2-latest-linux") || fallo "no pude consultar $1"
  [[ $nombre =~ ^[A-Za-z0-9._-]+\.tar\.gz$ ]] || fallo "respuesta inesperada de $1: $nombre"
  ARCHIVO="dl/$nombre"
  [ -f "$ARCHIVO" ] || curl -fsSL -o "$ARCHIVO" "$1/$nombre"
}
if [ ! -f server/csgo/addons/metamod/bin/server.so ]; then
  paso "Instalando Metamod:Source"
  bajar https://mms.alliedmods.net/mmsdrop/1.12 mmsource
  tar -xzf "$ARCHIVO" -C server/csgo
fi
if [ ! -f server/csgo/addons/sourcemod/bin/sourcemod_mm_i486.so ] || [ $ACTUALIZAR = 1 ]; then
  paso "Instalando SourceMod"
  bajar https://sm.alliedmods.net/smdrop/1.12 sourcemod
  # --skip-old-files: no pisar configs, datos ni la base de datos de skins al reinstalar
  tar -xzf "$ARCHIVO" -C server/csgo --skip-old-files
  tar -xzf "$ARCHIVO" -C server/csgo --overwrite --wildcards 'addons/sourcemod/bin/*' 'addons/sourcemod/scripting/*' 'addons/sourcemod/gamedata/*' 'addons/sourcemod/extensions/*'
fi

# El servidor es de 32 bits: el vdf de 64 bits hace que Metamod no cargue
rm -f server/csgo/addons/metamod_x64.vdf
printf '"Plugin"\n{\n\t"file"\t"addons/metamod/bin/server"\n}\n' > server/csgo/addons/metamod.vdf

# --- 5. config.env -------------------------------------------------------------------
if [ ! -f config.env ]; then
  paso "Creando config.env con una contraseña rcon aleatoria"
  pass=$(tr -dc 'A-Za-z0-9' < /dev/urandom | head -c 20 || true)
  sed "s/^RCON_PASSWORD=.*/RCON_PASSWORD=$pass/" config.example.env > config.env
  chmod 600 config.env
fi

# --- 6. skins y plugins ----------------------------------------------------------------
if [ ! -f data/skins.cfg ]; then
  paso "Generando la lista de skins"
  python3 -I tools/generar_skins.py server/csgo data/skins.cfg
fi
paso "Compilando plugins y copiando configs"
./desplegar.sh

PUERTO=$(sed -n 's/^PUERTO=//p' config.env)
cat <<EOF

Listo. Para jugar:
  ./arrancar.sh awp alone        (./arrancar.sh --help para ver modos y opciones)

Cada jugador necesita CS2 con la beta "csgo_legacy" (Steam > CS2 > Propiedades > Betas)
y entra desde la consola con:  connect <IP de este PC>:$PUERTO
Para jugar por internet hay que abrir el puerto UDP $PUERTO en el router o usar una VPN.
EOF
