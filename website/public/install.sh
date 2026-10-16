#!/bin/bash
# ScreenCraft installer for macOS.
#   curl -fsSL https://screencraft.mdnlabs.dev/install.sh | bash
#
# Downloads the latest ScreenCraft disk image with curl (so macOS does not tag it as "downloaded from
# the internet"), copies the app to Applications and opens it. Read it first if you like:
# it only downloads the disk image from this website and copies the app out of it.
set -euo pipefail

SITES=("https://screencraft.mdnlabs.dev" "https://getscreencraft.pages.dev")
URLS=()
# the manifest names the exact, versioned file for the newest release
for s in "${SITES[@]}"; do
  u="$(curl -fsSL -m 10 "$s/latest.json" 2>/dev/null | sed -n 's/.*"mac": *"\([^"]*\)".*/\1/p' | head -1 || true)"
  [ -n "$u" ] && URLS+=("$u")
done

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This installer is for macOS. Windows users: https://screencraft.mdnlabs.dev/download" >&2
  exit 1
fi

tmp="$(mktemp -d)"
mnt=""
cleanup() {
  [ -n "$mnt" ] && hdiutil detach -quiet "$mnt" 2>/dev/null || true
  rm -rf "$tmp"
}
trap cleanup EXIT

[ ${#URLS[@]} -gt 0 ] || { echo "Could not reach the ScreenCraft website. Check your connection and try again." >&2; exit 1; }
echo "Downloading ScreenCraft..."
ok=""
for u in "${URLS[@]}"; do
  if curl -fL --progress-bar -o "$tmp/ScreenCraft.dmg" "$u"; then ok=1; break; fi
done
[ -n "$ok" ] || { echo "Could not download ScreenCraft. Check your connection and try again." >&2; exit 1; }

mnt="$(hdiutil attach -nobrowse -readonly -noautoopen "$tmp/ScreenCraft.dmg" | awk -F'\t' '/\/Volumes\//{print $NF}' | tail -1)"
[ -d "$mnt/ScreenCraft.app" ] || { echo "The disk image did not contain ScreenCraft.app." >&2; exit 1; }

# quit a running copy so it can be replaced
osascript -e 'tell application "ScreenCraft" to quit' >/dev/null 2>&1 || true
sleep 1

dest="/Applications"
[ -w "$dest" ] || { dest="$HOME/Applications"; mkdir -p "$dest"; }
rm -rf "$dest/ScreenCraft.app"
cp -R "$mnt/ScreenCraft.app" "$dest/"
xattr -dr com.apple.quarantine "$dest/ScreenCraft.app" 2>/dev/null || true

echo "Installed to $dest/ScreenCraft.app"
open "$dest/ScreenCraft.app"
