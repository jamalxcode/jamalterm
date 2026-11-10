package main

import (
	"os"
	"path/filepath"
	"runtime/debug"
	"strings"
	"time"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// Crash reports stay on this Mac, like the problem log. If jamalterm's Go
// side dies, Go writes its last words to crash.log; on the next launch the
// window says so and offers Copy Diagnostics. Nothing is ever sent.

func (a *App) crashFile() string { return filepath.Join(a.configDir(), "crash.log") }

// watchCrashes reads a crash from last time (if any), then points Go's
// crash output at the file for this run.
func (a *App) watchCrashes() {
	if data, err := os.ReadFile(a.crashFile()); err == nil && strings.TrimSpace(string(data)) != "" {
		a.lastCrash = crashSummary(a.log.ScrubText(string(data)))
		first, _, _ := strings.Cut(a.lastCrash, "\n")
		a.log.Problem("crash", "jamalterm quit unexpectedly last time: %s", first)
	}
	os.MkdirAll(a.configDir(), 0o700)
	f, err := os.OpenFile(a.crashFile(), os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o600)
	if err != nil {
		return
	}
	debug.SetCrashOutput(f, debug.CrashOptions{}) // keeps its own copy of f
	f.Close()
}

// crashSummary is the panic message and the first frames of the crashing
// goroutine: enough to fix it, short enough to read.
func crashSummary(trace string) string {
	lines := strings.Split(strings.TrimSpace(trace), "\n")
	var out []string
	for _, l := range lines {
		if len(out) > 0 && strings.HasPrefix(l, "goroutine ") && len(out) > 3 {
			break // only the crashing goroutine
		}
		out = append(out, l)
		if len(out) >= 40 {
			break
		}
	}
	return strings.Join(out, "\n")
}

// onPanic: a panic Wails caught (in a call from the window). jamalterm
// keeps running; the problem log notes it.
func (a *App) onPanic(p *application.PanicDetails) {
	msg := "unknown"
	if p.Error != nil {
		msg = p.Error.Error()
	}
	first := crashSummary(a.log.ScrubText(p.StackTrace))
	a.log.Problem("panic", "%s | %s", msg, strings.ReplaceAll(first, "\n", " | "))
}

// testCrash: JAMALTERM_TEST_CRASH=1 crashes on purpose a few seconds in
// (to check that the next launch notices).
func testCrash() {
	if os.Getenv("JAMALTERM_TEST_CRASH") == "1" {
		go func() {
			time.Sleep(4 * time.Second)
			var m map[string]int
			m["boom"] = 1 // a nil-map write: a real runtime crash
		}()
	}
}

// LastCrash is a scrubbed summary of how jamalterm quit last time ("" if
// it didn't crash). The window offers to copy it with the diagnostics.
func (a *App) LastCrash() string { return a.lastCrash }

// DismissCrash forgets the crash from last time.
func (a *App) DismissCrash() { a.lastCrash = "" }
