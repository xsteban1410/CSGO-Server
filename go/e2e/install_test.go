package e2e

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// fakeSteamcmd "installs" app 740 into the folder it is given, or fails as FAKE_STEAMCMD says.
const fakeSteamcmd = `#!/bin/sh
case "$FAKE_STEAMCMD" in
offline) echo "Update state: no connection"; exit 1;;
full) echo "Error! App '740' state is 0x202 after update job."; exit 8;;
esac
while [ $# -gt 0 ]; do [ "$1" = "+force_install_dir" ] && dir="$2"; shift; done
mkdir -p "$dir/csgo/maps" "$dir/csgo/cfg"
: > "$dir/csgo/maps/de_dust2.bsp"
printf '#!/bin/sh\nexit 0\n' > "$dir/srcds_run"; chmod +x "$dir/srcds_run"
`

type installEnv struct {
	*env
	dl   string
	srv  *httptest.Server
	pins map[string]string
}

func needs32bit(t *testing.T) {
	for _, p := range []string{"/lib/ld-linux.so.2", "/lib32/ld-linux.so.2", "/usr/lib32/ld-linux.so.2"} {
		if _, err := os.Stat(p); err == nil {
			return
		}
	}
	t.Skip("install refuses to run without the 32-bit libc")
}

// pack builds a package from files and returns its name and hash.
func pack(t *testing.T, dir, name string, files map[string]string) (string, string) {
	t.Helper()
	src := t.TempDir()
	for f, text := range files {
		mode := os.FileMode(0o644)
		if strings.HasSuffix(f, "spcomp") {
			mode = 0o755
		}
		write(t, filepath.Join(src, f), text, mode)
	}
	out := filepath.Join(dir, name)
	if o, err := exec.Command("tar", "-czf", out, "-C", src, "addons").CombinedOutput(); err != nil {
		t.Fatalf("tar: %v\n%s", err, o)
	}
	b, err := os.ReadFile(out)
	if err != nil {
		t.Fatal(err)
	}
	sum := sha256.Sum256(b)
	return name, hex.EncodeToString(sum[:])
}

func smFiles(version string) map[string]string {
	return map[string]string{
		"addons/sourcemod/bin/sourcemod_mm_i486.so": "",
		"addons/sourcemod/bin/version":              version,
		"addons/sourcemod/scripting/spcomp":         fakeSpcomp,
		"addons/sourcemod/configs/core.cfg":         "\"FollowCSGOServerGuidelines\" \"yes\"\n",
	}
}

func (e *installEnv) publish(t *testing.T, version string) {
	t.Helper()
	mm, mmSum := pack(t, e.dl, "mm-"+version+".tar.gz", map[string]string{"addons/metamod/bin/server.so": ""})
	sm, smSum := pack(t, e.dl, "sm-"+version+".tar.gz", smFiles(version))
	write(t, filepath.Join(e.root, "pinned.toml"), fmt.Sprintf(
		"[metamod]\nversion = %q\nbase = %q\nfile = %q\nsha256 = %q\n[sourcemod]\nversion = %q\nbase = %q\nfile = %q\nsha256 = %q\n",
		version, e.srv.URL, mm, mmSum, version, e.srv.URL, sm, smSum), 0o644)
}

// newInstallEnv is a machine with nothing installed yet, a fake SteamCMD and a local download server.
func newInstallEnv(t *testing.T) *installEnv {
	t.Helper()
	needs32bit(t)
	e := &installEnv{env: &env{t: t, root: t.TempDir(), port: freePort(t)}, dl: t.TempDir()}
	e.install = filepath.Join(e.root, "server")
	e.writeConfig(password, "")
	mux := http.NewServeMux()
	mux.Handle("/", http.FileServer(http.Dir(e.dl)))
	// sends half of the file and then cuts the connection
	mux.HandleFunc("/cut/", func(w http.ResponseWriter, r *http.Request) {
		b, _ := os.ReadFile(filepath.Join(e.dl, filepath.Base(r.URL.Path)))
		w.Header().Set("Content-Length", fmt.Sprint(len(b)))
		w.WriteHeader(200)
		_, _ = w.Write(b[:len(b)/2])
		w.(http.Flusher).Flush()
		conn, _, _ := w.(http.Hijacker).Hijack()
		conn.(*net.TCPConn).Close()
	})
	e.srv = httptest.NewServer(mux)
	t.Cleanup(e.srv.Close)
	bin := filepath.Join(e.root, "bin")
	write(t, filepath.Join(bin, "steamcmd"), fakeSteamcmd, 0o755)
	e.extra = []string{
		"PATH=" + bin + ":" + os.Getenv("PATH"),
		"CSLS_PINNED=" + filepath.Join(e.root, "pinned.toml"),
		"CSLS_NO_RETRY_WAIT=1",
		"CSLS_NEEDED_BYTES=1024",
	}
	e.publish(t, "one")
	return e
}

func (e *installEnv) pinnedText() string {
	b, err := os.ReadFile(filepath.Join(e.root, "pinned.toml"))
	if err != nil {
		e.t.Fatal(err)
	}
	return string(b)
}

func (e *installEnv) installedVersion() string {
	b, err := os.ReadFile(filepath.Join(e.install, "csgo", "addons", "sourcemod", "bin", "version"))
	if err != nil {
		e.t.Fatal(err)
	}
	return string(b)
}

