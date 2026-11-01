package main

import (
	"context"
	"encoding/base64"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"time"

	"github.com/wailsapp/wails/v2/pkg/runtime"

	"jamalterm/internal/ai"
	"jamalterm/internal/settings"
	"jamalterm/internal/shell"
	"jamalterm/internal/update"
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
	eventProgress = "update:progress"  // {done, total}
	eventAITool   = "ai:tool"          // {id, toolId, command, run}
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
	release  *update.Release // newest release seen by CheckForUpdate
	toolWait map[string]chan toolReply
	// blurAtLaunch: the window was created translucent (blur behind).
	blurAtLaunch bool
}

type toolReply struct {
	result  string
	isError bool
}

func NewApp() *App {
	store := settings.NewStore(settings.DefaultPath())
	a := &App{
		store:    store,
		secrets:  settings.NewSecrets(store.Path()),
		sessions: map[int]*shell.Session{},
		toolWait: map[string]chan toolReply{},
	}
	a.cfg, _ = store.Load()
	a.ai = ai.New(a.secrets.APIKey, func() string { return a.Settings().AIModel })
	a.ai.Configure(
		func() string { return a.Settings().AIProvider },
		a.secrets.KeyFor,
		func(p string) string {
			switch p {
			case ai.ProviderOpenAI:
				return a.Settings().OpenAIModel
			case ai.ProviderDeepSeek:
				return a.Settings().DeepSeekModel
			}
			return ""
		})
	return a
}

func (a *App) startup(ctx context.Context) {
	a.ctx = ctx
	a.applyWindowOpacity()
}

// applyWindowOpacity makes the window itself see-through when the
// opacity setting is below 100; the page then draws its own
// semi-transparent background.
func (a *App) applyWindowOpacity() { a.SetWindowOpacity(a.Settings().Opacity) }

// SetWindowOpacity switches the window between solid and see-through
// (live preview from Settings; nothing is saved).
func (a *App) SetWindowOpacity(percent int) {
	if percent < 100 {
		runtime.WindowSetBackgroundColour(a.ctx, 0, 0, 0, 0)
	} else {
		runtime.WindowSetBackgroundColour(a.ctx, 21, 22, 30, 255)
	}
}

// BlurActive reports whether the window was started with background blur
// (it can only change at launch).
func (a *App) BlurActive() bool { return a.blurAtLaunch }

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

// GitBranch is the git branch for a folder ("" if none), shown on tabs.
func (a *App) GitBranch(dir string) string { return shell.GitBranch(dir) }

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
	a.applyWindowOpacity()
	runtime.EventsEmit(a.ctx, eventSettings, saved)
	return saved, nil
}

// HasAPIKey reports whether AI features can be used: an API key, or a
// sign-in with an Anthropic account.
func (a *App) HasAPIKey() bool {
	if p := a.Settings().AIProvider; p != ai.ProviderAnthropic {
		return a.secrets.KeyFor(p) != ""
	}
	return a.secrets.APIKey() != "" || ai.SignedInAccount().SignedIn
}

// AIProviders lists the assistants to choose from (Claude, ChatGPT, DeepSeek).
func (a *App) AIProviders() []ai.ProviderInfo { return ai.Providers }

// HasSavedProviderKey reports whether a key for provider is saved in jamalterm.
func (a *App) HasSavedProviderKey(provider string) bool { return a.secrets.HasSavedKeyFor(provider) }

// SetProviderKey saves a provider's API key in the Keychain ("" removes it).
func (a *App) SetProviderKey(provider, key string) error { return a.secrets.SetKeyFor(provider, key) }

// ListProviderModels asks ChatGPT or DeepSeek which chat models the saved
// key can use (newest first).
func (a *App) ListProviderModels(provider string) ([]string, error) {
	ctx, cancel := context.WithTimeout(a.ctx, 15*time.Second)
	defer cancel()
	return ai.ListModels(ctx, provider, a.secrets.KeyFor(provider))
}

