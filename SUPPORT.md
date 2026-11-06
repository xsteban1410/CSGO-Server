# Support

Try these first. If they do not help, [open an issue](https://github.com/xsteban1410/CSGO-Server/issues/new/choose).

| Problem | What to try |
|---|---|
| Something is missing or does not start | `csgo-legacy-server doctor` says what is missing and the command for your distribution |
| CS2 opens instead of CS:GO | Install the `csgo_legacy` beta (Steam › CS2 › Properties › Betas) and open it from the app or with `csgo-legacy-server play` |
| The game does not join by itself | The `connect` command is in your clipboard when you use the app: paste it in the game console |
| A friend over VPN cannot join | Use the "Friends over VPN" profile, and they use the **Join** tab with your VPN address and **Test connection**. Connecting to a 100.x address in the game does not work |
| No answer through the bridge | Check that the server is on, that both VPNs are connected, and open port PORT+1 in your firewall (`sudo ufw allow 27031/udp`) |
| Server does not start after a game update | `csgo-legacy-server install --update` |
| Bots stand still on a Workshop map | It has no navigation: press **Generate .nav**, or run `csgo-legacy-server nav` |
