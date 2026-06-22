#!/usr/bin/env bash
set -euo pipefail

# Build, package, upload, and publish a release to GitHub and Homebrew.
# Usage: scripts/release.sh [version]
# Default version: 0.1.0

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
source "$SCRIPT_DIR/roamium-resources.sh"
source "$SCRIPT_DIR/surfari-resources.sh"
VERSION="${1:-0.1.0}"
ARCH="aarch64-apple-darwin"
TARBALL_NAME="termsurf-${VERSION}-${ARCH}.tar.gz"
STAGING_DIR="$REPO_DIR/dist/release"
CHROMIUM_OUT="$REPO_DIR/chromium/src/out/Default"
WEBKIT_BUILD="$REPO_DIR/webkit/src/WebKitBuild/Debug"
GHOSTBOARD_APP="$REPO_DIR/ghostboard/macos/build/Release/TermSurf.app"
CASK_FILE="$REPO_DIR/homebrew/Casks/termsurf.rb"

echo "==> Packaging TermSurf v${VERSION} for ${ARCH}..."

# Check release builds exist
for f in \
  "$REPO_DIR/target/release/web" \
  "$GHOSTBOARD_APP/Contents/MacOS/termsurf" \
  "$REPO_DIR/target/release/roamium" \
  "$REPO_DIR/target/release/surfari" \
  "$REPO_DIR/surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib" \
  "$WEBKIT_BUILD/WebKit.framework"; do
  if [ ! -e "$f" ]; then
    echo "Error: Release build not found: $f"
    echo "Run: scripts/build.sh all --release"
    exit 1
  fi
done

# Clean and create staging directory
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR/roamium"
mkdir -p "$STAGING_DIR/surfari"

# Copy binaries
echo "==> Copying binaries..."
cp "$REPO_DIR/target/release/web" "$STAGING_DIR/"
cp "$REPO_DIR/target/release/roamium" "$STAGING_DIR/roamium/"
cp "$REPO_DIR/target/release/surfari" "$STAGING_DIR/surfari/"
cp "$REPO_DIR/surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib" "$STAGING_DIR/surfari/"
copy_surfari_runtime_resources "$WEBKIT_BUILD" "$STAGING_DIR/surfari"
rewrite_surfari_runtime_paths "$WEBKIT_BUILD" "$STAGING_DIR/surfari"
sign_surfari_runtime_artifacts "$STAGING_DIR/surfari"

# Copy Chromium dylibs and resources
copy_roamium_runtime_resources "$CHROMIUM_OUT" "$STAGING_DIR/roamium"

# Copy .app bundle
echo "==> Copying TermSurf.app..."
cp -R "$GHOSTBOARD_APP" "$STAGING_DIR/TermSurf.app"

# Create tarball
echo "==> Creating tarball..."
cd "$STAGING_DIR"
tar czf "$REPO_DIR/dist/$TARBALL_NAME" .

# Compute SHA256
SHA=$(shasum -a 256 "$REPO_DIR/dist/$TARBALL_NAME" | awk '{print $1}')
echo "==> SHA256: $SHA"

if [ "${TERMSURF_RELEASE_PACKAGE_ONLY:-0}" = "1" ]; then
  echo "==> Package-only mode: skipping GitHub upload and Homebrew cask update."
  echo "==> Tarball: dist/$TARBALL_NAME"
  exit 0
fi

# Upload to GitHub (delete old release if it exists)
echo "==> Uploading to GitHub..."
cd "$REPO_DIR"
gh release delete "v${VERSION}" --yes 2>/dev/null || true
gh release create "v${VERSION}" "dist/${TARBALL_NAME}" --title "v${VERSION}" --notes "v${VERSION}"

cd "$REPO_DIR/homebrew"
if [ "$(git rev-parse --abbrev-ref HEAD)" != "main" ]; then
  echo "==> Checking out Homebrew tap main branch..."
  git checkout main
fi

# Update Homebrew cask
echo "==> Updating Homebrew cask..."
sed -i '' "s/version \".*\"/version \"${VERSION}\"/" "$CASK_FILE"
sed -i '' "s/sha256 \".*\"/sha256 \"${SHA}\"/" "$CASK_FILE"

git add -A
git commit -m "v${VERSION}" || true
git push origin main

echo ""
echo "==> Released TermSurf v${VERSION}"
echo "==> Users: brew tap termsurf/termsurf && brew install --cask termsurf"
