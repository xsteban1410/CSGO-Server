// SPDX-License-Identifier: GPL-3.0-or-later
// The engine API for plugins that add game modes or tools, such as the ones in CSGO-Hub.
// #include <csls> and implement only the forwards you need. Convars are the rest of the API:
// a mode cfg sets modo_t_arma, modo_ct_vida and the others, and a plugin may change them too.
#if defined _csls_included
 #endinput
#endif
#define _csls_included

// Raised only when something here changes in a way that breaks plugins built for the previous one.
#define CSLS_API 1

// Before each map's chain of mode cfgs. Put your convars back to their defaults here
// (ConVar.RestoreDefault), so a mode never inherits the switches of the one before.
forward void CSLS_OnModeReset();

// After a player got the weapons, health and speed of the mode on spawn.
forward void CSLS_OnEquip(int client);

// Before the engine moves a human to the side the mode wants. Return Plugin_Handled to keep them where they are.
forward Action CSLS_OnPlaceHuman(int client);

// From csls_anticheat: ignore a player for `seconds`, while a plugin moves their aim or speed on purpose.
native void CSLS_AC_Exempt(int client, float seconds);

// The same, doing nothing when the anticheat is off or not installed.
stock void CSLS_Exempt(int client, float seconds)
{
	if (GetFeatureStatus(FeatureType_Native, "CSLS_AC_Exempt") == FeatureStatus_Available)
		CSLS_AC_Exempt(client, seconds);
}

// Data the app extracts for plugins that ask for it with `needs = ["nav"]` in their item.toml. Fills `out` with the
// path of the file for `map` (for "nav": a text graph of the map's navigation mesh) and returns whether it exists.
stock bool CSLS_ExportPath(const char[] name, const char[] map, char[] out, int len)
{
	BuildPath(Path_SM, out, len, "data/csls_export/%s/%s.txt", name, map);
	return FileExists(out);
}

// Always optional: a plugin keeps working without the anticheat. csls_anticheat defines CSLS_ANTICHEAT.
#if !defined CSLS_ANTICHEAT
public SharedPlugin __pl_csls_anticheat =
{
	name = "csls_anticheat",
	file = "csls_anticheat.smx",
	required = 0,
};

public void __pl_csls_anticheat_SetNTVOptional()
{
	MarkNativeAsOptional("CSLS_AC_Exempt");
}
#endif
