#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
COMPANY_DIR="$REPO_DIR"
RUST_DIR="$COMPANY_DIR"
CHROMIUM_SRC="$COMPANY_DIR/forks/chromium/src"
CHROMIUM_OUT="$CHROMIUM_SRC/out/Default"
CHROMIUM_PROTOC="$CHROMIUM_OUT/protoc"
WEBKIT_SRC="$COMPANY_DIR/forks/webkit/src"
WEBKIT_LIB_DIR="$RUST_DIR/rust/ah-webkitd/libtermsurf_webkit"
GHOSTTY_DIR="$COMPANY_DIR/forks/ghostty"

RELEASE=false
CLEAN=false
OPEN=false
PRINT_PATHS=false
COMPONENT=""

usage() {
  echo "Usage: $0 <component> [--release] [--clean] [--open]"
  echo "Components: ahterm, ahsh, ahweb, ahcalc, chromium-fork, ah-chromiumd, webkit-fork, webkit-lib, ah-webkitd, all"
  echo "Aliases: aht→ahterm, webtui→ahweb, chromium→ah-chromiumd, webkit→ah-webkitd"
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
    --print-paths) PRINT_PATHS=true ;;
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

if $PRINT_PATHS; then
  printf 'SCRIPT_DIR=%s\n' "$SCRIPT_DIR"
  printf 'REPO_DIR=%s\n' "$REPO_DIR"
  printf 'COMPANY_DIR=%s\n' "$COMPANY_DIR"
  printf 'RUST_DIR=%s\n' "$RUST_DIR"
  printf 'CHROMIUM_SRC=%s\n' "$CHROMIUM_SRC"
  printf 'WEBKIT_SRC=%s\n' "$WEBKIT_SRC"
  printf 'GHOSTTY_DIR=%s\n' "$GHOSTTY_DIR"
  exit 0
fi

if [ -z "$COMPONENT" ]; then
  usage
  exit 1
fi

# Export PROTOC from Chromium if available (needed by prost_build).
if [ -x "$CHROMIUM_PROTOC" ]; then
  export PROTOC="$CHROMIUM_PROTOC"
fi

build_chromium_fork() {
  if [ ! -d "$CHROMIUM_SRC" ]; then
    echo "==> Skipping Chromium ($CHROMIUM_SRC not found)"
    return
  fi
  export PATH="$COMPANY_DIR/forks/chromium/depot_tools:$PATH"
  cd "$CHROMIUM_SRC"
  if $CLEAN; then
    echo "==> Cleaning Chromium..."
    gn clean out/Default
  fi
  echo "==> Building Chromium..."
  autoninja -C out/Default libtermsurf_chromium
  echo "  Chromium: $CHROMIUM_OUT"
}

build_ahweb() {
  cd "$RUST_DIR"
  if $CLEAN; then
    echo "==> Cleaning ahweb..."
    cargo clean -p ahweb
  fi
  if $RELEASE; then
    echo "==> Building ahweb (release)..."
    cargo build --release -p ahweb
    echo "  ahweb: $RUST_DIR/target/release/ahweb"
  else
    echo "==> Building ahweb (debug)..."
    cargo build -p ahweb
    echo "  ahweb: $RUST_DIR/target/debug/ahweb"
  fi
}


build_ahsh() {
  local AHSH_DIR="$RUST_DIR/rust/ahsh"
  if [ ! -d "$COMPANY_DIR/forks/nushell" ]; then
    echo "Missing Nushell fork checkout: $COMPANY_DIR/forks/nushell" >&2
    echo "Reconstruct it from patches/nushell before building ahsh." >&2
    exit 1
  fi
  if [ ! -d "$COMPANY_DIR/forks/reedline" ]; then
    echo "Missing Reedline fork checkout: $COMPANY_DIR/forks/reedline" >&2
    echo "Reconstruct it from patches/reedline before building ahsh." >&2
    exit 1
  fi
  cd "$AHSH_DIR"
  if $CLEAN; then
    echo "==> Cleaning ahsh..."
    cargo clean
  fi
  if $RELEASE; then
    echo "==> Building ahsh (release)..."
    cargo build --release
    echo "  ahsh: $AHSH_DIR/target/release/ahsh"
  else
    echo "==> Building ahsh (debug)..."
    cargo build
    echo "  ahsh: $AHSH_DIR/target/debug/ahsh"
  fi
}


