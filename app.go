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
	"syscall"
	"time"

	"github.com/wailsapp/wails/v3/pkg/application"

	"jamalterm/internal/ai"
	"jamalterm/internal/diag"
	"jamalterm/internal/settings"
	"jamalterm/internal/shell"
	"jamalterm/internal/update"
	"jamalterm/internal/winstate"
)

// Events sent to the frontend.
const (
	eventData         = "pty:data"         // {id, data(base64)}
	eventExit         = "pty:exit"         // {id, code}
	eventMenu         = "menu"             // menu command name
	eventSettings     = "settings:changed" // Settings
	eventFilesDropped = "files:dropped"    // paths dropped on a window
	eventAIDelta      = "ai:delta"         // {id, text}
	eventAIDone       = "ai:done"          // {id, text}
	eventAIError      = "ai:error"         // {id, error}
	eventProgress     = "update:progress"  // {done, total}
	eventAITool       = "ai:tool"          // {id, toolId, command, run}
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
// It is a Wails service: its exported methods are callable from TypeScript
// (frontend/bindings, used through frontend/src/backend.ts).
type App struct {
	ctx context.Context // the app's: cancelled when it quits
	app *application.App

	store   *settings.Store
	secrets *settings.Secrets
	ai      *ai.Client
	log     *diag.Log // private problem log (never sent anywhere)

	mu       sync.Mutex
	cfg      settings.Settings
	sessions map[int]*shell.Session
	pumps    map[int]*shell.Pump // batched output per tab
	release  *update.Release     // newest release seen by CheckForUpdate
	toolWait map[string]chan toolReply
	// blurAtLaunch: the window was created translucent (blur behind).
	blurAtLaunch bool
	// providerItems are the AI menu's "Use Claude / ChatGPT / DeepSeek".
	providerItems map[string]*application.MenuItem
	// primary: this copy of jamalterm restores and saves tabs (a second
	// copy started by hand doesn't).
	primary  bool
	lockFile *os.File

	// Windows (windows.go): which window owns each tab and AI question,
	// each window's saved tabs, and what new windows start with.
	nextWin    int
	owner      map[int]string     // tab id → window name
	aiOwner    map[string]string  // AI request id → window name
	saved      *winstate.Sessions // each window's saved tabs
	restoreFor map[string]string  // window name → tabs to reopen
	adoptFor   map[string]string  // new window name → a tab moved into it (layout JSON)
	nextAdopt  string             // the next window opened adopts this tab
	winInfo    map[string]winInfo // window name → its tabs (menu bar icon, Dock menu)
	sw         switcher           // the menu bar icon and Dock menu
	startDirs  map[string]string  // window name → first tab's folder
	quitting   bool

	// Safety (safety.go).
	quitOK     bool            // you agreed to quit with programs running
	closeOK    map[string]bool // windows you agreed to close
	accepted   map[int]bool    // busy tabs you agreed to stop
	activeTab  map[string]int  // window name → focused tab
	appActive  bool
	secureItem *application.MenuItem
	menuItems  map[string]*application.MenuItem // by command ID
	// lastTermName: the terminal window used most recently (menu commands
	// go there while Settings is in front).
	lastTermName string
	// lastCrash: how jamalterm quit last time, if it crashed (crash.go).
	lastCrash string
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
		pumps:    map[int]*shell.Pump{},
		toolWait: map[string]chan toolReply{},

		providerItems: map[string]*application.MenuItem{},
		owner:         map[int]string{},
		aiOwner:       map[string]string{},
		saved:         winstate.New(),
		restoreFor:    map[string]string{},
		adoptFor:      map[string]string{},
		winInfo:       map[string]winInfo{},
		startDirs:     map[string]string{},
		closeOK:       map[string]bool{},
		accepted:      map[int]bool{},
		activeTab:     map[string]int{},
		appActive:     true,
		menuItems:     map[string]*application.MenuItem{},
	}
	a.ctx = context.Background()
	a.primary = a.takeInstanceLock()
	update.KeepPreviousIn = filepath.Join(a.configDir(), "previous")
	update.TrustCache = filepath.Join(a.configDir(), "sigstore")
	ai.LocalBaseURL = func() string { return a.Settings().LocalURL }
	var loadErr error
	a.cfg, loadErr = store.Load()
	a.log = diag.New(filepath.Join(filepath.Dir(store.Path()), "problems.log"))
	a.log.SetEnabled(a.cfg.KeepProblemLog)
	if loadErr != nil {
		a.log.Problem("settings", "settings file couldn't be read, using defaults: %v", loadErr)
	}
	a.ai = ai.New(a.secrets.APIKey, func() string { return a.Settings().AIModel })
	a.ai.OnUsage(func(id string, u ai.Usage) {
		a.emitTo(a.aiWindow(id), "ai:usage", map[string]any{"id": id, "model": u.Model, "input": u.Input, "output": u.Output})
	})
	a.ai.OnRedact(func(id string, n int) {
		a.emitTo(a.aiWindow(id), "ai:redacted", map[string]any{"id": id, "count": n})
	})
	a.ai.Configure(
		func() string { return a.Settings().AIProvider },
		a.secrets.KeyFor,
		func(p string) string {
			switch p {
			case ai.ProviderOpenAI:
				return a.Settings().OpenAIModel
			case ai.ProviderDeepSeek:
				return a.Settings().DeepSeekModel
			case ai.ProviderXAI:
				return a.Settings().XAIModel
			case ai.ProviderOpenRouter:
				return a.Settings().OpenRouterModel
			case ai.ProviderLocal:
				return a.Settings().LocalModel
			}
			return ""
		})
	return a
}

