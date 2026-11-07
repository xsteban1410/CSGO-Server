// Package stats keeps local history in SQLite: connections, players, maps, modes, hourly samples and sanctions.
// It stores SteamIDs and names, never IP addresses (docs/PRIVACY.md).
package stats

import (
	"database/sql"
	"fmt"
	"time"

	_ "modernc.org/sqlite" // the driver, without cgo
)

// DB is the stats database.
type DB struct{ db *sql.DB }

// migrations run in order, once each; add to the end and never edit one already published.
var migrations = []string{
	`CREATE TABLE players (steamid TEXT PRIMARY KEY, name TEXT NOT NULL, first_seen INTEGER NOT NULL, last_seen INTEGER NOT NULL);
	 CREATE TABLE connections (id INTEGER PRIMARY KEY, steamid TEXT NOT NULL, name TEXT NOT NULL, start INTEGER NOT NULL, end INTEGER);
	 CREATE INDEX connections_steamid ON connections(steamid);
	 CREATE TABLE map_plays (id INTEGER PRIMARY KEY, map TEXT NOT NULL, start INTEGER NOT NULL);
	 CREATE TABLE mode_plays (id INTEGER PRIMARY KEY, mode TEXT NOT NULL, start INTEGER NOT NULL);
	 CREATE TABLE samples (hour INTEGER PRIMARY KEY, count INTEGER NOT NULL, humans INTEGER NOT NULL, max_humans INTEGER NOT NULL);
	 CREATE TABLE events (id INTEGER PRIMARY KEY, time INTEGER NOT NULL, kind TEXT NOT NULL, text TEXT NOT NULL);
	 CREATE TABLE sanctions (id INTEGER PRIMARY KEY, steamid TEXT NOT NULL, name TEXT NOT NULL, kind TEXT NOT NULL, reason TEXT NOT NULL,
	                         notes TEXT NOT NULL DEFAULT '', minutes INTEGER NOT NULL, time INTEGER NOT NULL);`,
}

// Open creates or migrates the database.
func Open(path string) (*DB, error) {
	db, err := sql.Open("sqlite", "file:"+path+"?_pragma=journal_mode(WAL)&_pragma=busy_timeout(5000)&_pragma=synchronous(NORMAL)")
	if err != nil {
		return nil, err
	}
	db.SetMaxOpenConns(1)
	d := &DB{db}
	if err := d.migrate(); err != nil {
		db.Close()
		return nil, err
	}
	return d, nil
}

// Close closes the database.
func (d *DB) Close() error { return d.db.Close() }

func (d *DB) migrate() error {
	var v int
	if err := d.db.QueryRow("PRAGMA user_version").Scan(&v); err != nil {
		return err
	}
	if v > len(migrations) {
		return fmt.Errorf("stats.db is version %d, newer than this app (%d)", v, len(migrations))
	}
	for i := v; i < len(migrations); i++ {
		tx, err := d.db.Begin()
		if err != nil {
			return err
		}
		if _, err = tx.Exec(migrations[i]); err == nil {
			_, err = tx.Exec(fmt.Sprintf("PRAGMA user_version = %d", i+1))
		}
		if err != nil {
			tx.Rollback()
			return fmt.Errorf("migration %d: %w", i+1, err)
		}
		if err := tx.Commit(); err != nil {
			return err
		}
	}
	return nil
}

// Connect records a player joining.
func (d *DB) Connect(now time.Time, steamid, name string) error {
	t := now.Unix()
	if _, err := d.db.Exec(`INSERT INTO players VALUES (?, ?, ?, ?)
		ON CONFLICT(steamid) DO UPDATE SET name = excluded.name, last_seen = excluded.last_seen`, steamid, name, t, t); err != nil {
		return err
	}
	_, err := d.db.Exec(`INSERT INTO connections (steamid, name, start) VALUES (?, ?, ?)`, steamid, name, t)
	return err
}

// Disconnect closes the player's open connection.
func (d *DB) Disconnect(now time.Time, steamid string) error {
	t := now.Unix()
	if _, err := d.db.Exec(`UPDATE players SET last_seen = ? WHERE steamid = ?`, t, steamid); err != nil {
		return err
	}
	_, err := d.db.Exec(`UPDATE connections SET end = ? WHERE steamid = ? AND end IS NULL`, t, steamid)
	return err
}

// CloseOpen ends every open connection, for when the server stopped without saying so.
func (d *DB) CloseOpen(now time.Time) error {
	_, err := d.db.Exec(`UPDATE connections SET end = ? WHERE end IS NULL`, now.Unix())
	return err
}

// MapPlayed records a map load.
func (d *DB) MapPlayed(now time.Time, name string) error {
	_, err := d.db.Exec(`INSERT INTO map_plays (map, start) VALUES (?, ?)`, name, now.Unix())
	return err
}

// ModePlayed records a mode being applied.
func (d *DB) ModePlayed(now time.Time, name string) error {
	_, err := d.db.Exec(`INSERT INTO mode_plays (mode, start) VALUES (?, ?)`, name, now.Unix())
	return err
}

// Sample adds a measure of the human count to the hour's aggregate.
func (d *DB) Sample(now time.Time, humans int) error {
	_, err := d.db.Exec(`INSERT INTO samples VALUES (?, 1, ?, ?)
		ON CONFLICT(hour) DO UPDATE SET count = count + 1, humans = humans + excluded.humans,
		max_humans = max(max_humans, excluded.max_humans)`, now.Truncate(time.Hour).Unix(), humans, humans)
	return err
}

// Event records something for the Dashboard.
func (d *DB) Event(now time.Time, kind, text string) error {
	_, err := d.db.Exec(`INSERT INTO events (time, kind, text) VALUES (?, ?, ?)`, now.Unix(), kind, text)
	return err
}

