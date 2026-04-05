#!/usr/bin/env bash
set -euo pipefail

# Build, package, upload, and publish a release to GitHub and Homebrew.
# Usage: scripts/release.sh [version]
# Default version: 0.1.0

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
VERSION="${1:-0.1.0}"
ARCH="aarch64-apple-darwin"
TARBALL_NAME="termsurf-${VERSION}-${ARCH}.tar.gz"
STAGING_DIR="$REPO_DIR/dist/release"
CHROMIUM_OUT="$REPO_DIR/chromium/src/out/Default"
APP_TEMPLATE="$REPO_DIR/wezboard/assets/macos/TermSurf Wezboard.app"
CASK_FILE="$REPO_DIR/homebrew/Casks/termsurf.rb"

echo "==> Packaging TermSurf v${VERSION} for ${ARCH}..."

# Check release builds exist
for f in \
  "$REPO_DIR/webtui/target/release/web" \
  "$REPO_DIR/wezboard/target/release/wezboard" \
  "$REPO_DIR/wezboard/target/release/wezboard-gui" \
  "$REPO_DIR/roamium/target/release/roamium"; do
  if [ ! -f "$f" ]; then
    echo "Error: Release build not found: $f"
    echo "Run: scripts/build.sh all --release"
    exit 1
  fi
done

# Clean and create staging directory
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR/roamium"

# Copy binaries
echo "==> Copying binaries..."
cp "$REPO_DIR/webtui/target/release/web" "$STAGING_DIR/"
cp "$REPO_DIR/wezboard/target/release/wezboard" "$STAGING_DIR/"
cp "$REPO_DIR/roamium/target/release/roamium" "$STAGING_DIR/roamium/"

# Copy Chromium dylibs and resources
echo "==> Copying Chromium dylibs and resources..."
cp "$CHROMIUM_OUT"/*.dylib "$STAGING_DIR/roamium/"
cp "$CHROMIUM_OUT"/*.pak "$STAGING_DIR/roamium/"
cp "$CHROMIUM_OUT/icudtl.dat" "$STAGING_DIR/roamium/"
cp "$CHROMIUM_OUT"/v8_context_snapshot*.bin "$STAGING_DIR/roamium/"

# Copy .app bundle
echo "==> Copying Wezboard.app..."
cp -R "$APP_TEMPLATE" "$STAGING_DIR/TermSurf Wezboard.app"
mkdir -p "$STAGING_DIR/TermSurf Wezboard.app/Contents/MacOS"
mkdir -p "$STAGING_DIR/TermSurf Wezboard.app/Contents/Frameworks"
cp "$REPO_DIR/wezboard/target/release/wezboard-gui" "$STAGING_DIR/TermSurf Wezboard.app/Contents/MacOS/wezboard-gui"

# Move dylibs from bundle root to Contents/Frameworks (fixes codesign)
for dylib in "$STAGING_DIR/TermSurf Wezboard.app"/*.dylib; do
  if [ -f "$dylib" ]; then
    mv "$dylib" "$STAGING_DIR/TermSurf Wezboard.app/Contents/Frameworks/"
  fi
done

# Create tarball
echo "==> Creating tarball..."
cd "$STAGING_DIR"
tar czf "$REPO_DIR/dist/$TARBALL_NAME" .

# Compute SHA256
SHA=$(shasum -a 256 "$REPO_DIR/dist/$TARBALL_NAME" | awk '{print $1}')
echo "==> SHA256: $SHA"

# Upload to GitHub (delete old release if it exists)
echo "==> Uploading to GitHub..."
cd "$REPO_DIR"
gh release delete "v${VERSION}" --yes 2>/dev/null || true
gh release create "v${VERSION}" "dist/${TARBALL_NAME}" --title "v${VERSION}" --notes "v${VERSION}"

# Update Homebrew cask
echo "==> Updating Homebrew cask..."
sed -i '' "s/version \".*\"/version \"${VERSION}\"/" "$CASK_FILE"
sed -i '' "s/sha256 \".*\"/sha256 \"${SHA}\"/" "$CASK_FILE"

cd "$REPO_DIR/homebrew"
git add -A
git commit -m "v${VERSION}" || true
git push origin main

echo ""
echo "==> Released TermSurf v${VERSION}"
echo "==> Users: brew tap termsurf/termsurf && brew install --cask termsurf"
