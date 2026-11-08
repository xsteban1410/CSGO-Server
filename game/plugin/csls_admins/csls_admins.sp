// SPDX-License-Identifier: GPL-3.0-or-later
// Admins and roles from the app's config (configs/csls_admins.cfg, written by deploy).
// The server runs with -insecure, so a SteamID can be faked: the flags that give the server console
// ("login" in the file) are only granted after sm_login <password> in the game console.
#include <sourcemod>
#pragma semicolon 1
#pragma newdecls required

#define ADMINS_FILE "configs/csls_admins.cfg"
#define MAX_TRIES 3

public Plugin myinfo =
{
	name = "CSLS admins",
	author = "xsteban1410",
	description = "Admins and roles from the app, with a password for the console",
	version = "1.0",
	url = "https://github.com/xsteban1410/CSGO-Server"
};

KeyValues g_Admins;
bool g_LoggedIn[MAXPLAYERS + 1];
int g_Tries[MAXPLAYERS + 1];

public void OnPluginStart()
{
	LoadTranslations("csls_admins.phrases");
	RegConsoleCmd("sm_login", Cmd_Login, "sm_login <password>: unlock your role's console permissions");
	LoadAdmins();
}

void LoadAdmins()
{
	delete g_Admins;
	char path[PLATFORM_MAX_PATH];
	BuildPath(Path_SM, path, sizeof(path), ADMINS_FILE);
	g_Admins = new KeyValues("Admins");
	if (FileExists(path) && !g_Admins.ImportFromFile(path))
		LogError("cannot read %s", path);
}

// sm_reloadadmins rebuilds the cache, which drops the admins this plugin made: read the file again.
// SourceMod then checks every connected player again, which calls OnClientPostAdminFilter.
public void OnRebuildAdminCache(AdminCachePart part)
{
	if (part == AdminCache_Admins)
		LoadAdmins();
}

public void OnClientConnected(int client)
{
	g_LoggedIn[client] = false;
	g_Tries[client] = 0;
}

// Positions the KeyValues on the player's block. False if they are not an admin.
bool JumpTo(int client)
{
	int account = GetSteamAccountID(client, false);
	if (account == 0 || g_Admins == null)
		return false;
	char key[16];
	IntToString(account, key, sizeof(key));
	g_Admins.Rewind();
	return g_Admins.JumpToKey(key);
}

void AddFlags(AdminId admin, const char[] letters)
{
	for (int i = 0; letters[i] != '\0'; i++)
	{
		AdminFlag f;
		if (FindFlagByChar(letters[i], f))
			admin.SetFlag(f, true);
	}
}

bool OtherWithAccount(int client)
{
	int account = GetSteamAccountID(client, false);
	for (int i = 1; i <= MaxClients; i++)
		if (i != client && IsClientConnected(i) && !IsFakeClient(i) && GetSteamAccountID(i, false) == account)
			return true;
	return false;
}

public void OnClientPostAdminFilter(int client)
{
	if (IsFakeClient(client) || !JumpTo(client))
		return;
	// two players with an admin's SteamID: one of them faked it, and the second one in is not trusted
	if (OtherWithAccount(client))
	{
		LogAction(-1, client, "\"%L\" joined with the SteamID of an admin already playing: kicked", client);
		KickClient(client, "%T", "admin_duplicate", client);
		return;
	}
	char name[64], flags[32], login[32];
	g_Admins.GetString("name", name, sizeof(name));
	g_Admins.GetString("flags", flags, sizeof(flags));
	g_Admins.GetString("login", login, sizeof(login));
	AdminId admin = GetUserAdmin(client);
	if (admin == INVALID_ADMIN_ID)
	{
		admin = CreateAdmin(name);
		SetUserAdmin(client, admin, true);
	}
	AddFlags(admin, flags);
	if (g_LoggedIn[client])
		AddFlags(admin, login);
}

// "!login secret" in chat would show the password to everyone
public Action OnClientSayCommand(int client, const char[] command, const char[] text)
{
	if (client > 0 && (StrContains(text, "!login", false) == 0 || StrContains(text, "/login", false) == 0))
	{
		PrintToChat(client, "%T", "admin_login_console", client);
		return Plugin_Stop;
	}
	return Plugin_Continue;
}

public Action Cmd_Login(int client, int args)
{
	if (client == 0)
		return Plugin_Handled;
	if (GetCmdReplySource() == SM_REPLY_TO_CHAT)
	{
		PrintToChat(client, "%T", "admin_login_console", client);
		return Plugin_Handled;
	}
	char login[32], password[64], typed[64];
	if (!JumpTo(client))
	{
		ReplyToCommand(client, "%T", "admin_login_none", client);
		return Plugin_Handled;
	}
	g_Admins.GetString("login", login, sizeof(login));
	g_Admins.GetString("password", password, sizeof(password));
	if (login[0] == '\0' || password[0] == '\0')
	{
		ReplyToCommand(client, "%T", "admin_login_none", client);
		return Plugin_Handled;
	}
	GetCmdArgString(typed, sizeof(typed));
	TrimString(typed);
	StripQuotes(typed);
	if (!StrEqual(typed, password, true))
	{
		LogAction(client, -1, "\"%L\" typed a wrong sm_login password", client);
		if (++g_Tries[client] >= MAX_TRIES)
			KickClient(client, "%T", "admin_login_kicked", client);
		else
			ReplyToCommand(client, "%T", "admin_login_wrong", client, MAX_TRIES - g_Tries[client]);
		return Plugin_Handled;
	}
	g_LoggedIn[client] = true;
	g_Tries[client] = 0;
	AdminId admin = GetUserAdmin(client);
	if (admin != INVALID_ADMIN_ID)
		AddFlags(admin, login);
	LogAction(client, -1, "\"%L\" logged in as admin", client);
	ReplyToCommand(client, "%T", "admin_login_ok", client);
	return Plugin_Handled;
}
