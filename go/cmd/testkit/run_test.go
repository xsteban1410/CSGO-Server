package main

import (
	"os"
	"path/filepath"
	"testing"
)

func TestCommonPositions(t *testing.T) {
	if n := common([]string{"a", "b", "c"}, []string{"b", "c", "d"}); n != 2 {
		t.Fatal(n)
	}
}

func TestFingerprintFollowsWhatTheCaseUses(t *testing.T) {
	repo := t.TempDir()
	put := func(name, text string) {
		p := filepath.Join(repo, name)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(text), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	put("cfg/modo_base.cfg", "1")
	put("cfg/modo_awp.cfg", "1")
	put("cfg/modo_dm.cfg", "1")
	put("plugin/modos.sp", "1")
	awp := fingerprint(repo, "awp")
	put("cfg/modo_dm.cfg", "2")
	if fingerprint(repo, "awp") != awp {
		t.Fatal("another mode's cfg must not count")
	}
	put("cfg/modo_base.cfg", "2")
	if fingerprint(repo, "awp") == awp {
		t.Fatal("modo_base must count")
	}
	awp = fingerprint(repo, "awp")
	put("plugin/modos.sp", "2")
	if fingerprint(repo, "awp") == awp {
		t.Fatal("the plugins must count")
	}
}

func TestLintFindsProblems(t *testing.T) {
	repo := t.TempDir()
	put := func(name, text string) {
		p := filepath.Join(repo, name)
		_ = os.MkdirAll(filepath.Dir(p), 0o755)
		_ = os.WriteFile(p, []byte(text), 0o644)
	}
	put("modes/a.toml", `category = "x"`+"\n"+`name = { en = "A", es = "A" }`+"\n"+`description = { en = "A", es = "A" }`)
	put("cfg/modo_a.cfg", "modo_new 1\nhostname \"oops\n")
	put("cfg/modo_base.cfg", "sv_cheats 0\n")
	put("plugin/x.sp", `PrintToChatAll("%t", "missing_phrase");`)
	put("plugin/translations/csls.phrases.txt", "\"Phrases\"\n{\n}\n")
	got := lint(repo)
	if len(got) != 3 {
		t.Fatalf("wanted 3 problems: %q", got)
	}
}

func TestLintPassesOnTheRepo(t *testing.T) {
	if p := lint(filepath.Join("..", "..", "..", "game")); len(p) != 0 {
		t.Fatalf("%q", p)
	}
}
