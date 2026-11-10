package main

import (
	"context"
	"encoding/base64"
	"errors"
	goruntime "runtime"
	"time"

	"jamalterm/internal/ai"
	"jamalterm/internal/secureinput"
	"jamalterm/internal/shell"

	"github.com/wailsapp/wails/v3/pkg/application"
	"github.com/wailsapp/wails/v3/pkg/events"
)

// ---- Don't lose running work ------------------------------------------------

// BusyTab is a tab with a program running in it (not just the shell).
type BusyTab struct {
	ID   int    `json:"id"`
	Name string `json:"name"`
}

const eventConfirm = "confirm" // {kind: "quit" | "closeWindow", busy: []BusyTab}

// BusyTabs lists which of these tabs are running something.
func (a *App) BusyTabs(ids []int) []BusyTab {
	out := []BusyTab{}
	for _, id := range ids {
		if s := a.session(id); s != nil {
			if pid, name := s.Foreground(); pid != 0 {
				if name == "" {
					name = "a program"
				}
				out = append(out, BusyTab{ID: id, Name: name})
			}
		}
	}
	return out
}

// busyWhere lists busy tabs in a window ("" = every window), leaving out
// ones you already agreed to close.
func (a *App) busyWhere(window string) []BusyTab {
	a.mu.Lock()
	var ids []int
	for id := range a.sessions {
		if a.accepted[id] {
			continue
		}
		if window == "" || a.owner[id] == window {
			ids = append(ids, id)
		}
	}
	a.mu.Unlock()
	return a.BusyTabs(ids)
}

// shouldQuit: quitting would stop running programs, so ask the window in
// front first (it calls QuitNow if you agree).
func (a *App) shouldQuit() bool {
	a.mu.Lock()
	ok := a.quitOK
	a.mu.Unlock()
	if ok || len(a.windows()) == 0 {
		return true
	}
	busy := a.busyWhere("")
	if len(busy) == 0 {
		return true
	}
	a.toFront(eventConfirm, map[string]any{"kind": "quit", "busy": busy})
	return false
}

// QuitNow quits after you agreed to stop what's running.
func (a *App) QuitNow() {
	a.mu.Lock()
	a.quitOK = true
	a.mu.Unlock()
	a.app.Quit()
}

// vetoClose: closing this window would stop running programs. Ask in
// that window instead (it calls CloseWindowNow if you agree).
func (a *App) vetoClose(w application.Window) bool {
	if a.isQuitting() {
		return false
	}
	a.mu.Lock()
	ok := a.closeOK[w.Name()]
	a.mu.Unlock()
	if ok {
		return false
	}
	busy := a.busyWhere(w.Name())
	if len(busy) == 0 {
		return false
	}
	a.emitTo(w, eventConfirm, map[string]any{"kind": "closeWindow", "busy": busy})
	return true
}

// CloseWindowNow closes the calling window after you agreed to stop
// what's running in it.
func (a *App) CloseWindowNow(ctx context.Context) {
	w := winOf(ctx)
	if w == nil {
		return
	}
	a.allowClose(w.Name())
	w.Close()
}

// allowClose: the window may close; its programs may be stopped.
func (a *App) allowClose(name string) {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.closeOK[name] = true
	for id, owner := range a.owner {
		if owner == name {
			a.accepted[id] = true
		}
	}
}

// ---- Secure Keyboard Entry ----------------------------------------------------

// SetActiveTab tells which tab in the calling window has the focus.
func (a *App) SetActiveTab(ctx context.Context, id int) {
	if w := winOf(ctx); w != nil {
		a.mu.Lock()
		a.activeTab[w.Name()] = id
		a.mu.Unlock()
	}
}

// SecureInputActive reports whether typing is protected right now.
func (a *App) SecureInputActive() bool { return secureinput.On() }

// watchSecureInput turns Secure Keyboard Entry on while jamalterm is in
// front and either the setting is on or the tab in front is asking for a
// password, and tells the window so it can show a lock.
func (a *App) watchSecureInput(ctx context.Context) {
	if goruntime.GOOS == "darwin" {
		a.app.Event.OnApplicationEvent(events.Mac.ApplicationDidBecomeActive, func(*application.ApplicationEvent) { a.setAppActive(true) })
		a.app.Event.OnApplicationEvent(events.Mac.ApplicationDidResignActive, func(*application.ApplicationEvent) { a.setAppActive(false) })
	}
	t := time.NewTicker(300 * time.Millisecond)
	defer t.Stop()
	defer secureinput.Set(false)
	lastPW := map[int]bool{}
	for {
		select {
		case <-ctx.Done():
			return
		case <-t.C:
		}
		// Password prompts in any tab: the window shows a lock on it.
		a.mu.Lock()
		ids := make(map[int]string, len(a.sessions))
		for id := range a.sessions {
			ids[id] = a.owner[id]
		}
		active := a.appActive
		a.mu.Unlock()
		front := ""
		if w := a.app.Window.Current(); w != nil {
			front = w.Name()
		}
		a.mu.Lock()
		frontTab := a.activeTab[front]
		a.mu.Unlock()
		frontPW := false
		for id, owner := range ids {
			s := a.session(id)
			pw := s != nil && s.PasswordMode()
			if id == frontTab {
				frontPW = pw
			}
			if pw != lastPW[id] {
				lastPW[id] = pw
				a.emitTo(a.window(owner), "secure:password", map[string]any{"id": id, "on": pw})
			}
		}
		for id := range lastPW {
			if _, ok := ids[id]; !ok {
				delete(lastPW, id)
			}
		}
		want := active && (a.Settings().SecureInput || frontPW)
		if want != secureinput.On() {
			secureinput.Set(want)
			a.emitTo(nil, "secure:input", want)
		}
	}
}

