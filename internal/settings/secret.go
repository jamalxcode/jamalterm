package settings

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
)

// API keys live in the macOS Keychain, never in settings.json. On other
// systems (used for development) they are 0600 files next to it.
//
// Providers: "anthropic" (Claude), "openai" (ChatGPT), "deepseek", "xai" (Grok).

// Secrets stores the API keys.
type Secrets struct {
	fileDir string // fallback location (non-macOS)
	// useKeychain is true on macOS.
	useKeychain bool
}

func NewSecrets(settingsPath string) *Secrets {
	return &Secrets{fileDir: filepath.Dir(settingsPath), useKeychain: runtime.GOOS == "darwin"}
}

func keychainService(provider string) string { return "jamalterm-" + provider + "-api-key" }

func keyFile(provider string) string {
	if provider == "anthropic" {
		return "api-key" // name used before other providers existed
	}
	return "api-key-" + provider
}

var envVar = map[string]string{
	"anthropic": "ANTHROPIC_API_KEY",
	"openai":    "OPENAI_API_KEY",
	"deepseek":  "DEEPSEEK_API_KEY",
	"xai":       "XAI_API_KEY",
}

// APIKey is the Claude key (saved, else $ANTHROPIC_API_KEY).
func (s *Secrets) APIKey() string { return s.KeyFor("anthropic") }

// HasSavedKey reports whether a Claude key was saved in jamalterm.
func (s *Secrets) HasSavedKey() bool { return s.saved("anthropic") != "" }

// SetAPIKey saves the Claude key; "" deletes it.
func (s *Secrets) SetAPIKey(key string) error { return s.SetKeyFor("anthropic", key) }

// KeyFor returns a provider's saved key, or its environment variable.
func (s *Secrets) KeyFor(provider string) string {
	if k := s.saved(provider); k != "" {
		return k
	}
	if v, ok := envVar[provider]; ok {
		return strings.TrimSpace(os.Getenv(v))
	}
	return ""
}

// HasSavedKeyFor reports whether a key for provider was saved in jamalterm.
func (s *Secrets) HasSavedKeyFor(provider string) bool { return s.saved(provider) != "" }

func (s *Secrets) saved(provider string) string {
	if _, ok := envVar[provider]; !ok {
		return ""
	}
	if s.useKeychain {
		out, err := exec.Command("/usr/bin/security", "find-generic-password", "-s", keychainService(provider), "-w").Output()
		if err != nil {
			return ""
		}
		return strings.TrimSpace(string(out))
	}
	data, err := os.ReadFile(filepath.Join(s.fileDir, keyFile(provider)))
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(data))
}

// SetKeyFor saves a provider's key; an empty key deletes it.
func (s *Secrets) SetKeyFor(provider, key string) error {
	if _, ok := envVar[provider]; !ok {
		return errors.New("unknown AI provider " + provider)
	}
	key = strings.TrimSpace(key)
	if strings.ContainsAny(key, "\n\r\t \"'\\") {
		return errors.New("that doesn't look like an API key (it has spaces or quotes)")
	}
	if s.useKeychain {
		service := keychainService(provider)
		// Remove any old entry first; "not found" is fine.
		exec.Command("/usr/bin/security", "delete-generic-password", "-s", service).Run()
		if key == "" {
			return nil
		}
		// Sent on stdin ("security -i") so the key never shows up in the
		// process list. -U updates in place.
		cmd := exec.Command("/usr/bin/security", "-i")
		cmd.Stdin = strings.NewReader(`add-generic-password -U -a jamalterm -s ` + service + ` -w "` + key + "\"\n")
		if out, err := cmd.CombinedOutput(); err != nil || strings.Contains(string(out), "error") {
			return errors.New("Keychain: " + strings.TrimSpace(string(out)))
		}
		return nil
	}
	path := filepath.Join(s.fileDir, keyFile(provider))
	if key == "" {
		err := os.Remove(path)
		if errors.Is(err, os.ErrNotExist) {
			return nil
		}
		return err
	}
	if err := os.MkdirAll(s.fileDir, 0o700); err != nil {
		return err
	}
	return os.WriteFile(path, []byte(key+"\n"), 0o600)
}
