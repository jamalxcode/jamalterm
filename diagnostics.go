package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	goruntime "runtime"
	"strings"

	"jamalterm/internal/ai"
)

// problem records err in the private problem log and returns it unchanged.
func (a *App) problem(area string, err error) error {
	if err != nil {
		a.log.Problem(area, "%v", err)
	}
	return err
}

// LogProblem records a problem seen by the window (a script error). Only
// the error message is kept, scrubbed like everything else.
func (a *App) LogProblem(area, message string) {
	a.log.Problem("window "+area, "%s", message)
}

// shellLabel is the shell's name without its folder ("zsh").
func shellLabel(path string) string {
	if path == "" {
		path = os.Getenv("SHELL")
	}
	if path == "" {
		return "the shell"
	}
	return filepath.Base(path)
}

// Diagnostics is the report shown by Help → Copy Diagnostics. The window
// shows it in full before anything is copied, and it is never sent
// anywhere by jamalterm. It holds the version, system, settings (no keys,
// no folders) and the recent problem log, all scrubbed.
func (a *App) Diagnostics() string {
	cfg := a.Settings()
	var b strings.Builder
	line := func(format string, args ...any) { fmt.Fprintf(&b, format+"\n", args...) }

	line("jamalterm diagnostics")
	line("(made on this computer and not sent anywhere; read it before sharing)")
	line("")
	line("Version:   %s", version)
	line("System:    %s (%s)", osVersion(), goruntime.GOARCH)
	line("Shell:     %s, integration %s", shellLabel(cfg.Shell), onOff(cfg.ShellIntegration))
	line("Window:    opacity %d%%, blur %s, GPU drawing %s", cfg.Opacity, onOff(a.blurAtLaunch), onOff(!a.GPUOff()))
	line("Tabs open: %d", a.openTabs())

	provider := cfg.AIProvider
	name := provider
	for _, p := range ai.Providers {
		if p.ID == provider {
			name = p.Name
		}
	}
	keyState := "no key"
	if a.HasAPIKey() {
		keyState = "ready (key or sign-in found; not shown)"
	}
	line("AI:        %s, %s, %s", onOff(cfg.AIEnabled), name, keyState)
	line("")
	line("Settings:")
	for _, kv := range [][2]any{
		{"theme", cfg.Theme}, {"font size", cfg.FontSize}, {"line height", cfg.LineHeight},
		{"cursor", cfg.CursorStyle}, {"cursor blink", cfg.CursorBlink}, {"scrollback", cfg.Scrollback},
		{"custom shell", cfg.Shell != ""}, {"↑ browses", cfg.ArrowBrowse}, {"restore tabs", cfg.RestoreSession},
		{"prompt keys", cfg.PromptKeys}, {"notify", cfg.NotifyLongCommands}, {"screen reader", cfg.ScreenReader},
		{"confirm re-run", cfg.ConfirmRerun}, {"option is meta", cfg.OptionIsMeta},
		{"auto update", cfg.AutoUpdate}, {"share output with AI", cfg.AIShareOutput},
	} {
		line("  %-22s %v", kv[0], kv[1])
	}
	if a.lastCrash != "" {
		line("")
		line("Last crash (scrubbed):")
		for _, l := range strings.Split(a.lastCrash, "\n") {
			line("  %s", l)
		}
	}
	line("")
	if !a.log.Enabled() {
		line("Problem log: off (Settings → Privacy)")
		return a.log.ScrubText(b.String())
	}
	recent := a.log.Recent(40)
	if len(recent) == 0 {
		line("Recent problems: none")
	} else {
		line("Recent problems (oldest first, times in UTC):")
		for _, r := range recent {
			line("  %s", r)
		}
	}
	return a.log.ScrubText(b.String())
}

func (a *App) openTabs() int {
	a.mu.Lock()
	defer a.mu.Unlock()
	n := 0
	for _, s := range a.sessions {
		select {
		case <-s.Done():
		default:
			n++
		}
	}
	return n
}

func onOff(b bool) string {
	if b {
		return "on"
	}
	return "off"
}

func osVersion() string {
	switch goruntime.GOOS {
	case "darwin":
		if out, err := exec.Command("/usr/bin/sw_vers", "-productVersion").Output(); err == nil {
			return "macOS " + strings.TrimSpace(string(out))
		}
		return "macOS"
	default:
		if out, err := exec.Command("uname", "-sr").Output(); err == nil {
			return strings.TrimSpace(string(out))
		}
		return goruntime.GOOS
	}
}

// ProblemLogPath is where the problem log is kept.
func (a *App) ProblemLogPath() string { return a.log.Path() }

// ShowProblemLog reveals the log in Finder (or opens its folder).
func (a *App) ShowProblemLog() {
	p := a.log.Path()
	if _, err := os.Stat(p); err != nil {
		p = filepath.Dir(p)
		os.MkdirAll(p, 0o700)
	}
	if goruntime.GOOS == "darwin" {
		exec.Command("/usr/bin/open", "-R", p).Start()
		return
	}
	if fi, err := os.Stat(p); err == nil && !fi.IsDir() {
		p = filepath.Dir(p)
	}
	if x, err := exec.LookPath("xdg-open"); err == nil {
		exec.Command(x, p).Start()
	}
}
