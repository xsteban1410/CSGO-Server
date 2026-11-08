# Coding

- Write the smallest change that fully solves the problem. Reuse existing helpers, and add no dependency, abstraction or option nobody asked for.
- Fix bugs at their cause, not in each caller, and update everything the change affects: callers, tests, docs.
- Everything is in English: code, comments, logs, commit messages and docs. Spanish goes only in translations.
- Do not rename `modo_*` convars, mode ids, chat commands such as `!armas`, or the `clave` and `valor` skin columns. Users' configs and presets depend on them.
- App text: write English in `@tr("…")` or `t("…")` and add the Spanish to `crates/app/lang/es/LC_MESSAGES/csgo-legacy-server-app.po`. Plugin messages go in `game/plugin/translations/csls.phrases.txt`.
- A new game mode, mode plugin, admin tool or skin change goes to CSGO-Hub (`~/CSGO-Hub`, its own rules), not here. This repo is the engine: change it only when every mode needs it (docs/HUB.md). Reset any new engine `modo_*` convar in `game/cfg/modo_base.cfg`, and keep `game/plugin/include/csls.inc` compatible or raise `CSLS_API`.
- Never loosen `hub.rs` checks (paths, extensions, sizes, SHA-256, `check_dir`) or let a hub item replace an engine plugin. Admin permissions that give the console stay behind `sm_login`.
- Do not add Valve's convars to `game/cfg/modo_base.cfg`: they override `gamemode_*.cfg` and break the official modes.
- Add a line under `## [Unreleased]` in `CHANGELOG.md`. Do not change the version number.
- Edit one README only: `.github/workflows/readme-sync.yml` translates the changed lines into the other. Both must keep the same lines in the same order.
