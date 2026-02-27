#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="$(cd "$(dirname "$0")" && pwd)"

OPEN=false
CLEAN=false

for arg in "$@"; do
  case "$arg" in
    --open)  OPEN=true ;;
    --clean) CLEAN=true ;;
    *)
      echo "Usage: $0 [--clean] [--open]"
      exit 1
      ;;
  esac
done

# --- GUI (Zig + Xcode) ---

cd "$REPO_DIR/gui"

if $CLEAN; then
  echo "==> Cleaning GUI build..."
  rm -rf zig-out zig-cache macos/build/Debug
fi

echo "==> Building GUI (Debug)..."
zig build

APP="$REPO_DIR/gui/macos/build/Debug/TermSurf-Debug.app"

# --- Chromium ---

CHROMIUM_SRC="$REPO_DIR/chromium/src"

if [ -d "$CHROMIUM_SRC" ]; then
  export PATH="$REPO_DIR/chromium/depot_tools:$PATH"
  cd "$CHROMIUM_SRC"

  if $CLEAN; then
    echo "==> Cleaning Chromium build..."
    gn clean out/Default
  fi

  echo "==> Building Chromium..."
  autoninja -C out/Default chromium_profile_server
else
  echo "==> Skipping Chromium (chromium/src not found)"
fi

# --- Web TUI (Rust) ---

cd "$REPO_DIR/tui"

if $CLEAN; then
  echo "==> Cleaning TUI build..."
  cargo clean
fi

echo "==> Building TUI (debug)..."
cargo build

echo ""
echo "Done."
echo "  GUI: $APP"
echo "  TUI: $REPO_DIR/tui/target/debug/web"

if $OPEN; then
  echo "==> Opening $APP..."
  open "$APP"
fi
