package settings

import (
	"os"
	"path/filepath"
	"testing"
)

func TestRoundTripAndDefaults(t *testing.T) {
	path := filepath.Join(t.TempDir(), "jt", "settings.json")
	st := NewStore(path)

	got, err := st.Load()
	if err != nil || got != Defaults() {
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
	if bad, err := st.Load(); err == nil || bad != Defaults() {
		t.Fatalf("damaged file: %+v %v", bad, err)
	}
}

func TestProviderKeysAreSeparate(t *testing.T) {
	s := &Secrets{fileDir: t.TempDir()}
	t.Setenv("OPENAI_API_KEY", "")
	t.Setenv("DEEPSEEK_API_KEY", "ds-env")
	if err := s.SetKeyFor("openai", "sk-oa"); err != nil {
		t.Fatal(err)
	}
	if s.KeyFor("openai") != "sk-oa" || s.KeyFor("deepseek") != "ds-env" || s.HasSavedKeyFor("deepseek") {
		t.Fatal("keys mixed up")
	}
	if s.SetKeyFor("mistral", "x") == nil {
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

func TestSecretsFile(t *testing.T) {
	dir := t.TempDir()
	s := &Secrets{fileDir: dir}
	t.Setenv("ANTHROPIC_API_KEY", "from-env")
	if s.APIKey() != "from-env" || s.HasSavedKey() {
		t.Fatal("env fallback")
	}
	if err := s.SetAPIKey("  sk-test  "); err != nil {
		t.Fatal(err)
	}
	if s.APIKey() != "sk-test" || !s.HasSavedKey() {
		t.Fatal("saved key")
	}
	if fi, _ := os.Stat(filepath.Join(dir, "api-key")); fi.Mode().Perm() != 0o600 {
		t.Fatalf("key file mode %v", fi.Mode().Perm())
	}
	if err := s.SetAPIKey(""); err != nil || s.HasSavedKey() {
		t.Fatal("delete")
	}
	if s.SetAPIKey("a\nb") == nil {
		t.Fatal("multi-line key accepted")
	}
}
