package update

import (
	"archive/zip"
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func TestNewer(t *testing.T) {
	cases := []struct {
		a, b string
		want bool
	}{
		{"1.0.2", "1.0.1", true},
		{"1.1.0", "1.0.9", true},
		{"2.0.0", "1.9.9", true},
		{"1.0.1", "1.0.1", false},
		{"1.0.0", "1.0.1", false},
		{"v1.0.1", "1.0.0", true},
		{"1.0.0", "dev", true},
		{"1.0.0", "0.0.0-dev.abc1234", true},
		{"1.0.1", "1.0.1-dev.x", false},
	}
	for _, c := range cases {
		if got := Newer(c.a, c.b); got != c.want {
			t.Errorf("Newer(%q, %q) = %v", c.a, c.b, got)
		}
	}
}

// zipApp returns a zipped jamalterm.app whose executable holds content,
// and the executable's final bytes (codesign rewrites it on macOS).
// On macOS it is a real signed bundle zipped with ditto, so the test goes
// through the same unzip, bundle id and codesign checks as a real update.
func zipApp(t *testing.T, content []byte) ([]byte, []byte) {
	if runtime.GOOS != "darwin" {
		var buf bytes.Buffer
		zw := zip.NewWriter(&buf)
		w, _ := zw.CreateHeader(&zip.FileHeader{Name: "jamalterm.app/Contents/MacOS/jamalterm", Method: zip.Deflate})
		w.Write(content)
		zw.Close()
		return buf.Bytes(), content
	}
	dir := t.TempDir()
	app := filepath.Join(dir, "jamalterm.app")
	os.MkdirAll(filepath.Join(app, "Contents", "MacOS"), 0o755)
	os.WriteFile(filepath.Join(app, "Contents", "MacOS", "jamalterm"), content, 0o755)
	os.WriteFile(filepath.Join(app, "Contents", "Info.plist"), []byte(`<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.jamalxcode.jamalterm</string>
<key>CFBundleExecutable</key><string>jamalterm</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>`), 0o644)
	if out, err := exec.Command("/usr/bin/codesign", "--force", "--deep", "--sign", "-", app).CombinedOutput(); err != nil {
		t.Fatalf("codesign: %v %s", err, out)
	}
	zipPath := filepath.Join(dir, "app.zip")
	if out, err := exec.Command("/usr/bin/ditto", "-c", "-k", "--keepParent", app, zipPath).CombinedOutput(); err != nil {
		t.Fatalf("ditto: %v %s", err, out)
	}
	data, _ := os.ReadFile(zipPath)
	signed, _ := os.ReadFile(filepath.Join(app, "Contents", "MacOS", "jamalterm"))
	return data, signed
}

// appContent is what the "new version" executable contains: a real
// program on macOS (codesign needs one), plain text elsewhere.
func appContent(t *testing.T) []byte {
	if runtime.GOOS == "darwin" {
		data, err := os.ReadFile("/usr/bin/true")
		if err != nil {
			t.Fatal(err)
		}
		return data
	}
	return []byte("new version")
}

// fakeRelease serves a GitHub-like release with a zipped jamalterm.app.
func fakeRelease(t *testing.T, content []byte, corruptSum bool) []byte {
	zipBytes, final := zipApp(t, content)
	sum := sha256.Sum256(zipBytes)
	sumHex := hex.EncodeToString(sum[:])
	if corruptSum {
		sumHex = hex.EncodeToString(make([]byte, 32))
	}
	mux := http.NewServeMux()
	var srv *httptest.Server
	mux.HandleFunc("/repos/jamalxcode/jamalterm/releases/latest", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, `{"tag_name":"v9.9.9","body":"New stuff","html_url":"https://example/rel","assets":[
		 {"name":"jamalterm-9.9.9.dmg","browser_download_url":"%[1]s/x.dmg"},
		 {"name":"jamalterm-9.9.9-macos.zip","browser_download_url":"%[1]s/app.zip"},
		 {"name":"SHA256SUMS.txt","browser_download_url":"%[1]s/sums"}]}`, srv.URL)
	})
	mux.HandleFunc("/app.zip", func(w http.ResponseWriter, r *http.Request) { w.Write(zipBytes) })
	mux.HandleFunc("/sums", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, "%s  jamalterm-9.9.9.dmg\n%s  jamalterm-9.9.9-macos.zip\n", hex.EncodeToString(make([]byte, 32)), sumHex)
	})
	srv = httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	APIBase = srv.URL
	return final
}

// oldVersion is the installed app's executable in these tests.
var oldVersion []byte

// installedApp makes the "currently installed" jamalterm.app. On macOS it
// is a real signed bundle (going back checks the signature), so its
// executable is a copy of /bin/echo; elsewhere it's the text "old".
func installedApp(t *testing.T) string {
	dir := t.TempDir()
	app := filepath.Join(dir, "jamalterm.app")
	os.MkdirAll(filepath.Join(app, "Contents", "MacOS"), 0o755)
	exe := filepath.Join(app, "Contents", "MacOS", "jamalterm")
	if runtime.GOOS != "darwin" {
		os.WriteFile(exe, []byte("old"), 0o755)
		oldVersion = []byte("old")
		return app
	}
	data, err := os.ReadFile("/bin/echo")
	if err != nil {
		t.Fatal(err)
	}
	os.WriteFile(exe, data, 0o755)
	os.WriteFile(filepath.Join(app, "Contents", "Info.plist"), []byte(`<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.jamalxcode.jamalterm</string>
<key>CFBundleExecutable</key><string>jamalterm</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>`), 0o644)
	if out, err := exec.Command("/usr/bin/codesign", "--force", "--deep", "--sign", "-", app).CombinedOutput(); err != nil {
		t.Fatalf("codesign: %v %s", err, out)
	}
	oldVersion, _ = os.ReadFile(exe)
	return app
}

