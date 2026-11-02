package update

// Checking that a release was built by GitHub from this repository.
//
// Each release carries a Sigstore bundle (jamalterm-X-macos.zip.sigstore.json)
// made by GitHub's attest-build-provenance action. It proves, through
// Sigstore's public certificate authority and transparency log, that the
// zip with exactly these bytes was built by this repository's build.yml
// workflow on GitHub. No signing key is kept anywhere that could leak.

import (
	"context"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/sigstore/sigstore-go/pkg/bundle"
	"github.com/sigstore/sigstore-go/pkg/root"
	"github.com/sigstore/sigstore-go/pkg/tuf"
	sigverify "github.com/sigstore/sigstore-go/pkg/verify"
)

// BuiltBy is the workflow that must have built a release.
const (
	builderIssuer = "https://token.actions.githubusercontent.com"
	builderSAN    = `^https://github\.com/jamalxcode/jamalterm/\.github/workflows/build\.yml@refs/(heads/main|tags/v[0-9.]+(-[A-Za-z0-9.]+)?)$`
)

// TrustCache is where Sigstore's public keys are cached (set by the app).
var TrustCache = ""

// trustedRoot fetches Sigstore's public keys (signed and cached by TUF).
var trustedRoot = func(ctx context.Context) (root.TrustedMaterial, error) {
	opts := tuf.DefaultOptions().WithContext(ctx)
	if TrustCache != "" {
		os.MkdirAll(TrustCache, 0o700)
		opts = opts.WithCachePath(TrustCache)
	} else {
		opts = opts.WithCachePath(filepath.Join(os.TempDir(), "jamalterm-sigstore"))
	}
	return root.FetchTrustedRootWithOptions(opts)
}

// VerifyProvenance checks that bundleJSON proves this repository's GitHub
// workflow built the file whose SHA-256 is sha256Hex.
func VerifyProvenance(ctx context.Context, bundleJSON []byte, sha256Hex string) error {
	digest, err := hex.DecodeString(sha256Hex)
	if err != nil || len(digest) != 32 {
		return errors.New("bad checksum")
	}
	var b bundle.Bundle
	if err := b.UnmarshalJSON(bundleJSON); err != nil {
		return fmt.Errorf("the build proof is damaged: %w", err)
	}
	trusted, err := trustedRoot(ctx)
	if err != nil {
		return fmt.Errorf("couldn't get Sigstore's public keys to check the download: %w", err)
	}
	v, err := sigverify.NewVerifier(trusted, sigverify.WithSignedCertificateTimestamps(1), sigverify.WithTransparencyLog(1), sigverify.WithObserverTimestamps(1))
	if err != nil {
		return err
	}
	id, err := sigverify.NewShortCertificateIdentity(builderIssuer, "", "", builderSAN)
	if err != nil {
		return err
	}
	if _, err := v.Verify(&b, sigverify.NewPolicy(sigverify.WithArtifactDigest("sha256", digest), sigverify.WithCertificateIdentity(id))); err != nil {
		return fmt.Errorf("the download isn't proven to be built by jamalterm's GitHub workflow: %w", err)
	}
	return nil
}
