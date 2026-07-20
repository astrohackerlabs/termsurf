#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="$SCRIPT_DIR/build"
CONFIGURATION="Debug"
CLEAN=false
BACKEND="${TERMSURF_LADYBIRD_BACKEND:-stub}"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --configuration)
      CONFIGURATION="${2:?missing configuration}"
      shift 2
      ;;
    --clean)
      CLEAN=true
      shift
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if $CLEAN; then
  rm -rf "$BUILD_DIR"
fi

mkdir -p "$BUILD_DIR"

if [ "$BACKEND" = "real" ]; then
  LADYBIRD_DIR="$REPO_ROOT/forks/ladybird"
  if [ ! -d "$LADYBIRD_DIR/.git" ]; then
    echo "TERMSURF_LADYBIRD_BACKEND=real requires forks/ladybird" >&2
    exit 1
  fi

  echo "Building real Ladybird-backed libtermsurf_ladybird.dylib"
  (
    cd "$LADYBIRD_DIR"
    ./Meta/ladybird.py build --preset "$CONFIGURATION" --gui AppKit TermSurfLadybird
  )

  case "$CONFIGURATION" in
    Debug)
      LADYBIRD_BUILD_DIR="$LADYBIRD_DIR/Build/debug"
      ;;
    Release)
      LADYBIRD_BUILD_DIR="$LADYBIRD_DIR/Build/release"
      ;;
    *)
      LADYBIRD_BUILD_DIR="$LADYBIRD_DIR/Build/$(printf '%s' "$CONFIGURATION" | tr '[:upper:]' '[:lower:]')"
      ;;
  esac

  SOURCE_DYLIB="$LADYBIRD_BUILD_DIR/lib/libtermsurf_ladybird.dylib"
  if [ ! -f "$SOURCE_DYLIB" ]; then
    echo "Expected Ladybird dylib not found: $SOURCE_DYLIB" >&2
    exit 1
  fi

  cp "$SOURCE_DYLIB" "$BUILD_DIR/libtermsurf_ladybird.dylib"
  install_name_tool -id "@rpath/libtermsurf_ladybird.dylib" "$BUILD_DIR/libtermsurf_ladybird.dylib" || true
  install_name_tool -add_rpath "$LADYBIRD_BUILD_DIR/lib" "$BUILD_DIR/libtermsurf_ladybird.dylib" 2>/dev/null || true
  install_name_tool -add_rpath "$LADYBIRD_BUILD_DIR/bin" "$BUILD_DIR/libtermsurf_ladybird.dylib" 2>/dev/null || true
  echo "Staged real Ladybird ABI at $BUILD_DIR/libtermsurf_ladybird.dylib"
  exit 0
fi

if [ "$BACKEND" != "stub" ]; then
  echo "Unknown TERMSURF_LADYBIRD_BACKEND=$BACKEND; expected stub or real" >&2
  exit 1
fi

echo "Building stub libtermsurf_ladybird.dylib"
CFLAGS=("-dynamiclib" "-I$SCRIPT_DIR/include")
if [ "$CONFIGURATION" = "Release" ]; then
  CFLAGS+=("-O2")
else
  CFLAGS+=("-O0" "-g")
fi

clang "${CFLAGS[@]}" \
  "$SCRIPT_DIR/src/termsurf_ladybird.c" \
  -install_name "@rpath/libtermsurf_ladybird.dylib" \
  -o "$BUILD_DIR/libtermsurf_ladybird.dylib"
