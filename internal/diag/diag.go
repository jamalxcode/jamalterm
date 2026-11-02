// Package diag keeps a small, private problem log on this Mac.
//
// It exists so that when something goes wrong you can copy a report and
// paste it somewhere yourself. Nothing here is ever sent anywhere.
//
// Only problems are logged (an AI provider refused, an update failed, a
// shell didn't start, a script error in the window). Commands, terminal
// output, AI questions and answers, and keys are never logged. Every line
// is scrubbed before it is written: things that look like secrets are
// hidden, your home folder becomes ~, and email addresses and this
// computer's name are removed.
package diag

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"time"

	"jamalterm/internal/ai"
)

// MaxBytes is the log size before it rolls over (one older file is kept).
const MaxBytes = 256 << 10

// MaxLine is the longest message kept; the rest is cut.
const MaxLine = 400

// Log is the problem log. The zero value logs nothing.
type Log struct {
	mu      sync.Mutex
	path    string
	enabled bool
	now     func() time.Time
	home    string
	host    string
}

// New opens (lazily) the log at path. Nothing is written until the first
// problem.
func New(path string) *Log {
	home, _ := os.UserHomeDir()
	host, _ := os.Hostname()
	host = strings.TrimSuffix(host, ".local")
	return &Log{path: path, enabled: true, now: time.Now, home: home, host: host}
}

// Path is where the log lives.
func (l *Log) Path() string { return l.path }

// SetEnabled turns logging on or off. Turning it off deletes the log.
func (l *Log) SetEnabled(on bool) {
	if l == nil {
		return
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	l.enabled = on
	if !on {
		os.Remove(l.path)
		os.Remove(l.path + ".1")
	}
}

// Enabled reports whether problems are being logged.
func (l *Log) Enabled() bool {
	if l == nil {
		return false
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.enabled
}

// Problem records one problem. area is a short word like "ai", "update",
// "keychain", "shell" or "window".
func (l *Log) Problem(area, format string, args ...any) {
	if l == nil {
		return
	}
	msg := l.Scrub(fmt.Sprintf(format, args...))
	area = l.Scrub(area)
	line := l.now().UTC().Format("2006-01-02 15:04:05") + "  " + area + ": " + msg + "\n"

	l.mu.Lock()
	defer l.mu.Unlock()
	if !l.enabled || l.path == "" {
		return
	}
	if err := os.MkdirAll(filepath.Dir(l.path), 0o700); err != nil {
		return
	}
	if fi, err := os.Stat(l.path); err == nil && fi.Size()+int64(len(line)) > MaxBytes {
		os.Rename(l.path, l.path+".1")
	}
	f, err := os.OpenFile(l.path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		return
	}
	defer f.Close()
	f.WriteString(line)
}

var (
	emailRe = regexp.MustCompile(`[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}`)
	spaceRe = regexp.MustCompile(`\s+`)
)

// Scrub makes a message safe to keep: one line, secrets hidden, home
// folder shown as ~, no email addresses or computer name, and cut short.
func (l *Log) Scrub(s string) string {
	s = strings.TrimSpace(l.ScrubText(spaceRe.ReplaceAllString(s, " ")))
	if r := []rune(s); len(r) > MaxLine {
		s = string(r[:MaxLine]) + "…"
	}
	return s
}

// ScrubText hides secrets, the home folder, email addresses and the
// computer's name in a longer text, keeping its lines.
func (l *Log) ScrubText(s string) string {
	s, _ = ai.Redact(s)
	if l != nil && len(l.home) > 1 {
		s = strings.ReplaceAll(s, l.home, "~")
	}
	s = emailRe.ReplaceAllString(s, "[email]")
	if l != nil && len(l.host) >= 3 {
		s = replaceFold(s, l.host, "[this computer]")
	}
	return s
}

func replaceFold(s, old, repl string) string {
	re := regexp.MustCompile(`(?i)` + regexp.QuoteMeta(old))
	return re.ReplaceAllString(s, repl)
}

// Recent returns up to n of the newest log lines, oldest first.
func (l *Log) Recent(n int) []string {
	if l == nil {
		return nil
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	var lines []string
	for _, p := range []string{l.path + ".1", l.path} {
		f, err := os.Open(p)
		if err != nil {
			continue
		}
		sc := bufio.NewScanner(f)
		sc.Buffer(make([]byte, 0, 4096), 64<<10)
		for sc.Scan() {
			if t := strings.TrimSpace(sc.Text()); t != "" {
				lines = append(lines, t)
			}
		}
		f.Close()
	}
	if len(lines) > n {
		lines = lines[len(lines)-n:]
	}
	return lines
}
