package main

import (
	"errors"
	"fmt"
	"image"
	_ "image/jpeg" // wallpaper pictures
	_ "image/png"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	goruntime "runtime"
	"sort"
	"strings"
)

var lineSuffix = regexp.MustCompile(`(:\d+)+$`)

// resolvePath turns a path seen in the terminal into a real file: "~/x",
// relative to the tab's folder, or absolute. ok is false if nothing's there.
func resolvePath(dir, p, home string) (string, bool) {
	p = strings.Trim(strings.TrimSpace(p), `"'`+"`(),:;")
	p = lineSuffix.ReplaceAllString(p, "") // "main.go:12:3", as compilers print it
	if p == "" {
		return "", false
	}
	switch {
	case p == "~":
		p = home
	case strings.HasPrefix(p, "~/"):
		p = filepath.Join(home, p[2:])
	case !filepath.IsAbs(p):
		if dir == "" {
			return "", false
		}
		p = filepath.Join(dir, p)
	}
	if _, err := os.Stat(p); err != nil {
		return "", false
	}
	return p, true
}

// QuickLook previews a file or folder named in the terminal (Space in
// browse mode), like pressing Space in Finder.
func (a *App) QuickLook(dir, p string) error {
	home, _ := os.UserHomeDir()
	full, ok := resolvePath(dir, p, home)
	if !ok {
		return fmt.Errorf("no file called %q here", p)
	}
	if goruntime.GOOS == "darwin" {
		return exec.Command("/usr/bin/qlmanage", "-p", full).Start()
	}
	return a.OpenPath(full)
}

// WallpaperColors returns colors taken from the desktop picture, darkest
// first then the most colorful ones, as "#rrggbb" (for the "Match my
// wallpaper" theme). macOS only.
func (a *App) WallpaperColors() ([]string, error) {
	if goruntime.GOOS != "darwin" {
		return nil, errors.New("only on macOS")
	}
	out, err := exec.Command("/usr/bin/osascript", "-e", `tell application "System Events" to get picture of current desktop`).Output()
	if err != nil {
		return nil, fmt.Errorf("couldn't find the desktop picture: %w", err)
	}
	pic := strings.TrimSpace(string(out))
	// A small PNG copy (desktop pictures are often HEIC, which Go can't read).
	tmp, err := os.MkdirTemp("", "jt-wallpaper")
	if err != nil {
		return nil, err
	}
	defer os.RemoveAll(tmp)
	small := filepath.Join(tmp, "w.png")
	if err := exec.Command("/usr/bin/sips", "-s", "format", "png", "-Z", "96", pic, "--out", small).Run(); err != nil {
		return nil, fmt.Errorf("couldn't read the desktop picture: %w", err)
	}
	f, err := os.Open(small)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	img, _, err := image.Decode(f)
	if err != nil {
		return nil, err
	}
	return paletteOf(img), nil
}

// paletteOf picks colors from a picture: [average, darkest, then up to six
// colorful ones, most colorful first].
func paletteOf(img image.Image) []string {
	type px struct{ r, g, b float64 }
	var all []px
	var sr, sg, sb float64
	bnd := img.Bounds()
	for y := bnd.Min.Y; y < bnd.Max.Y; y++ {
		for x := bnd.Min.X; x < bnd.Max.X; x++ {
			r, g, b, _ := img.At(x, y).RGBA()
			p := px{float64(r >> 8), float64(g >> 8), float64(b >> 8)}
			all = append(all, p)
			sr, sg, sb = sr+p.r, sg+p.g, sb+p.b
		}
	}
	if len(all) == 0 {
		return nil
	}
	n := float64(len(all))
	hex := func(p px) string { return fmt.Sprintf("#%02x%02x%02x", int(p.r), int(p.g), int(p.b)) }
	lum := func(p px) float64 { return 0.2126*p.r + 0.7152*p.g + 0.0722*p.b }
	sat := func(p px) float64 {
		mx := math.Max(p.r, math.Max(p.g, p.b))
		mn := math.Min(p.r, math.Min(p.g, p.b))
		if mx == 0 {
			return 0
		}
		return (mx - mn) / mx
	}
	out := []string{hex(px{sr / n, sg / n, sb / n})}
	darkest := all[0]
	for _, p := range all {
		if lum(p) < lum(darkest) {
			darkest = p
		}
	}
	out = append(out, hex(darkest))
	// Colorful ones, skipping near-duplicates.
	sort.SliceStable(all, func(i, j int) bool { return sat(all[i])*lum(all[i]) > sat(all[j])*lum(all[j]) })
	var picked []px
	for _, p := range all {
		if sat(p) < 0.25 || len(picked) == 6 {
			continue
		}
		dup := false
		for _, q := range picked {
			if math.Abs(p.r-q.r)+math.Abs(p.g-q.g)+math.Abs(p.b-q.b) < 90 {
				dup = true
				break
			}
		}
		if !dup {
			picked = append(picked, p)
		}
	}
	for _, p := range picked {
		out = append(out, hex(p))
	}
	return out
}