build_chromiumd() {
  cd "$RUST_DIR"
  if [ ! -d "$CHROMIUM_OUT" ]; then
    echo "Missing Chromium output directory: $CHROMIUM_OUT" >&2
    echo "Build Chromium first with: $0 chromium-fork" >&2
    exit 1
  fi
  if $CLEAN; then
    echo "==> Cleaning Chromium..."
    cargo clean -p ah-chromiumd
  fi
  if $RELEASE; then
    echo "==> Building Chromium (release)..."
    cargo build --release -p ah-chromiumd
    cp "$RUST_DIR/target/release/ah-chromiumd" "$CHROMIUM_OUT/ah-chromiumd"
  else
    echo "==> Building Chromium (debug)..."
    cargo build -p ah-chromiumd
    cp "$RUST_DIR/target/debug/ah-chromiumd" "$CHROMIUM_OUT/ah-chromiumd"
  fi
  echo "  Chromium: $CHROMIUM_OUT/ah-chromiumd"
}

build_webkit_fork() {
  local CONFIGURATION
  CONFIGURATION="$(configuration)"
  local CONFIG_FLAG="--debug"
  if $RELEASE; then
    CONFIG_FLAG="--release"
  fi
  local WEBKIT_ARCH="${TERMSURF_WEBKIT_ARCH:-arm64}"
  local WEBKIT_SCOPE_FLAG="--only=WebKit"
  if [ "${TERMSURF_WEBKIT_FULL_BUILD:-0}" = "1" ]; then
    WEBKIT_SCOPE_FLAG=""
  fi
  local WEBKIT_BUILD_SETTINGS=(
    "--architecture=$WEBKIT_ARCH"
    "OVERRIDE_ENABLE_MODULE_VERIFIER=NO"
    "ENABLE_WK_LIBRARY_MODULE_VERIFIER=NO"
  )
  if $RELEASE; then
    # Xcode 26.5 can build bmalloc's Swift C++ interop module through both
    # staged and source header paths, which trips duplicate definitions.
    WEBKIT_BUILD_SETTINGS+=("WK_SWIFT_EXPLICIT_MODULES_ALLOW_CXX_INTEROP=NO")
  fi

  if [ ! -d "$WEBKIT_SRC" ]; then
    echo "==> Skipping WebKit ($WEBKIT_SRC not found)"
    return
  fi

  cd "$COMPANY_DIR"
  if $CLEAN; then
    echo "==> Cleaning WebKit ($CONFIGURATION)..."
    "$WEBKIT_SRC/Tools/Scripts/build-webkit" "$CONFIG_FLAG" --clean "${WEBKIT_BUILD_SETTINGS[@]}"
  fi

  echo "==> Building WebKit ($CONFIGURATION, $WEBKIT_ARCH)..."
  if $RELEASE && [ -n "$WEBKIT_SCOPE_FLAG" ]; then
    local WEBKIT_RELEASE_TARGETS=(
      "Everything up to WebKit"
      "WebInspectorUI"
    )
    for target in "${WEBKIT_RELEASE_TARGETS[@]}"; do
      echo "==> Building WebKit prerequisite ($target, $CONFIGURATION, $WEBKIT_ARCH)..."
      "$WEBKIT_SRC/Tools/Scripts/build-webkit" "$CONFIG_FLAG" "--only=$target" "${WEBKIT_BUILD_SETTINGS[@]}"
    done
  elif [ -n "$WEBKIT_SCOPE_FLAG" ]; then
    "$WEBKIT_SRC/Tools/Scripts/build-webkit" "$CONFIG_FLAG" "$WEBKIT_SCOPE_FLAG" "${WEBKIT_BUILD_SETTINGS[@]}"
  else
    "$WEBKIT_SRC/Tools/Scripts/build-webkit" "$CONFIG_FLAG" "${WEBKIT_BUILD_SETTINGS[@]}"
  fi
  echo "  WebKit: $WEBKIT_SRC/WebKitBuild/$CONFIGURATION"
}

build_webkit_lib() {
  local CONFIGURATION
  CONFIGURATION="$(configuration)"

  echo "==> Building libtermsurf_webkit ($CONFIGURATION)..."
  cd "$COMPANY_DIR"
  local args=("--configuration" "$CONFIGURATION")
  if $CLEAN; then
    args+=("--clean")
  fi
  "$WEBKIT_LIB_DIR/build.sh" "${args[@]}"
  echo "  libtermsurf_webkit: $WEBKIT_LIB_DIR/build/libtermsurf_webkit.dylib"
}

