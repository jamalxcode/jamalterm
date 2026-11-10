package main

import (
	"context"
	"fmt"
	"strings"

	"jamalterm/internal/ai"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// roles maps the command table's Role names to macOS's standard items.
var roles = map[string]application.Role{
	"about": application.About, "services": application.ServicesMenu,
	"hide": application.Hide, "hideOthers": application.HideOthers, "showAll": application.UnHide,
	"quit": application.Quit, "undo": application.Undo, "redo": application.Redo,
	"cut": application.Cut, "copy": application.Copy, "paste": application.Paste,
	"minimise": application.Minimise, "zoom": application.Zoom, "front": application.Front,
	"fullscreen": application.FullScreen,
}

// buildMenu creates the menu bar from the command table (commands.go).
// Plain items send a "menu" event to the window in front (frontend/src/
// main.ts, runMenu); native ones are done here; roles are macOS's own.
func (a *App) buildMenu() *application.Menu {
	m := a.app.NewMenu()
	menus := map[string]*application.Menu{}
	var order []string
	for _, c := range commands {
		sub := menus[c.Menu]
		if sub == nil {
			sub = application.NewMenu()
			menus[c.Menu] = sub
			order = append(order, c.Menu)
		} else if c.Sep {
			sub.AddSeparator()
		}
		switch c.Kind {
		case "role":
			sub.AddRole(roles[c.Role])
		case "check":
			a.secureItem = sub.AddCheckbox(c.Title, a.Settings().SecureInput)
			a.secureItem.OnClick(func(ctx *application.Context) {
				cfg := a.Settings()
				cfg.SecureInput = ctx.IsChecked()
				a.SaveSettings(cfg)
			})
		case "providers":
			// Switch assistants on the fly: ⌥⌘1 Claude, ⌥⌘2 ChatGPT, …
			current := a.Settings().AIProvider
			for i, p := range ai.Providers {
				id := p.ID
				it := sub.AddRadio("Use "+p.Name, id == current).SetAccelerator(keysHere("Cmd+Option+" + string(rune('1'+i))))
				it.OnClick(func(*application.Context) {
					if err := a.SetAIProvider(id); err != nil {
						a.toFront(eventMenu, "toast:"+err.Error())
					}
				})
				a.providerItems[id] = it
			}
		default:
			var it *application.MenuItem
			if c.Kind == "toggle" {
				it = sub.AddCheckbox(c.Title, false)
			} else {
				it = sub.Add(c.Title)
			}
			a.menuItems[c.ID] = it
			if _, ok := tabMenuLabel([]string{""}, tabNumber(c.ID)); !ok && tabNumber(c.ID) > 0 {
				it.SetHidden(true) // until the window says how many tabs it has
			}
			if k := keysHere(c.Keys); k != "" {
				it.SetAccelerator(k)
			}
			if f := a.native(c.ID); f != nil {
				it.OnClick(func(*application.Context) { f() })
			} else {
				id := c.ID
				it.OnClick(func(*application.Context) { a.toFront(eventMenu, id) })
			}
		}
	}
	for _, name := range order {
		item := application.NewSubmenu(name, menus[name])
		switch name {
		case "Window":
			item.SetRole(application.WindowMenu) // macOS lists the open windows here
		case "Help":
			item.SetRole(application.HelpMenu) // macOS adds its search field
		}
		m.Append(application.NewMenuFromItems(item))
	}
	return m
}

// MenuState is what the window in front tells the menu bar, so items say
// what they will do ("Close Pane" vs "Close Tab", "Hide Tab Bar"…).
type MenuState struct {
	Split     bool `json:"split"`
	TabBar    bool `json:"tabBar"`
	Broadcast bool `json:"broadcast"`
	// Tabs names the window's tabs in order ("Home — zsh"). The Window menu
	// lists only these, so there's no "Select Tab 7" with two tabs open.
	Tabs []string `json:"tabs"`
	// Active is the index of the tab in front.
	Active int `json:"active"`
}

// tabNumber is n for the "tabN" commands, else 0.
func tabNumber(id string) int {
	if len(id) == 4 && strings.HasPrefix(id, "tab") && id[3] >= '1' && id[3] <= '9' {
		return int(id[3] - '0')
	}
	return 0
}

// tabMenuLabel is what the Window menu shows for tab n (from 1); ok is false
// when the window has fewer tabs, and the item is hidden.
func tabMenuLabel(tabs []string, n int) (label string, ok bool) {
	if n < 1 || n > len(tabs) {
		return "", false
	}
	name := strings.TrimSpace(tabs[n-1])
	if name == "" {
		return fmt.Sprintf("Tab %d", n), true
	}
	return fmt.Sprintf("Tab %d: %s", n, name), true
}

// SetMenuState updates the menu bar for the window in front.
func (a *App) SetMenuState(ctx context.Context, s MenuState) {
	// Every window's tabs go in the menu bar icon and the Dock menu.
	if w := winOf(ctx); w != nil {
		a.mu.Lock()
		a.winInfo[w.Name()] = winInfo{tabs: s.Tabs, active: s.Active}
		a.mu.Unlock()
		a.refreshSwitcher()
	}
	if w := winOf(ctx); w != nil {
		if cur := a.app.Window.Current(); cur != nil && cur.Name() != w.Name() {
			return // only the window in front decides
		}
	}
	if it := a.menuItems["closeTab"]; it != nil {
		if s.Split {
			it.SetLabel("Close Pane")
		} else {
			it.SetLabel("Close Tab")
		}
	}
	if it := a.menuItems["toggleTabBar"]; it != nil {
		if s.TabBar {
			it.SetLabel("Hide Tab Bar")
		} else {
			it.SetLabel("Show Tab Bar")
		}
	}
	if it := a.menuItems["broadcast"]; it != nil {
		it.SetChecked(s.Broadcast)
	}
	for n := 1; n <= 9; n++ {
		if it := a.menuItems[fmt.Sprintf("tab%d", n)]; it != nil {
			label, ok := tabMenuLabel(s.Tabs, n)
			if ok {
				it.SetLabel(label)
			}
			it.SetHidden(!ok)
		}
	}
}
