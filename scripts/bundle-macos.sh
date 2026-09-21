#!/usr/bin/env bash
# Builds "WTM Video.app" (with its Finder/Dock icon) into target/bundle/.
# Run on a Mac:  ./scripts/bundle-macos.sh
set -euo pipefail
cd "$(dirname "$0")/.."

APP="target/bundle/WTM Video.app"
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)

# BINARY can point at a prebuilt program (the release packaging uses a
# universal one); otherwise build it here.
BINARY="${BINARY:-}"
if [ -z "$BINARY" ]; then
  cargo build --release -p wtm_gui
  BINARY=target/release/wtm-video-gui
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BINARY" "$APP/Contents/MacOS/wtm-video-gui"

# Build icon.icns from assets/icon.png using the tools that ship with macOS.
ICONSET="$(mktemp -d)/icon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" assets/icon.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  sips -z "$((size * 2))" "$((size * 2))" assets/icon.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/icon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>WTM Video</string>
  <key>CFBundleDisplayName</key><string>WTM Video</string>
  <key>CFBundleIdentifier</key><string>com.wtm.video</string>
  <key>CFBundleExecutable</key><string>wtm-video-gui</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# Ad-hoc sign so Apple Silicon Macs are happy to launch it locally.
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true

echo "Built: $APP"