// ServiceStartup runs when the app starts (Wails service).
func (a *App) ServiceStartup(ctx context.Context, _ application.ServiceOptions) error {
	a.ctx = ctx
	go a.watchSecureInput(ctx)
	return nil
}

// window finds a window by name (nil if it closed).
func (a *App) window(name string) application.Window {
	if name == "" || a.app == nil {
		return nil
	}
	w, _ := a.app.Window.GetByName(name)
	return w
}

func (a *App) aiWindow(reqID string) application.Window {
	a.mu.Lock()
	name := a.aiOwner[reqID]
	a.mu.Unlock()
	return a.window(name)
}

// applyWindowOpacity makes every window see-through when the opacity
// setting is below 100; the page then draws its own semi-transparent
// background.
func (a *App) applyWindowOpacity() {
	for _, w := range a.windows() {
		setOpacity(w, a.Settings().Opacity)
	}
}

func setOpacity(w application.Window, percent int) {
	if percent < 100 {
		w.SetBackgroundColour(application.NewRGBA(0, 0, 0, 0))
	} else {
		w.SetBackgroundColour(application.NewRGBA(21, 22, 30, 255))
	}
}

// SetWindowOpacity switches the calling window between solid and
// see-through (live preview from Settings; nothing is saved).
func (a *App) SetWindowOpacity(ctx context.Context, percent int) {
	if w := winOf(ctx); w != nil {
		setOpacity(w, percent)
	}
}

// BlurActive reports whether the window was started with background blur
// (it can only change at launch).
func (a *App) BlurActive() bool { return a.blurAtLaunch }

func (a *App) shutdown() {
	a.mu.Lock()
	defer a.mu.Unlock()
	for _, s := range a.sessions {
		s.Close()
	}
}

// ---- Shells (one per tab) ---------------------------------------------------

// Start launches a shell for tab id. cwd is where it starts ("" = home).
func (a *App) Start(ctx context.Context, id, cols, rows int, cwd string) error {
	win := winOf(ctx)
	a.mu.Lock()
	defer a.mu.Unlock()
	if win != nil {
		a.owner[id] = win.Name()
	}
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
	// Output is batched (≈60 times a second) and paced by the window's
	// acknowledgements (Ack), so heavy output can't freeze the app.
	pump := shell.NewPump(func(b []byte) {
		// Base64 keeps multi-byte characters split across batches intact.
		// To the window the tab is in now (tabs can move between windows).
		a.emitTo(a.tabWindow(id, win), eventData, ptyData{ID: id, Data: base64.StdEncoding.EncodeToString(b)})
	})
	s, err := shell.Start(shell.Options{
		Shell:       cfg.Shell,
		Dir:         cwd,
		Cols:        uint16(cols),
		Rows:        uint16(rows),
		Integration: cfg.ShellIntegration,
		OnOutput:    pump.Write,
		OnExit: func(err error) {
			pump.Close() // the last output goes out before the exit notice
			code := 0
			var ee *exec.ExitError
			if errors.As(err, &ee) {
				code = ee.ExitCode()
			}
			a.emitTo(a.tabWindow(id, win), eventExit, ptyExit{ID: id, Code: code})
		},
	})
	if err != nil {
		pump.Close()
		return a.problem("shell", fmt.Errorf("%s didn't start: %w", shellLabel(cfg.Shell), err))
	}
	a.sessions[id] = s
	a.pumps[id] = pump
	return nil
}

