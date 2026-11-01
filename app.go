package main

import (
	"context"
	"encoding/base64"
	"errors"
	"os/exec"
	"path/filepath"
	"sync"

	"github.com/wailsapp/wails/v2/pkg/runtime"

	"jamalterm/internal/shell"
)

// Events sent to the frontend.
const (
	eventData = "pty:data" // base64 of raw shell output
	eventExit = "pty:exit" // shell exited, payload is the exit code
)

// App is the bridge between the window (xterm.js) and the shell.
// Its exported methods are callable from TypeScript (see frontend/wailsjs).
type App struct {
	ctx context.Context

	mu      sync.Mutex
	session *shell.Session
}

func NewApp() *App { return &App{} }

func (a *App) startup(ctx context.Context) { a.ctx = ctx }

func (a *App) shutdown(ctx context.Context) {
	if s := a.current(); s != nil {
		s.Close()
	}
}

// Start launches the user's shell at the given size. Calling it again after
// the shell exited starts a new one.
func (a *App) Start(cols, rows int) error {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.session != nil {
		select {
		case <-a.session.Done():
		default:
			return nil // already running
		}
	}

	s, err := shell.Start(shell.Options{
		Cols: uint16(cols),
		Rows: uint16(rows),
		OnOutput: func(p []byte) {
			// Base64 keeps multi-byte characters that are split across
			// two reads intact; xterm.js reassembles them.
			runtime.EventsEmit(a.ctx, eventData, base64.StdEncoding.EncodeToString(p))
		},
		OnExit: func(err error) {
			code := 0
			var ee *exec.ExitError
			if errors.As(err, &ee) {
				code = ee.ExitCode()
			}
			runtime.EventsEmit(a.ctx, eventExit, code)
		},
	})
	if err != nil {
		return err
	}
	a.session = s
	return nil
}

// Write sends keyboard input or pasted text to the shell.
func (a *App) Write(data string) {
	if s := a.current(); s != nil {
		s.Write([]byte(data))
	}
}

// Resize is called whenever the window (and so the character grid) changes size.
func (a *App) Resize(cols, rows int) {
	if s := a.current(); s != nil && cols > 0 && rows > 0 {
		s.Resize(uint16(cols), uint16(rows))
	}
}

// ShellName is shown in the window title, e.g. "zsh".
func (a *App) ShellName() string {
	return filepath.Base(shell.DefaultShell())
}

func (a *App) current() *shell.Session {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.session
}
