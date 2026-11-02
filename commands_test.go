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
