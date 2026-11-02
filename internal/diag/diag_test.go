package diag

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func newTest(t *testing.T) *Log {
	l := New(filepath.Join(t.TempDir(), "logs", "problems.log"))
	l.home = "/Users/jamal"
	l.host = "Jamals-MacBook"
	l.now = func() time.Time { return time.Date(2026, 11, 2, 9, 30, 0, 0, time.UTC) }
	return l
}

func TestScrub(t *testing.T) {
	l := newTest(t)
	in := "open /Users/jamal/secret.txt failed\nkey sk-ant-api03-abcdefghijklmnopqrstuv for me@example.com on jamals-macbook"
	got := l.Scrub(in)
	for _, bad := range []string{"/Users/jamal", "sk-ant", "me@example.com", "macbook", "\n"} {
		if strings.Contains(strings.ToLower(got), strings.ToLower(bad)) {
			t.Errorf("scrubbed text still has %q: %s", bad, got)
		}
	}
	for _, want := range []string{"~/secret.txt", "[hidden secret]", "[email]", "[this computer]"} {
		if !strings.Contains(got, want) {
			t.Errorf("want %q in %s", want, got)
		}
	}
	long := l.Scrub(strings.Repeat("x", MaxLine+50))
	if len([]rune(long)) != MaxLine+1 {
		t.Errorf("long line not cut: %d", len([]rune(long)))
	}
}

func TestProblemAndRecent(t *testing.T) {
	l := newTest(t)
	if got := l.Recent(10); len(got) != 0 {
		t.Fatalf("empty log returned %v", got)
	}
	l.Problem("ai", "deepseek said %d: %s", 401, "Authorization: Bearer abcdefghijk123")
	l.Problem("update", "download failed")
	got := l.Recent(10)
	if len(got) != 2 {
		t.Fatalf("got %d lines: %v", len(got), got)
	}
	if got[0] != "2026-11-02 09:30:00  ai: deepseek said 401: Authorization: Bearer [hidden secret]" {
		t.Errorf("line 0 = %q", got[0])
	}
	if got := l.Recent(1); len(got) != 1 || !strings.Contains(got[0], "update") {
		t.Errorf("Recent(1) = %v", got)
	}
	fi, err := os.Stat(l.Path())
	if err != nil {
		t.Fatal(err)
	}
	if fi.Mode().Perm() != 0o600 {
		t.Errorf("log mode %v, want 0600", fi.Mode().Perm())
	}
}

func TestRollOver(t *testing.T) {
	l := newTest(t)
	msg := strings.Repeat("y", MaxLine)
	for i := 0; i < 2*MaxBytes/MaxLine; i++ {
		l.Problem("x", "%s", msg)
	}
	for _, p := range []string{l.Path(), l.Path() + ".1"} {
		fi, err := os.Stat(p)
		if err != nil {
			t.Fatal(err)
		}
		if fi.Size() > MaxBytes {
			t.Errorf("%s is %d bytes, over %d", p, fi.Size(), MaxBytes)
		}
	}
	if n := len(l.Recent(1 << 20)); n == 0 {
		t.Error("no lines after roll over")
	}
}

func TestDisable(t *testing.T) {
	l := newTest(t)
	l.Problem("a", "one")
	l.SetEnabled(false)
	if _, err := os.Stat(l.Path()); !os.IsNotExist(err) {
		t.Error("turning the log off should delete it")
	}
	l.Problem("a", "two")
	if len(l.Recent(10)) != 0 {
		t.Error("logged while off")
	}
	var nilLog *Log
	nilLog.Problem("a", "no panic")
}

func TestScrubTextKeepsLines(t *testing.T) {
	l := newTest(t)
	got := l.ScrubText("a /Users/jamal/x\nb ghp_abcdefghijklmnopqrstuvwxyz0123456789\n")
	if got != "a ~/x\nb [hidden secret]\n" {
		t.Errorf("got %q", got)
	}
}
