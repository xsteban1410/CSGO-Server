// SPDX-License-Identifier: GPL-3.0-or-later
// Skins, knives and weapons of choice, saved per player (SQLite).
//   !ws      skin of the weapon in hand (or another weapon)
//   !knife   knife model (also !cuchillo)
//   !armas   any weapon (only if skins_armas_libres = 1, e.g. in the "libre" mode; also !guns)
// The skin list comes from configs/skins.cfg (csgo-legacy-server skins).
#pragma semicolon 1
#include <sourcemod>
#include <sdktools>
#include <sdkhooks>
#include <cstrike>
#pragma newdecls required

public Plugin myinfo = { name = "Skins", author = "xsteban1410", version = "1.0" };

#define CLAVE_CUCHILLO "cuchillo"
#define DESGASTE 0.0001  // Factory New

KeyValues g_Kv;
StringMap g_PorIndice;          // "60" -> "weapon_m4a1_silencer"
StringMap g_Prefs[MAXPLAYERS + 1];
char g_SteamId[MAXPLAYERS + 1][24];
bool g_Dando[MAXPLAYERS + 1];   // avoids re-entering the hook while giving a weapon
Database g_Db;
ConVar g_ArmasLibres;

public void OnPluginStart()
{
	LoadTranslations("csls.phrases");
	g_ArmasLibres = CreateConVar("skins_armas_libres", "0", "1 = allows !armas to take any weapon");
	RegConsoleCmd("sm_ws", CmdWs, "Choose a skin");
	RegConsoleCmd("sm_skins", CmdWs, "Choose a skin");
	RegConsoleCmd("sm_knife", CmdKnife, "Choose a knife");
	RegConsoleCmd("sm_cuchillo", CmdKnife, "Choose a knife");
	RegConsoleCmd("sm_armas", CmdArmas, "Choose any weapon");
	RegConsoleCmd("sm_guns", CmdArmas, "Choose any weapon");
	RegServerCmd("skins_estado", CmdEstado, "Shows the SteamID and loaded skins of each player");
	CargarLista();
	Database.Connect(OnDb, "storage-local");
	for (int c = 1; c <= MaxClients; c++)
		if (IsClientInGame(c))
			OnClientPutInServer(c);
}

void CargarLista()
{
	char path[PLATFORM_MAX_PATH], arma[64], idx[12];
	BuildPath(Path_SM, path, sizeof(path), "configs/skins.cfg");
	g_Kv = new KeyValues("skins");
	if (!g_Kv.ImportFromFile(path))
		SetFailState("Could not read %s (generate it with: csgo-legacy-server skins)", path);
	g_PorIndice = new StringMap();
	g_Kv.JumpToKey("armas");
	if (g_Kv.GotoFirstSubKey())
	{
		do
		{
			g_Kv.GetSectionName(arma, sizeof(arma));
			g_Kv.GetString("indice", idx, sizeof(idx), "0");
			g_PorIndice.SetString(idx, arma);
		} while (g_Kv.GotoNextKey());
	}
	g_Kv.Rewind();
}

// --- database ---------------------------------------------------------

public void OnDb(Database db, const char[] error, any data)
{
	if (db == null)
	{
		LogError("No database, skins will not be saved: %s", error);
		return;
	}
	g_Db = db;
	g_Db.Query(OnQuery, "CREATE TABLE IF NOT EXISTS skins_prefs (steamid TEXT NOT NULL, clave TEXT NOT NULL, valor TEXT NOT NULL, PRIMARY KEY (steamid, clave))");
	for (int c = 1; c <= MaxClients; c++)
		if (IsClientInGame(c))
			Cargar(c);
}

public void OnQuery(Database db, DBResultSet r, const char[] error, any data)
{
	if (r == null)
		LogError("Error SQL: %s", error);
}

