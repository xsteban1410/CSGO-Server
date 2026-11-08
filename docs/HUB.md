# CSGO-Hub

[CSGO-Hub](https://github.com/xsteban1410/CSGO-Hub) holds the game modes, their plugins, skins and admin tools. This repository
is the engine that installs and runs them. How to make an item is in the hub's
[CONTRIBUTING.md](https://github.com/xsteban1410/CSGO-Hub/blob/main/CONTRIBUTING.md); this page is how the app handles them.

## What goes where

| Change | Repository |
|---|---|
| A new mode, or a fix to how a mode plays | CSGO-Hub |
| A plugin that adds a mode, a tool or a command | CSGO-Hub |
| Something every mode needs: the cfg chain, loadouts, teams, `modo_*` convars, the plugin API | CSGO-Server |
| Admins, the anticheat, rcon, deploy, the app | CSGO-Server |

A hub item that needs something new from the engine waits for a CSGO-Server release and sets `min_engine`.

## The index

CI publishes `https://xsteban1410.github.io/CSGO-Hub/index.json` from `main` (`CSLS_HUB_URL` overrides it for tests):

```json
{"format": 1, "items": [{"id": "zombies", "kind": "mode", "version": "1.0.0", "api": 1, "min_engine": "",
  "requires": ["modos_zombies"], "archive": "https://xsteban1410.github.io/CSGO-Hub/items/zombies-1.0.0.zip", "sha256": "…", "size": 1234,
  "preview_url": "https://xsteban1410.github.io/CSGO-Hub/previews/zombies-1.0.0.png", "has_code": false, "name": {"en": "…", "es": "…"}, "…": "…"}]}
```

The fields of `item.toml` plus where to get it. `format` changes only when an older app could not read the index.

## Installing

`core::hub` (`hub install`, the Hub tab):

1. Fetches the index with the https-only curl helper (`core::http`) and a size limit.
2. Resolves `requires`, dependencies first, refusing cycles; checks `api` and `min_engine`.
3. Downloads each zip, refuses it if its size or SHA-256 differ from the index, and unpacks it into `hub/.staging/<id>`:
   no links, no absolute paths or `..`, only the allowed extensions, size limits.
4. Checks the folder again as `hub check` does: `item.toml`, mode cfgs with `CfgDoc::validate` line by line, the metadata,
   no plugin named like an engine one, a preview under 300 KB.
5. Renames it to `hub/<id>`. The folder watcher (app) or the command (CLI) then deploys, compiling the plugins locally.

Nothing compiled is ever downloaded. `hub remove` refuses an item another installed one requires.

## Checking an item

`csgo-legacy-server hub check <folder>` is the same validator, and the hub's CI runs it on every pull request with the
engine ref in its `engine.toml`. The hub also lints convar resets and translations (`tools/lint.py`) and compiles every
plugin against `csls.inc` with the SourceMod build in `game/data/pinned.toml`.

## Trust

Only what is merged into the hub's `main`, after review and a green CI, reaches the index. The app checks every download
against the index and every file again, and asks before installing an item with plugin code. None of this proves a
plugin is harmless; the review does. A harmful item is removed from `main`, and so from the index.
