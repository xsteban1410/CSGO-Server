// SPDX-License-Identifier: GPL-3.0-or-later
// The engine's own anticheat: catches what a server can see of common cheats against players on your server.
// It cannot see a wallhack that only draws on the client, so it checks the client cvars that would show one.
// Settings come from [anticheat] in the app's config: deploy writes cfg/sourcemod/csls_anticheat.cfg.
// Plugins that move aim or speed on purpose (aimbot, noclip) call CSLS_Exempt, and the `cheats` permission is immune.
#include <sourcemod>
#include <sdktools>
#define CSLS_ANTICHEAT
#include <csls>
#pragma semicolon 1
#pragma newdecls required

#define EVENTS_FILE "data/csls_events_%d.jsonl"

public Plugin myinfo =
{
	name = "CSLS anticheat",
	author = "xsteban1410",
	description = "Server-side checks for aimbot, spinbot, bhop scripts, speedhack and wallhack cvars",
	version = "1.0",
	url = "https://github.com/xsteban1410/CSGO-Server"
};

enum Check
{
	Check_Angles,
	Check_Snap,
	Check_Bhop,
	Check_Speed,
	Check_Cvar,
	Check_Count
}

char g_CheckNames[Check_Count][] = { "angles", "aim_snap", "bhop", "speedhack", "cvar" };

ConVar g_Enabled, g_Action, g_BanMinutes, g_Cheats, g_AutoBhop;
char g_Path[PLATFORM_MAX_PATH];

float g_ExemptUntil[MAXPLAYERS + 1];
float g_LastAction[MAXPLAYERS + 1][Check_Count];
// angles
int g_BadAngles[MAXPLAYERS + 1];
// aim snap: the largest change of aim in the last ticks, and the kills right after one
float g_PrevAngles[MAXPLAYERS + 1][3];
float g_Snap[MAXPLAYERS + 1][3];
int g_SnapTick[MAXPLAYERS + 1];
int g_SnapKills[MAXPLAYERS + 1];
float g_FirstSnapKill[MAXPLAYERS + 1];
// bhop
bool g_WasOnGround[MAXPLAYERS + 1];
int g_GroundTicks[MAXPLAYERS + 1];
int g_PerfectJumps[MAXPLAYERS + 1];
// speedhack: commands per second against the tickrate
int g_Cmds[MAXPLAYERS + 1];
int g_FastSeconds[MAXPLAYERS + 1];

public APLRes AskPluginLoad2(Handle myself, bool late, char[] error, int err_max)
{
	RegPluginLibrary("csls_anticheat");
	CreateNative("CSLS_AC_Exempt", Native_Exempt);
	return APLRes_Success;
}

public void OnPluginStart()
{
	LoadTranslations("csls_anticheat.phrases");
	g_Enabled = CreateConVar("csls_ac_enabled", "1", "Anticheat on (1) or off (0)", _, true, 0.0, true, 1.0);
	g_Action = CreateConVar("csls_ac_action", "0", "What to do on a detection: 0 log, 1 kick, 2 ban", _, true, 0.0, true, 2.0);
	g_BanMinutes = CreateConVar("csls_ac_ban_minutes", "0", "Ban length in minutes (0 = permanent)", _, true, 0.0);
	AutoExecConfig(true, "csls_anticheat");
	g_Cheats = FindConVar("sv_cheats");
	g_AutoBhop = FindConVar("sv_autobunnyhopping");
	BuildPath(Path_SM, g_Path, sizeof(g_Path), EVENTS_FILE, FindConVar("hostport").IntValue);
	HookEvent("player_death", OnPlayerDeath);
	CreateTimer(1.0, Timer_Second, _, TIMER_REPEAT);
	CreateTimer(20.0, Timer_Cvars, _, TIMER_REPEAT);
}

public any Native_Exempt(Handle plugin, int params)
{
	int client = GetNativeCell(1);
	if (client < 1 || client > MaxClients)
		return 0;
	float until = GetEngineTime() + view_as<float>(GetNativeCell(2));
	if (until > g_ExemptUntil[client])
		g_ExemptUntil[client] = until;
	return 0;
}

