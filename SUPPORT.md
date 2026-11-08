# Support

Try these first. If they do not help, [open an issue](https://github.com/xsteban1410/CSGO-Server/issues/new/choose).

| Problem | What to try |
|---|---|
| Something is missing or does not start | `csgo-legacy-server doctor` says what is missing and the command for your distribution |
| CS2 opens instead of CS:GO | Install the `csgo_legacy` beta (Steam › CS2 › Properties › Betas) and open it from the app or with `csgo-legacy-server play` |
| The game does not join by itself | The `connect` command is in your clipboard when you use the app: paste it in the game console |
| A friend over VPN cannot join | Use the "Friends over VPN" profile, and they start the bridge in their app's Server tab (**A friend's server over VPN**) with your VPN address, then type the `connect` it copies. Connecting to a 100.x address in the game does not work |
| There are no modes to choose | Modes come from CSGO-Hub: **Install every mode** in the Server tab, the Hub tab, or `csgo-legacy-server hub install pack-all` |
| `sm_login` says nothing to unlock | Only roles with `console` or `root` need it; check the role in Players › Admins |
| No answer through the bridge | Check that the server is on, that both VPNs are connected, and open port PORT+1 in your firewall (`sudo ufw allow 27031/udp`) |
| Server does not start after a game update | `csgo-legacy-server install --update` |
| Bots stand still on a Workshop map | It has no navigation: press **Generate .nav**, or run `csgo-legacy-server nav` |
