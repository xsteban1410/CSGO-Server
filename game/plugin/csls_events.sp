// SPDX-License-Identifier: GPL-3.0-or-later
// Writes player and map events as JSON lines for the app's agent, which keeps the local history (docs/PRIVACY.md).
// The agent reads this file and not the console, because a player could fake console lines with a name or in chat.
// No IP addresses: only the SteamID and the name.
#include <sourcemod>
#pragma semicolon 1
#pragma newdecls required

// One file per port: test instances share the install and must not count as play.
#define EVENTS_FILE "data/csls_events_%d.jsonl"

public Plugin myinfo =
{
	name = "CSLS events",
	author = "xsteban1410",
	description = "Writes connections, maps and bans for the app",
	version = "1.0",
	url = "https://github.com/xsteban1410/CSGO-Server"
};

char g_Path[PLATFORM_MAX_PATH];
bool g_Seen[MAXPLAYERS + 1];

// Quotes, backslashes and control characters are escaped, so a name cannot add fields or lines.
void Escape(const char[] text, char[] out, int len)
{
	int o = 0;
	for (int i = 0; text[i] != '\0' && o < len - 7; i++)
	{
		int c = text[i] & 0xFF;
		if (c == '"' || c == '\\')
		{
			out[o++] = '\\';
			out[o++] = c;
		}
		else if (c < 0x20 || c == 0x7F)
			o += Format(out[o], len - o, "\\u%04x", c);
		else
			out[o++] = c;
	}
	out[o] = '\0';
}

void Write(const char[] event, const char[] id, const char[] name, const char[] extra = "")
{
	File f = OpenFile(g_Path, "a");
	if (f == null)
		return;
	char n[256], x[256];
	Escape(name, n, sizeof(n));
	Escape(extra, x, sizeof(x));
	f.WriteLine("{\"t\":%d,\"e\":\"%s\",\"id\":\"%s\",\"name\":\"%s\",\"extra\":\"%s\"}", GetTime(), event, id, n, x);
	delete f;
}

// SourceMod does not give a SteamID on LAN, so it is computed from the account id as in skins.sp.
bool SteamId(int client, char[] out, int len)
{
	int account = GetSteamAccountID(client, false);
	if (account <= 0)
		return false;
	Format(out, len, "STEAM_1:%d:%d", account & 1, account >>> 1);
	return true;
}

public void OnMapStart()
{
	BuildPath(Path_SM, g_Path, sizeof(g_Path), EVENTS_FILE, FindConVar("hostport").IntValue);
	char map[64];
	GetCurrentMap(map, sizeof(map));
	Write("map", "", map);
}

public void OnClientPutInServer(int client)
{
	if (IsFakeClient(client))
		return;
	char id[32], name[MAX_NAME_LENGTH];
	if (!SteamId(client, id, sizeof(id)))
		return;
	GetClientName(client, name, sizeof(name));
	g_Seen[client] = true;
	Write("connect", id, name);
}

public void OnClientDisconnect(int client)
{
	if (!g_Seen[client])
		return;
	g_Seen[client] = false;
	char id[32], name[MAX_NAME_LENGTH];
	if (SteamId(client, id, sizeof(id)))
	{
		GetClientName(client, name, sizeof(name));
		Write("disconnect", id, name);
	}
}

public Action OnBanClient(int client, int time, int flags, const char[] reason, const char[] kick_message, const char[] command, any source)
{
	char id[32], name[MAX_NAME_LENGTH], extra[192];
	if (SteamId(client, id, sizeof(id)))
	{
		GetClientName(client, name, sizeof(name));
		Format(extra, sizeof(extra), "%d|%s", time, reason);
		Write("ban", id, name, extra);
	}
	return Plugin_Continue;
}
