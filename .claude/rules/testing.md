# Testing

Run what applies before saying a change works, and report what you ran and what you could not run.

| Changed | Run |
|---|---|
| Rust | `cargo fmt --all`, `cargo test` and `cargo clippy --all-targets` (no warnings) |
| Shell scripts | `shellcheck tools/*.sh packaging/*.sh packaging/aur/*.sh packaging/appimage/AppRun` |
| `packaging/` | The `package`, `bundles` and `flatpak` jobs in CI build and try every package; `packaging/build-bundles.sh` builds the `.deb` and the AppImage locally |
| `tools/promote.sh` | `tools/test-promote.sh` |
| `tools/sync_readme.py` | `python3 tools/test_sync_readme.py` |
| Version or changelog | `tools/version.sh check` |
| `game/plugin/` | `./target/debug/csgo-legacy-server deploy` (compiles them; needs an installed server) |
| `go/` | `go vet ./...` and `go test ./...` from `go/`, after `cargo build` (the e2e tests run the real CLI against a fake server, in seconds) |
| `game/cfg/`, `game/plugin/` | `go run ./cmd/testkit -root .. lint` from `go/`; with an installed server and hub items installed (`hub install pack-all`), `... full <mode>` (only what changed runs; all modes take about 4 minutes) |
| `crates/core/src/hub.rs` | `cargo test`, and `csgo-legacy-server hub check ~/CSGO-Hub/items/*` |

New logic that is not trivial gets a small test. New user text needs its translation, and `everything_translated` fails without it.

`testkit` proves the game type, that bots move and that they respawn. It does not prove how a mode plays, skins, joining from another PC, `sm_login` and admin commands, or anticheat detections against real cheats: say those are untested unless a person tried them.