build_webkitd() {
  local CONFIGURATION
  CONFIGURATION="$(configuration)"

  build_webkit_lib

  cd "$RUST_DIR"
  if $CLEAN; then
    echo "==> Cleaning WebKit..."
    cargo clean -p ah-webkitd
  fi
  if $RELEASE; then
    echo "==> Building WebKit (release)..."
    cargo build --release -p ah-webkitd
    echo "  WebKit: $RUST_DIR/target/release/ah-webkitd"
  else
    echo "==> Building WebKit (debug)..."
    cargo build -p ah-webkitd
    echo "  WebKit: $RUST_DIR/target/debug/ah-webkitd"
  fi
}



build_ahterm() {
  local CONFIGURATION="Debug"
  local ZIG_OPTIMIZE="Debug"
  if $RELEASE; then
    CONFIGURATION="Release"
    ZIG_OPTIMIZE="ReleaseFast"
  fi

  echo "==> Building GhosttyKit / libghostty ($ZIG_OPTIMIZE)..."
  cd "$GHOSTTY_DIR"
  if [ -n "${TERMSURF_VERSION:-}" ]; then
    zig build -Demit-macos-app=false -Doptimize="$ZIG_OPTIMIZE" "-Dversion-string=$TERMSURF_VERSION"
  else
    zig build -Demit-macos-app=false -Doptimize="$ZIG_OPTIMIZE"
  fi

  cd "$GHOSTTY_DIR/macos"
  if $CLEAN; then
    echo "==> Cleaning ahterm ($CONFIGURATION)..."
    ./build.nu --configuration "$CONFIGURATION" --action clean
  fi

  echo "==> Building ahterm ($CONFIGURATION)..."
  if [ -n "${TERMSURF_VERSION:-}" ]; then
    ./build.nu --configuration "$CONFIGURATION" --action build --version "$TERMSURF_VERSION"
  else
    ./build.nu --configuration "$CONFIGURATION" --action build
  fi
  if $RELEASE; then
    codesign --force --deep --sign - "build/$CONFIGURATION/Astrohacker TermSurf.app"
  fi
  echo "  ahterm: $GHOSTTY_DIR/macos/build/$CONFIGURATION/Astrohacker TermSurf.app"
  echo "  ahterm executable: $GHOSTTY_DIR/macos/build/$CONFIGURATION/Astrohacker TermSurf.app/Contents/MacOS/ahterm"
}

build_ahcalc() {
  local AHCALC_DIR="$REPO_DIR/bun/ahcalc"
  if [ ! -d "$AHCALC_DIR" ]; then
    echo "Error: ahcalc package missing: $AHCALC_DIR" >&2
    exit 1
  fi
  if ! command -v bun >/dev/null 2>&1; then
    echo "Error: bun is required to build ahcalc (not found on PATH)" >&2
    exit 1
  fi
  if $CLEAN; then
    echo "==> Cleaning ahcalc dist..."
    rm -rf "$AHCALC_DIR/dist"
  fi
  # Prefer ASTROHACKER_VERSION (release); fall back to TERMSURF_VERSION if set.
  if [ -z "${ASTROHACKER_VERSION:-}" ] && [ -n "${TERMSURF_VERSION:-}" ]; then
    export ASTROHACKER_VERSION="$TERMSURF_VERSION"
  fi
  if $RELEASE; then
    echo "==> Building ahcalc (release${ASTROHACKER_VERSION:+, version $ASTROHACKER_VERSION})..."
  else
    echo "==> Building ahcalc (debug${ASTROHACKER_VERSION:+, version $ASTROHACKER_VERSION})..."
  fi
  (
    cd "$AHCALC_DIR"
    # Ensure package deps when node_modules missing (scoped, not monorepo-wide).
    if [ ! -d "$AHCALC_DIR/node_modules" ] && [ ! -d "$REPO_DIR/node_modules" ]; then
      bun install
    fi
    bun run build:ahcalc
  )
  echo "  ahcalc: $AHCALC_DIR/dist/ahcalc"
}

case "$COMPONENT" in
  chromium-fork) build_chromium_fork ;;
  ahweb|webtui) build_ahweb ;;
  ahsh)       build_ahsh ;;
  ahcalc)     build_ahcalc ;;
  ah-chromiumd|chromium)   build_chromiumd ;;
  webkit-fork) build_webkit_fork ;;
  webkit-lib) build_webkit_lib ;;
  ah-webkitd|webkit)     build_webkitd ;;
  ahterm|aht) build_ahterm ;;
  all)
    build_chromium_fork
    build_ahweb
    build_ahsh
    build_ahcalc
    build_chromiumd
    build_webkit_fork
    build_webkitd
    build_ahterm
    echo ""
    echo "Done (all)."
    ;;
  *)
    echo "Unknown component: $COMPONENT"
    usage
    exit 1
    ;;
esac
