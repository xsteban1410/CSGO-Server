// Package sched holds scheduled tasks: when they run and which fixed command each action is.
package sched

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// Task is an entry of schedule.json.
type Task struct {
	ID      string `json:"id"`
	Action  string `json:"action"`
	Arg     string `json:"arg,omitempty"`
	Every   string `json:"every"`             // daily, weekly or hours
	At      string `json:"at,omitempty"`      // HH:MM for daily and weekly
	Weekday int    `json:"weekday,omitempty"` // 0 is Sunday, for weekly
	Hours   int    `json:"hours,omitempty"`   // for hours
	Enabled bool   `json:"enabled"`
}

var (
	name  = regexp.MustCompile(`^[A-Za-z0-9_.-]{1,64}$`)
	clock = regexp.MustCompile(`^([01]\d|2[0-3]):[0-5]\d$`)
)

// Commands gives the CLI arguments of an action, or an error if it is not on the list. They run without a shell.
func (t Task) Commands() ([][]string, error) {
	switch t.Action {
	case "restart":
		return [][]string{{"stop"}, {"start"}}, nil
	case "backup":
		return [][]string{{"backup", "create", "--auto"}}, nil
	case "plugins":
		return [][]string{{"deploy"}}, nil
	case "map", "mode":
		if !name.MatchString(t.Arg) || strings.HasPrefix(t.Arg, "-") {
			return nil, fmt.Errorf("invalid %s name %q", t.Action, t.Arg)
		}
		if t.Action == "map" {
			return [][]string{{"mode", "--map", t.Arg}}, nil
		}
		return [][]string{{"mode", t.Arg}}, nil
	case "rcon":
		if strings.TrimSpace(t.Arg) == "" || strings.ContainsAny(t.Arg, "\r\n") {
			return nil, errors.New("invalid rcon command")
		}
		return [][]string{{"rcon", "--", t.Arg}}, nil
	}
	return nil, fmt.Errorf("unknown action %q", t.Action)
}

// Validate says whether the task can run.
func (t Task) Validate() error {
	if !name.MatchString(t.ID) {
		return fmt.Errorf("invalid task id %q", t.ID)
	}
	if _, err := t.Commands(); err != nil {
		return err
	}
	switch t.Every {
	case "hours":
		if t.Hours < 1 || t.Hours > 24*365 {
			return errors.New("hours out of range")
		}
	case "weekly":
		if t.Weekday < 0 || t.Weekday > 6 {
			return errors.New("weekday out of range")
		}
		fallthrough
	case "daily":
		if !clock.MatchString(t.At) {
			return fmt.Errorf("invalid time %q", t.At)
		}
	default:
		return fmt.Errorf("unknown frequency %q", t.Every)
	}
	return nil
}

// Next gives the first run after a moment. A validated task always has one. Runs missed while the agent was off are
// not made up: restarting a server hours late would surprise whoever is playing.
func (t Task) Next(after time.Time) time.Time {
	if t.Every == "hours" {
		return after.Truncate(time.Hour).Add(time.Duration(t.Hours) * time.Hour)
	}
	h, _ := strconv.Atoi(t.At[:2])
	m, _ := strconv.Atoi(t.At[3:])
	for d := 0; d < 9; d++ {
		c := time.Date(after.Year(), after.Month(), after.Day()+d, h, m, 0, 0, after.Location())
		if c.After(after) && (t.Every == "daily" || int(c.Weekday()) == t.Weekday) {
			return c
		}
	}
	return after.Add(24 * time.Hour)
}

// Load reads the tasks. A missing file is no tasks.
func Load(path string) ([]Task, error) {
	b, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return []Task{}, nil
	}
	if err != nil {
		return nil, err
	}
	tasks := []Task{}
	if err := json.Unmarshal(b, &tasks); err != nil {
		return nil, fmt.Errorf("%s: %w", path, err)
	}
	return tasks, Check(tasks)
}

// Check validates a list.
func Check(tasks []Task) error {
	seen := map[string]bool{}
	for _, t := range tasks {
		if err := t.Validate(); err != nil {
			return fmt.Errorf("task %q: %w", t.ID, err)
		}
		if seen[t.ID] {
			return fmt.Errorf("repeated task %q", t.ID)
		}
		seen[t.ID] = true
	}
	return nil
}

// Save writes the tasks atomically.
func Save(path string, tasks []Task) error {
	if err := Check(tasks); err != nil {
		return err
	}
	b, err := json.MarshalIndent(tasks, "", "  ")
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, b, 0o600); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}