public void OnClientPutInServer(int c)
{
	delete g_Prefs[c];
	g_Prefs[c] = new StringMap();
	g_SteamId[c][0] = '\0';
	if (IsFakeClient(c))
		return;
	SDKHook(c, SDKHook_WeaponEquipPost, OnEquip);
	Cargar(c);
}

// The SteamID arrives after joining: loaded when the server confirms it.
public void OnClientPostAdminCheck(int c)
{
	if (!IsFakeClient(c) && !g_SteamId[c][0])
		Cargar(c);
}

public void OnClientDisconnect(int c)
{
	delete g_Prefs[c];
}

void Cargar(int c)
{
	// validate=false: the server is -insecure and Steam does not validate the ID, but it is still the player's.
	if (g_Db == null || IsFakeClient(c) || g_SteamId[c][0])
		return;
	char id[32];
	if (!SteamId64(c, id, sizeof(id)))
	{
		CreateTimer(2.0, ReintentarCarga, GetClientUserId(c));
		return;
	}
	strcopy(g_SteamId[c], sizeof(g_SteamId[]), id);
	char q[160];
	g_Db.Format(q, sizeof(q), "SELECT clave, valor FROM skins_prefs WHERE steamid = '%s'", g_SteamId[c]);
	g_Db.Query(OnCargado, q, GetClientUserId(c));
}

// Player SteamID64. With the server in LAN/-insecure mode, SourceMod does not give it (returns
// "STEAM_ID_STOP_IGNORING_RETVALS"), but it does give the account number: 76561197960265728 + account.
bool SteamId64(int c, char[] out, int len)
{
	if (GetClientAuthId(c, AuthId_SteamID64, out, len, false) && strlen(out) == 17 && strncmp(out, "7656", 4) == 0)
		return true;
	int cuenta = GetSteamAccountID(c, false);
	if (cuenta <= 0)
		return false;
	// Adds in two 9-digit halves: SourcePawn integers are 32-bit
	int lo = 960265728 + cuenta % 1000000000, hi = 76561197 + cuenta / 1000000000 + lo / 1000000000;
	Format(out, len, "%d%09d", hi, lo % 1000000000);
	return true;
}

public Action ReintentarCarga(Handle t, int userid)
{
	int c = GetClientOfUserId(userid);
	if (c && IsClientInGame(c))
		Cargar(c);
	return Plugin_Stop;
}

public void OnCargado(Database db, DBResultSet r, const char[] error, int userid)
{
	int c = GetClientOfUserId(userid);
	if (r == null || c == 0 || g_Prefs[c] == null)
		return;
	char k[64], v[64];
	while (r.FetchRow())
	{
		r.FetchString(0, k, sizeof(k));
		r.FetchString(1, v, sizeof(v));
		g_Prefs[c].SetString(k, v);
	}
	if (IsPlayerAlive(c))
		Refrescar(c, "");
}

void Guardar(int c, const char[] clave, const char[] valor)
{
	if (valor[0])
		g_Prefs[c].SetString(clave, valor);
	else
		g_Prefs[c].Remove(clave);
	if (g_Db == null || !g_SteamId[c][0])
		return;
	char q[256];
	if (valor[0])
		g_Db.Format(q, sizeof(q), "INSERT OR REPLACE INTO skins_prefs VALUES ('%s', '%s', '%s')", g_SteamId[c], clave, valor);
	else
		g_Db.Format(q, sizeof(q), "DELETE FROM skins_prefs WHERE steamid = '%s' AND clave = '%s'", g_SteamId[c], clave);
	g_Db.Query(OnQuery, q);
}

// --- aplicar skins -------------------------------------------------------------

bool ClaveArma(int w, char[] arma, int len)
{
	char idx[12];
	IntToString(GetEntProp(w, Prop_Send, "m_iItemDefinitionIndex"), idx, sizeof(idx));
	return g_PorIndice.GetString(idx, arma, len);
}

bool EsCuchillo(const char[] arma) { return StrContains(arma, "knife") != -1 || StrContains(arma, "bayonet") != -1; }

