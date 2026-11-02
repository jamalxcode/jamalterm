package main

import (
	goruntime "runtime"

	"jamalterm/internal/ai"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// cmd is ⌘+key on macOS. Elsewhere (only used for development) it is
// Ctrl+Shift+key so the shell keeps Ctrl+W, Ctrl+K and friends.
func cmd(key string) string {
	if goruntime.GOOS == "darwin" {
		return "Cmd+" + key
	}
	return "Ctrl+Shift+" + key
}

// cmdShift is ⇧⌘+key (Ctrl+Alt+Shift+key in development builds).
func cmdShift(key string) string {
	if goruntime.GOOS == "darwin" {
		return "Cmd+Shift+" + key
	}
	return "Ctrl+Alt+Shift+" + key
}

// cmdOpt is ⌥⌘+key.
func cmdOpt(key string) string {
	if goruntime.GOOS == "darwin" {
		return "Cmd+Option+" + key
	}
	return "Ctrl+Alt+" + key
}

// item adds a menu entry that sends cmd to the window in front
// (frontend/src/main.ts, runMenu).
func (a *App) item(m *application.Menu, label, accel, command string) *application.MenuItem {
	it := m.Add(label).OnClick(func(*application.Context) { a.toFront(eventMenu, command) })
	if accel != "" {
		it.SetAccelerator(accel)
	}
	return it
}

// buildMenu creates the menu bar.
func (a *App) buildMenu() *application.Menu {
	m := a.app.NewMenu()

	appMenu := m.AddSubmenu("jamalterm")
	a.item(appMenu, "About jamalterm", "", "about")
	a.item(appMenu, "Check for Updates…", "", "checkUpdates")
	a.item(appMenu, "Go Back to Previous Version…", "", "revertUpdate")
	appMenu.AddSeparator()
	a.item(appMenu, "Settings…", cmd(","), "settings")
	a.secureItem = appMenu.AddCheckbox("Secure Keyboard Entry", a.Settings().SecureInput)
	a.secureItem.OnClick(func(c *application.Context) {
		cfg := a.Settings()
		cfg.SecureInput = c.IsChecked()
		a.SaveSettings(cfg)
	})
	appMenu.AddSeparator()
	appMenu.Add("Hide jamalterm").SetAccelerator(cmd("h")).OnClick(func(*application.Context) { a.app.Hide() })
	appMenu.Add("Quit jamalterm").SetAccelerator(cmd("q")).OnClick(func(*application.Context) { a.app.Quit() })

	shellMenu := m.AddSubmenu("Shell")
	a.item(shellMenu, "New Tab", cmd("t"), "newTab")
	a.item(shellMenu, "New Window", cmd("n"), "newWindow")
	shellMenu.AddSeparator()
	a.item(shellMenu, "Split Right", cmd("d"), "splitRight")
	a.item(shellMenu, "Split Down", cmdShift("d"), "splitDown")
	shellMenu.AddSeparator()
	a.item(shellMenu, "Close Pane", cmd("w"), "closeTab")
	a.item(shellMenu, "Close Tab", cmdShift("w"), "closeGroup")
	shellMenu.AddSeparator()
	a.item(shellMenu, "Enable Browse Mode on This Server", "", "remoteIntegration")
	shellMenu.AddSeparator()
	a.item(shellMenu, "Clear", cmd("k"), "clear")
	a.item(shellMenu, "Find…", cmd("f"), "find")

	m.AddRole(application.EditMenu)

	view := m.AddSubmenu("View")
	a.item(view, "Command Palette…", cmdShift("p"), "palette")
	view.AddSeparator()
	a.item(view, "Browse Commands", cmd("Up"), "browse")
	a.item(view, "Previous Command", cmdShift("Up"), "prevCommand")
	a.item(view, "Next Command", cmdShift("Down"), "nextCommand")
	view.AddSeparator()
	a.item(view, "Bigger Text", cmd("="), "zoomIn")
	a.item(view, "Smaller Text", cmd("-"), "zoomOut")
	a.item(view, "Actual Size", cmd("0"), "zoomReset")
	view.AddSeparator()
	view.Add("Reload Window").SetAccelerator(cmdShift("r")).OnClick(func(*application.Context) {
		if w := a.app.Window.Current(); w != nil {
			w.Reload() // tabs keep running and reattach
		}
	})

	aiMenu := m.AddSubmenu("AI")
	a.item(aiMenu, "Ask AI…", cmd("i"), "aiPanel")
	a.item(aiMenu, "Command from English…", cmd("j"), "aiCommand")
	a.item(aiMenu, "Explain Last Error", cmdShift("e"), "aiExplainLast")
	aiMenu.AddSeparator()
	// Switch assistants on the fly: ⌥⌘1 Claude, ⌥⌘2 ChatGPT, …
	current := a.Settings().AIProvider
	for i, p := range ai.Providers {
		id := p.ID
		it := aiMenu.AddRadio("Use "+p.Name, id == current).SetAccelerator(cmdOpt(string(rune('1' + i))))
		it.OnClick(func(*application.Context) {
			if err := a.SetAIProvider(id); err != nil {
				a.toFront(eventMenu, "toast:"+err.Error())
			}
		})
		a.providerItems[id] = it
	}

	win := m.AddSubmenu("Window")
	a.item(win, "Next Tab", cmdShift("]"), "nextTab")
	a.item(win, "Previous Tab", cmdShift("["), "prevTab")
	a.item(win, "Next Pane", cmd("]"), "nextPane")
	a.item(win, "Previous Pane", cmd("["), "prevPane")
	a.item(win, "Pane Left", cmdOpt("Left"), "paneLeft")
	a.item(win, "Pane Right", cmdOpt("Right"), "paneRight")
	a.item(win, "Pane Above", cmdOpt("Up"), "paneUp")
	a.item(win, "Pane Below", cmdOpt("Down"), "paneDown")
	win.AddSeparator()
	for i := 1; i <= 9; i++ {
		n := string(rune('0' + i))
		a.item(win, "Tab "+n, cmd(n), "tab"+n)
	}
	win.AddSeparator()
	win.Add("Minimize").SetAccelerator(cmd("m")).OnClick(func(*application.Context) {
		if w := a.app.Window.Current(); w != nil {
			w.Minimise()
		}
	})
	fs := "Cmd+Ctrl+F"
	if goruntime.GOOS != "darwin" {
		fs = "F11"
	}
	win.Add("Toggle Full Screen").SetAccelerator(fs).OnClick(func(*application.Context) {
		if w := a.app.Window.Current(); w != nil {
			w.ToggleFullscreen()
		}
	})
	win.AddSeparator()
	win.Add("Next Window").SetAccelerator(cmd("`")).OnClick(func(*application.Context) { a.cycleWindows() })

	help := m.AddSubmenu("Help")
	a.item(help, "Command Palette…", "", "palette")
	a.item(help, "Keyboard Shortcuts", "", "about")
	help.AddSeparator()
	a.item(help, "Copy Diagnostics…", "", "diagnostics")
	help.Add("Show Problem Log").OnClick(func(*application.Context) { a.ShowProblemLog() })
	return m
}
