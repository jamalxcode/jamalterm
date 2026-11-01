package main

import (
	"os"
	"os/exec"
	"path/filepath"
	goruntime "runtime"
	"strings"

	"jamalterm/internal/update"
)

// NewWindow opens another jamalterm window. Wails apps have one window, so
// each new window is its own copy of the app (it starts with a fresh tab
// in the current folder).
func (a *App) NewWindow(cwd string) error {
	if goruntime.GOOS == "darwin" {
		if app, err := update.AppBundle(); err == nil {
			// `open` returns once the new copy has launched.
			if cwd != "" && exec.Command("/usr/bin/open", "-n", "--env", "JAMALTERM_START_DIR="+cwd, app).Run() == nil {
				return nil
			}
			return exec.Command("/usr/bin/open", "-n", app).Run() // older macOS: no --env
		}
	}
	exe, err := os.Executable()
	if err != nil {
		return err
	}
	cmd := exec.Command(exe)
	cmd.Env = append(os.Environ(), "JAMALTERM_START_DIR="+cwd)
	return cmd.Start()
}

// StartDir is the folder a new window opens in ("" = home).
func (a *App) StartDir() string { return os.Getenv("JAMALTERM_START_DIR") }

// Notify shows a system notification (e.g. a long command finished while
// jamalterm was in the background).
func (a *App) Notify(title, body string) {
	switch goruntime.GOOS {
	case "darwin":
		script := `display notification "` + appleScriptQuote(body) + `" with title "` + appleScriptQuote(title) + `"`
		exec.Command("/usr/bin/osascript", "-e", script).Start()
	default:
		if p, err := exec.LookPath("notify-send"); err == nil {
			exec.Command(p, title, body).Start()
		}
	}
}

func appleScriptQuote(s string) string {
	s = strings.ReplaceAll(s, `\`, `\\`)
	s = strings.ReplaceAll(s, `"`, `\"`)
	return strings.ReplaceAll(s, "\n", " ")
}

// ResolvePath turns a path seen in the output (absolute, ~/…, or relative
// to the tab's folder) into an existing absolute path, or "".
func (a *App) ResolvePath(cwd, p string) string {
	p = strings.TrimSpace(p)
	if p == "" {
		return ""
	}
	if strings.HasPrefix(p, "~/") || p == "~" {
		home, _ := os.UserHomeDir()
		p = filepath.Join(home, strings.TrimPrefix(p, "~"))
	} else if !filepath.IsAbs(p) {
		if cwd == "" {
			return ""
		}
		p = filepath.Join(cwd, p)
	}
	if _, err := os.Stat(p); err != nil {
		return ""
	}
	return p
}

// OpenPath opens a file or folder with its default app (⌘-click in output).
func (a *App) OpenPath(p string) error {
	if _, err := os.Stat(p); err != nil {
		return err
	}
	if goruntime.GOOS == "darwin" {
		return exec.Command("/usr/bin/open", p).Start()
	}
	return exec.Command("xdg-open", p).Start()
}
