package main

import (
	goruntime "runtime"

	"jamalterm/internal/ai"

	"github.com/wailsapp/wails/v2/pkg/menu"
	"github.com/wailsapp/wails/v2/pkg/menu/keys"
	"github.com/wailsapp/wails/v2/pkg/runtime"
)

type menuData = menu.CallbackData

// cmd is ⌘+key on macOS. Elsewhere (only used for development) it is
// Ctrl+Shift+key so the shell keeps Ctrl+W, Ctrl+K and friends.
func cmd(key string) *keys.Accelerator {
	if goruntime.GOOS == "darwin" {
		return keys.CmdOrCtrl(key)
	}
	return keys.Combo(key, keys.CmdOrCtrlKey, keys.ShiftKey)
}

func cmdShift(key string) *keys.Accelerator {
	return keys.Combo(key, keys.CmdOrCtrlKey, keys.ShiftKey)
}

// buildMenu creates the menu bar. Each item sends a "menu" event that the
// frontend handles (frontend/src/main.ts, runMenu).
func (a *App) buildMenu() *menu.Menu {
	m := menu.NewMenu()

	appMenu := m.AddSubmenu("jamalterm")
	appMenu.AddText("About jamalterm", nil, a.menu("about"))
	appMenu.AddText("Check for Updates…", nil, a.menu("checkUpdates"))
	appMenu.AddText("Go Back to Previous Version…", nil, a.menu("revertUpdate"))
	appMenu.AddSeparator()
	appMenu.AddText("Settings…", cmd(","), a.menu("settings"))
	appMenu.AddSeparator()
	appMenu.AddText("Hide jamalterm", cmd("h"), func(_ *menu.CallbackData) { runtime.Hide(a.ctx) })
	appMenu.AddText("Quit jamalterm", cmd("q"), func(_ *menu.CallbackData) { runtime.Quit(a.ctx) })

	shellMenu := m.AddSubmenu("Shell")
	shellMenu.AddText("New Tab", cmd("t"), a.menu("newTab"))
	shellMenu.AddText("New Window", cmd("n"), a.menu("newWindow"))
	shellMenu.AddText("Split Right", cmd("d"), a.menu("splitRight"))
	shellMenu.AddText("Split Down", cmdShift("d"), a.menu("splitDown"))
	shellMenu.AddSeparator()
	shellMenu.AddText("Close Pane", cmd("w"), a.menu("closeTab"))
	shellMenu.AddText("Close Tab", cmdShift("w"), a.menu("closeGroup"))
	shellMenu.AddSeparator()
	shellMenu.AddText("Clear", cmd("k"), a.menu("clear"))
	shellMenu.AddText("Find…", cmd("f"), a.menu("find"))

	m.Append(menu.EditMenu())

	view := m.AddSubmenu("View")
	view.AddText("Browse Commands", cmd("up"), a.menu("browse"))
	view.AddText("Previous Command", cmdShift("up"), a.menu("prevCommand"))
	view.AddText("Next Command", cmdShift("down"), a.menu("nextCommand"))
	view.AddSeparator()
	view.AddText("Bigger Text", cmd("="), a.menu("zoomIn"))
	view.AddText("Smaller Text", cmd("-"), a.menu("zoomOut"))
	view.AddText("Actual Size", cmd("0"), a.menu("zoomReset"))

	aiMenu := m.AddSubmenu("AI")
	aiMenu.AddText("Ask AI…", cmd("i"), a.menu("aiPanel"))
	aiMenu.AddText("Command from English…", cmd("j"), a.menu("aiCommand"))
	aiMenu.AddText("Explain Last Error", cmdShift("e"), a.menu("aiExplainLast"))
	aiMenu.AddSeparator()
	// Switch assistants on the fly: ⌥⌘1 Claude, ⌥⌘2 ChatGPT, ⌥⌘3 DeepSeek.
	current := a.Settings().AIProvider
	for i, p := range ai.Providers {
		id := p.ID
		key := keys.Combo(string(rune('1'+i)), keys.CmdOrCtrlKey, keys.OptionOrAltKey)
		a.providerItems[id] = aiMenu.AddRadio("Use "+p.Name, id == current, key, func(_ *menu.CallbackData) {
			if err := a.SetAIProvider(id); err != nil {
				runtime.EventsEmit(a.ctx, eventMenu, "toast:"+err.Error())
			}
		})
	}

	win := m.AddSubmenu("Window")
	win.AddText("Next Tab", cmdShift("]"), a.menu("nextTab"))
	win.AddText("Previous Tab", cmdShift("["), a.menu("prevTab"))
	win.AddText("Next Pane", cmd("]"), a.menu("nextPane"))
	win.AddText("Previous Pane", cmd("["), a.menu("prevPane"))
	win.AddText("Pane Left", keys.Combo("left", keys.CmdOrCtrlKey, keys.OptionOrAltKey), a.menu("paneLeft"))
	win.AddText("Pane Right", keys.Combo("right", keys.CmdOrCtrlKey, keys.OptionOrAltKey), a.menu("paneRight"))
	win.AddText("Pane Above", keys.Combo("up", keys.CmdOrCtrlKey, keys.OptionOrAltKey), a.menu("paneUp"))
	win.AddText("Pane Below", keys.Combo("down", keys.CmdOrCtrlKey, keys.OptionOrAltKey), a.menu("paneDown"))
	win.AddSeparator()
	for i := 1; i <= 9; i++ {
		n := string(rune('0' + i))
		win.AddText("Tab "+n, cmd(n), a.menu("tab"+n))
	}
	win.AddSeparator()
	win.AddText("Minimize", cmd("m"), func(_ *menu.CallbackData) { runtime.WindowMinimise(a.ctx) })
	win.AddText("Toggle Full Screen", keys.Combo("f", keys.CmdOrCtrlKey, keys.ControlKey), func(_ *menu.CallbackData) {
		if runtime.WindowIsFullscreen(a.ctx) {
			runtime.WindowUnfullscreen(a.ctx)
		} else {
			runtime.WindowFullscreen(a.ctx)
		}
	})

	help := m.AddSubmenu("Help")
	help.AddText("Command Palette…", cmdShift("p"), a.menu("palette"))
	help.AddText("Keyboard Shortcuts", nil, a.menu("about"))
	help.AddSeparator()
	help.AddText("Copy Diagnostics…", nil, a.menu("diagnostics"))
	help.AddText("Show Problem Log", nil, func(_ *menu.CallbackData) { a.ShowProblemLog() })
	return m
}
