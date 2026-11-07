# Testing

Run what applies before saying a change works, and report what you ran and what you could not run.

| Changed | Run |
|---|---|
| Rust | `cargo fmt --all`, `cargo test` and `cargo clippy --all-targets` (no warnings) |
| Shell scripts | `shellcheck tools/*.sh packaging/*.sh packaging/aur/*.sh` |
| `tools/promote.sh` | `tools/test-promote.sh` |
| `tools/sync_readme.py` | `python3 tools/test_sync_readme.py` |
| Version or changelog | `tools/version.sh check` |
| `game/plugin/` | `./target/debug/csgo-legacy-server deploy` (compiles them; needs an installed server) |
| `go/` | `go vet ./...` and `go test ./...` from `go/`, after `cargo build` (the e2e tests run the real CLI against a fake server, in seconds) |
| `game/cfg/`, `game/modes/`, `game/plugin/` | `go run ./cmd/testkit -root .. lint` from `go/`; with an installed server, `... full <mode>` (only what changed runs; all modes take about 3 minutes) |

New logic that is not trivial gets a small test. New user text needs its translation, and `everything_translated` fails without it.

`testkit` proves the game type, that bots move and that they respawn. It does not prove how a mode plays, skins, or joining from another PC: say those are untested unless a person tried them.
