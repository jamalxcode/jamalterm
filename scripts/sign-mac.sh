#!/bin/bash
# Signs build/bin/jamalterm.app (CI). Three levels, depending on which
# GitHub secrets exist:
#   none                     ad hoc (works; macOS asks about the Keychain after each update)
#   MAC_SIGNING_P12 (+ MAC_SIGNING_PASSWORD)
#                            a stable certificate: the Keychain "Always Allow" survives
#                            updates. A self-signed one from scripts/make-signing-cert.sh
#                            is enough; an Apple "Developer ID Application" one is better.
#   + APPLE_ID, APPLE_APP_PASSWORD, APPLE_TEAM_ID (with a Developer ID certificate)
#                            notarized too: no more "unidentified developer" warning.
set -euo pipefail
APP="${1:-build/bin/jamalterm.app}"

if [[ -z "${MAC_SIGNING_P12:-}" ]]; then
  echo "Signing ad hoc (no MAC_SIGNING_P12 secret)."
  codesign --force --deep --sign - "$APP"
  codesign --verify --deep --strict "$APP"
  exit 0
fi

KC="$RUNNER_TEMP/jamalterm-signing.keychain-db"
KCPASS=$(openssl rand -hex 16)
security create-keychain -p "$KCPASS" "$KC"
security set-keychain-settings -lut 3600 "$KC"
security unlock-keychain -p "$KCPASS" "$KC"
echo "$MAC_SIGNING_P12" | base64 --decode > "$RUNNER_TEMP/cert.p12"
security import "$RUNNER_TEMP/cert.p12" -k "$KC" -P "${MAC_SIGNING_PASSWORD:-}" -T /usr/bin/codesign >/dev/null
rm -f "$RUNNER_TEMP/cert.p12"
security set-key-partition-list -S apple-tool:,apple: -s -k "$KCPASS" "$KC" >/dev/null
security list-keychains -d user -s "$KC" $(security list-keychains -d user | tr -d '"')

# The first code-signing identity in that keychain (self-signed ones count).
LINE=$(security find-identity -p codesigning "$KC" | awk '/[0-9]+\)/ {print; exit}')
ID=$(echo "$LINE" | awk '{print $2}')
NAME=$(echo "$LINE" | sed -E 's/^[^"]*"(.*)".*$/\1/')
[[ -n "$ID" ]] || { echo "No code-signing identity in MAC_SIGNING_P12" >&2; exit 1; }
echo "Signing with: $NAME"

if [[ "$NAME" == Developer\ ID\ Application* ]]; then
  # Hardened runtime + secure timestamp: required for notarization.
  codesign --force --deep --options runtime --timestamp --sign "$ID" "$APP"
else
  codesign --force --deep --sign "$ID" "$APP"
fi
codesign --verify --deep --strict "$APP"
codesign -dr - "$APP" 2>&1 | tail -1

if [[ "$NAME" == Developer\ ID\ Application* && -n "${APPLE_ID:-}" && -n "${APPLE_APP_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
  echo "Notarizing…"
  ditto -c -k --keepParent "$APP" "$RUNNER_TEMP/notarize.zip"
  xcrun notarytool submit "$RUNNER_TEMP/notarize.zip" --apple-id "$APPLE_ID" --password "$APPLE_APP_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
  xcrun stapler staple "$APP"
  spctl --assess --type execute -v "$APP"
fi
