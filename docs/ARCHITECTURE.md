# Architecture

## Layout

| Path | Role |
|---|---|
| `crates/core` | Library with all the logic: config, modes, rcon, deploy, install, server, bridge, Workshop, launching the game |
| `crates/cli` | `csgo-legacy-server`, a thin layer over `core` |
| `crates/app` | `csgo-legacy-server-app`, the Slint desktop app, also over `core` |
| `game/plugin/` | The engine's SourceMod plugins (`modos`, `csls_events`, `csls_directconnect`, `csls_admins`, `csls_anticheat`) and `include/csls.inc`, the API for hub plugins. Compiled at deploy time on the user's PC |
| `game/cfg/` | Configs every mode runs on: `modo_base`, `modo_clasico`, `modo_tipo_*` |
| `game/data/` | `pinned.toml`, the SourceMod and Metamod builds |
| `packaging/` | Desktop entry, metainfo, icons, PKGBUILDs and the package build script |
| `tools/` | Developer scripts: release versions and promotion |

Game modes, their plugins, skins and admin tools are not here: they are items of
[CSGO-Hub](https://github.com/xsteban1410/CSGO-Hub), described in [HUB.md](HUB.md).

The server runs as a `systemd --user` transient unit (`csgo-legacy-server`), so it survives closing the app. Without systemd
it falls back to a process group with a PID file.

## Layers of modes and plugins

Modes and plugins come from three places, later ones winning over earlier ones with the same id:

1. the engine (`game/`): no modes, only the engine plugins;
2. the hub (`~/.local/share/csgo-legacy-server/hub/<id>/`, `paths::hub_dir()`, or `CSLS_HUB_DIR`);
3. the user (`~/.config/csgo-legacy-server/{modes,plugins}`).

`Modes::load` reads them all and skips a broken mode with a warning instead of failing. A hub item cannot ship a plugin
with an engine plugin's name. Deploy keeps `user_plugins.txt` and `deployed_cfgs.txt` with what it put on the server, so
what was uninstalled is removed on the next deploy.

## How a mode is applied

Valve's `gamemode_*.cfg` runs first when a map loads, chosen by `game_type` and `game_mode`. Then the `modos` plugin queues,
in `OnConfigsExecuted`, in this order:

1. The `CSLS_OnModeReset` forward: each hub plugin puts its own convars back. Then `modo_base.cfg` resets the engine's
   convars and anything Valve's configs do not set.
2. `modo_clasico.cfg`, only with `game_type 0`: classic round rules.
3. `guardian_<map>.cfg`, only with `game_type 4`. Deploy copies it from `maps/cfg`, where `exec` cannot reach.
4. `modo_tipo_<type>.cfg`, for the match type chosen when the mode does not set its own `game_type`.
5. `modo_<mode>.cfg`.
6. `modo_extra`, the options of this launch: difficulty, bots, rounds, `--set`…

**Never put a convar in `modo_base.cfg` that Valve's game types rely on.** Doing so broke every official mode:
`mp_buy_anywhere 0` makes deathmatch bots stand still forever, trying to buy. The queue is only processed while the server
is not hibernating, so to test without players use `sv_hibernate_when_empty 0`.

Nothing is forced by default: `modo_base.cfg` sets `mp_humanteam any`, `bot_join_team any` and `bot_quota_mode fill`, which
counts humans in the quota, so `bot_quota 10` is a 5v5. Only modes whose rules need sides set `mp_humanteam` in their own cfg,
and `FixHumanTeam` holds those. `modo_equipos` (`--teams`) places a human once, on their first spawn on a map, and never moves
them again; `Comp_BalanceBots` moves bots to keep the teams even. `modo_humanos` is 0 unless asked for, so warmup is Valve's
timed one. `modo_clasico.cfg` sets `mp_halftime 0` and competitive modes turn it back on; `modo_veto` runs the map bans in warmup and `modo_listos` lets `!ready` end it. All of that
is in `game/plugin/modos_competitivo.sp`. `competitivo` and `premier` are `game_type 0`, `game_mode 1`; a mode's own `.cfg` still
runs last, so a mode can change any of it.

## The plugin API

`game/plugin/include/csls.inc` (`CSLS_API 1`) is what hub plugins build against. A change that breaks them raises
`CSLS_API`, and the hub index says which API each item needs.

- `CSLS_OnModeReset()`: before each map's chain of mode cfgs.
- `CSLS_OnEquip(client)`: after a player got the mode's weapons, health and speed.
- `CSLS_OnPlaceHuman(client)`: before a human is moved to the mode's side; `Plugin_Handled` keeps them.
- `CSLS_Exempt(client, seconds)`: tells the anticheat a plugin moves this player's aim or speed on purpose.

The `modo_*` convars are the rest of the API, and their names never change.

## Admins

`[[admins.users]]` and `[[admins.roles]]` in `config.toml` (`core::admins`). Deploy writes
`addons/sourcemod/configs/csls_admins.cfg` (mode 600) with each Steam account id, the flags its role gives by SteamID, and
the flags that need `sm_login`; then `sm_reloadadmins`. `csls_admins.sp` grants them in `OnClientPostAdminFilter`, reading
the account id with `GetSteamAccountID`, since SourceMod gives no SteamID on LAN. The server runs `-insecure`, where a
SteamID can be faked, so `console` and `root` (`sm_rcon`, `sm_cvar`, everything) wait for the password; three wrong ones
kick, and a second player with a connected admin's account is kicked. Permissions map to flags: `cheats` is `CUSTOM1`,
`bots` is `CUSTOM2`.

## Anticheat

`csls_anticheat.sp`, set by `[anticheat]` through `cfg/sourcemod/csls_anticheat.cfg`. It checks view angles (pitch beyond
±89 or roll), aim jumps of more than 45° in the ticks before a kill (four in 30 s), perfect bunny hops in a row, more user
commands a second than the tickrate allows (four seconds in a row), and the client cvars `r_drawothermodels`,
`mat_wireframe` and `mat_fullbright`. It skips bots, `sv_cheats 1`, players a plugin exempted and the `cheats`
permission. Each detection writes an `ac` line to the events file, which the agent stores as a sanction. A wallhack that
only draws on the client cannot be seen from the server.

## Language

`language` in `config.toml` is the only source: `csls_core::lang()` reads it once, and the system locale does not count. `deploy`
copies the skins catalog `skins.<lang>.cfg` (the copy generated from the game files in the data folder, or the one the skins hub item brings), and `modo_inicio.cfg` sets `csls_language`, which `modos.sp` applies to each client with `SetClientLanguage` so every plugin's phrases follow it.

## Deploying plugins

`deploy` compiles a plugin when the hash of its sources differs from the `<name>.smx.src` stamp written next to the `.smx`, not
by date: a packaged version and the repo have unrelated dates, and comparing them once left a new `.smx` with old phrases.
Changed phrases run `sm_reload_translations`, since a running server keeps them in memory.

## Playing over a VPN

Steam drops clients that connect to a VPN address (`STEAMAUTH`, code 10), and the server cannot accept them with `sv_lan 1`
unless they come from its LAN. So there are two bridges:

```text
game ──> friend's own LAN IP:27030 ──> friend's bridge ──VPN──> server bridge (PORT+1) ──> srcds (from the host's LAN IP)
```

The game thinks it is joining a server on its LAN, and the server sees a LAN client. Both bridges are `crates/core/src/bridge.rs`.

## Opening the game (Play)

`+connect` does not work: it runs before the engine is ready, and the game prints "Deferring connect command!" and discards
it. The `wait` command does not exist in this build, and the TCP console (`-netconport`) crashes the 64-bit Linux client.

So `steam_client::launch_csgo` starts the game with `-condebug`, watches its `console.log` for the menu to be ready, and runs
`steam://connect/IP:PORT` until the log shows the connection. It stops if the game closes, because otherwise Steam would
open CS2 to handle the URL.

## The agent

`go/cmd/agent` is the optional long-running helper, started with `agent start`. The app and the CLI work without it, with no
history or schedule. It talks JSON lines on `$XDG_RUNTIME_DIR/csgo-legacy-server/agent.sock` (folder `0700`, socket `0600`) and gets
its secrets from `internal agent-config` through a pipe, never in arguments or the environment.

- **One poll:** `status` and `modo_nombre` over one rcon connection, every 5 s with a client connected and 30 s without one. srcds writes them
  to `console.log`, so they go between the `csls_poll_begin` and `csls_poll_end` markers that the app's console hides. After a
  failure rcon rests 30 s, because the server bans an address after too many wrong passwords.
- **Finding the server:** `srcds_linux` by install folder and port, kept with its start time, so systemd and test instances are not
  confused with it and `/proc` is only walked while there is no server.
- **Logs:** `console.log` is read by offset, secrets are hidden and it is rotated by copy and cut, because srcds keeps the file open.
- **History:** player data comes from `game/plugin/csls_events.sp`, not the console, because a player could fake console lines with a
  name or in chat. Each port has its own file, so test instances do not count as play.
- **Tasks:** `schedule.json` holds a closed list of actions that run the CLI without a shell. Tasks never overlap, and runs missed
  while the agent was off are not made up.
- **Clients:** `core::agent` is the Rust client, and `agent rpc METHOD [JSON]` the same from a script. The app reads status from
  the agent when it is running and polls rcon itself only when it is not, so only one thing polls the server.

## Packages

| Package | Built by | Server after closing the app |
|---|---|---|
| Arch (`.pkg.tar.zst`) | `packaging/build-pkg.sh` | Keeps running, as a `systemd --user` unit |
| `.deb` | `packaging/build-bundles.sh` with nfpm, on Debian 12 | Keeps running |
| AppImage | `packaging/build-bundles.sh`, on Debian 12 | Keeps running |
| Flatpak | `packaging/flatpak/`, on Freedesktop 25.08 with its 32-bit extension | Stops: the sandbox has no systemd (`CSLS_NO_SYSTEMD`) |

An AppImage's mount folder goes away with the process, so nothing may keep a path into it. `cli_exe` returns the `.AppImage` file itself,
and `AppRun` starts the app with no arguments, the agent with `--agent` and the CLI otherwise. `$APPIMAGE` only counts if this process
lives inside `$APPDIR`, because a terminal opened from another AppImage passes it on, and an update must not overwrite that one.
Bundled libraries are found through the binary's rpath and not `LD_LIBRARY_PATH`, which the game and Steam would inherit.

## Rule blocks

A mode can have up to 16 rules, "when a kill happens, add 25 health". They are stored as `modo_regla_N` in the mode's cfg, reset in
`modo_base.cfg` and run by `game/plugin/modos_reglas.sp`. The events and actions are a closed catalog (`core::rules`), and
`CfgDoc::validate` checks each value, because an imported mode comes from someone else: a rule never runs code or a free command.
