package main

import (
	"bufio"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/xsteban1410/CSGO-Server/go/internal/rcon"
)

const (
	basePort   = 28100
	portStep   = 10
	bootLimit  = 120 * time.Second
	rconLimit  = 10 * time.Second
	ramPerInst = 1600 // MB per server
)

var posRe = regexp.MustCompile(`pos=.*`)

// instance is one test server.
type instance struct {
	port     int
	password string
	hosts    []string
	cmd      *exec.Cmd
	conn     *rcon.Client
}

func startInstance(bin string, port int) (*instance, error) {
	var info instanceInfo
	if err := cliJSON(bin, &info, "test-instance", "--port", strconv.Itoa(port)); err != nil {
		return nil, err
	}
	// Never fight the user's own server for a port.
	if l, err := net.ListenPacket("udp", fmt.Sprintf(":%d", port)); err != nil {
		return nil, fmt.Errorf("port %d is taken", port)
	} else {
		l.Close()
	}
	c := exec.Command(info.Argv[0], append(info.Argv[1:], "-nohltv", "-tickrate", "64", "-norestart")...)
	c.Dir = info.Dir
	c.Env = append(os.Environ(), "LD_LIBRARY_PATH=.:bin:"+os.Getenv("LD_LIBRARY_PATH"))
	c.Stdout, c.Stderr = io.Discard, io.Discard
	c.Stdin = nil
	c.SysProcAttr = &syscall.SysProcAttr{Setpgid: true, Pdeathsig: syscall.SIGTERM}
	if err := c.Start(); err != nil {
		return nil, err
	}
	in := &instance{port: port, password: info.RconPassword, hosts: info.RconHosts, cmd: c}
	deadline := time.Now().Add(bootLimit)
	for {
		if err := in.connect(); err == nil {
			_, err = in.exec("sv_hibernate_when_empty 0")
			if err == nil {
				return in, nil
			}
		} else if errors.Is(err, rcon.ErrAuth) {
			in.stop()
			return nil, err
		}
		if time.Now().After(deadline) {
			in.stop()
			return nil, fmt.Errorf("server on port %d did not answer in %s", port, bootLimit)
		}
		time.Sleep(200 * time.Millisecond)
	}
}

func (in *instance) stop() {
	if in.conn != nil {
		in.conn.Close()
	}
	if in.cmd.Process != nil {
		pg := -in.cmd.Process.Pid
		_ = syscall.Kill(pg, syscall.SIGTERM)
		done := make(chan struct{})
		go func() { _ = in.cmd.Wait(); close(done) }()
		select {
		case <-done:
		case <-time.After(5 * time.Second):
			_ = syscall.Kill(pg, syscall.SIGKILL)
			<-done
		}
	}
}

func (in *instance) connect() error {
	var last error
	for _, h := range in.hosts {
		c, err := rcon.Dial(net.JoinHostPort(h, strconv.Itoa(in.port)), in.password, time.Second)
		if err == nil {
			in.conn = c
			return nil
		}
		if errors.Is(err, rcon.ErrAuth) {
			return err
		}
		last = err
	}
	return last
}

// exec reconnects as needed: the engine drops rcon connections when a map loads.
func (in *instance) exec(cmd string) (string, error) {
	var last error
	for range 3 {
		if in.conn == nil {
			if err := in.connect(); err != nil {
				last = err
				time.Sleep(300 * time.Millisecond)
				continue
			}
		}
		o, err := in.conn.Exec(cmd, rconLimit)
		if err == nil {
			return o, nil
		}
		last = err
		in.conn.Close()
		in.conn = nil
	}
	return "", last
}

func (in *instance) val(name string) string {
	o, err := in.exec(name)
	if err != nil {
		return ""
	}
	return rcon.CvarValue(o)
}

func (in *instance) positions() []string {
	o, err := in.exec("modo_estado")
	if err != nil {
		return nil
	}
	p := posRe.FindAllString(o, -1)
	sort.Strings(p)
	return p
}

