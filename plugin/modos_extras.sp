// SPDX-License-Identifier: GPL-3.0-or-later
// Modes with their own rules: one bullet, random weapon, vampire and boss. Included by modos.sp.

ConVar g_UnaBala, g_Aleatorio, g_Vampiro, g_VampiroMax, g_Jefe, g_Anarquia;

// Anarchy: cvars that kick or ban on their own, and SourceMod plugins that do too
char g_AnarquiaCvars[][][] = {
	{"mp_autokick", "0"}, {"mp_tkpunish", "0"}, {"sv_kick_ban_duration", "0"},
	{"sv_vote_issue_kick_allowed", "0"}, {"mp_spawnprotectiontime", "0"}, {"sv_timeout", "300"},
	{"mp_disconnect_kills_players", "0"}
};
char g_AnarquiaPlugins[][] = { "basevotes", "antiflood", "reservedslots" };

// Weapons of the random mode (the same for everyone each round)
char g_Aleatorias[][] = {
	"weapon_awp", "weapon_ssg08", "weapon_ak47", "weapon_m4a1_silencer", "weapon_sg556", "weapon_aug",
	"weapon_famas", "weapon_galilar", "weapon_scar20", "weapon_nova", "weapon_xm1014", "weapon_mag7",
	"weapon_sawedoff", "weapon_negev", "weapon_m249", "weapon_p90", "weapon_mp9", "weapon_ump45",
	"weapon_bizon", "weapon_mp7", "weapon_deagle", "weapon_revolver", "weapon_fiveseven", "weapon_cz75a",
	"weapon_elite", "weapon_tec9", "weapon_p250", "weapon_taser"
};

void Extras_Start()
{
	g_UnaBala    = CreateConVar("modo_unabala", "0", "1 = one bullet in the pistol, each kill gives +1 and any hit kills");
	g_Aleatorio  = CreateConVar("modo_aleatorio", "0", "1 = each round a weapon is drawn, the same for everyone");
	g_Vampiro    = CreateConVar("modo_vampiro", "0", "Fraction of damage dealt that heals the attacker (0 = off)");
	g_VampiroMax = CreateConVar("modo_vampiro_max", "200", "Maximum health reachable by healing");
	g_Jefe       = CreateConVar("modo_jefe", "0", "Health of the giant boss bot each round (0 = no boss)");
	g_Anarquia   = CreateConVar("modo_anarquia", "0", "1 = nobody is kicked or banned automatically (admin kick/ban still work)");
	g_Anarquia.AddChangeHook(Extras_Anarquia);
	HookEvent("player_death", Extras_Death);
	HookEvent("player_hurt", Extras_Hurt);
}

// One bullet: leaves the pistol with 1 bullet and no reserve. Called from Equip.
void Extras_Equip(int c)
{
	if (!g_UnaBala.BoolValue)
		return;
	int w = GetPlayerWeaponSlot(c, CS_SLOT_SECONDARY);
	if (w != -1)
	{
		SetEntProp(w, Prop_Send, "m_iClip1", 1);
		SetEntProp(w, Prop_Send, "m_iPrimaryReserveAmmoCount", 0);
	}
}

public Action Extras_TakeDamage(int victim, int &attacker, int &inflictor, float &damage, int &damagetype)
{
	if (g_UnaBala.BoolValue && attacker >= 1 && attacker <= MaxClients && attacker != victim)
	{
		damage = 1000.0;
		return Plugin_Changed;
	}
	return Plugin_Continue;
}

public void Extras_Death(Event e, const char[] name, bool dontBroadcast)
{
	if (!g_UnaBala.BoolValue)
		return;
	int a = GetClientOfUserId(e.GetInt("attacker"));
	if (a < 1 || a == GetClientOfUserId(e.GetInt("userid")) || !IsPlayerAlive(a))
		return;
	int w = GetPlayerWeaponSlot(a, CS_SLOT_SECONDARY);
	if (w != -1)
		SetEntProp(w, Prop_Send, "m_iClip1", GetEntProp(w, Prop_Send, "m_iClip1") + 1);
}

public void Extras_Hurt(Event e, const char[] name, bool dontBroadcast)
{
	float f = g_Vampiro.FloatValue;
	if (f <= 0.0)
		return;
	int a = GetClientOfUserId(e.GetInt("attacker"));
	if (a < 1 || a == GetClientOfUserId(e.GetInt("userid")) || !IsPlayerAlive(a))
		return;
	int hp = GetClientHealth(a) + RoundToCeil(e.GetInt("dmg_health") * f);
	SetEntityHealth(a, hp < g_VampiroMax.IntValue ? hp : g_VampiroMax.IntValue);
}

// Random: drawn in round_prestart, before anyone spawns.
void Extras_PreStart()
{
	if (!g_Aleatorio.BoolValue)
		return;
	char w[32];
	strcopy(w, sizeof(w), g_Aleatorias[GetRandomInt(0, sizeof(g_Aleatorias) - 1)]);
	for (int i = 0; i < 2; i++)
	{
		g_Arma[i].SetString(w);
		g_Quitar[i].SetInt(1);
	}
	PrintToChatAll(" \x04[Aleatorio]\x01 %t", "random_round", w[7]);
}

// Boss: a random T bot turns giant, slow and with lots of health.
void Extras_FreezeEnd()
{
	if (g_Jefe.IntValue <= 0 || GameRules_GetProp("m_bWarmupPeriod"))
		return;
	int bots[MAXPLAYERS + 1], n = 0;
	for (int i = 1; i <= MaxClients; i++)
		if (IsClientInGame(i) && IsFakeClient(i) && IsPlayerAlive(i) && GetClientTeam(i) == CS_TEAM_T)
			bots[n++] = i;
	if (n == 0)
		return;
	int b = bots[GetRandomInt(0, n - 1)];
	SetEntityHealth(b, g_Jefe.IntValue);
	SetEntPropFloat(b, Prop_Send, "m_flModelScale", 1.6);
	SetEntPropFloat(b, Prop_Send, "m_flLaggedMovementValue", 0.85);
	SetEntityRenderMode(b, RENDER_TRANSCOLOR);
	SetEntityRenderColor(b, 255, 60, 60, 255);
	char bn[MAX_NAME_LENGTH];
	GetClientName(b, bn, sizeof(bn));
	PrintToChatAll(" \x04[Jefe]\x01 %t", "boss_is", bn, g_Jefe.IntValue);
}

// --- anarchy ----------------------------------------------------------------

public void Extras_Anarquia(ConVar cv, const char[] viejo, const char[] nuevo)
{
	bool on = cv.BoolValue;
	if (on == (StringToInt(viejo) != 0))
		return;
	for (int i = 0; i < sizeof(g_AnarquiaCvars); i++)
	{
		ConVar c = FindConVar(g_AnarquiaCvars[i][0]);
		if (c == null)
			continue;
		if (on)
			c.SetString(g_AnarquiaCvars[i][1]);
		else
			c.RestoreDefault();
	}
	for (int i = 0; i < sizeof(g_AnarquiaPlugins); i++)
		ServerCommand("sm plugins %s %s", on ? "unload" : "load", g_AnarquiaPlugins[i]);
	PrintToChatAll(" \x04[Modos]\x01 %t", on ? "anarchy_on" : "anarchy_off");
}
