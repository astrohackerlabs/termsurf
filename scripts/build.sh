#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
COMPANY_DIR="$REPO_DIR"
RUST_DIR="$COMPANY_DIR"
CHROMIUM_SRC="$COMPANY_DIR/forks/chromium/src"
CHROMIUM_OUT="$CHROMIUM_SRC/out/Default"
CHROMIUM_PROTOC="$CHROMIUM_OUT/protoc"
GHOSTTY_DIR="$COMPANY_DIR/forks/ghostty"

RELEASE=false
CLEAN=false
OPEN=false
PRINT_PATHS=false
COMPONENT=""

usage() {
  echo "Usage: $0 <component> [--release] [--clean] [--open]"
  echo "Components: ahterm, ahsh, ahweb, ahcalc, chromium-fork, ah-chromiumd, all"
  echo "Aliases: aht→ahterm, webtui→ahweb, chromium→ah-chromiumd"
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
  # Product GN args (incl. proprietary_codecs + Chrome ffmpeg) — Issue 26072212448824.
  echo "==> Ensuring Chromium product args.gn..."
  ASTROHACKER_CHROMIUM_OUT="$CHROMIUM_OUT" "$SCRIPT_DIR/ensure-chromium-args.sh"
  cd "$CHROMIUM_SRC"
  if $CLEAN; then
    echo "==> Cleaning Chromium..."
    gn clean out/Default
  fi
  echo "==> gn gen out/Default (product args)..."
  gn gen out/Default
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
  ahterm|aht) build_ahterm ;;
  all)
    # Shipped desktop engines: Chromium only (WebKit product targets removed).
    build_chromium_fork
    build_ahweb
    build_ahsh
    build_ahcalc
    build_chromiumd
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
