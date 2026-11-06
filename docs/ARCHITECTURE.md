# Architecture

## Layout

| Path | Role |
|---|---|
| `crates/core` | Library with all the logic: config, modes, rcon, deploy, install, server, bridge, Workshop, launching the game |
| `crates/cli` | `csgo-legacy-server`, a thin layer over `core` |
| `crates/gtk` | `csgo-legacy-server-gtk`, the GTK 4 and libadwaita app, also over `core` |
| `plugin/` | SourceMod plugins, compiled at deploy time on the user's PC |
| `cfg/` | Server configs. `modo_<id>.cfg` is a mode; `modo_base`, `modo_clasico` and `modo_tipo_*` are helpers |
| `modes/` | Metadata of each mode: name, category, description, game type and maps |
| `data/` | Data files installed with the app |
| `packaging/` | Desktop entry, metainfo, icons, PKGBUILDs and the package build script |
| `tools/` | Developer scripts: release versions and the mode test |

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
