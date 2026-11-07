// SPDX-License-Identifier: GPL-3.0-or-later
// Real matches: the team humans play on, !ready and map bans. Included by modos.sp.

ConVar g_Equipos, g_Listos, g_Veto, g_VetoMapas, g_VetoTiempo;
bool g_Listo[MAXPLAYERS + 1];
bool g_Colocado[MAXPLAYERS + 1];  // already placed by modo_equipos on this map

ArrayList g_VetoQuedan;         // maps still in play; null = no bans running
Handle g_VetoTimer;
int g_VetoTurno;
bool g_VetoHecho, g_VetoCambiaMapa;
char g_VetoFinal[64];

void Comp_Start()
{
	g_Equipos    = CreateConVar("modo_equipos", "", "Where humans start when they join: contra (balanced), juntos, libre. They can switch afterwards");
	g_Listos     = CreateConVar("modo_listos", "0", "1 = warmup ends when every human says !ready, or when its time runs out");
	g_Veto       = CreateConVar("modo_veto", "0", "1 = humans ban maps in warmup and the match is played on the last one");
	g_VetoMapas  = CreateConVar("modo_veto_mapas", "de_inferno de_train de_mirage de_nuke de_dust2 de_overpass de_vertigo", "Maps of the bans, separated by spaces");
	g_VetoTiempo = CreateConVar("modo_veto_tiempo", "20", "Seconds per ban; after that a random map is banned", _, true, 1.0);
	RegConsoleCmd("sm_ready", CmdListo, "Say you are ready to start");
	RegConsoleCmd("sm_listo", CmdListo, "Say you are ready to start");
	RegServerCmd("modo_veto_iniciar", CmdVetoIniciar, "Starts the map bans now, also without humans (for testing)");
}

void Comp_MapStart()
{
	for (int i = 0; i <= MaxClients; i++)
	{
		g_Listo[i] = false;
		g_Colocado[i] = false;
	}
	delete g_VetoQuedan;
	g_VetoTimer = null;  // TIMER_FLAG_NO_MAPCHANGE already removed it
	// the map change the bans asked for is the same match; any other starts a new one
	if (g_VetoCambiaMapa)
		g_VetoCambiaMapa = false;
	else
		g_VetoHecho = false;
}

void Comp_ClientGone(int c)
{
	g_Listo[c] = false;
	g_Colocado[c] = false;
}

// --- teams ------------------------------------------------------------------

// Team this human starts on according to modo_equipos (0 = whichever they are on).
int Comp_TeamFor(int c)
{
	char e[16];
	g_Equipos.GetString(e, sizeof(e));
	bool juntos = StrEqual(e, "juntos"), contra = StrEqual(e, "contra");
	if (!juntos && !contra)
		return 0;
	int t = 0, ct = 0;
	for (int i = 1; i <= MaxClients; i++)
		if (i != c && IsClientInGame(i) && !IsFakeClient(i))
		{
			if (GetClientTeam(i) == CS_TEAM_T)
				t++;
			else if (GetClientTeam(i) == CS_TEAM_CT)
				ct++;
		}
	if (t == ct)
		return 0;
	if (juntos)
		return t > ct ? CS_TEAM_T : CS_TEAM_CT;
	return t < ct ? CS_TEAM_T : CS_TEAM_CT;
}

// Only the first spawn on a map counts, so nobody gets moved back or dragged along later,
// and the half-time swap is left alone.
int Comp_Placement(int c)
{
	if (g_Colocado[c])
		return 0;
	g_Colocado[c] = true;
	return Comp_TeamFor(c);
}

int Comp_BotOn(int team)
{
	for (int i = MaxClients; i >= 1; i--)
		if (IsClientInGame(i) && IsFakeClient(i) && !IsClientSourceTV(i) && GetClientTeam(i) == team
			&& (!IsPlayerAlive(i) || GetPlayerWeaponSlot(i, CS_SLOT_C4) == -1))
			return i;
	return 0;
}

