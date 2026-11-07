package main

import (
	"context"
	"encoding/json"
	"errors"
	"log"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/xsteban1410/CSGO-Server/go/internal/logs"
	"github.com/xsteban1410/CSGO-Server/go/internal/rcon"
	"github.com/xsteban1410/CSGO-Server/go/internal/stats"
	"github.com/xsteban1410/CSGO-Server/go/internal/status"
)

// pollActive is the interval with the app open.
var pollActive = 5 * time.Second

const (
	pollIdle = 30 * time.Second
	// recently is how long after a client's last request the app counts as open
	recently = time.Minute
	// after failing, rcon rests: a server bans an address after too many wrong passwords
	rconRest = 30 * time.Second

	consoleMaxBytes = 50 << 20
	consoleCopies   = 3
	eventsMaxBytes  = 5 << 20
)

// snapshot is what the Dashboard shows.
type snapshot struct {
	Running  bool            `json:"running"`
	PID      int             `json:"pid,omitempty"`
	Uptime   int64           `json:"uptime,omitempty"`
	CPU      float64         `json:"cpu"`
	RssKB    int64           `json:"rss_kb"`
	RconOK   bool            `json:"rcon_ok"`
	Mode     string          `json:"mode,omitempty"`
	Server   status.Status   `json:"server"`
	Bridge   json.RawMessage `json:"bridge,omitempty"`
	StatsOn  bool            `json:"stats_on"`
	LastPoll int64           `json:"last_poll,omitempty"`
}

type agent struct {
	cfg     config
	cli     string
	db      *stats.DB
	ring    *logs.Ring
	console *logs.Tail
	events  *logs.Tail
	sched   string

	mu       sync.Mutex
	snap     snapshot
	lastCall time.Time
	running  map[string]bool
	history  []taskRun
	lastTick uint64
	lastAt   time.Time

	// only the monitor loop uses these
	pid      int
	pidStart uint64
	pidFinal bool
}

func (a *agent) touch() {
	a.mu.Lock()
	a.lastCall = time.Now()
	a.mu.Unlock()
}

func (a *agent) snapshot() snapshot {
	a.mu.Lock()
	defer a.mu.Unlock()
	s := a.snap
	s.StatsOn = a.db != nil
	return s
}

// clockTick is the kernel's USER_HZ, 100 on every Linux architecture we run on.
const clockTick = 100

// findServer looks for the server of this install and port. It is the real program, srcds_linux, because srcds_run is
// a shell that measures nothing; srcds_run only counts while it starts, and is not kept. Works with or without systemd.
func (a *agent) findServer() (pid int, final bool) {
	install, err := filepath.EvalSymlinks(filepath.Dir(a.cfg.CsgoDir))
	if err != nil {
		return 0, false
	}
	entries, err := os.ReadDir("/proc")
	if err != nil {
		return 0, false
	}
	port := "-port\x00" + strconv.Itoa(a.cfg.Port) + "\x00"
	for _, e := range entries {
		n, err := strconv.Atoi(e.Name())
		if err != nil {
			continue
		}
		comm, err := os.ReadFile("/proc/" + e.Name() + "/comm")
		if err != nil {
			continue
		}
		name := strings.TrimSpace(string(comm))
		if name != "srcds_linux" && name != "srcds_run" {
			continue
		}
		cwd, err := os.Readlink("/proc/" + e.Name() + "/cwd")
		if err != nil || cwd != install {
			continue
		}
		cmd, err := os.ReadFile("/proc/" + e.Name() + "/cmdline")
		if err != nil || !strings.Contains(string(cmd), port) {
			continue
		}
		if name == "srcds_linux" {
			return n, true
		}
		pid = n
	}
	return pid, false
}

// proc gives the server's numbers. The process found is kept with its start time, so /proc is only walked while there
// is none, and a reused PID does not count as the server.
func (a *agent) proc() (pid int, startTicks, cpuTicks uint64, rssKB int64, ok bool) {
	for try := 0; try < 2; try++ {
		if a.pid == 0 {
			var final bool
			if a.pid, final = a.findServer(); a.pid == 0 {
				return
			}
			a.pidFinal, a.pidStart = final, 0
		}
		start, cpu, rss, alive := procStat(a.pid)
		if alive && (a.pidStart == 0 || a.pidStart == start) {
			a.pidStart = start
			pid = a.pid
			if !a.pidFinal {
				a.pid = 0 // srcds_run: look again next time
			}
			return pid, start, cpu, rss, true
		}
		a.pid = 0
	}
	return
}

