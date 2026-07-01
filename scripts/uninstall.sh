#!/usr/bin/env bash
set -euo pipefail

COMPONENT="${1:-}"
APPLICATIONS_DIR="${TERMSURF_APPLICATIONS_DIR:-/Applications}"
ROAMIUM_INSTALL_DIR="${TERMSURF_ROAMIUM_INSTALL_DIR:-/opt/homebrew/opt/termsurf-roamium}"
GTUI_BIN_DIR="${TERMSURF_GTUI_BIN_DIR:-/usr/local/bin}"
GTUI_INSTALL_DIR="${TERMSURF_GTUI_INSTALL_DIR:-/usr/local/share/termsurf/gtui}"

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

LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"

uninstall_roamium() {
  echo "==> Uninstalling Roamium..."
  rm -rf "$ROAMIUM_INSTALL_DIR"
  rm -rf /usr/local/roamium
  rm -f /usr/local/bin/roamium
  rm -rf /usr/local/lib/roamium

  echo "  Removed: $ROAMIUM_INSTALL_DIR"
}

uninstall_ghostboard() {
  local APP_DIR="/Applications"
  if [ "$COMPONENT" = "ghostboard" ]; then
    APP_DIR="$APPLICATIONS_DIR"
  fi
  local APP="$APP_DIR/TermSurf.app"

  echo "==> Uninstalling Ghostboard..."
  rm -rf "$APP"

  echo "  Removed: $APP"
}

uninstall_webtui() {
  echo "==> Uninstalling webtui..."
  rm -f /usr/local/bin/web

  echo "  Removed: /usr/local/bin/web"
}

uninstall_gtui() {
  echo "==> Uninstalling TermSurf GTUI..."
  rm -f "$GTUI_BIN_DIR/termsurf"
  rm -rf "$GTUI_INSTALL_DIR"

  echo "  Removed: $GTUI_BIN_DIR/termsurf"
  echo "  Removed: $GTUI_INSTALL_DIR"
}

case "$COMPONENT" in
  roamium)    uninstall_roamium ;;
  ghostboard) uninstall_ghostboard ;;
  webtui)     uninstall_webtui ;;
  gtui)       uninstall_gtui ;;
  all)
    uninstall_roamium
    uninstall_ghostboard
    uninstall_webtui
    uninstall_gtui
    echo ""
    echo "Done (all)."
    ;;
esac
