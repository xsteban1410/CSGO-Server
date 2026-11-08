package main

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

var (
	phraseUse  = regexp.MustCompile(`"%t",\s*(?:[^"]*\?\s*)?"([a-z_0-9]+)"(?:\s*:\s*"([a-z_0-9]+)")?`)
	phraseDef  = regexp.MustCompile(`(?m)^\s*"([a-z_0-9]+)"\s*$`)
	modoConvar = regexp.MustCompile(`(?m)^\s*(modo_[a-z_0-9]+)\s`)
	baseOnly   = map[string]bool{"modo_base": true, "modo_clasico": true}
)

func readLines(path string) []string {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil
	}
	return strings.Split(string(b), "\n")
}

// lint finds what would only show up as a broken mode on a running server.
func lint(repo string) []string {
	var p []string
	metas, _ := filepath.Glob(filepath.Join(repo, "modes", "*.toml"))
	cfgs, _ := filepath.Glob(filepath.Join(repo, "cfg", "modo_*.cfg"))
	// the engine has no modes of its own (they live in CSGO-Hub, linted by its tools/lint.py), only modo_base and friends
	if len(cfgs) == 0 {
		return []string{"no cfgs under " + repo}
	}
	sort.Strings(metas)
	hasMeta := map[string]bool{}
	for _, m := range metas {
		id := strings.TrimSuffix(filepath.Base(m), ".toml")
		hasMeta[id] = true
		if _, err := os.Stat(filepath.Join(repo, "cfg", "modo_"+id+".cfg")); err != nil {
			p = append(p, fmt.Sprintf("modes/%s.toml: no cfg/modo_%s.cfg", id, id))
		}
		text, _ := os.ReadFile(m)
		for _, want := range []string{"name = {", "description = {", "en = ", "es = ", "category = "} {
			if !strings.Contains(string(text), want) {
				p = append(p, fmt.Sprintf("modes/%s.toml: missing %q", id, want))
			}
		}
	}
	base := map[string]bool{}
	for _, m := range modoConvar.FindAllStringSubmatch(strings.Join(readLines(filepath.Join(repo, "cfg", "modo_base.cfg")), "\n"), -1) {
		base[m[1]] = true
	}
	for name := range resetByPlugins(filepath.Join(repo, "plugin")) {
		base[name] = true
	}
	for _, c := range cfgs {
		name := strings.TrimSuffix(filepath.Base(c), ".cfg")
		id := strings.TrimPrefix(name, "modo_")
		if !baseOnly[name] && !strings.HasPrefix(id, "tipo_") && !hasMeta[id] {
			p = append(p, fmt.Sprintf("cfg/%s.cfg: no modes/%s.toml", name, id))
		}
		for i, l := range readLines(c) {
			l = strings.TrimSpace(l)
			if l == "" || strings.HasPrefix(l, "//") {
				continue
			}
			if strings.Count(l, `"`)%2 != 0 {
				p = append(p, fmt.Sprintf("cfg/%s.cfg:%d: unbalanced quotes", name, i+1))
			}
			// whatever a mode turns on has to be undone for the next one, or it leaks into it
			if f := strings.Fields(l); strings.HasPrefix(f[0], "modo_") && !base[f[0]] && !settingOfOneMode(f[0]) {
				p = append(p, fmt.Sprintf("cfg/%s.cfg:%d: %s is not reset in cfg/modo_base.cfg", name, i+1, f[0]))
			}
		}
	}
	// Every phrase a plugin names has to exist, in csls.phrases.txt or in the plugin's own translations.
	shared := phrasesIn(filepath.Join(repo, "plugin", "translations"))
	for _, s := range pluginSources(filepath.Join(repo, "plugin")) {
		own := phrasesIn(filepath.Join(filepath.Dir(s), "translations"))
		text := strings.Join(readLines(s), "\n")
		for _, m := range phraseUse.FindAllStringSubmatch(text, -1) {
			for _, key := range m[1:] {
				if key != "" && !shared[key] && !own[key] {
					p = append(p, fmt.Sprintf("plugin/%s: phrase %q is not in its translations", filepath.Base(s), key))
				}
			}
		}
	}
	return p
}

// pluginSources lists X.sp and N/N.sp under dir, the same plugins deploy finds.
func pluginSources(dir string) []string {
	top, _ := filepath.Glob(filepath.Join(dir, "*.sp"))
	sub, _ := filepath.Glob(filepath.Join(dir, "*", "*.sp"))
	return append(top, sub...)
}

// phrasesIn reads the phrase names of every .phrases.txt in dir.
func phrasesIn(dir string) map[string]bool {
	files, _ := filepath.Glob(filepath.Join(dir, "*.phrases.txt"))
	defined := map[string]bool{}
	for _, f := range files {
		for _, m := range phraseDef.FindAllStringSubmatch(strings.Join(readLines(f), "\n"), -1) {
			defined[m[1]] = true
		}
	}
	return defined
}

var createConvar = regexp.MustCompile(`CreateConVar\("(modo_[a-z0-9_]+)"`)

// resetByPlugins finds the modo_* convars of plugins that put their switches back in CSLS_OnModeReset,
// which replaces listing them in modo_base.cfg.
func resetByPlugins(dir string) map[string]bool {
	out := map[string]bool{}
	for _, s := range pluginSources(dir) {
		text := strings.Join(readLines(s), "\n")
		if !strings.Contains(text, "CSLS_OnModeReset") {
			continue
		}
		for _, m := range createConvar.FindAllStringSubmatch(text, -1) {
			out[m[1]] = true
		}
	}
	return out
}

// settingOfOneMode tells apart the tuning convars read only while their own mode is on: their mode's switch is
// what base resets, so a value left behind does nothing.
func settingOfOneMode(name string) bool {
	for _, prefix := range []string{"modo_z_", "modo_cheats_", "modo_clutch_", "modo_veto_", "modo_vampiro_"} {
		if strings.HasPrefix(name, prefix) {
			return true
		}
	}
	return false
}
