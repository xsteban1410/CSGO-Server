# Architecture

## Layout

| Path | Role |
|---|---|
| `crates/core` | Library with all the logic: config, modes, rcon, deploy, install, server, bridge, Workshop, launching the game |
| `crates/cli` | `csgo-legacy-server`, a thin layer over `core` |
| `crates/gtk` | `csgo-legacy-server-gtk`, the GTK 4 and libadwaita app, also over `core`. The one that ships for now |
| `crates/app` | The Slint app that will replace it, also over `core`. Not packaged yet |
| `game/plugin/` | SourceMod plugins, compiled at deploy time on the user's PC |
| `game/cfg/` | Server configs. `modo_<id>.cfg` is a mode; `modo_base`, `modo_clasico` and `modo_tipo_*` are helpers |
| `game/modes/` | Metadata of each mode: name, category, description, game type and maps |
| `game/data/` | Data files installed with the app |
| `packaging/` | Desktop entry, metainfo, icons, PKGBUILDs and the package build script |
| `tools/` | Developer scripts: release versions and promotion |

The server runs as a `systemd --user` transient unit (`csgo-legacy-server`), so it survives closing the app. Without systemd
it falls back to a process group with a PID file.

## How a mode is applied

Valve's `gamemode_*.cfg` runs first when a map loads, chosen by `game_type` and `game_mode`. Then the `modos` plugin queues,
in `OnConfigsExecuted`, in this order:

1. `modo_base.cfg` resets the plugin's convars and anything Valve's configs do not set.
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

## Language

`language` in `config.toml` is the only source: `csls_core::lang()` reads it once, and the system locale does not count. `deploy`
copies `game/data/skins.<lang>.cfg` (or the copy generated from the game files in the data folder), and `modo_inicio.cfg` sets `csls_language`, which `modos.sp` applies to each client with `SetClientLanguage` so every plugin's phrases follow it.

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

## Joining from the app

`+connect` does not work: it runs before the engine is ready, and the game prints "Deferring connect command!" and discards
it. The `wait` command does not exist in this build, and the TCP console (`-netconport`) crashes the 64-bit Linux client.

So `steam_client::launch_csgo` starts the game with `-condebug`, watches its `console.log` for the menu to be ready, and runs
`steam://connect/IP:PORT` until the log shows the connection. It stops if the game closes, because otherwise Steam would
open CS2 to handle the URL.

## The agent

`go/cmd/agent` is the optional long-running helper, started with `agent start`. The app and the CLI work without it, with no
history or schedule. It talks JSON lines on `$XDG_RUNTIME_DIR/csgo-legacy-server/agent.sock` (folder `0700`, socket `0600`) and gets
its secrets from `internal agent-config` through a pipe, never in arguments or the environment.

- **One poll:** `status` and `modo_nombre` over one rcon connection, every 5 s with a client connected and 30 s without one. After a
  failure rcon rests 30 s, because the server bans an address after too many wrong passwords.
- **Finding the server:** `srcds_linux` by install folder and port, kept with its start time, so systemd and test instances are not
  confused with it and `/proc` is only walked while there is no server.
- **Logs:** `console.log` is read by offset, secrets are hidden and it is rotated by copy and cut, because srcds keeps the file open.
- **History:** player data comes from `game/plugin/csls_events.sp`, not the console, because a player could fake console lines with a
  name or in chat. Each port has its own file, so test instances do not count as play.
- **Tasks:** `schedule.json` holds a closed list of actions that run the CLI without a shell. Tasks never overlap, and runs missed
  while the agent was off are not made up.
- **Clients:** `core::agent` is the Rust client, and `agent rpc METHOD [JSON]` the same from a script. The Slint app reads status from
  the agent when it is running and polls rcon itself only when it is not, so only one thing polls the server.

## Rule blocks

A mode can have up to 16 rules, "when a kill happens, add 25 health". They are stored as `modo_regla_N` in the mode's cfg, reset in
`modo_base.cfg` and run by `game/plugin/modos_reglas.sp`. The events and actions are a closed catalog (`core::rules`), and
`CfgDoc::validate` checks each value, because an imported mode comes from someone else: a rule never runs code or a free command.
