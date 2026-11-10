package main

import (
	"context"
	"encoding/json"
	"errors"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// Tabs move between windows with their shells still running: the shells
// change owner, and the other window reconnects to them (as after a reload)
// from the tab's saved layout.

const eventAdopt = "tab:adopt" // a tab moved into this window (layout JSON)

// tabIDs lists the tab ids in a saved layout ({"tab":{"id":3}} or a split
// {"a":…,"b":…}).
func tabIDs(layout string) []int {
	var walk func(n map[string]json.RawMessage) []int
	walk = func(n map[string]json.RawMessage) []int {
		if raw, ok := n["tab"]; ok {
			var t struct {
				ID *int `json:"id"`
			}
			if json.Unmarshal(raw, &t) == nil && t.ID != nil {
				return []int{*t.ID}
			}
			return nil
		}
		var out []int
		for _, k := range []string{"a", "b"} {
			var child map[string]json.RawMessage
			if json.Unmarshal(n[k], &child) == nil {
				out = append(out, walk(child)...)
			}
		}
		return out
	}
	var root map[string]json.RawMessage
	if json.Unmarshal([]byte(layout), &root) != nil {
		return nil
	}
	return walk(root)
}

// tabWindow is the window tab id is in now (fallback: the one it started in).
func (a *App) tabWindow(id int, fallback application.Window) application.Window {
	a.mu.Lock()
	name := a.owner[id]
	a.mu.Unlock()
	if name == "" || a.app == nil || (fallback != nil && fallback.Name() == name) {
		return fallback
	}
	if w, ok := a.app.Window.GetByName(name); ok {
		return w
	}
	return fallback
}

// MoveTab hands a tab (layout JSON, its shells keep running) to another
// window: target is a window name, or "" for a new window. The calling
// window removes the tab itself (without ending its shells).
func (a *App) MoveTab(ctx context.Context, layout, target string) error {
	ids := tabIDs(layout)
	if len(ids) == 0 {
		return errors.New("nothing to move")
	}
	if target == "" {
		a.mu.Lock()
		a.nextAdopt = layout
		a.mu.Unlock()
		dir := ""
		if src := winOf(ctx); src != nil {
			a.mu.Lock()
			dir = a.startDirs[src.Name()]
			a.mu.Unlock()
		}
		a.openWindow(dir, "", -1)
		return nil
	}
	w, ok := a.app.Window.GetByName(target)
	if !ok {
		return errors.New("that window is gone")
	}
	a.mu.Lock()
	for _, id := range ids {
		a.owner[id] = target
	}
	a.mu.Unlock()
	a.emitTo(w, eventAdopt, layout)
	w.Show()
	w.Focus()
	return nil
}

// Adopted returns the tab this (new) window was opened to take, once.
func (a *App) Adopted(ctx context.Context) string {
	w := winOf(ctx)
	if w == nil {
		return ""
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	s := a.adoptFor[w.Name()]
	delete(a.adoptFor, w.Name())
	return s
}

// WindowName names the calling window (tabs dragged between windows say
// where they came from).
func (a *App) WindowName(ctx context.Context) string {
	if w := winOf(ctx); w != nil {
		return w.Name()
	}
	return ""
}

// TakeTab moves a tab that was dragged here from another window: the tab's
// shells now belong to the calling window, and the window it came from is
// told to let it go.
func (a *App) TakeTab(ctx context.Context, layout, from string) error {
	w := winOf(ctx)
	ids := tabIDs(layout)
	if w == nil || len(ids) == 0 || from == w.Name() {
		return errors.New("nothing to move")
	}
	a.mu.Lock()
	for _, id := range ids {
		if a.owner[id] != from {
			a.mu.Unlock()
			return errors.New("that tab has already moved")
		}
		a.owner[id] = w.Name()
	}
	a.mu.Unlock()
	if src, ok := a.app.Window.GetByName(from); ok {
		a.emitTo(src, eventTabGone, ids)
	}
	return nil
}

const eventTabGone = "tab:gone" // a tab of this window was taken by another

// MergeAllWindows moves every other window's tabs into the window in front,
// like Terminal's Window → Merge All Windows.
func (a *App) MergeAllWindows(ctx context.Context) {
	w := winOf(ctx)
	if w == nil {
		return
	}
	for _, other := range a.app.Window.GetAll() {
		if other.Name() == w.Name() || other.Name() == settingsWindowName {
			continue
		}
		a.emitTo(other, eventSendTabs, w.Name())
	}
}

const eventSendTabs = "tabs:send" // move all your tabs to this window
