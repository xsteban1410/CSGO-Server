// SPDX-License-Identifier: GPL-3.0-or-later
// Anarchy (`--anarchy` or modo_anarquia 1): nothing kicks or bans by itself; admin kick and ban still work.
// It is a launch option, so it lives in the engine. Included by modos.sp.

ConVar g_Anarquia;

// cvars that kick or ban on their own, and SourceMod plugins that do too
char g_AnarquiaCvars[][][] = {
	{"mp_autokick", "0"}, {"mp_tkpunish", "0"}, {"sv_kick_ban_duration", "0"},
	{"sv_vote_issue_kick_allowed", "0"}, {"mp_spawnprotectiontime", "0"}, {"sv_timeout", "300"},
	{"mp_disconnect_kills_players", "0"}
};
char g_AnarquiaPlugins[][] = { "basevotes", "antiflood", "reservedslots" };

void Anarquia_Start()
{
	g_Anarquia = CreateConVar("modo_anarquia", "0", "1 = nobody is kicked or banned automatically (admin kick/ban still work)");
	g_Anarquia.AddChangeHook(Anarquia_Changed);
}

public void Anarquia_Changed(ConVar cv, const char[] viejo, const char[] nuevo)
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
