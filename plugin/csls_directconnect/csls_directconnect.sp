// csls_directconnect: permite conectar por IP (connect IP:PUERTO) a un servidor de CS:GO Legacy.
// El motor solo acepta jugadores que vienen de un lobby de matchmaking; este plugin parchea en memoria
// las dos comprobaciones (servidor exclusivo de lobby y el id de lobby del challenge) y las deja como
// estaban al descargarse. Las direcciones y los bytes están en gamedata/csls_directconnect.games.txt.
//
// Copyright (C) 2026 xsteban1410 — GPL-3.0-or-later.
#pragma semicolon 1
#pragma newdecls required

#include <sourcemod>

public Plugin myinfo =
{
	name = "csls direct connect",
	author = "xsteban1410",
	description = "Direct IP connections without a matchmaking lobby",
	version = "1.0.0",
	url = "https://github.com/xsteban1410/CSGO-Server"
};

#define MAX_PATCHES 4
#define MAX_BYTES 32

// cada parche: nombre de la dirección en gamedata; los bytes en Keys "<nombre>:bytes"
// y el desplazamiento opcional en Offsets "<nombre>:offset"
static const char g_Names[][] = { "lobby_exclusive", "challenge_lobby_id" };

Address g_Addr[MAX_PATCHES];
int g_Len[MAX_PATCHES];
int g_Saved[MAX_PATCHES][MAX_BYTES];
int g_Count;

public void OnPluginStart()
{
	GameData gd = new GameData("csls_directconnect.games");
	if (gd == null)
		SetFailState("missing gamedata csls_directconnect.games.txt");

	int want[MAX_PATCHES][MAX_BYTES];
	for (int i = 0; i < sizeof(g_Names); i++)
	{
		Address base = gd.GetAddress(g_Names[i]);
		if (base == Address_Null)
		{
			delete gd;
			SetFailState("signature not found: %s (engine updated?)", g_Names[i]);
		}
		char key[64], hex[256];
		Format(key, sizeof(key), "%s:bytes", g_Names[i]);
		if (!gd.GetKeyValue(key, hex, sizeof(hex)))
		{
			delete gd;
			SetFailState("missing key %s", key);
		}
		Format(key, sizeof(key), "%s:offset", g_Names[i]);
		int off = gd.GetOffset(key);
		g_Addr[i] = base + view_as<Address>(off > 0 ? off : 0);
		g_Len[i] = ParseHex(hex, want[i]);
	}
	delete gd;

	// todo encontrado: ahora sí se escribe
	g_Count = sizeof(g_Names);
	for (int i = 0; i < g_Count; i++)
	{
		for (int b = 0; b < g_Len[i]; b++)
		{
			Address at = g_Addr[i] + view_as<Address>(b);
			g_Saved[i][b] = LoadFromAddress(at, NumberType_Int8);
			StoreToAddress(at, want[i][b], NumberType_Int8);
		}
	}
}

public void OnPluginEnd()
{
	for (int i = 0; i < g_Count; i++)
		for (int b = 0; b < g_Len[i]; b++)
			StoreToAddress(g_Addr[i] + view_as<Address>(b), g_Saved[i][b], NumberType_Int8);
}

// "31 C0 C3" -> {0x31, 0xC0, 0xC3}; devuelve cuántos bytes leyó.
int ParseHex(const char[] s, int out[MAX_BYTES])
{
	int n, digits, cur;
	for (int i = 0; s[i] != '\0' && n < MAX_BYTES; i++)
	{
		int v = HexDigit(s[i]);
		if (v < 0)
			continue;
		cur = cur * 16 + v;
		if (++digits == 2)
		{
			out[n++] = cur;
			cur = 0;
			digits = 0;
		}
	}
	return n;
}

int HexDigit(int c)
{
	if (c >= '0' && c <= '9') return c - '0';
	if (c >= 'a' && c <= 'f') return c - 'a' + 10;
	if (c >= 'A' && c <= 'F') return c - 'A' + 10;
	return -1;
}
