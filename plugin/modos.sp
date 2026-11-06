// SPDX-License-Identifier: GPL-3.0-or-later
// Modos de juego contra bots para CS:GO Legacy.
// Al cargar el mapa ejecuta: modo_base.cfg -> modo_clasico.cfg (rondas clásicas) -> modo_tipo_<modo_tipo>.cfg -> modo_<modo_nombre>.cfg -> modo_extra
#pragma semicolon 1
#include <sourcemod>
#include <sdktools>
#include <cstrike>
#include <sdkhooks>
#pragma newdecls required

public Plugin myinfo = { name = "Modos", author = "xsteban1410", version = "4.0" };

#define T_IDX 0
#define CT_IDX 1

ConVar g_Nombre, g_Tipo, g_GameType, g_Humanos, g_Extra, g_Retake, g_Sitio, g_Pausa, g_HumanTeam;
ConVar g_Quitar[2], g_Arma[2], g_Pistola[2], g_Objeto[2], g_Vida[2], g_Velocidad[2], g_Color[2];

int g_Site;                 // 0 = A, 1 = B en la ronda actual
float g_SiteCenter[3];

#include "modos_extras.sp"
#include "modos_zombies.sp"

public void OnPluginStart()
{
	LoadTranslations("csls.phrases");
	g_Nombre  = CreateConVar("modo_nombre", "awp", "Modo activo (cfg/modo_<nombre>.cfg)");
	g_Tipo    = CreateConVar("modo_tipo", "", "Ajustes del tipo de partida (cfg/modo_tipo_<nombre>.cfg; vacio = ninguno)");
	g_GameType = FindConVar("game_type");
	g_Humanos = CreateConVar("modo_humanos", "2", "Humanos en equipo para acabar el calentamiento (0 = no controlar)");
	g_Extra   = CreateConVar("modo_extra", "", "Comandos que se ejecutan despues del cfg del modo (separados por ;)");
	g_Retake  = CreateConVar("modo_retake", "0", "1 = bomba plantada al empezar la ronda y T apareciendo en el sitio");
	g_Sitio   = CreateConVar("modo_sitio", "random", "Sitio del retake: a, b o random");

	char team[2][4] = { "t", "ct" }, name[48];
	for (int i = 0; i < 2; i++)
	{
		Format(name, sizeof(name), "modo_%s_quitar", team[i]);
		g_Quitar[i] = CreateConVar(name, "0", "1 = al aparecer quitar todo menos el cuchillo y dar las armas del modo");
		Format(name, sizeof(name), "modo_%s_arma", team[i]);
		g_Arma[i] = CreateConVar(name, "", "Arma principal al aparecer");
		Format(name, sizeof(name), "modo_%s_pistola", team[i]);
		g_Pistola[i] = CreateConVar(name, "", "Pistola al aparecer");
		Format(name, sizeof(name), "modo_%s_objeto", team[i]);
		g_Objeto[i] = CreateConVar(name, "", "Objeto extra al aparecer (granada, zeus, item_defuser...)");
		Format(name, sizeof(name), "modo_%s_vida", team[i]);
		g_Vida[i] = CreateConVar(name, "0", "Vida al aparecer (0 = normal)");
		Format(name, sizeof(name), "modo_%s_velocidad", team[i]);
		g_Velocidad[i] = CreateConVar(name, "1.0", "Multiplicador de velocidad");
		Format(name, sizeof(name), "modo_%s_color", team[i]);
		g_Color[i] = CreateConVar(name, "", "Color RGB del jugador, ej: 80,255,80 (vacio = normal)");
	}

	Extras_Start();
	Zombies_Start();
	for (int i = 1; i <= MaxClients; i++)
		if (IsClientInGame(i))
			OnClientPutInServer(i);

	g_Pausa = FindConVar("mp_warmup_pausetimer");
	g_HumanTeam = FindConVar("mp_humanteam");
	HookEvent("player_spawn", OnSpawn);
	HookEvent("round_prestart", OnRoundPreStart);
	HookEvent("round_freeze_end", OnFreezeEnd);
	CreateTimer(1.0, CheckWarmup, _, TIMER_REPEAT);
	RegServerCmd("modo_sitios", CmdSitios, "Muestra el centro de los sitios A/B y un punto de aparicion de prueba");
	// modo inicial que escribe csgo-legacy-server start (solo al cargar el plugin)
	ServerCommand("exec modo_inicio");
	ServerExecute();
}