// Moving a human leaves one team with more players, and the bot quota only counts the total.
// Evens the teams with bots, at the start of a round or in warmup, which is when nobody notices.
// Bots survive a mode change: the ones a previous mode left on the humans' team go to the side bot_join_team says.
void Comp_BotsToJoinTeam()
{
	char j[8];
	FindConVar("bot_join_team").GetString(j, sizeof(j));
	int team = StrEqual(j, "T", false) ? CS_TEAM_T : StrEqual(j, "CT", false) ? CS_TEAM_CT : 0;
	if (team == 0)
		return;
	for (int i = 1; i <= MaxClients; i++)
		if (IsClientInGame(i) && IsFakeClient(i) && !IsClientSourceTV(i) && (GetClientTeam(i) == CS_TEAM_T || GetClientTeam(i) == CS_TEAM_CT) && GetClientTeam(i) != team)
			CS_SwitchTeam(i, team);
}

void Comp_BalanceBots(bool roundStart)
{
	char e[16];
	g_Equipos.GetString(e, sizeof(e));
	if (roundStart)
		Comp_BotsToJoinTeam();
	// With free sides bots keep the teams even too, except where the mode has no teams of its own.
	char j[8];
	FindConVar("bot_join_team").GetString(j, sizeof(j));
	if (!e[0] && !(g_GameType.IntValue == 0 && StrEqual(j, "any", false)))
		return;
	bool warmup = GameRules_GetProp("m_bWarmupPeriod") != 0;
	if (!roundStart && !warmup && !GameRules_GetProp("m_bFreezePeriod"))
		return;
	int t = GetTeamClientCount(CS_TEAM_T), ct = GetTeamClientCount(CS_TEAM_CT);
	while (t - ct >= 2 || ct - t >= 2)
	{
		int from = t > ct ? CS_TEAM_T : CS_TEAM_CT;
		int bot = Comp_BotOn(from);
		if (bot == 0)
			return;
		CS_SwitchTeam(bot, from == CS_TEAM_T ? CS_TEAM_CT : CS_TEAM_T);
		if (!roundStart && IsPlayerAlive(bot))
			CS_RespawnPlayer(bot);
		t += from == CS_TEAM_T ? -1 : 1;
		ct += from == CS_TEAM_T ? 1 : -1;
	}
}

// --- !ready -----------------------------------------------------------------

public Action CmdListo(int c, int args)
{
	if (c == 0 || !g_Listos.BoolValue || !GameRules_GetProp("m_bWarmupPeriod"))
		return Plugin_Handled;
	g_Listo[c] = !g_Listo[c];
	PrintToChat(c, " \x04[Modos]\x01 %t", g_Listo[c] ? "ready_yes" : "ready_no");
	return Plugin_Handled;
}

int Comp_ReadyCount()
{
	int n = 0;
	for (int i = 1; i <= MaxClients; i++)
		if (IsClientInGame(i) && !IsFakeClient(i) && GetClientTeam(i) >= CS_TEAM_T && g_Listo[i])
			n++;
	return n;
}

// --- map bans ---------------------------------------------------------------

// Are bans pending? Starts them the first time, so warmup waits for them.
bool Veto_Pending()
{
	if (!g_Veto.BoolValue || g_VetoHecho)
		return false;
	if (g_VetoQuedan == null)
		Veto_Start();
	return !g_VetoHecho;
}

void Veto_Start()
{
	char todo[256], mapas[16][64];
	g_VetoMapas.GetString(todo, sizeof(todo));
	int n = ExplodeString(todo, " ", mapas, sizeof(mapas), sizeof(mapas[]));
	g_VetoQuedan = new ArrayList(ByteCountToCells(64));
	for (int i = 0; i < n; i++)
		if (mapas[i][0] && IsMapValid(mapas[i]))
			g_VetoQuedan.PushString(mapas[i]);
	g_VetoTurno = 0;
	if (g_VetoQuedan.Length < 2)
	{
		g_VetoHecho = true;
		delete g_VetoQuedan;
		return;
	}
	PrintToChatAll(" \x04[Premier]\x01 %t", "veto_start");
	Veto_Next();
}

