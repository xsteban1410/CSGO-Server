// Package e2e runs the real CLI against fakesrcds, in a private XDG tree with no systemd, so nothing here
// can touch the user's server or config. It needs `cargo build` first; CSLS_BIN picks another binary.
package e2e

import (
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync/atomic"
	"syscall"
	"testing"
	"time"
)

const password = "e2e-rcon-secret"

var (
	cliBin  string
	fakeBin string
	repo    string
)

func TestMain(m *testing.M) {
	wd, _ := os.Getwd()
	repo = filepath.Clean(filepath.Join(wd, "..", ".."))
	cliBin = os.Getenv("CSLS_BIN")
	if cliBin == "" {
		cliBin = filepath.Join(repo, "target", "debug", "csgo-legacy-server")
	}
	if _, err := os.Stat(cliBin); err != nil {
		fmt.Fprintln(os.Stderr, "e2e: run `cargo build` first:", err)
		os.Exit(1)
	}
	dir, err := os.MkdirTemp("", "csls-e2e-bin")
	if err != nil {
		panic(err)
	}
	fakeBin = filepath.Join(dir, "fakesrcds")
	if o, err := exec.Command("go", "build", "-o", fakeBin, "github.com/xsteban1410/CSGO-Server/go/cmd/fakesrcds").CombinedOutput(); err != nil {
		fmt.Fprintf(os.Stderr, "e2e: building fakesrcds: %v\n%s", err, o)
		os.Exit(1)
	}
	code := m.Run()
	os.RemoveAll(dir)
	os.Exit(code)
}

// env is one private install and config.
type env struct {
	t       *testing.T
	root    string
	install string
	port    int
	extra   []string
}

// nextPort hands each test its own range, because a port probed free may be taken by a parallel test a moment later.
var nextPort atomic.Int32

func freePort(t *testing.T) int {
	t.Helper()
	for range 500 {
		p := 20000 + (os.Getpid()%200)*100 + int(nextPort.Add(2))
		l, err := net.Listen("tcp", fmt.Sprintf("127.0.0.1:%d", p))
		if err != nil {
			continue
		}
		l.Close()
		if u, err := net.ListenUDP("udp", &net.UDPAddr{Port: p}); err == nil {
			u.Close()
			return p
		}
	}
	t.Fatal("no free port")
	return 0
}

func write(t *testing.T, path, text string, mode os.FileMode) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(text), mode); err != nil {
		t.Fatal(err)
	}
}

// fakeSpcomp fails on a source holding SYNTAX_ERROR, like a real compiler, and otherwise writes the output.
const fakeSpcomp = `#!/bin/sh
for a in "$@"; do case "$a" in -o*) out="${a#-o}";; *.sp) src="$a";; esac; done
if grep -q SYNTAX_ERROR "$src"; then echo "$src(1) : error 001: expected token"; exit 1; fi
echo compiled > "$out"
`

