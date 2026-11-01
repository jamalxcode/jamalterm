package main

import (
	"embed"
	"runtime"

	"github.com/wailsapp/wails/v2"
	"github.com/wailsapp/wails/v2/pkg/menu"
	"github.com/wailsapp/wails/v2/pkg/options"
	"github.com/wailsapp/wails/v2/pkg/options/assetserver"
	"github.com/wailsapp/wails/v2/pkg/options/mac"
)

//go:embed all:frontend/dist
var assets embed.FS

func main() {
	app := NewApp()

	// macOS needs an Edit menu, otherwise Cmd+C / Cmd+V / Cmd+A do nothing
	// inside the web view.
	appMenu := menu.NewMenu()
	if runtime.GOOS == "darwin" {
		appMenu.Append(menu.AppMenu())
		appMenu.Append(menu.EditMenu())
		appMenu.Append(menu.WindowMenu())
	}

	err := wails.Run(&options.App{
		Title:     "jamalterm",
		Width:     960,
		Height:    620,
		MinWidth:  400,
		MinHeight: 240,
		AssetServer: &assetserver.Options{
			Assets: assets,
		},
		// Same as the theme background, so there's no flash on launch.
		BackgroundColour: &options.RGBA{R: 21, G: 22, B: 30, A: 255},
		Menu:             appMenu,
		OnStartup:        app.startup,
		OnShutdown:       app.shutdown,
		Bind:             []interface{}{app},
		Mac: &mac.Options{
			// Content runs under a transparent title bar, like modern terminals.
			TitleBar:             mac.TitleBarHiddenInset(),
			WebviewIsTransparent: true,
			WindowIsTranslucent:  false,
			About: &mac.AboutInfo{
				Title:   "jamalterm",
				Message: "A terminal with a free-floating cursor.",
			},
		},
	})
	if err != nil {
		println("Error:", err.Error())
	}
}
