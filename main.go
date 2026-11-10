package main

import (
	"embed"
	"log"

	"github.com/wailsapp/wails/v3/pkg/application"
)

//go:embed all:frontend/dist
var assets embed.FS

// version is set at build time: -ldflags "-X main.version=1.0.0"
var version = "dev"

// Version is shown in About.
func (a *App) Version() string { return version }

func main() {
	a := NewApp()
	cfg := a.Settings()
	// Blur ("vibrancy") behind a see-through window can only be chosen
	// when a window is created; it applies to windows opened afterwards.
	a.blurAtLaunch = cfg.Blur && cfg.Opacity < 100

	app := application.New(application.Options{
		Name:        "jamalterm",
		Description: "A terminal with a free-floating cursor",
		Services:    []application.Service{application.NewService(a)},
		Assets:      application.AssetOptions{Handler: application.BundledAssetFileServer(assets)},
		Mac: application.MacOptions{
			// Closing the last window quits, like before.
			ApplicationShouldTerminateAfterLastWindowClosed: true,
		},
		ShouldQuit: func() bool {
			if !a.shouldQuit() {
				return false // the window asks first
			}
			a.setQuitting()
			return true
		},
		OnShutdown: a.shutdown,
	})
	a.app = app
	app.Menu.Set(a.buildMenu())
	a.registerContextMenu()
	a.openStartWindows()
	if err := app.Run(); err != nil {
		log.Fatal(err)
	}
}