// Ack: the window has drawn n bytes of tab id's output.
func (a *App) Ack(id, n int) {
	a.mu.Lock()
	p := a.pumps[id]
	a.mu.Unlock()
	if p != nil {
		p.Ack(n)
	}
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
	p := a.pumps[id]
	delete(a.sessions, id)
	delete(a.pumps, id)
	delete(a.owner, id)
	delete(a.accepted, id)
	a.mu.Unlock()
	if p != nil {
		p.Close() // never leave the shell blocked waiting for the window
	}
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
		return a.Settings(), a.problem("settings", err)
	}
	saved, err := a.store.Load()
	if err != nil {
		return a.Settings(), a.problem("settings", err)
	}
	a.mu.Lock()
	a.cfg = saved
	a.mu.Unlock()
	a.log.SetEnabled(saved.KeepProblemLog)
	a.applyWindowOpacity()
	a.refreshProviderMenu()
	a.refreshSwitcher() // the menu bar icon may have been turned on or off
	a.emitTo(nil, eventSettings, saved)
	return saved, nil
}

// HasAPIKey reports whether AI features can be used: an API key, or a
// sign-in with an Anthropic account.
func (a *App) HasAPIKey() bool {
	if p := a.Settings().AIProvider; p == ai.ProviderLocal {
		return true // no key; the server is checked when you ask
	} else if p != ai.ProviderAnthropic {
		return a.secrets.KeyFor(p) != ""
	}
	return a.secrets.APIKey() != "" || ai.SignedInAccount().SignedIn
}

// SetAIProvider switches the assistant ("anthropic", "openai",
// "deepseek") and saves it, from the AI menu or the panel header.
func (a *App) SetAIProvider(id string) error {
	cfg := a.Settings()
	if cfg.AIProvider == id {
		return nil
	}
	cfg.AIProvider = id
	_, err := a.SaveSettings(cfg)
	return err
}

// refreshProviderMenu moves the AI menu's checkmark to the active assistant.
func (a *App) refreshProviderMenu() {
	current := a.Settings().AIProvider
	for id, item := range a.providerItems {
		item.SetChecked(id == current)
	}
	if a.secureItem != nil {
		a.secureItem.SetChecked(a.Settings().SecureInput)
	}
}

// SignInOpenRouter runs OpenRouter's browser sign-in and saves the key
// it returns (encrypted on this Mac). It returns once the user has approved.
func (a *App) SignInOpenRouter() error {
	ctx, cancel := context.WithTimeout(a.ctx, 6*time.Minute)
	defer cancel()
	key, err := ai.OpenRouterSignIn(ctx, func(u string) { a.OpenURL(u) })
	if err != nil {
		return a.problem("openrouter sign-in", err)
	}
	return a.problem("keys", a.secrets.SetKeyFor(ai.ProviderOpenRouter, key))
}

// AIProviders lists the assistants to choose from (Claude, ChatGPT, DeepSeek).
func (a *App) AIProviders() []ai.ProviderInfo { return ai.Providers }

// HasSavedProviderKey reports whether a key for provider is saved in jamalterm.
func (a *App) HasSavedProviderKey(provider string) bool { return a.secrets.HasSavedKeyFor(provider) }

// SetProviderKey saves a provider's API key ("" removes it).
func (a *App) SetProviderKey(provider, key string) error {
	return a.problem("keys", a.secrets.SetKeyFor(provider, key))
}

// ListProviderModels asks ChatGPT or DeepSeek which chat models the saved
// key can use (newest first).
func (a *App) ListProviderModels(provider string) ([]string, error) {
	ctx, cancel := context.WithTimeout(a.ctx, 15*time.Second)
	defer cancel()
	models, err := ai.ListModels(ctx, provider, a.secrets.KeyFor(provider))
	return models, a.problem("ai models", err)
}

// AnthropicAccount reports the `ant auth login` sign-in, if any.
func (a *App) AnthropicAccount() ai.Account { return ai.SignedInAccount() }

// HasSavedAPIKey reports whether a key is saved in jamalterm (vs. the env).
func (a *App) HasSavedAPIKey() bool { return a.secrets.HasSavedKey() }

// SetAPIKey saves the Claude API key ("" removes it).
func (a *App) SetAPIKey(key string) error { return a.problem("keys", a.secrets.SetAPIKey(key)) }

// ---- AI ---------------------------------------------------------------------

// AIAsk starts a question; the answer streams back as ai:* events.
func (a *App) AIAsk(ctx context.Context, req ai.Request) {
	win := winOf(ctx)
	if win != nil {
		a.mu.Lock()
		a.aiOwner[req.ID] = win.Name()
		a.mu.Unlock()
	}
	go func() {
		defer func() {
			a.mu.Lock()
			delete(a.aiOwner, req.ID)
			a.mu.Unlock()
		}()
		text, err := a.ai.Ask(a.ctx, req, func(delta string) {
			a.emitTo(win, eventAIDelta, aiEvent{ID: req.ID, Text: delta})
		}, func(call ai.ToolCall) (string, bool) {
			return a.runTool(req.ID, call, req.Context)
		})
		if err != nil {
			if errors.Is(err, context.Canceled) {
				err = errors.New("stopped")
			} else {
				// The error only: never the question or the answer.
				a.log.Problem("ai", "%s: %v", a.Settings().AIProvider, err)
			}
			a.emitTo(win, eventAIError, aiEvent{ID: req.ID, Error: err.Error()})
			return
		}
		a.emitTo(win, eventAIDone, aiEvent{ID: req.ID, Text: text})
	}()
}

