package shell

import (
	"strings"
	"testing"
)

// The line typed into a "remote" shell (here: a local one started without
// integration) turns the marks on for that session.
func TestRemoteSetup(t *testing.T) {
	for _, sh := range []string{"bash", "zsh"} {
		t.Run(sh, func(t *testing.T) {
			c := &capture{}
			exited := make(chan error, 1)
			s := startShell(t, sh, c, exited)
			line := RemoteSetup()
			if !strings.HasPrefix(line, " eval ") || len(line) > 8000 {
				t.Fatalf("odd setup line (%d bytes)", len(line))
			}
			s.Write([]byte(line + "\n"))
			waitFor(t, c, "browse mode is on for this server session")
			s.Write([]byte("echo remote-ok\n"))
			waitFor(t, c, "\x1b]133;D;0\x07")
			out := c.String()
			for _, want := range []string{"\x1b]133;A\x07", "\x1b]133;C\x07", "\x1b]6973;cmd;ZWNobyByZW1vdGUtb2s=\x07", "remote-ok"} {
				if !strings.Contains(out, want) {
					t.Fatalf("missing %q in:\n%q", want, out)
				}
			}
			// Running it again does nothing.
			before := strings.Count(c.String(), "browse mode is on")
			s.Write([]byte(line + "\necho again\n"))
			waitFor(t, c, "again\r\n")
			if strings.Count(c.String(), "browse mode is on") != before {
				t.Fatal("set up twice")
			}
		})
	}
}
