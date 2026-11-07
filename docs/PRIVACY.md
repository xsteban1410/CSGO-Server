# Privacy

The app has no telemetry and no account. It sends nothing to the project.

## What is kept on your PC

| Data | Where | Notes |
|---|---|---|
| Rcon password, GSLT token, Web API key | `~/.config/csgo-legacy-server/config.toml` (mode 600) | Also written at start to `cfg/csls_secrets.cfg` and `webapi_authkey.txt` in the server folder (mode 600) |
| SteamID and chosen skins of each player who picks one | `csgo/addons/sourcemod/data/sqlite/sourcemod-local.sq3` in the server folder | This includes your friends' SteamIDs |
| Player names, SteamIDs and IPs | Server and SourceMod logs, under `csgo/` in the server folder | Written by the game server and SourceMod |
| SteamID, name, connection times, bans and notes of each player; maps and modes played | `~/.local/share/csgo-legacy-server/stats.db`, written by the agent if you run it. **Never IP addresses.** History older than `stats_days` (365) is deleted; sanctions stay until you erase them. `stats = false` turns it off | Local only |
| Backups | `~/.local/share/csgo-legacy-server/backups/` (mode 600) | They contain the password, tokens and skins above: do not share them |

## How to delete it

- Skins: stop the server and delete `sourcemod-local.sq3`. This removes the skins of every player, and any other data SourceMod plugins keep in that file.
- Logs: delete `csgo/logs/` and `csgo/addons/sourcemod/logs/`.
- History: stop the agent (`agent stop`) and delete `stats.db`.
- Backups: delete the files in the folder above.
- Settings and secrets: delete `~/.config/csgo-legacy-server/`.

## What the app contacts

| Service | When | What it learns |
|---|---|---|
| AlliedModders (`mms.alliedmods.net`, `sm.alliedmods.net`) | Installing or updating | Your IP |
| Valve (SteamCMD, Steam CDN) | Installing or updating the server | Your IP |
| Steam Web API and Steam Community | When you use the Workshop tab or download a map | Your IP and the map ids |
| `api.ipify.org` | Only in the public network profiles, to show your public address | Your IP |
| Your router, through `upnpc` | Only if you turn UPnP on | The UDP port to open |

The game server itself talks to Valve and to the players who join, as any Source server does. With a public profile, anyone who knows your address can see it.
