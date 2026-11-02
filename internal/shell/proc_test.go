package shell

import (
	"testing"
	"time"
)

func waitTrue(t *testing.T, what string, f func() bool) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		if f() {
			return
		}
		time.Sleep(30 * time.Millisecond)
	}
	t.Fatalf("timed out waiting for %s", what)
}

func TestForegroundAndPasswordMode(t *testing.T) {
	c := &capture{}
	exited := make(chan error, 1)
	s := startShell(t, "bash", c, exited)
	defer s.Close()
	time.Sleep(300 * time.Millisecond)
	if s.Busy() {
		t.Fatal("busy at the prompt")
	}

	s.Write([]byte("sleep 3\n"))
	waitTrue(t, "sleep in front", func() bool { _, n := s.Foreground(); return n == "sleep" })
	s.Write([]byte("\x03")) // Ctrl+C
	waitTrue(t, "back at the prompt", func() bool { return !s.Busy() })

	if s.PasswordMode() {
		t.Fatal("password mode at the prompt")
	}
	s.Write([]byte("read -s secret\n"))
	waitTrue(t, "password mode", s.PasswordMode)
	s.Write([]byte("x\n"))
	waitTrue(t, "password mode over", func() bool { return !s.PasswordMode() })
}
