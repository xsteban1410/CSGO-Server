# Troubleshooting

Start with `csgo-legacy-server doctor`, then the table in [SUPPORT.md](../SUPPORT.md). This page covers what those two cannot fix for you.

## The server does not start

| Message | Cause | What to do |
|---|---|---|
| The install is incomplete | An install was interrupted, so the marker file is still there | Run `csgo-legacy-server install`; it resumes where it stopped |
| Another install is running | Another process holds the install lock | Wait for it. The lock goes away by itself if that process dies |
| Checksum mismatch on Metamod or SourceMod | The download was corrupted, or the pinned build changed on the server | Run `install` again. If it keeps failing, open an issue: the pin needs updating |
| Port in use | Another server or program holds the UDP port | Stop it, or change the port in Settings |

## The game does not behave

- **Plugins stopped working after a game update.** Look for `signature not found` in the server log: Valve changed `engine.so`, and `csls_directconnect` needs new signatures. Open an issue with the output of `doctor --report`.
- **A plugin does not load.** `doctor` reports plugins older than their sources and the last SourceMod error. `csgo-legacy-server deploy` compiles them again, and a plugin that fails to compile keeps its last working `.smx`.
- **The bots do not move in `retake`.** That is expected until the bomb is planted: they defend it.
- **A skin does not show.** Run `skins_estado` in the server console. It prints the SteamID, the loaded skins and the netprops of each weapon.
- **The mode looks wrong.** `modo_estado` prints the wave, team, health and speed of each player, and `modo_sitios` the bomb sites used by `retake`.

## Restoring

`csgo-legacy-server backup list`, then `backup restore <file> --dry-run` shows what would change. `restore` saves the current state first, so you can undo it.

## Reporting a problem

`csgo-legacy-server doctor --report` prints a report with passwords, IPs and SteamIDs hidden. Read it before you paste it into an issue.
