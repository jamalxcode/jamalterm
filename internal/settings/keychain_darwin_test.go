//go:build darwin && cgo

package settings

import (
	"fmt"
	"os/exec"
	"testing"
	"time"
)

// Runs on the macOS CI machine: keys an older version left in the Keychain
// can be moved out on request, and checking for them doesn't read them.
func TestMoveFromKeychain(t *testing.T) {
	service := fmt.Sprintf("jamalterm-test-%d", time.Now().UnixNano())
	// -A: any app may read it, so the test never waits on a prompt.
	if out, err := exec.Command("/usr/bin/security", "add-generic-password", "-A", "-s", service, "-a", keychainAccount,
		"-w", `{"anthropic":"sk-ant-old","openrouter":"sk-or-old"}`).CombinedOutput(); err != nil {
		t.Skipf("no usable Keychain here: %v %s", err, out)
	}
	defer keychainDelete(service, keychainAccount)
	s := &Secrets{dir: t.TempDir(), machine: func() string { return "m" }, keychainService: service}
	if !s.KeychainKeysWaiting() {
		t.Fatal("didn't see the old item")
	}
	if s.KeyFor("openrouter") == "sk-or-old" {
		t.Fatal("read the Keychain without being asked")
	}
	s.SetKeyFor("anthropic", "sk-ant-new") // a key saved here wins
	n, err := s.MoveFromKeychain()
	if err != nil || n != 1 {
		t.Fatalf("moved %d, %v", n, err)
	}
	if s.KeyFor("openrouter") != "sk-or-old" || s.KeyFor("anthropic") != "sk-ant-new" {
		t.Fatal("wrong keys after the move")
	}
	if s.KeychainKeysWaiting() {
		t.Fatal("Keychain item not deleted")
	}
}
