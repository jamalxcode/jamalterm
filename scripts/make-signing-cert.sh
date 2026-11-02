#!/bin/bash
# One-time setup, run on YOUR Mac (not in CI): makes a free self-signed
# code-signing certificate for jamalterm and saves it as GitHub secrets,
# so every release is signed with the same identity. Then macOS remembers
# "Always Allow" for jamalterm's Keychain item across updates.
#
# Needs: the GitHub CLI, signed in (brew install gh && gh auth login).
# The private key never leaves this Mac except into the encrypted GitHub
# secret; the temporary files are deleted at the end.
set -euo pipefail
REPO="${1:-jamalxcode/jamalterm}"
command -v gh >/dev/null || { echo "Install the GitHub CLI first: brew install gh && gh auth login"; exit 1; }
DIR=$(mktemp -d); trap 'rm -rf "$DIR"' EXIT
PASS=$(openssl rand -hex 16)

cat > "$DIR/cert.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = jamalterm self-signed
O = jamal xcode
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF
openssl req -x509 -newkey rsa:3072 -nodes -days 3650 -config "$DIR/cert.cnf" -keyout "$DIR/key.pem" -out "$DIR/cert.pem" 2>/dev/null
openssl pkcs12 -export -legacy -inkey "$DIR/key.pem" -in "$DIR/cert.pem" -name "jamalterm self-signed" -passout "pass:$PASS" -out "$DIR/cert.p12" 2>/dev/null \
  || openssl pkcs12 -export -inkey "$DIR/key.pem" -in "$DIR/cert.pem" -name "jamalterm self-signed" -passout "pass:$PASS" -out "$DIR/cert.p12"

base64 < "$DIR/cert.p12" | gh secret set MAC_SIGNING_P12 --repo "$REPO"
printf '%s' "$PASS" | gh secret set MAC_SIGNING_PASSWORD --repo "$REPO"
echo "✓ Saved MAC_SIGNING_P12 and MAC_SIGNING_PASSWORD to $REPO."
echo "  The next release is signed with it. After updating to it, macOS asks about"
echo "  the Keychain one last time: choose Always Allow."
