#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"

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
CHROMIUM_OUT="$CHROMIUM_SRC/out/Default"

if [ -d "$CHROMIUM_SRC" ]; then
  export PATH="$REPO_DIR/chromium/depot_tools:$PATH"
  cd "$CHROMIUM_SRC"

  if $CLEAN; then
    echo "==> Cleaning Chromium build..."
    gn clean out/Default
  fi

  echo "==> Building Chromium..."
  autoninja -C out/Default libtermsurf_chromium
else
  echo "==> Skipping Chromium (chromium/src not found)"
fi

# --- Web TUI (Rust) ---

# prost_build needs protoc. Use Chromium's built copy if available,
# so users don't need a system-installed protoc.
CHROMIUM_PROTOC="$CHROMIUM_OUT/protoc"
if [ -x "$CHROMIUM_PROTOC" ]; then
  export PROTOC="$CHROMIUM_PROTOC"
fi

cd "$REPO_DIR/webtui"

if $CLEAN; then
  echo "==> Cleaning TUI build..."
  cargo clean
fi

echo "==> Building TUI (debug)..."
cargo build

# --- Roamium (Rust) ---

cd "$REPO_DIR/roamium"

if $CLEAN; then
  echo "==> Cleaning Roamium build..."
  cargo clean
fi

echo "==> Building Roamium (debug)..."
cargo build
cp "$REPO_DIR/roamium/target/debug/roamium" "$CHROMIUM_OUT/roamium"

echo ""
echo "Done."
echo "  GUI:     $APP"
echo "  TUI:     $REPO_DIR/webtui/target/debug/web"
echo "  Roamium: $CHROMIUM_OUT/roamium"

if $OPEN; then
  echo "==> Opening $APP..."
  open "$APP"
fi