// runTool asks the window to type (or, after the user confirms, run) a
// command and waits for what happened.
func (a *App) runTool(reqID string, call ai.ToolCall, shown string) (string, bool) {
	ch := make(chan toolReply, 1)
	a.mu.Lock()
	a.toolWait[call.ID] = ch
	a.mu.Unlock()
	defer func() {
		a.mu.Lock()
		delete(a.toolWait, call.ID)
		a.mu.Unlock()
	}()
	// How careful to be: risky commands (and ones that appear in the output
	// the AI was shown) need a second confirmation in the window.
	risk := ai.CommandRisk(call.Command, shown)
	a.emitTo(a.aiWindow(reqID), eventAITool, map[string]any{"id": reqID, "toolId": call.ID, "command": call.Command, "run": call.Run, "risk": risk})
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

// ---- Session (tabs restored after a restart) ------------------------------

func (a *App) configDir() string { return filepath.Dir(a.store.Path()) }

func (a *App) sessionFile() string { return filepath.Join(a.configDir(), "session.json") }

// takeInstanceLock makes the first window the one that owns the session.
func (a *App) takeInstanceLock() bool {
	os.MkdirAll(a.configDir(), 0o700)
	f, err := os.OpenFile(filepath.Join(a.configDir(), "window.lock"), os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		return true
	}
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		f.Close()
		return false
	}
	a.lockFile = f // held until the app exits
	return true
}

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
	rel, err := update.LatestIn(ctx, a.Settings().UpdateChannel == "beta")
	if err != nil {
		a.saveUpdateStatus(info, err)
		return info, a.problem("update check", err)
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
	a.saveUpdateStatus(info, nil)
	return info, nil
}

// InstallUpdate downloads and installs the release found by
// CheckForUpdate. With relaunch it restarts jamalterm right away (tabs are
// restored); without, the new version starts next time.
func (a *App) InstallUpdate(ctx context.Context, relaunch bool) error {
	win := winOf(ctx)
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
		a.emitTo(win, eventProgress, map[string]int64{"done": done, "total": total})
	})
	if err != nil {
		return a.problem("update", err)
	}
	if !relaunch {
		return nil
	}
	if err := update.Relaunch(app); err != nil {
		return a.problem("update", fmt.Errorf("updated to %s, but couldn't restart: %v. Quit and open jamalterm again", rel.Version, err))
	}
	go func() {
		time.Sleep(200 * time.Millisecond)
		a.app.Quit()
	}()
	return nil
}

// PreviousVersion is the version kept from before the last update ("" if
// none) for "Go Back to Previous Version".
func (a *App) PreviousVersion() string {
	p := update.PreviousApp()
	if p == "" {
		return ""
	}
	if v := update.AppVersion(p); v != "" {
		return v
	}
	return "previous"
}

// RevertUpdate goes back to the kept version and restarts.
func (a *App) RevertUpdate() error {
	app, err := update.AppBundle()
	if err != nil {
		return err
	}
	if err := update.Revert(app); err != nil {
		return a.problem("go back", err)
	}
	if err := update.Relaunch(app); err != nil {
		return a.problem("go back", fmt.Errorf("went back, but couldn't restart: %v. Quit and open jamalterm again", err))
	}
	go func() {
		time.Sleep(200 * time.Millisecond)
		a.app.Quit()
	}()
	return nil
}

// ---- Window -----------------------------------------------------------------

// Quit closes the app (after the last tab closes).
func (a *App) Quit() { a.app.Quit() }

// OpenURL opens a link in the default browser.
func (a *App) OpenURL(url string) { a.problem("browser", a.app.Browser.OpenURL(url)) }

// eventThemePreview tells terminal windows to show a theme for a moment
// (hovering a swatch in Settings); "" goes back to the saved one.
const eventThemePreview = "theme:preview"

// PreviewTheme shows a theme in every terminal window without saving it.
func (a *App) PreviewTheme(id string) {
	a.emitTo(nil, eventThemePreview, id)
}

// GitRepo names the git repository dir is in ("" outside one), for project
// tab colors.
func (a *App) GitRepo(dir string) string {
	root := shell.GitRoot(dir)
	if root == "" {
		return ""
	}
	return filepath.Base(root)
}
