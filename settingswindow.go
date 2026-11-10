package main

import (
	"context"

	"github.com/wailsapp/wails/v3/pkg/application"
	"github.com/wailsapp/wails/v3/pkg/events"
)

// The Settings window (⌘,). Like Terminal and Safari it is its own window
// with a toolbar of panes, and every change applies at once (no Save
// button). It isn't a terminal window: it's not saved, counted or asked
// about when quitting.

const settingsWindowName = "settings"

// OpenSettings shows the Settings window (or brings it to the front).
// page optionally picks a pane, find highlights a setting.
func (a *App) OpenSettings() { a.OpenSettingsAt("", "") }

// OpenSettingsAt opens Settings on a pane ("appearance", "ai"…), with the
// setting whose label contains find highlighted.
func (a *App) OpenSettingsAt(page, find string) {
	if w, ok := a.app.Window.GetByName(settingsWindowName); ok {
		w.Show()
		w.Focus()
		if page != "" {
			a.emitTo(w, "settings:show", map[string]string{"page": page, "find": find})
		}
		return
	}
	url := "/settings.html"
	if page != "" {
		url += "#" + page + ":" + find
	}
	w := a.app.Window.NewWithOptions(application.WebviewWindowOptions{
		Name:                settingsWindowName,
		Title:               "Settings",
		Width:               720,
		Height:              620,
		MinWidth:            620,
		MinHeight:           420,
		URL:                 url,
		BackgroundColour:    application.NewRGBA(236, 236, 236, 255),
		MinimiseButtonState: application.ButtonDisabled,
		MaximiseButtonState: application.ButtonDisabled,
		Mac: application.MacWindow{
			// A standard title bar: the window title names the pane.
			TitleBar: application.MacTitleBarDefault,
		},
	})
	_ = w
}

// CloseSettings closes the Settings window (Esc, ⌘W).
func (a *App) CloseSettings() {
	if w, ok := a.app.Window.GetByName(settingsWindowName); ok {
		w.Close()
	}
}

// IsSettingsWindow: the calling window is Settings.
func (a *App) IsSettingsWindow(ctx context.Context) bool {
	w := winOf(ctx)
	return w != nil && w.Name() == settingsWindowName
}

// ToTerminal sends a menu command to the terminal window last used (for
// Settings' "Sign in", "Check now", …).
func (a *App) ToTerminal(command string) {
	if w := a.lastTerminal(); w != nil {
		w.Focus()
		a.emitTo(w, eventMenu, command)
	}
}

// lastTerminal is the terminal window used most recently.
func (a *App) lastTerminal() application.Window {
	a.mu.Lock()
	name := a.lastTermName
	a.mu.Unlock()
	if w := a.window(name); w != nil {
		return w
	}
	if all := a.windows(); len(all) > 0 {
		return all[len(all)-1]
	}
	return nil
}

// trackFocus remembers the last terminal window to have the focus.
func (a *App) trackFocus(w *application.WebviewWindow, name string) {
	w.OnWindowEvent(events.Common.WindowFocus, func(*application.WindowEvent) {
		a.mu.Lock()
		a.lastTermName = name
		a.mu.Unlock()
	})
}

// SettingsOpen reports whether the Settings window is open (smoke test).
func (a *App) SettingsOpen() bool {
	_, ok := a.app.Window.GetByName(settingsWindowName)
	return ok
}
