//go:build darwin && cgo

package settings

import (
	"fmt"
	"os/exec"
	"strings"
	"testing"
	"time"
)

// Runs on the macOS CI machine against the real login Keychain.
func TestNativeKeychain(t *testing.T) {
	service := fmt.Sprintf("jamalterm-test-%d", time.Now().UnixNano())
	if err := keychainSet(service, "k", []byte("v1")); err != nil {
		t.Skipf("no usable Keychain here: %v", err)
	}
	defer keychainDelete(service, "k")
	got, found, err := keychainGet(service, "k")
	if err != nil || !found || string(got) != "v1" {
		t.Fatalf("get: %q %v %v", got, found, err)
	}
	keychainSet(service, "k", []byte("v2"))
	if got, _, _ := keychainGet(service, "k"); string(got) != "v2" {
		t.Fatalf("replace: %q", got)
	}
	keychainDelete(service, "k")
	if _, found, _ := keychainGet(service, "k"); found {
		t.Fatal("still there after delete")
	}
}

func TestKeysMoveFromSecurityTool(t *testing.T) {
	service := fmt.Sprintf("jamalterm-test-%d", time.Now().UnixNano())
	// Pretend 1.x saved two keys with the security tool.
	for p, k := range map[string]string{"anthropic": "sk-ant-old", "openrouter": "sk-or-old"} {
		if out, err := exec.Command("/usr/bin/security", "add-generic-password", "-a", "jamalterm", "-s", legacyService(service, p), "-w", k).CombinedOutput(); err != nil {
			t.Skipf("no usable Keychain here: %v %s", err, out)
		}
	}
	s := &Secrets{useKeychain: true, service: service}
	defer keychainDelete(service, keychainAccount)
	if s.KeyFor("anthropic") != "sk-ant-old" || s.KeyFor("openrouter") != "sk-or-old" {
		t.Fatal("keys not moved")
	}
	if legacyGet(service, "anthropic") != "" {
		t.Fatal("old item not deleted")
	}
	// A fresh reader finds them in the one native item.
	s2 := &Secrets{useKeychain: true, service: service}
	if s2.KeyFor("openrouter") != "sk-or-old" {
		t.Fatal("not stored natively")
	}
	s2.SetKeyFor("openrouter", "")
	s3 := &Secrets{useKeychain: true, service: service}
	if s3.HasSavedKeyFor("openrouter") || !strings.HasPrefix(s3.KeyFor("anthropic"), "sk-ant") {
		t.Fatal("delete removed the wrong key")
	}
}
