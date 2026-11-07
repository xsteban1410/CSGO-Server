// testkit checks the game modes quickly.
//
//	testkit lint            coherence of cfg, modes and plugin sources; no server (CI)
//	testkit smoke           one case per game type on real servers
//	testkit full [case...]  every mode and match type
//
// Servers are test instances on their own high ports, with the install shared; they never touch the user's
// server. Results are cached by the hash of what each case depends on, so unchanged cases do not run again.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

func main() {
	flag.Usage = func() {
		fmt.Fprintln(os.Stderr, "usage: testkit [flags] lint | smoke | full [mode[:type] ...]")
		flag.PrintDefaults()
	}
	jobs := flag.Int("jobs", 0, "servers at once (0 = as many as free RAM allows)")
	noCache := flag.Bool("no-cache", false, "run cases whose inputs did not change")
	bin := flag.String("bin", "", "the CLI (default target/debug/csgo-legacy-server)")
	root := flag.String("root", ".", "the repo")
	flag.Parse()

	repo, err := filepath.Abs(*root)
	if err != nil {
		fatal(err)
	}
	if *bin == "" {
		*bin = filepath.Join(repo, "target", "debug", "csgo-legacy-server")
	}
	start := time.Now()
	failed := 0
	switch flag.Arg(0) {
	case "lint":
		problems := lint(repo)
		for _, p := range problems {
			fmt.Println(p)
		}
		failed = len(problems)
	case "smoke":
		failed = runCases(repo, *bin, smokeCases, *jobs, *noCache)
	case "full":
		cases := flag.Args()[1:]
		if len(cases) == 0 {
			cases = allCases(*bin)
		}
		failed = runCases(repo, *bin, cases, *jobs, *noCache)
	default:
		flag.Usage()
		os.Exit(2)
	}
	fmt.Printf("%d failures in %s\n", failed, time.Since(start).Round(time.Second))
	if failed > 0 {
		os.Exit(1)
	}
}

func fatal(err error) {
	fmt.Fprintln(os.Stderr, "testkit:", err)
	os.Exit(2)
}

// A case per game type.
var smokeCases = []string{"normal", "competitivo", "wingman", "dm", "armsrace", "guardian", "dangerzone", "normal:ffa", "normal:tdm", "normal:demolition", "retake"}

func allCases(bin string) []string {
	o, err := exec.Command(bin, "modes").Output()
	if err != nil {
		fatal(fmt.Errorf("listing modes: %w", err))
	}
	var cases []string
	for _, l := range strings.Split(strings.TrimSpace(string(o)), "\n") {
		if f := strings.Fields(l); len(f) > 0 {
			cases = append(cases, f[0])
		}
	}
	return append(cases, "normal:short", "normal:casual", "normal:wingman", "normal:dm", "normal:ffa", "normal:tdm", "normal:armsrace", "normal:demolition", "normal:guardian", "normal:dz", "awp:dm")
}

// instanceInfo is the output of `csgo-legacy-server test-instance`.
type instanceInfo struct {
	Dir          string   `json:"dir"`
	Argv         []string `json:"argv"`
	RconPassword string   `json:"rcon_password"`
	RconHosts    []string `json:"rcon_hosts"`
}

// modePlan is the output of `csgo-legacy-server mode --plan`.
type modePlan struct {
	Map      string   `json:"map"`
	Commands []string `json:"commands"`
}

func cliJSON(bin string, out any, args ...string) error {
	c := exec.Command(bin, args...)
	var stderr strings.Builder
	c.Stderr = &stderr
	o, err := c.Output()
	if err != nil {
		return fmt.Errorf("%s: %w: %s", strings.Join(args, " "), err, strings.TrimSpace(stderr.String()))
	}
	return json.Unmarshal(o, out)
}
