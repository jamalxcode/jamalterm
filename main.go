package main

import (
	"embed"

	"github.com/wailsapp/wails/v2"
	"github.com/wailsapp/wails/v2/pkg/options"
	"github.com/wailsapp/wails/v2/pkg/options/assetserver"
	"github.com/wailsapp/wails/v2/pkg/options/linux"
	"github.com/wailsapp/wails/v2/pkg/options/mac"
)

//go:embed all:frontend/dist
var assets embed.FS

// version is set at build time: -ldflags "-X main.version=1.0.0"
var version = "dev"

// Version is shown in About.
func (a *App) Version() string { return version }

func main() {
	app := NewApp()
	cfg := app.Settings()
	// See-through window: the window is transparent and the page draws a
	// semi-transparent background. Blur ("vibrancy") can only be chosen
	// when the window is created.
	app.blurAtLaunch = cfg.Blur && cfg.Opacity < 100
	bg := &options.RGBA{R: 21, G: 22, B: 30, A: 255}
	if cfg.Opacity < 100 {
		bg.A = 0
	}

	err := wails.Run(&options.App{
		Title:     "jamalterm",
		Width:     1100,
		Height:    700,
		MinWidth:  480,
		MinHeight: 300,
		AssetServer: &assetserver.Options{
			Assets: assets,
		},
		// Same as the default theme background, so there's no flash on launch.
		BackgroundColour: bg,
		Menu:             app.buildMenu(),
		OnStartup:        app.startup,
		OnShutdown:       app.shutdown,
		Bind:             []interface{}{app},
		// Lets the see-through window work in Linux development builds too.
		Linux: &linux.Options{WindowIsTranslucent: cfg.Opacity < 100},
		Mac: &mac.Options{
			// Tabs sit in a transparent, standard-height title bar, centred
			// on the window buttons (style.css: body.mac --titlebar).
			TitleBar:             mac.TitleBarHidden(),
			WebviewIsTransparent: true,
			WindowIsTranslucent:  app.blurAtLaunch,
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
