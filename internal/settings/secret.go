package settings

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/hkdf"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
)

// API keys never go in settings.json, and never in the macOS Keychain.
//
// They live in one encrypted file next to the settings
// (~/Library/Application Support/jamalterm/keys.enc):
//
//   - AES-256-GCM, with a key made from a random 32-byte secret
//     (keys.secret) mixed with this computer's hardware ID and your user ID
//     (HKDF-SHA256). A copy of the folder (a backup, a sync, a zip) can't
//     be read on another computer or by another account.
//   - Both files are readable only by you (0600, folder 0700) and left out
//     of Time Machine backups.
//
// This is the same protection SSH keys and the GitHub and AWS command-line
// tools rely on: it keeps keys from other accounts, copies and backups. A
// program already running as you could still read them, as with any
// local storage. Nothing ever asks for a password.
//
// Providers: "anthropic" (Claude), "openai" (ChatGPT), "deepseek", "xai"
// (Grok), "openrouter".

const (
	keysFile   = "keys.enc"
	secretFile = "keys.secret"
)

// Secrets stores the API keys.
type Secrets struct {
	dir string
	// machine is this computer's ID (replaceable in tests).
	machine func() string
	// keychainService names the old Keychain item ("jamalterm").
	keychainService string

	mu     sync.Mutex
	loaded bool
	keys   map[string]string
	// unreadable: a keys file exists but couldn't be decrypted (e.g. it was
	// copied from another Mac). Saving a key starts a new file.
	unreadable bool
}

func NewSecrets(settingsPath string) *Secrets {
	return &Secrets{dir: filepath.Dir(settingsPath), machine: machineID, keychainService: "jamalterm"}
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

// Unreadable reports that saved keys couldn't be unlocked on this computer.
func (s *Secrets) Unreadable() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.loadLocked()
	return s.unreadable
}

func (s *Secrets) saved(provider string) string {
	if _, ok := envVar[provider]; !ok {
		return ""
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.loadLocked()
	return s.keys[provider]
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
	s.mu.Lock()
	defer s.mu.Unlock()
	s.loadLocked()
	if key == "" {
		delete(s.keys, provider)
	} else {
		s.keys[provider] = key
	}
	if err := s.storeLocked(); err != nil {
		s.loaded = false // read the file again next time
		return err
	}
	s.unreadable = false
	return nil
}

// loadLocked reads the keys file once.
func (s *Secrets) loadLocked() {
	if s.loaded {
		return
	}
	s.loaded = true
	s.keys = map[string]string{}
	data, err := os.ReadFile(filepath.Join(s.dir, keysFile))
	if errors.Is(err, os.ErrNotExist) {
		s.moveOldFiles()
		return
	}
	if err != nil {
		s.unreadable = true
		return
	}
	plain, err := s.open(data)
	if err != nil || json.Unmarshal(plain, &s.keys) != nil {
		s.keys = map[string]string{}
		s.unreadable = true
	}
}

// moveOldFiles moves keys that development builds kept in plain 0600
// files (api-key, api-key-openai, …) into the encrypted file.
func (s *Secrets) moveOldFiles() {
	var moved []string
	for p := range envVar {
		name := "api-key-" + p
		if p == "anthropic" {
			name = "api-key"
		}
		path := filepath.Join(s.dir, name)
		if b, err := os.ReadFile(path); err == nil {
			if k := strings.TrimSpace(string(b)); k != "" {
				s.keys[p] = k
				moved = append(moved, path)
			}
		}
	}
	if len(moved) > 0 && s.storeLocked() == nil {
		for _, p := range moved {
			os.Remove(p)
		}
	}
}

type sealed struct {
	Version int    `json:"version"`
	Nonce   string `json:"nonce"`
	Data    string `json:"data"`
}

func (s *Secrets) storeLocked() error {
	if err := os.MkdirAll(s.dir, 0o700); err != nil {
		return err
	}
	os.Chmod(s.dir, 0o700)
	path := filepath.Join(s.dir, keysFile)
	if len(s.keys) == 0 {
		err := os.Remove(path)
		if errors.Is(err, os.ErrNotExist) {
			return nil
		}
		return err
	}
	aead, err := s.cipher(true)
	if err != nil {
		return err
	}
	plain, _ := json.Marshal(s.keys)
	nonce := make([]byte, aead.NonceSize())
	if _, err := rand.Read(nonce); err != nil {
		return err
	}
	out, _ := json.Marshal(sealed{Version: 1, Nonce: base64.StdEncoding.EncodeToString(nonce),
		Data: base64.StdEncoding.EncodeToString(aead.Seal(nil, nonce, plain, []byte("jamalterm keys v1")))})
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, out, 0o600); err != nil {
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		return err
	}
	excludeFromBackups(path)
	return nil
}

