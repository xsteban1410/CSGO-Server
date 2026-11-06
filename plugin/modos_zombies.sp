// SPDX-License-Identifier: GPL-3.0-or-later
// Zombie waves: zombie types, knockback, infection and fog. Included by modos.sp.
// Each wave is a round: zombies (T) keep respawning and humans (CT) must survive the time.
// If the CTs win it moves to the next wave; if they all die, it repeats. After the last wave, victory.

#define Z_NORMAL   0
#define Z_CORREDOR 1
#define Z_TANQUE   2

ConVar g_Z, g_ZOleadas, g_ZBase, g_ZInc, g_ZVida, g_ZVidaOleada, g_ZVelocidad, g_ZRetroceso, g_ZInfeccion, g_ZNiebla;
int g_Oleada = 1;
int g_ZTipo[MAXPLAYERS + 1];
bool g_Infectado[MAXPLAYERS + 1];
int g_Niebla = INVALID_ENT_REFERENCE;

float g_ZMulVida[] = { 1.0, 0.5, 3.0 };
float g_ZMulVel[]  = { 1.0, 1.5, 0.8 };
float g_ZEscala[]  = { 1.0, 1.0, 1.25 };
int   g_ZColor[][] = { { 80, 255, 80 }, { 255, 230, 60 }, { 170, 40, 40 } };

void Zombies_Start()
{
	g_Z           = CreateConVar("modo_zombies", "0", "1 = zombie wave mode (Ts are zombies)");
	g_ZOleadas    = CreateConVar("modo_z_oleadas", "5", "Waves to win");
	g_ZBase       = CreateConVar("modo_z_base", "6", "Zombies in the first wave");
	g_ZInc        = CreateConVar("modo_z_incremento", "2", "Extra zombies each wave");
	g_ZVida       = CreateConVar("modo_z_vida", "250", "Health of a normal zombie in the first wave");
	g_ZVidaOleada = CreateConVar("modo_z_vida_oleada", "75", "Extra health per wave");
	g_ZVelocidad  = CreateConVar("modo_z_velocidad", "1.2", "Base zombie speed");
	g_ZRetroceso  = CreateConVar("modo_z_retroceso", "1.0", "Force with which bullets push zombies (0 = none)");
	g_ZInfeccion  = CreateConVar("modo_z_infeccion", "1", "1 = a human killed by a zombie becomes a zombie");
	g_ZNiebla     = CreateConVar("modo_z_niebla", "1", "1 = niebla");
	RegServerCmd("modo_estado", CmdEstado, "Shows the wave and the health, speed and size of each player");
	HookEvent("round_start", Zombies_RoundStart);
	HookEvent("round_end", Zombies_RoundEnd);
	HookEvent("player_death", Zombies_Death);
}

void Zombies_MapStart()
{
	g_Oleada = 1;
	g_Niebla = INVALID_ENT_REFERENCE;
}

bool EsZombi(int c) { return g_Z.BoolValue && c >= 1 && c <= MaxClients && IsClientInGame(c) && GetClientTeam(c) == CS_TEAM_T; }

// Called from FixHumanTeam: an infected player stays on T until the next wave.
bool Zombies_Infectado(int c) { return g_Z.BoolValue && g_Infectado[c]; }

void Zombies_PreStart()
{
	if (!g_Z.BoolValue)
		return;
	for (int i = 1; i <= MaxClients; i++)
		g_Infectado[i] = false;
	if (!GameRules_GetProp("m_bWarmupPeriod"))
		ServerCommand("bot_quota %d", g_ZBase.IntValue + g_ZInc.IntValue * (g_Oleada - 1));
}

public void Zombies_RoundStart(Event e, const char[] name, bool dontBroadcast)
{
	if (!g_Z.BoolValue)
		return;
	Niebla();
	if (!GameRules_GetProp("m_bWarmupPeriod"))
		PrintCenterTextAll("%t", "wave_of", g_Oleada, g_ZOleadas.IntValue);
}

