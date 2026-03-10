#!/bin/bash
# Generate app icon assets from a source image
# Usage: ./scripts/generate-icons.sh [source-image]
#
# If no source image is provided, defaults to assets/termsurf-2-black-2.png.
# Generates all icon sizes for AppIcon.appiconset and AppIconImage.imageset.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
GUI_DIR="$REPO_ROOT/ghostboard"

PROD_SOURCE="${1:-$REPO_ROOT/assets/ghostboard-1.png}"
APPICONSET="$GUI_DIR/macos/Assets.xcassets/AppIcon.appiconset"
APPICONIMAGE="$GUI_DIR/macos/Assets.xcassets/AppIconImage.imageset"

# Check source file exists
if [ ! -f "$PROD_SOURCE" ]; then
    echo "Error: Source icon not found: $PROD_SOURCE"
    exit 1
fi

# Generate AppIcon.appiconset sizes
echo "Generating AppIcon.appiconset sizes..."
mkdir -p "$APPICONSET"

for size in 16 32 64 128 256 512 1024; do
    echo "  Creating icon-${size}.png"
    sips -z $size $size "$PROD_SOURCE" --out "$APPICONSET/icon-${size}.png" 2>/dev/null
done

# Update AppIconImage.imageset (runtime icon-switching)
echo "Updating AppIconImage.imageset..."
cp "$APPICONSET/icon-256.png" "$APPICONIMAGE/macOS-AppIcon-256px-128pt@2x.png"
cp "$APPICONSET/icon-512.png" "$APPICONIMAGE/macOS-AppIcon-512px.png"
cp "$APPICONSET/icon-1024.png" "$APPICONIMAGE/macOS-AppIcon-1024px.png"

echo ""
echo "Done! Rebuild the app to see the new icons."
echo ""
echo "Note: For best quality, source icons should be at least 1024x1024 pixels."
