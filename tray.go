package main

import (
	_ "embed"
	"fmt"
	goruntime "runtime"
	"sort"
	"sync"
	"time"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// The menu bar icon and the Dock menu list every window and its tabs, so a
// terminal is one click away even when it's buried under other apps. Both
// are built from the same list (switchItems).

//go:embed build/trayicon.png
var trayIcon []byte

// switchItem is one line of the list: a window, one of its tabs, or a command.
type switchItem struct {
	Label    string
	Indent   bool // a tab, under its window
	Checked  bool // the window in front
	Disabled bool
	Sep      bool // a separator line
	run      func()
}

// winInfo is what a window told us about itself (SetMenuState).
type winInfo struct {
	tabs   []string
	active int
}

// switchItems lists the windows (by their active tab's name) and their tabs,
// then New Window, New Tab and Show All Windows.
func switchItems(names []string, info map[string]winInfo, front string, act func(win string, tab int)) []switchItem {
	var out []switchItem
	for i, name := range names {
		w := info[name]
		title := fmt.Sprintf("Window %d", i+1)
		if w.active >= 0 && w.active < len(w.tabs) && w.tabs[w.active] != "" {
			title = w.tabs[w.active]
		}
		win := name
		out = append(out, switchItem{Label: title, Checked: name == front, run: func() { act(win, 0) }})
		if len(w.tabs) > 1 {
			for t, tab := range w.tabs {
				n := t + 1
				out = append(out, switchItem{Label: fmt.Sprintf("Tab %d: %s", n, tab), Indent: true, run: func() { act(win, n) }})
			}
		}
	}
	if len(out) > 0 {
		out = append(out, switchItem{Sep: true})
	}
	return out
}

type switcher struct {
	mu    sync.Mutex
	timer *time.Timer
	tray  *application.SystemTray
}

// refreshSwitcher rebuilds the menu bar icon's menu and the Dock menu soon
// (changes come in bursts while tabs open and close).
func (a *App) refreshSwitcher() {
	a.sw.mu.Lock()
	defer a.sw.mu.Unlock()
	if a.sw.timer != nil {
		a.sw.timer.Stop()
	}
	a.sw.timer = time.AfterFunc(150*time.Millisecond, a.rebuildSwitcher)
}

func (a *App) currentSwitchItems() []switchItem {
	if a.app == nil {
		return nil
	}
	var names []string
	for _, w := range a.app.Window.GetAll() {
		if w.Name() != settingsWindowName {
			names = append(names, w.Name())
		}
	}
	sort.Strings(names) // window-1, window-2… in the order they were opened
	front := ""
	if cur := a.app.Window.Current(); cur != nil {
		front = cur.Name()
	}
	a.mu.Lock()
	info := make(map[string]winInfo, len(a.winInfo))
	for k, v := range a.winInfo {
		info[k] = v
	}
	a.mu.Unlock()
	items := switchItems(names, info, front, a.bringForward)
	items = append(items,
		switchItem{Label: "New Window", run: func() { a.openWindow("", "", 0) }},
		switchItem{Label: "New Tab", run: func() { a.inFront("newTab") }},
		switchItem{Label: "Show All Windows", run: a.showAll},
	)
	return items
}

// bringForward shows a window (and selects tab n when n > 0).
func (a *App) bringForward(name string, tab int) {
	w, ok := a.app.Window.GetByName(name)
	if !ok {
		return
	}
	a.app.Show()
	w.Show()
	if w.IsMinimised() {
		w.UnMinimise()
	}
	w.Focus()
	if tab > 0 {
		a.emitTo(w, eventMenu, fmt.Sprintf("tab%d", tab))
	}
}

// inFront runs a menu command in the terminal window in front (or a new
// window when there's none).
func (a *App) inFront(cmd string) {
	a.app.Show()
	if w := a.lastTerminal(); w != nil {
		w.Show()
		w.Focus()
		a.emitTo(w, eventMenu, cmd)
		return
	}
	a.openWindow("", "", 0)
}

func (a *App) showAll() {
	a.app.Show()
	for _, w := range a.app.Window.GetAll() {
		if w.Name() == settingsWindowName {
			continue
		}
		if w.IsMinimised() {
			w.UnMinimise()
		}
		w.Show()
	}
}

func (a *App) rebuildSwitcher() {
	if a.app == nil {
		return
	}
	items := a.currentSwitchItems()
	setDockMenu(items)
	if goruntime.GOOS != "darwin" {
		return // the menu bar icon is a Mac feature here; Linux builds are for development
	}
	a.sw.mu.Lock()
	defer a.sw.mu.Unlock()
	if !a.Settings().MenuBarIcon {
		if a.sw.tray != nil {
			a.sw.tray.Destroy()
			a.sw.tray = nil
		}
		return
	}
	if a.sw.tray == nil {
		a.sw.tray = a.app.SystemTray.New()
		a.sw.tray.SetTemplateIcon(trayIcon)
		a.sw.tray.SetTooltip("jamalterm: your windows and tabs")
	}
	menu := application.NewMenu()
	for _, it := range items {
		if it.Sep {
			menu.AddSeparator()
			continue
		}
		label := it.Label
		if it.Indent {
			label = "    " + label
		}
		var m *application.MenuItem
		if it.Checked {
			m = menu.AddCheckbox(label, true)
		} else {
			m = menu.Add(label)
		}
		m.SetEnabled(!it.Disabled)
		if it.run != nil {
			run := it.run
			m.OnClick(func(*application.Context) { run() })
		}
	}
	menu.AddSeparator()
	menu.Add("Hide This Icon (Settings → Appearance)").OnClick(func(*application.Context) {
		cfg := a.Settings()
		cfg.MenuBarIcon = false
		a.SaveSettings(cfg)
	})
	a.sw.tray.SetMenu(menu)
	a.sw.tray.Show()
}
