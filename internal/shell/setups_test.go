package shell

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"
)

// TestRealShellSetups runs browse mode's checks under the shell setups people
// actually use: oh-my-zsh, Powerlevel10k (with instant prompt), prezto and
// starship. They hook the prompt the same way we do, so this is where
// integration breaks first.
//
// It needs JT_SHELL_SETUPS, a folder holding clones of ohmyzsh,
// powerlevel10k and prezto (CI's "shells" job makes one); starship must be on
// PATH. Without it the test is skipped.
func TestRealShellSetups(t *testing.T) {
	fw := os.Getenv("JT_SHELL_SETUPS")
	if fw == "" {
		t.Skip("JT_SHELL_SETUPS not set")
	}
	old := waitTimeout
	waitTimeout = 30 * time.Second // first runs compile completions
	defer func() { waitTimeout = old }()

	need := func(t *testing.T, paths ...string) {
		for _, p := range paths {
			if _, err := os.Stat(filepath.Join(fw, p)); err != nil {
				t.Fatalf("%s missing from JT_SHELL_SETUPS", p)
			}
		}
	}
	starship := func(t *testing.T) {
		if _, err := exec.LookPath("starship"); err != nil {
			t.Fatal("starship not on PATH")
		}
	}

	cases := []struct {
		name   string
		shell  string
		launch int // how many times to start it (p10k's instant prompt only shows from the second start)
		setup  func(t *testing.T, home string) (rcFile, rc string)
	}{
		{"oh-my-zsh", "zsh", 1, func(t *testing.T, home string) (string, string) {
			need(t, "ohmyzsh/oh-my-zsh.sh")
			return ".zshrc", `export ZSH="` + fw + `/ohmyzsh"
ZSH_THEME=robbyrussell
DISABLE_AUTO_UPDATE=true
plugins=(git)
source $ZSH/oh-my-zsh.sh
`
		}},
		{"powerlevel10k", "zsh", 2, func(t *testing.T, home string) (string, string) {
			need(t, "powerlevel10k/powerlevel10k.zsh-theme", "powerlevel10k/config/p10k-lean.zsh")
			copyFile(t, filepath.Join(fw, "powerlevel10k/config/p10k-lean.zsh"), filepath.Join(home, ".p10k.zsh"))
			return ".zshrc", `if [[ -r "${XDG_CACHE_HOME:-$HOME/.cache}/p10k-instant-prompt-${(%):-%n}.zsh" ]]; then
  source "${XDG_CACHE_HOME:-$HOME/.cache}/p10k-instant-prompt-${(%):-%n}.zsh"
fi
POWERLEVEL9K_DISABLE_CONFIGURATION_WIZARD=true
source "` + fw + `/powerlevel10k/powerlevel10k.zsh-theme"
source ~/.p10k.zsh
`
		}},
		{"oh-my-zsh+powerlevel10k", "zsh", 2, func(t *testing.T, home string) (string, string) {
			need(t, "ohmyzsh/oh-my-zsh.sh", "powerlevel10k/powerlevel10k.zsh-theme")
			custom := filepath.Join(home, "omz-custom")
			os.MkdirAll(filepath.Join(custom, "themes"), 0o755)
			os.Symlink(filepath.Join(fw, "powerlevel10k"), filepath.Join(custom, "themes", "powerlevel10k"))
			copyFile(t, filepath.Join(fw, "powerlevel10k/config/p10k-classic.zsh"), filepath.Join(home, ".p10k.zsh"))
			return ".zshrc", `if [[ -r "${XDG_CACHE_HOME:-$HOME/.cache}/p10k-instant-prompt-${(%):-%n}.zsh" ]]; then
  source "${XDG_CACHE_HOME:-$HOME/.cache}/p10k-instant-prompt-${(%):-%n}.zsh"
fi
export ZSH="` + fw + `/ohmyzsh"
ZSH_CUSTOM="` + custom + `"
ZSH_THEME="powerlevel10k/powerlevel10k"
POWERLEVEL9K_DISABLE_CONFIGURATION_WIZARD=true
DISABLE_AUTO_UPDATE=true
plugins=(git)
source $ZSH/oh-my-zsh.sh
source ~/.p10k.zsh
`
		}},
		{"prezto", "zsh", 1, func(t *testing.T, home string) (string, string) {
			need(t, "prezto/init.zsh", "prezto/runcoms/zpreztorc")
			os.Symlink(filepath.Join(fw, "prezto"), filepath.Join(home, ".zprezto"))
			copyFile(t, filepath.Join(fw, "prezto/runcoms/zpreztorc"), filepath.Join(home, ".zpreztorc"))
			return ".zshrc", "source \"${ZDOTDIR:-$HOME}/.zprezto/init.zsh\"\n"
		}},
		{"starship/zsh", "zsh", 1, func(t *testing.T, home string) (string, string) {
			starship(t)
			return ".zshrc", "eval \"$(starship init zsh)\"\n"
		}},
		{"starship/bash", "bash", 1, func(t *testing.T, home string) (string, string) {
			starship(t)
			return ".bash_profile", "eval \"$(starship init bash)\"\n"
		}},
		{"starship/fish", "fish", 1, func(t *testing.T, home string) (string, string) {
			starship(t)
			return ".config/fish/config.fish", "starship init fish | source\n"
		}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			path, err := exec.LookPath(tc.shell)
			if err != nil {
				t.Skipf("%s not installed", tc.shell)
			}
			home := t.TempDir()
			t.Setenv("HOME", home)
			t.Setenv("XDG_CACHE_HOME", filepath.Join(home, ".cache"))
			t.Setenv("XDG_CONFIG_HOME", filepath.Join(home, ".config"))
			t.Setenv("STARSHIP_CONFIG", filepath.Join(home, "starship.toml"))
			os.Unsetenv("ZDOTDIR")

			rcFile, rc := tc.setup(t, home)
			// The last line proves the whole rc file ran.
			set := "export JT_USER_RC=setup-loaded\n"
			if tc.shell == "fish" {
				set = "set -gx JT_USER_RC setup-loaded\n"
			}
			full := filepath.Join(home, rcFile)
			os.MkdirAll(filepath.Dir(full), 0o755)
			if err := os.WriteFile(full, []byte(rc+set), 0o644); err != nil {
				t.Fatal(err)
			}
			for i := 0; i < tc.launch; i++ {
				checkIntegration(t, path, nil, home, "setup-loaded")
			}
		})
	}
}

func copyFile(t *testing.T, from, to string) {
	t.Helper()
	b, err := os.ReadFile(from)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(to, b, 0o644); err != nil {
		t.Fatal(err)
	}
}
