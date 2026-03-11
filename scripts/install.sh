#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
CHROMIUM_OUT="$REPO_DIR/chromium/src/out/Default"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"

COMPONENT="${1:-}"

if [ -z "$COMPONENT" ]; then
  echo "Usage: $0 <component>"
  echo "Components: ghostboard, wezboard, roamium, webtui, all"
  exit 1
fi

install_ghostboard() {
  local APP="/Applications/TermSurf Ghostboard.app"
  local SRC="$REPO_DIR/ghostboard/macos/build/ReleaseLocal/TermSurf Ghostboard.app"
  local WEB="$REPO_DIR/webtui/target/release/web"

  if [ ! -d "$SRC" ]; then
    echo "Error: Release build not found at $SRC"
    echo "Run: scripts/build.sh ghostboard --release"
    exit 1
  fi

  echo "==> Installing Ghostboard to $APP..."
  sudo rm -rf "$APP"
  sudo cp -R "$SRC" "$APP"

  # Bundle web TUI.
  if [ -f "$WEB" ]; then
    echo "==> Bundling web TUI..."
    sudo cp "$WEB" "$APP/Contents/MacOS/web"
  else
    echo "Warning: web TUI not found at $WEB (skipping)"
  fi

  # Re-sign.
  echo "==> Codesigning..."
  sudo codesign --force --deep --sign - "$APP"

  # Unregister build tree copies from Launch Services.
  echo "==> Unregistering build tree copies..."
  "$LSREGISTER" -u "$REPO_DIR/ghostboard/macos/build/Debug/TermSurf Ghostboard Debug.app" 2>/dev/null || true
  "$LSREGISTER" -u "$REPO_DIR/ghostboard/macos/build/Debug/TermSurf Ghostboard.app" 2>/dev/null || true
  "$LSREGISTER" -u "$REPO_DIR/ghostboard/macos/build/ReleaseLocal/TermSurf Ghostboard.app" 2>/dev/null || true

  # Symlinks.
  echo "==> Symlinking CLI tools..."
  sudo ln -sf "$APP/Contents/MacOS/termsurf" /usr/local/bin/termsurf
  sudo ln -sf "$APP/Contents/MacOS/web" /usr/local/bin/web

  echo "  App:  $APP"
  echo "  CLI:  /usr/local/bin/termsurf"
  echo "  Web:  /usr/local/bin/web"
}

install_roamium() {
  local ROAMIUM_SRC="$REPO_DIR/roamium/target/release/roamium"
  local INSTALL_DIR="/usr/local/roamium"

  if [ ! -f "$ROAMIUM_SRC" ]; then
    echo "Error: Release build not found at $ROAMIUM_SRC"
    echo "Run: scripts/build.sh roamium --release"
    exit 1
  fi

  echo "==> Installing Roamium to $INSTALL_DIR..."
  sudo mkdir -p "$INSTALL_DIR"
  sudo cp "$ROAMIUM_SRC" "$INSTALL_DIR/roamium"

  echo "==> Copying dylibs..."
  sudo cp "$CHROMIUM_OUT"/*.dylib "$INSTALL_DIR/"

  echo "==> Copying resources..."
  sudo cp "$CHROMIUM_OUT"/*.pak "$INSTALL_DIR/"
  sudo cp "$CHROMIUM_OUT/icudtl.dat" "$INSTALL_DIR/"
  sudo cp "$CHROMIUM_OUT"/v8_context_snapshot*.bin "$INSTALL_DIR/"

  # Clean up old install locations.
  sudo rm -f /usr/local/bin/roamium
  sudo rm -rf /usr/local/lib/roamium

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
  sudo rm -rf "$APP"
  sudo cp -R "$TEMPLATE" "$APP"
  sudo mkdir -p "$APP/Contents/MacOS"
  sudo cp "$BINARY" "$APP/Contents/MacOS/wezboard-gui"

  echo "==> Codesigning..."
  sudo codesign --force --deep --sign - "$APP"

  echo "  App: $APP"
}

install_webtui() {
  echo "webtui is bundled inside Ghostboard during 'install.sh ghostboard'."
  echo "To install standalone, run: scripts/install.sh ghostboard"
}

case "$COMPONENT" in
  ghostboard) install_ghostboard ;;
  roamium)    install_roamium ;;
  wezboard)   install_wezboard ;;
  webtui)     install_webtui ;;
  all)
    install_ghostboard
    install_roamium
    install_wezboard
    echo ""
    echo "Done (all)."
    ;;
  *)
    echo "Unknown component: $COMPONENT"
    echo "Components: ghostboard, wezboard, roamium, webtui, all"
    exit 1
    ;;
esac
