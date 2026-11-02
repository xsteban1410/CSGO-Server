# CS:GO Legacy Server

[English](README.md)

Monta un servidor dedicado de **CS:GO Legacy** (app 740) y juega con amigos contra bots, desde una app GTK o desde la terminal.
Trae más de 30 modos (francotiradores, zombis por oleadas, retakes, Guardián, cheats aleatorios, anarquía…), guarda las skins
de cada jugador, compila solo los plugins de SourceMod y busca mapas en el Workshop de Steam.

Funciona en cualquier distro de 64 bits con glibc y librerías de 32 bits. Probado en Arch/CachyOS.

<p><img src="packaging/screenshots/server.png" width="49%" alt="Pestaña Servidor"> <img src="packaging/screenshots/modes.png" width="49%" alt="Modes"></p>

## Instalación

**Arch Linux (AUR):**

```bash
yay -S csgo-legacy-server        # o csgo-legacy-server-git
```

**Desde el código** (cualquier distro; necesita Rust, GTK 4 ≥ 4.14 y libadwaita ≥ 1.6 con sus archivos de desarrollo):

```bash
git clone https://github.com/xsteban1410/CSGO-Server.git && cd CSGO-Server
cargo build --release
./target/release/csgo-legacy-server-gtk       # desde el repo encuentra cfg/, modes/ y plugin/
```

Para instalarlo en otro sitio, copia `cfg/ modes/ plugin/ data/` a `<prefijo>/share/csgo-legacy-server/` junto a
`<prefijo>/bin`, o apunta `CSLS_RESOURCES` a una carpeta que los tenga.

### Primer arranque

No se descarga nada hasta que lo pidas. La app pregunta **Crear un servidor** o **Solo unirme a servidores**:

- *Crear un servidor* busca antes una instalación existente (cualquier carpeta con `srcds_run`) y la reutiliza. Si no hay,
  eliges carpeta (~40 GB libres) y descarga SteamCMD, el servidor dedicado (~33 GB), Metamod:Source y SourceMod, y compila
  los plugins. La descarga sigue en segundo plano si cierras la app y se reanuda si se corta.
- *Solo unirme* se salta todo eso: es para los amigos que solo juegan.

Sin pantalla: `csgo-legacy-server install` hace lo mismo, y `csgo-legacy-server doctor` dice qué falta y el comando
exacto para tu distro (apt, pacman, dnf, zypper, xbps, emerge).

Cada jugador necesita **CS2 con la beta `csgo_legacy`** (Steam › CS2 › Propiedades › Betas) y entra con `connect IP:PUERTO`.
El botón **Jugar** de la app (o `csgo-legacy-server play local`) abre CS:GO Legacy conectando ya a tu servidor.

## La app

| Pestaña | |
|---|---|
| Servidor | estado, dirección para conectar, Iniciar (modo → mapa → opciones → red), cambiar modo o mapa en vivo, reiniciar ronda, acabar calentamiento, añadir/quitar bots, presets |
| Jugadores | expulsar, banear (minutos o permanente, con motivo), lista de baneos y desbanear, skins guardadas de cada jugador |
| Modos | crea tus modos o duplica uno de serie; importa/exporta en `.toml` para compartirlos |
| Workshop | busca mapas sin clave de API, instala por ID o enlace (también colecciones), genera la navegación de bots, importa un `.bsp` local |
| Consola | log del servidor en vivo y comandos rcon |
| Unirse | entrar al servidor de un amigo por VPN |
| Ajustes | nombre, contraseña, perfil de red, puerto, tickrate, GSLT, plugin de skins, actualizar el servidor |

El servidor corre como unidad `systemd --user`: sigue encendido aunque cierres la app.

## Perfiles de red

