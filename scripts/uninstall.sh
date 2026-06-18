#!/usr/bin/env bash
set -euo pipefail

COMPONENT="${1:-}"

if [ -z "$COMPONENT" ]; then
  echo "Usage: $0 <component>"
  echo "Components: wezboard, ghostboard, roamium, webtui, all"
  exit 1
fi

# Re-exec as root so we only prompt for the password once.
if [ "$(id -u)" -ne 0 ]; then
  exec sudo "$0" "$@"
fi

LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"

uninstall_roamium() {
  echo "==> Uninstalling Roamium..."
  rm -rf /usr/local/roamium
  rm -f /usr/local/bin/roamium
  rm -rf /usr/local/lib/roamium

  echo "  Removed: /usr/local/roamium"
}

uninstall_wezboard() {
  local APP="/Applications/TermSurf Wezboard.app"

  echo "==> Uninstalling Wezboard..."
  rm -rf "$APP"

  echo "  Removed: $APP"
}

uninstall_ghostboard() {
  local APP="/Applications/TermSurf Ghostboard.app"

  echo "==> Uninstalling Ghostboard..."
  rm -rf "$APP"

  echo "  Removed: $APP"
}

uninstall_webtui() {
  echo "==> Uninstalling webtui..."
  rm -f /usr/local/bin/web

  echo "  Removed: /usr/local/bin/web"
}

case "$COMPONENT" in
  roamium)    uninstall_roamium ;;
  wezboard)   uninstall_wezboard ;;
  ghostboard) uninstall_ghostboard ;;
  webtui)     uninstall_webtui ;;
  all)
    uninstall_roamium
    uninstall_wezboard
    uninstall_ghostboard
    uninstall_webtui
    echo ""
    echo "Done (all)."
    ;;
  *)
    echo "Unknown component: $COMPONENT"
    echo "Components: wezboard, ghostboard, roamium, webtui, all"
    exit 1
    ;;
esac
