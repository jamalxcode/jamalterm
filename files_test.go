package main

import (
	"image"
	"image/color"
	"os"
	"path/filepath"
	"testing"
)

func TestResolvePath(t *testing.T) {
	home := t.TempDir()
	dir := filepath.Join(home, "proj")
	os.MkdirAll(dir, 0o755)
	os.WriteFile(filepath.Join(dir, "main.go"), nil, 0o644)
	cases := []struct {
		in   string
		want string
		ok   bool
	}{
		{"main.go", filepath.Join(dir, "main.go"), true},
		{"main.go:12:", filepath.Join(dir, "main.go"), true}, // as compilers print it
		{"'main.go'", filepath.Join(dir, "main.go"), true},
		{"~/proj", dir, true},
		{"~", home, true},
		{dir, dir, true},
		{"nope.txt", "", false},
		{"", "", false},
	}
	for _, c := range cases {
		got, ok := resolvePath(dir, c.in, home)
		if got != c.want || ok != c.ok {
			t.Errorf("resolvePath(%q) = %q %v, want %q %v", c.in, got, ok, c.want, c.ok)
		}
	}
}

func TestPaletteOf(t *testing.T) {
	img := image.NewRGBA(image.Rect(0, 0, 10, 10))
	for y := 0; y < 10; y++ {
		for x := 0; x < 10; x++ {
			c := color.RGBA{20, 20, 30, 255} // dark navy
			if x < 3 {
				c = color.RGBA{230, 120, 30, 255} // orange
			}
			img.Set(x, y, c)
		}
	}
	p := paletteOf(img)
	if len(p) < 3 {
		t.Fatalf("palette %v", p)
	}
	if p[1] != "#14141e" {
		t.Errorf("darkest = %s", p[1])
	}
	if p[2] != "#e6781e" {
		t.Errorf("most colorful = %s", p[2])
	}
}
