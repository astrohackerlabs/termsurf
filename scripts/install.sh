#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
CHROMIUM_OUT="$REPO_DIR/chromium/src/out/Default"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"
GHOSTBOARD_RELEASE_APP="$REPO_DIR/ghostboard/macos/build/Release/TermSurf Ghostboard.app"

COMPONENT="${1:-}"

if [ -z "$COMPONENT" ]; then
  echo "Usage: $0 <component>"
  echo "Components: wezboard, ghostboard, roamium, webtui, all"
  exit 1
fi

if [ "$COMPONENT" = "ghostboard" ] && [ ! -x "$GHOSTBOARD_RELEASE_APP/Contents/MacOS/ghostboard" ]; then
  echo "Error: Release app not found at $GHOSTBOARD_RELEASE_APP"
  echo "Run: scripts/build.sh ghostboard --release"
  exit 1
fi

# Re-exec as root so we only prompt for the password once.
if [ "$(id -u)" -ne 0 ]; then
  exec sudo "$0" "$@"
fi

install_roamium() {
  local ROAMIUM_SRC="$REPO_DIR/target/release/roamium"
  local INSTALL_DIR="/usr/local/roamium"

  if [ ! -f "$ROAMIUM_SRC" ]; then
    echo "Error: Release build not found at $ROAMIUM_SRC"
    echo "Run: scripts/build.sh roamium --release"
    exit 1
  fi

  echo "==> Installing Roamium to $INSTALL_DIR..."
  mkdir -p "$INSTALL_DIR"
  cp "$ROAMIUM_SRC" "$INSTALL_DIR/roamium"

  echo "==> Copying dylibs..."
  cp "$CHROMIUM_OUT"/*.dylib "$INSTALL_DIR/"

  echo "==> Copying resources..."
  cp "$CHROMIUM_OUT"/*.pak "$INSTALL_DIR/"
  cp "$CHROMIUM_OUT/icudtl.dat" "$INSTALL_DIR/"
  cp "$CHROMIUM_OUT"/v8_context_snapshot*.bin "$INSTALL_DIR/"

  echo "==> Codesigning Roamium..."
  codesign --force --sign - "$INSTALL_DIR/roamium" || true

  # Clean up old install locations.
  rm -f /usr/local/bin/roamium
  rm -rf /usr/local/lib/roamium

  echo "  Dir: $INSTALL_DIR"
  echo "  Bin: $INSTALL_DIR/roamium"
}

install_wezboard() {
  local BINARY="$REPO_DIR/wezboard/target/release/wezboard-gui"
  local TEMPLATE="$REPO_DIR/wezboard/assets/macos/TermSurf Wezboard.app"
  local APP="/Applications/TermSurf Wezboard.app"

  if [ ! -f "$BINARY" ]; then
    echo "Error: Release build not found at $BINARY"
    echo "Run: scripts/build.sh wezboard --release"
    exit 1
  fi

  echo "==> Installing Wezboard to $APP..."
  rm -rf "$APP"
  cp -R "$TEMPLATE" "$APP"
  mkdir -p "$APP/Contents/MacOS"
  cp "$BINARY" "$APP/Contents/MacOS/wezboard-gui"

  echo "==> Codesigning..."
  codesign --force --deep --sign - "$APP" || true

  local CLI="$REPO_DIR/wezboard/target/release/wezboard"
  if [ -f "$CLI" ]; then
    echo "==> Installing wezboard CLI to /usr/local/bin/wezboard..."
    cp "$CLI" /usr/local/bin/wezboard
    codesign --force --sign - /usr/local/bin/wezboard || true
    echo "  Bin: /usr/local/bin/wezboard"
  fi

  echo "  App: $APP"
}

install_ghostboard() {
  local APP_SRC="$GHOSTBOARD_RELEASE_APP"
  local APP="/Applications/TermSurf Ghostboard.app"

  if [ ! -x "$APP_SRC/Contents/MacOS/ghostboard" ]; then
    echo "Error: Release app not found at $APP_SRC"
    echo "Run: scripts/build.sh ghostboard --release"
    exit 1
  fi

  echo "==> Installing Ghostboard to $APP..."
  rm -rf "$APP"
  cp -R "$APP_SRC" "$APP"

  echo "==> Codesigning..."
  codesign --force --deep --sign - "$APP" || true

  if [ -x "$LSREGISTER" ]; then
    "$LSREGISTER" -f -R -trusted "$APP" || true
  fi

  echo "  App: $APP"
}

install_webtui() {
  local WEB="$REPO_DIR/target/release/web"

  if [ ! -f "$WEB" ]; then
    echo "Error: Release build not found at $WEB"
    echo "Run: scripts/build.sh webtui --release"
    exit 1
  fi

  echo "==> Installing webtui to /usr/local/bin/web..."
  cp "$WEB" /usr/local/bin/web
  codesign --force --sign - /usr/local/bin/web || true

  echo "  Bin: /usr/local/bin/web"
}

case "$COMPONENT" in
  roamium)    install_roamium ;;
  wezboard)   install_wezboard ;;
  ghostboard) install_ghostboard ;;
  webtui)     install_webtui ;;
  all)
    install_roamium
    install_wezboard
    install_ghostboard
    install_webtui
    echo ""
    echo "Done (all)."
    ;;
  *)
    echo "Unknown component: $COMPONENT"
    echo "Components: wezboard, ghostboard, roamium, webtui, all"
    exit 1
    ;;
esac