func TestInstallFromNothing(t *testing.T) {
	t.Parallel()
	e := newInstallEnv(t)
	e.must("install")
	if v := e.installedVersion(); v != "one" {
		t.Fatal(v)
	}
	man, err := os.ReadFile(filepath.Join(e.root, "data", "csgo-legacy-server", "install.json"))
	if err != nil || !strings.Contains(string(man), `"sourcemod": "one"`) {
		t.Fatalf("manifest: %v\n%s", err, man)
	}
	// repeating it does no harm
	e.must("install")
}

func TestInterruptedDownload(t *testing.T) {
	t.Parallel()
	e := newInstallEnv(t)
	text := strings.ReplaceAll(e.pinnedText(), e.srv.URL, e.srv.URL+"/cut")
	write(t, filepath.Join(e.root, "pinned.toml"), text, 0o644)
	e.mustFail("could not download", "install")
	matches, _ := filepath.Glob(filepath.Join(e.root, "data", "csgo-legacy-server", "dl", "*"))
	if len(matches) != 0 {
		t.Fatalf("a cut download stayed behind: %v", matches)
	}
	// the server folder is marked incomplete, so it is not taken for a working install
	if _, err := os.Stat(filepath.Join(e.install, ".csls-install-incomplete")); err != nil {
		t.Fatal(err)
	}
	e.mustFail("did not finish", "start", "awp")
	// and the next try, with a good download, finishes it
	e.publish(t, "one")
	e.must("install")
	// it gets as far as running the server, which in this fake install exits at once
	e.mustFail("closed while starting", "start", "awp")
}

func TestCorruptDownload(t *testing.T) {
	t.Parallel()
	e := newInstallEnv(t)
	sum := "0000000000000000000000000000000000000000000000000000000000000000"
	text := e.pinnedText()
	i := strings.LastIndex(text, "sha256 = ")
	write(t, filepath.Join(e.root, "pinned.toml"), text[:i]+fmt.Sprintf("sha256 = %q\n", sum), 0o644)
	e.mustFail("does not match its checksum", "install")
}

func TestDamagedPackage(t *testing.T) {
	t.Parallel()
	e := newInstallEnv(t)
	bad := filepath.Join(e.dl, "bad.tar.gz")
	write(t, bad, "this is not a tarball", 0o644)
	sum := sha256.Sum256([]byte("this is not a tarball"))
	write(t, filepath.Join(e.root, "pinned.toml"), fmt.Sprintf(
		"[metamod]\nversion = \"x\"\nbase = %q\nfile = \"bad.tar.gz\"\nsha256 = %q\n[sourcemod]\nversion = \"x\"\nbase = %q\nfile = \"bad.tar.gz\"\nsha256 = %q\n",
		e.srv.URL, hex.EncodeToString(sum[:]), e.srv.URL, hex.EncodeToString(sum[:])), 0o644)
	e.mustFail("is damaged", "install")
}

func TestDiskFullDuringDownload(t *testing.T) {
	t.Parallel()
	e := newInstallEnv(t)
	dl := filepath.Join(e.root, "data", "csgo-legacy-server", "dl")
	if err := os.MkdirAll(dl, 0o755); err != nil {
		t.Fatal(err)
	}
	// writes to /dev/full fail with ENOSPC, the same error as a real full disk
	if err := os.Symlink("/dev/full", filepath.Join(dl, "mm-one.tar.gz.part")); err != nil {
		t.Fatal(err)
	}
	e.mustFail("could not download", "install")
}

func TestNotEnoughSpace(t *testing.T) {
	t.Parallel()
	e := newInstallEnv(t)
	e.extra = append(e.extra, "CSLS_NEEDED_BYTES=99999999999999999")
	e.mustFail("free needed", "install")
}

func TestSteamcmdOffline(t *testing.T) {
	t.Parallel()
	e := newInstallEnv(t)
	e.extra = append(e.extra, "FAKE_STEAMCMD=offline")
	e.mustFail("could not install app 740", "install")
}

func TestSteamcmdDiskFull(t *testing.T) {
	t.Parallel()
	e := newInstallEnv(t)
	e.extra = append(e.extra, "FAKE_STEAMCMD=full")
	e.mustFail("ran out of disk space", "install")
}

func TestFailedUpdateRollsBack(t *testing.T) {
	t.Parallel()
	e := newInstallEnv(t)
	e.must("install")
	e.publish(t, "two")
	write(t, filepath.Join(e.root, "config", "csgo-legacy-server", "plugins", "broken.sp"), "public Plugin myinfo;\nSYNTAX_ERROR\n", 0o644)
	e.mustFail("do not compile", "install", "--update")
	if v := e.installedVersion(); v != "one" {
		t.Fatalf("still on %s after a failed update", v)
	}
	if _, err := os.Stat(filepath.Join(e.install, ".csls-install-incomplete")); !os.IsNotExist(err) {
		t.Fatalf("the rolled back install should count as complete: %v", err)
	}
	// with the problem gone, the update applies
	if err := os.Remove(filepath.Join(e.root, "config", "csgo-legacy-server", "plugins", "broken.sp")); err != nil {
		t.Fatal(err)
	}
	e.must("install", "--update")
	if v := e.installedVersion(); v != "two" {
		t.Fatal(v)
	}
}

func TestUpdateRefusedWhileRunning(t *testing.T) {
	t.Parallel()
	e := newEnv(t)
	e.start()
	e.mustFail("stop the server", "install", "--update")
}
