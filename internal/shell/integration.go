package shell

import (
	"embed"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
)

// The scripts are embedded in the app and written to disk on first use.
//
//go:embed all:integration
var integrationFS embed.FS

// installIntegration writes the scripts to dir (overwriting old versions).
func installIntegration(dir string) error {
	return fs.WalkDir(integrationFS, "integration", func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel := strings.TrimPrefix(path, "integration")
		target := filepath.Join(dir, rel)
		if d.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		data, err := integrationFS.ReadFile(path)
		if err != nil {
			return err
		}
		if old, err := os.ReadFile(target); err == nil && string(old) == string(data) {
			return nil
		}
		return os.WriteFile(target, data, 0o644)
	})
}

// IntegrationDir is where the scripts are installed.
func IntegrationDir() string {
	base, err := os.UserCacheDir()
	if err != nil {
		base = os.TempDir()
	}
	return filepath.Join(base, "jamalterm", "shell-integration")
}

// withIntegration changes how the shell is started so it loads our hooks
// after the user's own startup files. The user's files are never edited.
// Unknown shells (fish, nu, ...) start unchanged.
func withIntegration(shellPath string, args []string, env []string, home string) ([]string, []string) {
	dir := IntegrationDir()
	if err := installIntegration(dir); err != nil {
		return args, env
	}
	login := len(args) == 1 && args[0] == "-l"
	env = append(env, "JAMALTERM_INTEGRATION="+dir)

	switch filepath.Base(shellPath) {
	case "zsh":
		// zsh reads $ZDOTDIR/.zshenv first; ours restores ZDOTDIR and then
		// zsh carries on with the user's .zprofile, .zshrc, .zlogin.
		env = append(env, "ZDOTDIR="+filepath.Join(dir, "zsh"))
		if z, ok := os.LookupEnv("ZDOTDIR"); ok {
			env = append(env, "JAMALTERM_ZDOTDIR="+z)
		}
		return args, env

	case "bash":
		// --rcfile only applies to non-login shells, so rc.bash reads the
		// login files itself when a login shell was asked for.
		if login {
			env = append(env, "JAMALTERM_BASH_LOGIN=1")
			args = nil
		} else if len(args) > 0 {
			return args, env // custom arguments: leave alone
		}
		return append([]string{"--rcfile", filepath.Join(dir, "bash", "rc.bash"), "-i"}, args...), env

	case "fish":
		// fish runs -C commands after its own config.
		return append(args, "-C", "source "+shellQuote(filepath.Join(dir, "fish", "integration.fish"))), env

	case "tcsh", "csh":
		// tcsh has no --rcfile; point HOME at our .tcshrc for a moment.
		env = append(env, "HOME="+filepath.Join(dir, "tcsh"), "JAMALTERM_HOME="+home)
		return args, env
	}
	return args, env
}

func shellQuote(s string) string { return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'" }
