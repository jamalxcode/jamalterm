package main

import (
	"context"
	"encoding/base64"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"sync"

	"github.com/wailsapp/wails/v2/pkg/runtime"

	"jamalterm/internal/ai"
	"jamalterm/internal/settings"
	"jamalterm/internal/shell"
)

// Events sent to the frontend.
const (
	eventData     = "pty:data"         // {id, data(base64)}
	eventExit     = "pty:exit"         // {id, code}
	eventMenu     = "menu"             // menu command name
	eventSettings = "settings:changed" // Settings
	eventAIDelta  = "ai:delta"         // {id, text}
	eventAIDone   = "ai:done"          // {id, text}
	eventAIError  = "ai:error"         // {id, error}
)

type ptyData struct {
	ID   int    `json:"id"`
	Data string `json:"data"`
}

type ptyExit struct {
	ID   int `json:"id"`
	Code int `json:"code"`
}

type aiEvent struct {
	ID    string `json:"id"`
	Text  string `json:"text,omitempty"`
	Error string `json:"error,omitempty"`
}

// App is the bridge between the window and the shells, settings and AI.
// Its exported methods are callable from TypeScript (frontend/wailsjs).
type App struct {
	ctx context.Context

	store   *settings.Store
	secrets *settings.Secrets
	ai      *ai.Client

	mu       sync.Mutex
	cfg      settings.Settings
	sessions map[int]*shell.Session
}

func NewApp() *App {
	store := settings.NewStore(settings.DefaultPath())
	a := &App{
		store:    store,
		secrets:  settings.NewSecrets(store.Path()),
		sessions: map[int]*shell.Session{},
	}
	a.cfg, _ = store.Load()
	a.ai = ai.New(a.secrets.APIKey, func() string { return a.Settings().AIModel })
	return a
}

func (a *App) startup(ctx context.Context) { a.ctx = ctx }

func (a *App) shutdown(ctx context.Context) {
	a.mu.Lock()
	defer a.mu.Unlock()
	for _, s := range a.sessions {
		s.Close()
	}
}

// ---- Shells (one per tab) ---------------------------------------------------

// Start launches a shell for tab id. cwd is where it starts ("" = home).
func (a *App) Start(id, cols, rows int, cwd string) error {
	a.mu.Lock()
	defer a.mu.Unlock()
	if s, ok := a.sessions[id]; ok {
		select {
		case <-s.Done():
		default:
			return nil // already running
		}
	}
	if cwd != "" {
		if fi, err := os.Stat(cwd); err != nil || !fi.IsDir() {
			cwd = ""
		}
	}
	cfg := a.cfg
	s, err := shell.Start(shell.Options{
		Shell:       cfg.Shell,
		Dir:         cwd,
		Cols:        uint16(cols),
		Rows:        uint16(rows),
		Integration: cfg.ShellIntegration,
		OnOutput: func(p []byte) {
			// Base64 keeps multi-byte characters split across reads intact.
			runtime.EventsEmit(a.ctx, eventData, ptyData{ID: id, Data: base64.StdEncoding.EncodeToString(p)})
		},
		OnExit: func(err error) {
			code := 0
			var ee *exec.ExitError
			if errors.As(err, &ee) {
				code = ee.ExitCode()
			}
			runtime.EventsEmit(a.ctx, eventExit, ptyExit{ID: id, Code: code})
		},
	})
	if err != nil {
		return err
	}
	a.sessions[id] = s
	return nil
}

// Write sends keyboard input or pasted text to tab id's shell.
func (a *App) Write(id int, data string) {
	if s := a.session(id); s != nil {
		s.Write([]byte(data))
	}
}

// Resize is called when the window or font size changes.
func (a *App) Resize(id, cols, rows int) {
	if s := a.session(id); s != nil && cols > 0 && rows > 0 {
		s.Resize(uint16(cols), uint16(rows))
	}
}

// CloseTab ends tab id's shell.
func (a *App) CloseTab(id int) {
	a.mu.Lock()
	s := a.sessions[id]
	delete(a.sessions, id)
	a.mu.Unlock()
	if s != nil {
		s.Close()
	}
}

// ShellName is the shell's name, e.g. "zsh".
func (a *App) ShellName() string {
	if sh := a.Settings().Shell; sh != "" {
		return filepath.Base(sh)
	}
	return filepath.Base(shell.DefaultShell())
}

// HomeDir lets the frontend show "~" instead of the full path.
func (a *App) HomeDir() string {
	h, _ := os.UserHomeDir()
	return h
}

func (a *App) session(id int) *shell.Session {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.sessions[id]
}

// GPUOff is true when JAMALTERM_NO_GPU=1 (draw text without WebGL).
func (a *App) GPUOff() bool { return os.Getenv("JAMALTERM_NO_GPU") == "1" }

// ---- Settings ---------------------------------------------------------------

func (a *App) Settings() settings.Settings {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.cfg
}

// SaveSettings stores new settings and tells every window.
func (a *App) SaveSettings(cfg settings.Settings) (settings.Settings, error) {
	if err := a.store.Save(cfg); err != nil {
		return a.Settings(), err
	}
	saved, err := a.store.Load()
	if err != nil {
		return a.Settings(), err
	}
	a.mu.Lock()
	a.cfg = saved
	a.mu.Unlock()
	runtime.EventsEmit(a.ctx, eventSettings, saved)
	return saved, nil
}

// HasAPIKey reports whether AI features can be used.
func (a *App) HasAPIKey() bool { return a.secrets.APIKey() != "" }

// HasSavedAPIKey reports whether a key is saved in jamalterm (vs. the env).
func (a *App) HasSavedAPIKey() bool { return a.secrets.HasSavedKey() }

// SetAPIKey saves the Claude API key in the Keychain ("" removes it).
func (a *App) SetAPIKey(key string) error { return a.secrets.SetAPIKey(key) }

// ---- AI ---------------------------------------------------------------------

// AIAsk starts a question; the answer streams back as ai:* events.
func (a *App) AIAsk(req ai.Request) {
	go func() {
		text, err := a.ai.Ask(a.ctx, req, func(delta string) {
			runtime.EventsEmit(a.ctx, eventAIDelta, aiEvent{ID: req.ID, Text: delta})
		})
		if err != nil {
			if errors.Is(err, context.Canceled) {
				err = errors.New("stopped")
			}
			runtime.EventsEmit(a.ctx, eventAIError, aiEvent{ID: req.ID, Error: err.Error()})
			return
		}
		runtime.EventsEmit(a.ctx, eventAIDone, aiEvent{ID: req.ID, Text: text})
	}()
}

// AICancel stops a streaming answer.
func (a *App) AICancel(id string) { a.ai.Cancel(id) }

// AIReset forgets a conversation (the panel's "New chat").
func (a *App) AIReset(chat string) { a.ai.Reset(chat) }

// ---- Window -----------------------------------------------------------------

// Quit closes the app (after the last tab closes).
func (a *App) Quit() { runtime.Quit(a.ctx) }

// OpenURL opens a link in the default browser.
func (a *App) OpenURL(url string) { runtime.BrowserOpenURL(a.ctx, url) }

func (a *App) menu(cmd string) func(_ *menuData) {
	return func(_ *menuData) { runtime.EventsEmit(a.ctx, eventMenu, cmd) }
}
