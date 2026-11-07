// Package logs follows growing log files by offset, so a big log is never read twice, and rotates them by size.
package logs

import (
	"bytes"
	"fmt"
	"io"
	"os"
	"regexp"
	"strings"
	"sync"
	"time"
)

// Tail returns complete lines added to a file since the last call.
type Tail struct {
	Path   string
	Offset int64
	rest   []byte
}

// maxRead bounds one call, so a sudden huge log cannot use up the memory.
const maxRead = 4 << 20

// Read returns the new complete lines. A file that shrank was rotated, so it starts over.
func (t *Tail) Read() ([]string, error) {
	f, err := os.Open(t.Path)
	if err != nil {
		if os.IsNotExist(err) {
			t.Offset, t.rest = 0, nil
			return nil, nil
		}
		return nil, err
	}
	defer f.Close()
	info, err := f.Stat()
	if err != nil {
		return nil, err
	}
	if info.Size() < t.Offset {
		t.Offset, t.rest = 0, nil
	}
	if info.Size() == t.Offset {
		return nil, nil
	}
	b, err := io.ReadAll(io.NewSectionReader(f, t.Offset, maxRead))
	if err != nil {
		return nil, err
	}
	t.Offset += int64(len(b))
	b = append(t.rest, b...)
	i := bytes.LastIndexByte(b, '\n')
	if i < 0 {
		t.rest = b
		return nil, nil
	}
	t.rest = append([]byte(nil), b[i+1:]...)
	return strings.Split(string(b[:i]), "\n"), nil
}

// Rotate keeps `keep` old copies once a file passes maxBytes. srcds keeps writing to the same path, so the file is
// copied and then cut, not renamed; a line written between the two can be lost, which is fine for a log.
func Rotate(path string, maxBytes int64, keep int) (bool, error) {
	info, err := os.Stat(path)
	if err != nil || info.Size() < maxBytes {
		return false, nil
	}
	for i := keep - 1; i >= 1; i-- {
		_ = os.Rename(fmt.Sprintf("%s.%d", path, i), fmt.Sprintf("%s.%d", path, i+1))
	}
	in, err := os.Open(path)
	if err != nil {
		return false, err
	}
	defer in.Close()
	out, err := os.OpenFile(path+".1", os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o600)
	if err != nil {
		return false, err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return false, err
	}
	if err := out.Close(); err != nil {
		return false, err
	}
	return true, os.Truncate(path, 0)
}

// Category of a console line.
const (
	CatError   = "error"
	CatWarning = "warning"
	CatChat    = "chat"
	CatPlayer  = "player"
	CatPlugin  = "plugin"
	CatServer  = "server"
)

var (
	errorRe  = regexp.MustCompile(`(?i)\b(error|failed|fatal|exception|segmentation|not found|unable to)\b`)
	warnRe   = regexp.MustCompile(`(?i)\b(warning|warn|deprecated)\b`)
	chatRe   = regexp.MustCompile(`^(\[ALL\]|\[T\]|\[CT\]|\*DEAD\*|.*\bsay(_team)?\b)`)
	playerRe = regexp.MustCompile(`(?i)(connected|disconnect|dropped|joined|entered the game|kicked|banned)`)
	pluginRe = regexp.MustCompile(`^(\[SM\]|\[META\]|\[Modos\]|\[Premier\]|\[Clutch\]|\[Jefe\]|L \d\d/\d\d/\d{4})`)
)

// Classify gives the category of a line for colors and filters.
func Classify(l string) string {
	switch {
	case errorRe.MatchString(l):
		return CatError
	case warnRe.MatchString(l):
		return CatWarning
	case pluginRe.MatchString(l):
		return CatPlugin
	case chatRe.MatchString(l):
		return CatChat
	case playerRe.MatchString(l):
		return CatPlayer
	}
	return CatServer
}

// Line is one line of the console, numbered so a client can ask for what it has not seen.
type Line struct {
	Seq      int64     `json:"seq"`
	Time     time.Time `json:"time"`
	Category string    `json:"category"`
	Text     string    `json:"text"`
}

// Ring keeps the last lines in memory.
type Ring struct {
	mu   sync.Mutex
	max  int
	next int64
	buf  []Line
}

// NewRing makes a ring of max lines.
func NewRing(max int) *Ring { return &Ring{max: max, next: 1} }

// Add appends lines, marking the polling lines the agent causes itself as nothing to show.
func (r *Ring) Add(now time.Time, lines []string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	for _, l := range lines {
		l = strings.TrimRight(l, "\r")
		if l == "" {
			continue
		}
		r.buf = append(r.buf, Line{Seq: r.next, Time: now, Category: Classify(l), Text: l})
		r.next++
	}
	if extra := len(r.buf) - r.max; extra > 0 {
		r.buf = append(r.buf[:0], r.buf[extra:]...)
	}
}

// Since returns the lines after seq.
func (r *Ring) Since(seq int64, limit int) []Line {
	r.mu.Lock()
	defer r.mu.Unlock()
	out := []Line{}
	for _, l := range r.buf {
		if l.Seq > seq {
			out = append(out, l)
		}
	}
	if limit > 0 && len(out) > limit {
		out = out[len(out)-limit:]
	}
	return out
}
