package logs

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestTailReadsOnlyWhatIsNew(t *testing.T) {
	p := filepath.Join(t.TempDir(), "x.log")
	tail := &Tail{Path: p}
	if l, err := tail.Read(); err != nil || l != nil {
		t.Fatalf("missing file: %v %v", l, err)
	}
	if err := os.WriteFile(p, []byte("one\ntwo\nhal"), 0o644); err != nil {
		t.Fatal(err)
	}
	l, _ := tail.Read()
	if len(l) != 2 || l[1] != "two" {
		t.Fatalf("%q", l)
	}
	f, _ := os.OpenFile(p, os.O_APPEND|os.O_WRONLY, 0)
	_, _ = f.WriteString("f\n")
	f.Close()
	if l, _ := tail.Read(); len(l) != 1 || l[0] != "half" {
		t.Fatalf("%q", l)
	}
	// rotated: starts over
	_ = os.WriteFile(p, []byte("x\n"), 0o644)
	if l, _ := tail.Read(); len(l) != 1 || l[0] != "x" {
		t.Fatalf("%q", l)
	}
}

func TestRotateKeepsCopies(t *testing.T) {
	p := filepath.Join(t.TempDir(), "x.log")
	_ = os.WriteFile(p, []byte("small"), 0o644)
	if done, _ := Rotate(p, 100, 2); done {
		t.Fatal("under the limit")
	}
	for _, text := range []string{"first", "second", "third"} {
		_ = os.WriteFile(p, []byte(text), 0o644)
		if done, err := Rotate(p, 1, 2); !done || err != nil {
			t.Fatal(done, err)
		}
	}
	if b, _ := os.ReadFile(p + ".1"); string(b) != "third" {
		t.Fatal(string(b))
	}
	if b, _ := os.ReadFile(p + ".2"); string(b) != "second" {
		t.Fatal(string(b))
	}
	if _, err := os.Stat(p + ".3"); !os.IsNotExist(err) {
		t.Fatal("kept too many")
	}
	if info, _ := os.Stat(p); info.Size() != 0 {
		t.Fatal("not cut")
	}
}

func TestClassify(t *testing.T) {
	for line, want := range map[string]string{
		"[SM] Exception reported: oops":   CatError,
		"Failed to load plugin x":         CatError,
		"Warning: something old":          CatWarning,
		"[SM] Loaded plugin modos":        CatPlugin,
		"Luis connected":                  CatPlayer,
		"[ALL] Luis: hola":                CatChat,
		"Executing dedicated server cfg.": CatServer,
	} {
		if got := Classify(line); got != want {
			t.Errorf("%q: got %s, want %s", line, got, want)
		}
	}
}

func TestRing(t *testing.T) {
	r := NewRing(2)
	r.Add(time.Now(), []string{"a", "", "b\r", "c"})
	got := r.Since(0, 0)
	if len(got) != 2 || got[0].Text != "b" || got[1].Text != "c" {
		t.Fatalf("%+v", got)
	}
	if got := r.Since(got[1].Seq, 0); len(got) != 0 {
		t.Fatalf("%+v", got)
	}
}