void Pintar(int c, int w, const char[] arma)
{
	char v[12];
	if (!g_Prefs[c].GetString(arma, v, sizeof(v)))
		return;
	// a new item id each time: the client only reads the paint again when the id changes
	static int idHigh = 16384;
	SetEntProp(w, Prop_Send, "m_iItemIDLow", -1);
	SetEntProp(w, Prop_Send, "m_iItemIDHigh", idHigh++);
	SetEntProp(w, Prop_Send, "m_nFallbackPaintKit", StringToInt(v));
	SetEntPropFloat(w, Prop_Send, "m_flFallbackWear", DESGASTE);
	SetEntProp(w, Prop_Send, "m_nFallbackSeed", 0);
	SetEntProp(w, Prop_Send, "m_nFallbackStatTrak", -1);
	SetEntProp(w, Prop_Send, "m_iAccountID", GetSteamAccountID(c, false));
	if (EsCuchillo(arma))
		SetEntProp(w, Prop_Send, "m_iEntityQuality", 3);  // ★
}

public void OnEquip(int c, int w)
{
	if (g_Dando[c] || g_Prefs[c] == null || !IsValidEntity(w))
		return;
	char arma[64], modelo[64];
	if (!ClaveArma(w, arma, sizeof(arma)) && !IsCuchilloBase(w))
		return;
	if ((IsCuchilloBase(w) || EsCuchillo(arma)) && g_Prefs[c].GetString(CLAVE_CUCHILLO, modelo, sizeof(modelo)) && !StrEqual(arma, modelo))
	{
		RequestFrame(CambiarCuchillo, GetClientUserId(c));  // a weapon cannot be removed inside its own hook
		return;
	}
	Pintar(c, w, arma);
}

bool IsCuchilloBase(int w)
{
	int i = GetEntProp(w, Prop_Send, "m_iItemDefinitionIndex");
	return i == 42 || i == 59;  // default CT / T knife
}

int BuscarCuchillo(int c)
{
	char arma[64];
	int n = GetEntPropArraySize(c, Prop_Send, "m_hMyWeapons");
	for (int i = 0; i < n; i++)
	{
		int w = GetEntPropEnt(c, Prop_Send, "m_hMyWeapons", i);
		if (w != -1 && (IsCuchilloBase(w) || (ClaveArma(w, arma, sizeof(arma)) && EsCuchillo(arma))))
			return w;
	}
	return -1;
}

// Gives a new weapon with the skin applied (replaces 'vieja' if given, keeping its ammo and whether it was in hand).
int Dar(int c, int vieja, const char[] arma)
{
	int clip = -1, reserva = -1;
	bool enMano = false;
	if (vieja != -1)
	{
		enMano = GetEntPropEnt(c, Prop_Send, "m_hActiveWeapon") == vieja;
		clip = GetEntProp(vieja, Prop_Send, "m_iClip1");
		reserva = GetEntProp(vieja, Prop_Send, "m_iPrimaryReserveAmmoCount");
		RemovePlayerItem(c, vieja);
		AcceptEntityInput(vieja, "Kill");
	}
	g_Dando[c] = true;
	int w = GivePlayerItem(c, arma);
	g_Dando[c] = false;
	if (w == -1)
	{
		LogError("Could not give %s", arma);
		if (EsCuchillo(arma))  // never leave a player without a knife
			GivePlayerItem(c, GetClientTeam(c) == CS_TEAM_T ? "weapon_knife_t" : "weapon_knife");
		return -1;
	}
	Pintar(c, w, arma);
	if (EsCuchillo(arma))
		EquipPlayerWeapon(c, w);
	else if (clip >= 0)
	{
		SetEntProp(w, Prop_Send, "m_iClip1", clip);
		SetEntProp(w, Prop_Send, "m_iPrimaryReserveAmmoCount", reserva);
	}
	if (enMano)
		FakeClientCommandEx(c, "use %s", arma);
	return w;
}

