#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
APP="/Applications/TermSurf Ghostboard.app"
SRC="$REPO_DIR/ghostboard/macos/build/ReleaseLocal/TermSurf Ghostboard.app"
WEB="$REPO_DIR/webtui/target/release/web"

# Verify release build exists.
if [ ! -d "$SRC" ]; then
  echo "Error: Release build not found at $SRC"
  echo "Run build-release.sh first."
  exit 1
fi

# Copy app bundle.
echo "==> Installing to $APP..."
rm -rf "$APP"
cp -R "$SRC" "$APP"

# Bundle web TUI.
if [ -f "$WEB" ]; then
  echo "==> Bundling web TUI..."
  cp "$WEB" "$APP/Contents/MacOS/web"
else
  echo "Warning: web TUI not found at $WEB (skipping)"
fi

# Re-sign the app bundle. The additions above invalidate the original code
# signature, which breaks XPC (SMAppService, anonymous listeners). Ad-hoc
# signing is sufficient for local development.
echo "==> Re-signing app bundle..."
codesign --force --deep --sign - "$APP"

# Unregister build tree copies so Spotlight only finds /Applications.
echo "==> Unregistering build tree copies from Launch Services..."
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"
"$LSREGISTER" -u "$REPO_DIR/ghostboard/macos/build/Debug/TermSurf Ghostboard Debug.app" 2>/dev/null || true
"$LSREGISTER" -u "$REPO_DIR/ghostboard/macos/build/Debug/TermSurf Ghostboard.app" 2>/dev/null || true
"$LSREGISTER" -u "$REPO_DIR/ghostboard/macos/build/ReleaseLocal/TermSurf Ghostboard.app" 2>/dev/null || true

# Symlink CLI tools.
echo "==> Symlinking CLI tools to /usr/local/bin/..."
ln -sf "$APP/Contents/MacOS/termsurf" /usr/local/bin/termsurf
ln -sf "$APP/Contents/MacOS/web" /usr/local/bin/web

echo ""
echo "Done."
echo "  App:  $APP"
echo "  CLI:  /usr/local/bin/termsurf"
echo "  Web:  /usr/local/bin/web"
