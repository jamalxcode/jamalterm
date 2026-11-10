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
	// "toggle" an item with a checkmark the window sets; "providers" the AI
	// assistant choices; "role" a standard macOS item (Role says which).
	Kind string `json:"kind,omitempty"`
	Role string `json:"-"`
	// Sep: a separator line before it in the menu.
	Sep bool `json:"-"`
}

// The menus follow Apple's layout for apps like Terminal: the app menu,
// Shell, Edit, View, then the app's own (AI), Window and Help.
// docs/DESIGN.md explains the choices.
var commands = func() []Command {
	c := []Command{
		{ID: "about", Title: "About jamalterm", Menu: "jamalterm", Kind: "role", Role: "about"},
		{ID: "checkUpdates", Title: "Check for Updates…", Menu: "jamalterm", Palette: true},
		{ID: "revertUpdate", Title: "Go Back to Previous Version…", Menu: "jamalterm", Palette: true},
		{ID: "settings", Title: "Settings…", Menu: "jamalterm", Keys: "Cmd+,", Palette: true, Native: true, Sep: true},
		{ID: "secureInput", Title: "Secure Keyboard Entry", Menu: "jamalterm", Kind: "check"},
		{ID: "services", Menu: "jamalterm", Kind: "role", Role: "services", Sep: true},
		{ID: "hide", Title: "Hide jamalterm", Menu: "jamalterm", Kind: "role", Role: "hide", Sep: true},
		{ID: "hideOthers", Title: "Hide Others", Menu: "jamalterm", Kind: "role", Role: "hideOthers"},
		{ID: "showAll", Title: "Show All", Menu: "jamalterm", Kind: "role", Role: "showAll"},
		{ID: "quit", Title: "Quit jamalterm", Menu: "jamalterm", Kind: "role", Role: "quit", Sep: true},

		{ID: "newWindow", Title: "New Window", Menu: "Shell", Keys: "Cmd+N", Palette: true},
		{ID: "newTab", Title: "New Tab", Menu: "Shell", Keys: "Cmd+T", Palette: true},
		{ID: "recentFolders", Title: "Open Recent Folder…", Menu: "Shell", Keys: "Cmd+Shift+O", Palette: true},
		{ID: "splitRight", Title: "Split Pane Right", Menu: "Shell", Keys: "Cmd+D", Palette: true, Sep: true},
		{ID: "splitDown", Title: "Split Pane Down", Menu: "Shell", Keys: "Cmd+Shift+D", Palette: true},
		// ⌘W closes the pane when the tab is split ("Close Pane"), else the tab.
		{ID: "closeTab", Title: "Close Tab", Menu: "Shell", Keys: "Cmd+W", Palette: true, Sep: true},
		{ID: "closeGroup", Title: "Close Tab and Its Panes", Menu: "Shell", Keys: "Cmd+Option+W", Palette: true},
		{ID: "closeWindow", Title: "Close Window", Menu: "Shell", Keys: "Cmd+Shift+W", Palette: true},
		{ID: "remoteIntegration", Title: "Enable Browse Mode on This Server", Menu: "Shell", Palette: true, Sep: true},
		{ID: "clear", Title: "Clear Scrollback", Menu: "Shell", Keys: "Cmd+K", Palette: true, Sep: true},

		{ID: "undo", Menu: "Edit", Kind: "role", Role: "undo"},
		{ID: "redo", Menu: "Edit", Kind: "role", Role: "redo"},
		{ID: "cut", Menu: "Edit", Kind: "role", Role: "cut", Sep: true},
		{ID: "copy", Menu: "Edit", Kind: "role", Role: "copy"},
		{ID: "paste", Menu: "Edit", Kind: "role", Role: "paste"},
		{ID: "selectAll", Title: "Select All", Menu: "Edit", Keys: "Cmd+A", Palette: true},
		{ID: "find", Title: "Find…", Menu: "Edit", Keys: "Cmd+F", Palette: true, Sep: true},
		{ID: "findNext", Title: "Find Next", Menu: "Edit", Keys: "Cmd+G", Palette: true},
		{ID: "findPrevious", Title: "Find Previous", Menu: "Edit", Keys: "Cmd+Shift+G", Palette: true},
		{ID: "findSelection", Title: "Use Selection for Find", Menu: "Edit", Keys: "Cmd+E", Palette: true},

		{ID: "toggleTabBar", Title: "Show Tab Bar", Menu: "View", Keys: "Cmd+Shift+T", Palette: true},
		{ID: "palette", Title: "Command Palette…", Menu: "View", Keys: "Cmd+Shift+P", Sep: true},
		{ID: "browse", Title: "Browse Commands", Menu: "View", Keys: "Cmd+Up", Palette: true, Sep: true},
		{ID: "prevCommand", Title: "Previous Command", Menu: "View", Keys: "Cmd+Shift+Up", Palette: true},
		{ID: "nextCommand", Title: "Next Command", Menu: "View", Keys: "Cmd+Shift+Down", Palette: true},
		{ID: "zoomIn", Title: "Bigger", Menu: "View", Keys: "Cmd+=", Palette: true, Sep: true},
		{ID: "zoomOut", Title: "Smaller", Menu: "View", Keys: "Cmd+-", Palette: true},
		{ID: "zoomReset", Title: "Default Font Size", Menu: "View", Keys: "Cmd+0", Palette: true},
		// The system item: it reads "Exit Full Screen" when full screen.
		{ID: "fullscreen", Menu: "View", Kind: "role", Role: "fullscreen", Sep: true},
		{ID: "reloadWindow", Title: "Reload Window", Menu: "View", Keys: "Cmd+Shift+R", Native: true, Palette: true},

		{ID: "aiPanel", Title: "Ask AI…", Menu: "AI", Keys: "Cmd+I", Palette: true},
		{ID: "aiCommand", Title: "Command from English…", Menu: "AI", Keys: "Cmd+J", Palette: true},
		{ID: "aiExplainLast", Title: "Explain Last Error", Menu: "AI", Keys: "Cmd+Shift+E", Palette: true},
		{ID: "providers", Menu: "AI", Kind: "providers", Sep: true},

		{ID: "minimize", Title: "Minimize", Menu: "Window", Kind: "role", Role: "minimise"},
		{ID: "zoom", Title: "Zoom", Menu: "Window", Kind: "role", Role: "zoom"},
		{ID: "prevTab", Title: "Show Previous Tab", Menu: "Window", Keys: "Cmd+Shift+[", Palette: true, Sep: true},
		{ID: "nextTab", Title: "Show Next Tab", Menu: "Window", Keys: "Cmd+Shift+]", Palette: true},
		{ID: "prevPane", Title: "Select Previous Pane", Menu: "Window", Keys: "Cmd+[", Palette: true, Sep: true},
		{ID: "nextPane", Title: "Select Next Pane", Menu: "Window", Keys: "Cmd+]", Palette: true},
		{ID: "paneLeft", Title: "Select Pane on the Left", Menu: "Window", Keys: "Cmd+Option+Left", Palette: true},
		{ID: "paneRight", Title: "Select Pane on the Right", Menu: "Window", Keys: "Cmd+Option+Right", Palette: true},
		{ID: "paneUp", Title: "Select Pane Above", Menu: "Window", Keys: "Cmd+Option+Up", Palette: true},
		{ID: "paneDown", Title: "Select Pane Below", Menu: "Window", Keys: "Cmd+Option+Down", Palette: true},
		{ID: "broadcast", Title: "Broadcast Typing to All Panes", Menu: "Window", Keys: "Cmd+Option+I", Palette: true, Kind: "toggle"},
	}
	for i := 1; i <= 9; i++ {
		n := fmt.Sprint(i)
		c = append(c, Command{ID: "tab" + n, Title: "Select Tab " + n, Menu: "Window", Keys: "Cmd+" + n, Sep: i == 1})
	}
	c = append(c,
		// macOS adds the list of open windows after this.
		Command{ID: "front", Title: "Bring All to Front", Menu: "Window", Kind: "role", Role: "front", Sep: true},

		Command{ID: "help", Title: "jamalterm Help", Menu: "Help", Native: true, Palette: true},
		Command{ID: "shortcuts", Title: "Keyboard Shortcuts", Menu: "Help", Palette: true},
		Command{ID: "diagnostics", Title: "Copy Diagnostics…", Menu: "Help", Palette: true, Sep: true},
		Command{ID: "showLog", Title: "Show Problem Log", Menu: "Help", Native: true, Palette: true},
	)
	return c
}()

// Commands lists every command (for the palette and ⌘ keys in the window).
func (a *App) Commands() []Command {
	out := make([]Command, 0, len(commands))
	for _, c := range commands {
		if c.Kind == "" || c.Kind == "toggle" {
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
	case "settings":
		return a.OpenSettings
	case "help":
		return func() { a.OpenURL("https://github.com/jamalxcode/jamalterm#readme") }
	case "showLog":
		return a.ShowProblemLog
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
