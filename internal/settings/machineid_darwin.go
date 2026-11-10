package settings

import (
	"os/exec"
	"regexp"
)

var uuidRe = regexp.MustCompile(`"IOPlatformUUID" = "([0-9A-Fa-f-]+)"`)

// machineID is this Mac's hardware UUID.
func machineID() string {
	out, err := exec.Command("/usr/sbin/ioreg", "-rd1", "-c", "IOPlatformExpertDevice").Output()
	if err != nil {
		return ""
	}
	if m := uuidRe.FindSubmatch(out); m != nil {
		return string(m[1])
	}
	return ""
}
