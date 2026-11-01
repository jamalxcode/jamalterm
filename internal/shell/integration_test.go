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
			if err := os.WriteFile(filepath.Join(home, tc.rcFile), []byte(tc.rc), 0o644); err != nil {
				t.Fatal(err)
			}
			want := strings.Fields(strings.ReplaceAll(tc.rc, "=", " "))[2]

			c := &capture{}
			exited := make(chan error, 1)
			s, err := Start(Options{
				Shell: path, Args: tc.args, Dir: home, Integration: true,
				Cols: 100, Rows: 30,
				OnOutput: c.write, OnExit: func(err error) { exited <- err },
			})
			if err != nil {
				t.Fatal(err)
			}
			defer s.Close()

			waitFor(t, c, "\x1b]133;A\x07") // first prompt
			s.Write([]byte("echo \"rc=$JT_USER_RC home=$HOME\"\n"))
			waitFor(t, c, "rc="+want+" home="+home)

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
		})
	}
}

func lastCwd(out string) string {
	m := reCwd.FindAllStringSubmatch(out, -1)
	if len(m) == 0 {
		return ""
	}
	return m[len(m)-1][1]
}