func (s *Secrets) open(data []byte) ([]byte, error) {
	var f sealed
	if err := json.Unmarshal(data, &f); err != nil || f.Version != 1 {
		return nil, errors.New("not a keys file")
	}
	nonce, err1 := base64.StdEncoding.DecodeString(f.Nonce)
	box, err2 := base64.StdEncoding.DecodeString(f.Data)
	if err1 != nil || err2 != nil {
		return nil, errors.New("damaged keys file")
	}
	aead, err := s.cipher(false)
	if err != nil {
		return nil, err
	}
	if len(nonce) != aead.NonceSize() {
		return nil, errors.New("damaged keys file")
	}
	return aead.Open(nil, nonce, box, []byte("jamalterm keys v1"))
}

// cipher is AES-256-GCM with a key from the secret file, this computer
// and this user. create makes the secret file if it's missing.
func (s *Secrets) cipher(create bool) (cipher.AEAD, error) {
	path := filepath.Join(s.dir, secretFile)
	secret, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) && create {
		secret = make([]byte, 32)
		if _, err := rand.Read(secret); err != nil {
			return nil, err
		}
		if err := os.WriteFile(path, secret, 0o600); err != nil {
			return nil, err
		}
		excludeFromBackups(path)
	} else if err != nil {
		return nil, err
	}
	if len(secret) != 32 {
		return nil, errors.New("damaged keys secret")
	}
	info := "jamalterm api keys|" + s.machine() + "|" + strconv.Itoa(os.Getuid())
	key, err := hkdf.Key(sha256.New, secret, nil, info, 32)
	if err != nil {
		return nil, err
	}
	block, err := aes.NewCipher(key)
	if err != nil {
		return nil, err
	}
	return cipher.NewGCM(block)
}

// excludeFromBackups keeps a file out of Time Machine (macOS). Tests
// replace it (tmutil can be slow or blocked on build machines).
var excludeFromBackups = func(path string) {
	if runtime.GOOS == "darwin" {
		exec.Command("/usr/bin/tmutil", "addexclusion", path).Run()
	}
}

// ---- Moving keys out of the Keychain (older versions kept them there) ----

const keychainAccount = "api-keys"

// KeychainKeysWaiting reports whether keys saved by an older version are
// still in the Keychain. Checking never shows a password prompt.
func (s *Secrets) KeychainKeysWaiting() bool {
	return keychainAvailable() && keychainExists(s.keychainService, keychainAccount)
}

// MoveFromKeychain copies the older version's keys from the Keychain into
// the keys file and deletes the Keychain item. macOS asks for your
// password one last time to allow reading it. Keys already saved here win.
func (s *Secrets) MoveFromKeychain() (int, error) {
	data, found, err := keychainGet(s.keychainService, keychainAccount)
	if err != nil {
		return 0, fmt.Errorf("couldn't read the Keychain: %w", err)
	}
	if !found {
		return 0, nil
	}
	var old map[string]string
	if err := json.Unmarshal(data, &old); err != nil {
		return 0, errors.New("the Keychain item isn't in the expected form")
	}
	s.mu.Lock()
	s.loadLocked()
	n := 0
	for p, k := range old {
		if _, ok := envVar[p]; ok && k != "" && s.keys[p] == "" {
			s.keys[p] = k
			n++
		}
	}
	err = s.storeLocked()
	s.mu.Unlock()
	if err != nil {
		return 0, err
	}
	keychainDelete(s.keychainService, keychainAccount)
	return n, nil
}