public void OnMapStart()
{
	Zombies_MapStart();
}

public void OnClientPutInServer(int c)
{
	SDKHook(c, SDKHook_OnTakeDamage, OnTakeDamage);
	SDKHook(c, SDKHook_OnTakeDamagePost, Zombies_TakeDamagePost);
}

public Action OnTakeDamage(int victim, int &attacker, int &inflictor, float &damage, int &damagetype)
{
	if (Extras_TakeDamage(victim, attacker, inflictor, damage, damagetype) == Plugin_Changed)
		return Plugin_Changed;
	return Zombies_TakeDamage(victim, attacker, inflictor, damage, damagetype);
}

public void OnConfigsExecuted()
{
	char n[32], tipo[32], extra[512], cmd[128] = "exec modo_base";
	g_Nombre.GetString(n, sizeof(n));
	g_Tipo.GetString(tipo, sizeof(tipo));
	g_Extra.GetString(extra, sizeof(extra));
	if (g_GameType.IntValue == 0)  // los demás (DM, Carrera, Guardián, Danger Zone...) traen sus propias reglas
		StrCat(cmd, sizeof(cmd), "; exec modo_clasico");
	else if (g_GameType.IntValue == 4)  // las reglas de Guardián van por mapa
	{
		char map[64];
		GetCurrentMap(map, sizeof(map));
		Format(cmd, sizeof(cmd), "%s; exec guardian_%s", cmd, map);
	}
	if (tipo[0])
		Format(cmd, sizeof(cmd), "%s; exec modo_tipo_%s", cmd, tipo);
	ServerCommand("%s; exec modo_%s; %s", cmd, n, extra);
	// Sin ServerExecute: se ejecuta en cola, despues del gamemode_*.cfg de Valve (que pisaria bot_quota etc.).
	// Con el servidor vacio e hibernando, la cola se procesa cuando entra alguien.
}

int TeamIdx(int team) { return team == CS_TEAM_T ? T_IDX : CT_IDX; }

// --- calentamiento: esperar a N humanos -------------------------------------

int CountHumans()
{
	int h = 0;
	for (int i = 1; i <= MaxClients; i++)
		if (IsClientInGame(i) && !IsFakeClient(i) && GetClientTeam(i) >= CS_TEAM_T)
			h++;
	return h;
}

public Action CheckWarmup(Handle t)
{
	int need = g_Humanos.IntValue;
	if (FindEntityByClassname(-1, "cs_gamerules") == -1 || !GameRules_GetProp("m_bWarmupPeriod"))
		return Plugin_Continue;
	if (need <= 0)
	{
		g_Pausa.SetInt(0);  // sin control: calentamiento normal por tiempo
		return Plugin_Continue;
	}
	int h = CountHumans();
	if (h >= need)
	{
		g_Pausa.SetInt(0);
		ServerCommand("mp_warmup_end");
		PrintToChatAll(" \x04[Modos]\x01 %t", "match_starts", h);
	}
	else
	{
		g_Pausa.SetInt(1);
		PrintHintTextToAll("%t", "warmup_waiting", h, need);
	}
	return Plugin_Continue;
}

// --- equipamiento al aparecer -------------------------------------------------

public void OnSpawn(Event e, const char[] name, bool dontBroadcast)
{
	CreateTimer(0.2, Equip, e.GetInt("userid"));
}

void Give(int c, ConVar cv)
{
	char w[32];
	cv.GetString(w, sizeof(w));
	if (w[0])
		GivePlayerItem(c, w);
}

