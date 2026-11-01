#pragma semicolon 1

#include <sourcemod>

#pragma newdecls required

public Plugin myinfo = 
{
	name = "No Lobby Reservation",
	author = "vanz",
	version = "0.0.2"
};

enum struct mem_patch
{
	Address addr;
	int len;
	char patch[256];
	char orig[256];

	bool Init(GameData conf, const char[] key, Address addr)
	{
		int offset, pos, curPos;
		char byte[16], bytes[512];
		
		if (this.len)
			return false;
		
		if (!conf.GetKeyValue(key, bytes, sizeof(bytes)))
			return false;
		
		offset = conf.GetOffset(key);
		
		if (offset == -1)
			offset = 0;
		
		this.addr = addr + view_as<Address>(offset);
		
		StrCat(bytes, sizeof(bytes), " ");
		
		while ((pos = SplitString(bytes[curPos], " ", byte, sizeof(byte))) != -1)
		{
			curPos += pos;
			
			TrimString(byte);
			
			if (byte[0])
			{
				this.patch[this.len] = StringToInt(byte, 16);
				this.orig[this.len] = LoadFromAddress(this.addr + view_as<Address>(this.len), NumberType_Int8);
				this.len++;
			}
		}
		
		return true;
	}
	
	void Apply()
	{
		for (int i = 0; i < this.len; i++)
			StoreToAddress(this.addr + view_as<Address>(i), this.patch[i], NumberType_Int8);
	}
	
	void Restore()
	{
		for (int i = 0; i < this.len; i++)
			StoreToAddress(this.addr + view_as<Address>(i), this.orig[i], NumberType_Int8);
	}
}

mem_patch g_isExclusiveToLobbyConnectionsPatch;
mem_patch g_lobbyIdPatch;

public void OnPluginStart()
{
	GameData conf = new GameData("nolobbyreservation.games");
	
	if (conf == null) 
		SetFailState("Failed to load nolobbyreservation gamedata");

	Address isExclusiveToLobbyConnectionsAddr = conf.GetAddress("CBaseServer::IsExclusiveToLobbyConnections");

	if (!isExclusiveToLobbyConnectionsAddr) 
		SetFailState("Failed to load CBaseServer::IsExclusiveToLobbyConnections signature from gamedata");

	g_isExclusiveToLobbyConnectionsPatch.Init(conf, "CBaseServer::IsExclusiveToLobbyConnections_Patch", isExclusiveToLobbyConnectionsAddr);

	Address lobbyIdAddr = conf.GetAddress("CBaseServer::ReplyChallenge_LobbyId");

	if (!lobbyIdAddr) 
		SetFailState("Failed to load CBaseServer::ReplyChallenge_LobbyId signature from gamedata");

	g_lobbyIdPatch.Init(conf, "CBaseServer::ReplyChallenge_LobbyId_Patch", lobbyIdAddr);

	g_isExclusiveToLobbyConnectionsPatch.Apply();
	g_lobbyIdPatch.Apply();

	delete conf;
}

public void OnPluginEnd()
{
	g_isExclusiveToLobbyConnectionsPatch.Restore();
	g_lobbyIdPatch.Restore();
}
