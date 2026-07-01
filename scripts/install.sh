#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
CHROMIUM_OUT="$REPO_DIR/chromium/src/out/Default"
source "$SCRIPT_DIR/roamium-resources.sh"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"
GHOSTBOARD_RELEASE_APP="$REPO_DIR/ghostboard/macos/build/Release/TermSurf.app"
APPLICATIONS_DIR="${TERMSURF_APPLICATIONS_DIR:-/Applications}"
ROAMIUM_INSTALL_DIR="${TERMSURF_ROAMIUM_INSTALL_DIR:-/opt/homebrew/opt/termsurf-roamium}"
GTUI_BIN_DIR="${TERMSURF_GTUI_BIN_DIR:-/usr/local/bin}"
GTUI_INSTALL_DIR="${TERMSURF_GTUI_INSTALL_DIR:-/usr/local/share/termsurf/gtui}"

COMPONENT="${1:-}"

if [ -z "$COMPONENT" ]; then
  echo "Usage: $0 <component>"
  echo "Components: ghostboard, roamium, webtui, gtui, all"
  exit 1
fi

case "$COMPONENT" in
  roamium | ghostboard | webtui | gtui | all) ;;
  *)
    echo "Unknown component: $COMPONENT"
    echo "Components: ghostboard, roamium, webtui, gtui, all"
    exit 1
    ;;
esac

if [ "$COMPONENT" = "ghostboard" ] && [ ! -x "$GHOSTBOARD_RELEASE_APP/Contents/MacOS/ghostboard" ]; then
  echo "Error: Release app not found at $GHOSTBOARD_RELEASE_APP"
  echo "Run: scripts/build.sh ghostboard --release"
  exit 1
fi

needs_root() {
  if [ "$COMPONENT" = "roamium" ] && [ "$ROAMIUM_INSTALL_DIR" != "/opt/homebrew/opt/termsurf-roamium" ]; then
    mkdir -p "$ROAMIUM_INSTALL_DIR" || {
      echo "Error: TERMSURF_ROAMIUM_INSTALL_DIR is not writable: $ROAMIUM_INSTALL_DIR"
      exit 1
    }
    [ -w "$ROAMIUM_INSTALL_DIR" ] && return 1
    echo "Error: TERMSURF_ROAMIUM_INSTALL_DIR is not writable: $ROAMIUM_INSTALL_DIR"
    exit 1
  fi
  if [ "$COMPONENT" = "ghostboard" ] && [ "$APPLICATIONS_DIR" != "/Applications" ]; then
    mkdir -p "$APPLICATIONS_DIR" || {
      echo "Error: TERMSURF_APPLICATIONS_DIR is not writable: $APPLICATIONS_DIR"
      exit 1
    }
    [ -w "$APPLICATIONS_DIR" ] && return 1
    echo "Error: TERMSURF_APPLICATIONS_DIR is not writable: $APPLICATIONS_DIR"
    exit 1
  fi
  if [ "$COMPONENT" = "gtui" ]; then
    mkdir -p "$GTUI_BIN_DIR" "$GTUI_INSTALL_DIR" 2>/dev/null || return 0
    [ -w "$GTUI_BIN_DIR" ] && [ -w "$GTUI_INSTALL_DIR" ] && return 1
    return 0
  fi
  return 0
}

# Re-exec as root so we only prompt for the password once.
if [ "$(id -u)" -ne 0 ] && needs_root; then
  exec sudo env \
    TERMSURF_APPLICATIONS_DIR="$APPLICATIONS_DIR" \
    TERMSURF_ROAMIUM_INSTALL_DIR="$ROAMIUM_INSTALL_DIR" \
    TERMSURF_GTUI_BIN_DIR="$GTUI_BIN_DIR" \
    TERMSURF_GTUI_INSTALL_DIR="$GTUI_INSTALL_DIR" \
    "$0" "$@"
fi

install_roamium() {
  local ROAMIUM_SRC="$REPO_DIR/target/release/roamium"
  local INSTALL_DIR="$ROAMIUM_INSTALL_DIR"

  if [ ! -f "$ROAMIUM_SRC" ]; then
    echo "Error: Release build not found at $ROAMIUM_SRC"
    echo "Run: scripts/build.sh roamium --release"
    exit 1
  fi

  echo "==> Installing Roamium to $INSTALL_DIR..."
  mkdir -p "$INSTALL_DIR"
  cp "$ROAMIUM_SRC" "$INSTALL_DIR/roamium"

  copy_roamium_runtime_resources "$CHROMIUM_OUT" "$INSTALL_DIR"

  echo "==> Codesigning Roamium..."
  codesign --force --sign - "$INSTALL_DIR/roamium" || true

  # Clean up old install locations.
  rm -rf /usr/local/roamium
  rm -f /usr/local/bin/roamium
  rm -rf /usr/local/lib/roamium

  echo "  Dir: $INSTALL_DIR"
  echo "  Bin: $INSTALL_DIR/roamium"
}

install_ghostboard() {
  local APP_SRC="$GHOSTBOARD_RELEASE_APP"
  local APP_DIR="/Applications"
  if [ "$COMPONENT" = "ghostboard" ]; then
    APP_DIR="$APPLICATIONS_DIR"
  fi
  local APP="$APP_DIR/TermSurf.app"

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

install_gtui() {
  local TERMSURF_CLI="$REPO_DIR/target/release/termsurf"

  if [ ! -f "$TERMSURF_CLI" ]; then
    echo "Error: Release build not found at $TERMSURF_CLI"
    echo "Run: scripts/build.sh gtui --release"
    exit 1
  fi

  echo "==> Installing TermSurf GTUI to $GTUI_BIN_DIR/termsurf..."
  mkdir -p "$GTUI_BIN_DIR" "$GTUI_INSTALL_DIR"
  cp "$TERMSURF_CLI" "$GTUI_BIN_DIR/termsurf"
  rm -rf "$GTUI_INSTALL_DIR/app"
  cp -R "$REPO_DIR/gtui/app" "$GTUI_INSTALL_DIR/app"
  codesign --force --sign - "$GTUI_BIN_DIR/termsurf" || true

  echo "  Bin: $GTUI_BIN_DIR/termsurf"
  echo "  App: $GTUI_INSTALL_DIR/app"
}

case "$COMPONENT" in
  roamium)    install_roamium ;;
  ghostboard) install_ghostboard ;;
  webtui)     install_webtui ;;
  gtui)       install_gtui ;;
  all)
    install_roamium
    install_ghostboard
    install_webtui
    install_gtui
    echo ""
    echo "Done (all)."
    ;;
esac