// Si el modo obliga a los humanos a un equipo (mp_humanteam), los mueve.
bool FixHumanTeam(int c)
{
	if (IsFakeClient(c) || Zombies_Infectado(c))
		return false;
	char ht[8];
	g_HumanTeam.GetString(ht, sizeof(ht));
	int want = StrEqual(ht, "CT", false) ? CS_TEAM_CT : StrEqual(ht, "T", false) ? CS_TEAM_T : 0;
	if (want == 0 || GetClientTeam(c) == want)
		return false;
	CS_SwitchTeam(c, want);
	CS_RespawnPlayer(c);
	return true;
}

public Action Equip(Handle t, any userid)
{
	int c = GetClientOfUserId(userid);
	if (c < 1 || !IsClientInGame(c) || !IsPlayerAlive(c) || FixHumanTeam(c))
		return Plugin_Stop;
	int i = TeamIdx(GetClientTeam(c));
	if (g_Quitar[i].BoolValue)
	{
		for (int s = 0; s <= 4; s++)
		{
			if (s == CS_SLOT_KNIFE) continue;
			int w;
			while ((w = GetPlayerWeaponSlot(c, s)) != -1)
			{
				RemovePlayerItem(c, w);
				AcceptEntityInput(w, "Kill");
			}
		}
		Give(c, g_Arma[i]);
		Give(c, g_Pistola[i]);
		Give(c, g_Objeto[i]);
		SetEntProp(c, Prop_Send, "m_ArmorValue", 100);
		SetEntProp(c, Prop_Send, "m_bHasHelmet", 1);
	}
	if (g_Vida[i].IntValue > 0)
		SetEntityHealth(c, g_Vida[i].IntValue);
	SetEntPropFloat(c, Prop_Send, "m_flLaggedMovementValue", g_Velocidad[i].FloatValue);
	SetEntPropFloat(c, Prop_Send, "m_flModelScale", 1.0);  // por si fue jefe o tanque

	char col[32], rgb[3][8];
	g_Color[i].GetString(col, sizeof(col));
	if (col[0] && ExplodeString(col, ",", rgb, 3, 8) == 3)
	{
		SetEntityRenderMode(c, RENDER_TRANSCOLOR);
		SetEntityRenderColor(c, StringToInt(rgb[0]), StringToInt(rgb[1]), StringToInt(rgb[2]), 255);
	}
	else
		SetEntityRenderColor(c, 255, 255, 255, 255);

	if (g_Retake.BoolValue && GetClientTeam(c) == CS_TEAM_T && !GameRules_GetProp("m_bWarmupPeriod"))
	{
		float p[3];
		if (FindSitePoint(p))
			TeleportEntity(c, p, NULL_VECTOR, NULL_VECTOR);
	}
	Extras_Equip(c);
	Zombies_Equip(c);
	return Plugin_Stop;
}

// --- retakes: bomba ya plantada en A o B ----------------------------------------

public void OnRoundPreStart(Event e, const char[] name, bool dontBroadcast)
{
	Extras_PreStart();
	Zombies_PreStart();
	if (!g_Retake.BoolValue)
		return;
	char s[8];
	g_Sitio.GetString(s, sizeof(s));
	g_Site = StrEqual(s, "a", false) ? 0 : StrEqual(s, "b", false) ? 1 : GetRandomInt(0, 1);
	int res = FindEntityByClassname(-1, "cs_player_manager");
	if (res != -1)
		GetEntPropVector(res, Prop_Send, g_Site == 0 ? "m_bombsiteCenterA" : "m_bombsiteCenterB", g_SiteCenter);
}

