// SPDX-License-Identifier: GPL-3.0-or-later
// Skins, cuchillos y armas a elección, guardados por jugador (SQLite).
//   !ws      skin del arma en la mano (o de otra arma)
//   !knife   modelo de cuchillo (también !cuchillo)
//   !armas   cualquier arma (solo si skins_armas_libres = 1, p.ej. en el modo "libre"; también !guns)
// La lista de skins sale de configs/skins.cfg (csgo-legacy-server skins).
#pragma semicolon 1
#include <sourcemod>
#include <sdktools>
#include <sdkhooks>
#include <cstrike>
#pragma newdecls required

public Plugin myinfo = { name = "Skins", author = "xsteban1410", version = "1.0" };

#define CLAVE_CUCHILLO "cuchillo"
#define DESGASTE 0.0001  // Recién fabricado

KeyValues g_Kv;
StringMap g_PorIndice;          // "60" -> "weapon_m4a1_silencer"
StringMap g_Prefs[MAXPLAYERS + 1];
char g_SteamId[MAXPLAYERS + 1][24];
bool g_Dando[MAXPLAYERS + 1];   // evita reentrar en el hook mientras damos un arma
Database g_Db;
ConVar g_ArmasLibres;

public void OnPluginStart()
{
	LoadTranslations("csls.phrases");
	g_ArmasLibres = CreateConVar("skins_armas_libres", "0", "1 = permite !armas para coger cualquier arma");
	RegConsoleCmd("sm_ws", CmdWs, "Elegir skin");
	RegConsoleCmd("sm_skins", CmdWs, "Elegir skin");
	RegConsoleCmd("sm_knife", CmdKnife, "Elegir cuchillo");
	RegConsoleCmd("sm_cuchillo", CmdKnife, "Elegir cuchillo");
	RegConsoleCmd("sm_armas", CmdArmas, "Elegir cualquier arma");
	RegConsoleCmd("sm_guns", CmdArmas, "Elegir cualquier arma");
	RegServerCmd("skins_estado", CmdEstado, "Muestra el SteamID y las skins cargadas de cada jugador");
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
		SetFailState("No pude leer %s (genéralo con: csgo-legacy-server skins)", path);
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

// --- base de datos ---------------------------------------------------------

public void OnDb(Database db, const char[] error, any data)
{
	if (db == null)
	{
		LogError("Sin base de datos, las skins no se guardarán: %s", error);
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

// El SteamID llega después de entrar: se carga cuando el servidor lo confirma.
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
	// validate=false: el servidor es -insecure y Steam no valida el ID, pero sigue siendo el del jugador.
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

// SteamID64 del jugador. Con el servidor en modo LAN/-insecure, SourceMod no lo da (devuelve
// "STEAM_ID_STOP_IGNORING_RETVALS"), pero sí el número de cuenta: 76561197960265728 + cuenta.
bool SteamId64(int c, char[] out, int len)
{
	if (GetClientAuthId(c, AuthId_SteamID64, out, len, false) && strlen(out) == 17 && strncmp(out, "7656", 4) == 0)
		return true;
	int cuenta = GetSteamAccountID(c, false);
	if (cuenta <= 0)
		return false;
	// Suma en dos mitades de 9 cifras: los enteros de SourcePawn son de 32 bits
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
	SetEntProp(w, Prop_Send, "m_iItemIDLow", -1);
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
		RequestFrame(CambiarCuchillo, GetClientUserId(c));  // no se puede quitar un arma dentro del propio hook
		return;
	}
	Pintar(c, w, arma);
}

bool IsCuchilloBase(int w)
{
	int i = GetEntProp(w, Prop_Send, "m_iItemDefinitionIndex");
	return i == 42 || i == 59;  // cuchillo CT / T por defecto
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

// Da un arma nueva con la skin aplicada (sustituye a 'vieja' si se indica).
int Dar(int c, int vieja, const char[] arma)
{
	if (vieja != -1)
	{
		RemovePlayerItem(c, vieja);
		AcceptEntityInput(vieja, "Kill");
	}
	g_Dando[c] = true;
	int w = GivePlayerItem(c, arma);
	g_Dando[c] = false;
	if (w != -1)
	{
		Pintar(c, w, arma);
		EquipPlayerWeapon(c, w);
	}
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

// Vuelve a dar las armas que tenga de 'solo' (o todas si está vacío) para que se vea la skin al momento.
void Refrescar(int c, const char[] solo)
{
	char arma[64];
	int n = GetEntPropArraySize(c, Prop_Send, "m_hMyWeapons");
	for (int i = 0; i < n; i++)
	{
		int w = GetEntPropEnt(c, Prop_Send, "m_hMyWeapons", i);
		if (w == -1)
			continue;
		bool cuchillo = IsCuchilloBase(w) || (ClaveArma(w, arma, sizeof(arma)) && EsCuchillo(arma));
		if (cuchillo && (!solo[0] || StrEqual(solo, CLAVE_CUCHILLO) || EsCuchillo(solo)))
			RequestFrame(CambiarCuchillo, GetClientUserId(c));
		else if (!cuchillo && ClaveArma(w, arma, sizeof(arma)) && (!solo[0] || StrEqual(solo, arma)))
			Dar(c, w, arma);
	}
}

// --- menús -------------------------------------------------------------------

public Action CmdWs(int c, int args)
{
	if (c == 0 || g_Prefs[c] == null)
		return Plugin_Handled;
	char arma[64];
	int w = GetEntPropEnt(c, Prop_Send, "m_hActiveWeapon");
	if (w != -1 && IsCuchilloBase(w))
		g_Prefs[c].GetString(CLAVE_CUCHILLO, arma, sizeof(arma));
	else if (w == -1 || !ClaveArma(w, arma, sizeof(arma)))
		arma[0] = '\0';
	if (arma[0])
		MenuSkins(c, arma);
	else
		MenuArmasSkin(c);
	return Plugin_Handled;
}

void MenuArmasSkin(int c)
{
	Menu m = new Menu(OnArmaSkin);
	m.SetTitle("¿Skin para qué arma?");
	char arma[64], nombre[64];
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
	g_Kv.JumpToKey("armas");
	if (!g_Kv.JumpToKey(arma))
	{
		g_Kv.Rewind();
		delete m;
		PrintToChat(c, " \x04[Skins]\x01 %t", "skins_none");
		return;
	}
	g_Kv.GetString("nombre", nombre, sizeof(nombre), arma);
	m.SetTitle("Skin para %s", nombre);
	Format(skin, sizeof(skin), "%s|", arma);
	m.AddItem(skin, actual[0] ? "Sin skin" : "Sin skin (actual)");
	m.AddItem("_armas", "« Otra arma");
	if (g_Kv.JumpToKey("pintura") && g_Kv.GotoFirstSubKey(false))
	{
		do
		{
			g_Kv.GetSectionName(id, sizeof(id));
			g_Kv.GetString(NULL_STRING, nombre, sizeof(nombre));
			char info[80];
			Format(info, sizeof(info), "%s|%s", arma, id);
			if (StrEqual(id, actual))
				Format(nombre, sizeof(nombre), "%s (actual)", nombre);
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
	m.SetTitle("Elige cuchillo");
	m.AddItem("", actual[0] ? "Por defecto" : "Por defecto (actual)");
	g_Kv.JumpToKey("cuchillos");
	if (g_Kv.GotoFirstSubKey(false))
	{
		do
		{
			g_Kv.GetSectionName(arma, sizeof(arma));
			g_Kv.GetString(NULL_STRING, nombre, sizeof(nombre));
			if (StrEqual(arma, actual))
				Format(nombre, sizeof(nombre), "%s (actual)", nombre);
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
			MenuSkins(c, arma);  // y directamente la skin del cuchillo
	}
	else if (a == MenuAction_End)
		delete m;
	return 0;
}

// --- !armas: cualquier arma ---------------------------------------------------

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
	m.SetTitle("Elige arma");
	char arma[64], nombre[64];
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
		CmdArmas(c, 0);  // menú nuevo para elegir otra
	}
	else if (a == MenuAction_End)
		delete m;
	return 0;
}

public Action CmdEstado(int args)
{
	char id[32];
	for (int c = 1; c <= MaxClients; c++)
		if (IsClientInGame(c) && !IsFakeClient(c))
		{
			SteamId64(c, id, sizeof(id));
			PrintToServer("%N guardado=%s ahora=%s skins=%d", c, g_SteamId[c], id, g_Prefs[c] == null ? -1 : g_Prefs[c].Size);
		}
	return Plugin_Handled;
}
