# Changelog

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The section of each version becomes the notes of its GitHub release, so keep the headings as `## [x.y.z] - date`.

## [Unreleased]

### Added

- CSGO-Hub: game modes, plugins and packs come from [CSGO-Hub](https://github.com/xsteban1410/CSGO-Hub). `csgo-legacy-server hub list|search|info|install|update|remove` installs them with what they require, checks each download's SHA-256 and every file again, and compiles plugins locally. `hub check <dir>` is the validator the hub's CI runs.
- Admins and roles: `[admins]` in the config and `csgo-legacy-server admins list|add|remove|role|password`. Roles grant a closed set of permissions (`console`, `kick`, `ban`, `map`, `slay`, `chat`, `cheats`, `bots`, `root`); the ones that give the server console (`sm_rcon`) also need `sm_login <password>` in the game console, since SteamIDs can be faked with `-insecure`. Three wrong passwords kick, and a second player with an admin's SteamID is kicked.
- An anticheat in the engine, set in `[anticheat]` of the config (`enabled`, `action = "log" | "kick" | "ban"`, `ban_minutes`): impossible view angles, aim snaps before kills, scripted bunny hops, speedhack and wallhack client cvars. Detections show in the app's sanctions. Plugins that move aim or speed on purpose call `CSLS_Exempt`, and the `cheats` permission is immune.
- A Hub tab in the app: browse CSGO-Hub with previews, search and filters, install with what an item needs, update and remove. With no modes installed, the Server and Modes tabs offer to install them all.
- Admins in the Players tab (add one from a connected player, change the role, copy a new `sm_login` password) and the anticheat in Settings.
- `game/plugin/include/csls.inc`, the engine API for hub plugins: `CSLS_OnModeReset`, `CSLS_OnEquip`, `CSLS_OnPlaceHuman` and `CSLS_Exempt`.

### Changed

- The README translation works a paragraph at a time, so sentences over a line break stay whole, keeps project terms (hub items, plugins, skins, admins, anticheat) and speaks to the reader as tú in Spanish.
- `pre-releases` reaches `master` after one quiet day instead of three, or at once when the owner opens an issue titled `approve release` (see [CONTRIBUTING.md](CONTRIBUTING.md#branches)).
- The app ships without game modes: the 46 modes, their plugins and skins moved to CSGO-Hub (`hub install pack-all` brings them all back). Modes load in layers: hub, then your own; a broken one is skipped with a warning instead of hiding every mode.
- The side bar groups the tabs (Server, Content, Manage) with icons, and arrow keys move between them. Buttons that delete are drawn in red.
- Deploy removes mode cfgs and plugins of items you uninstalled, and the plugins that moved to the hub unless you installed them.

### Removed

- The Join tab, `play cs2` and the "Only join servers" first-run choice. The bridge to a friend's server over VPN is now in the Server tab, and `play [IP:PORT|local]` still opens CS:GO Legacy.

### Fixed

- Updating the server while it runs said "Stop the server?" as an error; it now says to stop it first.

- Map names can no longer point outside `maps/` (absolute paths, empty parts), and the default map and a mode's maps are checked too.
- Humans, bots, rounds and tickrate have limits in the core, so the CLI checks them like the app.
- The rcon password is never made from the clock: without `/dev/urandom` the app stops and says so.
- Only plugins with plain names (letters, numbers, `-`, `_`) are deployed, since the name goes into an rcon command.
- An unreadable mode metadata file is an error instead of a mode with empty metadata.
- Launching the game from a Snap Steam works with quotes in a path.
- Downloads only use https, also after redirects, and have a size limit; a Workshop map must match the size Steam announces.

## [1.3.0] - 2026-11-07

### Added

- A new desktop app built with Slint replaces the GTK one, with a tray icon. The program is now `csgo-legacy-server-app`. On top of the old tabs it has:
  - a Server tab with RCON and agent state, warnings and recent events;
  - a console with filters by category, saving the log, autocomplete, favorites, persistent history and aliases (also `alias` in the CLI);
  - player history, sanctions and CSV and JSON export;
  - in the mode editor, round time, bots, a preview of the cfg, and rule blocks ("when a kill happens, add 25 health") from a closed catalog, run by the new `modos_reglas` plugin;
  - Plugins (on and off, install `.sp` or `.smx`, reload, go back to an earlier version; also `plugins` in the CLI), Tasks, Backups, Statistics and Updates tabs.
- Releases also carry a `.deb` (Debian 12 and newer, Ubuntu 24.04 and newer), an AppImage, a sandboxed Flatpak and a `SHA256SUMS` file.
- The GTK app and its `csgo-legacy-server-gtk` program are gone: update any shortcut of your own.
- The Arch packages include the agent, so `agent start` works there.
- `updates` in the CLI and a button in the app ask GitHub for the latest release. Only on request, unless you turn on the check at startup. Nothing installs by itself; an AppImage replaces itself only after you accept and its SHA256SUMS match.
- `agent start|stop|status`: an optional helper that watches the server with a single poll, rotates `console.log`, and keeps a local history of players, maps, modes and bans in `stats.db` (no IP addresses, see `docs/PRIVACY.md`; `stats = false` turns it off). It also runs scheduled restarts, map or mode changes, rcon commands, backups and plugin deploys from `schedule.json`.
- `backup create --auto` and `backup_keep` in the config set how many automatic backups stay.
- `go/cmd/testkit` tests configs in seconds without a server and all modes in about 3 minutes instead of 12, on parallel servers, skipping what did not change.
- `doctor` and `start` warn about weak or shared passwords, rcon open to the network, a public server without a password and loose file permissions. Empty rcon passwords are generated, and secret commands stay out of the console history.
- The bridge has limits per server, per IP and per second, an address allowlist (`bridge_allow`, VPN range and loopback by default, `bridge_max_clients`) and `bridge --status`.
- Installs write a manifest, `install.json`, with versions and hashes. SourceMod and Metamod are checked, extracted aside and swapped as a whole, and a failed update restores the previous version.
- Installs check free disk space first and during SteamCMD, and say so clearly when the disk fills up.
- Shell completions for bash, zsh and fish: `completions <shell>`, installed by the packages.
- `docs/TROUBLESHOOTING.md`, and `docs/PRIVACY.md` with what is stored and what the app contacts.
- CI builds and tests the core and CLI on Debian and Fedora, audits dependencies and fails on clippy warnings.
- **Language** in Settings (`language` in `config.toml`): English by default, Spanish if you choose it. It no longer depends on the
  system locale, and it covers the app, the command line, skin names and in-game messages for every player. Install progress and
  common errors are translated too.
- Backups: `backup create|list|restore [--dry-run]` and the **Backups** tab save your config, modes, plugins, presets and skins. `install --update` makes one first.
- `doctor` also checks unfinished installs, stale plugins, SourceMod errors, config permissions and whether the server survives logging out. **Settings → Diagnosis** shows the same. `doctor --report` hides private data, and `status --json` is for scripts.
- The Server tab shows CPU, RAM, uptime and the last SourceMod error, and the app tells you if the server stops on its own. The install shows its download speed.
- Changing only one README translates the changed lines into the other, with Google Translate.
- `.claude/` holds rules for contributors who use Claude Code.
- Competitive category with normal matches instead of custom rules: `competitivo` (MR15), `premier` (MR12 with map bans), `wingman`
  (2v2) and `duelo` (1v1 without bots). They have economy, half time and overtime, and bots fill the empty seats.
- `--teams together|versus|free` and **Teams** in the wizard, `!ready`/`!listo` to end the warmup, and map bans in `premier`.
- New modes: `aim`, `hardcore`, `pesados`, `ffa`, `tdm`, `dangerzone`, `trigger`, `stabzap`, `clutch` and `ninjas`.
- `modo_<team>_color` accepts `R,G,B,alpha` for see-through players.

### Changed

- The game files moved to `game/` (`game/cfg`, `game/modes`, `game/plugin`, `game/data`). Installed packages keep their layout.
- Nothing is forced any more, as on a Valve or community server: everyone picks their team, bots fill the empty seats and warmup runs out by itself. Only modes whose rules need sides fix them (`retake`, `defensa`, `clutch`, `zombies`, `ninjas`, `aim`, Guardian). Configs saved earlier stop waiting for two humans; presets keep what they saved.
- `competitivo` and `premier` start humans balanced, one on each side, and `--teams` only says where a human starts: it no longer moves anyone back or along with a teammate.
- `competitivo` and `premier` end in a draw at 15-15 and 12-12 unless the new `--no-draw` rule (**No draws** in the wizard) is on, which plays overtime until someone wins.
- `--no-bots` (**No bots** in the wizard) plays humans only, and the Wingman match type fills four seats.
- All code comments, messages, logs, tests and documentation are now in English. The app, plugin messages and `README.es.md`
  are still translated into Spanish, but errors coming from the core now always appear in English.
- `modo_estado` prints `team=`, `health=`, `speed=`, `scale=` and `type=`.
- Dependabot Rust updates go to `pre-releases`. Contributor pull requests are moved to it automatically, and it reaches `master` after three quiet days (see [CONTRIBUTING.md](CONTRIBUTING.md#branches)).
- Every mode description was rewritten so it says what you actually play, and the wizard groups modes by category with rarely used options under **Advanced**.
- Weapon and round modes have a short freeze time, no friendly fire and no money rewards, since there is nothing to buy.
- `zeus`, `decoy` and `unabala` are now deathmatch modes, with real scoring; Zeus and decoys never run out.
- `pistolas` is now called Deagle, which is what it gives you.
- `guardian` no longer uses `exec`, so it can be duplicated in the Modes tab.
- SourceMod and Metamod are pinned to the tested builds and checked with SHA-256 before unpacking, instead of taking the latest.
- An interrupted install or update is detected: the server will not start until `install` resumes it. Two installs cannot run at
  once, the server must be stopped first, and downloads retry with growing waits.

### Security

- The rcon password, GSLT token and workshop key are no longer process arguments, which anyone on the PC could read with `ps`.
  They live in files only you can read, and so does `server.cfg`, which holds the server password.
- Internet and community profiles warn at start that the server is reachable from the internet, has no VAC and whether it has a password.
- `doctor` warns if rcon can be reached from other machines, and a GSLT with quotes or `;` is rejected.

### Fixed

- The app window has its app id, so docks show its name and icon.
- The agent's polling no longer floods the app's console.
- Choosing to only join servers is remembered.
- Stopping the server no longer signals an unrelated process when its PID was reused, and leftover servers are found and stopped. `status` says why the server stopped last.
- Rcon reports wrong passwords and lost connections as such, and reconnects during a deploy.
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

[Unreleased]: https://github.com/xsteban1410/CSGO-Server/compare/v1.3.0...HEAD
[1.3.0]: https://github.com/xsteban1410/CSGO-Server/compare/v1.1.0...v1.3.0
[1.1.0]: https://github.com/xsteban1410/CSGO-Server/compare/v1.0.2...v1.1.0
[1.0.2]: https://github.com/xsteban1410/CSGO-Server/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/xsteban1410/CSGO-Server/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/xsteban1410/CSGO-Server/releases/tag/v1.0.0
