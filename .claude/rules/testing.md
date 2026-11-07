# Testing

Run what applies before saying a change works, and report what you ran and what you could not run.

| Changed | Run |
|---|---|
| Rust | `cargo fmt --all`, `cargo test` and `cargo clippy --all-targets` (no warnings) |
| Shell scripts | `shellcheck tools/*.sh packaging/*.sh packaging/aur/*.sh` |
| `tools/promote.sh` | `tools/test-promote.sh` |
| `tools/sync_readme.py` | `python3 tools/test_sync_readme.py` |
| Version or changelog | `tools/version.sh check` |
| `plugin/` | `./target/debug/csgo-legacy-server deploy` (compiles them; needs an installed server) |
| `cfg/`, `modes/`, `plugin/` | `tools/test-modes.sh <mode>` with the server running; all modes take about 12 minutes |

New logic that is not trivial gets a small test. New user text needs its translation, and `everything_translated` fails without it.

`tools/test-modes.sh` proves the game type, that bots move and that they respawn. It does not prove how a mode plays, skins, or joining from another PC: say those are untested unless a person tried them.
