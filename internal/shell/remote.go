package shell

import "encoding/base64"

// RemoteSetup is one line to type into a bash or zsh on another machine
// (over ssh) that turns on browse mode there for that session only. It
// starts with a space so most shells leave it out of the history.
func RemoteSetup() string {
	script, err := integrationFS.ReadFile("integration/remote.sh")
	if err != nil {
		return ""
	}
	b := base64.StdEncoding.EncodeToString(script)
	// GNU base64 takes -d, older macOS -D.
	return ` eval "$(printf '%s' '` + b + `' | { base64 -d 2>/dev/null || base64 -D; })"`
}