public void OnClientConnected(int client)
{
	g_ExemptUntil[client] = 0.0;
	for (int c = 0; c < view_as<int>(Check_Count); c++)
		g_LastAction[client][c] = 0.0;
	g_BadAngles[client] = 0;
	g_SnapKills[client] = 0;
	g_PerfectJumps[client] = 0;
	g_GroundTicks[client] = 0;
	g_Cmds[client] = 0;
	g_FastSeconds[client] = 0;
}

bool Watched(int client)
{
	return g_Enabled.BoolValue && !g_Cheats.BoolValue && IsClientInGame(client) && !IsFakeClient(client)
		&& GetEngineTime() >= g_ExemptUntil[client] && !CheckCommandAccess(client, "csls_anticheat_immune", ADMFLAG_CUSTOM1);
}

float AngleDiff(float a, float b)
{
	float d = FloatAbs(a - b);
	while (d > 360.0)
		d -= 360.0;
	return d > 180.0 ? 360.0 - d : d;
}

public Action OnPlayerRunCmd(int client, int &buttons, int &impulse, float vel[3], float angles[3], int &weapon)
{
	if (!Watched(client))
		return Plugin_Continue;
	g_Cmds[client]++;

	// a real client keeps pitch within ±89 and never rolls the view: spinbots and anti-aim do
	if (FloatAbs(angles[0]) > 89.5 || FloatAbs(angles[2]) > 0.5)
	{
		if (++g_BadAngles[client] >= 10)
			Detect(client, Check_Angles, "pitch %.1f roll %.1f", angles[0], angles[2]);
	}

	float delta = AngleDiff(angles[1], g_PrevAngles[client][1]) + FloatAbs(angles[0] - g_PrevAngles[client][0]);
	int tick = GetGameTickCount();
	g_Snap[client][tick % 3] = delta;
	g_SnapTick[client] = tick;
	g_PrevAngles[client] = angles;

	// a script jumps on the very first tick on the ground, every time
	bool onGround = (GetEntityFlags(client) & FL_ONGROUND) != 0;
	if (g_AutoBhop == null || !g_AutoBhop.BoolValue)
	{
		if (onGround)
			g_GroundTicks[client]++;
		else if (g_WasOnGround[client] && GetEntityMoveType(client) == MOVETYPE_WALK)
		{
			if (g_GroundTicks[client] <= 1)
			{
				if (++g_PerfectJumps[client] >= 15)
					Detect(client, Check_Bhop, "%d perfect jumps in a row", g_PerfectJumps[client]);
			}
			else
				g_PerfectJumps[client] = 0;
			g_GroundTicks[client] = 0;
		}
	}
	g_WasOnGround[client] = onGround;
	return Plugin_Continue;
}

public void OnPlayerDeath(Event event, const char[] name, bool dontBroadcast)
{
	int attacker = GetClientOfUserId(event.GetInt("attacker"));
	int victim = GetClientOfUserId(event.GetInt("userid"));
	if (attacker < 1 || attacker == victim || !Watched(attacker))
		return;
	char weapon[32];
	event.GetString("weapon", weapon, sizeof(weapon));
	if (StrContains(weapon, "knife") != -1 || StrContains(weapon, "grenade") != -1 || StrEqual(weapon, "inferno"))
		return;
	// the aim jumped more than 45 degrees in one of the last three ticks before the kill
	float snap = 0.0;
	for (int i = 0; i < 3; i++)
		if (g_Snap[attacker][i] > snap)
			snap = g_Snap[attacker][i];
	if (snap < 45.0 || GetGameTickCount() - g_SnapTick[attacker] > 3)
		return;
	float now = GetEngineTime();
	if (g_SnapKills[attacker] == 0 || now - g_FirstSnapKill[attacker] > 30.0)
	{
		g_SnapKills[attacker] = 0;
		g_FirstSnapKill[attacker] = now;
	}
	if (++g_SnapKills[attacker] >= 4)
		Detect(attacker, Check_Snap, "%d kills right after a %.0f degree snap in 30 s", g_SnapKills[attacker], snap);
}

