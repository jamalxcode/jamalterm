package main

import (
	"context"
	"fmt"
	goruntime "runtime"
	"strings"
)

// Command is something you can do from the menu bar, the command palette
// or a shortcut. This table is the only list: the menu is built from it,
// and the window reads it (Commands) for the palette and for ⌘ keys.
type Command struct {
	ID    string `json:"id"`
	Title string `json:"title"`
	Menu  string `json:"menu"` // menu bar menu
	// Keys in Mac terms, e.g. "Cmd+Shift+D" ("" = none). Development builds
	// on Linux use Ctrl+Shift for Cmd so the shell keeps Ctrl+W and friends.
	Keys string `json:"keys"`
	// Palette: offered in the command palette.
	Palette bool `json:"palette"`
	// Native: done by the app itself (windows, quitting…), not the window.
	Native bool `json:"native"`
	// Kind of menu entry: "" a plain item; "check" Secure Keyboard Entry;
	// "providers" the AI assistant choices; "edit" the standard Edit menu.
	Kind string `json:"kind,omitempty"`
	// Sep: a separator line before it in the menu.
	Sep bool `json:"-"`
}

var commands = func() []Command {
	c := []Command{
		{ID: "about", Title: "About jamalterm", Menu: "jamalterm"},
		{ID: "checkUpdates", Title: "Check for Updates…", Menu: "jamalterm", Palette: true},
		{ID: "revertUpdate", Title: "Go Back to Previous Version…", Menu: "jamalterm", Palette: true},
		{ID: "settings", Title: "Settings…", Menu: "jamalterm", Keys: "Cmd+,", Palette: true, Sep: true},
		{ID: "secureInput", Title: "Secure Keyboard Entry", Menu: "jamalterm", Kind: "check"},
		{ID: "hide", Title: "Hide jamalterm", Menu: "jamalterm", Keys: "Cmd+H", Native: true, Sep: true},
		{ID: "quit", Title: "Quit jamalterm", Menu: "jamalterm", Keys: "Cmd+Q", Native: true},

		{ID: "newTab", Title: "New Tab", Menu: "Shell", Keys: "Cmd+T", Palette: true},
		{ID: "newWindow", Title: "New Window", Menu: "Shell", Keys: "Cmd+N", Palette: true},
		{ID: "splitRight", Title: "Split Right", Menu: "Shell", Keys: "Cmd+D", Palette: true, Sep: true},
		{ID: "splitDown", Title: "Split Down", Menu: "Shell", Keys: "Cmd+Shift+D", Palette: true},
		{ID: "closeTab", Title: "Close Pane", Menu: "Shell", Keys: "Cmd+W", Palette: true, Sep: true},
		{ID: "closeGroup", Title: "Close Tab", Menu: "Shell", Keys: "Cmd+Shift+W", Palette: true},
		{ID: "remoteIntegration", Title: "Enable Browse Mode on This Server", Menu: "Shell", Palette: true, Sep: true},
		{ID: "clear", Title: "Clear", Menu: "Shell", Keys: "Cmd+K", Palette: true, Sep: true},
		{ID: "find", Title: "Find…", Menu: "Shell", Keys: "Cmd+F", Palette: true},

		{ID: "edit", Menu: "Edit", Kind: "edit"},

		{ID: "palette", Title: "Command Palette…", Menu: "View", Keys: "Cmd+Shift+P"},
		{ID: "browse", Title: "Browse Commands", Menu: "View", Keys: "Cmd+Up", Palette: true, Sep: true},
		{ID: "prevCommand", Title: "Previous Command", Menu: "View", Keys: "Cmd+Shift+Up", Palette: true},
		{ID: "nextCommand", Title: "Next Command", Menu: "View", Keys: "Cmd+Shift+Down", Palette: true},
		{ID: "zoomIn", Title: "Bigger Text", Menu: "View", Keys: "Cmd+=", Palette: true, Sep: true},
		{ID: "zoomOut", Title: "Smaller Text", Menu: "View", Keys: "Cmd+-", Palette: true},
		{ID: "zoomReset", Title: "Actual Size", Menu: "View", Keys: "Cmd+0", Palette: true},
		{ID: "reloadWindow", Title: "Reload Window", Menu: "View", Keys: "Cmd+Shift+R", Native: true, Palette: true, Sep: true},

		{ID: "aiPanel", Title: "Ask AI…", Menu: "AI", Keys: "Cmd+I", Palette: true},
		{ID: "aiCommand", Title: "Command from English…", Menu: "AI", Keys: "Cmd+J", Palette: true},
		{ID: "aiExplainLast", Title: "Explain Last Error", Menu: "AI", Keys: "Cmd+Shift+E", Palette: true},
		{ID: "providers", Menu: "AI", Kind: "providers", Sep: true},

		{ID: "nextTab", Title: "Next Tab", Menu: "Window", Keys: "Cmd+Shift+]", Palette: true},
		{ID: "prevTab", Title: "Previous Tab", Menu: "Window", Keys: "Cmd+Shift+[", Palette: true},
		{ID: "nextPane", Title: "Next Pane", Menu: "Window", Keys: "Cmd+]", Palette: true},
		{ID: "prevPane", Title: "Previous Pane", Menu: "Window", Keys: "Cmd+[", Palette: true},
		{ID: "paneLeft", Title: "Pane Left", Menu: "Window", Keys: "Cmd+Option+Left", Palette: true},
		{ID: "paneRight", Title: "Pane Right", Menu: "Window", Keys: "Cmd+Option+Right", Palette: true},
		{ID: "paneUp", Title: "Pane Above", Menu: "Window", Keys: "Cmd+Option+Up", Palette: true},
		{ID: "paneDown", Title: "Pane Below", Menu: "Window", Keys: "Cmd+Option+Down", Palette: true},
		{ID: "broadcast", Title: "Broadcast Typing to All Panes", Menu: "Window", Keys: "Cmd+Option+I", Palette: true},
	}
	for i := 1; i <= 9; i++ {
		n := fmt.Sprint(i)
		c = append(c, Command{ID: "tab" + n, Title: "Tab " + n, Menu: "Window", Keys: "Cmd+" + n, Sep: i == 1})
	}
	c = append(c,
		Command{ID: "minimize", Title: "Minimize", Menu: "Window", Keys: "Cmd+M", Native: true, Sep: true},
		Command{ID: "fullscreen", Title: "Toggle Full Screen", Menu: "Window", Keys: "Cmd+Ctrl+F", Native: true, Palette: true},
		Command{ID: "nextWindow", Title: "Next Window", Menu: "Window", Keys: "Cmd+`", Native: true, Palette: true, Sep: true},

		Command{ID: "palette", Title: "Command Palette…", Menu: "Help"},
		Command{ID: "about", Title: "Keyboard Shortcuts", Menu: "Help", Palette: true},
		Command{ID: "diagnostics", Title: "Copy Diagnostics…", Menu: "Help", Palette: true, Sep: true},
		Command{ID: "showLog", Title: "Show Problem Log", Menu: "Help", Native: true, Palette: true},
	)
	return c
}()

