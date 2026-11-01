// Package shell runs the user's shell (zsh, bash, tcsh, fish, ...) inside a
// pseudo-terminal and streams its output to a callback.
//
// It knows nothing about the GUI, so it can be tested on its own.
package shell

import (
	"errors"
	"io"
	"os"
	"os/exec"
	"os/user"
	"path/filepath"
	"runtime"
	"strings"
	"sync"

	"github.com/creack/pty"
)

// Options configures a Session.
type Options struct {
	// Shell is the path to the shell. Empty means DefaultShell().
	Shell string
	// Args are extra arguments. Nil means a login shell ("-l"), which is
	// what Terminal.app and iTerm2 start, so ~/.zprofile etc. are read and
	// PATH is set up properly even when the app is launched from Finder.
	Args []string
	// Dir is the starting directory. Empty means the user's home directory.
	Dir string
	// Env is added on top of the inherited environment.
	Env []string
	// Integration loads jamalterm's hooks into zsh, bash and tcsh so the
	// terminal can tell where each command starts and ends.
	Integration bool
	// Cols and Rows are the initial terminal size.
	Cols, Rows uint16

	// OnOutput receives everything the shell writes. It is called from a
	// single goroutine, in order. The slice is only valid during the call.
	OnOutput func([]byte)
	// OnExit is called once when the shell exits.
	OnExit func(err error)
}

// Session is one running shell attached to a PTY.
type Session struct {
	cmd  *exec.Cmd
	ptmx *os.File

	mu     sync.Mutex
	closed bool
	done   chan struct{}
}

// DefaultShell returns the user's login shell: $SHELL, then the account
// database, then /bin/zsh (the macOS default).
func DefaultShell() string {
	if s := os.Getenv("SHELL"); s != "" {
		return s
	}
	if s := loginShellFromAccount(); s != "" {
		return s
	}
	for _, s := range []string{"/bin/zsh", "/bin/bash", "/bin/sh"} {
		if _, err := os.Stat(s); err == nil {
			return s
		}
	}
	return "/bin/sh"
}

// Start launches the shell.
func Start(o Options) (*Session, error) {
	shellPath := o.Shell
	if shellPath == "" {
		shellPath = DefaultShell()
	}
	args := o.Args
	if args == nil {
		args = []string{"-l"}
	}
	dir := o.Dir
	if dir == "" {
		if home, err := os.UserHomeDir(); err == nil {
			dir = home
		}
	}
	if o.Cols == 0 {
		o.Cols = 80
	}
	if o.Rows == 0 {
		o.Rows = 24
	}

	extraEnv := o.Env
	if o.Integration {
		home, _ := os.UserHomeDir()
		args, extraEnv = withIntegration(shellPath, args, append([]string{}, o.Env...), home)
	}

	cmd := exec.Command(shellPath, args...)
	cmd.Dir = dir
	cmd.Env = buildEnv(os.Environ(), shellPath, extraEnv)
	if filepath.Base(shellPath) == "tcsh" || filepath.Base(shellPath) == "csh" {
		// csh decides it is a login shell from a leading "-" in argv[0].
		if len(args) == 1 && args[0] == "-l" {
			cmd.Args = []string{"-" + filepath.Base(shellPath)}
		}
	}

	ptmx, err := pty.StartWithSize(cmd, &pty.Winsize{Cols: o.Cols, Rows: o.Rows})
	if err != nil {
		return nil, err
	}

	s := &Session{cmd: cmd, ptmx: ptmx, done: make(chan struct{})}
	go s.readLoop(o.OnOutput, o.OnExit)
	return s, nil
}

func (s *Session) readLoop(onOutput func([]byte), onExit func(error)) {
	buf := make([]byte, 32*1024)
	for {
		n, err := s.ptmx.Read(buf)
		if n > 0 && onOutput != nil {
			onOutput(buf[:n])
		}
		if err != nil {
			break // EOF / EIO: the shell side of the PTY was closed
		}
	}
	waitErr := s.cmd.Wait()
	s.Close()
	close(s.done)
	if onExit != nil {
		onExit(waitErr)
	}
}

// Write sends keystrokes (or pasted text) to the shell.
func (s *Session) Write(p []byte) (int, error) {
	s.mu.Lock()
	closed := s.closed
	s.mu.Unlock()
	if closed {
		return 0, io.ErrClosedPipe
	}
	return s.ptmx.Write(p)
}

// Resize tells the shell (and programs like vim) the new window size.
// The kernel sends SIGWINCH to the foreground process for us.
func (s *Session) Resize(cols, rows uint16) error {
	if cols == 0 || rows == 0 {
		return errors.New("shell: invalid size")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return io.ErrClosedPipe
	}
	return pty.Setsize(s.ptmx, &pty.Winsize{Cols: cols, Rows: rows})
}

// Close hangs up the terminal, which makes the shell exit.
func (s *Session) Close() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return nil
	}
	s.closed = true
	return s.ptmx.Close()
}

// Done is closed after the shell has exited.
func (s *Session) Done() <-chan struct{} { return s.done }

// Pid of the shell process.
func (s *Session) Pid() int { return s.cmd.Process.Pid }

func buildEnv(base []string, shellPath string, extra []string) []string {
	env := map[string]string{}
	var order []string
	set := func(kv string) {
		k, v, ok := strings.Cut(kv, "=")
		if !ok {
			return
		}
		if _, seen := env[k]; !seen {
			order = append(order, k)
		}
		env[k] = v
	}
	for _, kv := range base {
		set(kv)
	}

	set("TERM=xterm-256color")
	set("COLORTERM=truecolor")
	set("TERM_PROGRAM=jamalterm")
	set("SHELL=" + shellPath)
	// Apps launched from Finder get no locale; without one, zsh mangles
	// UTF-8 (emoji, accents) and some tools fall back to ASCII.
	if env["LANG"] == "" && env["LC_ALL"] == "" && env["LC_CTYPE"] == "" {
		set("LANG=en_US.UTF-8")
	}
	// Don't leak variables from whatever terminal launched us in dev mode.
	for _, k := range []string{"TERM_PROGRAM_VERSION", "TERM_SESSION_ID", "ITERM_SESSION_ID", "ITERM_PROFILE"} {
		delete(env, k)
	}
	for _, kv := range extra {
		set(kv)
	}

	out := make([]string, 0, len(order))
	for _, k := range order {
		if v, ok := env[k]; ok {
			out = append(out, k+"="+v)
		}
	}
	return out
}

func loginShellFromAccount() string {
	u, err := user.Current()
	if err != nil {
		return ""
	}
	if runtime.GOOS == "darwin" {
		// macOS keeps accounts in Directory Services, not /etc/passwd.
		out, err := exec.Command("/usr/bin/dscl", ".", "-read", "/Users/"+u.Username, "UserShell").Output()
		if err == nil {
			if f := strings.Fields(string(out)); len(f) == 2 && filepath.IsAbs(f[1]) {
				return f[1]
			}
		}
		return ""
	}
	data, err := os.ReadFile("/etc/passwd")
	if err != nil {
		return ""
	}
	for _, line := range strings.Split(string(data), "\n") {
		f := strings.Split(line, ":")
		if len(f) >= 7 && f[0] == u.Username && filepath.IsAbs(f[6]) {
			return f[6]
		}
	}
	return ""
}