func TestInstallReplacesTheApp(t *testing.T) {
	content := fakeRelease(t, appContent(t), false)
	rel, err := Latest(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if rel.Version != "9.9.9" || rel.Notes != "New stuff" || rel.zipName != "jamalterm-9.9.9-macos.zip" {
		t.Fatalf("release: %+v", rel)
	}
	app := installedApp(t)
	var calls int
	if err := Install(context.Background(), rel, app, func(done, total int64) { calls++ }); err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(filepath.Join(app, "Contents", "MacOS", "jamalterm"))
	if !bytes.Equal(got, content) || calls == 0 {
		t.Fatalf("app not replaced (%d bytes), progress calls %d", len(got), calls)
	}
	// No staging folders are left next to the app.
	entries, _ := os.ReadDir(filepath.Dir(app))
	if len(entries) != 1 {
		t.Fatalf("leftovers: %v", entries)
	}
}

func TestBadChecksumChangesNothing(t *testing.T) {
	fakeRelease(t, []byte("evil"), true)
	rel, err := Latest(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	app := installedApp(t)
	if err := Install(context.Background(), rel, app, nil); err == nil {
		t.Fatal("expected a checksum error")
	}
	got, _ := os.ReadFile(filepath.Join(app, "Contents", "MacOS", "jamalterm"))
	if !bytes.Equal(got, oldVersion) {
		t.Fatalf("app was changed (%d bytes)", len(got))
	}
}

func TestKeepAndRevert(t *testing.T) {
	content := fakeRelease(t, appContent(t), false)
	KeepPreviousIn = t.TempDir()
	defer func() { KeepPreviousIn = "" }()
	rel, err := Latest(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	app := installedApp(t)
	if PreviousApp() != "" {
		t.Fatal("nothing kept yet")
	}
	if err := Install(context.Background(), rel, app, nil); err != nil {
		t.Fatal(err)
	}
	prev := PreviousApp()
	if prev == "" {
		t.Fatal("old version not kept")
	}
	if old, _ := os.ReadFile(filepath.Join(prev, "Contents", "MacOS", "jamalterm")); !bytes.Equal(old, oldVersion) {
		t.Fatalf("kept the wrong version (%d bytes)", len(old))
	}
	// Revert: the old one is back, the new one is kept.
	if err := Revert(app); err != nil {
		t.Fatal(err)
	}
	if got, _ := os.ReadFile(filepath.Join(app, "Contents", "MacOS", "jamalterm")); !bytes.Equal(got, oldVersion) {
		t.Fatalf("after revert: wrong version (%d bytes)", len(got))
	}
	if kept, _ := os.ReadFile(filepath.Join(PreviousApp(), "Contents", "MacOS", "jamalterm")); !bytes.Equal(kept, content) {
		t.Fatal("new version not kept after revert")
	}
	entries, _ := os.ReadDir(filepath.Dir(app))
	if len(entries) != 1 {
		t.Fatalf("leftovers next to the app: %v", entries)
	}
}

func TestCompareTestVersions(t *testing.T) {
	order := []string{"0.0.0-dev.abc", "3.0.9", "3.1.0-beta.1", "3.1.0-beta.2", "3.1.0-beta.10", "3.1.0", "v3.1.1"}
	for i := range order {
		for j := range order {
			want := 0
			if i > j {
				want = 1
			} else if i < j {
				want = -1
			}
			if got := Compare(order[i], order[j]); got != want {
				t.Errorf("Compare(%s, %s) = %d, want %d", order[i], order[j], got, want)
			}
		}
	}
}

func TestLatestInBetaChannel(t *testing.T) {
	asset := func(v string) string {
		return `[{"name":"jamalterm-` + v + `-macos.zip","browser_download_url":"z"},{"name":"SHA256SUMS.txt","browser_download_url":"s"}]`
	}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch {
		case strings.HasSuffix(r.URL.Path, "/releases/latest"):
			w.Write([]byte(`{"tag_name":"v3.0.0","assets":` + asset("3.0.0") + `}`))
		case strings.HasSuffix(r.URL.Path, "/releases"):
			w.Write([]byte(`[
				{"tag_name":"v3.1.0-beta.2","prerelease":true,"assets":` + asset("3.1.0-beta.2") + `},
				{"tag_name":"v3.2.0-beta.1","draft":true,"assets":` + asset("3.2.0-beta.1") + `},
				{"tag_name":"v3.1.0-beta.1","prerelease":true,"assets":` + asset("3.1.0-beta.1") + `},
				{"tag_name":"v3.0.0","assets":` + asset("3.0.0") + `}]`))
		default:
			http.NotFound(w, r)
		}
	}))
	defer srv.Close()
	old := APIBase
	APIBase = srv.URL
	defer func() { APIBase = old }()

	stable, err := LatestIn(context.Background(), false)
	if err != nil || stable.Version != "3.0.0" {
		t.Fatalf("stable = %q, %v", stable.Version, err)
	}
	beta, err := LatestIn(context.Background(), true)
	if err != nil || beta.Version != "3.1.0-beta.2" {
		t.Fatalf("beta = %q, %v (drafts must be skipped)", beta.Version, err)
	}
}
