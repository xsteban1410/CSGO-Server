// The agent is the long-running helper of the app: it watches the server with one poll, follows the logs, keeps the
// stats and runs scheduled tasks. It acts only by calling the CLI with fixed arguments, without a shell.
package main

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"log"
	"net"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"syscall"
	"time"

	"github.com/xsteban1410/CSGO-Server/go/internal/logs"
	"github.com/xsteban1410/CSGO-Server/go/internal/sched"
	"github.com/xsteban1410/CSGO-Server/go/internal/stats"
)

// config is what `csgo-legacy-server internal agent-config` gives through a pipe, never in arguments or environment.
type config struct {
	RconAddrs    []string `json:"rcon_addrs"`
	RconPassword string   `json:"rcon_password"`
	CsgoDir      string   `json:"csgo_dir"`
	DataDir      string   `json:"data_dir"`
	ConfigDir    string   `json:"config_dir"`
	RuntimeDir   string   `json:"runtime_dir"`
	Port         int      `json:"port"`
	StatsEnabled bool     `json:"stats_enabled"`
	StatsDays    int      `json:"stats_days"`
	Secrets      []string `json:"secrets"`
}

func loadConfig(cli string) (config, error) {
	var c config
	cmd := exec.Command(cli, "internal", "agent-config")
	cmd.Stderr = os.Stderr
	out, err := cmd.Output()
	if err != nil {
		return c, fmt.Errorf("%s internal agent-config: %w", cli, err)
	}
	return c, json.Unmarshal(out, &c)
}

// sibling is the CLI next to this program. Without one, it is looked for in the PATH.
func sibling() string {
	if exe, err := os.Executable(); err == nil {
		p := filepath.Join(filepath.Dir(exe), "csgo-legacy-server")
		if _, err := os.Stat(p); err == nil {
			return p
		}
	}
	return "csgo-legacy-server"
}

func main() {
	cli := flag.String("cli", sibling(), "the command line program")
	flag.DurationVar(&pollActive, "poll", pollActive, "seconds between polls with the app open")
	flag.Parse()
	log.SetFlags(log.LstdFlags)
	cfg, err := loadConfig(*cli)
	if err != nil {
		log.Fatal(err)
	}
	if err := os.MkdirAll(cfg.RuntimeDir, 0o700); err != nil {
		log.Fatal(err)
	}
	if err := os.Chmod(cfg.RuntimeDir, 0o700); err != nil {
		log.Fatal(err)
	}
	sock := filepath.Join(cfg.RuntimeDir, "agent.sock")
	if c, err := net.DialTimeout("unix", sock, time.Second); err == nil {
		c.Close()
		log.Fatal("another agent is running")
	}
	_ = os.Remove(sock)
	ln, err := net.Listen("unix", sock)
	if err != nil {
		log.Fatal(err)
	}
	defer os.Remove(sock)
	if err := os.Chmod(sock, 0o600); err != nil {
		log.Fatal(err)
	}

	var db *stats.DB
	if cfg.StatsEnabled {
		if db, err = stats.Open(filepath.Join(cfg.DataDir, "stats.db")); err != nil {
			// without stats the rest still works, and an unreadable database must not be overwritten
			log.Printf("stats are off: %v", err)
			db = nil
		} else {
			defer db.Close()
		}
	}
	a := newAgent(cfg, *cli, db)
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer stop()
	go a.monitor(ctx)
	go a.scheduler(ctx)
	go func() {
		<-ctx.Done()
		ln.Close()
	}()
	for {
		c, err := ln.Accept()
		if err != nil {
			if !errors.Is(err, net.ErrClosed) {
				log.Print(err)
			}
			return
		}
		go a.serve(ctx, c)
	}
}

// request is one line from a client.
type request struct {
	Method string          `json:"method"`
	Since  int64           `json:"since,omitempty"`
	Limit  int             `json:"limit,omitempty"`
	Days   int             `json:"days,omitempty"`
	ID     string          `json:"id,omitempty"`
	Tasks  []sched.Task    `json:"tasks,omitempty"`
	Data   json.RawMessage `json:"data,omitempty"`
}

