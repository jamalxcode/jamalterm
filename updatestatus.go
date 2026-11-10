package main

import (
	"context"
	_ "embed"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"time"

	"jamalterm/internal/update"
)

// What the app knows about updates, so you can always see it (Settings →
// Updates): when it last checked, and what it found.

//go:embed CHANGELOG.md
var changelog string

// UpdateStatus is the result of the latest check.
type UpdateStatus struct {
	Current   string `json:"current"`
	CheckedAt int64  `json:"checkedAt"` // Unix ms, 0 = never
	Latest    string `json:"latest"`
	Available bool   `json:"available"`
	Error     string `json:"error"`
	Channel   string `json:"channel"`
}

func (a *App) statusFile() string { return filepath.Join(a.configDir(), "update-status.json") }

// UpdateStatus returns what the last check found.
func (a *App) UpdateStatus() UpdateStatus {
	var s UpdateStatus
	if b, err := os.ReadFile(a.statusFile()); err == nil {
		json.Unmarshal(b, &s)
	}
	s.Current = version
	s.Channel = a.Settings().UpdateChannel
	// A check from before an update may name the version you now have.
	if s.Available && !update.Newer(s.Latest, version) {
		s.Available = false
	}
	return s
}

func (a *App) saveUpdateStatus(info UpdateInfo, err error) {
	s := UpdateStatus{CheckedAt: time.Now().UnixMilli(), Latest: info.Latest, Available: info.Available}
	if err != nil {
		s.Error = err.Error()
	}
	b, _ := json.Marshal(s)
	os.MkdirAll(a.configDir(), 0o700)
	os.WriteFile(a.statusFile(), b, 0o600)
}

// Notice shows a native information alert on the calling window (for
// "You're up to date").
func (a *App) Notice(ctx context.Context, title, message string) {
	d := a.app.Dialog.Info().SetTitle(title).SetMessage(message)
	d.AddButton("OK").SetAsDefault()
	if w := winOf(ctx); w != nil {
		d.AttachToWindow(w)
	}
	d.Show()
}

// WhatsNew is the CHANGELOG for the versions after since, up to this one
// (shown once after an update). "" if there's nothing to show.
func (a *App) WhatsNew(since string) string {
	if since == "" || !update.Newer(version, since) {
		return ""
	}
	var out []string
	keep := false
	for _, line := range strings.Split(changelog, "\n") {
		if v, ok := strings.CutPrefix(line, "## "); ok {
			v = strings.TrimSpace(v)
			keep = update.Compare(v, since) > 0 && update.Compare(v, version) <= 0
		}
		if keep {
			out = append(out, line)
		}
	}
	return strings.TrimSpace(strings.Join(out, "\n"))
}