// caseResult is one line of the report.
type caseResult struct {
	name, state, gt, respawn, mapName string
	bots, moved, alive                int
	took                              time.Duration
}

func (r caseResult) String() string {
	return fmt.Sprintf("%-24s %-12s gt=%-4s bots=%-2d moved=%-2d respawn=%-2s alive=%-2d map=%-16s %s", r.name, r.state, r.gt, r.bots, r.moved, r.respawn, r.alive, r.mapName, r.took.Round(time.Second))
}

// expectStill: bots that never move by design. Guardian waits for a human, aim stands still, retake holds the bomb.
func expectStill(mode, gt string) bool {
	return gt == "4/0" || mode == "aim" || mode == "retake" || mode == "duelo"
}

func waitFor(limit time.Duration, f func() bool) bool {
	deadline := time.Now().Add(limit)
	for {
		if f() {
			return true
		}
		if time.Now().After(deadline) {
			return false
		}
		time.Sleep(300 * time.Millisecond)
	}
}

func checkCase(in *instance, bin, name string) caseResult {
	start := time.Now()
	r := caseResult{name: name, state: "ok", alive: -1}
	defer func() { r.took = time.Since(start) }()
	mode, kind, hasKind := strings.Cut(name, ":")
	args := []string{"mode", mode, "--plan", "--set", "modo_humanos=0", "--set", "mp_warmuptime=5", "--set", "mp_freezetime=0", "--set", "bot_join_after_player=0"}
	if hasKind {
		args = append(args, "-t", kind)
	}
	var plan modePlan
	if err := cliJSON(bin, &plan, args...); err != nil {
		r.state = "PLAN: " + err.Error()
		return r
	}
	before := in.val("host_map")
	for _, c := range plan.Commands {
		// with the map change the answer may not arrive in time, even though the map loads
		if _, err := in.exec(c); err != nil && !strings.HasPrefix(c, "changelevel") {
			r.state = "RCON: " + err.Error()
			return r
		}
	}
	// Same map: changelevel still reloads. Give it time to start, or the bots seen below are the old map's.
	if before == plan.Map+".bsp" {
		time.Sleep(2 * time.Second)
	} else {
		waitFor(30*time.Second, func() bool { return in.val("host_map") == plan.Map+".bsp" })
	}
	if mode != "duelo" {
		waitFor(25*time.Second, func() bool { return len(in.positions()) > 0 })
	}
	_, _ = in.exec("mp_warmup_end")
	// The round restart teleports everyone, which would look like moving.
	freeze, _ := strconv.ParseFloat(in.val("mp_freezetime"), 64)
	time.Sleep(time.Duration(freeze)*time.Second + 4*time.Second)
	r.gt = in.val("game_type") + "/" + in.val("game_mode")
	r.respawn = in.val("mp_respawn_on_death_t")
	r.mapName = strings.TrimSuffix(in.val("host_map"), ".bsp")

	a := in.positions()
	r.bots = len(a)
	still := expectStill(mode, r.gt)
	if !still {
		waitFor(10*time.Second, func() bool {
			r.moved = len(a) - common(a, in.positions())
			return r.moved > 0
		})
	}
	if r.respawn == "1" {
		_, _ = in.exec("sv_cheats 1; bot_kill; sv_cheats 0")
		time.Sleep(2 * time.Second)
		waitFor(10*time.Second, func() bool { r.alive = len(in.positions()); return r.alive > 0 })
	}
	switch {
	case r.bots == 0 && mode != "duelo":
		r.state = "NO_BOTS"
	case r.bots > 1 && r.moved == 0 && !still:
		r.state = "NOT_MOVING"
	case r.respawn == "1" && r.alive == 0:
		r.state = "NO_RESPAWN"
	}
	return r
}

func common(a, b []string) int {
	seen := map[string]bool{}
	for _, x := range b {
		seen[x] = true
	}
	n := 0
	for _, x := range a {
		if seen[x] {
			n++
		}
	}
	return n
}

