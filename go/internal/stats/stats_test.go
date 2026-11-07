package stats

import (
	"path/filepath"
	"testing"
	"time"
)

func open(t *testing.T) (*DB, string) {
	t.Helper()
	p := filepath.Join(t.TempDir(), "stats.db")
	d, err := Open(p)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { d.Close() })
	return d, p
}

func TestHistory(t *testing.T) {
	d, _ := open(t)
	now := time.Unix(1_800_000_000, 0)
	must := func(err error) {
		t.Helper()
		if err != nil {
			t.Fatal(err)
		}
	}
	must(d.Connect(now, "STEAM_1:0:1", "Old name"))
	must(d.Disconnect(now.Add(time.Minute), "STEAM_1:0:1"))
	must(d.Connect(now.Add(time.Hour), "STEAM_1:0:1", "New name"))
	p, err := d.Players(now.Add(time.Hour + 30*time.Second))
	must(err)
	if len(p) != 1 || p[0].Name != "New name" || p[0].Connections != 2 || p[0].Seconds != 90 {
		t.Fatalf("%+v", p)
	}
	must(d.CloseOpen(now.Add(2 * time.Hour)))
	must(d.MapPlayed(now, "de_dust2"))
	must(d.MapPlayed(now, "de_dust2"))
	must(d.MapPlayed(now, "de_inferno"))
	must(d.ModePlayed(now, "awp"))
	must(d.Sample(now, 2))
	must(d.Sample(now, 4))
	s, err := d.Summarize(now.Add(-time.Hour))
	must(err)
	if s.Maps[0].Name != "de_dust2" || s.Maps[0].Count != 2 || s.Modes[0].Name != "awp" || s.AvgPlayers != 3 {
		t.Fatalf("%+v", s)
	}
	if len(s.PerDay) != 1 || s.PerDay[0].Count != 1 {
		t.Fatalf("%+v", s.PerDay)
	}
}

func TestPruneKeepsSanctionsAndEraseAllDoesNot(t *testing.T) {
	d, _ := open(t)
	old := time.Unix(1_000_000_000, 0)
	now := time.Unix(1_800_000_000, 0)
	_ = d.Connect(old, "banned", "x")
	_ = d.Disconnect(old, "banned")
	_ = d.Connect(old, "gone", "y")
	_ = d.Disconnect(old, "gone")
	if err := d.AddSanction(Sanction{SteamID: "banned", Name: "x", Kind: "ban", Reason: "r", Minutes: 0, Time: old.Unix()}); err != nil {
		t.Fatal(err)
	}
	if err := d.Prune(now); err != nil {
		t.Fatal(err)
	}
	p, _ := d.Players(now)
	if len(p) != 1 || p[0].SteamID != "banned" {
		t.Fatalf("%+v", p)
	}
	if s, _ := d.Sanctions(); len(s) != 1 {
		t.Fatalf("%+v", s)
	}
	if err := d.EraseAll(); err != nil {
		t.Fatal(err)
	}
	if s, _ := d.Sanctions(); len(s) != 0 {
		t.Fatalf("%+v", s)
	}
}

func TestReopenKeepsDataAndRefusesNewerDatabase(t *testing.T) {
	d, p := open(t)
	_ = d.Connect(time.Unix(1, 0), "a", "b")
	d.Close()
	d2, err := Open(p)
	if err != nil {
		t.Fatal(err)
	}
	if l, _ := d2.Players(time.Unix(2, 0)); len(l) != 1 {
		t.Fatalf("%+v", l)
	}
	if _, err := d2.db.Exec("PRAGMA user_version = 99"); err != nil {
		t.Fatal(err)
	}
	d2.Close()
	if _, err := Open(p); err == nil {
		t.Fatal("a newer database must not be touched")
	}
}