// Commands lists every command (for the palette and ⌘ keys in the window).
func (a *App) Commands() []Command {
	out := make([]Command, 0, len(commands))
	for _, c := range commands {
		if c.Kind == "" {
			c.Keys = keysHere(c.Keys)
			out = append(out, c)
		}
	}
	return out
}

// keysHere turns Mac keys into this build's (Linux development builds use
// Ctrl+Shift for ⌘).
func keysHere(k string) string {
	if k == "" || goruntime.GOOS == "darwin" {
		return k
	}
	if k == "Cmd+Ctrl+F" {
		return "F11"
	}
	r := strings.NewReplacer("Cmd+Shift+", "Ctrl+Alt+Shift+", "Cmd+Option+", "Ctrl+Alt+", "Cmd+", "Ctrl+Shift+")
	return r.Replace(k)
}

// RunCommand does a command the app handles itself (from the palette or a
// key the menu bar didn't catch).
func (a *App) RunCommand(ctx context.Context, id string) {
	if f := a.native(id); f != nil {
		f()
	}
}

func (a *App) native(id string) func() {
	switch id {
	case "hide":
		return a.app.Hide
	case "quit":
		return a.app.Quit
	case "showLog":
		return a.ShowProblemLog
	case "nextWindow":
		return a.cycleWindows
	case "minimize":
		return func() {
			if w := a.app.Window.Current(); w != nil {
				w.Minimise()
			}
		}
	case "fullscreen":
		return func() {
			if w := a.app.Window.Current(); w != nil {
				w.ToggleFullscreen()
			}
		}
	case "reloadWindow":
		return func() {
			if w := a.app.Window.Current(); w != nil {
				w.Reload() // tabs keep running and reattach
			}
		}
	}
	return nil
}