// Sanction is a recorded ban, kick or mute.
type Sanction struct {
	SteamID string `json:"steamid"`
	Name    string `json:"name"`
	Kind    string `json:"kind"`
	Reason  string `json:"reason"`
	Notes   string `json:"notes"`
	Minutes int    `json:"minutes"`
	Time    int64  `json:"time"`
}

// AddSanction records one.
func (d *DB) AddSanction(s Sanction) error {
	_, err := d.db.Exec(`INSERT INTO sanctions (steamid, name, kind, reason, notes, minutes, time) VALUES (?, ?, ?, ?, ?, ?, ?)`,
		s.SteamID, s.Name, s.Kind, s.Reason, s.Notes, s.Minutes, s.Time)
	return err
}

// Sanctions lists them, newest first.
func (d *DB) Sanctions() ([]Sanction, error) {
	rows, err := d.db.Query(`SELECT steamid, name, kind, reason, notes, minutes, time FROM sanctions ORDER BY time DESC, id DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []Sanction{}
	for rows.Next() {
		var s Sanction
		if err := rows.Scan(&s.SteamID, &s.Name, &s.Kind, &s.Reason, &s.Notes, &s.Minutes, &s.Time); err != nil {
			return nil, err
		}
		out = append(out, s)
	}
	return out, rows.Err()
}

// PlayerRow is a player's history.
type PlayerRow struct {
	SteamID     string `json:"steamid"`
	Name        string `json:"name"`
	FirstSeen   int64  `json:"first_seen"`
	LastSeen    int64  `json:"last_seen"`
	Connections int    `json:"connections"`
	Seconds     int64  `json:"seconds"`
}

// Players lists known players, last seen first. A connection still open counts up to now.
func (d *DB) Players(now time.Time) ([]PlayerRow, error) {
	rows, err := d.db.Query(`SELECT p.steamid, p.name, p.first_seen, p.last_seen, count(c.id),
		coalesce(sum(coalesce(c.end, ?) - c.start), 0)
		FROM players p LEFT JOIN connections c ON c.steamid = p.steamid
		GROUP BY p.steamid ORDER BY p.last_seen DESC`, now.Unix())
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []PlayerRow{}
	for rows.Next() {
		var p PlayerRow
		if err := rows.Scan(&p.SteamID, &p.Name, &p.FirstSeen, &p.LastSeen, &p.Connections, &p.Seconds); err != nil {
			return nil, err
		}
		out = append(out, p)
	}
	return out, rows.Err()
}

// Count is a name with a number.
type Count struct {
	Name  string `json:"name"`
	Count int    `json:"count"`
}

// Summary is what the Stats tab draws.
type Summary struct {
	Maps       []Count `json:"maps"`
	Modes      []Count `json:"modes"`
	PerDay     []Count `json:"per_day"`
	PerHour    []Count `json:"per_hour"`
	AvgPlayers float64 `json:"avg_players"`
}

func (d *DB) counts(query string, args ...any) ([]Count, error) {
	rows, err := d.db.Query(query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []Count{}
	for rows.Next() {
		var c Count
		if err := rows.Scan(&c.Name, &c.Count); err != nil {
			return nil, err
		}
		out = append(out, c)
	}
	return out, rows.Err()
}

// Summarize gives the numbers since a date.
func (d *DB) Summarize(since time.Time) (Summary, error) {
	t := since.Unix()
	var s Summary
	var err error
	if s.Maps, err = d.counts(`SELECT map, count(*) c FROM map_plays WHERE start >= ? GROUP BY map ORDER BY c DESC`, t); err != nil {
		return s, err
	}
	if s.Modes, err = d.counts(`SELECT mode, count(*) c FROM mode_plays WHERE start >= ? GROUP BY mode ORDER BY c DESC`, t); err != nil {
		return s, err
	}
	if s.PerDay, err = d.counts(`SELECT date(start, 'unixepoch', 'localtime') day, count(DISTINCT steamid) FROM connections
		WHERE start >= ? GROUP BY day ORDER BY day`, t); err != nil {
		return s, err
	}
	if s.PerHour, err = d.counts(`SELECT strftime('%H', hour, 'unixepoch', 'localtime') h, sum(humans) FROM samples
		WHERE hour >= ? GROUP BY h ORDER BY h`, t); err != nil {
		return s, err
	}
	err = d.db.QueryRow(`SELECT coalesce(1.0 * sum(humans) / sum(count), 0) FROM samples WHERE hour >= ?`, t).Scan(&s.AvgPlayers)
	return s, err
}

// Prune removes history older than a date, but keeps sanctions: they are a decision, not a measure.
func (d *DB) Prune(before time.Time) error {
	t := before.Unix()
	for _, q := range []string{
		`DELETE FROM connections WHERE start < ? AND end IS NOT NULL`,
		`DELETE FROM map_plays WHERE start < ?`,
		`DELETE FROM mode_plays WHERE start < ?`,
		`DELETE FROM samples WHERE hour < ?`,
		`DELETE FROM events WHERE time < ?`,
	} {
		if _, err := d.db.Exec(q, t); err != nil {
			return err
		}
	}
	_, err := d.db.Exec(`DELETE FROM players WHERE last_seen < ? AND steamid NOT IN (SELECT steamid FROM sanctions)`, t)
	return err
}

// EraseAll deletes every player record and all history, including sanctions.
func (d *DB) EraseAll() error {
	for _, t := range []string{"connections", "players", "map_plays", "mode_plays", "samples", "events", "sanctions"} {
		if _, err := d.db.Exec("DELETE FROM " + t); err != nil {
			return err
		}
	}
	_, err := d.db.Exec("VACUUM")
	return err
}
