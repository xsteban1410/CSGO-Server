// SPDX-License-Identifier: GPL-3.0-or-later
// Random cheats: glowing power-ups around the map; whoever touches one gets a random cheat for a while.
// Separate plugin from modos.sp; enabled with modo_cheats 1 (reset by modo_base.cfg).
#pragma semicolon 1
#pragma newdecls required

#include <sourcemod>
#include <sdktools>
#include <sdkhooks>
#include <cstrike>

public Plugin myinfo = { name = "Modes: random cheats", author = "xsteban1410", version = "1.0" };

#define EF_BONEMERGE          (1 << 0)
#define EF_NOSHADOW           (1 << 4)
#define EF_NORECEIVESHADOW    (1 << 6)
#define EF_PARENT_ANIMATES    (1 << 9)
#define MAX_PU 32
#define MAX_PUNTOS 256

enum Cheat
{
	C_Ninguno = -1,
	C_Wallhack,
	C_Aimbot,
	C_Municion,
	C_Velocidad,
	C_Gravedad,
	C_Invencible,
	C_Invisible,
	C_UnTiro,
	C_Bhop,
	C_Superpulso,
	C_Regeneracion,
	C_Total
}

char g_Nombres[C_Total][] = {
	"wallhack", "aimbot", "municion", "velocidad", "gravedad", "invencible",
	"invisible", "untiro", "bhop", "superpulso", "regeneracion"
};
// cheat names are in translations/csls.phrases.txt (cheat_0…)

// possible power-up models (the first that exists)
char g_Modelos[][] = {
	"models/props_survival/upgrades/upgrade_dz_helmet.mdl",
	"models/weapons/w_eq_healthshot_dropped.mdl",
	"models/weapons/w_ied_dropped.mdl"
};
char g_Modelo[PLATFORM_MAX_PATH];

ConVar g_On, g_Duracion, g_Cantidad, g_Reaparicion, g_Bots, g_Lista;

Cheat g_Cheat[MAXPLAYERS + 1] = { C_Ninguno, ... };
float g_Hasta[MAXPLAYERS + 1];
float g_VelAntes[MAXPLAYERS + 1];
ArrayList g_Glows[MAXPLAYERS + 1];

int g_Pu[MAX_PU];          // references to the power-up entities
float g_PuVuelve[MAX_PU];   // when a taken one comes back
ArrayList g_Puntos;         // possible positions (spawns + places players pass through)

public void OnPluginStart()
{
	LoadTranslations("csls.phrases");
	g_On          = CreateConVar("modo_cheats", "0", "1 = power-ups with random cheats around the map");
	g_Duracion    = CreateConVar("modo_cheats_duracion", "30", "Seconds each cheat lasts");
	g_Cantidad    = CreateConVar("modo_cheats_cantidad", "6", "Power-ups on the map at once", _, true, 1.0, true, float(MAX_PU));
	g_Reaparicion = CreateConVar("modo_cheats_reaparicion", "20", "Seconds until a taken power-up respawns");
	g_Bots        = CreateConVar("modo_cheats_bots", "1", "1 = bots also pick up power-ups");
	g_Lista       = CreateConVar("modo_cheats_lista", "", "Allowed cheats separated by commas (empty = all): wallhack,aimbot,municion,velocidad,gravedad,invencible,invisible,untiro,bhop,superpulso,regeneracion");
	g_On.AddChangeHook(OnToggle);
	g_Puntos = new ArrayList(3);
	HookEvent("round_start", OnRoundStart, EventHookMode_PostNoCopy);
	HookEvent("player_death", OnDeath);
	HookEvent("player_jump", OnJump);
	HookEvent("weapon_fire", OnFire);
	RegServerCmd("cheats_estado", CmdEstado, "Shows the active cheat of each player and the power-ups");
	CreateTimer(0.1, Tick, _, TIMER_REPEAT);
	CreateTimer(1.0, TickLento, _, TIMER_REPEAT);
	for (int c = 1; c <= MaxClients; c++)
		if (IsClientInGame(c))
			OnClientPutInServer(c);
}

public void OnMapStart()
{
	g_Modelo[0] = '\0';
	for (int i = 0; i < sizeof(g_Modelos); i++)
	{
		if (FileExists(g_Modelos[i], true))
		{
			strcopy(g_Modelo, sizeof(g_Modelo), g_Modelos[i]);
			PrecacheModel(g_Modelo, true);
			break;
		}
	}
	g_Puntos.Clear();
	for (int i = 0; i < MAX_PU; i++)
		g_Pu[i] = INVALID_ENT_REFERENCE;
}

