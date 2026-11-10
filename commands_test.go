package main

import (
	"strings"
	"testing"
)

// Every shortcut is used once, and every plain command is something the
// window knows (frontend/src/main.ts runMenu).
func TestCommandTable(t *testing.T) {
	seen := map[string]string{}
	for _, c := range commands {
		if c.Keys == "" {
			continue
		}
		if prev, ok := seen[c.Keys]; ok && prev != c.ID {
			t.Errorf("%s is used by both %s and %s", c.Keys, prev, c.ID)
		}
		seen[c.Keys] = c.ID
	}
	for _, c := range commands {
		if c.Kind == "" && c.Title == "" {
			t.Errorf("%s has no title", c.ID)
		}
	}
	if got := keysHere("Cmd+Shift+D"); !strings.Contains(got, "Shift+D") {
		t.Errorf("keysHere: %s", got)
	}
}

// The Window menu lists only the tabs the window has, by name.
func TestTabMenuLabels(t *testing.T) {
	tabs := []string{"Home — zsh", "jamalterm — vim"}
	if l, ok := tabMenuLabel(tabs, 1); !ok || l != "Tab 1: Home — zsh" {
		t.Fatalf("tab 1: %q %v", l, ok)
	}
	if l, ok := tabMenuLabel(tabs, 2); !ok || l != "Tab 2: jamalterm — vim" {
		t.Fatalf("tab 2: %q %v", l, ok)
	}
	for _, n := range []int{0, 3, 9} {
		if _, ok := tabMenuLabel(tabs, n); ok {
			t.Fatalf("tab %d shown with two tabs open", n)
		}
	}
	if l, _ := tabMenuLabel([]string{" "}, 1); l != "Tab 1" {
		t.Fatalf("unnamed tab: %q", l)
	}
	if tabNumber("tab3") != 3 || tabNumber("tab10") != 0 || tabNumber("toggleTabBar") != 0 {
		t.Fatal("tabNumber")
	}
}