// AnthropicAccount reports the `ant auth login` sign-in, if any.
func (a *App) AnthropicAccount() ai.Account { return ai.SignedInAccount() }

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
		}, func(call ai.ToolCall) (string, bool) {
			return a.runTool(req.ID, call)
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

// runTool asks the window to type (or, after the user confirms, run) a
// command and waits for what happened.
func (a *App) runTool(reqID string, call ai.ToolCall) (string, bool) {
	ch := make(chan toolReply, 1)
	a.mu.Lock()
	a.toolWait[call.ID] = ch
	a.mu.Unlock()
	defer func() {
		a.mu.Lock()
		delete(a.toolWait, call.ID)
		a.mu.Unlock()
	}()
	runtime.EventsEmit(a.ctx, eventAITool, map[string]any{"id": reqID, "toolId": call.ID, "command": call.Command, "run": call.Run})
	select {
	case r := <-ch:
		return r.result, r.isError
	case <-time.After(10 * time.Minute):
		return "No answer from the user; nothing was run.", true
	case <-a.ctx.Done():
		return "jamalterm is closing; nothing was run.", true
	}
}

// AIToolResult is the window's answer to an ai:tool event.
func (a *App) AIToolResult(toolID, result string, isError bool) {
	a.mu.Lock()
	ch := a.toolWait[toolID]
	a.mu.Unlock()
	if ch != nil {
		select {
		case ch <- toolReply{result, isError}:
		default:
		}
	}
}

// AICancel stops a streaming answer.
func (a *App) AICancel(id string) { a.ai.Cancel(id) }

// AIReset forgets a conversation (the panel's "New chat").
func (a *App) AIReset(chat string) { a.ai.Reset(chat) }

// ---- Updates ------------------------------------------------------------------

// UpdateInfo tells the frontend whether a newer jamalterm exists.
type UpdateInfo struct {
	Available bool   `json:"available"`
	Current   string `json:"current"`
	Latest    string `json:"latest"`
	Notes     string `json:"notes"`
	URL       string `json:"url"`
	// CanInstall is false when the app can't replace itself (e.g. a
	// development build); Reason says why.
	CanInstall bool   `json:"canInstall"`
	Reason     string `json:"reason"`
}

// CheckForUpdate asks GitHub for the newest release.
func (a *App) CheckForUpdate() (UpdateInfo, error) {
	info := UpdateInfo{Current: version}
	ctx, cancel := context.WithTimeout(a.ctx, 20*time.Second)
	defer cancel()
	rel, err := update.Latest(ctx)
	if err != nil {
		return info, err
	}
	info.Latest, info.Notes, info.URL = rel.Version, rel.Notes, rel.URL
	info.Available = update.Newer(rel.Version, version)
	if _, err := update.AppBundle(); err != nil {
		info.Reason = err.Error()
	} else {
		info.CanInstall = true
	}
	a.mu.Lock()
	a.release = &rel
	a.mu.Unlock()
	return info, nil
}

// InstallUpdate downloads and installs the release found by
// CheckForUpdate, then restarts jamalterm. Open tabs close.
func (a *App) InstallUpdate() error {
	a.mu.Lock()
	rel := a.release
	a.mu.Unlock()
	if rel == nil {
		return errors.New("check for updates first")
	}
	app, err := update.AppBundle()
	if err != nil {
		return err
	}
	err = update.Install(a.ctx, *rel, app, func(done, total int64) {
		runtime.EventsEmit(a.ctx, eventProgress, map[string]int64{"done": done, "total": total})
	})
	if err != nil {
		return err
	}
	if err := update.Relaunch(app); err != nil {
		return fmt.Errorf("updated to %s, but couldn't restart: %v. Quit and open jamalterm again", rel.Version, err)
	}
	go func() {
		time.Sleep(200 * time.Millisecond)
		runtime.Quit(a.ctx)
	}()
	return nil
}

// ---- Window -----------------------------------------------------------------

// Quit closes the app (after the last tab closes).
func (a *App) Quit() { runtime.Quit(a.ctx) }

// OpenURL opens a link in the default browser.
func (a *App) OpenURL(url string) { runtime.BrowserOpenURL(a.ctx, url) }

func (a *App) menu(cmd string) func(_ *menuData) {
	return func(_ *menuData) { runtime.EventsEmit(a.ctx, eventMenu, cmd) }
}