public void OnClientPutInServer(int c)
{
	SDKHook(c, SDKHook_OnTakeDamage, OnTakeDamage);
	g_Cheat[c] = C_Ninguno;
}

public void OnClientDisconnect(int c)
{
	Quitar(c, false);
}

public void OnToggle(ConVar cv, const char[] viejo, const char[] nuevo)
{
	if (!cv.BoolValue)
	{
		for (int c = 1; c <= MaxClients; c++)
			if (IsClientInGame(c))
				Quitar(c, false);
		BorrarPowerups();
	}
}

// --- power-ups ----------------------------------------------------------------

void BorrarPowerups()
{
	for (int i = 0; i < MAX_PU; i++)
	{
		int e = EntRefToEntIndex(g_Pu[i]);
		if (e > MaxClients)
			RemoveEntity(e);
		g_Pu[i] = INVALID_ENT_REFERENCE;
		g_PuVuelve[i] = 0.0;
	}
}

void AnadirPunto(const float p[3])
{
	float q[3];
	for (int i = 0; i < g_Puntos.Length; i++)
	{
		g_Puntos.GetArray(i, q);
		if (GetVectorDistance(p, q) < 300.0)
			return;
	}
	if (g_Puntos.Length >= MAX_PUNTOS)
		g_Puntos.Erase(GetRandomInt(0, g_Puntos.Length - 1));
	g_Puntos.PushArray(p);
}

void PuntosDeAparicion()
{
	char cls[][] = { "info_player_terrorist", "info_player_counterterrorist", "info_deathmatch_spawn" };
	float p[3];
	for (int k = 0; k < sizeof(cls); k++)
	{
		int e = -1;
		while ((e = FindEntityByClassname(e, cls[k])) != -1)
		{
			GetEntPropVector(e, Prop_Data, "m_vecOrigin", p);
			AnadirPunto(p);
		}
	}
}

bool PuntoLibre(float out[3])
{
	if (g_Puntos.Length == 0)
		return false;
	float q[3];
	for (int intento = 0; intento < 12; intento++)
	{
		g_Puntos.GetArray(GetRandomInt(0, g_Puntos.Length - 1), out);
		bool cerca = false;
		for (int i = 0; i < MAX_PU && !cerca; i++)
		{
			int e = EntRefToEntIndex(g_Pu[i]);
			if (e <= MaxClients)
				continue;
			GetEntPropVector(e, Prop_Data, "m_vecOrigin", q);
			cerca = GetVectorDistance(out, q) < 250.0;
		}
		if (!cerca)
			return true;
	}
	return true;
}

int CrearPowerup(const float p[3])
{
	if (g_Modelo[0] == '\0')
		return -1;
	int e = CreateEntityByName("prop_dynamic_glow");
	if (e == -1)
		return -1;
	DispatchKeyValue(e, "model", g_Modelo);
	DispatchKeyValue(e, "solid", "0");
	DispatchKeyValue(e, "glowenabled", "1");
	DispatchKeyValue(e, "glowcolor", "255 200 0");
	DispatchKeyValue(e, "glowdist", "3000");
	DispatchKeyValue(e, "glowstyle", "0");
	DispatchSpawn(e);
	float pos[3];
	pos = p;
	pos[2] += 24.0;
	TeleportEntity(e, pos, NULL_VECTOR, NULL_VECTOR);
	return EntIndexToEntRef(e);
}

public void OnRoundStart(Event e, const char[] name, bool dontBroadcast)
{
	for (int c = 1; c <= MaxClients; c++)
		if (IsClientInGame(c))
			Quitar(c, false);
	// entities are removed when the round restarts: create them again
	for (int i = 0; i < MAX_PU; i++)
	{
		g_Pu[i] = INVALID_ENT_REFERENCE;
		g_PuVuelve[i] = 0.0;
	}
	if (g_On.BoolValue)
		PuntosDeAparicion();
}

// --- cheats -------------------------------------------------------------------

bool Permitido(Cheat ch)
{
	char lista[256];
	g_Lista.GetString(lista, sizeof(lista));
	TrimString(lista);
	if (lista[0] == '\0')
		return true;
	char partes[C_Total][24];
	int n = ExplodeString(lista, ",", partes, sizeof(partes), sizeof(partes[]));
	for (int i = 0; i < n; i++)
	{
		TrimString(partes[i]);
		if (StrEqual(partes[i], g_Nombres[ch], false))
			return true;
	}
	return false;
}

