# CS:GO Legacy Server

[English](README.md) · [Changelog](CHANGELOG.md) (en inglés) · [Contribuir](CONTRIBUTING.md) (en inglés)

[![CI](https://github.com/xsteban1410/CSGO-Server/actions/workflows/ci.yml/badge.svg)](https://github.com/xsteban1410/CSGO-Server/actions/workflows/ci.yml)

Monta un servidor dedicado de **CS:GO Legacy** (app 740) y juega con amigos contra bots, desde una app GTK o desde la terminal.
Trae más de 30 modos (francotiradores, zombis por oleadas, retakes, Guardián, cheats aleatorios, anarquía…), guarda las skins
de cada jugador, compila solo los plugins de SourceMod y busca mapas en el Workshop de Steam.

Funciona en cualquier distro de 64 bits con glibc y librerías de 32 bits. Probado en Arch/CachyOS.

<p><img src="packaging/screenshots/server.png" width="49%" alt="Pestaña Servidor"> <img src="packaging/screenshots/modes.png" width="49%" alt="Modes"></p>

## Instalación

**Arch Linux:** abre la [última versión](https://github.com/xsteban1410/CSGO-Server/releases/latest) y ejecuta el comando
`sudo pacman -U …` de sus notas. El paquete del AUR todavía no está publicado.

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
| Unirse | entrar al servidor de un amigo; con una IP 100.x activa solo el puente VPN, prueba la conexión y da el `connect` exacto |
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
csgo-legacy-server backup create | list | restore <file> [--dry-run]   your config, modes, plugins, presets and skins
csgo-legacy-server deploy [--force] [--watch]          compilar plugins, copiar configs
csgo-legacy-server agent start | stop | status        optional helper: history and scheduled tasks
csgo-legacy-server alias [name command...] [--delete] console shortcuts, also for rcon
csgo-legacy-server plugins list|enable|disable|install|reload   plugins on the server
csgo-legacy-server updates [--json]                   ask GitHub for the latest release
csgo-legacy-server nav [--force]                       navegación de bots del mapa actual
csgo-legacy-server skins                               rehacer la lista de skins tras actualizar el juego
csgo-legacy-server rcon "comando" | play [cs2|local|IP:PUERTO] | bridge | doctor | install [--update]
```

Opciones de start/mode: `-m/--map`, `-t/--type competitive|short|casual|wingman|dm|ffa|tdm|armsrace|demolition|guardian|dz`, `-d/--difficulty 0-3`, `-b/--bots`,
`-r/--rounds`, `-s/--site a|b|random`, `--no-bots`, `--teams versus|together|free` (modos competitivos, donde comienzan los humanos), `--no-draw` (`competitivo` y `premier`: tiempo extra hasta que alguien gane), `-p/--precision` (sin propagación ni retroceso para todos), `--anarchy`,
`-w/--workshop ID`, `--set cvar=valor` (repetible), `--preset NOMBRE`, `--save-preset NOMBRE`.

```bash
csgo-legacy-server start awp alone
csgo-legacy-server mode retake 2 -s b -d 2 -m de_inferno
csgo-legacy-server mode zombies --set modo_z_oleadas=10 --set modo_z_vida=400
csgo-legacy-server start premier 2 --teams versus
```

## Modos

Cada uno elige su propio equipo, como en un servidor comunitario o de Valve. Los bots (expertos) llenan los asientos vacíos y se van cuando se une un humano; `--no-bots` solo juega con humanos. El calentamiento termina solo a menos que le pidas que espere a los jugadores (`start awp 2`). Solo los modos cuyas reglas necesitan que los lados las arreglen: `retake`, `defensa`, `clutch`, `zombies`, `ninjas`, `aim` y Guardian.

| Modo | |
|---|---|
| `aim` | **Práctica de puntería**: los robots se quedan quietos y no disparan, tienes munición y armas infinitas. Calienta tu puntería |
| `aleatorio` | **Arma aleatoria**: en cada ronda, todos obtienen la misma arma aleatoria, desde AWP hasta Zeus. |
| `anarquia` | **Anarquía** — Caos: todos contra todos con fuego amigo, reaparece, compra en cualquier lugar y nadie es expulsado |
| `armsrace` | **Carrera armamentista** — Oficial: cada muerte te otorga la siguiente arma, el primero en conseguir el cuchillo dorado gana. Sus propios mapas ar_ |
| `autosniper` | **Auto-francotirador**: todos obtienen un SCAR-20 y un Deagle. Rondas cortas, sin compras |
| `awp` | **AWP**: todos obtienen un AWP y un Deagle. Rondas cortas, sin compras |
| `awpdm` | **AWP deathmatch** — Deathmatch solo con AWP y Deagle |
| `cheats` | **Trucos aleatorios**: toma potenciadores brillantes para realizar un truco aleatorio durante 30 s: wallhack, aimbot, modo dios... |
| `clutch` | **Clutch**: cada ronda que ganas agrega un bot, cada ronda que pierdes elimina uno. ¿A cuántos puedes vencer? |
| `competitivo` | **Competitivo** — Lo real: 5 contra 5 con economía, primero a 16, los lados se intercambian en el medio tiempo y en la prórroga. Los bots llenan los asientos vacíos |
| `cuchillo` | **Cuchillos**: solo cuchillo. Rondas cortas |
| `dangerzone` | **Zona de peligro** — Batalla real oficial en mapas dz_: encuentra equipo, evita la tormenta, sé el último en pie |
| `decoy` | **Decoy** — Combate a muerte con 1 HP y señuelos infinitos: un golpe directo mata |
| `defensa` | **Post-plantación** — Lo opuesto a Retomar: la bomba se coloca y tú (T) la defiendes de 5 robots que vienen a desactivarla. |
| `demolicion` | **Demolición** — Oficial: rondas de bombas donde obtienes una nueva arma en cada ronda por tus muertes. Pequeños mapas |
| `dm` | **Deathmatch** — Deathmatch oficial: compra desde el menú, reaparece de inmediato, 10 minutos |
| `dmcabeza` | **Combate a muerte con disparos en la cabeza**: combate a muerte donde solo los disparos en la cabeza causan daño |
| `dmcuchillo` | **Deathmatch cuchillo** — Deathmatch solo cuchillo |
| `duelo` | **Duelo**: tú contra tu amigo, sin robots. La misma arma aleatoria en cada ronda, el primero en llegar a 10 victorias. |
| `escopetas` | **Escopetas**: todos obtienen una XM1014 y una P250. Rondas cortas, sin compras |
| `ffa` | **Todos contra todos**: combate a muerte donde todos son tus enemigos y tus amigos también |
| `flyingscout` | **Flying Scoutsman** — Juego de guerra oficial: SSG 08, gravedad muy baja, saltos de conejo y precisión perfecta en el aire |
| `guardian` | **Guardian** — Cooperativo oficial: defiende la zona contra oleadas de robots, en mapas gd_ |
| `hardcore` | **Hardcore** — Reglas competitivas sin radar: las balas impactan un 60 % más fuerte y el fuego amigo causa todo el daño |
| `headshot` | **Solo disparos a la cabeza** — AK-47 y Deagle, y solo los disparos a la cabeza causan daño |
| `jefe` | **Jefe**: Rondas normales, pero un robot en cada ronda es un gigante rojo lento con 5000 HP. |
| `libre` | **Juego libre** - Sin reglas: reaparece, compra en cualquier lugar y cualquier arma con !guns. Bueno para probar cosas. |
| `luna` | **Moon** — AWP y Deagle con gravedad muy baja, como en la Luna |
| `negev` | **Negev** — Todo el mundo recibe un Néguev y nada más. Rondas cortas, sin compras |
| `ninjas` | **Ninjas**: Bots rápidos, casi invisibles y con cuchillos te cazan. Compra lo que quieras y escúchalos. |
| `normal` | **Normal** — Rondas con $16000 en cada ronda, por lo que siempre puedes comprar cualquier cosa |
| `pesados` | **Armadura pesada** — Todos con un traje de asalto pesado y un M249: resistente y lento |
| `pistolas` | **Deagle**: todos reciben solo un Deagle. Rondas cortas, sin compras |
| `premier` | **Premier** — Reglas de Premier: 5 contra 5, primero en llegar al 13, tiempo extra y prohibiciones de mapas en el calentamiento. Sin clasificación CS, necesita los servidores de Valve |
| `rehenes` | **Rehenes** — Rescata o protege a los rehenes, en cs_maps |
| `retake` | **Retomar** — La bomba ya está colocada y tú (CT) debes desactivarla contra 5 robots que la custodian. |
| `rifles` | **Rifles**: todos reciben un AK-47 y un Deagle. Rondas cortas, sin compras |
| `scout` | **Scout**: todos obtienen un SSG 08 y un P250. Rondas cortas, sin compras |
| `stabzap` | **Stab Stab Zap** — Juego de guerra oficial: solo un cuchillo y un Zeus que se recarga en 30 s |
| `tdm` | **Combate a muerte en equipo**: combate a muerte donde las muertes puntúan para tu equipo y el mejor equipo gana |
| `trigger` | **Disciplina de disparo** — Juego de guerra oficial: cada disparo fallido te lastima, pero nunca te mata |
| `unabala` | **Uno en la Cámara** — Combate a muerte con un Deagle y una bala. Cualquier golpe mata, y cada muerte te da una bala. |
| `vampiro` | **Vampiro** — AK-47 y Deagle, y la mitad del daño que infliges te cura (hasta 200 HP) |
| `wingman` | **Wingman**: 2 contra 2 oficial en un sitio de bomba, dieciséis rondas. Los mapas pequeños funcionan mejor |
| `zeus` | **Zeus** — Combate a muerte con un Zeus que nunca se agota |
| `zombies` | **Zombies**: sobrevive a cinco oleadas de robots zombies con todas las armas y munición infinita. Si mueres, te vuelves |

**Partidos competitivos** (`competitivo`, `premier`, `wingman`, `duelo`) son partidos normales, no reglas personalizadas: economía e intercambio de lados en el medio tiempo. `competitivo` y `premier` inician a los humanos equilibrados, por lo que dos humanos obtienen un lado cada uno; `--teams` (o **Equipos** en el asistente) cambia el lugar donde comienzan y cualquiera puede cambiar de equipo después. Los robots llenan los asientos vacíos. Un empate es un empate, a menos que actives `--no-draw` (**No hay empates** en el asistente), que juega tiempo extra hasta que alguien gana. En `competitivo`, `premier` y `wingman` el partido comienza cuando todos escriben `!ready` o cuando finaliza el calentamiento; `duelo` termina su calentamiento. En `premier`, los humanos se turnan para prohibir mapas durante el calentamiento, a menos que elijas un mapa al comenzar. No tiene clasificación CS ni emparejamiento, que necesitan los servidores de Valve.

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
- Complementos: coloque un `.sp` (o `name/name.sp`) en `~/.config/csgo-legacy-server/plugins/`, o use `plugins install`.

Con la app abierta (o `deploy --watch`) lo nuevo o cambiado se compila y se recarga en el servidor encendido; los errores
de compilación salen con su línea y el servidor sigue con la versión anterior. Los plugins borrados se descargan.

### Copias de seguridad

`backup create` (o **Configuración → Copias de seguridad**) guarda su configuración, modos, complementos, ajustes preestablecidos y máscaras en `~/.local/share/csgo-legacy-server/backups/`, nunca en el servidor. `install --update` hace uno primero y se queda con los últimos cinco automáticos. `restore --dry-run` muestra lo que cambiaría, y una restauración guarda primero los archivos actuales y deja los archivos que la copia de seguridad no tiene. Las copias de seguridad contienen sus contraseñas, así que no las comparta.

## Mapas del workshop

Busca en la pestaña Workshop o con `csgo-legacy-server workshop search aim`. Los mapas de CS2 (Source 2) no funcionan en
CS:GO Legacy y salen marcados como incompatibles. Los instalados van a `maps/csls/<id>/` y aparecen en el paso de mapa. Los
bots solo se mueven en mapas con malla de navegación: pulsa **Generar .nav** (o `nav`) con el mapa cargado. Los mapas de
surf, bhop y KZ no sirven para bots.

## Comandos en el chat del juego

| | |
|---|---|
| `!ws` | skin del arma que llevas en la mano (o de cualquier otra); las más buscadas salen primero |
| `!guns` / `!armas` | cualquier arma (solo en los modos de juego libre y práctica de puntería) |
| `!ready` / `!listo` | comenzar el partido sin esperar a que termine el calentamiento (modos competitivos) |
| `!armas` / `!guns` | cualquier arma (solo en el modo libre) |
Las opciones se guardan por SteamID y se aplican cada vez que te unes. El idioma elegido en Configuración (inglés por defecto o español) se aplica a la aplicación, la línea de comando, los nombres de los skins y los mensajes del juego para cada jugador.
Se guarda por SteamID y se aplica cada vez que entras. Los mensajes salen en el idioma del juego de cada jugador (inglés/español).

## Archivos

| | |
|---|---|
| `~/.config/csgo-legacy-server/config.toml` | ajustes (lleva la contraseña rcon, permisos 600) |
| `~/.config/csgo-legacy-server/{modes,plugins,presets.toml}` | tus modos, plugins y presets |
| `crates/core`, `crates/cli`, `crates/gtk`, `crates/app` | Rust: biblioteca principal, línea de comandos, aplicación GTK y la aplicación Slint que la reemplazará |
| `game/plugin/`, `game/cfg/`, `game/modes/` | Complementos de SourceMod, configuraciones de modo, metadatos de modo |
| `go/` | El agente y las herramientas de prueba. |
| `docs/ARCHITECTURE.md`, `docs/TROUBLESHOOTING.md`, `docs/PRIVACY.md` | Cómo encajan las piezas, cómo solucionar problemas y qué datos se conservan |

## Notas y límites

- Sin demorarse, systemd detiene los servicios del usuario cuando cierra la sesión, y el servidor con ellos. Para que siga funcionando, ejecute `loginctl enable-linger`; `doctor` te indica si está apagado.
- `doctor --report` imprime un informe sobre problemas con contraseñas, IP e ID de Steam ocultos, y `status --json` está destinado a scripts.
- Los paquetes instalan terminaciones de shell. De la fuente: `csgo-legacy-server completions fish > ~/.config/fish/completions/csgo-legacy-server.fish` (también `bash` y `zsh`).

- `game/plugin/csls_directconnect` parchea el motor en la memoria para que los jugadores puedan unirse por IP sin un lobby de emparejamiento. Es Linux
  para Linux corresponden al `engine.so` actual; si Valve actualiza los binarios del servidor, el log dirá "signature not
  found" y habrá que volver a sacarlas. La idea viene de [NoLobbyReservation](https://github.com/vanz666/NoLobbyReservation).
- El servidor arranca con `-insecure` (sin VAC). Las skins son cosméticas y solo se ven en este servidor; los skin changers van
  contra las normas de Valve en servidores públicos con GSLT, por eso en Ajustes se puede desactivar el plugin de skins.
- Probado en Arch Linux y CachyOS. CI construye el núcleo y la CLI en Debian y Fedora, pero no se ha probado nada más allí. No probado en distribuciones musl o SteamOS (sistema de solo lectura: use distrobox). Sin guantes.

## Ramas

`master` es la versión estable, y las releases salen de ella. Lo nuevo y las contribuciones van primero a `pre-releases`, que
pasa sola a `master` tras tres días sin problemas. Los detalles están en [CONTRIBUTING.md](CONTRIBUTING.md#branches).

## Licencia

GPL-3.0-or-later. Ver [LICENSE](LICENSE).
