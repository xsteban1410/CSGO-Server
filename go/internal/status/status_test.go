package status

import "testing"

const sample = `hostname: CS:GO Legacy
version : 1.38.8.1/13881 1575/8853 secure
map     : de_dust2
players : 1 humans, 2 bots (20/0 max) (not hibernating)

# userid name uniqueid connected ping loss state rate adr
#  2 "Mollie" BOT active 128
# 3 1 "Luis \"x\"" STEAM_1:0:123 01:02 40 0 active 128000 192.168.1.12:27005
#end
`

func TestParse(t *testing.T) {
	s := Parse(sample)
	if s.Hostname != "CS:GO Legacy" || s.Map != "de_dust2" || s.Humans != 1 || s.Bots != 2 || s.Max != 20 {
		t.Fatalf("%+v", s)
	}
	if len(s.Players) != 2 {
		t.Fatalf("%+v", s.Players)
	}
	if !s.Players[0].Bot || s.Players[0].Name != "Mollie" {
		t.Fatalf("%+v", s.Players[0])
	}
	h := s.Players[1]
	if h.Bot || h.SteamID != "STEAM_1:0:123" || h.Connected != "01:02" || h.Ping != 40 || h.UserID != 3 {
		t.Fatalf("%+v", h)
	}
}

func TestParseGarbage(t *testing.T) {
	if s := Parse("nonsense\n# \"\n"); len(s.Players) != 0 || s.Map != "" {
		t.Fatalf("%+v", s)
	}
}
