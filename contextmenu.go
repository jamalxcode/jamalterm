package main

import (
	"encoding/json"

	"github.com/wailsapp/wails/v3/pkg/application"
)

// The terminal's right-click menu: a native macOS menu, as Apple expects
// for apps like this (frontend: .pane { --custom-contextmenu: terminal }).
func (a *App) registerContextMenu() {
	m := application.NewContextMenu("terminal")
	m.AddRole(application.Copy)
	m.AddRole(application.Paste)
	item := func(label, command string) {
		m.Add(label).OnClick(func(*application.Context) { a.toFront(eventMenu, command) })
	}
	item("Select All", "selectAll")
	m.AddSeparator()
	item("Ask AI About the Selection", "aiAskSelection")
	item("Find Selection", "findSelection")
	m.AddSeparator()
	item("Split Pane Right", "splitRight")
	item("Split Pane Down", "splitDown")
	m.AddSeparator()
	item("Clear Scrollback", "clear")
	m.Update()
}

// ---- Window size and position, kept with each window's saved tabs ----------

type frame struct {
	X int `json:"x"`
	Y int `json:"y"`
	W int `json:"w"`
	H int `json:"h"`
}

// withFrame adds the window's current size and position to its saved tabs.
func withFrame(data string, w application.Window) string {
	var m map[string]json.RawMessage
	if json.Unmarshal([]byte(data), &m) != nil || m == nil {
		return data
	}
	x, y := w.RelativePosition()
	width, height := w.Size()
	if width < 200 || height < 150 {
		return data // minimised or not laid out yet
	}
	f, _ := json.Marshal(frame{X: x, Y: y, W: width, H: height})
	m["frame"] = f
	out, err := json.Marshal(m)
	if err != nil {
		return data
	}
	return string(out)
}

// frameOf reads the saved size and position, if any.
func frameOf(data string) (frame, bool) {
	var m struct {
		Frame *frame `json:"frame"`
	}
	if json.Unmarshal([]byte(data), &m) != nil || m.Frame == nil || m.Frame.W < 200 || m.Frame.H < 150 {
		return frame{}, false
	}
	return *m.Frame, true
}
