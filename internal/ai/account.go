package ai

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/anthropics/anthropic-sdk-go/v2/config"
)

// Account describes the Anthropic sign-in made with `ant auth login`
// (OAuth in the browser). The SDK reads the same files, so when this
// says SignedIn, a client created without an API key works.
type Account struct {
	SignedIn  bool   `json:"signedIn"`
	Profile   string `json:"profile"`
	Workspace string `json:"workspace"`
}

// SignedInAccount looks for an `ant auth login` profile without reading
// any token.
func SignedInAccount() Account {
	cfg, err := config.LoadConfig()
	if err != nil || cfg == nil || cfg.AuthenticationInfo == nil || cfg.AuthenticationInfo.Type != config.AuthenticationTypeUserOAuth {
		return Account{}
	}
	creds := cfg.AuthenticationInfo.CredentialsPath
	if creds == "" {
		return Account{}
	}
	if _, err := os.Stat(creds); err != nil {
		return Account{}
	}
	// Profile name: the credentials file is credentials/<profile>.json.
	profile := strings.TrimSuffix(filepath.Base(creds), ".json")
	return Account{SignedIn: true, Profile: profile, Workspace: cfg.WorkspaceID}
}
