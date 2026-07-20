#!/usr/bin/env bash
set -euo pipefail

COMPONENT="${1:-}"
APPLICATIONS_DIR="${TERMSURF_APPLICATIONS_DIR:-/Applications}"
CHROMIUMD_INSTALL_DIR="${ASTROHACKER_CHROMIUM_INSTALL_DIR:-/opt/homebrew/opt/astrohacker-terminal-ah-chromiumd}"

usage() {
  echo "Usage: $0 <component>"
  echo "Components: ahterm, ah-chromiumd, ahweb, all"
  echo "Aliases: aht→ahterm, webtui→ahweb"
}

if [ -z "$COMPONENT" ]; then
  usage
  exit 1
fi

# Normalize legacy aliases to product names.
case "$COMPONENT" in
  aht) COMPONENT=ahterm ;;
  webtui) COMPONENT=ahweb ;;
esac

case "$COMPONENT" in
  ahterm | ah-chromiumd | ahweb | all) ;;
  *)
    echo "Unknown component: $COMPONENT"
    usage
    exit 1
    ;;
esac

needs_root() {
  if [ "$COMPONENT" = "ah-chromiumd" ] && [ "$CHROMIUMD_INSTALL_DIR" != "/opt/homebrew/opt/astrohacker-terminal-ah-chromiumd" ]; then
    mkdir -p "$CHROMIUMD_INSTALL_DIR" || {
      echo "Error: ASTROHACKER_CHROMIUM_INSTALL_DIR is not writable: $CHROMIUMD_INSTALL_DIR"
      exit 1
    }
    [ -w "$CHROMIUMD_INSTALL_DIR" ] && return 1
    echo "Error: ASTROHACKER_CHROMIUM_INSTALL_DIR is not writable: $CHROMIUMD_INSTALL_DIR"
    exit 1
  fi
  if [ "$COMPONENT" = "ahterm" ] && [ "$APPLICATIONS_DIR" != "/Applications" ]; then
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
    ASTROHACKER_CHROMIUM_INSTALL_DIR="$CHROMIUMD_INSTALL_DIR" \
    "$0" "$@"
fi

LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"

uninstall_chromiumd() {
  echo "==> Uninstalling ah-chromiumd..."
  rm -rf "$CHROMIUMD_INSTALL_DIR"
  rm -rf /usr/local/chromium
  rm -f /usr/local/bin/chromium
  rm -rf /usr/local/lib/chromium
  rm -rf /opt/homebrew/opt/astrohacker-terminal-chromium

  echo "  Removed: $CHROMIUMD_INSTALL_DIR"
}

uninstall_ahterm() {
  local APP_DIR="$APPLICATIONS_DIR"
  local APP="$APP_DIR/Astrohacker Terminal.app"

  echo "==> Uninstalling Astrohacker Terminal..."
  rm -rf "$APP"

  echo "  Removed: $APP"
}

uninstall_ahweb() {
  echo "==> Uninstalling ahweb..."
  rm -f /usr/local/bin/ahweb
  rm -f /usr/local/bin/web

  echo "  Removed: /usr/local/bin/ahweb (and legacy /usr/local/bin/web if present)"
}


case "$COMPONENT" in
  ah-chromiumd) uninstall_chromiumd ;;
  ahterm)       uninstall_ahterm ;;
  ahweb)        uninstall_ahweb ;;
  all)
    uninstall_chromiumd
    uninstall_ahterm
    uninstall_ahweb
    echo ""
    echo "Done (all)."
    ;;
esac
