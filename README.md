# CS:GO Legacy Server

[Español](README.es.md) · [Changelog](CHANGELOG.md) · [Contributing](CONTRIBUTING.md)

[![CI](https://github.com/xsteban1410/CSGO-Server/actions/workflows/ci.yml/badge.svg)](https://github.com/xsteban1410/CSGO-Server/actions/workflows/ci.yml)

Host a **CS:GO Legacy** dedicated server (app 740) and play with friends against bots, from a desktop app or the command line.
Game modes, plugins and admin tools come from [CSGO-Hub](https://github.com/xsteban1410/CSGO-Hub) (snipers, zombie waves,
retakes, Guardian, random cheats, skins…), installed from the app's Hub tab. It compiles SourceMod plugins by itself, has
admins with roles and its own anticheat, and browses the Steam Workshop for maps.

Works on any 64-bit glibc distro with 32-bit libraries. Tested on Arch/CachyOS.

<p><img src="packaging/screenshots/server.png" width="49%" alt="Server tab"> <img src="packaging/screenshots/wizard.png" width="49%" alt="Choosing a mode"></p>
<p><img src="packaging/screenshots/console.png" width="49%" alt="Console"> <img src="packaging/screenshots/modes.png" width="49%" alt="Mode editor"></p>
<p><img src="packaging/screenshots/players.png" width="49%" alt="Players"> <img src="packaging/screenshots/plugins.png" width="49%" alt="Plugins"></p>

## Install

**Arch Linux:** open the [latest release](https://github.com/xsteban1410/CSGO-Server/releases/latest) and run the
`sudo pacman -U …` command from its notes. The AUR package is not published yet.

The next packages come with releases after 1.1.0:

**Debian 12 or newer, Ubuntu 24.04 or newer:** download the `.deb` from the release and run `sudo apt install ./csgo-legacy-server_*_amd64.deb`.

**Any distro, AppImage:** download it, `chmod +x` it and run it. It needs fontconfig, freetype and glibc 2.36 or newer. With arguments it is the command line: `./csgo-legacy-server-*.AppImage status`. It does not add itself to your app menu.

**Any distro, Flatpak:** `flatpak install --user csgo-legacy-server-*.flatpak`. It is sandboxed: the server stops when you close the app, and you open the game yourself and type the `connect` command it shows.

Each release has a `SHA256SUMS` file: `sha256sum -c SHA256SUMS --ignore-missing`.

**From source** (any distro; needs Rust, Go for the agent, and the fontconfig, freetype, xkbcommon and Wayland development files):

```bash
git clone https://github.com/xsteban1410/CSGO-Server.git && cd CSGO-Server
cargo build --release
(cd go && go build -o ../target/release/csgo-legacy-server-agent ./cmd/agent)   # optional: history and scheduled tasks
./target/release/csgo-legacy-server-app       # run from the repo: it finds game/ here
```

To install elsewhere, copy `cfg/ modes/ plugin/ data/` from `game/` to `<prefix>/share/csgo-legacy-server/` next to
`<prefix>/bin`, or point `CSLS_RESOURCES` at a folder that has them.

### First run

Nothing is downloaded until you ask. **Create a server** looks for an existing install first (any folder with `srcds_run`)
and reuses it. Otherwise pick a folder (~40 GB free), and it downloads SteamCMD, the dedicated server (~33 GB),
Metamod:Source and SourceMod, then compiles the plugins. The download keeps going in the background if you close the app,
and resumes if it is interrupted.

The app comes without game modes: the Server tab offers **Install every mode**, or pick them in the **Hub** tab
(`csgo-legacy-server hub install pack-all` from the command line).

Headless: `csgo-legacy-server install` does the same, and `csgo-legacy-server doctor` tells you what is missing and the
exact command for your distro (apt, pacman, dnf, zypper, xbps, emerge).

Players need **CS2 with the `csgo_legacy` beta** (Steam › CS2 › Properties › Betas) and join with `connect IP:PORT`.
The app's **Play** button (or `csgo-legacy-server play local`) opens CS:GO Legacy already connecting to your server.

## The app

| Tab | |
|---|---|
| Server | status, players, CPU, RAM, rcon and agent, warnings and recent events, connect address, Start (mode → map → options → network), change mode or map live, restart round, end warmup, add/remove bots, presets, the bridge to a friend's server over VPN |
| Players | kick, ban (minutes or permanent, with reason), bans list and unban, admins and their roles, saved skins, connection history, sanctions (anticheat detections too) with notes, export to CSV and JSON |
| Modes | make your own modes or duplicate an installed one, with weapons, health, speed, round time, bots and a preview of the cfg; rule blocks such as "when a kill happens, add 25 health"; import/export as `.toml` to share them |
| Hub | game modes, plugins, admin tools and packs from CSGO-Hub, with previews: install with what they need, update, remove |
| Workshop | search maps without an API key, install by ID or link (collections too), generate bot navigation, import a local `.bsp` |
| Console | live server log with filters and search, rcon commands with autocomplete, history, favorites and aliases, save the log |
| Plugins | turn on and off, install `.sp` or `.smx`, reload, go back to an earlier version; compile errors are shown here |
| Tasks | scheduled restarts, map or mode changes, rcon commands, backups and plugin deploys, with recent runs (needs the agent) |
| Backups | create, restore with a preview of what changes, keep the last N |
| Statistics | players per day, activity by hour, maps and modes; stored only on this PC, never IP addresses, and you can erase it (needs the agent) |
| Updates | ask GitHub for a new release, only when you press it or turn the check on; nothing installs by itself |
| Settings | name, password, network profile, port, tickrate, GSLT, skins plugin, anticheat, update the server |

The server runs as a `systemd --user` unit, so it keeps running when you close the app (not in the Flatpak).

Closing the window leaves the app in the system tray, with status, start, stop and quit. The tray needs a StatusNotifier host: GNOME needs the AppIndicator extension, and without a tray closing the window quits the app.

## Network profiles

| Profile | Use it for |
|---|---|
| LAN | players on your local network (`sv_lan 1`) |
| Friends over VPN | Tailscale, ZeroTier…: Steam drops clients that connect to a VPN IP, so the server stays in LAN mode and a UDP bridge on PORT+1 forwards your friends. Each friend starts the bridge in the Server tab (**A friend's server over VPN**, or `csgo-legacy-server bridge 27030 <your VPN IP>:27031`) and connects to **their own** local IP |
| Internet | open UDP port; shows your public IP, tells you the firewall command (ufw/firewalld) and can open the router port with UPnP (`miniupnpc`) |
| Community | Internet + a [GSLT token](https://steamcommunity.com/dev/managegameservers) (app 730) so the server shows up in Steam's server browser |

## Command line

```
csgo-legacy-server start [mode] [alone|N] [options]   start the server
csgo-legacy-server mode  <mode> [alone|N] [options]   change mode/map while running
csgo-legacy-server stop | status | players | maps | modes
csgo-legacy-server kick <userid> | ban <userid> <minutes> [reason] | unban <STEAM_1:x:y>
csgo-legacy-server bots +2 | bots -1 | round [--warmup]
csgo-legacy-server hub list|search|info|install|update|remove   modes and plugins from CSGO-Hub
csgo-legacy-server admins list|add|remove|role|password          admins and roles for the game
csgo-legacy-server workshop search|install|list|remove|import
csgo-legacy-server mode-file export|import|delete     share custom modes
csgo-legacy-server presets [--delete name]            saved start options
csgo-legacy-server backup create | list | restore <file> [--dry-run]   your config, modes, plugins, presets and skins
csgo-legacy-server deploy [--force] [--watch]         compile plugins, copy configs
csgo-legacy-server agent start | stop | status        optional helper: history and scheduled tasks
csgo-legacy-server alias [name command...] [--delete] console shortcuts, also for rcon
csgo-legacy-server plugins list|enable|disable|install|reload   plugins on the server
csgo-legacy-server updates [--json]                   ask GitHub for the latest release
csgo-legacy-server nav [--force]                      bot navigation for the current map
csgo-legacy-server skins                              rebuild the skins list after a game update
csgo-legacy-server rcon "command" | play [local|IP:PORT] | bridge | doctor | install [--update]
```

Start/mode options: `-m/--map`, `-t/--type competitive|short|casual|wingman|dm|ffa|tdm|armsrace|demolition|guardian|dz`, `-d/--difficulty 0-3`, `-b/--bots`,
`-r/--rounds`, `-s/--site a|b|random`, `--no-bots`, `--teams versus|together|free` (competitive modes, where humans start), `--no-draw` (`competitivo` and `premier`: overtime until someone wins), `-p/--precision` (no spread or recoil for everyone), `--anarchy`,
`-w/--workshop ID`, `--set cvar=value` (repeatable), `--preset NAME`, `--save-preset NAME`.

```bash
csgo-legacy-server start awp alone
csgo-legacy-server mode retake 2 -s b -d 2 -m de_inferno
csgo-legacy-server mode zombies --set modo_z_oleadas=10 --set modo_z_vida=400
csgo-legacy-server start premier 2 --teams versus
```

## Modes

Modes live in [CSGO-Hub](https://github.com/xsteban1410/CSGO-Hub), so a mode can be fixed or added without a new
version of the app: `pack-all` has all of them, `pack-valve` the official ones, and each item says what it is and what it
needs. `csgo-legacy-server modes` lists the installed ones.

Everyone picks their own team, as on a Valve or community server. Bots (expert) fill the empty seats and leave when a human joins; `--no-bots` plays humans only. Warmup runs out by itself unless you ask it to wait for players (`start awp 2`). Only modes whose rules need sides fix them.

**Competitive matches** (`competitivo`, `premier`, `wingman`, `duelo`) are normal matches, not custom rules: economy and side swap at half time. `competitivo` and `premier` start humans balanced, so two humans get one side each; `--teams` (or **Teams** in the wizard) changes where they start, and anyone can switch team afterwards. Bots fill the empty seats. A tie is a draw, unless you turn on `--no-draw` (**No draws** in the wizard), which plays overtime until someone wins. In `competitivo`, `premier` and `wingman` the match starts when everyone types `!ready` or when the warmup ends; `duelo` runs its warmup out. In `premier`, humans take turns banning maps during warmup unless you choose a map when you start. It has no CS Rating or matchmaking, which need Valve's servers.

Per-team settings for your own modes or `--set` (`t` or `ct`): `modo_<team>_quitar`, `_arma`, `_pistola`,
`_objeto`, `_vida`, `_velocidad`, `_color` (`R,G,B`).

**Anarchy** (`--anarchy` with any mode): no automatic kicks or bans (idle, team kills, vote kick, flood,
reserved slots). Kicks and bans you make from the app still work.

### Admins and anticheat

Admins are SteamIDs with a role (Players tab, or `csgo-legacy-server admins add STEAM_1:0:123 --name Ana --role admin`).
Roles grant `console`, `kick`, `ban`, `map`, `slay`, `chat`, `cheats`, `bots` or `root`; the defaults are `owner`, `admin`,
`moderator` and `cheats`. The server runs with `-insecure`, where a SteamID can be faked, so the roles with the server
console (`sm_rcon`) also need `sm_login <password>` typed in the game console: three wrong passwords kick, and a second
player with an admin's SteamID is kicked. Admin tools such as `!am` (aimbot) and `!wh` (wallhack) are hub items
(`pack-admin`) and need the `cheats` permission.

The anticheat (Settings, or `[anticheat]` in the config) looks for impossible view angles, aim snaps right before
kills, scripted bunny hops, speedhack and wallhack client cvars, and logs, kicks or bans. It cannot see a wallhack that
only draws on the player's PC. Detections show in the Players tab with the sanctions.

### Your own modes and plugins

- Hub items: `~/.local/share/csgo-legacy-server/hub/<id>/`, managed by the Hub tab or `hub install|remove`.
- Modes: `~/.config/csgo-legacy-server/modes/<id>.toml` + `<id>.cfg` (the Modes tab writes them for you). They win over a hub mode with the same id.
- Plugins: drop a `.sp` (or `name/name.sp`) into `~/.config/csgo-legacy-server/plugins/`, or use `plugins install`.

With the app open (or `deploy --watch`) new or changed files are compiled and reloaded in the running server; compile
errors are shown with their line and the server keeps the previous version. Deleted plugins are unloaded.

### Backups

`backup create` (or the **Backups** tab) saves your config, modes, plugins, presets and skins in `~/.local/share/csgo-legacy-server/backups/`, never the server. `install --update` makes one first and keeps the last five automatic ones. `restore --dry-run` shows what would change, and a restore saves the current files first and leaves files the backup does not have. Backups hold your passwords, so do not share them.

## Workshop maps

Search in the Workshop tab or `csgo-legacy-server workshop search aim`. CS2 maps (Source 2) cannot run on CS:GO Legacy and are
marked as incompatible. Installed maps live in `maps/csls/<id>/` and show up in the map step. Bots only move on maps with a
navigation mesh: press **Generate .nav** (or `nav`) with the map loaded. Surf, bhop and KZ maps are no good for bots.

## In-game chat commands

| | |
|---|---|
| `!ready` / `!listo` | start the match without waiting for the warmup to end (competitive modes) |
| `sm_login <password>` | in the console (~): unlock the server console for your admin role |
| `!ws`, `!knife` | skins and knives, with the `skins` hub item |
| `!guns` / `!armas` | any weapon (free play and aim practice modes, from the hub) |
| `!am`, `!wh`, `!nc`, `!god`, `!tp`, `!bring`, `!respawn`, `!hp`, `!bots` | admin tools from `pack-admin`, for roles with the permission |

Choices are saved per SteamID and applied every time you join. The language chosen in Settings (English by default, or Spanish) applies to the app, the command line, skin names and in-game messages for every player.

## Files

| | |
|---|---|
| `~/.config/csgo-legacy-server/config.toml` | settings (rcon password inside, mode 600) |
| `~/.config/csgo-legacy-server/{modes,plugins,presets.toml}` | your modes, plugins and presets |
| `~/.local/share/csgo-legacy-server/` | server (default location), SteamCMD, logs, workshop cache |
| `crates/core`, `crates/cli`, `crates/app` | Rust: core library, command line, Slint desktop app |
| `game/plugin/`, `game/cfg/` | The engine's SourceMod plugins and API (`include/csls.inc`), and the configs every mode runs on |
| `go/` | The agent and the test tools |
| `docs/ARCHITECTURE.md`, `docs/HUB.md`, `docs/TROUBLESHOOTING.md`, `docs/PRIVACY.md` | How the pieces fit together, the hub format, fixing problems, and what data is kept |

## Notes and limits

- Without lingering, systemd stops user services when you log out, and the server with them. To keep it running, run `loginctl enable-linger`; `doctor` tells you if it is off.
- `doctor --report` prints a report for issues with passwords, IPs and SteamIDs hidden, and `status --json` is meant for scripts.
- The packages install shell completions. From source: `csgo-legacy-server completions fish > ~/.config/fish/completions/csgo-legacy-server.fish` (also `bash` and `zsh`).

- `game/plugin/csls_directconnect` patches the engine in memory so players can join by IP without a matchmaking lobby. Its Linux
  signatures match the current `engine.so`; if Valve updates the server binaries it will log "signature not found" and they
  must be found again. The idea comes from [NoLobbyReservation](https://github.com/vanz666/NoLobbyReservation).
- The server runs with `-insecure` (no VAC). Skins are cosmetic and only visible on this server; skin changers break Valve's
  rules for public GSLT servers, so skins are a hub item you can leave out or turn off in Settings. Admin tools such as
  the aimbot only work on your own `-insecure` server, for roles you gave that permission.
- Played on Arch Linux and CachyOS. CI builds and tests everything on Debian and Fedora, and tries the `.deb`, AppImage and Flatpak, but nobody has played on them yet. Not tested on musl distros or SteamOS (read-only system: use distrobox). No gloves.

## Branches

`master` is the stable version, and releases are built from it. New work and contributions go to `pre-releases` first, which
reaches `master` by itself after three days without problems. Details are in [CONTRIBUTING.md](CONTRIBUTING.md#branches).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
