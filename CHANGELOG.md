# Changelog

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The section of each version becomes the notes of its GitHub release, so keep the headings as `## [x.y.z] - date`.

## [Unreleased]

### Changed

- `tools/test-modes.sh` turns off hibernation by itself and polls instead of waiting, so all modes take about 12 minutes instead of about an hour.
- All code comments, messages, logs, tests and documentation are now in English. The app, plugin messages and `README.es.md`
  are still translated into Spanish, but errors coming from the core now always appear in English.
- `tools/probar-modos.sh` is now `tools/test-modes.sh`, and `modo_estado` prints `team=`, `health=`, `speed=`, `scale=` and `type=`.
- Dependabot Rust updates go to `pre-releases`.
- Every mode description was rewritten so it says what you actually play, and the wizard groups modes by category with rarely used options under **Advanced**.
- Weapon and round modes have a short freeze time, no friendly fire and no money rewards, since there is nothing to buy.
- `zeus`, `decoy` and `unabala` are now deathmatch modes, with real scoring; Zeus and decoys never run out.
- `pistolas` is now called Deagle, which is what it gives you.
- `guardian` no longer uses `exec`, so it can be duplicated in the Modes tab.

### Added

- Shell completions for bash, zsh and fish: `completions <shell>`, installed by the packages.
- `docs/TROUBLESHOOTING.md`, and `docs/PRIVACY.md` with what is stored and what the app contacts.
- CI builds and tests the core and CLI on Debian and Fedora, audits dependencies and fails on clippy warnings.
- **Language** in Settings (`language` in `config.toml`): English by default, Spanish if you choose it. It no longer depends on the
  system locale, and it covers the app, the command line, skin names and in-game messages for every player. Install progress and
  common errors are translated too.
- Backups: `backup create|list|restore [--dry-run]` and **Settings → Backups** save your config, modes, plugins, presets and skins. `install --update` makes one first.
- `doctor` also checks unfinished installs, stale plugins, SourceMod errors, config permissions and whether the server survives logging out. **Settings → Diagnosis** shows the same. `doctor --report` hides private data, and `status --json` is for scripts.
- The Server tab shows CPU, RAM, uptime and the last SourceMod error, and the app tells you if the server stops on its own. The install shows its download speed.
- Changing only one README translates the changed lines into the other, with Google Translate.
- `.claude/` holds rules for contributors who use Claude Code.
- Competitive category with normal matches instead of custom rules: `competitivo` (MR15), `premier` (MR12 with map bans), `wingman`
  (2v2) and `duelo` (1v1 without bots). They have economy, half time and overtime, and bots fill the empty seats.
- `--teams together|versus|free` and **Teams** in the wizard, `!ready`/`!listo` to end the warmup, and map bans in `premier`.
- New modes: `aim`, `hardcore`, `pesados`, `ffa`, `tdm`, `dangerzone`, `trigger`, `stabzap`, `clutch` and `ninjas`.
- `modo_<team>_color` accepts `R,G,B,alpha` for see-through players.

### Changed (install)

- SourceMod and Metamod are pinned to the tested builds and checked with SHA-256 before unpacking, instead of taking the latest.
- An interrupted install or update is detected: the server will not start until `install` resumes it. Two installs cannot run at
  once, the server must be stopped first, and downloads retry with growing waits.

### Security

- The rcon password, GSLT token and workshop key are no longer process arguments, which anyone on the PC could read with `ps`.
  They live in files only you can read, and so does `server.cfg`, which holds the server password.
- Internet and community profiles warn at start that the server is reachable from the internet, has no VAC and whether it has a password.
- `doctor` warns if rcon can be reached from other machines, and a GSLT with quotes or `;` is rejected.

### Fixed

- Skins and knives did nothing when the installed package and the repo were mixed: `deploy` compared file dates, kept an old
  `.smx` with newer phrases, and the menus failed with `Language phrase not found`. It now compares a hash of the sources and
  reloads changed phrases in the running server.
- The skin menus no longer stop opening after an error, and changing a skin keeps your ammo and the weapon in your hand.
- Paints now show right after choosing them: each one gets a new item id.
- Changing mode on a running server no longer returns to the first mode after a hot reload.
- Half time no longer breaks the match in modes that force the humans' team.
- Bots a previous mode left on the humans' team move to the correct side when the round starts.
- The sound of a cheat power-up is precached, so it plays when you grab one.
- Modes no longer inherit `sv_maxspeed`, `sv_friction`, `sv_enablebunnyhopping`, damage scale, overtime and similar settings from the previous one.
- `pre-releases` branch: contributor pull requests are moved to it automatically, and it reaches `master` after three quiet
  days (see [CONTRIBUTING.md](CONTRIBUTING.md#branches)).

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
- `tools/test-modes.sh` runs every mode and match type on a real server and checks bots move and respawn.
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
- Changing mode failed with a lost rcon connection and left the change half done when plugins had to be reloaded.
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
