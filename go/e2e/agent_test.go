package e2e

import (
	"bufio"
	"encoding/json"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

type agentEnv struct {
	*env
	conn *net.UnixConn
	in   *bufio.Reader
	log  string
}

func buildAgent(t *testing.T) string {
	t.Helper()
	bin := filepath.Join(t.TempDir(), "agent")
	if o, err := exec.Command("go", "build", "-o", bin, "github.com/xsteban1410/CSGO-Server/go/cmd/agent").CombinedOutput(); err != nil {
		t.Fatalf("%v\n%s", err, o)
	}
	return bin
}

func startAgent(t *testing.T, e *env) *agentEnv {
	t.Helper()
	logPath := filepath.Join(e.root, "agent.log")
	logFile, err := os.Create(logPath)
	if err != nil {
		t.Fatal(err)
	}
	cli := filepath.Join(e.root, "cli.sh")
	// the CLI with the environment of this test
	write(t, cli, fmt.Sprintf("#!/bin/sh\nexport XDG_CONFIG_HOME=%q XDG_DATA_HOME=%q XDG_RUNTIME_DIR=%q HOME=%q CSLS_NO_SYSTEMD=1 CSLS_RESOURCES=%q CSLS_HUB_DIR=%q\nexec %q \"$@\"\n",
		filepath.Join(e.root, "config"), filepath.Join(e.root, "data"), filepath.Join(e.root, "run"), e.root, filepath.Join(repo, "game"), filepath.Join(repo, "crates", "core", "tests", "fixtures", "hub"), cliBin), 0o755)
	if err := os.MkdirAll(filepath.Join(e.root, "data", "csgo-legacy-server"), 0o700); err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command(buildAgent(t), "--cli", cli, "--poll", "100ms")
	cmd.Stdout, cmd.Stderr = logFile, logFile
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		_ = cmd.Process.Signal(os.Interrupt)
		_ = cmd.Wait()
		logFile.Close()
	})
	sock := filepath.Join(e.root, "run", "csgo-legacy-server", "agent.sock")
	a := &agentEnv{env: e, log: logPath}
	eventually(t, "the socket", func() bool {
		c, err := net.Dial("unix", sock)
		if err != nil {
			return false
		}
		a.conn = c.(*net.UnixConn)
		a.in = bufio.NewReader(c)
		return true
	})
	t.Cleanup(func() { a.conn.Close() })
	if info, err := os.Stat(sock); err != nil || info.Mode().Perm() != 0o600 {
		t.Fatalf("socket: %v %v", info, err)
	}
	if info, err := os.Stat(filepath.Dir(sock)); err != nil || info.Mode().Perm() != 0o700 {
		t.Fatalf("runtime dir: %v %v", info, err)
	}
	return a
}

func eventually(t *testing.T, what string, ok func() bool) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for !ok() {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %s", what)
		}
		time.Sleep(25 * time.Millisecond)
	}
}

// call sends a request and decodes its result into out, returning the agent's error.
func (a *agentEnv) call(out any, req map[string]any) string {
	a.t.Helper()
	b, _ := json.Marshal(req)
	if _, err := a.conn.Write(append(b, '\n')); err != nil {
		a.t.Fatal(err)
	}
	line, err := a.in.ReadBytes('\n')
	if err != nil {
		a.t.Fatal(err)
	}
	var r struct {
		OK     bool            `json:"ok"`
		Error  string          `json:"error"`
		Result json.RawMessage `json:"result"`
	}
	if err := json.Unmarshal(line, &r); err != nil {
		a.t.Fatal(err)
	}
	if r.OK && out != nil {
		if err := json.Unmarshal(r.Result, out); err != nil {
			a.t.Fatal(err)
		}
	}
	return r.Error
}

type agentStatus struct {
	Running bool   `json:"running"`
	RconOK  bool   `json:"rcon_ok"`
	Mode    string `json:"mode"`
	PID     int    `json:"pid"`
	RssKB   int64  `json:"rss_kb"`
}

func (a *agentEnv) status() agentStatus {
	var s agentStatus
	if msg := a.call(&s, map[string]any{"method": "status"}); msg != "" {
		a.t.Fatal(msg)
	}
	return s
}

func TestAgentWatchesTheServer(t *testing.T) {
	e := newEnv(t)
	e.start()
	a := startAgent(t, e)
	eventually(t, "the server to be seen", func() bool { s := a.status(); return s.Running && s.RconOK })
	if s := a.status(); s.PID == 0 || s.RssKB == 0 {
		t.Fatalf("%+v", s)
	}
	e.must("stop")
	eventually(t, "the stop to be seen", func() bool { return !a.status().Running })
	if s := a.status(); s.RconOK || s.PID != 0 {
		t.Fatalf("%+v", s)
	}
	e.start()
	eventually(t, "the new server to be seen", func() bool { s := a.status(); return s.Running && s.RconOK })
}

