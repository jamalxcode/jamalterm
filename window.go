package main

import (
	"os"
	"os/exec"
	"path/filepath"
	goruntime "runtime"
	"strings"
)

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
