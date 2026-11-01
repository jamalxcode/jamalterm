// Package settings loads and saves the user's preferences as JSON in
// ~/Library/Application Support/jamalterm/settings.json (macOS).
package settings

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"sync"
)

// Settings are edited in the Settings window (Cmd+,).
type Settings struct {
	Theme       string  `json:"theme"`
	FontFamily  string  `json:"fontFamily"`
	FontSize    int     `json:"fontSize"`
	LineHeight  float64 `json:"lineHeight"`
	CursorStyle string  `json:"cursorStyle"` // block, bar, underline
	// Opacity of the window in percent (30–100). Below 100 the desktop
	// shows through.
	Opacity int `json:"opacity"`
	// Blur what's behind a see-through window (macOS; applies on restart).
	Blur        bool   `json:"blur"`
	CursorBlink bool   `json:"cursorBlink"`
	Scrollback  int    `json:"scrollback"`
	Shell       string `json:"shell"` // empty = your login shell
	// ShellIntegration lets jamalterm know where commands start and end.
	// Browse mode and the AI features need it.
	ShellIntegration bool `json:"shellIntegration"`
	// ArrowBrowse makes ↑ at an empty prompt start browse mode (the
	// free-floating cursor) instead of going to the shell's history.
	ArrowBrowse bool `json:"arrowBrowse"`
	// ConfirmRerun asks before re-running a command from browse mode.
	ConfirmRerun bool `json:"confirmRerun"`
	OptionIsMeta bool `json:"optionIsMeta"`

	// AutoUpdate checks GitHub for new versions at launch and every 6 hours.
	AutoUpdate bool `json:"autoUpdate"`

	AIEnabled bool `json:"aiEnabled"`
	// AIProvider answers questions: "anthropic" (Claude), "openai"
	// (ChatGPT), "deepseek" or "xai" (Grok).
	AIProvider string `json:"aiProvider"`
	AIModel    string `json:"aiModel"` // Claude model
	// OpenAIModel / DeepSeekModel: "" picks automatically.
	OpenAIModel     string `json:"openaiModel"`
	DeepSeekModel   string `json:"deepseekModel"`
	XAIModel        string `json:"xaiModel"`
	OpenRouterModel string `json:"openrouterModel"`
	GeminiModel     string `json:"geminiModel"`
	// GeminiProject: Google Cloud project for Gemini via Google sign-in
	// ("" = the sign-in's or gcloud's default project).
	GeminiProject string `json:"geminiProject"`
	// AIShareOutput sends recent command output to Claude with questions.
	AIShareOutput bool `json:"aiShareOutput"`
}

// Defaults are used for anything missing from the file.
func Defaults() Settings {
	return Settings{
		Theme:            "jamal-night",
		FontFamily:       `"JetBrains Mono", monospace`,
		FontSize:         14,
		LineHeight:       1.15,
		CursorStyle:      "block",
		Opacity:          100,
		Blur:             true,
		CursorBlink:      true,
		Scrollback:       10000,
		ShellIntegration: true,
		ArrowBrowse:      true,
		ConfirmRerun:     false,
		OptionIsMeta:     true,
		AutoUpdate:       true,
		AIEnabled:        true,
		AIProvider:       "anthropic",
		AIModel:          "claude-opus-5-5",
		AIShareOutput:    true,
	}
}

// Store reads and writes one settings file.
type Store struct {
	path string
	mu   sync.Mutex
}

// DefaultPath is the per-user settings file.
func DefaultPath() string {
	dir, err := os.UserConfigDir()
	if err != nil {
		dir = os.TempDir()
	}
	return filepath.Join(dir, "jamalterm", "settings.json")
}

func NewStore(path string) *Store { return &Store{path: path} }

func (s *Store) Path() string { return s.path }

// Load returns the saved settings, falling back to defaults for a missing
// file or missing fields. A damaged file is reported but defaults still load.
func (s *Store) Load() (Settings, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	cfg := Defaults()
	data, err := os.ReadFile(s.path)
	if errors.Is(err, os.ErrNotExist) {
		return cfg, nil
	}
	if err != nil {
		return cfg, err
	}
	if err := json.Unmarshal(data, &cfg); err != nil {
		return Defaults(), err
	}
	return cfg.normalized(), nil
}

// Save writes atomically, so a crash can't leave half a file.
func (s *Store) Save(cfg Settings) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	cfg = cfg.normalized()
	if err := os.MkdirAll(filepath.Dir(s.path), 0o700); err != nil {
		return err
	}
	data, err := json.MarshalIndent(cfg, "", "  ")
	if err != nil {
		return err
	}
	tmp := s.path + ".tmp"
	if err := os.WriteFile(tmp, append(data, '\n'), 0o600); err != nil {
		return err
	}
	return os.Rename(tmp, s.path)
}

func (c Settings) normalized() Settings {
	d := Defaults()
	if c.FontSize < 8 || c.FontSize > 48 {
		c.FontSize = d.FontSize
	}
	if c.LineHeight < 1 || c.LineHeight > 2 {
		c.LineHeight = d.LineHeight
	}
	switch c.CursorStyle {
	case "block", "bar", "underline":
	default:
		c.CursorStyle = d.CursorStyle
	}
	if c.Opacity < 30 || c.Opacity > 100 {
		c.Opacity = d.Opacity
	}
	if c.Scrollback < 1000 || c.Scrollback > 200000 {
		c.Scrollback = d.Scrollback
	}
	if c.Theme == "" {
		c.Theme = d.Theme
	}
	if c.FontFamily == "" {
		c.FontFamily = d.FontFamily
	}
	switch c.AIProvider {
	case "anthropic", "openai", "deepseek", "xai", "openrouter", "gemini":
	default:
		c.AIProvider = d.AIProvider
	}
	c.OpenAIModel = strings.TrimSpace(c.OpenAIModel)
	c.DeepSeekModel = strings.TrimSpace(c.DeepSeekModel)
	c.XAIModel = strings.TrimSpace(c.XAIModel)
	c.OpenRouterModel = strings.TrimSpace(c.OpenRouterModel)
	c.GeminiModel = strings.TrimSpace(c.GeminiModel)
	c.GeminiProject = strings.TrimSpace(c.GeminiProject)
	if c.AIModel == "" {
		c.AIModel = d.AIModel
	}
	return c
}