func TestAgentGivesUpOnAWrongPasswordWithoutHammering(t *testing.T) {
	e := newEnv(t)
	e.start()
	e.writeConfig("not-the-password-1234", "")
	a := startAgent(t, e)
	eventually(t, "the server to be seen", func() bool { return a.status().Running })
	if a.status().RconOK {
		t.Fatal("wrong password accepted")
	}
	eventually(t, "the log", func() bool {
		b, _ := os.ReadFile(a.log)
		return strings.Contains(string(b), "wrong password")
	})
	b, _ := os.ReadFile(a.log)
	if strings.Count(string(b), "wrong password") != 1 {
		t.Fatalf("rcon must rest after failing:\n%s", b)
	}
}

func TestAgentKeepsHistoryFromThePluginFile(t *testing.T) {
	e := newEnv(t)
	a := startAgent(t, e)
	events := filepath.Join(e.install, "csgo", "addons", "sourcemod", "data", fmt.Sprintf("csls_events_%d.jsonl", e.port))
	lines := []string{
		`{"t":1800000000,"e":"map","id":"","name":"de_dust2","extra":""}`,
		`{"t":1800000000,"e":"connect","id":"STEAM_1:0:1","name":"Luis \"x\"","extra":""}`,
		`not json at all`,
		`{"t":1800000060,"e":"disconnect","id":"STEAM_1:0:1","name":"Luis \"x\"","extra":""}`,
		`{"t":1800000060,"e":"ban","id":"STEAM_1:0:1","name":"Luis \"x\"","extra":"30|too good|really"}`,
	}
	write(t, events, strings.Join(lines, "\n")+"\n", 0o644)
	var players []struct {
		SteamID string `json:"steamid"`
		Name    string `json:"name"`
		Seconds int64  `json:"seconds"`
	}
	eventually(t, "the player", func() bool {
		a.call(&players, map[string]any{"method": "players"})
		return len(players) == 1 && players[0].Seconds == 60
	})
	if players[0].Name != `Luis "x"` {
		t.Fatalf("%+v", players)
	}
	var sanctions []struct {
		Reason  string `json:"reason"`
		Minutes int    `json:"minutes"`
	}
	eventually(t, "the sanction", func() bool {
		a.call(&sanctions, map[string]any{"method": "sanctions"})
		return len(sanctions) == 1
	})
	if sanctions[0].Reason != "too good|really" || sanctions[0].Minutes != 30 {
		t.Fatalf("%+v", sanctions)
	}
	// no address column anywhere: the database holds no IPs by construction
	if msg := a.call(nil, map[string]any{"method": "erase_stats"}); msg != "" {
		t.Fatal(msg)
	}
	a.call(&players, map[string]any{"method": "players"})
	if len(players) != 0 {
		t.Fatalf("%+v", players)
	}
}

func TestAgentHidesSecretsInTheConsole(t *testing.T) {
	e := newEnv(t)
	a := startAgent(t, e)
	console := filepath.Join(e.install, "csgo", "console.log")
	write(t, console, "rcon_password "+password+"\nsomething with "+password+" inside\nnormal line\n", 0o644)
	var lines []struct {
		Text     string `json:"text"`
		Category string `json:"category"`
	}
	eventually(t, "the console", func() bool {
		a.call(&lines, map[string]any{"method": "lines"})
		return len(lines) == 3
	})
	for _, l := range lines {
		if strings.Contains(l.Text, password) {
			t.Fatalf("secret in %q", l.Text)
		}
	}
}

func TestAgentRunsOnlyListedTasks(t *testing.T) {
	e := newEnv(t)
	a := startAgent(t, e)
	bad := map[string]any{"id": "x", "action": "shell", "arg": "touch /tmp/owned", "every": "daily", "at": "04:00"}
	if msg := a.call(nil, map[string]any{"method": "set_tasks", "tasks": []any{bad}}); !strings.Contains(msg, "unknown action") {
		t.Fatalf("%q", msg)
	}
	backup := map[string]any{"id": "b", "action": "backup", "every": "daily", "at": "04:00", "enabled": true}
	if msg := a.call(nil, map[string]any{"method": "set_tasks", "tasks": []any{backup}}); msg != "" {
		t.Fatal(msg)
	}
	if msg := a.call(nil, map[string]any{"method": "run_task", "id": "b"}); msg != "" {
		t.Fatal(msg)
	}
	var history []struct {
		ID     string `json:"id"`
		OK     bool   `json:"ok"`
		Output string `json:"output"`
	}
	eventually(t, "the task", func() bool {
		a.call(&history, map[string]any{"method": "task_history"})
		return len(history) == 1
	})
	if !history[0].OK {
		t.Fatalf("%+v", history[0])
	}
	if !strings.Contains(e.must("backup", "list"), "auto") {
		t.Fatal("the scheduled backup must count as automatic")
	}
}

func TestAgentConfigRefusesToPrintToATerminalOnly(t *testing.T) {
	e := newEnv(t)
	// here stdout is a pipe, as for the agent
	o := e.must("internal", "agent-config")
	var c struct {
		RconPassword string   `json:"rcon_password"`
		RconAddrs    []string `json:"rcon_addrs"`
	}
	if err := json.Unmarshal([]byte(o), &c); err != nil {
		t.Fatalf("%v\n%s", err, o)
	}
	if c.RconPassword != password || len(c.RconAddrs) == 0 {
		t.Fatalf("%+v", c)
	}
}