type response struct {
	OK     bool   `json:"ok"`
	Error  string `json:"error,omitempty"`
	Result any    `json:"result,omitempty"`
}

// maxRequest bounds a line; the socket is private, but a bug in a client should not use up the memory.
const maxRequest = 1 << 20

func (a *agent) serve(ctx context.Context, c net.Conn) {
	defer c.Close()
	in := bufio.NewScanner(c)
	in.Buffer(make([]byte, 4096), maxRequest)
	out := json.NewEncoder(c)
	for in.Scan() {
		var req request
		if err := json.Unmarshal(in.Bytes(), &req); err != nil {
			_ = out.Encode(response{Error: "invalid request"})
			continue
		}
		a.touch()
		if req.Method == "follow" {
			a.follow(ctx, c, out, req.Since)
			return
		}
		result, err := a.call(req)
		if err != nil {
			_ = out.Encode(response{Error: err.Error()})
			continue
		}
		if err := out.Encode(response{OK: true, Result: result}); err != nil {
			return
		}
	}
}

// follow sends new console lines as they come, until the client leaves.
func (a *agent) follow(ctx context.Context, c net.Conn, out *json.Encoder, since int64) {
	t := time.NewTicker(500 * time.Millisecond)
	defer t.Stop()
	gone := make(chan struct{})
	go func() {
		// the client sends nothing more: a read that ends means it closed
		_, _ = c.Read(make([]byte, 1))
		close(gone)
	}()
	for {
		a.touch()
		lines := a.ring.Since(since, 0)
		if len(lines) > 0 {
			since = lines[len(lines)-1].Seq
			if err := out.Encode(response{OK: true, Result: lines}); err != nil {
				return
			}
		}
		select {
		case <-ctx.Done():
			return
		case <-gone:
			return
		case <-t.C:
		}
	}
}

var errNoStats = errors.New("stats are off")

func (a *agent) call(r request) (any, error) {
	now := time.Now()
	switch r.Method {
	case "status":
		return a.snapshot(), nil
	case "lines":
		return a.ring.Since(r.Since, r.Limit), nil
	case "tasks":
		return a.taskList()
	case "set_tasks":
		return nil, a.setTasks(r.Tasks)
	case "run_task":
		return nil, a.runNow(r.ID)
	case "task_history":
		return a.taskHistory(), nil
	}
	if a.db == nil {
		switch r.Method {
		case "players", "sanctions", "add_sanction", "summary", "erase_stats":
			return nil, errNoStats
		}
		return nil, fmt.Errorf("unknown method %q", r.Method)
	}
	switch r.Method {
	case "players":
		return a.db.Players(now)
	case "sanctions":
		return a.db.Sanctions()
	case "add_sanction":
		var s stats.Sanction
		if err := json.Unmarshal(r.Data, &s); err != nil {
			return nil, err
		}
		if s.SteamID == "" || s.Kind == "" {
			return nil, errors.New("a sanction needs a SteamID and a kind")
		}
		if s.Time == 0 {
			s.Time = now.Unix()
		}
		return nil, a.db.AddSanction(s)
	case "summary":
		days := r.Days
		if days <= 0 {
			days = 30
		}
		return a.db.Summarize(now.AddDate(0, 0, -days))
	case "erase_stats":
		return nil, a.db.EraseAll()
	}
	return nil, fmt.Errorf("unknown method %q", r.Method)
}

// consoleKeep is how many lines of the console stay in memory for the app.
const consoleKeep = 20000

func newAgent(cfg config, cli string, db *stats.DB) *agent {
	return &agent{
		cfg:     cfg,
		cli:     cli,
		db:      db,
		ring:    logs.NewRing(consoleKeep),
		console: &logs.Tail{Path: filepath.Join(cfg.CsgoDir, "console.log")},
		events:  &logs.Tail{Path: filepath.Join(cfg.CsgoDir, "addons", "sourcemod", "data", fmt.Sprintf("csls_events_%d.jsonl", cfg.Port))},
		sched:   filepath.Join(cfg.ConfigDir, "schedule.json"),
		running: map[string]bool{},
	}
}