public void CambiarCuchillo(int userid)
{
	int c = GetClientOfUserId(userid);
	if (c == 0 || !IsPlayerAlive(c) || g_Prefs[c] == null)
		return;
	char modelo[64];
	int k = BuscarCuchillo(c);
	if (k == -1)
		return;
	if (!g_Prefs[c].GetString(CLAVE_CUCHILLO, modelo, sizeof(modelo)))
		strcopy(modelo, sizeof(modelo), GetClientTeam(c) == CS_TEAM_T ? "weapon_knife_t" : "weapon_knife");
	Dar(c, k, modelo);
}

// Gives back the weapons of 'solo' (or all if empty) so the skin shows right away.
// With 'solo' a chosen "no skin" has to apply too; without it only weapons with a saved skin change.
void Refrescar(int c, const char[] solo)
{
	char arma[64], v[12];
	int armas[64], n = 0;
	int size = GetEntPropArraySize(c, Prop_Send, "m_hMyWeapons");
	// copied first: giving a weapon changes the array
	for (int i = 0; i < size && n < sizeof(armas); i++)
	{
		int w = GetEntPropEnt(c, Prop_Send, "m_hMyWeapons", i);
		if (w != -1)
			armas[n++] = w;
	}
	bool cuchillo = false;
	for (int i = 0; i < n; i++)
	{
		int w = armas[i];
		bool clave = ClaveArma(w, arma, sizeof(arma));
		if (IsCuchilloBase(w) || (clave && EsCuchillo(arma)))
			cuchillo = true;
		else if (clave && (solo[0] ? StrEqual(solo, arma) : g_Prefs[c].GetString(arma, v, sizeof(v))))
			Dar(c, w, arma);
	}
	if (cuchillo && (solo[0] ? (StrEqual(solo, CLAVE_CUCHILLO) || EsCuchillo(solo)) : g_Prefs[c].GetString(CLAVE_CUCHILLO, v, sizeof(v))))
		RequestFrame(CambiarCuchillo, GetClientUserId(c));
}

// --- menus -------------------------------------------------------------------

public Action CmdWs(int c, int args)
{
	if (c == 0 || g_Prefs[c] == null)
		return Plugin_Handled;
	char arma[64];
	int w = IsPlayerAlive(c) ? GetEntPropEnt(c, Prop_Send, "m_hActiveWeapon") : -1;
	if (w != -1 && IsCuchilloBase(w))
	{
		// the default knife has no skins: first choose a model
		if (!g_Prefs[c].GetString(CLAVE_CUCHILLO, arma, sizeof(arma)))
			return CmdKnife(c, 0);
	}
	else if (w == -1 || !ClaveArma(w, arma, sizeof(arma)))
		arma[0] = '\0';
	if (arma[0])
		MenuSkins(c, arma);
	else
		MenuArmasSkin(c);
	return Plugin_Handled;
}

// Menu entry in the player's language, marked if it is the current choice.
void AddOption(Menu m, const char[] info, const char[] phrase, int c, bool current)
{
	char text[128];
	Format(text, sizeof(text), "%T", phrase, c);
	if (current)
		Format(text, sizeof(text), "%T", "skins_menu_current", c, text);
	m.AddItem(info, text);
}

void MenuArmasSkin(int c)
{
	Menu m = new Menu(OnArmaSkin);
	m.SetTitle("%T", "skins_menu_which", c);
	char arma[64], nombre[64];
	g_Kv.Rewind();
	g_Kv.JumpToKey("armas");
	if (g_Kv.GotoFirstSubKey())
	{
		do
		{
			g_Kv.GetSectionName(arma, sizeof(arma));
			g_Kv.GetString("nombre", nombre, sizeof(nombre), arma);
			m.AddItem(arma, nombre);
		} while (g_Kv.GotoNextKey());
	}
	g_Kv.Rewind();
	m.Display(c, MENU_TIME_FOREVER);
}

