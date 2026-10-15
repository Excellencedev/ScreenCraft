#!/usr/bin/env bash
# Builds ScreenCraft and packs it into dist/ScreenCraft-<version>.dmg with a drag-to-
# Applications layout. Usage: ./scripts/dmg.sh
set -euo pipefail
cd "$(dirname "$0")/.."

./scripts/bundle.sh

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
VOL="ScreenCraft"
OUT="dist/ScreenCraft-${VERSION}.dmg"
RW="dist/.ScreenCraft-rw.dmg"
STAGE="$(mktemp -d)/$VOL"
mkdir -p "$STAGE"
cp -R dist/ScreenCraft.app "$STAGE/"
ln -s /Applications "$STAGE/Applications"

rm -f "$OUT" "$RW"
hdiutil create -quiet -volname "$VOL" -srcfolder "$STAGE" -fs HFS+ -format UDRW -ov "$RW"

# Best effort: icon view with the app on the left and Applications on the right.
hdiutil detach -quiet "/Volumes/$VOL" 2>/dev/null || true
MNT="$(hdiutil attach -readwrite -noverify -noautoopen "$RW" | awk -F'\t' '/\/Volumes\//{print $NF}' | tail -1)"
test -d "$MNT" || { echo "could not mount $RW"; exit 1; }
VOL="$(basename "$MNT")"
osascript <<OSA >/dev/null 2>&1 || echo "note: could not style the disk image window (Finder automation unavailable); it still works"
tell application "Finder"
  tell disk "$VOL"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {200, 140, 760, 460}
    set opts to the icon view options of container window
    set arrangement of opts to not arranged
    set icon size of opts to 112
    set position of item "ScreenCraft.app" of container window to {150, 160}
    set position of item "Applications" of container window to {410, 160}
    close
    open
    update without registering applications
    delay 1
    close
  end tell
end tell
OSA
# the mounted disk shows the app icon (travels inside the image)
cp dist/ScreenCraft.app/Contents/Resources/ScreenCraft.icns "$MNT/.VolumeIcon.icns"
SetFile -a C "$MNT" 2>/dev/null || true
SetFile -a V "$MNT/.VolumeIcon.icns" 2>/dev/null || true
sync
hdiutil detach -quiet "$MNT" || hdiutil detach -force -quiet "$MNT"
hdiutil convert -quiet "$RW" -format UDZO -imagekey zlib-level=9 -o "$OUT"
rm -f "$RW"
# the .dmg file itself gets the app icon in Finder on this Mac (a download strips it, the
# mounted volume icon above is what everyone else sees)
swift -e 'import AppKit
let a = CommandLine.arguments
if let i = NSImage(contentsOfFile: a[1]) { NSWorkspace.shared.setIcon(i, forFile: a[2], options: []) }' dist/ScreenCraft.app/Contents/Resources/ScreenCraft.icns "$OUT" 2>/dev/null || true
# the repo keeps the current installer at its root under a fixed name
cp "$OUT" ScreenCraft.dmg
echo "Built $OUT ($(du -h "$OUT" | cut -f1)) and copied it to ./ScreenCraft.dmg"
