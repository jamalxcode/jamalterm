package shell

import (
	"os"
	"strconv"
	"strings"

	"golang.org/x/sys/unix"
)

const ioctlGetTermios = unix.TCGETS

func procName(pid int) string {
	b, err := os.ReadFile("/proc/" + strconv.Itoa(pid) + "/comm")
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(b))
}