Cheat Sortear()
{
	Cheat ok[C_Total];
	int n = 0;
	for (int i = 0; i < view_as<int>(C_Total); i++)
		if (Permitido(view_as<Cheat>(i)))
			ok[n++] = view_as<Cheat>(i);
	return n == 0 ? C_Ninguno : ok[GetRandomInt(0, n - 1)];
}

void Dar(int c, Cheat ch)
{
	Quitar(c, false);
	if (ch == C_Ninguno)
		return;
	g_Cheat[c] = ch;
	g_Hasta[c] = GetGameTime() + g_Duracion.FloatValue;
	switch (ch)
	{
		case C_Velocidad:
		{
			g_VelAntes[c] = GetEntPropFloat(c, Prop_Send, "m_flLaggedMovementValue");
			SetEntPropFloat(c, Prop_Send, "m_flLaggedMovementValue", g_VelAntes[c] * 1.6);
		}
		case C_Gravedad: SetEntityGravity(c, 0.4);
		case C_Invencible: SetEntProp(c, Prop_Data, "m_takedamage", 0, 1);
		case C_Invisible:
		{
			SetEntityRenderMode(c, RENDER_TRANSALPHA);
			SetEntityRenderColor(c, 255, 255, 255, 12);
		}
		case C_Wallhack: CrearGlows(c);
	}
	EmitSoundToClient(c, "ui/item_drop3_rare.wav");
	char nombre[64];
	CheatName(ch, c, nombre, sizeof(nombre));
	PrintToChat(c, " \x04[Cheats]\x01 %t", "cheat_got", nombre, g_Duracion.IntValue);
}

void Quitar(int c, bool avisar)
{
	Cheat ch = g_Cheat[c];
	if (ch == C_Ninguno)
		return;
	g_Cheat[c] = C_Ninguno;
	if (IsClientInGame(c))
	{
		switch (ch)
		{
			case C_Velocidad: SetEntPropFloat(c, Prop_Send, "m_flLaggedMovementValue", g_VelAntes[c] > 0.0 ? g_VelAntes[c] : 1.0);
			case C_Gravedad: SetEntityGravity(c, 1.0);
			case C_Invencible: SetEntProp(c, Prop_Data, "m_takedamage", 2, 1);
			case C_Invisible:
			{
				SetEntityRenderMode(c, RENDER_NORMAL);
				SetEntityRenderColor(c, 255, 255, 255, 255);
			}
		}
		if (avisar)
		{
			char nombre[64];
			CheatName(ch, c, nombre, sizeof(nombre));
			PrintToChat(c, " \x04[Cheats]\x01 %t", "cheat_over", nombre);
		}
	}
	BorrarGlows(c);
}

// Wallhack: a glowing copy of each enemy model attached to them, visible only to the owner.
void CrearGlows(int c)
{
	BorrarGlows(c);
	g_Glows[c] = new ArrayList();
	char modelo[PLATFORM_MAX_PATH];
	for (int e = 1; e <= MaxClients; e++)
	{
		if (!IsClientInGame(e) || !IsPlayerAlive(e) || GetClientTeam(e) == GetClientTeam(c) && !FindConVar("mp_teammates_are_enemies").BoolValue || e == c)
			continue;
		GetClientModel(e, modelo, sizeof(modelo));
		int g = CreateEntityByName("prop_dynamic_glow");
		if (g == -1)
			continue;
		DispatchKeyValue(g, "model", modelo);
		DispatchKeyValue(g, "solid", "0");
		DispatchKeyValue(g, "glowenabled", "1");
		DispatchKeyValue(g, "glowcolor", "255 40 40");
		DispatchKeyValue(g, "glowdist", "10000");
		DispatchKeyValue(g, "glowstyle", "0");
		DispatchSpawn(g);
		SetEntProp(g, Prop_Send, "m_fEffects", EF_BONEMERGE | EF_NOSHADOW | EF_NORECEIVESHADOW | EF_PARENT_ANIMATES);
		SetVariantString("!activator");
		AcceptEntityInput(g, "SetParent", e, g);
		SetEntPropEnt(g, Prop_Send, "m_hOwnerEntity", c);
		SDKHook(g, SDKHook_SetTransmit, GlowTransmit);
		g_Glows[c].Push(EntIndexToEntRef(g));
	}
}

public Action GlowTransmit(int ent, int client)
{
	return GetEntPropEnt(ent, Prop_Send, "m_hOwnerEntity") == client ? Plugin_Continue : Plugin_Handled;
}