int Veto_Player()
{
	int humans[MAXPLAYERS], n = 0;
	for (int i = 1; i <= MaxClients; i++)
		if (IsClientInGame(i) && !IsFakeClient(i))
			humans[n++] = i;
	return n == 0 ? 0 : humans[g_VetoTurno % n];
}

void Veto_Next()
{
	if (g_VetoQuedan.Length == 1)
	{
		Veto_End();
		return;
	}
	int c = Veto_Player();
	if (c != 0)
	{
		char mapa[64];
		Menu m = new Menu(OnVeto);
		m.SetTitle("%T", "veto_title", c);
		m.ExitButton = false;
		for (int i = 0; i < g_VetoQuedan.Length; i++)
		{
			g_VetoQuedan.GetString(i, mapa, sizeof(mapa));
			m.AddItem(mapa, mapa);
		}
		m.Display(c, g_VetoTiempo.IntValue);
		PrintToChatAll(" \x04[Premier]\x01 %t", "veto_turn", c);
	}
	g_VetoTimer = CreateTimer(g_VetoTiempo.FloatValue, Veto_Timeout, _, TIMER_FLAG_NO_MAPCHANGE);
}

public Action Veto_Timeout(Handle t)
{
	g_VetoTimer = null;
	if (g_VetoQuedan != null)
	{
		char mapa[64];
		g_VetoQuedan.GetString(GetRandomInt(0, g_VetoQuedan.Length - 1), mapa, sizeof(mapa));
		Veto_Ban(0, mapa);
	}
	return Plugin_Stop;
}

public int OnVeto(Menu m, MenuAction a, int c, int item)
{
	if (a == MenuAction_End)
		delete m;
	else if (a == MenuAction_Select && g_VetoQuedan != null && c == Veto_Player())
	{
		char mapa[64];
		m.GetItem(item, mapa, sizeof(mapa));
		if (g_VetoQuedan.FindString(mapa) != -1)
			Veto_Ban(c, mapa);
	}
	return 0;
}

void Veto_Ban(int c, const char[] mapa)
{
	delete g_VetoTimer;
	g_VetoQuedan.Erase(g_VetoQuedan.FindString(mapa));
	if (c != 0)
		PrintToChatAll(" \x04[Premier]\x01 %t", "veto_banned", c, mapa);
	else
		PrintToChatAll(" \x04[Premier]\x01 %t", "veto_banned_time", mapa);
	PrintToServer("[Premier] banned %s, %d left", mapa, g_VetoQuedan.Length);
	g_VetoTurno++;
	Veto_Next();
}

void Veto_End()
{
	char actual[64];
	g_VetoQuedan.GetString(0, g_VetoFinal, sizeof(g_VetoFinal));
	delete g_VetoQuedan;
	g_VetoHecho = true;
	GetCurrentMap(actual, sizeof(actual));
	PrintToChatAll(" \x04[Premier]\x01 %t", "veto_map", g_VetoFinal);
	PrintToServer("[Premier] map: %s", g_VetoFinal);
	if (!StrEqual(actual, g_VetoFinal))
	{
		g_VetoCambiaMapa = true;
		CreateTimer(3.0, Veto_ChangeMap, _, TIMER_FLAG_NO_MAPCHANGE);
	}
}

public Action Veto_ChangeMap(Handle t)
{
	ServerCommand("changelevel %s", g_VetoFinal);
	return Plugin_Stop;
}

public Action CmdVetoIniciar(int args)
{
	if (g_VetoQuedan == null)
	{
		g_VetoHecho = false;
		Veto_Start();
	}
	return Plugin_Handled;
}
