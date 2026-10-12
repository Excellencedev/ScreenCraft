#!/usr/bin/env bash
# Builds ScreenCraft in release mode and assembles a double-clickable ScreenCraft.app.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p screencraft

APP="dist/ScreenCraft.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/screencraft "$APP/Contents/MacOS/ScreenCraft"

# icon.icns from the PNG
ICONSET="$(mktemp -d)/ScreenCraft.iconset"
mkdir -p "$ICONSET"
for s in 16 32 64 128 256 512; do
  sips -z $s $s src-tauri/icons/icon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
done
cp "$ICONSET/icon_32x32.png"   "$ICONSET/icon_16x16@2x.png"
cp "$ICONSET/icon_64x64.png"   "$ICONSET/icon_32x32@2x.png"
cp "$ICONSET/icon_256x256.png" "$ICONSET/icon_128x128@2x.png"
cp "$ICONSET/icon_512x512.png" "$ICONSET/icon_256x256@2x.png"
cp src-tauri/icons/icon.png    "$ICONSET/icon_512x512@2x.png"
rm -f "$ICONSET/icon_64x64.png"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/ScreenCraft.icns"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>ScreenCraft</string>
  <key>CFBundleDisplayName</key><string>ScreenCraft</string>
  <key>CFBundleIdentifier</key><string>dev.screencraft.app</string>
  <key>CFBundleExecutable</key><string>ScreenCraft</string>
  <key>CFBundleIconFile</key><string>ScreenCraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>15.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSCameraUsageDescription</key><string>ScreenCraft uses the camera to add your face to recordings.</string>
  <key>NSMicrophoneUsageDescription</key><string>ScreenCraft uses the microphone to record narration.</string>
  <key>NSScreenCaptureUsageDescription</key><string>ScreenCraft records your screen.</string>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true
echo "Built $APP"
