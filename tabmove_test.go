package main

import (
	"fmt"
	"reflect"
	"testing"

	"github.com/wailsapp/wails/v3/pkg/application"
)

func TestTabIDs(t *testing.T) {
	cases := map[string][]int{
		`{"tab":{"id":3,"cwd":"/x"}}`: {3},
		`{"dir":"row","ratio":0.5,"a":{"tab":{"id":4}},"b":{"dir":"column","a":{"tab":{"id":5}},"b":{"tab":{"id":6}}}}`: {4, 5, 6},
		`{"tab":{"cwd":"/no-id"}}`: nil,
		`not json`:                 nil,
	}
	for in, want := range cases {
		if got := tabIDs(in); !reflect.DeepEqual(got, want) {
			t.Errorf("tabIDs(%s) = %v, want %v", in, got, want)
		}
	}
}

// Output follows a tab to the window it moved to.
func TestTabWindowFollowsOwner(t *testing.T) {
	a := &App{owner: map[int]string{}}
	if a.tabWindow(1, nil) != nil {
		t.Fatal("no owner: the fallback")
	}
}

func TestSwitchItems(t *testing.T) {
	info := map[string]winInfo{
		"window-1": {tabs: []string{"Home — zsh", "jamalterm — vim"}, active: 1},
		"window-2": {tabs: []string{"srv — zsh"}},
	}
	var picked []string
	items := switchItems([]string{"window-1", "window-2"}, info, "window-2", func(w string, tab int) {
		picked = append(picked, fmt.Sprintf("%s#%d", w, tab))
	})
	var labels []string
	for _, it := range items {
		l := it.Label
		if it.Indent {
			l = "  " + l
		}
		if it.Checked {
			l += " ✓"
		}
		if it.Sep {
			l = "---"
		}
		labels = append(labels, l)
	}
	want := []string{"jamalterm — vim", "  Tab 1: Home — zsh", "  Tab 2: jamalterm — vim", "srv — zsh ✓", "---"}
	if !reflect.DeepEqual(labels, want) {
		t.Fatalf("got %q\nwant %q", labels, want)
	}
	items[2].run()
	items[3].run()
	if !reflect.DeepEqual(picked, []string{"window-1#2", "window-2#0"}) {
		t.Fatalf("picked %v", picked)
	}
	// A window that hasn't reported yet still gets a line.
	if got := switchItems([]string{"window-9"}, nil, "", func(string, int) {}); got[0].Label != "Window 1" {
		t.Fatalf("unnamed window: %q", got[0].Label)
	}
}

func TestWindowAt(t *testing.T) {
	frames := map[string]application.Rect{
		"window-1": {X: 0, Y: 0, Width: 800, Height: 600},
		"window-2": {X: 900, Y: 100, Width: 800, Height: 600},
	}
	cases := []struct {
		x, y int
		want string
	}{
		{950, 120, "window-2"}, // over the other window
		{100, 100, ""},         // over the window it came from
		{850, 50, ""},          // between windows: a new window
	}
	for _, c := range cases {
		if got := windowAt(frames, "window-1", c.x, c.y); got != c.want {
			t.Errorf("windowAt(%d,%d) = %q, want %q", c.x, c.y, got, c.want)
		}
	}
}
