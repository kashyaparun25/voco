#!/usr/bin/env bash
#
# Voco one-line installer.
#   curl -fsSL https://raw.githubusercontent.com/kashyaparun25/voco/main/scripts/install.sh | bash
#
# Downloads the latest release DMG, installs Voco.app to /Applications, and
# removes the Gatekeeper quarantine flag so the (ad-hoc signed) app opens cleanly.
set -euo pipefail

REPO="kashyaparun25/voco"
APP="Voco"
API="https://api.github.com/repos/${REPO}/releases/latest"

say()  { printf '\033[36m▸\033[0m %s\n' "$1"; }
ok()   { printf '\033[32m✓\033[0m %s\n' "$1"; }
die()  { printf '\033[31m✗\033[0m %s\n' "$1" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || die "Voco is a macOS app."
if [ "$(uname -m)" != "arm64" ]; then
  die "Voco requires an Apple Silicon Mac (arm64). Detected: $(uname -m)."
fi

say "Finding the latest ${APP} release…"
URL=$(curl -fsSL "$API" \
  | grep -o '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*\.dmg"' \
  | head -1 | sed 's/.*"\(https[^"]*\)"/\1/')
[ -n "${URL:-}" ] || die "Couldn't find a .dmg in the latest release. See https://github.com/${REPO}/releases"

TMP="$(mktemp -d)"
DMG="${TMP}/${APP}.dmg"
trap 'rm -rf "$TMP"' EXIT

say "Downloading $(basename "$URL")…"
curl -fSL# "$URL" -o "$DMG"

say "Mounting…"
MNT="$(hdiutil attach "$DMG" -nobrowse -readonly | grep -Eo '/Volumes/[^"]+' | tail -1)"
[ -n "${MNT:-}" ] || die "Failed to mount the disk image."

say "Installing to /Applications…"
rm -rf "/Applications/${APP}.app"
cp -R "${MNT}/${APP}.app" "/Applications/"
hdiutil detach "$MNT" >/dev/null || true

say "Removing quarantine (ad-hoc signed build)…"
xattr -cr "/Applications/${APP}.app" 2>/dev/null || true

ok "${APP} installed to /Applications."
echo "  Launch it from Spotlight or: open -a ${APP}"