public Action CmdSitios(int args)
{
	int res = FindEntityByClassname(-1, "cs_player_manager");
	if (res == -1)
	{
		PrintToServer("No hay cs_player_manager");
		return Plugin_Handled;
	}
	float a[3], b[3], p[3];
	GetEntPropVector(res, Prop_Send, "m_bombsiteCenterA", a);
	GetEntPropVector(res, Prop_Send, "m_bombsiteCenterB", b);
	PrintToServer("A: %.0f %.0f %.0f | B: %.0f %.0f %.0f", a[0], a[1], a[2], b[0], b[1], b[2]);
	g_SiteCenter = a;
	bool ok = FindSitePoint(p);
	PrintToServer("Punto en A: %s %.0f %.0f %.0f", ok ? "ok" : "NO", p[0], p[1], p[2]);
	return Plugin_Handled;
}

public bool NoPlayers(int ent, int mask) { return ent > MaxClients; }

// Punto de suelo libre, a la vista del centro del sitio y a menos de 350 unidades.
bool FindSitePoint(float out[3])
{
	float top[3], under[3], floorZ;
	top = g_SiteCenter;
	top[2] += 40.0;
	under = g_SiteCenter;
	under[2] -= 600.0;
	TR_TraceRayFilter(top, under, MASK_PLAYERSOLID, RayType_EndPoint, NoPlayers);
	if (!TR_DidHit())
		return false;
	TR_GetEndPosition(under);
	floorZ = under[2];
	for (int i = 0; i < 150; i++)
	{
		float s[3], down[3], f[3];
		s[0] = g_SiteCenter[0] + GetRandomFloat(-350.0, 350.0);
		s[1] = g_SiteCenter[1] + GetRandomFloat(-350.0, 350.0);
		s[2] = top[2];
		TR_TraceRayFilter(top, s, MASK_PLAYERSOLID, RayType_EndPoint, NoPlayers);
		if (TR_DidHit())
			continue;
		down = s;
		down[2] -= 400.0;
		TR_TraceRayFilter(s, down, MASK_PLAYERSOLID, RayType_EndPoint, NoPlayers);
		if (!TR_DidHit())
			continue;
		TR_GetEndPosition(f);
		if (FloatAbs(f[2] - floorZ) > 48.0)  // mismo nivel que el sitio, no un piso de abajo
			continue;
		f[2] += 2.0;
		TR_TraceHull(f, f, view_as<float>({-16.0, -16.0, 0.0}), view_as<float>({16.0, 16.0, 72.0}), MASK_PLAYERSOLID);
		if (TR_DidHit())
			continue;
		out = f;
		return true;
	}
	return false;
}

public void OnFreezeEnd(Event e, const char[] name, bool dontBroadcast)
{
	Extras_FreezeEnd();
	if (!g_Retake.BoolValue || GameRules_GetProp("m_bWarmupPeriod"))
		return;
	float p[3];
	if (!FindSitePoint(p))
	{
		LogError("Retake: no encontré suelo libre en el sitio %c", g_Site == 0 ? 'A' : 'B');
		return;
	}
	int planter = 0;
	for (int i = 1; i <= MaxClients && !planter; i++)
		if (IsClientInGame(i) && IsPlayerAlive(i) && GetClientTeam(i) == CS_TEAM_T)
			planter = i;

	int c4 = CreateEntityByName("planted_c4");
	if (c4 == -1)
		return;
	GameRules_SetProp("m_bBombPlanted", 1);
	SetEntProp(c4, Prop_Send, "m_bBombTicking", 1);
	SetEntProp(c4, Prop_Send, "m_nBombSite", g_Site);
	if (DispatchSpawn(c4))
	{
		ActivateEntity(c4);
		TeleportEntity(c4, p, NULL_VECTOR, NULL_VECTOR);
		Event ev = CreateEvent("bomb_planted");
		if (ev != null)
		{
			ev.SetInt("userid", planter ? GetClientUserId(planter) : 0);
			ev.SetInt("site", g_Site);
			ev.Fire();
		}
		PrintToChatAll(" \x04[Retake]\x01 %t", "bomb_planted_at", g_Site == 0 ? 'A' : 'B');
	}
}
