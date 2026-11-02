// Command verify-release checks a release zip against its Sigstore proof,
// exactly as jamalterm's updater does (CI runs it on every new build).
//
//	go run ./cmd/verify-release jamalterm-X-macos.zip jamalterm-X-macos.zip.sigstore.json [other-file]
//
// With other-file it also checks that the proof does NOT pass for another
// file, so a check that accepts anything can't slip through.
package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"os"
	"time"

	"jamalterm/internal/update"
)

func sum(path string) string {
	f, err := os.Open(path)
	if err != nil {
		fail(err)
	}
	defer f.Close()
	h := sha256.New()
	io.Copy(h, f)
	return hex.EncodeToString(h.Sum(nil))
}

func fail(err error) {
	fmt.Fprintln(os.Stderr, "verify-release:", err)
	os.Exit(1)
}

func main() {
	if len(os.Args) < 3 {
		fail(fmt.Errorf("usage: verify-release <zip> <bundle> [other-file]"))
	}
	proof, err := os.ReadFile(os.Args[2])
	if err != nil {
		fail(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()
	if err := update.VerifyProvenance(ctx, proof, sum(os.Args[1])); err != nil {
		fail(err)
	}
	fmt.Println("✓ proven: built by jamalterm's GitHub workflow:", os.Args[1])
	if len(os.Args) > 3 {
		if err := update.VerifyProvenance(ctx, proof, sum(os.Args[3])); err == nil {
			fail(fmt.Errorf("the proof also passed for %s; the check is broken", os.Args[3]))
		}
		fmt.Println("✓ and it is rejected for a different file:", os.Args[3])
	}
}
