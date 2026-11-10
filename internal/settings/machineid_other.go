//go:build !darwin

package settings

import (
	"os"
	"strings"
)

// machineID is this computer's ID (systemd's machine-id).
func machineID() string {
	for _, p := range []string{"/etc/machine-id", "/var/lib/dbus/machine-id"} {
		if b, err := os.ReadFile(p); err == nil {
			return strings.TrimSpace(string(b))
		}
	}
	return ""
}
