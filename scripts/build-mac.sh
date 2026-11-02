#!/bin/bash
# Builds build/bin/jamalterm.app for Apple silicon and Intel (macOS 12+).
# Usage: scripts/build-mac.sh [version]   (run frontend's `npm run build` first)
set -euo pipefail
cd "$(dirname "$0")/.."
V="${1:-dev}"
PLIST_V="${V%%-*}"; [[ "$PLIST_V" =~ ^[0-9.]+$ ]] || PLIST_V="0.0.0"

export CGO_ENABLED=1 MACOSX_DEPLOYMENT_TARGET=12.0
export CGO_CFLAGS="-mmacosx-version-min=12.0" CGO_LDFLAGS="-mmacosx-version-min=12.0"
# private_mac_apis: lets the window be see-through (Settings → Window opacity).
TAGS="production,private_mac_apis"
LDFLAGS="-w -s -X main.version=$V"

mkdir -p build/bin
for arch in arm64 amd64; do
  GOOS=darwin GOARCH=$arch go build -tags "$TAGS" -trimpath -buildvcs=false -ldflags "$LDFLAGS" -o "build/bin/jamalterm-$arch" .
done

APP=build/bin/jamalterm.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/jamalterm" build/bin/jamalterm-arm64 build/bin/jamalterm-amd64
rm build/bin/jamalterm-arm64 build/bin/jamalterm-amd64
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
