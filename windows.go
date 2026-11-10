package main

import (
	"jamalterm/internal/winstate"

	"context"
	"os"
	"sort"
	"strings"

	"github.com/wailsapp/wails/v3/pkg/application"
	"github.com/wailsapp/wails/v3/pkg/events"
)

// ---- Windows ------------------------------------------------------------------
//
// Every window runs the same page. Tabs belong to the window that started
// them: their output, AI answers and update progress go only there. Menu
// commands go to the window in front.

// winOf is the window that made a call ("" context: none).
func winOf(ctx context.Context) application.Window {
	if ctx == nil {
		return nil
	}
	w, _ := ctx.Value(application.WindowKey).(application.Window)
	return w
}

// emitTo sends an event to one window, or to all when w is nil.
func (a *App) emitTo(w application.Window, name string, data any) {
	if a.app == nil {
		return
	}
	if w == nil {
		a.app.Event.Emit(name, data)
		return
	}
	w.DispatchWailsEvent(&application.CustomEvent{Name: name, Data: data})
}

// toFront sends an event to the window in front (menu commands). While
// Settings is in front, ⌘W closes it and other commands go to the terminal
// window used last.
func (a *App) toFront(name string, data any) {
	w := a.app.Window.Current()
	if w != nil && w.Name() == settingsWindowName {
		if name == eventMenu && (data == "closeTab" || data == "closeWindow" || data == "closeGroup") {
			w.Close()
			return
		}
		if name == eventMenu && (data == "selectAll" || data == "find") {
			a.emitTo(w, name, data) // Settings handles these itself
			return
		}
		w = a.lastTerminal()
		if w != nil {
			w.Focus()
		}
	}
	if w == nil {
		if all := a.windows(); len(all) > 0 {
			w = all[0]
		}
	}
	if w == nil {
		if name == eventMenu && (data == "newWindow" || data == "newTab") {
			a.openWindow("", "", 0)
		}
		return
	}
	a.emitTo(w, name, data)
}

// windows lists jamalterm's windows, oldest first.
func (a *App) windows() []application.Window {
	var out []application.Window
	for _, w := range a.app.Window.GetAll() {
		if strings.HasPrefix(w.Name(), "jt-") {
			out = append(out, w)
		}
	}
	sort.Slice(out, func(i, j int) bool { return winNum(out[i].Name()) < winNum(out[j].Name()) })
	return out
}

func winNum(name string) int { return winstate.Num(name) }

// cycleWindows brings the next window to the front (⌘`).
func (a *App) cycleWindows() {
	all := a.windows()
	if len(all) < 2 {
		return
	}
	cur := a.app.Window.Current()
	for i, w := range all {
		if cur != nil && w.Name() == cur.Name() {
			all[(i+1)%len(all)].Focus()
			return
		}
	}
	all[0].Focus()
}

// openWindow opens a window. startDir is where its first tab starts;
// restore is the saved tabs it reopens ("" = a fresh tab). cascade > 0
// places the n-th reopened window; 0 places it next to the one in front;
// -1 centres it.
func (a *App) openWindow(startDir, restore string, cascade int) application.Window {
	a.mu.Lock()
	a.nextWin++
	name := winstate.Name(a.nextWin)
	a.startDirs[name] = startDir
	if restore != "" {
		a.restoreFor[name] = restore
	}
	if a.nextAdopt != "" {
		// A tab moving here: its shells belong to this window from now on.
		a.adoptFor[name] = a.nextAdopt
		for _, id := range tabIDs(a.nextAdopt) {
			a.owner[id] = name
		}
		a.nextAdopt = ""
	}
	a.mu.Unlock()

	cfg := a.Settings()
	bg := application.NewRGBA(21, 22, 30, 255)
	if cfg.Opacity < 100 {
		bg = application.NewRGBA(0, 0, 0, 0)
	}
	// The page draws its own background, so the webview is always
	// see-through; the window colour decides how solid it looks.
	backdrop := application.MacBackdropTransparent
	if a.blurAtLaunch {
		backdrop = application.MacBackdropTranslucent
	}
	bgType := application.BackgroundTypeSolid
	if cfg.Opacity < 100 {
		bgType = application.BackgroundTypeTranslucent
	}
	opts := application.WebviewWindowOptions{
		Name:             name,
		Title:            "jamalterm",
		Width:            1100,
		Height:           700,
		MinWidth:         480,
		MinHeight:        300,
		URL:              "/",
		BackgroundColour: bg,
		BackgroundType:   bgType,
		// Files dropped on the window are typed at the prompt as paths.
		EnableFileDrop: true,
		Mac: application.MacWindow{
			// Tabs sit in a transparent, standard-height title bar, centred
			// on the window buttons (style.css: body.mac --titlebar).
			TitleBar: application.MacTitleBarHidden,
			Backdrop: backdrop,
		},
	}
	// A reopened window goes back where it was, at its size. A new window
	// steps down and right from the one in front.
	if f, ok := frameOf(restore); ok {
		opts.InitialPosition = application.WindowXY
		opts.X, opts.Y, opts.Width, opts.Height = f.X, f.Y, f.W, f.H
	} else if cascade > 0 {
		opts.InitialPosition = application.WindowXY
		opts.X, opts.Y = 40+28*cascade, 30+28*cascade
	} else if cur := a.app.Window.Current(); cur != nil && cascade == 0 {
		x, y := cur.RelativePosition()
		opts.InitialPosition = application.WindowXY
		opts.X, opts.Y = x+28, y+28
	}
	w := a.app.Window.NewWithOptions(opts)
	w.OnWindowEvent(events.Common.WindowFilesDropped, func(e *application.WindowEvent) {
		if files := e.Context().DroppedFiles(); len(files) > 0 {
			a.emitTo(w, eventFilesDropped, files)
		}
	})
	w.RegisterHook(events.Common.WindowClosing, func(e *application.WindowEvent) {
		if a.vetoClose(w) {
			e.Cancel()
			return
		}
		a.windowClosing(name)
	})
	a.reloadOnCrash(w)
	a.trackFocus(w, name)
	return w
}