| Perfil | Para qué |
|---|---|
| LAN | jugadores de tu red local (`sv_lan 1`) |
| Amigos por VPN | Tailscale, ZeroTier…: Steam echa a quien se conecta a una IP de VPN, así que el servidor sigue en modo LAN y un puente UDP en PUERTO+1 pasa a tus amigos. Cada amigo abre la pestaña **Unirse** (o lanza `csgo-legacy-server bridge 27030 <tu IP de VPN>:27031`) y se conecta a **su propia** IP local |
| Internet | puerto UDP abierto; muestra tu IP pública, te da el comando del cortafuegos (ufw/firewalld) y puede abrir el puerto del router por UPnP (`miniupnpc`) |
| Comunitario | Internet + un [token GSLT](https://steamcommunity.com/dev/managegameservers) (app 730) para salir en el navegador de servidores de Steam |

## Terminal

```
csgo-legacy-server start [modo] [alone|N] [opciones]   arrancar el servidor
csgo-legacy-server mode  <modo> [alone|N] [opciones]   cambiar de modo/mapa encendido
csgo-legacy-server stop | status | players | maps | modes
csgo-legacy-server kick <userid> | ban <userid> <minutos> [motivo] | unban <STEAM_1:x:y>
csgo-legacy-server bots +2 | bots -1 | round [--warmup]
csgo-legacy-server workshop search|install|list|remove|import
csgo-legacy-server mode-file export|import|delete      compartir modos propios
csgo-legacy-server presets [--delete nombre]           opciones de arranque guardadas
csgo-legacy-server deploy [--force] [--watch]          compilar plugins, copiar configs
csgo-legacy-server nav [--force]                       navegación de bots del mapa actual
csgo-legacy-server skins                               rehacer la lista de skins tras actualizar el juego
csgo-legacy-server rcon "comando" | play [cs2|local|IP:PUERTO] | bridge | doctor | install [--update]
```

Opciones de start/mode: `-m/--map`, `-t/--type competitive|casual|wingman`, `-d/--difficulty 0-3`, `-b/--bots`,
`-r/--rounds`, `-s/--site a|b|random`, `-p/--precision` (sin dispersión ni retroceso para todos), `--anarchy`,
`-w/--workshop ID`, `--set cvar=valor` (repetible), `--preset NOMBRE`, `--save-preset NOMBRE`.

```bash
csgo-legacy-server start awp alone
csgo-legacy-server mode retake 2 -s b -d 2 -m de_inferno
csgo-legacy-server mode zombies --set modo_z_oleadas=10 --set modo_z_vida=400
```

## Modos

Los humanos van en CT y los bots en T (5, expertos) salvo que el modo diga otra cosa.

| Modo | |
|---|---|
| `aleatorio` | **Arma aleatoria** — Cada ronda se sortea un arma igual para todos |
| `anarquia` | **Anarquía** — Todos contra todos con fuego amigo, compras en cualquier sitio y sin expulsiones ni baneos automáticos |
| `armsrace` | **Carrera de armas** — Carrera de armas oficial (mapas ar_) |
| `autosniper` | **Autosniper** — SCAR-20 + Deagle |
| `awp` | **AWP** — AWP + Deagle |
| `awpdm` | **Deathmatch AWP** — Deathmatch solo AWP |
| `cheats` | **Cheats aleatorios** — Coge power-ups por el mapa y ten un cheat al azar 30 s: wallhack, aimbot, invencible… |
| `cuchillo` | **Cuchillo** — Solo cuchillo |
| `decoy` | **Señuelo** — Granada señuelo y 1 de vida, con reaparición |
| `defensa` | **Defensa** — La bomba ya está plantada; vosotros (T) la defendéis contra 5 bots |
| `demolicion` | **Demolición** — Demolición oficial, arma nueva cada ronda |
| `dm` | **Deathmatch** — Deathmatch oficial |
| `dmcabeza` | **Deathmatch cabeza** — Deathmatch solo con tiros a la cabeza |
| `dmcuchillo` | **Deathmatch cuchillo** — Deathmatch solo cuchillo |
| `escopetas` | **Escopetas** — XM1014 + P250 |
| `flyingscout` | **Flying Scoutsman** — SSG 08, gravedad baja y precisión total en el aire |
| `guardian` | **Guardián** — Guardián oficial, defendéis la zona contra oleadas de bots |
| `headshot` | **Solo cabeza** — AK-47 + Deagle, solo daño a la cabeza |
| `jefe` | **Jefe** — Partida normal, pero un bot por ronda es un jefe gigante con 5000 de vida |
| `libre` | **Libre** — Cualquier arma con !armas, reaparición, dinero infinito |
| `luna` | **Luna** — AWP + Deagle con gravedad baja |
| `negev` | **Negev** — Solo Negev |
| `normal` | **Normal** — Partida normal, compras libres con 16000 |
| `pistolas` | **Pistolas** — Solo Deagle |
| `rehenes` | **Rehenes** — Rescatad a los rehenes contra 5 bots (mapas cs_) |
| `retake` | **Retake** — La bomba ya está plantada; vosotros (CT) la desactiváis contra 5 bots |
| `rifles` | **Rifles** — AK-47 + Deagle |
| `scout` | **Scout** — SSG 08 + P250 |
| `unabala` | **Una bala** — Deagle con una bala, cualquier impacto mata, cada baja da otra |
| `vampiro` | **Vampiro** — AK + Deagle; el daño que haces te cura (hasta 200) |
| `zeus` | **Zeus** — Solo Zeus, con reaparición |
| `zombies` | **Zombis** — Oleadas de zombis cada vez más duros |

Ajustes por equipo para tus modos o `--set` (`t` o `ct`): `modo_<eq>_quitar`, `_arma`, `_pistola`, `_objeto`,
`_vida`, `_velocidad`, `_color` (`R,G,B`).

**Zombis:** cada oleada es una ronda. Los bots zombi (T) solo llevan cuchillo y reaparecen sin parar; vosotros (CT) tenéis dinero y
munición infinitos. Si aguantáis el tiempo, pasáis a la siguiente; si os comen, se repite. Zombis normales, corredores (rápidos y
débiles) y tanques (grandes, lentos, triple de vida); las balas los empujan y el fuego los quema; quien muere vuelve como zombi; niebla.
Ajustes: `modo_z_oleadas` (5), `modo_z_base`/`modo_z_incremento` (6/2), `modo_z_vida`/`modo_z_vida_oleada` (250/75),
`modo_z_velocidad` (1.2), `modo_z_retroceso` (1.0), `modo_z_infeccion` (1), `modo_z_niebla` (1), `mp_roundtime` (1.5).

**Cheats aleatorios:** aparecen power-ups brillantes por el mapa; al cogerlos recibes un cheat al azar durante 30 s (wallhack,
aimbot, munición infinita, velocidad, gravedad baja, invencible, invisible, un tiro mata, autobhop, superpulso, regeneración).
Ajustes: `modo_cheats_duracion`, `_cantidad`, `_reaparicion`, `_bots`, `_lista`.

**Anarquía** (modo o `--anarchy` con cualquier modo): nadie es expulsado ni baneado automáticamente (inactividad, matar
aliados, votekick, flood, plazas reservadas). Los kicks y baneos que hagas desde la app siguen funcionando.

### Tus modos y plugins

- Modos: `~/.config/csgo-legacy-server/modes/<id>.toml` + `<id>.cfg` (la pestaña Modos los escribe por ti).
- Plugins: suelta un `.sp` (o `nombre/nombre.sp`) en `~/.config/csgo-legacy-server/plugins/`.

Con la app abierta (o `deploy --watch`) lo nuevo o cambiado se compila y se recarga en el servidor encendido; los errores
de compilación salen con su línea y el servidor sigue con la versión anterior. Los plugins borrados se descargan.

## Mapas del workshop

Busca en la pestaña Workshop o con `csgo-legacy-server workshop search aim`. Los mapas de CS2 (Source 2) no funcionan en
CS:GO Legacy y salen marcados como incompatibles. Los instalados van a `maps/csls/<id>/` y aparecen en el paso de mapa. Los
bots solo se mueven en mapas con malla de navegación: pulsa **Generar .nav** (o `nav`) con el mapa cargado. Los mapas de
surf, bhop y KZ no sirven para bots.

## Comandos en el chat del juego

| | |
|---|---|
| `!ws` | skin del arma que llevas en la mano (o de cualquier otra); las más buscadas salen primero |
| `!knife` / `!cuchillo` | modelo de cuchillo (Karambit, Mariposa, M9…) y su skin |
| `!armas` / `!guns` | cualquier arma (solo en el modo libre) |

Se guarda por SteamID y se aplica cada vez que entras. Los mensajes salen en el idioma del juego de cada jugador (inglés/español).

## Archivos

| | |
|---|---|
| `~/.config/csgo-legacy-server/config.toml` | ajustes (lleva la contraseña rcon, permisos 600) |
| `~/.config/csgo-legacy-server/{modes,plugins,presets.toml}` | tus modos, plugins y presets |
| `~/.local/share/csgo-legacy-server/` | servidor (ubicación por defecto), SteamCMD, logs, caché del workshop |
| `crates/core`, `crates/cli`, `crates/gtk` | Rust: núcleo, terminal y app GTK |
| `plugin/`, `cfg/`, `modes/` | plugins de SourceMod, configs y metadatos de los modos |

## Notas y límites

- `plugin/csls_directconnect` parchea el motor en memoria para que se pueda entrar por IP sin lobby de matchmaking. Sus firmas
  para Linux corresponden al `engine.so` actual; si Valve actualiza los binarios del servidor, el log dirá "signature not
  found" y habrá que volver a sacarlas. La idea viene de [NoLobbyReservation](https://github.com/vanz666/NoLobbyReservation).
- El servidor arranca con `-insecure` (sin VAC). Las skins son cosméticas y solo se ven en este servidor; los skin changers van
  contra las normas de Valve en servidores públicos con GSLT, por eso en Ajustes se puede desactivar el plugin de skins.
- No hay guantes. No probado en distros musl ni en SteamOS (sistema de solo lectura: usa distrobox).

## Licencia

GPL-3.0-or-later. Ver [LICENSE](LICENSE).
