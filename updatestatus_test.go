package main

import (
	"strings"
	"testing"
)

func TestWhatsNew(t *testing.T) {
	old := version
	defer func() { version = old }()
	version = "3.2.0"
	a := &App{}
	got := a.WhatsNew("3.0.0")
	if !strings.Contains(got, "## 3.2.0") || !strings.Contains(got, "## 3.1.0") || strings.Contains(got, "## 3.0.0") {
		t.Fatalf("got:\n%.300s", got)
	}
	if a.WhatsNew("3.2.0") != "" || a.WhatsNew("") != "" {
		t.Fatal("nothing new expected")
	}
}
