# Safety

Never:

- delete the server install (`server/`, about 33 GB), the user's modes, plugins, skins or config, or any folder you did not create;
- commit or print passwords, GSLT tokens, API keys, `config.toml`, `server/`, `*.smx` or other build output;
- change `LICENSE`, remove `-insecure`, or add anything meant for official servers or to get around VAC, bans or Steam's rules;
- add telemetry, collect player data, or contact a new service without saying so;
- open the game, Steam or anything on the user's desktop, or start or stop a server, without asking;
- loosen `check_cvar`, `valid_map_name`, `valid_address`, `valid_id` or `CfgDoc::validate`: they stop command injection;
- check out or run pull request code in a `pull_request_target` workflow.

Before touching a path in the "Sensitive areas" table of `CONTRIBUTING.md`, read why it is dangerous and test what it asks for. If you cannot run that test, for example joining with a real client, say so plainly.

Known traps in the client: `+connect` is discarded, `-netconport` crashes the Linux client, and `steam://connect` opens CS2 if the game is closed.
