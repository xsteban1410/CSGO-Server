# Changelog

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The section of each version becomes the notes of its GitHub release, so keep the headings as `## [x.y.z] - date`.

## [Unreleased]

## [1.1.0] - 2026-11-06

### Added

- Eight more match types in the start wizard and in `--type`: short competitive (`short`), deathmatch (`dm`),
  free-for-all deathmatch (`ffa`), team deathmatch (`tdm`), Arms Race (`armsrace`), Demolition (`demolition`),
  Guardian (`guardian`) and Danger Zone (`dz`). The wizard only offers the ones that fit the chosen mode, and types with
  their own maps pick a suitable map.
- Join tab: **Test connection** button, the exact `connect` command to type, and the VPN bridge turns on by itself with a
  Tailscale address (100.x).
- Server tab: the address to give friends over VPN and the bridge status, with a button to start it.
- `csgo-legacy-server bridge --test IP:PORT` checks that a server answers, through a bridge too.
- `tools/probar-modos.sh` runs every mode and match type on a real server and checks bots move and respawn.
- Continuous integration builds an installable package on every push, and pushing a new version publishes the release
  by itself, with its notes taken from this file.
- Changelog, contributing guide with the sensitive areas of the code, code of conduct, security policy, support guide,
  architecture notes, and templates for issues and pull requests. Pull requests touching sensitive areas get a label and a warning.

### Changed

- The bridge logs whether the server answers, and a brief network error no longer stops it for everybody.
- Starting the server stops any bridge left by the Join tab, which could keep the port busy.
- The client now writes `csgo/console.log` when launched from the app (`-condebug`).
- `PKGBUILD` in the repository uses `SKIP` as checksum: the real one is computed when publishing to the AUR.

### Fixed

- **Play and Join never entered the server.** The game discarded `+connect` ("Deferring connect command!"). The app now
  waits for the menu and uses `steam://connect`, retrying until the game console confirms the connection.
- **Deathmatch bots stood still and nobody respawned.** `modo_base.cfg` overwrote the rules of Valve's game modes, which
  also broke Arms Race, Demolition and Guardian. Classic round rules now only apply to `game_type 0`.
- Guardian now uses Valve's per-map rules (respawn, money, round time), which the server never executed.
- Changing mode failed with "rcon: conexión perdida" and left the change half done when plugins had to be reloaded.
- Respawn waves and the "no objective" winner from Guardian no longer leak into the next mode.
- Changing mode from a Danger Zone map to a normal one no longer stays on that map.

## [1.0.2] - 2026-11-02

### Fixed

- The **Create a server** button did nothing.

## [1.0.1] - 2026-11-01

### Changed

- New icon: a classic crosshair and a server rack in Counter-Strike colors, with PNGs for each size.

## [1.0.0] - 2026-11-01

First release as a GTK app and command line in Rust, replacing the old shell scripts.

### Added

- GTK app with tabs for server, players, modes, Workshop, console, join and settings, in English and Spanish.
- Command line with the same features, plus `doctor` to find what is missing on the system.
- Guided install of SteamCMD, the dedicated server, Metamod:Source and SourceMod, which resumes if interrupted.
- More than 30 game modes, your own modes as `.toml` + `.cfg`, presets, and plugins compiled and reloaded automatically.
- Network profiles: LAN, friends over VPN with a UDP bridge, Internet with optional UPnP, and community with a GSLT token.
- Player management (kick, ban, unban), skins saved per player, and Workshop map search and install.
- `csls_directconnect`, a GPL plugin that lets players join by IP without a lobby.
- Packaging for Arch Linux and continuous integration.

### Changed

- License changed to GPL-3.0-or-later.

[Unreleased]: https://github.com/xsteban1410/CSGO-Server/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/xsteban1410/CSGO-Server/compare/v1.0.2...v1.1.0
[1.0.2]: https://github.com/xsteban1410/CSGO-Server/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/xsteban1410/CSGO-Server/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/xsteban1410/CSGO-Server/releases/tag/v1.0.0
