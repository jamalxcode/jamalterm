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
		a.moveToNewWindow(ctx, layout)
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

// moveToNewWindow opens a window that takes the tab.
func (a *App) moveToNewWindow(ctx context.Context, layout string) application.Window {
	a.mu.Lock()
	a.nextAdopt = layout
	a.mu.Unlock()
	dir := ""
	if src := winOf(ctx); src != nil {
		a.mu.Lock()
		dir = a.startDirs[src.Name()]
		a.mu.Unlock()
	}
	return a.openWindow(dir, "", -1)
}

// windowAt picks the window under a screen point (top-left origin, points),
// skipping the one the tab came from. "" = none.
func windowAt(frames map[string]application.Rect, skip string, x, y int) string {
	best := ""
	for name, r := range frames {
		if name == skip || r.Width == 0 {
			continue
		}
		if x >= r.X && x < r.X+r.Width && y >= r.Y && y < r.Y+r.Height {
			if best == "" || name < best {
				best = name
			}
		}
	}
	return best
}

// DropTab: a tab was dragged out of its window and let go at screen point
// (x, y). Over another jamalterm window, the tab moves there; anywhere else,
// it gets a new window at that spot.
func (a *App) DropTab(ctx context.Context, layout string, x, y int) error {
	if len(tabIDs(layout)) == 0 {
		return errors.New("nothing to move")
	}
	src := ""
	if w := winOf(ctx); w != nil {
		src = w.Name()
	}
	frames := map[string]application.Rect{}
	for _, w := range a.app.Window.GetAll() {
		if w.Name() != settingsWindowName && !w.IsMinimised() {
			frames[w.Name()] = w.Bounds()
		}
	}
	if target := windowAt(frames, src, x, y); target != "" {
		return a.MoveTab(ctx, layout, target)
	}
	if w := a.moveToNewWindow(ctx, layout); w != nil {
		w.SetPosition(max(0, x-120), max(0, y-16)) // the new window's tab under the pointer
	}
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