func (a *App) setAppActive(on bool) {
	a.mu.Lock()
	a.appActive = on
	a.mu.Unlock()
}

// ---- A reloaded window gets its tabs back ---------------------------------------

// LiveTabs lists the calling window's tabs whose shells are still running
// (after the window reloaded, e.g. View → Reload Window).
func (a *App) LiveTabs(ctx context.Context) []int {
	w := winOf(ctx)
	out := []int{}
	if w == nil {
		return out
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	for id, owner := range a.owner {
		if owner != w.Name() {
			continue
		}
		if s := a.sessions[id]; s != nil {
			select {
			case <-s.Done():
			default:
				out = append(out, id)
			}
		}
	}
	return out
}

// Attach reconnects a reloaded window to a running tab: it returns the
// recent output (base64) and output carries on from there.
func (a *App) Attach(ctx context.Context, id int) (string, error) {
	a.mu.Lock()
	p := a.pumps[id]
	if w := winOf(ctx); w != nil && p != nil {
		a.owner[id] = w.Name()
	}
	a.mu.Unlock()
	if p == nil {
		return "", errors.New("that tab's shell has ended")
	}
	return base64.StdEncoding.EncodeToString(p.Recent()), nil
}

// reloadOnCrash reloads a window whose page crashed; its tabs reattach.
func (a *App) reloadOnCrash(w *application.WebviewWindow) {
	w.OnWindowEvent(events.Mac.WebViewWebContentProcessDidTerminate, func(*application.WindowEvent) {
		a.log.Problem("window", "the page stopped unexpectedly; reloading (tabs keep running)")
		w.Reload()
	})
}

// RemoteSetupLine is the line that turns on browse mode in a bash or zsh
// on another machine (Shell → Enable Browse Mode on This Server).
func (a *App) RemoteSetupLine() string { return shell.RemoteSetup() }

// LocalServer is a model server found on this Mac.
type LocalServer struct {
	Name   string   `json:"name"`
	URL    string   `json:"url"`
	Models []string `json:"models"`
}

// FindLocalServers looks for Ollama and LM Studio running on this Mac.
func (a *App) FindLocalServers() []LocalServer {
	out := []LocalServer{}
	for _, s := range ai.LocalServers {
		ctx, cancel := context.WithTimeout(a.ctx, 2*time.Second)
		models, err := ai.ListLocalModels(ctx, s.URL)
		cancel()
		if err == nil {
			out = append(out, LocalServer{Name: s.Name, URL: s.URL, Models: models})
		}
	}
	return out
}

// KeychainKeysWaiting: an older version left keys in the Keychain (Settings
// offers to move them; checking never shows a password prompt).
func (a *App) KeychainKeysWaiting() bool { return a.secrets.KeychainKeysWaiting() }

// MoveKeysFromKeychain moves those keys into jamalterm's own encrypted
// file and deletes the Keychain item. macOS asks one last time.
func (a *App) MoveKeysFromKeychain() (int, error) {
	n, err := a.secrets.MoveFromKeychain()
	return n, a.problem("keys", err)
}

// KeysUnreadable: saved keys couldn't be unlocked on this computer (the
// folder was copied from another Mac); they need entering again.
func (a *App) KeysUnreadable() bool { return a.secrets.Unreadable() }

// Confirm shows a native macOS alert, as a sheet on the calling window, and
// reports whether you chose the action. Cancel is the default button (⏎ and
// Esc), as Apple suggests when the action can't be undone.
func (a *App) Confirm(ctx context.Context, title, message, action string) bool {
	ch := make(chan bool, 1)
	d := a.app.Dialog.Warning().SetTitle(title).SetMessage(message)
	d.AddButton(action).OnClick(func() { ch <- true })
	cancel := d.AddButton("Cancel").OnClick(func() { ch <- false })
	d.SetDefaultButton(cancel)
	d.SetCancelButton(cancel)
	if w := winOf(ctx); w != nil {
		d.AttachToWindow(w)
	}
	d.Show()
	select {
	case v := <-ch:
		return v
	case <-ctx.Done():
		return false
	}
}

// CommandRisk checks a command before you run it from an AI answer (the
// Run buttons on code blocks): risky ones need a second confirmation.
// shown is the terminal text the AI was given.
func (a *App) CommandRisk(command, shown string) ai.Risk { return ai.CommandRisk(command, shown) }
