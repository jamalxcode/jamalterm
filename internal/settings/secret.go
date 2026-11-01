package settings

import (
	"encoding/json"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
)

// API keys never go in settings.json.
//
// macOS: one native Keychain item ("jamalterm" / "api-keys") holds every
// provider's key as JSON. Items an app adds through the Keychain API trust
// only that app, so other programs — including the `security` tool — must
// ask first. (jamalterm is ad-hoc signed, so after an update macOS asks
// once more: "Always Allow".) Keys saved by older versions with the
// `security` tool are moved over and the old items deleted.
//
// Other systems (development builds): 0600 files next to settings.json.
//
// Providers: "anthropic" (Claude), "openai" (ChatGPT), "deepseek", "xai"
// (Grok), "openrouter".

// Secrets stores the API keys.
type Secrets struct {
	fileDir string // fallback location (non-macOS)
	// useKeychain is true on macOS.
	useKeychain bool
	// service names the Keychain item ("jamalterm"; tests use their own).
	service string

	mu     sync.Mutex
	loaded bool
	keys   map[string]string
}

func NewSecrets(settingsPath string) *Secrets {
	return &Secrets{fileDir: filepath.Dir(settingsPath), useKeychain: nativeKeychainAvailable(), service: "jamalterm"}
}

const keychainAccount = "api-keys"

func keyFile(provider string) string {
	if provider == "anthropic" {
		return "api-key" // name used before other providers existed
	}
	return "api-key-" + provider
}

var envVar = map[string]string{
	"anthropic":  "ANTHROPIC_API_KEY",
	"openai":     "OPENAI_API_KEY",
	"deepseek":   "DEEPSEEK_API_KEY",
	"xai":        "XAI_API_KEY",
	"openrouter": "OPENROUTER_API_KEY",
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
		s.mu.Lock()
		defer s.mu.Unlock()
		s.loadLocked()
		return s.keys[provider]
	}
	data, err := os.ReadFile(filepath.Join(s.fileDir, keyFile(provider)))
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(data))
}

// loadLocked reads the Keychain item once (one prompt at most).
func (s *Secrets) loadLocked() {
	if s.loaded {
		return
	}
	s.loaded = true
	s.keys = map[string]string{}
	data, found, err := keychainGet(s.service, keychainAccount)
	if err != nil {
		return // denied or locked: behave as if no keys are saved
	}
	if found {
		json.Unmarshal(data, &s.keys)
		return
	}
	// First run of this version: move keys saved by the `security` tool.
	moved := false
	for p := range envVar {
		if k := legacyGet(s.service, p); k != "" {
			s.keys[p] = k
			moved = true
		}
	}
	if moved && s.storeLocked() == nil {
		for p := range envVar {
			legacyDelete(s.service, p)
		}
	}
}

func (s *Secrets) storeLocked() error {
	if len(s.keys) == 0 {
		return keychainDelete(s.service, keychainAccount)
	}
	data, _ := json.Marshal(s.keys)
	return keychainSet(s.service, keychainAccount, data)
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
		s.mu.Lock()
		defer s.mu.Unlock()
		s.loadLocked()
		if key == "" {
			delete(s.keys, provider)
		} else {
			s.keys[provider] = key
		}
		if err := s.storeLocked(); err != nil {
			s.loaded = false // re-read next time
			return err
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

// Keys saved by jamalterm 1.x with the `security` tool.
func legacyService(service, provider string) string { return service + "-" + provider + "-api-key" }

func legacyGet(service, provider string) string {
	out, err := exec.Command("/usr/bin/security", "find-generic-password", "-s", legacyService(service, provider), "-w").Output()
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(out))
}

func legacyDelete(service, provider string) {
	exec.Command("/usr/bin/security", "delete-generic-password", "-s", legacyService(service, provider)).Run()
}