func newEnv(t *testing.T) *env {
	t.Helper()
	root := t.TempDir()
	e := &env{t: t, root: root, install: filepath.Join(root, "server"), port: freePort(t)}
	for _, m := range []string{"de_dust2", "de_nuke"} {
		write(t, filepath.Join(e.install, "csgo", "maps", m+".bsp"), "", 0o644)
	}
	write(t, filepath.Join(e.install, "csgo", "steam.inf"), "appID=740\r\n", 0o644)
	write(t, filepath.Join(e.install, "csgo", "addons", "sourcemod", "scripting", "spcomp"), fakeSpcomp, 0o755)
	if err := os.MkdirAll(filepath.Join(e.install, "csgo", "cfg"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(fakeBin, filepath.Join(e.install, "srcds_run")); err != nil {
		t.Fatal(err)
	}
	e.writeConfig(password, "")
	t.Cleanup(func() { _, _ = e.run("stop") })
	return e
}

func (e *env) writeConfig(rconPassword, serverPassword string) {
	write(e.t, filepath.Join(e.root, "config", "csgo-legacy-server", "config.toml"), fmt.Sprintf(
		"version = 1\n[server]\ninstall_dir = %q\nport = %d\nrcon_password = %q\nrcon_host = \"127.0.0.1\"\npassword = %q\n",
		e.install, e.port, rconPassword, serverPassword), 0o600)
}

func (e *env) run(args ...string) (string, error) {
	c := exec.Command(cliBin, args...)
	c.Env = append(os.Environ(),
		"XDG_CONFIG_HOME="+filepath.Join(e.root, "config"),
		"XDG_DATA_HOME="+filepath.Join(e.root, "data"),
		"XDG_RUNTIME_DIR="+filepath.Join(e.root, "run"),
		"HOME="+e.root,
		"CSLS_NO_SYSTEMD=1",
		"CSLS_RESOURCES="+repo,
	)
	c.Env = append(c.Env, e.extra...)
	o, err := c.CombinedOutput()
	return string(o), err
}

func (e *env) must(args ...string) string {
	e.t.Helper()
	o, err := e.run(args...)
	if err != nil {
		e.t.Fatalf("%v: %v\n%s", args, err, o)
	}
	return o
}

func (e *env) mustFail(want string, args ...string) string {
	e.t.Helper()
	o, err := e.run(args...)
	if err == nil {
		e.t.Fatalf("%v should fail\n%s", args, o)
	}
	if !strings.Contains(o, want) {
		e.t.Fatalf("%v: wanted %q in:\n%s", args, want, o)
	}
	return o
}

func (e *env) start() {
	e.t.Helper()
	e.must("start", "awp")
	e.waitRcon()
}

func (e *env) waitRcon() {
	e.t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for {
		o, err := e.run("rcon", "echo up")
		if err == nil && strings.Contains(o, "up") {
			return
		}
		if time.Now().After(deadline) {
			e.t.Fatalf("no rcon: %v\n%s", err, o)
		}
		time.Sleep(20 * time.Millisecond)
	}
}

func (e *env) waitStopped() {
	e.t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for {
		if o, _ := e.run("status"); strings.Contains(o, "Stopped") {
			return
		}
		if time.Now().After(deadline) {
			e.t.Fatal("the server still looks alive")
		}
		time.Sleep(20 * time.Millisecond)
	}
}

func TestStartTwice(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	e.start()
	e.mustFail("already running", "start", "awp")
}

func TestStopWhenStopped(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	e.must("stop")
	e.must("stop")
}

func TestPortBusy(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	u, err := net.ListenUDP("udp", &net.UDPAddr{Port: e.port})
	if err != nil {
		t.Fatal(err)
	}
	defer u.Close()
	e.mustFail("is taken", "start", "awp")
}

func TestUnexpectedCrash(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	e.start()
	_, _ = e.run("rcon", "fake_crash")
	e.waitStopped()
	// after a crash a new start just works
	e.start()
}

func TestRconDropped(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	e.start()
	o, err := e.run("rcon", "fake_drop")
	if err == nil {
		t.Fatalf("a dropped connection must be an error:\n%s", o)
	}
	if strings.Contains(o, password) {
		t.Fatalf("password in the error:\n%s", o)
	}
	// the next command connects again
	e.waitRcon()
}

func TestWrongRconPassword(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	e.start()
	e.writeConfig("another-password", "")
	if o := e.mustFail("wrong password", "rcon", "status"); strings.Contains(o, "another-password") {
		t.Fatalf("password in the error:\n%s", o)
	}
}

func TestFailedMapChange(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	e.extra = []string{"FAKESRCDS_FAIL_MAP=de_nuke"}
	e.start()
	e.mustFail("changelevel failed", "mode", "awp", "-m", "de_nuke")
	if o := e.must("rcon", "host_map"); !strings.Contains(o, "de_dust2") {
		t.Fatalf("map changed anyway:\n%s", o)
	}
}

func TestMissingMap(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	e.mustFail("does not exist", "start", "awp", "-m", "de_nothing")
}

func TestSecretsNotInProcessArguments(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	e.writeConfig(password, "server-secret")
	e.start()
	pid, err := os.ReadFile(filepath.Join(e.root, "run", "csgo-legacy-server", "csgo-legacy-server.pid"))
	if err != nil {
		t.Fatal(err)
	}
	cmdline, err := os.ReadFile("/proc/" + strings.Fields(string(pid))[0] + "/cmdline")
	if err != nil {
		t.Fatal(err)
	}
	for _, s := range []string{password, "server-secret"} {
		if strings.Contains(string(cmdline), s) {
			t.Fatalf("%q in the arguments: %s", s, cmdline)
		}
	}
	info, err := os.Stat(filepath.Join(e.install, "csgo", "cfg", "csls_secrets.cfg"))
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode().Perm() != 0o600 {
		t.Fatalf("secrets file is %v", info.Mode().Perm())
	}
}

func pidFile(e *env) string {
	return filepath.Join(e.root, "run", "csgo-legacy-server", "csgo-legacy-server.pid")
}

func TestReusedPidIsNotKilled(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	other := exec.Command("sleep", "30")
	if err := other.Start(); err != nil {
		t.Fatal(err)
	}
	defer other.Process.Kill()
	// an old PID file that now points at somebody else's process
	write(t, pidFile(e), fmt.Sprintf("%d 12345", other.Process.Pid), 0o644)
	if o := e.must("status"); !strings.Contains(o, "Stopped") {
		t.Fatalf("took another process for the server:\n%s", o)
	}
	e.must("stop")
	if err := other.Process.Signal(syscall.Signal(0)); err != nil {
		t.Fatal("stop killed a process that was not ours")
	}
}

func TestOrphanIsDetectedAndStopped(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	e.start()
	if err := os.Remove(pidFile(e)); err != nil {
		t.Fatal(err)
	}
	e.mustFail("without being tracked", "start", "awp")
	e.must("stop")
	if o := e.must("start", "awp"); !strings.Contains(o, "ready") {
		t.Fatalf("no server after clearing the orphan:\n%s", o)
	}
}

func TestCrashIsReported(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	e.start()
	_, _ = e.run("rcon", "fake_crash")
	e.waitStopped()
	if o := e.must("status"); !strings.Contains(o, "closed by itself") {
		t.Fatalf("no explanation:\n%s", o)
	}
	e.start()
	e.must("stop")
	if o := e.must("status"); strings.Contains(o, "closed by itself") {
		t.Fatalf("a requested stop is no crash:\n%s", o)
	}
}

func TestSamePasswordsWarn(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	e.writeConfig("same-long-password", "same-long-password")
	if o := e.must("start", "awp"); !strings.Contains(o, "different") {
		t.Fatalf("no warning:\n%s", o)
	}
}

func TestOpenConfigWarns(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	p := filepath.Join(e.root, "config", "csgo-legacy-server", "config.toml")
	if err := os.Chmod(p, 0o644); err != nil {
		t.Fatal(err)
	}
	if o := e.must("start", "awp"); !strings.Contains(o, "chmod 600") {
		t.Fatalf("no warning:\n%s", o)
	}
}

func TestCorruptConfig(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	write(t, filepath.Join(e.root, "config", "csgo-legacy-server", "config.toml"), "not [ toml", 0o600)
	e.mustFail("invalid config", "status")
}

func TestPluginSyntaxError(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	write(t, filepath.Join(e.root, "config", "csgo-legacy-server", "plugins", "broken.sp"), "public Plugin myinfo;\nSYNTAX_ERROR\n", 0o644)
	e.mustFail("error 001", "start", "awp")
}

func TestPluginRemovedAndChanged(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	src := filepath.Join(e.root, "config", "csgo-legacy-server", "plugins", "mine.sp")
	write(t, src, "// myinfo one\n", 0o644)
	if o := e.must("deploy"); !strings.Contains(o, "mine") {
		t.Fatalf("not compiled:\n%s", o)
	}
	smx := filepath.Join(e.install, "csgo", "addons", "sourcemod", "plugins", "mine.smx")
	if _, err := os.Stat(smx); err != nil {
		t.Fatal(err)
	}
	if o := e.must("deploy"); strings.Contains(o, "mine") {
		t.Fatalf("recompiled with no change:\n%s", o)
	}
	write(t, src, "// myinfo two\n", 0o644)
	if o := e.must("deploy"); !strings.Contains(o, "mine") {
		t.Fatalf("change not compiled:\n%s", o)
	}
	if err := os.Remove(src); err != nil {
		t.Fatal(err)
	}
	e.must("deploy")
	if _, err := os.Stat(smx); !os.IsNotExist(err) {
		t.Fatalf("removed plugin still deployed: %v", err)
	}
}