void BorrarGlows(int c)
{
	if (g_Glows[c] == null)
		return;
	for (int i = 0; i < g_Glows[c].Length; i++)
	{
		int g = EntRefToEntIndex(g_Glows[c].Get(i));
		if (g > MaxClients)
			RemoveEntity(g);
	}
	delete g_Glows[c];
}

// --- bucles -------------------------------------------------------------------

public Action Tick(Handle t)
{
	if (!g_On.BoolValue)
		return Plugin_Continue;
	float ahora = GetGameTime();
	int cantidad = g_Cantidad.IntValue;
	float p[3], q[3];
	for (int i = 0; i < cantidad; i++)
	{
		int e = EntRefToEntIndex(g_Pu[i]);
		if (e <= MaxClients)
		{
			if (ahora >= g_PuVuelve[i] && PuntoLibre(p))
				g_Pu[i] = CrearPowerup(p);
			continue;
		}
		// gira
		GetEntPropVector(e, Prop_Data, "m_vecOrigin", q);
		float ang[3];
		ang[1] = float(RoundToFloor(ahora * 120.0) % 360);
		TeleportEntity(e, NULL_VECTOR, ang, NULL_VECTOR);
		for (int c = 1; c <= MaxClients; c++)
		{
			if (!IsClientInGame(c) || !IsPlayerAlive(c) || IsFakeClient(c) && !g_Bots.BoolValue)
				continue;
			GetClientAbsOrigin(c, p);
			p[2] += 24.0;
			if (GetVectorDistance(p, q) < 56.0)
			{
				RemoveEntity(e);
				g_Pu[i] = INVALID_ENT_REFERENCE;
				g_PuVuelve[i] = ahora + g_Reaparicion.FloatValue;
				Dar(c, Sortear());
				break;
			}
		}
	}
	return Plugin_Continue;
}

public Action TickLento(Handle t)
{
	if (!g_On.BoolValue)
		return Plugin_Continue;
	float ahora = GetGameTime();
	float p[3];
	for (int c = 1; c <= MaxClients; c++)
	{
		if (!IsClientInGame(c) || !IsPlayerAlive(c))
			continue;
		// learn places people walk through (no navmesh: SourceMod does not expose it in CS:GO)
		if (GetEntityFlags(c) & FL_ONGROUND)
		{
			GetClientAbsOrigin(c, p);
			AnadirPunto(p);
		}
		Cheat ch = g_Cheat[c];
		if (ch == C_Ninguno)
			continue;
		int queda = RoundToCeil(g_Hasta[c] - ahora);
		if (queda <= 0)
		{
			Quitar(c, true);
			continue;
		}
		if (ch == C_Regeneracion)
		{
			int v = GetClientHealth(c);
			if (v < 150)
				SetEntityHealth(c, v + 10 > 150 ? 150 : v + 10);
		}
		if (!IsFakeClient(c))
		{
			char nombre[64];
			CheatName(ch, c, nombre, sizeof(nombre));
			PrintHintText(c, "<font color='#ffc800'>%s</font>: %d s", nombre, queda);
		}
	}
	return Plugin_Continue;
}

// --- effects driven by events ----------------------------------------------

public void OnDeath(Event e, const char[] name, bool dontBroadcast)
{
	int c = GetClientOfUserId(e.GetInt("userid"));
	if (c > 0)
		Quitar(c, false);
}

public void OnJump(Event e, const char[] name, bool dontBroadcast)
{
	int c = GetClientOfUserId(e.GetInt("userid"));
	if (c > 0 && g_Cheat[c] == C_Superpulso)
		RequestFrame(Impulso, GetClientUserId(c));
}

public void Impulso(int uid)
{
	int c = GetClientOfUserId(uid);
	if (c < 1 || !IsPlayerAlive(c))
		return;
	float v[3];
	GetEntPropVector(c, Prop_Data, "m_vecVelocity", v);
	v[2] += 350.0;
	TeleportEntity(c, NULL_VECTOR, NULL_VECTOR, v);
}

public void OnFire(Event e, const char[] name, bool dontBroadcast)
{
	int c = GetClientOfUserId(e.GetInt("userid"));
	if (c > 0 && g_Cheat[c] == C_Municion)
		RequestFrame(Recargar, GetClientUserId(c));
}

