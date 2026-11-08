# CS:GO Legacy Server

[English](README.md) · [Changelog](CHANGELOG.md) (en inglés) · [Contribuir](CONTRIBUTING.md) (en inglés)

[![CI](https://github.com/xsteban1410/CSGO-Server/actions/workflows/ci.yml/badge.svg)](https://github.com/xsteban1410/CSGO-Server/actions/workflows/ci.yml)

Monta un servidor dedicado de **CS:GO Legacy** (app 740) y juega con amigos contra bots, desde una app de escritorio o la terminal.
Los modos de juego, plugins y herramientas de admin vienen de [CSGO-Hub](https://github.com/xsteban1410/CSGO-Hub) (francotiradores, oleadas de zombies, retakes, Guardian, cheats aleatorios, skins…) y se instalan desde la pestaña Hub de la app. Compila los plugins de SourceMod por su cuenta, tiene admins con roles y anticheat propio, y busca mapas en el Steam Workshop.

Funciona en cualquier distro de 64 bits con glibc y librerías de 32 bits. Probado en Arch/CachyOS.

<p><img src="packaging/screenshots/server.png" width="49%" alt="Server tab"> <img src="packaging/screenshots/wizard.png" width="49%" alt="Choosing a mode"></p>
<p><img src="packaging/screenshots/console.png" width="49%" alt="Console"> <img src="packaging/screenshots/modes.png" width="49%" alt="Mode editor"></p>
<p><img src="packaging/screenshots/players.png" width="49%" alt="Players"> <img src="packaging/screenshots/plugins.png" width="49%" alt="Plugins"></p>

## Instalación

**Arch Linux:** abre la [última release](https://github.com/xsteban1410/CSGO-Server/releases/latest) y ejecuta el comando
`sudo pacman -U …` de sus notas. El paquete del AUR aún no está publicado.

Los siguientes paquetes vienen en las releases posteriores a la 1.1.0:

**Debian 12 o posterior, Ubuntu 24.04 o posterior:** descarga el `.deb` de la release y ejecuta `sudo apt install ./csgo-legacy-server_*_amd64.deb`.

**Cualquier distro, AppImage:** descárgala, dale `chmod +x` y ejecútala. Necesita fontconfig, freetype y glibc 2.36 o posterior. Con argumentos funciona como la terminal: `./csgo-legacy-server-*.AppImage status`. No se añade al menú de aplicaciones.

**Cualquier distro, Flatpak:** `flatpak install --user csgo-legacy-server-*.flatpak`. Va aislado: el servidor se para al cerrar la app, y el juego lo abres tú y escribes el comando `connect` que te muestra.

Cada release trae un archivo `SHA256SUMS`: `sha256sum -c SHA256SUMS --ignore-missing`.

**Desde el código** (cualquier distro; necesita Rust, Go para el agente y los archivos de desarrollo de fontconfig, freetype, xkbcommon y Wayland):

```bash
git clone https://github.com/xsteban1410/CSGO-Server.git && cd CSGO-Server
cargo build --release
(cd go && go build -o ../target/release/csgo-legacy-server-agent ./cmd/agent)   # opcional: historial y tareas programadas
./target/release/csgo-legacy-server-app       # desde el repo: encuentra game/ aquí
```

Para instalarlo en otro sitio, copia `cfg/ modes/ plugin/ data/` de `game/` a `<prefijo>/share/csgo-legacy-server/` junto a
`<prefijo>/bin`, o apunta `CSLS_RESOURCES` a una carpeta que los tenga.

### Primer arranque

No se descarga nada hasta que lo pidas. **Crear un servidor** busca primero una instalación existente (cualquier carpeta con `srcds_run`) y la reutiliza. Si no, elige una carpeta (~40 GB libres) y descarga SteamCMD, el servidor dedicado (~33 GB), Metamod:Source y SourceMod, y luego compila los plugins. La descarga sigue en segundo plano si cierras la app y se reanuda si se corta.

La app viene sin modos de juego: la pestaña Servidor ofrece **Instalar todos los modos**, o elígelos en la pestaña **Hub** (`csgo-legacy-server hub install pack-all` desde la terminal).

Sin pantalla: `csgo-legacy-server install` hace lo mismo, y `csgo-legacy-server doctor` dice qué falta y el comando
exacto para tu distro (apt, pacman, dnf, zypper, xbps, emerge).

Cada jugador necesita **CS2 con la beta `csgo_legacy`** (Steam › CS2 › Propiedades › Betas) y entra con `connect IP:PUERTO`.
El botón **Jugar** de la app (o `csgo-legacy-server play local`) abre CS:GO Legacy conectando ya a tu servidor.

## La app

| Pestaña | |
|---|---|
| Servidor | estado, jugadores, CPU, RAM, rcon y agente, avisos y eventos recientes, dirección para conectar, Iniciar (modo → mapa → opciones → red), cambiar de modo o mapa en marcha, reiniciar la ronda, acabar el calentamiento, añadir/quitar bots, presets, el puente al server de un amigo por VPN |
| Jugadores | expulsar, banear (minutos o permanente, con motivo), lista de baneos y desbanear, admins y sus roles, skins guardadas, historial de conexiones, sanciones (también las detecciones del anticheat) con notas, exportar a CSV y JSON |
| Modos | crea tus propios modos o duplica uno instalado, con armas, vida, velocidad, tiempo de ronda, bots y una vista previa del cfg; bloques de reglas como "cuando hay una kill, suma 25 de vida"; importar/exportar como `.toml` para compartirlos |
| Hub | modos de juego, plugins, herramientas de admin y packs de CSGO-Hub, con previews: instalar con lo que necesitan, actualizar, quitar |
| Workshop | busca mapas sin clave de API, instala por ID o enlace (también colecciones), genera la navegación de los bots, importa un `.bsp` local |
| Consola | log del servidor en directo con filtros y búsqueda, comandos rcon con autocompletado, historial, favoritos y alias, guardar el log |
| Plugins | activar y desactivar, instalar `.sp` o `.smx`, recargar, volver a una versión anterior; aquí salen los errores de compilación |
| Tareas | reinicios programados, cambios de mapa o modo, comandos rcon, copias de seguridad y despliegues de plugins, con las últimas ejecuciones (necesita el agente) |
| Copias de seguridad | crear, restaurar viendo antes qué cambia, conservar las últimas N |
| Estadísticas | jugadores por día, actividad por hora, mapas y modos; se guardan solo en este PC, nunca con direcciones IP, y puedes borrarlas (necesita el agente) |
| Actualizaciones | pregunta a GitHub por una release nueva, solo cuando pulsas el botón o activas la comprobación; nada se instala solo |
| Ajustes | nombre, contraseña, perfil de red, puerto, tickrate, GSLT, plugin de skins, anticheat, actualizar el servidor |

El servidor corre como unidad `systemd --user`, así que sigue en marcha cuando cierras la app (no en el Flatpak).

Al cerrar la ventana, la app se queda en la bandeja del sistema, con estado, iniciar, parar y salir. La bandeja necesita un host StatusNotifier: GNOME necesita la extensión AppIndicator, y sin bandeja cerrar la ventana cierra la app.

## Perfiles de red

| Perfil | Para qué |
|---|---|
| LAN | jugadores de tu red local (`sv_lan 1`) |
| Amigos por VPN | Tailscale, ZeroTier…: Steam echa a los clientes que conectan a una IP de VPN, así que el servidor sigue en modo LAN y un puente UDP en PUERTO+1 reenvía a tus amigos. Cada amigo arranca el puente en la pestaña Servidor (**El server de un amigo por VPN**, o `csgo-legacy-server bridge 27030 <tu IP de VPN>:27031`) y conecta a **su propia** IP local |
| Internet | puerto UDP abierto; muestra tu IP pública, te da el comando del cortafuegos (ufw/firewalld) y puede abrir el puerto del router por UPnP (`miniupnpc`) |
| Comunitario | Internet + un [token GSLT](https://steamcommunity.com/dev/managegameservers) (app 730) para salir en el navegador de servidores de Steam |

## Terminal

```
csgo-legacy-server start [modo] [alone|N] [opciones]   arrancar el servidor
csgo-legacy-server mode  <modo> [alone|N] [opciones]   cambiar de modo/mapa en marcha
csgo-legacy-server stop | status | players | maps | modes
csgo-legacy-server kick <userid> | ban <userid> <minutos> [motivo] | unban <STEAM_1:x:y>
csgo-legacy-server bots +2 | bots -1 | round [--warmup]
csgo-legacy-server hub list|search|info|install|update|remove   modos y plugins de CSGO-Hub
csgo-legacy-server admins list|add|remove|role|password          admins y roles del juego
csgo-legacy-server workshop search|install|list|remove|import
csgo-legacy-server mode-file export|import|delete      compartir modos propios
csgo-legacy-server presets [--delete nombre]           opciones de arranque guardadas
csgo-legacy-server backup create | list | restore <archivo> [--dry-run]   tu config, modos, plugins, presets y skins
csgo-legacy-server deploy [--force] [--watch]          compilar plugins, copiar configs
csgo-legacy-server agent start | stop | status         ayudante opcional: historial y tareas programadas
csgo-legacy-server alias [nombre comando...] [--delete] atajos de consola, también para rcon
csgo-legacy-server plugins list|enable|disable|install|reload   plugins del servidor
csgo-legacy-server updates [--json]                    pregunta a GitHub por la última release
csgo-legacy-server nav [--force]                       navegación de bots del mapa actual
csgo-legacy-server skins                               rehacer la lista de skins tras actualizar el juego
csgo-legacy-server rcon "comando" | play [local|IP:PUERTO] | bridge | doctor | install [--update]
```

Opciones de start/mode: `-m/--map`, `-t/--type competitive|short|casual|wingman|dm|ffa|tdm|armsrace|demolition|guardian|dz`, `-d/--difficulty 0-3`, `-b/--bots`,
`-r/--rounds`, `-s/--site a|b|random`, `--no-bots`, `--teams versus|together|free` (modos competitivos, dónde empiezan los humanos), `--no-draw` (`competitivo` y `premier`: prórroga hasta que alguien gane), `-p/--precision` (sin dispersión ni retroceso para nadie), `--anarchy`,
`-w/--workshop ID`, `--set cvar=valor` (repetible), `--preset NOMBRE`, `--save-preset NOMBRE`.

```bash
csgo-legacy-server start awp alone
csgo-legacy-server mode retake 2 -s b -d 2 -m de_inferno
csgo-legacy-server mode zombies --set modo_z_oleadas=10 --set modo_z_vida=400
csgo-legacy-server start premier 2 --teams versus
```

## Modos

Los modos están en [CSGO-Hub](https://github.com/xsteban1410/CSGO-Hub), así que un modo se puede arreglar o añadir sin sacar una versión nueva de la app: `pack-all` los trae todos, `pack-valve` los oficiales, y cada item dice qué es y qué necesita. `csgo-legacy-server modes` lista los instalados.

Cada uno elige su equipo, como en un servidor de Valve o comunitario. Los bots (expertos) llenan los huecos y se van cuando entra un humano; `--no-bots` juega solo con humanos. El calentamiento se acaba solo, salvo que le pidas esperar a los jugadores (`start awp 2`). Solo fijan los bandos los modos cuyas reglas los necesitan.

**Partidas competitivas** (`competitivo`, `premier`, `wingman`, `duelo`) son partidas normales, no reglas propias: economía y cambio de lado en el descanso. `competitivo` y `premier` reparten a los humanos, así que dos humanos quedan uno en cada lado; `--teams` (o **Equipos** en el asistente) cambia dónde empiezan, y cualquiera puede cambiarse de equipo después. Los bots llenan los huecos. Un empate es empate, salvo que actives `--no-draw` (**Sin empate** en el asistente), que juega prórroga hasta que alguien gane. En `competitivo`, `premier` y `wingman` la partida empieza cuando todos escriben `!ready` o cuando acaba el calentamiento; `duelo` agota su calentamiento. En `premier`, los humanos vetan mapas por turnos durante el calentamiento, salvo que elijas mapa al arrancar. No tiene CS Rating ni matchmaking, que necesitan los servidores de Valve.

Ajustes por equipo para tus modos o `--set` (`t` o `ct`): `modo_<eq>_quitar`, `_arma`, `_pistola`,
`_objeto`, `_vida`, `_velocidad`, `_color` (`R,G,B`).

**Anarquía** (`--anarchy` con cualquier modo): sin expulsiones ni baneos automáticos (inactividad, matar compañeros, votación de expulsión, flood, plazas reservadas). Las expulsiones y baneos que hagas desde la app siguen funcionando.

### Admins y anticheat

Los admins son SteamIDs con un rol (pestaña Jugadores, o `csgo-legacy-server admins add STEAM_1:0:123 --name Ana --role admin`). Los roles dan `console`, `kick`, `ban`, `map`, `slay`, `chat`, `cheats`, `bots` o `root`; los de serie son `owner`, `admin`, `moderator` y `cheats`. El servidor corre con `-insecure`, donde una SteamID se puede falsificar, así que los roles con la consola del servidor (`sm_rcon`) además tienen que escribir `sm_login <contraseña>` en la consola del juego: tres contraseñas mal expulsan, y se expulsa a un segundo jugador con la SteamID de un admin. Las herramientas de admin como `!am` (aimbot) y `!wh` (wallhack) son items del hub (`pack-admin`) y necesitan el permiso `cheats`.

El anticheat (Ajustes, o `[anticheat]` en la config) busca ángulos de vista imposibles, giros bruscos de mira justo antes de una kill, bunny hops con script, speedhack y cvars de cliente de wallhack, y registra, expulsa o banea. No puede ver un wallhack que solo se dibuja en el PC del jugador. Las detecciones salen en la pestaña Jugadores, con las sanciones.

### Tus modos y plugins

- Items del hub: `~/.local/share/csgo-legacy-server/hub/<id>/`, gestionados desde la pestaña Hub o con `hub install|remove`.
- Modos: `~/.config/csgo-legacy-server/modes/<id>.toml` + `<id>.cfg` (la pestaña Modos los escribe por ti). Ganan a un modo del hub con el mismo id.
- Plugins: deja un `.sp` (o `nombre/nombre.sp`) en `~/.config/csgo-legacy-server/plugins/`, o usa `plugins install`.

Con la app abierta (o `deploy --watch`) lo nuevo o cambiado se compila y se recarga en el servidor encendido; los errores
de compilación salen con su línea y el servidor sigue con la versión anterior. Los plugins borrados se descargan.

### Copias de seguridad

`backup create` (o la pestaña **Copias de seguridad**) guarda tu config, modos, plugins, presets y skins en `~/.local/share/csgo-legacy-server/backups/`, nunca el servidor. `install --update` hace una antes y conserva las cinco automáticas más recientes. `restore --dry-run` muestra qué cambiaría, y una restauración guarda antes los archivos actuales y deja los que la copia no tiene. Las copias llevan tus contraseñas, así que no las compartas.

## Mapas del Workshop

Busca en la pestaña Workshop o con `csgo-legacy-server workshop search aim`. Los mapas de CS2 (Source 2) no funcionan en
CS:GO Legacy y salen marcados como incompatibles. Los instalados van a `maps/csls/<id>/` y aparecen en el paso de mapa. Los
bots solo se mueven en mapas con malla de navegación: pulsa **Generar .nav** (o `nav`) con el mapa cargado. Los mapas de surf, bhop y KZ no sirven para bots.

## Comandos en el chat del juego

| | |
|---|---|
| `!ready` / `!listo` | empezar la partida sin esperar a que acabe el calentamiento (modos competitivos) |
| `sm_login <contraseña>` | en la consola (~): desbloquea la consola del servidor para tu rol de admin |
| `!ws`, `!knife` | skins y cuchillos, con el item `skins` del hub |
| `!guns` / `!armas` | cualquier arma (modos de juego libre y de práctica de puntería, del hub) |
| `!am`, `!wh`, `!nc`, `!god`, `!tp`, `!bring`, `!respawn`, `!hp`, `!bots` | herramientas de admin de `pack-admin`, para roles con el permiso |

Lo que eliges se guarda por SteamID y se aplica cada vez que entras. El idioma elegido en Ajustes (inglés por defecto, o español) se aplica a la app, la terminal, los nombres de las skins y los mensajes del juego para todos los jugadores.

## Archivos

| | |
|---|---|
| `~/.config/csgo-legacy-server/config.toml` | ajustes (lleva la contraseña rcon, permisos 600) |
| `~/.config/csgo-legacy-server/{modes,plugins,presets.toml}` | tus modos, plugins y presets |
| `~/.local/share/csgo-legacy-server/` | servidor (ubicación por defecto), SteamCMD, logs, caché del Workshop |
| `crates/core`, `crates/cli`, `crates/app` | Rust: biblioteca principal, terminal, app de escritorio Slint |
| `game/plugin/`, `game/cfg/` | Los plugins de SourceMod del motor y su API (`include/csls.inc`), y las configs sobre las que corre cada modo |
| `go/` | El agente y las herramientas de pruebas |
| `docs/ARCHITECTURE.md`, `docs/HUB.md`, `docs/TROUBLESHOOTING.md`, `docs/PRIVACY.md` | Cómo encajan las piezas, el formato del hub, cómo resolver problemas y qué datos se guardan |

## Notas y límites

- Sin lingering, systemd para los servicios de usuario al cerrar sesión, y el servidor con ellos. Para que siga en marcha, ejecuta `loginctl enable-linger`; `doctor` te avisa si está desactivado.
- `doctor --report` imprime un informe para issues con contraseñas, IPs y SteamIDs ocultas, y `status --json` es para scripts.
- Los paquetes instalan el autocompletado de la shell. Desde el código: `csgo-legacy-server completions fish > ~/.config/fish/completions/csgo-legacy-server.fish` (también `bash` y `zsh`).

- `game/plugin/csls_directconnect` parchea el motor en memoria para que se pueda entrar por IP sin un lobby de matchmaking. Sus firmas
  para Linux corresponden al `engine.so` actual; si Valve actualiza los binarios del servidor, el log dirá "signature not
  found" y habrá que volver a sacarlas. La idea viene de [NoLobbyReservation](https://github.com/vanz666/NoLobbyReservation).
- El servidor arranca con `-insecure` (sin VAC). Las skins son cosméticas y solo se ven en este servidor; los skin changers incumplen
  las reglas de Valve para servidores públicos con GSLT, así que las skins son un item del hub que puedes no instalar o desactivar en Ajustes. Las herramientas de admin como
  el aimbot solo funcionan en tu propio servidor `-insecure`, para los roles a los que diste ese permiso.
- Jugado en Arch Linux y CachyOS. La CI compila y prueba todo en Debian y Fedora, y prueba el `.deb`, la AppImage y el Flatpak, pero nadie ha jugado aún con ellos. Sin probar en distros con musl ni en SteamOS (sistema de solo lectura: usa distrobox). Sin guantes.

## Ramas

`master` es la versión estable, y las releases salen de ella. Lo nuevo y las contribuciones van primero a `pre-releases`, que
pasa sola a `master` tras un día sin problemas. Los detalles están en [CONTRIBUTING.md](CONTRIBUTING.md#branches).

## Licencia

GPL-3.0-or-later. Ver [LICENSE](LICENSE).
