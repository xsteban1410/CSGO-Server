# Coding

- Write the smallest change that fully solves the problem. Reuse existing helpers, and add no dependency, abstraction or option nobody asked for.
- Fix bugs at their cause, not in each caller, and update everything the change affects: callers, tests, docs.
- Everything is in English: code, comments, logs, commit messages and docs. Spanish goes only in translations.
- Do not rename `modo_*` convars, mode ids, chat commands such as `!armas`, or the `clave` and `valor` skin columns. Users' configs and presets depend on them.
- App text: write English in `t("…")` and add the Spanish to `crates/gtk/src/i18n.rs`. Plugin messages go in `plugin/translations/csls.phrases.txt`.
- A new game mode is `cfg/modo_<id>.cfg` plus `modes/<id>.toml`. Reset any new `modo_*` convar in `cfg/modo_base.cfg`.
- Do not add Valve's convars to `cfg/modo_base.cfg`: they override `gamemode_*.cfg` and break the official modes.
- Add a line under `## [Unreleased]` in `CHANGELOG.md`. Do not change the version number.
- Edit one README only: `.github/workflows/readme-sync.yml` translates the changed lines into the other. Both must keep the same lines in the same order.
