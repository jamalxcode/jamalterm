package settings

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
)

// The Claude API key lives in the macOS Keychain, never in settings.json.
// On other systems (used for development) it is a 0600 file next to it.

const keychainService = "jamalterm-anthropic-api-key"

// Secrets stores the API key.
type Secrets struct {
	fileDir string // fallback location (non-macOS)
	// useKeychain is true on macOS.
	useKeychain bool
}

func NewSecrets(settingsPath string) *Secrets {
	return &Secrets{fileDir: filepath.Dir(settingsPath), useKeychain: runtime.GOOS == "darwin"}
}

// APIKey returns the saved key, or $ANTHROPIC_API_KEY if none was saved.
func (s *Secrets) APIKey() string {
	if k := s.saved(); k != "" {
		return k
	}
	return strings.TrimSpace(os.Getenv("ANTHROPIC_API_KEY"))
}

// HasSavedKey reports whether a key was saved in jamalterm itself.
func (s *Secrets) HasSavedKey() bool { return s.saved() != "" }

func (s *Secrets) saved() string {
	if s.useKeychain {
		out, err := exec.Command("/usr/bin/security", "find-generic-password", "-s", keychainService, "-w").Output()
		if err != nil {
			return ""
		}
		return strings.TrimSpace(string(out))
	}
	data, err := os.ReadFile(filepath.Join(s.fileDir, "api-key"))
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(data))
}

// SetAPIKey saves the key; an empty key deletes it.
func (s *Secrets) SetAPIKey(key string) error {
	key = strings.TrimSpace(key)
	if strings.ContainsAny(key, "\n\r\t \"'\\") {
		return errors.New("that doesn't look like an API key (it has spaces or quotes)")
	}
	if s.useKeychain {
		// Remove any old entry first; "not found" is fine.
		exec.Command("/usr/bin/security", "delete-generic-password", "-s", keychainService).Run()
		if key == "" {
			return nil
		}
		// Sent on stdin ("security -i") so the key never shows up in the
		// process list. -U updates in place.
		cmd := exec.Command("/usr/bin/security", "-i")
		cmd.Stdin = strings.NewReader(`add-generic-password -U -a jamalterm -s ` + keychainService + ` -w "` + key + "\"\n")
		if out, err := cmd.CombinedOutput(); err != nil || strings.Contains(string(out), "error") {
			return errors.New("Keychain: " + strings.TrimSpace(string(out)))
		}
		return nil
	}
	path := filepath.Join(s.fileDir, "api-key")
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
