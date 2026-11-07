// SPDX-License-Identifier: GPL-3.0-or-later
// Mode rules as blocks: modo_regla_N "<event> <action> <value>". A closed catalog: this runs no commands.
// The app and the core validate the text too (crates/core/src/rules.rs); a cfg edited by hand reaches here as is,
// so anything outside the catalog is simply ignored. Included by modos.sp.

#define MAX_REGLAS 16

ConVar g_Regla[MAX_REGLAS];

void Reglas_Start()
{
	char name[32];
	for (int i = 0; i < MAX_REGLAS; i++)
	{
		Format(name, sizeof(name), "modo_regla_%d", i + 1);
		g_Regla[i] = CreateConVar(name, "", "Rule of the mode: <event> <action> <value>");
	}
	HookEvent("player_spawn", Reglas_OnSpawn);
	HookEvent("player_death", Reglas_OnDeath);
	HookEvent("round_freeze_end", Reglas_OnRoundStart);
}

void Reglas_Apply(int c, const char[] action, const char[] value)
{
	if (StrEqual(action, "give"))
	{
		// only what the catalog allows, even from a cfg edited by hand
		if (StrContains(value, "weapon_") == 0 || StrContains(value, "item_") == 0)
			GivePlayerItem(c, value);
	}
	else if (StrEqual(action, "health"))
	{
		int health = GetClientHealth(c) + StringToInt(value);
		SetEntityHealth(c, health < 1 ? 1 : health);
	}
	else if (StrEqual(action, "speed"))
	{
		float speed = StringToFloat(value);
		if (speed >= 0.1 && speed <= 5.0)
			SetEntPropFloat(c, Prop_Send, "m_flLaggedMovementValue", speed);
	}
	else if (StrEqual(action, "money"))
	{
		int money = GetEntProp(c, Prop_Send, "m_iAccount") + StringToInt(value);
		SetEntProp(c, Prop_Send, "m_iAccount", money < 0 ? 0 : money > 16000 ? 16000 : money);
	}
	else if (StrEqual(action, "message"))
	{
		PrintToChat(c, "%s", value);
	}
}

void Reglas_Run(int c, const char[] event)
{
	if (c < 1 || c > MaxClients || !IsClientInGame(c) || !IsPlayerAlive(c))
		return;
	char text[160], parts[3][128];
	for (int i = 0; i < MAX_REGLAS; i++)
	{
		g_Regla[i].GetString(text, sizeof(text));
		if (!text[0])
			continue;
		// the third part keeps its spaces: a message has them
		if (ExplodeString(text, " ", parts, sizeof(parts), sizeof(parts[]), true) == 3 && StrEqual(parts[0], event))
			Reglas_Apply(c, parts[1], parts[2]);
	}
}

public Action Reglas_AfterSpawn(Handle t, int userid)
{
	Reglas_Run(GetClientOfUserId(userid), "spawn");
	return Plugin_Stop;
}

public void Reglas_OnSpawn(Event e, const char[] name, bool dontBroadcast)
{
	// after Equip (0.2 s in modos.sp), which can strip weapons
	CreateTimer(0.3, Reglas_AfterSpawn, e.GetInt("userid"));
}

public void Reglas_OnDeath(Event e, const char[] name, bool dontBroadcast)
{
	int killer = GetClientOfUserId(e.GetInt("attacker"));
	if (killer == GetClientOfUserId(e.GetInt("userid")))
		return;
	Reglas_Run(killer, "kill");
	if (e.GetBool("headshot"))
		Reglas_Run(killer, "headshot");
}

public void Reglas_OnRoundStart(Event e, const char[] name, bool dontBroadcast)
{
	for (int c = 1; c <= MaxClients; c++)
		Reglas_Run(c, "round_start");
}
