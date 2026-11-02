package main

// The self-test run by CI on a real Mac (JAMALTERM_SMOKE=<folder>): the
// window drives itself through the main features (frontend/src/smoke.ts),
// screenshots go into the folder, and report.json says what passed.

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	goruntime "runtime"
	"strings"
	"time"
)

func smokeDir() string { return os.Getenv("JAMALTERM_SMOKE") }

// SmokeMode: the calling window should run the self-test (only the first
// window does; the test opens a second one).
func (a *App) SmokeMode(ctx context.Context) bool {
	w := winOf(ctx)
	return smokeDir() != "" && w != nil && winNum(w.Name()) == 1
}

// SmokeShot saves a screenshot of the screen as <name>.png.
func (a *App) SmokeShot(name string) string {
	dir := smokeDir()
	if dir == "" {
		return "not in smoke mode"
	}
	out := filepath.Join(dir, filepath.Base(name)+".png")
	var cmd *exec.Cmd
	if goruntime.GOOS == "darwin" {
		cmd = exec.Command("/usr/sbin/screencapture", "-x", out)
	} else if p, err := exec.LookPath("import"); err == nil {
		cmd = exec.Command(p, "-window", "root", out)
	} else {
		return "no screenshot tool"
	}
	if b, err := cmd.CombinedOutput(); err != nil {
		return strings.TrimSpace(fmt.Sprintf("%v %s", err, b))
	}
	return ""
}

// SmokeWaitKeys tells CI the window is ready for real key presses (it
// writes ready-for-keys), then waits until CI writes keys-done. False if
// CI didn't send any (e.g. no permission to press keys).
func (a *App) SmokeWaitKeys(ctx context.Context) bool {
	dir := smokeDir()
	if dir == "" {
		return false
	}
	os.WriteFile(filepath.Join(dir, "ready-for-keys"), nil, 0o644)
	deadline := time.Now().Add(45 * time.Second)
	for time.Now().Before(deadline) {
		if b, err := os.ReadFile(filepath.Join(dir, "keys-done")); err == nil {
			return strings.TrimSpace(string(b)) == "ok"
		}
		select {
		case <-ctx.Done():
			return false
		case <-time.After(200 * time.Millisecond):
		}
	}
	return false
}

// WindowCount is how many jamalterm windows are open.
func (a *App) WindowCount() int { return len(a.windows()) }

// SmokeReport saves the window's results with a few facts from Go, then quits.
func (a *App) SmokeReport(report string) error {
	dir := smokeDir()
	if dir == "" {
		return nil
	}
	var r map[string]any
	if err := json.Unmarshal([]byte(report), &r); err != nil {
		return err
	}
	r["version"] = version
	r["windows"] = len(a.windows())
	r["goSeconds"] = time.Since(startedAt).Seconds()
	data, _ := json.MarshalIndent(r, "", "  ")
	if err := os.WriteFile(filepath.Join(dir, "report.json"), data, 0o644); err != nil {
		return err
	}
	go func() {
		time.Sleep(300 * time.Millisecond)
		a.QuitNow()
	}()
	return nil
}

var startedAt = time.Now()
