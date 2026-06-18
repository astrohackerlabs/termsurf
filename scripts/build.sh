#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
CHROMIUM_SRC="$REPO_DIR/chromium/src"
CHROMIUM_OUT="$CHROMIUM_SRC/out/Default"
CHROMIUM_PROTOC="$CHROMIUM_OUT/protoc"

RELEASE=false
CLEAN=false
OPEN=false
COMPONENT=""

for arg in "$@"; do
  case "$arg" in
    --release) RELEASE=true ;;
    --clean)   CLEAN=true ;;
    --open)    OPEN=true ;;
    -*)
      echo "Unknown flag: $arg"
      echo "Usage: $0 <component> [--release] [--clean] [--open]"
      echo "Components: wezboard, ghostboard, roamium, webtui, chromium, all"
      exit 1
      ;;
    *)
      if [ -z "$COMPONENT" ]; then
        COMPONENT="$arg"
      else
        echo "Error: multiple components specified"
        exit 1
      fi
      ;;
  esac
done

if [ -z "$COMPONENT" ]; then
  echo "Usage: $0 <component> [--release] [--clean] [--open]"
  echo "Components: wezboard, ghostboard, roamium, webtui, chromium, all"
  exit 1
fi

# Export PROTOC from Chromium if available (needed by prost_build).
if [ -x "$CHROMIUM_PROTOC" ]; then
  export PROTOC="$CHROMIUM_PROTOC"
fi

build_chromium() {
  if [ ! -d "$CHROMIUM_SRC" ]; then
    echo "==> Skipping Chromium (chromium/src not found)"
    return
  fi
  export PATH="$REPO_DIR/chromium/depot_tools:$PATH"
  cd "$CHROMIUM_SRC"
  if $CLEAN; then
    echo "==> Cleaning Chromium..."
    gn clean out/Default
  fi
  echo "==> Building Chromium..."
  autoninja -C out/Default libtermsurf_chromium
  echo "  Chromium: $CHROMIUM_OUT"
}

build_webtui() {
  cd "$REPO_DIR"
  if $CLEAN; then
    echo "==> Cleaning webtui..."
    cargo clean -p webtui
  fi
  if $RELEASE; then
    echo "==> Building webtui (release)..."
    cargo build --release -p webtui
    echo "  webtui: $REPO_DIR/target/release/web"
  else
    echo "==> Building webtui (debug)..."
    cargo build -p webtui
    echo "  webtui: $REPO_DIR/target/debug/web"
  fi
}

build_roamium() {
  cd "$REPO_DIR"
  if $CLEAN; then
    echo "==> Cleaning Roamium..."
    cargo clean -p roamium
  fi
  if $RELEASE; then
    echo "==> Building Roamium (release)..."
    cargo build --release -p roamium
    cp "$REPO_DIR/target/release/roamium" "$CHROMIUM_OUT/roamium"
  else
    echo "==> Building Roamium (debug)..."
    cargo build -p roamium
    cp "$REPO_DIR/target/debug/roamium" "$CHROMIUM_OUT/roamium"
  fi
  echo "  Roamium: $CHROMIUM_OUT/roamium"
}

build_wezboard() {
  cd "$REPO_DIR/wezboard"
  if $CLEAN; then
    echo "==> Cleaning Wezboard..."
    cargo clean
  fi
  if $RELEASE; then
    echo "==> Building Wezboard (release)..."
    cargo build --release -p wezboard-gui -p wezboard
    echo "  Wezboard: $REPO_DIR/wezboard/target/release/wezboard-gui"
    echo "  Wezboard CLI: $REPO_DIR/wezboard/target/release/wezboard"
  else
    echo "==> Building Wezboard (debug)..."
    cargo build -p wezboard-gui -p wezboard
    echo "  Wezboard: $REPO_DIR/wezboard/target/debug/wezboard-gui"
    echo "  Wezboard CLI: $REPO_DIR/wezboard/target/debug/wezboard"
  fi
}

build_ghostboard() {
  local CONFIGURATION="Debug"
  if $RELEASE; then
    CONFIGURATION="Release"
  fi

  cd "$REPO_DIR/ghostboard/macos"
  if $CLEAN; then
    echo "==> Cleaning Ghostboard ($CONFIGURATION)..."
    ./build.nu --configuration "$CONFIGURATION" --action clean
  fi

  echo "==> Building Ghostboard ($CONFIGURATION)..."
  ./build.nu --configuration "$CONFIGURATION" --action build
  echo "  Ghostboard: $REPO_DIR/ghostboard/macos/build/$CONFIGURATION/TermSurf Ghostboard.app"
  echo "  Ghostboard executable: $REPO_DIR/ghostboard/macos/build/$CONFIGURATION/TermSurf Ghostboard.app/Contents/MacOS/ghostboard"
}

case "$COMPONENT" in
  chromium)   build_chromium ;;
  webtui)     build_webtui ;;
  roamium)    build_roamium ;;
  wezboard)   build_wezboard ;;
  ghostboard) build_ghostboard ;;
  all)
    build_chromium
    build_webtui
    build_roamium
    build_wezboard
    build_ghostboard
    echo ""
    echo "Done (all)."
    ;;
  *)
    echo "Unknown component: $COMPONENT"
    echo "Components: wezboard, ghostboard, roamium, webtui, chromium, all"
    exit 1
    ;;
esac
