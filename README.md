# CSGO Server

Servidor privado de **CS:GO Legacy** para jugar con amigos contra bots. Usa la rama `csgo_legacy` de la app 730.
Trae más de 30 modos: francotiradores, zombies por oleadas, retakes, Guardián, One in the Chamber… También guarda las skins de cada jugador.
Todo se configura desde la terminal.

Probado en Linux (CachyOS, Arch). Debería funcionar en cualquier distro de 64 bits con las librerías de 32 bits.

## Instalación

```bash
git clone https://github.com/xsteban1410/CSGO-Server.git
cd CSGO-Server
./setup.sh
```

`setup.sh` hace todo esto, y si lo vuelves a lanzar se salta lo que ya está hecho:
1. Comprueba las dependencias (curl, tar, python3 y las librerías de 32 bits). Si falta algo, te dice el comando para apt, pacman o dnf.
2. Descarga SteamCMD y el servidor dedicado de CS:GO (app **740**, unos 33 GB) en `server/`.
3. Borra `server/bin/libgcc_s.so.1`. Esa copia vieja hace que el servidor se cuelgue en distros modernas.
4. Instala [Metamod:Source 1.12](https://www.sourcemm.net/) y [SourceMod 1.12](https://www.sourcemod.net/), y deja `metamod.vdf` preparado para 32 bits.
5. Crea `config.env` con una contraseña rcon aleatoria.
6. Genera la lista de skins, compila los plugins y copia las configs.

`./setup.sh --actualizar` vuelve a validar los archivos del servidor con SteamCMD y reaplica los arreglos.

Cada jugador necesita **CS2 con la beta `csgo_legacy`** (Steam > CS2 > Propiedades > Betas) y entra desde la consola con `connect <IP>:<PUERTO>`.
Para que alguien se conecte desde fuera de tu red, abre el puerto UDP (27030 por defecto) en el router o usad la VPN que prefiráis.

## Uso

```
./arrancar.sh [modo] [alone|N] [opciones]   # arrancar el servidor
./modo.sh     <modo> [alone|N] [opciones]   # cambiar de modo con el servidor encendido
```

| Argumento | Qué hace |
|---|---|
| *(nada)* | la partida empieza cuando haya `HUMANOS` humanos (2 por defecto) |
| `alone` | empieza en cuanto entras tú |
| `N` | espera a N humanos |
| `-m, --mapa` | mapa (`de_dust2`, `de_inferno`, `de_mirage`, `de_nuke`…) |
| `-t, --tipo` | `competitivo`, `casual` o `wingman` |
| `-d, --dificultad` | bots de 0 (fácil) a 3 (experto) |
| `-b, --bots` | cantidad de bots |
| `-r, --rondas` | rondas máximas |
| `-s, --sitio` | retake/defensa: `a`, `b` o `random` |
| `-w, --workshop` | mapa del workshop por su ID (ver abajo) |
| `--set cvar=valor` | cualquier variable del servidor, repetible |

Ejemplos:

```bash
./modo.sh awp alone
./modo.sh retake 2 -s b -d 2 -m de_inferno
./modo.sh zombies --set modo_t_vida=800 --set modo_t_velocidad=1.5
./modo.sh libre alone -t casual
```

Cada cambio de modo recarga el mapa. Así las reglas oficiales del tipo elegido se aplican limpias y no quedan restos del modo anterior.

## Modos

Por defecto, los humanos van en CT y los bots en T (5, en dificultad experto). Cada modo es un archivo `cfg/modo_<nombre>.cfg`, así que añadir uno nuevo es crear otro archivo.

| Modo | Descripción |
|---|---|
| `awp` | AWP + Deagle |
| `scout` | SSG 08 + P250 |
| `autosniper` | SCAR-20 + Deagle |
| `luna` | AWP + Deagle con gravedad baja |
| `rifles` | AK-47 + Deagle |
| `headshot` | AK-47 + Deagle, solo daño a la cabeza |
| `escopetas` | XM1014 + P250 |
| `negev` | Negev |
| `pistolas` | solo Deagle |
| `cuchillo` | solo cuchillo |
| `zeus` | solo Zeus, con reaparición |
| `decoy` | granada señuelo y 1 de vida, con reaparición |
| `normal` | partida normal con compras libres |
| `libre` | partida libre: cualquier arma con `!armas`, reaparición, dinero infinito |
| `zombies` | oleadas de zombis cada vez más duros (ver abajo) |
| `retake` | la bomba ya está plantada en A/B; vosotros (CT) la desactiváis contra 5 bots que la defienden |
| `defensa` | la bomba ya está plantada; vosotros (T) la defendéis contra 5 bots que vienen a desactivarla |
| `dm` | deathmatch oficial |
| `awpdm` | deathmatch solo AWP |
| `dmcuchillo` | deathmatch solo cuchillo |
| `armsrace` | carrera de armas oficial (mapas `ar_*`) |
| `demolicion` | Demolición oficial: arma nueva cada ronda según tus bajas (`de_shortdust`, `de_lake`…) |
| `guardian` | Guardián oficial: defendéis la zona contra oleadas de bots (`gd_cbble`) |
| `rehenes` | rescatad a los rehenes contra 5 bots (mapas `cs_*`, por defecto `cs_office`) |
| `flyingscout` | Flying Scoutsman: SSG 08, gravedad baja y precisión total en el aire |
| `unabala` | One in the Chamber: Deagle con una bala, cualquier impacto mata, cada baja da otra bala |
| `aleatorio` | cada ronda se sortea un arma (de la AWP al Zeus) igual para todos |
| `vampiro` | AK + Deagle; el daño que haces te cura (hasta 200) |
| `jefe` | partida normal, pero un bot por ronda es un jefe gigante con 5000 de vida |
| `dmcabeza` | deathmatch solo con tiros a la cabeza |

Ajustes por equipo disponibles para los cfg o para `--set` (con `t` o `ct`):
`modo_<eq>_quitar`, `_arma`, `_pistola`, `_objeto`, `_vida`, `_velocidad`, `_color` (formato `R,G,B`).

### Zombies

Cada oleada es una ronda. Los bots zombi (T) solo llevan cuchillo y reaparecen sin parar. Vosotros (CT) tenéis dinero y munición infinitos:
- Si aguantáis el tiempo de la oleada, pasáis a la siguiente, con más zombis y más vida. Si os comen, se repite.
- Si superáis la última oleada, ganáis.
- Hay tres tipos de zombi:
  - **normal** (verde)
  - **corredor** (amarillo): rápido y débil
  - **tanque** (rojo): grande, lento y con el triple de vida; aparece desde la oleada 2
- Las balas los empujan hacia atrás (las escopetas y la AWP más), y el fuego les hace el triple de daño.
- **Infección**: si un zombi te mata, vuelves como zombi hasta la siguiente oleada.
- Se juega con niebla.

Todo se ajusta con `--set`:

| Variable | Por defecto | Qué es |
|---|---|---|
| `modo_z_oleadas` | 5 | oleadas para ganar |
| `modo_z_base` / `modo_z_incremento` | 6 / 2 | zombis en la 1.ª oleada y cuántos más en cada una |
| `modo_z_vida` / `modo_z_vida_oleada` | 250 / 75 | vida inicial y la que suma cada oleada |
| `modo_z_velocidad` | 1.2 | velocidad base |
| `modo_z_retroceso` | 1.0 | fuerza del empujón (0 = sin empujón) |
| `modo_z_infeccion` | 1 | 0 = sin infección |
| `modo_z_niebla` | 1 | 0 = sin niebla |
| `mp_roundtime` | 1.5 | minutos por oleada |

```bash
./modo.sh zombies alone --set modo_z_oleadas=10 --set modo_z_vida=400
```

### Otros ajustes de los modos

| Variable | Modo |
|---|---|
| `modo_vampiro` / `modo_vampiro_max` | fracción del daño que te cura y vida máxima |
| `modo_jefe` | vida del jefe (0 = sin jefe; se puede usar en cualquier modo) |
| `modo_unabala`, `modo_aleatorio` | 1 = activa la regla en cualquier modo |

## Mapas del workshop

1. Saca una clave de la Steam Web API en <https://steamcommunity.com/dev/apikey> y ponla en `config.env` como `WORKSHOP_AUTHKEY=`.
2. Juega cualquier modo en un mapa del workshop con su ID, que es el número de la URL del mapa:
   `./modo.sh awp alone -w 243702660`. El servidor lo descarga la primera vez.
3. Los bots solo se mueven si el mapa tiene malla de navegación (`.nav`). Si no la trae, con el mapa cargado lanza `./nav.sh`. La genera, recarga el mapa y la deja guardada.

Algunos mapas que funcionan bien con bots: **Aim Botz** (`243702660`, entrenar puntería), mapas `aim_*` / `fy_*` y versiones de los mapas oficiales.
Los mapas de surf, bhop y KZ no sirven, porque los bots no saben jugarlos.

## Comandos en el chat del juego

| Comando | Qué hace |
|---|---|
| `!ws` | skin del arma que tienes en la mano, o de cualquier otra. Las más buscadas salen primero. |
| `!knife` / `!cuchillo` | modelo de cuchillo (Karambit, Mariposa, M9…), y después su skin |
| `!armas` / `!guns` | coger cualquier arma (solo en el modo `libre`) |

Las elecciones se guardan por SteamID en SQLite (`addons/sourcemod/data/sqlite/sourcemod-local.sq3`) y se aplican solas cada vez que entras.

## Estructura

```
setup.sh                       instalación completa
arrancar.sh  modo.sh  lib.sh   CLI (opciones compartidas en lib.sh)
nav.sh                         genera el .nav del mapa actual
config.example.env             valores por defecto
cfg/                           server.cfg, modo_base.cfg y un cfg por modo
plugin/modos.sp                modos, calentamiento, ajustes por equipo, retakes
plugin/modos_zombies.sp        zombies por oleadas (incluido en modos.sp)
plugin/modos_extras.sp         una bala, aleatorio, vampiro, jefe
plugin/skins.sp                !ws, !knife, !armas + guardado
plugin/nolobbyreservation/     parche para poder conectar por IP (firmas para Linux)
tools/generar_skins.py         lista de skins desde items_game.txt
desplegar.sh  rcon.py          compilar/copiar y control remoto (`python3 rcon.py "comando"`)
```

## Notas y límites

- **NoLobbyReservation:** CS:GO Legacy rechaza conexiones directas sin este parche. Las firmas para Linux de esta versión del `engine.so` las regeneré yo. Si Valve actualizara los binarios, habría que buscarlas de nuevo. El código original es de [vanz666/NoLobbyReservation](https://github.com/vanz666/NoLobbyReservation), que no declara licencia: pide permiso antes de publicar la versión modificada.
- El servidor arranca con `-insecure` (sin VAC) y sin token GSLT. Es para jugar en privado, no para listarse en público.
- Las skins son cosméticas y solo se ven en este servidor. Usar skin changers en servidores con GSLT público va contra las normas de Valve.
- Licencia: MIT para el código propio (ver `LICENSE`), excepto `plugin/nolobbyreservation/`.
- No hay guantes: necesitan crear entidades aparte y no están implementados.
