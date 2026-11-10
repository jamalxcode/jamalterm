package shell

import (
	"os"
	"path/filepath"
	"strings"
)

// GitBranch returns the branch checked out in dir or a parent folder ("" when
// it isn't a git repository). It only reads .git/HEAD, so it's instant and
// works for worktrees too. A detached HEAD gives the short commit id.
func GitBranch(dir string) string {
	for d := dir; d != "" && d != "/" && d != "."; d = filepath.Dir(d) {
		gitPath := filepath.Join(d, ".git")
		fi, err := os.Stat(gitPath)
		if err != nil {
			continue
		}
		gitDir := gitPath
		if !fi.IsDir() {
			// Worktree or submodule: ".git" is a file "gitdir: <path>".
			data, err := os.ReadFile(gitPath)
			if err != nil {
				return ""
			}
			line := strings.TrimSpace(string(data))
			if !strings.HasPrefix(line, "gitdir:") {
				return ""
			}
			gitDir = strings.TrimSpace(strings.TrimPrefix(line, "gitdir:"))
			if !filepath.IsAbs(gitDir) {
				gitDir = filepath.Join(d, gitDir)
			}
		}
		head, err := os.ReadFile(filepath.Join(gitDir, "HEAD"))
		if err != nil {
			return ""
		}
		h := strings.TrimSpace(string(head))
		if ref, ok := strings.CutPrefix(h, "ref: refs/heads/"); ok {
			return ref
		}
		if len(h) >= 7 {
			return h[:7] // detached HEAD
		}
		return ""
	}
	return ""
}

// GitRoot returns the folder holding .git for dir or a parent ("" when it
// isn't in a repository).
func GitRoot(dir string) string {
	for d := dir; d != "" && d != "/" && d != "."; d = filepath.Dir(d) {
		if _, err := os.Stat(filepath.Join(d, ".git")); err == nil {
			return d
		}
	}
	return ""
}