public int OnArmaSkin(Menu m, MenuAction a, int c, int item)
{
	if (a == MenuAction_Select)
	{
		char arma[64];
		m.GetItem(item, arma, sizeof(arma));
		MenuSkins(c, arma);
	}
	else if (a == MenuAction_End)
		delete m;
	return 0;
}

void MenuSkins(int c, const char[] arma)
{
	char nombre[64], id[12], skin[96], actual[12];
	g_Prefs[c].GetString(arma, actual, sizeof(actual));
	Menu m = new Menu(OnSkin);
	g_Kv.Rewind();
	g_Kv.JumpToKey("armas");
	if (!g_Kv.JumpToKey(arma))
	{
		g_Kv.Rewind();
		delete m;
		PrintToChat(c, " \x04[Skins]\x01 %t", "skins_none");
		return;
	}
	g_Kv.GetString("nombre", nombre, sizeof(nombre), arma);
	m.SetTitle("%T", "skins_menu_for", c, nombre);
	Format(skin, sizeof(skin), "%s|", arma);
	AddOption(m, skin, "skins_menu_none", c, !actual[0]);
	AddOption(m, "_armas", "skins_menu_other", c, false);
	if (g_Kv.JumpToKey("pintura") && g_Kv.GotoFirstSubKey(false))
	{
		do
		{
			g_Kv.GetSectionName(id, sizeof(id));
			g_Kv.GetString(NULL_STRING, nombre, sizeof(nombre));
			char info[80];
			Format(info, sizeof(info), "%s|%s", arma, id);
			if (StrEqual(id, actual))
				Format(nombre, sizeof(nombre), "%T", "skins_menu_current", c, nombre);
			m.AddItem(info, nombre);
		} while (g_Kv.GotoNextKey(false));
	}
	g_Kv.Rewind();
	m.Display(c, MENU_TIME_FOREVER);
}

public int OnSkin(Menu m, MenuAction a, int c, int item)
{
	if (a == MenuAction_Select)
	{
		char info[80], partes[2][64];
		m.GetItem(item, info, sizeof(info));
		if (StrEqual(info, "_armas"))
		{
			MenuArmasSkin(c);
			return 0;
		}
		ExplodeString(info, "|", partes, 2, 64);
		Guardar(c, partes[0], partes[1]);
		if (IsPlayerAlive(c))
			Refrescar(c, partes[0]);
		PrintToChat(c, " \x04[Skins]\x01 %t", "skins_saved");
		MenuSkins(c, partes[0]);
	}
	else if (a == MenuAction_End)
		delete m;
	return 0;
}

public Action CmdKnife(int c, int args)
{
	if (c == 0 || g_Prefs[c] == null)
		return Plugin_Handled;
	char arma[64], nombre[64], actual[64];
	g_Prefs[c].GetString(CLAVE_CUCHILLO, actual, sizeof(actual));
	Menu m = new Menu(OnKnife);
	m.SetTitle("%T", "skins_menu_knife", c);
	AddOption(m, "", "skins_menu_default", c, !actual[0]);
	g_Kv.Rewind();
	g_Kv.JumpToKey("cuchillos");
	if (g_Kv.GotoFirstSubKey(false))
	{
		do
		{
			g_Kv.GetSectionName(arma, sizeof(arma));
			g_Kv.GetString(NULL_STRING, nombre, sizeof(nombre));
			if (StrEqual(arma, actual))
				Format(nombre, sizeof(nombre), "%T", "skins_menu_current", c, nombre);
			m.AddItem(arma, nombre);
		} while (g_Kv.GotoNextKey(false));
	}
	g_Kv.Rewind();
	m.Display(c, MENU_TIME_FOREVER);
	return Plugin_Handled;
}

public int OnKnife(Menu m, MenuAction a, int c, int item)
{
	if (a == MenuAction_Select)
	{
		char arma[64];
		m.GetItem(item, arma, sizeof(arma));
		Guardar(c, CLAVE_CUCHILLO, arma);
		if (IsPlayerAlive(c))
			RequestFrame(CambiarCuchillo, GetClientUserId(c));
		if (arma[0])
			MenuSkins(c, arma);  // and straight to the knife skin
	}
	else if (a == MenuAction_End)
		delete m;
	return 0;
}

