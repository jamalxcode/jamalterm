#!/bin/bash
# Builds build/bin/jamalterm.app for Apple silicon and Intel (macOS 12+), from
# the Rust code. Same bundle id as 3.x, so it's the same app to macOS.
# Usage: scripts/build-mac.sh [version]
set -euo pipefail
cd "$(dirname "$0")/.."
V="${1:-dev}"
PLIST_V="${V%%-*}"; [[ "$PLIST_V" =~ ^[0-9.]+$ ]] || PLIST_V="0.0.0"
export MACOSX_DEPLOYMENT_TARGET=12.0

rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  JAMALTERM_VERSION="$V" cargo build --release --locked --target "$t"
done

APP=build/bin/jamalterm.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/jamalterm" \
  target/aarch64-apple-darwin/release/jamalterm target/x86_64-apple-darwin/release/jamalterm
sed "s/@VERSION@/$PLIST_V/g" build/darwin/Info.plist > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"

# The icon, from build/appicon.png.
ICONSET=$(mktemp -d)/iconfile.iconset
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s build/appicon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) build/appicon.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/iconfile.icns"
echo "Built $APP ($V)"