public void Recargar(int uid)
{
	int c = GetClientOfUserId(uid);
	if (c < 1 || !IsPlayerAlive(c))
		return;
	int w = GetEntPropEnt(c, Prop_Send, "m_hActiveWeapon");
	if (w > MaxClients && HasEntProp(w, Prop_Send, "m_iClip1"))
		SetEntProp(w, Prop_Send, "m_iClip1", GetEntProp(w, Prop_Send, "m_iClip1") + 1);
}

public Action OnTakeDamage(int victim, int &attacker, int &inflictor, float &damage, int &damagetype)
{
	if (attacker >= 1 && attacker <= MaxClients && attacker != victim && g_Cheat[attacker] == C_UnTiro)
	{
		damage = 1000.0;
		return Plugin_Changed;
	}
	return Plugin_Continue;
}

bool Visible(int c, int e, float cabeza[3])
{
	float ojo[3];
	GetClientEyePosition(c, ojo);
	GetClientEyePosition(e, cabeza);
	cabeza[2] -= 4.0;
	TR_TraceRayFilter(ojo, cabeza, MASK_SHOT, RayType_EndPoint, FiltroJugadores, c);
	return !TR_DidHit() || TR_GetEntityIndex() == e;
}

public bool FiltroJugadores(int ent, int mask, int yo)
{
	return ent != yo && (ent < 1 || ent > MaxClients);
}

public Action OnPlayerRunCmd(int c, int &buttons, int &impulse, float vel[3], float angles[3], int &weapon)
{
	Cheat ch = g_Cheat[c];
	if (ch == C_Bhop && (buttons & IN_JUMP) && !(GetEntityFlags(c) & FL_ONGROUND)
		&& GetEntityMoveType(c) != MOVETYPE_LADDER && GetEntProp(c, Prop_Data, "m_nWaterLevel") < 2)
	{
		buttons &= ~IN_JUMP;
		return Plugin_Changed;
	}
	if (ch != C_Aimbot || !IsPlayerAlive(c))
		return Plugin_Continue;
	// aims softly at the visible enemy head closest to the crosshair
	float ojo[3], cabeza[3], mejor[3], dir[3], quiere[3];
	GetClientEyePosition(c, ojo);
	float mejorDif = 60.0;
	bool hay = false;
	bool todos = FindConVar("mp_teammates_are_enemies").BoolValue;
	for (int e = 1; e <= MaxClients; e++)
	{
		if (e == c || !IsClientInGame(e) || !IsPlayerAlive(e) || !todos && GetClientTeam(e) == GetClientTeam(c))
			continue;
		if (!Visible(c, e, cabeza))
			continue;
		MakeVectorFromPoints(ojo, cabeza, dir);
		GetVectorAngles(dir, quiere);
		float dy = FloatAbs(AngleDiff(quiere[1], angles[1]));
		float dx = FloatAbs(AngleDiff(quiere[0], angles[0]));
		if (dx + dy < mejorDif)
		{
			mejorDif = dx + dy;
			mejor = quiere;
			hay = true;
		}
	}
	if (!hay)
		return Plugin_Continue;
	angles[0] += AngleDiff(mejor[0], angles[0]) * 0.35;
	angles[1] += AngleDiff(mejor[1], angles[1]) * 0.35;
	if (!IsFakeClient(c))
		TeleportEntity(c, NULL_VECTOR, angles, NULL_VECTOR);
	return Plugin_Changed;
}

float AngleDiff(float a, float b)
{
	float d = a - b;
	while (d > 180.0) d -= 360.0;
	while (d < -180.0) d += 360.0;
	return d;
}

public Action CmdEstado(int args)
{
	int vivos = 0;
	for (int i = 0; i < MAX_PU; i++)
		if (EntRefToEntIndex(g_Pu[i]) > MaxClients)
			vivos++;
	PrintToServer("modo_cheats=%d power-ups=%d puntos=%d modelo=%s", g_On.IntValue, vivos, g_Puntos.Length, g_Modelo);
	for (int c = 1; c <= MaxClients; c++)
		if (IsClientInGame(c) && g_Cheat[c] != C_Ninguno)
			PrintToServer("%N: %s (%d s)", c, g_Nombres[g_Cheat[c]], RoundToCeil(g_Hasta[c] - GetGameTime()));
	return Plugin_Handled;
}

// Cheat name in the player's language (phrases cheat_0..cheat_N).
void CheatName(Cheat ch, int client, char[] out, int len)
{
	char key[16];
	Format(key, sizeof(key), "cheat_%d", view_as<int>(ch));
	Format(out, len, "%T", key, client);
}
