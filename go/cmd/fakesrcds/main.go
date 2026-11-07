// fakesrcds stands in for srcds_run in tests: it speaks RCON and A2S in milliseconds and can misbehave on
// request. It reads the rcon password from cfg/csls_secrets.cfg, as the real server does.
//
// FAKESRCDS_START_DELAY: wait this long before listening (e.g. 2s).
// FAKESRCDS_FAIL_MAP:    changelevel to this map fails.
// RCON commands fake_crash (dies with exit 139), fake_hang (stops answering) and fake_drop (closes the connection).
package main

import (
	"fmt"
	"net"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"time"

	"github.com/xsteban1410/CSGO-Server/go/internal/rcon"
)

type server struct {
	mu       sync.Mutex
	password string
	cvars    map[string]string
	hang     chan struct{}
	failMap  string
}

var secret = regexp.MustCompile(`rcon_password "([^"]*)"`)

func readPassword() string {
	b, err := os.ReadFile(filepath.Join("csgo", "cfg", "csls_secrets.cfg"))
	if err != nil {
		return ""
	}
	if m := secret.FindSubmatch(b); m != nil {
		return string(m[1])
	}
	return ""
}

func (s *server) exec(line string) string {
	s.mu.Lock()
	defer s.mu.Unlock()
	f := strings.Fields(line)
	if len(f) == 0 {
		return ""
	}
	switch f[0] {
	case "quit":
		go func() { time.Sleep(20 * time.Millisecond); os.Exit(0) }()
		return ""
	case "fake_crash":
		os.Exit(139)
	case "fake_hang":
		<-s.hang
	case "status":
		return fmt.Sprintf("hostname: fake\nmap     : %s\nplayers : 0 humans, %s bots (20/0 max) (not hibernating)\n", s.cvars["host_map_name"], s.cvars["bot_quota"])
	case "echo":
		return strings.Join(f[1:], " ") + "\n"
	case "changelevel":
		if len(f) > 1 && f[1] == s.failMap {
			return fmt.Sprintf("changelevel failed: %s not found\n", f[1])
		}
		if len(f) > 1 {
			s.cvars["host_map_name"] = f[1]
			s.cvars["host_map"] = f[1] + ".bsp"
		}
		return ""
	}
	if len(f) == 1 {
		v, ok := s.cvars[f[0]]
		if !ok {
			return fmt.Sprintf("Unknown command \"%s\"\n", f[0])
		}
		return fmt.Sprintf("\"%s\" = \"%s\" ( def. \"\" )\n", f[0], v)
	}
	s.cvars[f[0]] = strings.Trim(strings.TrimSpace(strings.TrimPrefix(line, f[0])), `"`)
	return ""
}

func (s *server) serve(c net.Conn) {
	defer c.Close()
	authed := false
	for {
		id, kind, body, err := rcon.ReadPacket(c)
		if err != nil {
			return
		}
		switch {
		case kind == rcon.TypeAuth:
			_ = rcon.WritePacket(c, id, rcon.TypeResponse, "")
			if body != s.password || s.password == "" {
				_ = rcon.WritePacket(c, -1, rcon.TypeAuthResp, "")
				return
			}
			authed = true
			_ = rcon.WritePacket(c, id, rcon.TypeAuthResp, "")
		case !authed:
			return
		case kind == rcon.TypeExec && body == "fake_drop":
			return
		case kind == rcon.TypeExec:
			if err := rcon.WritePacket(c, id, rcon.TypeResponse, s.exec(body)); err != nil {
				return
			}
		default: // the empty marker the client sends to find the end of a response
			if err := rcon.WritePacket(c, id, rcon.TypeResponse, ""); err != nil {
				return
			}
		}
	}
}

// answerA2S replies to A2S_INFO just enough for a client to see a server there.
func answerA2S(u *net.UDPConn) {
	b := make([]byte, 1400)
	for {
		n, from, err := u.ReadFromUDP(b)
		if err != nil {
			return
		}
		if n > 5 && b[4] == 'T' {
			_, _ = u.WriteToUDP(append([]byte{0xff, 0xff, 0xff, 0xff, 'I', 17}, []byte("fake\x00de_dust2\x00csgo\x00fake\x00")...), from)
		}
	}
}

func main() {
	port := 0
	for i, a := range os.Args {
		if a == "-port" && i+1 < len(os.Args) {
			_, _ = fmt.Sscan(os.Args[i+1], &port)
		}
	}
	if port == 0 {
		fmt.Fprintln(os.Stderr, "fakesrcds: no -port")
		os.Exit(2)
	}
	if d, err := time.ParseDuration(os.Getenv("FAKESRCDS_START_DELAY")); err == nil {
		time.Sleep(d)
	}
	map0 := "de_dust2"
	for i, a := range os.Args {
		if a == "+map" && i+1 < len(os.Args) {
			map0 = os.Args[i+1]
		}
	}
	s := &server{
		password: readPassword(),
		hang:     make(chan struct{}),
		failMap:  os.Getenv("FAKESRCDS_FAIL_MAP"),
		cvars:    map[string]string{"host_map": map0 + ".bsp", "host_map_name": map0, "bot_quota": "0", "modo_nombre": "", "sv_hibernate_when_empty": "1"},
	}
	u, err := net.ListenUDP("udp", &net.UDPAddr{Port: port})
	if err != nil {
		fmt.Fprintln(os.Stderr, "fakesrcds:", err)
		os.Exit(1)
	}
	go answerA2S(u)
	l, err := net.Listen("tcp", fmt.Sprintf("127.0.0.1:%d", port))
	if err != nil {
		fmt.Fprintln(os.Stderr, "fakesrcds:", err)
		os.Exit(1)
	}
	for {
		c, err := l.Accept()
		if err != nil {
			return
		}
		go s.serve(c)
	}
}
