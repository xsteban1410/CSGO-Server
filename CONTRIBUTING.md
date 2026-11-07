# Contributing

Thanks for helping! Bug reports, new game modes, translations and code are all welcome. For something large, open an issue
first so we can agree on the approach. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) explains how the pieces fit together.

**Before you open a pull request, read [what you must not change](#what-you-must-not-change) and the
[sensitive areas](#sensitive-areas).**

## Reporting

Use the [issue forms](https://github.com/xsteban1410/CSGO-Server/issues/new/choose), which ask for what is needed. Search for an
existing issue first, remove passwords and tokens from your logs, and report security problems privately ([SECURITY.md](SECURITY.md)).

## Pull request steps

1. Fork the repository and create a branch from `pre-releases`.
2. Make your change, keeping it to a single topic.
3. Run the [checks](#checks) and add a line to `CHANGELOG.md` under `Unreleased`.
4. Open the pull request against `pre-releases` and fill in its template. Do not change the version number: the maintainer does that when releasing.

## Branches

- `master` is what users get: releases are built from it. Only the maintainer pushes to it.
- `pre-releases` is where work is tried first. Pull requests from contributors go here, and if you open one against `master`,
  CI moves it. CI also builds a package for every push, under **Actions › the run › Artifacts**.
- Once a day, `.github/workflows/promote.yml` merges `pre-releases` into `master` if all of this is true:
  - its newest commit is at least three days old, so any new commit restarts the wait;
  - CI passed on that commit;
  - no issue labelled `blocks-release` is open;
  - no issue labelled `bug` or `connection` has been opened since the first commit that is not in `master` yet.

The last rule cannot tell whether a report is about that change, so any new report holds the promotion until the maintainer
closes it or removes its label. To keep unfinished work out of `master`, open an issue labelled `blocks-release`.

If the merge conflicts, or the changes touch `.github/workflows` (the workflow token cannot push those), the job opens a
`blocks-release` issue with the commands to merge by hand. Close it afterwards, or promotion stays on hold. Changes pushed
straight to `master` reach `pre-releases` at the next daily run, unless it holds unpromoted commits or the push is rejected.

## Using Claude Code

The repository includes rules for [Claude Code](https://claude.com/claude-code) in `.claude/`: how to code, test and stay safe here. They load by themselves. You remain responsible for what you submit.

## Set up

You need Rust and the GTK ≥ 4.14 and libadwaita ≥ 1.6 development files. To run a server you also need 32-bit libraries and
about 40 GB.

```bash
git clone https://github.com/xsteban1410/CSGO-Server.git && cd CSGO-Server
cargo build
./target/debug/csgo-legacy-server doctor
./target/debug/csgo-legacy-server install      # downloads ~33 GB; only needed to run a server
./target/debug/csgo-legacy-server-gtk          # from the repository, it uses its game/ folder
```

## Checks

```bash
cargo test
cargo fmt --all
cargo clippy --all-targets
shellcheck tools/*.sh packaging/*.sh packaging/aur/*.sh
```

CI runs these, compiles the plugins and builds the package. To test modes on a real server, without players:

```bash
cd go
go test ./...                               # the CLI against a fake server, in seconds; needs `cargo build`
go run ./cmd/testkit -root .. lint          # configs, modes and phrases, without a server
go run ./cmd/testkit -root .. smoke         # one case per game type on real servers, about a minute
go run ./cmd/testkit -root .. full          # all of them, about 3 minutes; what did not change is skipped
go run ./cmd/testkit -root .. full dm normal:ffa   # only these (mode or mode:type)
```

It checks the game type, that bots move and that they respawn. Anything needing a person (aim, feel, skins, joining from
another PC) has to be played, so say in your pull request what you could not test.

## Adding a game mode

1. Create `game/cfg/modo_<id>.cfg` with the rules, and `game/modes/<id>.toml` with its name and description in English and Spanish.
2. Only set what your mode changes. Weapons per team go in `modo_t_*` and `modo_ct_*`.
3. If it needs logic, add it to `game/plugin/modos*.sp` behind a `modo_*` convar, and reset that convar in `game/cfg/modo_base.cfg`.
4. Add it to the table in both READMEs and run `go run ./cmd/testkit -root .. full <id>` from `go/`.

Do not add Valve's convars to `game/cfg/modo_base.cfg` unless you checked that no `gamemode_*.cfg` relies on them. The
[architecture notes](docs/ARCHITECTURE.md#how-a-mode-is-applied) explain why.

## Translations and text

- App: write English in `t("…")` and add the Spanish to `crates/gtk/src/i18n.rs`. A test fails if one is missing.
- Plugins: `game/plugin/translations/csls.phrases.txt`.
- READMEs: edit one, and a workflow translates the changed lines into the other after the push. Both must keep the same lines in the same order. If you edit both, nothing is translated.

## Pull request rules

- Fix bugs at their cause, and add a small test for new logic that is not trivial.
- Everything in the repository is written in English: commit messages, code, comments, logs and documentation. Only translations are in Spanish: `README.es.md`, `crates/gtk/src/i18n.rs`, the `es` entries of `game/plugin/translations/csls.phrases.txt` and of `game/modes/*.toml`, and `game/data/skins.es.cfg`. English is the default language everywhere: Spanish only shows when chosen in Settings. CI fails if accented text appears anywhere else.
- One commit per change, with the message `type(scope): what changed`, for example `fix(skins): recompile plugins when sources change`. Types: `feat`, `fix`, `i18n`, `refactor`, `test`, `docs`, `ci`, `chore`. The scope is the area touched: `skins`, `modes`, `deploy`, `gtk`...
- Contributions are licensed under [GPL-3.0-or-later](LICENSE).

## What you must not change

Pull requests doing any of these will be closed:

- Change `LICENSE` or remove license headers. Everything here is GPL-3.0-or-later.
- Remove `-insecure`, or add anything meant to work on official servers or to get around VAC, bans or Steam's rules.
- Add code that collects or sends player data, telemetry or credentials, or contacts a new service without saying so.
- Commit passwords, GSLT tokens, API keys, `config.toml`, the `server/` folder, `*.smx` or other build output.
- Delete users' files: the server install, `skins.cfg`, their modes and plugins and their config must survive every update.

## Sensitive areas

You can change these, but a mistake breaks things for everyone or opens a security hole. Explain why in your pull request and
say exactly how you tested it. CI labels these pull requests `sensitive` and the maintainer reviews them by hand.

| Path | Why it is dangerous | What to do |
|---|---|---|
| `game/cfg/modo_base.cfg`, `game/cfg/modo_clasico.cfg` | They run before every mode. A wrong convar breaks all modes: `mp_buy_anywhere 0` froze the deathmatch bots | Run the whole `tools/test-modes.sh` |
| `game/plugin/modos.sp` (`OnConfigsExecuted`) | Decides the order of every config | Same, and read the [architecture notes](docs/ARCHITECTURE.md#how-a-mode-is-applied) |
| `game/plugin/csls_directconnect/` | Patches the engine in memory with signatures. Wrong bytes crash the server, and nobody can join without it | Test with a real client joining by IP |
| `crates/core/src/bridge.rs`, `sv_lan` and the bridge in `server.rs` | Friends over VPN depend on exactly this recipe. Steam rejects clients otherwise (`STEAMAUTH` code 10) | Test from a second PC over a real VPN |
| `crates/core/src/steam_client.rs` | `+connect` is discarded, the TCP console crashes the client, and `steam://connect` opens CS2 if the game is closed | Test with the real game, on a real client |
| `check_cvar`, `valid_map_name`, `valid_address`, `valid_id`, `CfgDoc::validate` | They stop values from injecting console commands. They are a security boundary | Never loosen them without tests that try to break them |
| `go/cmd/agent`, `go/internal/sched`, `game/plugin/csls_events.sp` | The agent holds the rcon password and runs commands, and the plugin writes what a player can influence. Keep the list of actions closed, no shell, and no IP in the database | `go test ./...`, and try it on a real server |
| `crates/core/src/rcon.rs`, `config.rs` | The rcon password gives full control of the server, and the config must stay at mode 600 | Never log secrets or print them |
| `crates/core/src/install.rs`, `deploy.rs` | They download and run code, and write into the user's install, which is 33 GB to download again | Test on a copy, and never delete a folder you did not create |
| `.github/`, `.claude/`, `tools/version.sh`, `tools/promote.sh`, `tools/sync_readme.py`, `packaging/` | They publish what users install | Only the maintainer merges these. Do not add secrets or third-party actions without a good reason |

## Releasing (maintainers)

Write the changes under `## [Unreleased]` in `CHANGELOG.md` as you go. To publish, on `master` (a bump on `pre-releases` is released when it is promoted):

```bash
tools/version.sh bump 1.2.0
git commit -am "1.2.0: summary" && git push
```

`bump` changes the version everywhere and turns `Unreleased` into the notes of 1.2.0. When that reaches `master`,
`.github/workflows/release.yml` runs CI, creates the tag `v1.2.0` and publishes the release with the package and its changelog.
It refuses to publish if the version has no notes. Every push also leaves a package under **Actions › the run › Artifacts**,
for testing before a release.