// defaultJobs fits the servers in the free RAM and leaves a core for each one.
func defaultJobs() int {
	jobs := 1
	f, err := os.Open("/proc/meminfo")
	if err != nil {
		return jobs
	}
	defer f.Close()
	s := bufio.NewScanner(f)
	for s.Scan() {
		if v, ok := strings.CutPrefix(s.Text(), "MemAvailable:"); ok {
			kb, _ := strconv.Atoi(strings.Fields(v)[0])
			jobs = kb / 1024 / ramPerInst
		}
	}
	return max(1, min(jobs-1, 6))
}

// fingerprint is the hash of everything a case depends on.
func fingerprint(repo, name string) string {
	mode, kind, _ := strings.Cut(name, ":")
	h := sha256.New()
	files := []string{"cfg/modo_base.cfg", "cfg/modo_clasico.cfg", "cfg/modo_" + mode + ".cfg", "modes/" + mode + ".toml"}
	if kind != "" {
		files = append(files, "cfg/modo_tipo_*.cfg")
	}
	files = append(files, "plugin/*.sp")
	for _, pattern := range files {
		matches, _ := filepath.Glob(filepath.Join(repo, pattern))
		sort.Strings(matches)
		for _, m := range matches {
			b, _ := os.ReadFile(m)
			h.Write(b)
		}
	}
	h.Write([]byte(name))
	return hex.EncodeToString(h.Sum(nil))
}

func loadCache(path string) map[string]string {
	c := map[string]string{}
	if b, err := os.ReadFile(path); err == nil {
		_ = json.Unmarshal(b, &c)
	}
	return c
}

func saveCache(path string, c map[string]string) {
	if b, err := json.MarshalIndent(c, "", " "); err == nil {
		_ = os.MkdirAll(filepath.Dir(path), 0o755)
		_ = os.WriteFile(path, b, 0o644)
	}
}

func runCases(repo, bin string, cases []string, jobs int, noCache bool) int {
	cachePath := filepath.Join(repo, "target", "testkit-cache.json")
	cache := loadCache(cachePath)
	var todo []string
	for _, c := range cases {
		if !noCache && cache[c] == fingerprint(repo, c) {
			continue
		}
		todo = append(todo, c)
	}
	if skipped := len(cases) - len(todo); skipped > 0 {
		fmt.Printf("%d cases unchanged since they last passed (--no-cache runs them)\n", skipped)
	}
	if len(todo) == 0 {
		return 0
	}
	if jobs <= 0 {
		jobs = defaultJobs()
	}
	jobs = min(jobs, len(todo))
	if o, err := exec.Command(bin, "deploy").CombinedOutput(); err != nil {
		fatal(fmt.Errorf("deploy: %w\n%s", err, o))
	}
	fmt.Printf("%d cases on %d servers\n", len(todo), jobs)

	queue := make(chan string, len(todo))
	for _, c := range todo {
		queue <- c
	}
	close(queue)
	var (
		mu     sync.Mutex
		failed int
		wg     sync.WaitGroup
	)
	report := func(line string, ok bool, name string) {
		mu.Lock()
		defer mu.Unlock()
		fmt.Println(line)
		if ok {
			cache[name] = fingerprint(repo, name)
		} else {
			failed++
			delete(cache, name)
		}
	}
	for i := range jobs {
		wg.Add(1)
		go func(port int) {
			defer wg.Done()
			in, err := startInstance(bin, port)
			if err != nil {
				for c := range queue {
					report(fmt.Sprintf("%-24s NO_SERVER %v", c, err), false, c)
				}
				return
			}
			defer in.stop()
			for c := range queue {
				r := checkCase(in, bin, c)
				report(r.String(), r.state == "ok", c)
			}
		}(basePort + i*portStep)
	}
	wg.Wait()
	saveCache(cachePath, cache)
	return failed
}
