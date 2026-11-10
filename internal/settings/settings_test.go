package settings

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestRoundTripAndDefaults(t *testing.T) {
	path := filepath.Join(t.TempDir(), "jt", "settings.json")
	st := NewStore(path)

	got, err := st.Load()
	if err != nil || !reflect.DeepEqual(got, Defaults()) {
		t.Fatalf("missing file should give defaults: %+v %v", got, err)
	}

	got.Theme = "dracula"
	got.FontSize = 99 // out of range -> default
	got.CursorStyle = "bar"
	if err := st.Save(got); err != nil {
		t.Fatal(err)
	}
	again, err := st.Load()
	if err != nil {
		t.Fatal(err)
	}
	if again.Theme != "dracula" || again.CursorStyle != "bar" || again.FontSize != 14 {
		t.Fatalf("round trip: %+v", again)
	}

	// Fields missing from an older file keep their defaults.
	os.WriteFile(path, []byte(`{"theme":"nord"}`), 0o600)
	old, _ := st.Load()
	if old.Theme != "nord" || !old.CursorBlink || !old.ArrowBrowse || old.Opacity != 100 || old.AIModel != "claude-opus-5-5" {
		t.Fatalf("partial file: %+v", old)
	}

	// A damaged file still gives usable settings.
	os.WriteFile(path, []byte(`{nope`), 0o600)
	if bad, err := st.Load(); err == nil || !reflect.DeepEqual(bad, Defaults()) {
		t.Fatalf("damaged file: %+v %v", bad, err)
	}
}

func TestProviderKeysAreSeparate(t *testing.T) {
	s := testSecrets(t.TempDir(), "mac-1")
	t.Setenv("OPENAI_API_KEY", "")
	t.Setenv("DEEPSEEK_API_KEY", "ds-env")
	if err := s.SetKeyFor("openai", "sk-oa"); err != nil {
		t.Fatal(err)
	}
	if s.KeyFor("openai") != "sk-oa" || s.KeyFor("deepseek") != "ds-env" || s.HasSavedKeyFor("deepseek") {
		t.Fatal("keys mixed up")
	}
	if s.SetKeyFor("gemini", "x") == nil {
		t.Fatal("unknown provider accepted")
	}
	st := NewStore(filepath.Join(t.TempDir(), "s.json"))
	cfg := Defaults()
	cfg.AIProvider = "bogus"
	st.Save(cfg)
	if got, _ := st.Load(); got.AIProvider != "anthropic" {
		t.Fatalf("provider %q", got.AIProvider)
	}
}

func init() { excludeFromBackups = func(string) {} }

func testSecrets(dir, machine string) *Secrets {
	return &Secrets{dir: dir, machine: func() string { return machine }, keychainService: "jamalterm-test-none"}
}

func TestSecretsFile(t *testing.T) {
	dir := t.TempDir()
	s := testSecrets(dir, "mac-1")
	t.Setenv("ANTHROPIC_API_KEY", "from-env")
	if s.APIKey() != "from-env" || s.HasSavedKey() {
		t.Fatal("env fallback")
	}
	if err := s.SetAPIKey("  sk-ant-test-123  "); err != nil {
		t.Fatal(err)
	}
	if s.APIKey() != "sk-ant-test-123" || !s.HasSavedKey() {
		t.Fatal("saved key")
	}
	for _, f := range []string{keysFile, secretFile} {
		fi, err := os.Stat(filepath.Join(dir, f))
		if err != nil || fi.Mode().Perm() != 0o600 {
			t.Fatalf("%s: %v %v", f, fi, err)
		}
	}
	// Encrypted: the key isn't in the file.
	raw, _ := os.ReadFile(filepath.Join(dir, keysFile))
	if strings.Contains(string(raw), "sk-ant-test") {
		t.Fatal("key stored in plain text")
	}
	// A fresh reader on the same computer finds it.
	if testSecrets(dir, "mac-1").APIKey() != "sk-ant-test-123" {
		t.Fatal("not read back")
	}
	if err := s.SetAPIKey(""); err != nil || s.HasSavedKey() {
		t.Fatal("delete")
	}
	if s.SetAPIKey("a\nb") == nil {
		t.Fatal("multi-line key accepted")
	}
}

// A copy of the folder can't be read on another computer.
func TestSecretsTiedToThisComputer(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("OPENAI_API_KEY", "")
	if err := testSecrets(dir, "mac-1").SetKeyFor("openai", "sk-secret-1"); err != nil {
		t.Fatal(err)
	}
	other := testSecrets(dir, "mac-2")
	if other.KeyFor("openai") != "" || !other.Unreadable() {
		t.Fatal("another computer read the key")
	}
	// Saving there starts over instead of failing.
	if err := other.SetKeyFor("openai", "sk-new"); err != nil || other.Unreadable() {
		t.Fatal("couldn't start over")
	}
}

// Plain key files from development builds are moved in and deleted.
func TestOldKeyFilesMoved(t *testing.T) {
	dir := t.TempDir()
	os.WriteFile(filepath.Join(dir, "api-key"), []byte("sk-ant-old\n"), 0o600)
	os.WriteFile(filepath.Join(dir, "api-key-openrouter"), []byte("sk-or-old"), 0o600)
	s := testSecrets(dir, "m")
	if s.KeyFor("anthropic") != "sk-ant-old" || s.KeyFor("openrouter") != "sk-or-old" {
		t.Fatal("not moved")
	}
	if _, err := os.Stat(filepath.Join(dir, "api-key")); !os.IsNotExist(err) {
		t.Fatal("old file left behind")
	}
	if s.KeychainKeysWaiting() {
		t.Fatal("no Keychain item expected")
	}
}

func TestRemovedProviderFallsBackToClaude(t *testing.T) {
	path := filepath.Join(t.TempDir(), "settings.json")
	os.WriteFile(path, []byte(`{"aiProvider":"gemini","geminiModel":"x"}`), 0o600)
	got, err := NewStore(path).Load()
	if err != nil || got.AIProvider != "anthropic" {
		t.Fatalf("provider %q err %v", got.AIProvider, err)
	}
}

func TestHostColorsCleanedUp(t *testing.T) {
	c := Defaults()
	c.HostColors = []HostColor{{Pattern: " prod-* ", Color: "red"}, {Pattern: "", Color: "blue"}, {Pattern: "staging", Color: "pink"}}
	got := c.normalized().HostColors
	want := []HostColor{{Pattern: "prod-*", Color: "red"}, {Pattern: "staging", Color: "red"}}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

func TestNewLookSettings(t *testing.T) {
	path := filepath.Join(t.TempDir(), "s.json")
	os.WriteFile(path, []byte(`{"theme":"nord"}`), 0o600)
	got, _ := NewStore(path).Load()
	if !got.ProjectColors || !got.MenuBarIcon || !got.FinishMoments || got.FinishSound || got.CursorEffect != "none" {
		t.Fatalf("defaults for an older file: %+v", got)
	}
	c := Defaults()
	c.CursorEffect = "sparkles"
	if c.normalized().CursorEffect != "none" {
		t.Fatal("unknown cursor effect kept")
	}
}
