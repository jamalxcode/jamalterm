package shell

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func TestGitBranch(t *testing.T) {
	root := t.TempDir()
	if GitBranch(root) != "" {
		t.Fatal("not a repo")
	}
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git not installed")
	}
	run := func(dir string, args ...string) {
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(), "GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@t", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@t")
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v %s", args, err, out)
		}
	}
	repo := filepath.Join(root, "repo")
	sub := filepath.Join(repo, "a", "b")
	os.MkdirAll(sub, 0o755)
	run(repo, "init", "-q", "-b", "feature/tabs")
	if got := GitBranch(sub); got != "feature/tabs" {
		t.Fatalf("branch from a subfolder = %q", got)
	}
	run(repo, "commit", "-q", "--allow-empty", "-m", "x")
	run(repo, "worktree", "add", "-q", "-b", "other", filepath.Join(root, "wt"))
	if got := GitBranch(filepath.Join(root, "wt")); got != "other" {
		t.Fatalf("worktree branch = %q", got)
	}
	run(repo, "checkout", "-q", "--detach")
	if got := GitBranch(repo); len(got) != 7 {
		t.Fatalf("detached = %q", got)
	}
}