public void Zombies_RoundEnd(Event e, const char[] name, bool dontBroadcast)
{
	if (!g_Z.BoolValue || GameRules_GetProp("m_bWarmupPeriod"))
		return;
	if (e.GetInt("winner") != CS_TEAM_CT)
	{
		PrintToChatAll(" \x04[Zombies]\x01 %t", "zombies_ate_you", g_Oleada);
		return;
	}
	if (g_Oleada >= g_ZOleadas.IntValue)
	{
		PrintToChatAll(" \x04[Zombies]\x01 %t", "zombies_victory_chat", g_Oleada);
		PrintCenterTextAll("%t", "zombies_victory_center", g_Oleada);
		g_Oleada = 1;
		ServerCommand("mp_restartgame 15");
		return;
	}
	g_Oleada++;
	PrintToChatAll(" \x04[Zombies]\x01 %t", "wave_cleared", g_Oleada);
}

// Zombie type, health, speed, size and color. Called from Equip (after giving weapons).
void Zombies_Equip(int c)
{
	if (!g_Z.BoolValue)
		return;
	if (g_Niebla != INVALID_ENT_REFERENCE && EntRefToEntIndex(g_Niebla) != INVALID_ENT_REFERENCE)
		SetEntPropEnt(c, Prop_Send, "m_PlayerFog.m_hCtrl", EntRefToEntIndex(g_Niebla));
	if (GetClientTeam(c) != CS_TEAM_T)
		return;
	int r = GetRandomInt(1, 100), tipo = Z_NORMAL;
	if (IsFakeClient(c))
	{
		if (g_Oleada >= 2 && r <= 15)
			tipo = Z_TANQUE;
		else if (r > 70)
			tipo = Z_CORREDOR;
	}
	g_ZTipo[c] = tipo;
	int vida = RoundToCeil((g_ZVida.IntValue + g_ZVidaOleada.IntValue * (g_Oleada - 1)) * g_ZMulVida[tipo]);
	SetEntityHealth(c, vida);
	SetEntPropFloat(c, Prop_Send, "m_flLaggedMovementValue", g_ZVelocidad.FloatValue * g_ZMulVel[tipo]);
	SetEntPropFloat(c, Prop_Send, "m_flModelScale", g_ZEscala[tipo]);
	SetEntProp(c, Prop_Send, "m_ArmorValue", 0);
	SetEntityRenderMode(c, RENDER_TRANSCOLOR);
	SetEntityRenderColor(c, g_ZColor[tipo][0], g_ZColor[tipo][1], g_ZColor[tipo][2], 255);
	if (!IsFakeClient(c))
		PrintCenterText(c, "%t", "you_are_zombie");
}

// Fuego x3 contra zombis.
public Action Zombies_TakeDamage(int victim, int &attacker, int &inflictor, float &damage, int &damagetype)
{
	if (EsZombi(victim) && (damagetype & DMG_BURN))
	{
		damage *= 3.0;
		return Plugin_Changed;
	}
	return Plugin_Continue;
}

// Knockback: the bullet pushes the zombie along the shot (shotguns and AWP more).
public void Zombies_TakeDamagePost(int victim, int attacker, int inflictor, float damage, int damagetype)
{
	float k = g_ZRetroceso.FloatValue;
	if (k <= 0.0 || !EsZombi(victim) || attacker < 1 || attacker > MaxClients || !IsClientInGame(attacker)
		|| GetClientTeam(attacker) != CS_TEAM_CT || !(damagetype & DMG_BULLET))
		return;
	char w[32];
	GetClientWeapon(attacker, w, sizeof(w));
	if (StrEqual(w, "weapon_nova") || StrEqual(w, "weapon_xm1014") || StrEqual(w, "weapon_mag7")
		|| StrEqual(w, "weapon_sawedoff") || StrEqual(w, "weapon_awp"))
		k *= 2.0;
	if (g_ZTipo[victim] == Z_TANQUE)
		k /= 3.0;
	float from[3], to[3], dir[3], vel[3];
	GetClientEyePosition(attacker, from);
	GetClientAbsOrigin(victim, to);
	MakeVectorFromPoints(from, to, dir);
	dir[2] = 0.0;
	NormalizeVector(dir, dir);
	GetEntPropVector(victim, Prop_Data, "m_vecVelocity", vel);
	float f = damage * 6.0 * k;
	if (f > 600.0)
		f = 600.0;
	vel[0] += dir[0] * f;
	vel[1] += dir[1] * f;
	vel[2] += 0.15 * f;
	TeleportEntity(victim, NULL_VECTOR, NULL_VECTOR, vel);
}