// windowClosing: a window is going away. When you close a window its tabs
// end and it's forgotten; when jamalterm quits, every window is kept so it
// comes back next time.
func (a *App) windowClosing(name string) {
	// Closing the last window quits jamalterm: keep it like a quit, so
	// its tabs come back. (Closing its last tab saved an empty window
	// first, so that starts fresh.)
	if a.isQuitting() || len(a.windows()) <= 1 {
		return
	}
	a.mu.Lock()
	var ids []int
	for id, w := range a.owner {
		if w == name {
			ids = append(ids, id)
		}
	}
	a.saved.Drop(name)
	delete(a.startDirs, name)
	delete(a.restoreFor, name)
	delete(a.adoptFor, name)
	delete(a.winTabs, name)
	a.mu.Unlock()
	for _, id := range ids {
		a.CloseTab(id)
	}
	a.writeSessions()
}

func (a *App) setQuitting() {
	a.mu.Lock()
	a.quitting = true
	a.mu.Unlock()
}

func (a *App) isQuitting() bool {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.quitting || (a.app != nil && a.app.Context().Err() != nil)
}

// openStartWindows reopens the windows from last time (or one new window).
func (a *App) openStartWindows() {
	saved := a.loadSessions()
	if len(saved) == 0 {
		a.openWindow(os.Getenv("JAMALTERM_START_DIR"), "", -1)
		return
	}
	if len(saved) == 1 {
		a.openWindow("", saved[0], -1)
		return
	}
	for i, s := range saved {
		a.openWindow("", s, i+1)
	}
}

// WindowID is a number unique to the calling window, so tab numbers from
// different windows never collide.
func (a *App) WindowID(ctx context.Context) int {
	if w := winOf(ctx); w != nil {
		return winNum(w.Name())
	}
	return 0
}

// NewWindow opens another window whose first tab starts in cwd.
func (a *App) NewWindow(cwd string) error {
	a.openWindow(cwd, "", 0)
	return nil
}

// StartDir is the folder the calling window's first tab opens in ("" = home).
func (a *App) StartDir(ctx context.Context) string {
	w := winOf(ctx)
	if w == nil {
		return ""
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.startDirs[w.Name()]
}

// CloseWindow closes the calling window (its last tab was closed).
// Closing the last window quits.
func (a *App) CloseWindow(ctx context.Context) {
	if w := winOf(ctx); w != nil {
		a.allowClose(w.Name()) // the window already asked about each tab
		w.Close()
	}
}

// ---- Session: every window's tabs, restored after a restart -----------------

func (a *App) loadSessions() []string {
	if !a.primary || !a.Settings().RestoreSession {
		return nil
	}
	data, err := os.ReadFile(a.sessionFile())
	if err != nil {
		return nil
	}
	return winstate.Decode(data)
}

// IsPrimaryWindow: this copy of jamalterm restores and saves tabs (a second
// copy started by hand doesn't).
func (a *App) IsPrimaryWindow() bool { return a.primary }

// SaveSession stores the calling window's tabs (JSON from the window).
// The file can contain terminal output, so it's 0600.
func (a *App) SaveSession(ctx context.Context, data string) error {
	w := winOf(ctx)
	if !a.primary || w == nil {
		return nil
	}
	if err := a.saved.Set(w.Name(), withFrame(data, w)); err != nil {
		return a.problem("session", err)
	}
	return a.writeSessions()
}

func (a *App) writeSessions() error {
	if !a.primary {
		return nil
	}
	data := a.saved.Encode()
	tmp := a.sessionFile() + ".tmp"
	if err := os.WriteFile(tmp, data, 0o600); err != nil {
		return a.problem("session", err)
	}
	return a.problem("session", os.Rename(tmp, a.sessionFile()))
}

// LoadSession returns the tabs the calling window should reopen ("" = none).
func (a *App) LoadSession(ctx context.Context) string {
	w := winOf(ctx)
	if w == nil {
		return ""
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	s, ok := a.restoreFor[w.Name()]
	delete(a.restoreFor, w.Name())
	if !ok {
		// A reloaded window: its latest layout (its tabs reattach).
		s = a.saved.Get(w.Name())
	}
	return s
}

// ForgetSession deletes saved tabs and output (Settings → Privacy).
func (a *App) ForgetSession() {
	a.saved.Clear()
	os.Remove(a.sessionFile())
}
