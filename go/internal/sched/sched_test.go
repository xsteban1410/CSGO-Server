package sched

import (
	"path/filepath"
	"testing"
	"time"
)

func TestNext(t *testing.T) {
	// Friday 2026-11-06 22:00 UTC
	now := time.Date(2026, 11, 6, 22, 0, 0, 0, time.UTC)
	for _, c := range []struct {
		task Task
		want time.Time
	}{
		{Task{Every: "daily", At: "23:30"}, time.Date(2026, 11, 6, 23, 30, 0, 0, time.UTC)},
		{Task{Every: "daily", At: "04:00"}, time.Date(2026, 11, 7, 4, 0, 0, 0, time.UTC)},
		{Task{Every: "weekly", At: "04:00", Weekday: 0}, time.Date(2026, 11, 8, 4, 0, 0, 0, time.UTC)},
		{Task{Every: "weekly", At: "21:00", Weekday: 5}, time.Date(2026, 11, 13, 21, 0, 0, 0, time.UTC)},
		{Task{Every: "hours", Hours: 6}, time.Date(2026, 11, 7, 4, 0, 0, 0, time.UTC)},
	} {
		if got := c.task.Next(now); !got.Equal(c.want) {
			t.Errorf("%+v: got %v, want %v", c.task, got, c.want)
		}
	}
}

func TestOnlyListedActionsWithSafeArguments(t *testing.T) {
	good := Task{ID: "a", Action: "map", Arg: "de_dust2", Every: "daily", At: "04:00"}
	if err := good.Validate(); err != nil {
		t.Fatal(err)
	}
	for _, bad := range []Task{
		{ID: "a", Action: "shell", Arg: "rm -rf /", Every: "daily", At: "04:00"},
		{ID: "a", Action: "map", Arg: "de_dust2; quit", Every: "daily", At: "04:00"},
		{ID: "a", Action: "mode", Arg: "--help", Every: "daily", At: "04:00"},
		{ID: "a", Action: "rcon", Arg: "a\nquit", Every: "daily", At: "04:00"},
		{ID: "../a", Action: "backup", Every: "daily", At: "04:00"},
		{ID: "a", Action: "backup", Every: "daily", At: "25:00"},
		{ID: "a", Action: "backup", Every: "hours"},
		{ID: "a", Action: "backup", Every: "weekly", At: "04:00", Weekday: 7},
		{ID: "a", Action: "backup", Every: "sometimes"},
	} {
		if bad.Validate() == nil {
			t.Errorf("accepted %+v", bad)
		}
	}
}

func TestSaveAndLoad(t *testing.T) {
	p := filepath.Join(t.TempDir(), "x", "schedule.json")
	if l, err := Load(p); err != nil || len(l) != 0 {
		t.Fatal(l, err)
	}
	tasks := []Task{{ID: "b", Action: "backup", Every: "daily", At: "04:00", Enabled: true}}
	if err := Save(p, tasks); err != nil {
		t.Fatal(err)
	}
	if l, err := Load(p); err != nil || len(l) != 1 || !l[0].Enabled {
		t.Fatal(l, err)
	}
	if Save(p, append(tasks, tasks[0])) == nil {
		t.Fatal("repeated ids")
	}
}