// --- !armas: any weapon ---------------------------------------------------

static const char PISTOLAS[][] = { "weapon_glock", "weapon_hkp2000", "weapon_usp_silencer", "weapon_p250",
	"weapon_fiveseven", "weapon_tec9", "weapon_cz75a", "weapon_deagle", "weapon_revolver", "weapon_elite" };

bool EsPistola(const char[] arma)
{
	for (int i = 0; i < sizeof(PISTOLAS); i++)
		if (StrEqual(arma, PISTOLAS[i]))
			return true;
	return false;
}

public Action CmdArmas(int c, int args)
{
	if (c == 0)
		return Plugin_Handled;
	if (!g_ArmasLibres.BoolValue)
	{
		PrintToChat(c, " \x04[Skins]\x01 %t", "skins_guns_free_only");
		return Plugin_Handled;
	}
	Menu m = new Menu(OnArmas);
	m.SetTitle("%T", "skins_menu_weapon", c);
	char arma[64], nombre[64];
	g_Kv.Rewind();
	g_Kv.JumpToKey("armas");
	if (g_Kv.GotoFirstSubKey())
	{
		do
		{
			g_Kv.GetSectionName(arma, sizeof(arma));
			if (EsCuchillo(arma) || StrEqual(arma, "weapon_taser"))
				continue;
			g_Kv.GetString("nombre", nombre, sizeof(nombre), arma);
			m.AddItem(arma, nombre);
		} while (g_Kv.GotoNextKey());
	}
	g_Kv.Rewind();
	m.Display(c, MENU_TIME_FOREVER);
	return Plugin_Handled;
}

public int OnArmas(Menu m, MenuAction a, int c, int item)
{
	if (a == MenuAction_Select && IsPlayerAlive(c) && g_ArmasLibres.BoolValue)
	{
		char arma[64];
		m.GetItem(item, arma, sizeof(arma));
		int slot = EsPistola(arma) ? CS_SLOT_SECONDARY : CS_SLOT_PRIMARY;
		Dar(c, GetPlayerWeaponSlot(c, slot), arma);
		CmdArmas(c, 0);  // new menu to choose another
	}
	else if (a == MenuAction_End)
		delete m;
	return 0;
}

public Action CmdEstado(int args)
{
	char id[32], clase[64];
	for (int c = 1; c <= MaxClients; c++)
		if (IsClientInGame(c) && !IsFakeClient(c))
		{
			SteamId64(c, id, sizeof(id));
			PrintToServer("%N saved=%s now=%s skins=%d", c, g_SteamId[c], id, g_Prefs[c] == null ? -1 : g_Prefs[c].Size);
			if (!IsPlayerAlive(c))
				continue;
			int n = GetEntPropArraySize(c, Prop_Send, "m_hMyWeapons");
			for (int i = 0; i < n; i++)
			{
				int w = GetEntPropEnt(c, Prop_Send, "m_hMyWeapons", i);
				if (w == -1)
					continue;
				GetEntityClassname(w, clase, sizeof(clase));
				PrintToServer("  %s def=%d paint=%d wear=%.4f idlow=%d idhigh=%d quality=%d account=%d", clase, GetEntProp(w, Prop_Send, "m_iItemDefinitionIndex"),
					GetEntProp(w, Prop_Send, "m_nFallbackPaintKit"), GetEntPropFloat(w, Prop_Send, "m_flFallbackWear"), GetEntProp(w, Prop_Send, "m_iItemIDLow"),
					GetEntProp(w, Prop_Send, "m_iItemIDHigh"), GetEntProp(w, Prop_Send, "m_iEntityQuality"), GetEntProp(w, Prop_Send, "m_iAccountID"));
			}
		}
	return Plugin_Handled;
}
