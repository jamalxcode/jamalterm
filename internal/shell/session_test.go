package shell

import (
	"bytes"
	"os/exec"
	"strings"
	"sync"
	"testing"
	"time"
)

type capture struct {
	mu  sync.Mutex
	buf bytes.Buffer
}

func (c *capture) write(p []byte) { c.mu.Lock(); c.buf.Write(p); c.mu.Unlock() }
func (c *capture) String() string { c.mu.Lock(); defer c.mu.Unlock(); return c.buf.String() }

// waitTimeout is longer for heavy shell setups (oh-my-zsh, prezto).
var waitTimeout = 5 * time.Second

func waitFor(t *testing.T, c *capture, want string) {
	t.Helper()
	deadline := time.Now().Add(waitTimeout)
	for time.Now().Before(deadline) {
		if strings.Contains(c.String(), want) {
			return
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatalf("timed out waiting for %q; got:\n%s", want, c.String())
}

func startShell(t *testing.T, name string, c *capture, exited chan error) *Session {
	t.Helper()
	path, err := exec.LookPath(name)
	if err != nil {
		t.Skipf("%s not installed", name)
	}
	s, err := Start(Options{
		Shell:    path,
		Args:     []string{}, // not a login shell: keep the test independent of dotfiles
		Cols:     100,
		Rows:     30,
		Env:      []string{"PS1=$ ", "PROMPT=$ "},
		OnOutput: c.write,
		OnExit:   func(err error) { exited <- err },
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { s.Close() })
	return s
}

func TestShells(t *testing.T) {
	for _, name := range []string{"bash", "zsh", "tcsh", "sh"} {
		t.Run(name, func(t *testing.T) {
			c := &capture{}
			exited := make(chan error, 1)
			s := startShell(t, name, c, exited)

			// The shell sees a real terminal of the size we asked for.
			s.Write([]byte("stty size; echo TERM=$TERM\n"))
			waitFor(t, c, "30 100")
			waitFor(t, c, "TERM=xterm-256color")

			// Resizing reaches the shell.
			if err := s.Resize(120, 40); err != nil {
				t.Fatal(err)
			}
			s.Write([]byte("stty size\n"))
			waitFor(t, c, "40 120")

			// Typing "exit" ends the session and fires OnExit.
			s.Write([]byte("exit\n"))
			select {
			case <-exited:
			case <-time.After(5 * time.Second):
				t.Fatal("shell did not exit")
			}
			<-s.Done()
			if _, err := s.Write([]byte("x")); err == nil {
				t.Fatal("write after exit should fail")
			}
		})
	}
}

func TestCloseKillsShell(t *testing.T) {
	c := &capture{}
	exited := make(chan error, 1)
	s := startShell(t, "sh", c, exited)
	s.Close()
	select {
	case <-exited:
	case <-time.After(5 * time.Second):
		t.Fatal("shell did not exit after Close")
	}
}

func TestBuildEnv(t *testing.T) {
	env := buildEnv([]string{"PATH=/bin", "TERM=dumb", "ITERM_PROFILE=x"}, "/bin/zsh", []string{"FOO=bar"})
	got := strings.Join(env, "\n")
	for _, want := range []string{"PATH=/bin", "TERM=xterm-256color", "COLORTERM=truecolor", "SHELL=/bin/zsh", "LANG=en_US.UTF-8", "FOO=bar"} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %s in\n%s", want, got)
		}
	}
	if strings.Contains(got, "ITERM_PROFILE") || strings.Contains(got, "TERM=dumb") {
		t.Errorf("stale variables kept:\n%s", got)
	}
}
