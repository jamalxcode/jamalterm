package shell

import (
	"encoding/base64"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
	"time"
)

var (
	reCmd  = regexp.MustCompile(`\x1b\]6973;cmd;([A-Za-z0-9+/=]*)\x07`)
	reDone = regexp.MustCompile(`\x1b\]133;D;(\d+)\x07`)
	reCwd  = regexp.MustCompile(`\x1b\]7;file://[^/]*(/[^\x07]*)\x07`)
)

func TestIntegration(t *testing.T) {
	cases := []struct {
		shell  string
		args   []string // nil = login shell
		rcFile string
		rc     string
	}{
		{"zsh", nil, ".zshrc", "export JT_USER_RC=zshrc-loaded\n"},
		{"zsh", []string{}, ".zshrc", "export JT_USER_RC=zshrc-loaded\n"},
		{"bash", nil, ".bash_profile", "export JT_USER_RC=bash-profile-loaded\n"},
		{"bash", []string{}, ".bashrc", "export JT_USER_RC=bashrc-loaded\n"},
		{"tcsh", nil, ".tcshrc", "setenv JT_USER_RC tcshrc-loaded\n"},
		{"tcsh", []string{}, ".cshrc", "setenv JT_USER_RC cshrc-loaded\n"},
		{"fish", nil, ".config/fish/config.fish", "set -gx JT_USER_RC fish-config-loaded\n"},
	}
	for _, tc := range cases {
		name := tc.shell + "/login"
		if tc.args != nil {
			name = tc.shell + "/interactive"
		}
		t.Run(name, func(t *testing.T) {
			path, err := exec.LookPath(tc.shell)
			if err != nil {
				t.Skipf("%s not installed", tc.shell)
			}
			home := t.TempDir()
			t.Setenv("HOME", home)
			t.Setenv("XDG_CACHE_HOME", t.TempDir())
			os.Unsetenv("ZDOTDIR")
			os.MkdirAll(filepath.Dir(filepath.Join(home, tc.rcFile)), 0o755)
			if err := os.WriteFile(filepath.Join(home, tc.rcFile), []byte(tc.rc), 0o644); err != nil {
				t.Fatal(err)
			}
			words := strings.Fields(strings.ReplaceAll(tc.rc, "=", " "))
			want := words[len(words)-1]

			checkIntegration(t, path, tc.args, home, want)
		})
	}
}

// checkIntegration starts the shell in home and checks what browse mode
// relies on: prompt marks, each command as typed, exit codes and the folder.
// wantRC is printed by the user's rc file, proving it ran to the end.
func checkIntegration(t *testing.T, path string, args []string, home, wantRC string) {
	t.Helper()
	c := &capture{}
	exited := make(chan error, 1)
	s, err := Start(Options{
		Shell: path, Args: args, Dir: home, Integration: true,
		Cols: 100, Rows: 30,
		OnOutput: c.write, OnExit: func(err error) { exited <- err },
	})
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()

	waitFor(t, c, "\x1b]133;A\x07") // first prompt
	s.Write([]byte("echo \"rc=$JT_USER_RC home=$HOME\"\n"))
	waitFor(t, c, "rc="+wantRC+" home="+home)

	s.Write([]byte("sh -c 'exit 7'\n"))
	waitFor(t, c, "\x1b]133;D;7\x07")

	s.Write([]byte("cd /tmp && ls / | head -1\n"))
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) && lastCwd(c.String()) != "/tmp" {
		time.Sleep(20 * time.Millisecond)
	}

	out := c.String()
	var cmds []string
	for _, m := range reCmd.FindAllStringSubmatch(out, -1) {
		b, _ := base64.StdEncoding.DecodeString(m[1])
		cmds = append(cmds, strings.TrimRight(string(b), "\n"))
	}
	wantCmds := []string{`echo "rc=$JT_USER_RC home=$HOME"`, "sh -c 'exit 7'", "cd /tmp && ls / | head -1"}
	if strings.Join(cmds, "|") != strings.Join(wantCmds, "|") {
		t.Errorf("commands:\n got %q\nwant %q", cmds, wantCmds)
	}
	codes := reDone.FindAllStringSubmatch(out, -1)
	var gotCodes []string
	for _, m := range codes {
		gotCodes = append(gotCodes, m[1])
	}
	if g := strings.Join(gotCodes, ","); !strings.HasSuffix(g, "0,7,0") {
		t.Errorf("exit codes = %s, want ...0,7,0", g)
	}
	if cwd := lastCwd(out); cwd != "/tmp" {
		t.Errorf("cwd = %q, want /tmp", cwd)
	}
	if n := strings.Count(out, "\x1b]133;C\x07"); n != 3 {
		t.Errorf("got %d command-start marks, want 3", n)
	}

	s.Write([]byte("exit\n"))
	select {
	case <-exited:
	case <-time.After(5 * time.Second):
		t.Fatal("shell did not exit")
	}
}

func lastCwd(out string) string {
	m := reCwd.FindAllStringSubmatch(out, -1)
	if len(m) == 0 {
		return ""
	}
	return m[len(m)-1][1]
}

// Your own tcsh precmd/postcmd aliases still run.
func TestTcshKeepsUserAliases(t *testing.T) {
	path, err := exec.LookPath("tcsh")
	if err != nil {
		t.Skip("tcsh not installed")
	}
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("XDG_CACHE_HOME", t.TempDir())
	os.WriteFile(filepath.Join(home, ".tcshrc"), []byte("alias precmd 'echo USER-PRECMD'\nalias postcmd 'echo USER-POSTCMD'\n"), 0o644)
	c := &capture{}
	exited := make(chan error, 1)
	s, err := Start(Options{Shell: path, Args: []string{}, Dir: home, Integration: true, Cols: 100, Rows: 30,
		OnOutput: c.write, OnExit: func(err error) { exited <- err }})
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	waitFor(t, c, "\x1b]133;A\x07")
	s.Write([]byte("echo hi-there\n"))
	// Startup files can run commands (and so postcmd) before the first
	// prompt, so look only at what comes after the command we typed.
	waitFor(t, c, "\x1b]133;C\x07")
	deadline := time.Now().Add(5 * time.Second)
	var after string
	for time.Now().Before(deadline) {
		out := c.String()
		after = out[strings.Index(out, "\x1b]133;C\x07"):]
		if strings.Contains(after, "\x1b]133;D;0\x07") && strings.Contains(after, "USER-PRECMD") {
			break
		}
		time.Sleep(20 * time.Millisecond)
	}
	if !strings.Contains(after, "hi-there") || !strings.Contains(after, "\x1b]133;D;0\x07") {
		t.Fatalf("command didn't finish with our marks:\n%q", c.String())
	}
	if !strings.Contains(after, "USER-PRECMD") {
		t.Fatalf("user precmd didn't run at the next prompt:\n%q", c.String())
	}
	// Ours reports the command first, then yours runs, then its output.
	if p := strings.Index(after, "USER-POSTCMD"); p < 0 || p > strings.LastIndex(after, "hi-there") {
		t.Fatalf("user postcmd didn't run for the command:\n%q", c.String())
	}
}
