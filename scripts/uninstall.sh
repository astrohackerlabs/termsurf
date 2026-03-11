#!/usr/bin/env bash
set -euo pipefail

COMPONENT="${1:-}"

if [ -z "$COMPONENT" ]; then
  echo "Usage: $0 <component>"
  echo "Components: ghostboard, wezboard, roamium, webtui, all"
  exit 1
fi

LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"

uninstall_ghostboard() {
  local APP="/Applications/TermSurf Ghostboard.app"

  echo "==> Uninstalling Ghostboard..."
  rm -rf "$APP"
  rm -f /usr/local/bin/termsurf
  rm -f /usr/local/bin/web
  "$LSREGISTER" -u "$APP" 2>/dev/null || true

  echo "  Removed: $APP"
  echo "  Removed: /usr/local/bin/termsurf"
  echo "  Removed: /usr/local/bin/web"
}

uninstall_roamium() {
  echo "==> Uninstalling Roamium..."
  sudo rm -rf /usr/local/roamium
  sudo rm -f /usr/local/bin/roamium
  sudo rm -rf /usr/local/lib/roamium

  echo "  Removed: /usr/local/roamium"
}

uninstall_wezboard() {
  local APP="/Applications/TermSurf Wezboard.app"

  echo "==> Uninstalling Wezboard..."
  sudo rm -rf "$APP"

  echo "  Removed: $APP"
}

uninstall_webtui() {
  echo "==> Uninstalling webtui..."
  rm -f /usr/local/bin/web

  echo "  Removed: /usr/local/bin/web"
}

case "$COMPONENT" in
  ghostboard) uninstall_ghostboard ;;
  roamium)    uninstall_roamium ;;
  wezboard)   uninstall_wezboard ;;
  webtui)     uninstall_webtui ;;
  all)
    uninstall_ghostboard
    uninstall_roamium
    uninstall_wezboard
    echo ""
    echo "Done (all)."
    ;;
  *)
    echo "Unknown component: $COMPONENT"
    echo "Components: ghostboard, wezboard, roamium, webtui, all"
    exit 1
    ;;
esac