// Infection: a human killed by a zombie becomes a zombie.
public void Zombies_Death(Event e, const char[] name, bool dontBroadcast)
{
	if (!g_Z.BoolValue || !g_ZInfeccion.BoolValue || GameRules_GetProp("m_bWarmupPeriod"))
		return;
	int v = GetClientOfUserId(e.GetInt("userid")), a = GetClientOfUserId(e.GetInt("attacker"));
	if (v < 1 || IsFakeClient(v) || GetClientTeam(v) != CS_TEAM_CT || !EsZombi(a))
		return;
	// If it was the last living human the round is already lost: no need to convert
	for (int i = 1; i <= MaxClients; i++)
		if (i != v && IsClientInGame(i) && IsPlayerAlive(i) && GetClientTeam(i) == CS_TEAM_CT)
		{
			g_Infectado[v] = true;
			char vn[MAX_NAME_LENGTH];
			GetClientName(v, vn, sizeof(vn));
			PrintToChatAll(" \x04[Zombies]\x01 %t", "infected", vn);
			CreateTimer(2.0, Infectar, GetClientUserId(v));
			return;
		}
}

public Action Infectar(Handle t, any userid)
{
	int c = GetClientOfUserId(userid);
	if (c >= 1 && g_Infectado[c] && IsClientInGame(c) && !IsPlayerAlive(c))
	{
		CS_SwitchTeam(c, CS_TEAM_T);
		CS_RespawnPlayer(c);
	}
	return Plugin_Stop;
}

// Fog: uses the map's env_fog_controller or creates one.
void Niebla()
{
	if (!g_ZNiebla.BoolValue)
		return;
	int f = FindEntityByClassname(-1, "env_fog_controller");
	if (f == -1)
	{
		f = CreateEntityByName("env_fog_controller");
		if (f == -1)
			return;
		DispatchKeyValue(f, "targetname", "modo_niebla");
		DispatchKeyValue(f, "fogenable", "1");
		DispatchKeyValue(f, "fogblend", "0");
		DispatchKeyValue(f, "spawnflags", "1");
		DispatchSpawn(f);
	}
	SetEntProp(f, Prop_Send, "m_fog.enable", 1);
	SetEntPropFloat(f, Prop_Send, "m_fog.start", 150.0);
	SetEntPropFloat(f, Prop_Send, "m_fog.end", 1400.0);
	SetEntPropFloat(f, Prop_Send, "m_fog.maxdensity", 0.85);
	SetEntProp(f, Prop_Send, "m_fog.colorPrimary", 0xFF5A6E64);  // ABGR: gris verdoso
	AcceptEntityInput(f, "TurnOn");
	g_Niebla = EntIndexToEntRef(f);
	for (int i = 1; i <= MaxClients; i++)
		if (IsClientInGame(i))
			SetEntPropEnt(i, Prop_Send, "m_PlayerFog.m_hCtrl", f);
}

public Action CmdEstado(int args)
{
	PrintToServer("Oleada %d/%d calentamiento=%d", g_Oleada, g_ZOleadas.IntValue, GameRules_GetProp("m_bWarmupPeriod"));
	PrintToServer("round=%d T=%d score=%d CT=%d score=%d clutch=%d", GameRules_GetProp("m_totalRoundsPlayed"), GetTeamClientCount(CS_TEAM_T), CS_GetTeamScore(CS_TEAM_T),
		GetTeamClientCount(CS_TEAM_CT), CS_GetTeamScore(CS_TEAM_CT), g_ClutchBots);
	for (int i = 1; i <= MaxClients; i++)
		if (IsClientInGame(i) && IsPlayerAlive(i))
		{
			float p[3];
			GetClientAbsOrigin(i, p);
			PrintToServer("%N team=%d health=%d armor=%d speed=%.2f scale=%.2f type=%d pos=%.0f,%.0f,%.0f", i, GetClientTeam(i), GetClientHealth(i), GetEntProp(i, Prop_Send, "m_ArmorValue"),
				GetEntPropFloat(i, Prop_Send, "m_flLaggedMovementValue"), GetEntPropFloat(i, Prop_Send, "m_flModelScale"), g_ZTipo[i], p[0], p[1], p[2]);
		}
	return Plugin_Handled;
}
