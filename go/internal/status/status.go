// Package status reads the output of the CS:GO `status` command, as crates/core/src/players.rs does for the app.
package status

import (
	"regexp"
	"strconv"
	"strings"
)

// Player is a line of the player table.
type Player struct {
	UserID    int    `json:"userid"`
	Name      string `json:"name"`
	SteamID   string `json:"steamid"`
	Bot       bool   `json:"bot"`
	Connected string `json:"connected"`
	Ping      int    `json:"ping"`
}

// Status is what the server says about itself.
type Status struct {
	Hostname string   `json:"hostname"`
	Map      string   `json:"map"`
	Humans   int      `json:"humans"`
	Bots     int      `json:"bots"`
	Max      int      `json:"max"`
	Players  []Player `json:"players"`
}

// # 2 1 "name" STEAM_1:0:1 00:10 40 0 active 128000 1.2.3.4:27005
// #  3 "BOT" BOT active 128
var line = regexp.MustCompile(`^#\s*(\d+)(?:\s+\d+)?\s+"(.*)"\s+(\S+)\s+(.*)$`)

var clock = regexp.MustCompile(`^\d+(:\d+)+$`)

// Parse never fails: whatever it does not understand stays empty.
func Parse(text string) Status {
	var s Status
	for _, l := range strings.Split(text, "\n") {
		l = strings.TrimSpace(l)
		switch {
		case strings.HasPrefix(l, "hostname:"):
			s.Hostname = strings.TrimSpace(strings.TrimPrefix(l, "hostname:"))
		case strings.HasPrefix(l, "map"):
			if _, v, ok := strings.Cut(l, ":"); ok {
				if f := strings.Fields(v); len(f) > 0 {
					s.Map = f[0]
				}
			}
		case strings.HasPrefix(l, "players :"):
			f := strings.Fields(strings.TrimPrefix(l, "players :"))
			if len(f) > 4 {
				s.Humans, _ = strconv.Atoi(f[0])
				s.Bots, _ = strconv.Atoi(f[2])
				m, _, _ := strings.Cut(strings.TrimPrefix(f[4], "("), "/")
				s.Max, _ = strconv.Atoi(m)
			}
		default:
			if m := line.FindStringSubmatch(l); m != nil {
				p := Player{Name: m[2], SteamID: m[3], Bot: m[3] == "BOT"}
				p.UserID, _ = strconv.Atoi(m[1])
				rest := strings.Fields(m[4])
				if len(rest) > 1 && clock.MatchString(rest[0]) {
					p.Connected = rest[0]
					p.Ping, _ = strconv.Atoi(rest[1])
				}
				s.Players = append(s.Players, p)
			}
		}
	}
	return s
}
