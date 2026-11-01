// Package update lets jamalterm update itself from GitHub Releases.
//
// It downloads the release's .zip, checks it against SHA256SUMS.txt from the
// same release, checks the code signature and bundle id, then swaps the
// .app in place (rolling back if anything fails). The caller relaunches.
package update

import (
	"archive/zip"
	"bufio"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"time"
)

const (
	repo     = "jamalxcode/jamalterm"
	bundleID = "com.jamalxcode.jamalterm"
)

// APIBase can be changed in tests.
var APIBase = "https://api.github.com"

// Release is the newest published version.
type Release struct {
	Version string `json:"version"` // "1.0.2"
	Notes   string `json:"notes"`
	URL     string `json:"url"` // release page
	zipURL  string
	zipName string
	sumsURL string
}

var client = &http.Client{Timeout: 5 * time.Minute}

// Latest asks GitHub for the newest release.
func Latest(ctx context.Context) (Release, error) {
	req, _ := http.NewRequestWithContext(ctx, "GET", APIBase+"/repos/"+repo+"/releases/latest", nil)
	req.Header.Set("Accept", "application/vnd.github+json")
	req.Header.Set("User-Agent", "jamalterm-updater")
	resp, err := client.Do(req)
	if err != nil {
		return Release{}, fmt.Errorf("couldn't reach GitHub: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return Release{}, fmt.Errorf("GitHub answered %s", resp.Status)
	}
	var r struct {
		Tag    string `json:"tag_name"`
		Body   string `json:"body"`
		URL    string `json:"html_url"`
		Assets []struct {
			Name string `json:"name"`
			URL  string `json:"browser_download_url"`
		} `json:"assets"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&r); err != nil {
		return Release{}, err
	}
	rel := Release{Version: strings.TrimPrefix(r.Tag, "v"), Notes: r.Body, URL: r.URL}
	for _, a := range r.Assets {
		switch {
		case strings.HasSuffix(a.Name, "-macos.zip"):
			rel.zipURL, rel.zipName = a.URL, a.Name
		case a.Name == "SHA256SUMS.txt":
			rel.sumsURL = a.URL
		}
	}
	if rel.zipURL == "" || rel.sumsURL == "" {
		return rel, errors.New("the latest release has no macOS download yet")
	}
	return rel, nil
}

// Newer reports whether version a is newer than b ("1.2.0" > "1.1.9").
// Development builds ("dev", "0.0.0-dev.abc") are older than any release.
func Newer(a, b string) bool {
	pa, pb := parse(a), parse(b)
	for i := 0; i < 3; i++ {
		if pa[i] != pb[i] {
			return pa[i] > pb[i]
		}
	}
	return false
}

func parse(v string) [3]int {
	var out [3]int
	v = strings.TrimPrefix(v, "v")
	if i := strings.IndexAny(v, "-+"); i >= 0 {
		v = v[:i]
	}
	for i, p := range strings.SplitN(v, ".", 3) {
		n, err := strconv.Atoi(p)
		if err != nil {
			return [3]int{}
		}
		out[i] = n
	}
	return out
}

// AppBundle is the .app this program runs from, e.g. /Applications/jamalterm.app.
func AppBundle() (string, error) {
	exe, err := os.Executable()
	if err != nil {
		return "", err
	}
	if p, err := filepath.EvalSymlinks(exe); err == nil {
		exe = p
	}
	// .../jamalterm.app/Contents/MacOS/jamalterm
	app := filepath.Dir(filepath.Dir(filepath.Dir(exe)))
	if !strings.HasSuffix(app, ".app") {
		return "", errors.New("not running from an installed jamalterm.app (development build)")
	}
	return app, nil
}

// Progress is called while downloading.
type Progress func(done, total int64)

// Install downloads rel and replaces the app at appPath with it.
func Install(ctx context.Context, rel Release, appPath string, progress Progress) error {
	parent := filepath.Dir(appPath)
	// Stage next to the app so the final swap is a rename on the same disk.
	stage, err := os.MkdirTemp(parent, ".jamalterm-update-")
	if err != nil {
		return fmt.Errorf("can't write to %s (%v); download the new version from the release page instead", parent, err)
	}
	defer os.RemoveAll(stage)

	want, err := expectedSum(ctx, rel)
	if err != nil {
		return err
	}
	zipPath := filepath.Join(stage, rel.zipName)
	got, err := download(ctx, rel.zipURL, zipPath, progress)
	if err != nil {
		return err
	}
	if !strings.EqualFold(got, want) {
		return errors.New("the download is damaged (checksum mismatch); nothing was changed")
	}

	if err := unzip(zipPath, stage); err != nil {
		return fmt.Errorf("couldn't unpack the update: %w", err)
	}
	newApp := filepath.Join(stage, "jamalterm.app")
	if err := verify(newApp); err != nil {
		return err
	}

	// Swap: old -> backup, new -> app. Put the old one back on failure.
	backup := filepath.Join(stage, "previous.app")
	if err := os.Rename(appPath, backup); err != nil {
		return fmt.Errorf("couldn't move the old version aside: %w", err)
	}
	if err := os.Rename(newApp, appPath); err != nil {
		if rerr := os.Rename(backup, appPath); rerr != nil {
			return fmt.Errorf("update failed (%v) and the old app couldn't be restored (%v); reinstall from the release page", err, rerr)
		}
		return fmt.Errorf("couldn't move the new version in place: %w", err)
	}
	// Keep the old version so the user can switch back (Revert). The
	// running process keeps its open files, so moving it is safe.
	keepPrevious(backup)
	if runtime.GOOS == "darwin" {
		exec.Command("/usr/bin/xattr", "-dr", "com.apple.quarantine", appPath).Run()
	}
	return nil
}

// KeepPreviousIn is where the replaced version is kept ("" = don't keep).
var KeepPreviousIn string

func keepPrevious(app string) {
	if KeepPreviousIn == "" {
		return
	}
	dst := filepath.Join(KeepPreviousIn, "jamalterm.app")
	os.RemoveAll(dst)
	if os.MkdirAll(KeepPreviousIn, 0o755) == nil {
		os.Rename(app, dst) // different disk: just don't keep it
	}
}

// PreviousApp is the kept older version, or "".
func PreviousApp() string {
	if KeepPreviousIn == "" {
		return ""
	}
	p := filepath.Join(KeepPreviousIn, "jamalterm.app")
	if _, err := os.Stat(filepath.Join(p, "Contents", "MacOS", "jamalterm")); err != nil {
		return ""
	}
	return p
}

// AppVersion reads an app bundle's version ("" if unknown).
func AppVersion(app string) string {
	if runtime.GOOS != "darwin" {
		return ""
	}
	out, err := exec.Command("/usr/libexec/PlistBuddy", "-c", "Print :CFBundleShortVersionString", filepath.Join(app, "Contents", "Info.plist")).Output()
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(out))
}

// Revert swaps the kept older version back in. The version being replaced
// is kept in turn, so reverting again goes forward.
func Revert(appPath string) error {
	prev := PreviousApp()
	if prev == "" {
		return errors.New("there's no earlier version to go back to")
	}
	if err := verify(prev); err != nil {
		return err
	}
	stage, err := os.MkdirTemp(filepath.Dir(appPath), ".jamalterm-revert-")
	if err != nil {
		return fmt.Errorf("can't write to %s: %v", filepath.Dir(appPath), err)
	}
	defer os.RemoveAll(stage)
	current := filepath.Join(stage, "current.app")
	if err := os.Rename(appPath, current); err != nil {
		return fmt.Errorf("couldn't move the current version aside: %w", err)
	}
	if err := os.Rename(prev, appPath); err != nil {
		// Different disk: copy instead of rename.
		if cerr := exec.Command("/usr/bin/ditto", prev, appPath).Run(); cerr != nil {
			os.Rename(current, appPath)
			return fmt.Errorf("couldn't put the earlier version in place: %w", err)
		}
		os.RemoveAll(prev)
	}
	keepPrevious(current)
	return nil
}

func expectedSum(ctx context.Context, rel Release) (string, error) {
	req, _ := http.NewRequestWithContext(ctx, "GET", rel.sumsURL, nil)
	resp, err := client.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return "", fmt.Errorf("couldn't get the checksums: %s", resp.Status)
	}
	sc := bufio.NewScanner(resp.Body)
	for sc.Scan() {
		f := strings.Fields(sc.Text())
		if len(f) == 2 && strings.TrimPrefix(f[1], "*") == rel.zipName && len(f[0]) == 64 {
			return f[0], nil
		}
	}
	return "", errors.New("the release has no checksum for the download")
}

func download(ctx context.Context, url, dest string, progress Progress) (string, error) {
	req, _ := http.NewRequestWithContext(ctx, "GET", url, nil)
	resp, err := client.Do(req)
	if err != nil {
		return "", fmt.Errorf("download failed: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return "", fmt.Errorf("download failed: %s", resp.Status)
	}
	f, err := os.Create(dest)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha256.New()
	total := resp.ContentLength
	var done int64
	buf := make([]byte, 64*1024)
	for {
		n, rerr := resp.Body.Read(buf)
		if n > 0 {
			if _, err := f.Write(buf[:n]); err != nil {
				return "", err
			}
			h.Write(buf[:n])
			done += int64(n)
			if progress != nil {
				progress(done, total)
			}
		}
		if rerr == io.EOF {
			break
		}
		if rerr != nil {
			return "", fmt.Errorf("download failed: %w", rerr)
		}
	}
	return hex.EncodeToString(h.Sum(nil)), f.Close()
}

func unzip(zipPath, dest string) error {
	if runtime.GOOS == "darwin" {
		// ditto keeps permissions, symlinks and extended attributes exactly.
		out, err := exec.Command("/usr/bin/ditto", "-x", "-k", zipPath, dest).CombinedOutput()
		if err != nil {
			return fmt.Errorf("%v: %s", err, out)
		}
		return nil
	}
	r, err := zip.OpenReader(zipPath)
	if err != nil {
		return err
	}
	defer r.Close()
	for _, f := range r.File {
		target := filepath.Join(dest, f.Name)
		if !strings.HasPrefix(target, filepath.Clean(dest)+string(os.PathSeparator)) {
			return fmt.Errorf("bad path in zip: %s", f.Name)
		}
		if f.FileInfo().IsDir() {
			os.MkdirAll(target, 0o755)
			continue
		}
		os.MkdirAll(filepath.Dir(target), 0o755)
		rc, err := f.Open()
		if err != nil {
			return err
		}
		w, err := os.OpenFile(target, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, f.Mode().Perm())
		if err != nil {
			rc.Close()
			return err
		}
		_, err = io.Copy(w, rc)
		rc.Close()
		w.Close()
		if err != nil {
			return err
		}
	}
	return nil
}

// verify checks the unpacked app is really jamalterm and intact.
func verify(app string) error {
	if _, err := os.Stat(filepath.Join(app, "Contents", "MacOS", "jamalterm")); err != nil {
		return errors.New("the update doesn't contain jamalterm.app")
	}
	if runtime.GOOS != "darwin" {
		return nil
	}
	id, err := exec.Command("/usr/libexec/PlistBuddy", "-c", "Print :CFBundleIdentifier", filepath.Join(app, "Contents", "Info.plist")).Output()
	if err != nil || strings.TrimSpace(string(id)) != bundleID {
		return errors.New("the update isn't a jamalterm app; nothing was changed")
	}
	if out, err := exec.Command("/usr/bin/codesign", "--verify", "--deep", "--strict", app).CombinedOutput(); err != nil {
		return fmt.Errorf("the update's code signature is broken; nothing was changed (%s)", strings.TrimSpace(string(out)))
	}
	return nil
}

// Relaunch starts the app again a moment after this process exits.
func Relaunch(appPath string) error {
	if runtime.GOOS != "darwin" {
		return errors.New("relaunch is only supported on macOS")
	}
	cmd := exec.Command("/bin/sh", "-c", `sleep 1; /usr/bin/open -n "$0"`, appPath)
	cmd.Stdin, cmd.Stdout, cmd.Stderr = nil, nil, nil
	return cmd.Start()
}
