package main

import (
	"jamalterm/internal/ai"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// buildMenu creates the menu bar from the command table (commands.go).
// Plain items send a "menu" event to the window in front (frontend/src/
// main.ts, runMenu); native ones are done here.
func (a *App) buildMenu() *application.Menu {
	m := a.app.NewMenu()
	menus := map[string]*application.Menu{}
	for _, c := range commands {
		if c.Kind == "edit" {
			m.AddRole(application.EditMenu)
			continue
		}
		sub := menus[c.Menu]
		if sub == nil {
			sub = m.AddSubmenu(c.Menu)
			menus[c.Menu] = sub
		} else if c.Sep {
			sub.AddSeparator()
		}
		switch c.Kind {
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
			it := sub.Add(c.Title)
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
	return m
}
