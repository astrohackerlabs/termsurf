#!/bin/bash
# Build the cef-osr example app
#
# Usage:
#   ./scripts/build-osr.sh [--clean] [--open] [--release]
#
# Flags:
#   --clean    Remove existing app bundle before building
#   --open     Run the app after building
#   --release  Build in release mode

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
APP_PATH="$REPO_DIR/cef-osr.app"

# Parse flags
CLEAN=false
OPEN=false
RELEASE=false
for arg in "$@"; do
    case $arg in
        --clean) CLEAN=true ;;
        --open) OPEN=true ;;
        --release) RELEASE=true ;;
    esac
done

cd "$REPO_DIR"

# Clean if requested or if app bundle exists (avoid nesting issue)
if [ "$CLEAN" = true ] || [ -d "$APP_PATH" ]; then
    echo "Removing existing app bundle..."
    rm -rf "$APP_PATH"
fi

# Build
if [ "$RELEASE" = true ]; then
    echo "Building cef-osr (release)..."
    cargo build -p cef-osr --release
    cargo run --bin bundle-cef-app --release -- cef-osr -o .
else
    echo "Building cef-osr (debug)..."
    cargo build -p cef-osr
    cargo run --bin bundle-cef-app -- cef-osr -o .
fi

# Verify
if [ ! -d "$APP_PATH/Contents" ]; then
    echo "ERROR: App bundle not created correctly at $APP_PATH"
    exit 1
fi

echo ""
echo "=== Build Complete ==="
echo "App: $APP_PATH"

# Open if requested
if [ "$OPEN" = true ]; then
    echo "Running cef-osr..."
    "$APP_PATH/Contents/MacOS/cef-osr"
fi