func procStat(pid int) (start, cpu uint64, rssKB int64, ok bool) {
	stat, err := os.ReadFile("/proc/" + strconv.Itoa(pid) + "/stat")
	if err != nil {
		return
	}
	// the command name can hold spaces, so the fields count from the last parenthesis
	s := string(stat)
	i := strings.LastIndexByte(s, ')')
	if i < 0 {
		return
	}
	p := strings.Fields(s[i+1:])
	if len(p) < 22 || p[0] == "Z" {
		return
	}
	start, _ = strconv.ParseUint(p[19], 10, 64)
	u, _ := strconv.ParseUint(p[11], 10, 64)
	k, _ := strconv.ParseUint(p[12], 10, 64)
	pages, _ := strconv.ParseInt(p[21], 10, 64)
	return start, u + k, pages * int64(os.Getpagesize()) / 1024, true
}

func bootTime() int64 {
	b, err := os.ReadFile("/proc/stat")
	if err != nil {
		return 0
	}
	for _, l := range strings.Split(string(b), "\n") {
		if v, ok := strings.CutPrefix(l, "btime "); ok {
			n, _ := strconv.ParseInt(v, 10, 64)
			return n
		}
	}
	return 0
}

// monitor is the only loop that polls: one rcon connection, one request per interval.
func (a *agent) monitor(ctx context.Context) {
	var client *rcon.Client
	defer func() {
		if client != nil {
			client.Close()
		}
	}()
	var lastPoll, rconAfter, nextRotate, nextPrune time.Time
	wasRunning, lastMode := false, ""
	boot := bootTime()
	// the console only from now: older lines belong to another run. The events are all read: they are history.
	if info, err := os.Stat(a.console.Path); err == nil {
		a.console.Offset = info.Size()
	}
	a.events.Offset = a.loadOffset()

	t := time.NewTicker(min(time.Second, pollActive))
	defer t.Stop()
	for {
		now := time.Now()
		a.readLogs(now)
		if now.After(nextRotate) {
			nextRotate = now.Add(time.Minute)
			a.rotate()
		}
		if a.db != nil && a.cfg.StatsDays > 0 && now.After(nextPrune) {
			nextPrune = now.Add(24 * time.Hour)
			if err := a.db.Prune(now.AddDate(0, 0, -a.cfg.StatsDays)); err != nil {
				log.Printf("prune: %v", err)
			}
		}
		// worked out every second, so opening the app shortens the wait at once
		a.mu.Lock()
		interval := pollIdle
		if now.Sub(a.lastCall) < recently {
			interval = pollActive
		}
		a.mu.Unlock()
		if !now.Before(lastPoll.Add(interval)) {
			lastPoll = now
			pid, start, cpu, rss, alive := a.proc()
			s := snapshot{Running: alive, LastPoll: now.Unix()}
			if alive {
				s.PID, s.RssKB = pid, rss
				s.Uptime = now.Unix() - (boot + int64(start/clockTick))
				s.CPU = a.cpuPercent(cpu, now)
				if client == nil && now.After(rconAfter) {
					if client = a.dial(); client == nil {
						rconAfter = now.Add(rconRest)
					}
				}
				if client != nil {
					out, mode, err := pollServer(client.Exec)
					if err != nil {
						client.Close()
						client = nil
						rconAfter = now.Add(rconRest)
					} else {
						s.RconOK = true
						s.Server = status.Parse(out)
						s.Mode = rcon.CvarValue(mode)
					}
				}
			} else if client != nil {
				client.Close()
				client = nil
			}
			if b, err := os.ReadFile(filepath.Join(a.cfg.RuntimeDir, "bridge.json")); err == nil && json.Valid(b) {
				s.Bridge = b
			}
			a.mu.Lock()
			a.snap = s
			a.mu.Unlock()
			a.record(now, s, wasRunning, &lastMode)
			wasRunning = alive
		}
		select {
		case <-ctx.Done():
			return
		case <-t.C:
		}
	}
}

// dial tries the addresses in order. A wrong password is not tried on the others: the server counts every attempt.
func (a *agent) dial() *rcon.Client {
	for _, addr := range a.cfg.RconAddrs {
		c, err := rcon.Dial(addr, a.cfg.RconPassword, time.Second)
		if err == nil {
			return c
		}
		if errors.Is(err, rcon.ErrAuth) {
			log.Print("rcon: wrong password")
			return nil
		}
	}
	return nil
}

func (a *agent) cpuPercent(ticks uint64, now time.Time) float64 {
	a.mu.Lock()
	defer a.mu.Unlock()
	var pct float64
	if !a.lastAt.IsZero() && ticks >= a.lastTick {
		if dt := now.Sub(a.lastAt).Seconds(); dt > 0 {
			pct = float64(ticks-a.lastTick) / clockTick / dt * 100
		}
	}
	a.lastTick, a.lastAt = ticks, now
	return pct
}

