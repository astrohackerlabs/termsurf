#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
CHROMIUM_SRC="$REPO_DIR/chromium/src"
CHROMIUM_OUT="$CHROMIUM_SRC/out/Default"
CHROMIUM_PROTOC="$CHROMIUM_OUT/protoc"
WEBKIT_SRC="$REPO_DIR/webkit/src"
SURFARI_LIB_DIR="$REPO_DIR/surfari/libtermsurf_webkit"

RELEASE=false
CLEAN=false
OPEN=false
COMPONENT=""

usage() {
  echo "Usage: $0 <component> [--release] [--clean] [--open]"
  echo "Components: ghostboard, roamium, webtui, chromium, webkit, surfari-lib, surfari, all"
}

configuration() {
  if $RELEASE; then
    echo "Release"
  else
    echo "Debug"
  fi
}

for arg in "$@"; do
  case "$arg" in
    --release) RELEASE=true ;;
    --clean)   CLEAN=true ;;
    --open)    OPEN=true ;;
    -*)
      echo "Unknown flag: $arg"
      usage
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
  usage
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

build_webkit() {
  local CONFIGURATION
  CONFIGURATION="$(configuration)"
  local CONFIG_FLAG="--debug"
  if $RELEASE; then
    CONFIG_FLAG="--release"
  fi

  if [ ! -d "$WEBKIT_SRC" ]; then
    echo "==> Skipping WebKit (webkit/src not found)"
    return
  fi

  cd "$REPO_DIR"
  if $CLEAN; then
    echo "==> Cleaning WebKit ($CONFIGURATION)..."
    "$WEBKIT_SRC/Tools/Scripts/build-webkit" "$CONFIG_FLAG" --clean
  fi

  echo "==> Building WebKit ($CONFIGURATION)..."
  "$WEBKIT_SRC/Tools/Scripts/build-webkit" "$CONFIG_FLAG"
  echo "  WebKit: $WEBKIT_SRC/WebKitBuild/$CONFIGURATION"
}

build_surfari_lib() {
  local CONFIGURATION
  CONFIGURATION="$(configuration)"

  echo "==> Building libtermsurf_webkit ($CONFIGURATION)..."
  cd "$REPO_DIR"
  local args=("--configuration" "$CONFIGURATION")
  if $CLEAN; then
    args+=("--clean")
  fi
  "$SURFARI_LIB_DIR/build.sh" "${args[@]}"
  echo "  libtermsurf_webkit: $SURFARI_LIB_DIR/build/libtermsurf_webkit.dylib"
}

build_surfari() {
  local CONFIGURATION
  CONFIGURATION="$(configuration)"

  build_surfari_lib

  cd "$REPO_DIR"
  if $CLEAN; then
    echo "==> Cleaning Surfari..."
    cargo clean -p surfari
  fi
  if $RELEASE; then
    echo "==> Building Surfari (release)..."
    cargo build --release -p surfari
    echo "  Surfari: $REPO_DIR/target/release/surfari"
  else
    echo "==> Building Surfari (debug)..."
    cargo build -p surfari
    echo "  Surfari: $REPO_DIR/target/debug/surfari"
  fi
}

build_ghostboard() {
  local CONFIGURATION="Debug"
  local ZIG_OPTIMIZE="Debug"
  if $RELEASE; then
    CONFIGURATION="Release"
    ZIG_OPTIMIZE="ReleaseFast"
  fi

  echo "==> Building GhostboardKit ($ZIG_OPTIMIZE)..."
  cd "$REPO_DIR/ghostboard"
  zig build -Demit-macos-app=false -Doptimize="$ZIG_OPTIMIZE"

  cd "$REPO_DIR/ghostboard/macos"
  if $CLEAN; then
    echo "==> Cleaning Ghostboard ($CONFIGURATION)..."
    ./build.nu --configuration "$CONFIGURATION" --action clean
  fi

  echo "==> Building Ghostboard ($CONFIGURATION)..."
  ./build.nu --configuration "$CONFIGURATION" --action build
  if $RELEASE; then
    codesign --force --deep --sign - "build/$CONFIGURATION/TermSurf.app"
  fi
  echo "  Ghostboard: $REPO_DIR/ghostboard/macos/build/$CONFIGURATION/TermSurf.app"
  echo "  Ghostboard executable: $REPO_DIR/ghostboard/macos/build/$CONFIGURATION/TermSurf.app/Contents/MacOS/termsurf"
}

case "$COMPONENT" in
  chromium)   build_chromium ;;
  webtui)     build_webtui ;;
  roamium)    build_roamium ;;
  webkit)     build_webkit ;;
  surfari-lib) build_surfari_lib ;;
  surfari)    build_surfari ;;
  ghostboard) build_ghostboard ;;
  all)
    build_chromium
    build_webtui
    build_roamium
    build_webkit
    build_surfari
    build_ghostboard
    echo ""
    echo "Done (all)."
    ;;
  *)
    echo "Unknown component: $COMPONENT"
    usage
    exit 1
    ;;
esac
