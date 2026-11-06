# Security Policy

## Supported versions

Only the [latest release](https://github.com/xsteban1410/CSGO-Server/releases/latest) receives security fixes.

## Reporting a vulnerability

Please do not open a public issue. Use **Security › Report a vulnerability** on this repository, with the steps to
reproduce it and its impact. You should get an answer within a few days.

## What is in scope

- This project's code: the app, the command line, the bridge, the plugins and how they handle input from players.
- Secrets it manages: the rcon password, GSLT token and Workshop API key in `config.toml`.

Bugs in CS:GO, Steam, SourceMod or Metamod:Source should go to their own projects.

## Things to keep in mind when hosting

- The server runs with `-insecure`, so there is no VAC.
- Anyone who can reach the rcon port and knows the password controls the server. Keep the file private (it is created with
  mode 600) and do not share logs without removing secrets.
- The Internet and Community profiles expose the server to everybody. Use a server password if it is not meant to be public.
- The VPN bridge forwards to your server whatever reaches port PORT+1. Open that port only to your VPN if you can.