func (a *agent) record(now time.Time, s snapshot, was bool, lastMode *string) {
	if a.db == nil {
		return
	}
	var err error
	if s.Running && !was {
		err = a.db.Event(now, "server", "started")
	}
	if !s.Running && was {
		err = a.db.Event(now, "server", "stopped")
		// whoever was playing left with the server
		if e := a.db.CloseOpen(now); err == nil {
			err = e
		}
		*lastMode = ""
	}
	if err == nil && s.RconOK {
		err = a.db.Sample(now, s.Server.Humans)
		if s.Mode != "" && s.Mode != *lastMode {
			*lastMode = s.Mode
			if e := a.db.ModePlayed(now, s.Mode); err == nil {
				err = e
			}
		}
	}
	if err != nil {
		log.Printf("stats: %v", err)
	}
}

var secretLine = regexp.MustCompile(`(?i)\b(rcon_password|sv_password|sv_setsteamaccount)\b.*`)

// redact hides secrets from what clients can read: the known values and whatever follows a secret convar.
func redact(line string, secrets []string) string {
	for _, s := range secrets {
		if s != "" {
			line = strings.ReplaceAll(line, s, "***")
		}
	}
	return secretLine.ReplaceAllString(line, "$1 ***")
}

func (a *agent) readLogs(now time.Time) {
	lines, err := a.console.Read()
	if err != nil {
		log.Printf("console.log: %v", err)
	}
	for i, l := range lines {
		lines[i] = redact(l, a.cfg.Secrets)
	}
	a.ring.Add(now, lines)

	lines, err = a.events.Read()
	if err != nil {
		log.Printf("events: %v", err)
	}
	if len(lines) == 0 {
		return
	}
	for _, l := range lines {
		a.handleEvent(l)
	}
	a.saveOffset(a.events.Offset)
}

// event is a line of csls_events.jsonl.
type event struct {
	Time  int64  `json:"t"`
	Kind  string `json:"e"`
	ID    string `json:"id"`
	Name  string `json:"name"`
	Extra string `json:"extra"`
}

func (a *agent) handleEvent(line string) {
	if a.db == nil {
		return
	}
	var e event
	if json.Unmarshal([]byte(line), &e) != nil || e.Time <= 0 {
		return
	}
	when := time.Unix(e.Time, 0)
	var err error
	switch e.Kind {
	case "connect":
		if err = a.db.Connect(when, e.ID, e.Name); err == nil {
			err = a.db.Event(when, "player", e.Name+" connected")
		}
	case "disconnect":
		if err = a.db.Disconnect(when, e.ID); err == nil {
			err = a.db.Event(when, "player", e.Name+" disconnected")
		}
	case "map":
		err = a.db.MapPlayed(when, e.Name)
	case "ban":
		minutes, reason, _ := strings.Cut(e.Extra, "|")
		m, _ := strconv.Atoi(minutes)
		err = a.db.AddSanction(stats.Sanction{SteamID: e.ID, Name: e.Name, Kind: "ban", Reason: reason, Minutes: m, Time: e.Time})
	}
	if err != nil {
		log.Printf("event: %v", err)
	}
}

func (a *agent) offsetPath() string { return filepath.Join(a.cfg.DataDir, "agent-events.offset") }

func (a *agent) loadOffset() int64 {
	b, err := os.ReadFile(a.offsetPath())
	if err != nil {
		return 0
	}
	n, _ := strconv.ParseInt(strings.TrimSpace(string(b)), 10, 64)
	return n
}

func (a *agent) saveOffset(n int64) {
	if err := os.WriteFile(a.offsetPath(), []byte(strconv.FormatInt(n, 10)), 0o600); err != nil {
		log.Printf("offset: %v", err)
	}
}

func (a *agent) rotate() {
	if _, err := logs.Rotate(a.console.Path, consoleMaxBytes, consoleCopies); err != nil {
		log.Printf("rotate: %v", err)
	}
	// the events were read into the database, so a full file only has to be emptied
	if info, err := os.Stat(a.events.Path); err == nil && info.Size() > eventsMaxBytes && a.events.Offset >= info.Size() {
		if err := os.Truncate(a.events.Path, 0); err != nil {
			log.Printf("rotate: %v", err)
			return
		}
		a.events.Offset = 0
		a.saveOffset(0)
	}
}

// pollServer reads the status and the mode. srcds writes what rcon runs to console.log, so the poll goes between the
// markers that the app's console hides (POLL_BEGIN and POLL_END in crates/app), or it would flood the console.
func pollServer(exec func(string, time.Duration) (string, error)) (out, mode string, err error) {
	if _, err = exec("echo csls_poll_begin", 3*time.Second); err != nil {
		return
	}
	if out, err = exec("status", 3*time.Second); err != nil {
		return
	}
	if mode, err = exec("modo_nombre", 3*time.Second); err != nil {
		return
	}
	_, err = exec("echo csls_poll_end", 3*time.Second)
	return
}