public Action Timer_Second(Handle timer)
{
	float limit = 1.3 / GetTickInterval();
	for (int i = 1; i <= MaxClients; i++)
	{
		// the angle check counts per second, so a single bad packet does nothing
		g_BadAngles[i] = 0;
		if (!IsClientInGame(i) || !Watched(i))
		{
			g_Cmds[i] = 0;
			g_FastSeconds[i] = 0;
			continue;
		}
		// lag sends a burst and then less, so only several fast seconds in a row count
		if (g_Cmds[i] > limit)
		{
			if (++g_FastSeconds[i] >= 4)
				Detect(i, Check_Speed, "%d commands a second at tickrate %.0f", g_Cmds[i], 1.0 / GetTickInterval());
		}
		else
			g_FastSeconds[i] = 0;
		g_Cmds[i] = 0;
	}
	return Plugin_Continue;
}

public Action Timer_Cvars(Handle timer)
{
	for (int i = 1; i <= MaxClients; i++)
	{
		if (IsClientInGame(i) && Watched(i))
		{
			QueryClientConVar(i, "r_drawothermodels", OnCvar, 1);
			QueryClientConVar(i, "mat_wireframe", OnCvar, 0);
			QueryClientConVar(i, "mat_fullbright", OnCvar, 0);
		}
	}
	return Plugin_Continue;
}

public void OnCvar(QueryCookie cookie, int client, ConVarQueryResult result, const char[] cvar, const char[] value, any expected)
{
	if (result != ConVarQuery_Okay || !IsClientInGame(client) || !Watched(client))
		return;
	if (StringToInt(value) != expected)
		Detect(client, Check_Cvar, "%s %s", cvar, value);
}

void Detect(int client, Check check, const char[] format, any ...)
{
	// one action per check and player each minute, so a log does not fill up
	float now = GetEngineTime();
	if (g_LastAction[client][check] != 0.0 && now - g_LastAction[client][check] < 60.0)
		return;
	g_LastAction[client][check] = now;
	char detail[128], extra[192], id[32], what[32];
	VFormat(detail, sizeof(detail), format, 4);
	int action = g_Action.IntValue;
	strcopy(what, sizeof(what), action == 2 ? "ban" : action == 1 ? "kick" : "log");
	Format(extra, sizeof(extra), "%s|%s|%s", g_CheckNames[check], what, detail);
	if (!SteamId(client, id, sizeof(id)))
		strcopy(id, sizeof(id), "");
	WriteEvent(id, client, extra);
	LogAction(-1, client, "anticheat: \"%L\" %s (%s), action %s", client, g_CheckNames[check], detail, what);
	if (action == 2)
	{
		char reason[64];
		Format(reason, sizeof(reason), "Anticheat: %s", g_CheckNames[check]);
		BanClient(client, g_BanMinutes.IntValue, BANFLAG_AUTO, reason, reason, "csls_anticheat");
	}
	else if (action == 1)
		KickClient(client, "%T", "ac_kicked", client, g_CheckNames[check]);
}

// The same SteamID as csls_events, computed from the account id since SourceMod does not give one on LAN.
bool SteamId(int client, char[] out, int len)
{
	int account = GetSteamAccountID(client, false);
	if (account <= 0)
		return false;
	Format(out, len, "STEAM_1:%d:%d", account & 1, account >>> 1);
	return true;
}

// A line for the app's agent, escaped as in csls_events so a name cannot add fields or lines.
void WriteEvent(const char[] id, int client, const char[] extra)
{
	char name[MAX_NAME_LENGTH], n[256], x[256];
	GetClientName(client, name, sizeof(name));
	Escape(name, n, sizeof(n));
	Escape(extra, x, sizeof(x));
	File f = OpenFile(g_Path, "a");
	if (f == null)
		return;
	f.WriteLine("{\"t\":%d,\"e\":\"ac\",\"id\":\"%s\",\"name\":\"%s\",\"extra\":\"%s\"}", GetTime(), id, n, x);
	delete f;
}

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
