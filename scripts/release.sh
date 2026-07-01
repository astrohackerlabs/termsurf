#!/usr/bin/env bash
set -euo pipefail

# Build and package a release. Publishing requires TERMSURF_RELEASE_PUBLISH=1.
# Requires local checkouts at ~/dev/termsurf and ~/dev/homebrew-termsurf.
# Usage: scripts/release.sh [version]
# Default version: 0.1.0

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
source "$SCRIPT_DIR/roamium-resources.sh"
VERSION="${1:-0.1.0}"
ARCH="aarch64-apple-darwin"
TARBALL_NAME="termsurf-${VERSION}-${ARCH}.tar.gz"
STAGING_DIR="$REPO_DIR/dist/release"
CHROMIUM_OUT="$REPO_DIR/chromium/src/out/Default"
WEBKIT_RELEASE_OUT="$REPO_DIR/webkit/src/WebKitBuild/Release"
GHOSTBOARD_APP="$REPO_DIR/ghostboard/macos/build/Release/TermSurf.app"
SURFARI_LIB="$REPO_DIR/surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib"
PUBLIC_REPO="${TERMSURF_PUBLIC_REPO:-$HOME/dev/termsurf}"
PUBLIC_GITHUB_REPO="${TERMSURF_PUBLIC_GITHUB_REPO:-termsurf/termsurf}"
HOMEBREW_TAP_REPO="${TERMSURF_HOMEBREW_TAP_REPO:-$HOME/dev/homebrew-termsurf}"
CASK_FILE="$HOMEBREW_TAP_REPO/Casks/termsurf.rb"
SURFARI_RUNTIME_ARTIFACTS=(
  WebKit.framework
  WebCore.framework
  JavaScriptCore.framework
  WebKitLegacy.framework
  WebInspectorUI.framework
  WebGPU.framework
  libANGLE-shared.dylib
  libWebKitSwift.dylib
  libwebrtc.dylib
  com.apple.WebKit.GPU.xpc
  com.apple.WebKit.Model.xpc
  com.apple.WebKit.Networking.xpc
  com.apple.WebKit.WebContent.CaptivePortal.xpc
  com.apple.WebKit.WebContent.Development.xpc
  com.apple.WebKit.WebContent.EnhancedSecurity.xpc
  com.apple.WebKit.WebContent.xpc
)

surfari_runtime_artifact_source() {
  local artifact="$1"
  local source="$WEBKIT_RELEASE_OUT/$artifact"

  if [ "$artifact" = "libWebKitSwift.dylib" ] &&
    [ ! -e "$source" ] &&
    [ -e "$WEBKIT_RELEASE_OUT/WebKit.framework/Versions/A/Frameworks/libWebKitSwift.dylib" ]; then
    source="$WEBKIT_RELEASE_OUT/WebKit.framework/Versions/A/Frameworks/libWebKitSwift.dylib"
  fi

  printf '%s\n' "$source"
}

cleanup_surfari_rpaths() {
  local surfari_bin="$STAGING_DIR/surfari/surfari"
  local surfari_lib="$STAGING_DIR/surfari/libtermsurf_webkit.dylib"

  install_name_tool -delete_rpath "$REPO_DIR/surfari/libtermsurf_webkit/build" "$surfari_bin" 2>/dev/null || true
  install_name_tool -delete_rpath "$WEBKIT_RELEASE_OUT" "$surfari_lib" 2>/dev/null || true
  install_name_tool -add_rpath "@loader_path" "$surfari_lib" 2>/dev/null || true
}

echo "==> Packaging TermSurf v${VERSION} for ${ARCH}..."

check_ghostboard_version() {
  local cli_version
  local short_version
  local build_version
  local first_line

  cli_version="$("$GHOSTBOARD_APP/Contents/MacOS/ghostboard" +version 2>&1)"
  first_line="$(printf '%s\n' "$cli_version" | sed -n '1p')"
  short_version="$(/usr/bin/defaults read "$GHOSTBOARD_APP/Contents/Info" CFBundleShortVersionString)"
  build_version="$(/usr/bin/defaults read "$GHOSTBOARD_APP/Contents/Info" CFBundleVersion)"

  if [ "$first_line" != "TermSurf $VERSION" ]; then
    echo "Error: Ghostboard CLI version mismatch"
    echo "  expected: TermSurf $VERSION"
    echo "  actual:   $first_line"
    echo "Rebuild with: TERMSURF_VERSION=$VERSION scripts/build.sh all --release"
    exit 1
  fi

  if [ "$short_version" != "$VERSION" ]; then
    echo "Error: CFBundleShortVersionString mismatch"
    echo "  expected: $VERSION"
    echo "  actual:   $short_version"
    echo "Rebuild with: TERMSURF_VERSION=$VERSION scripts/build.sh all --release"
    exit 1
  fi

  if [ "$build_version" != "$VERSION" ]; then
    echo "Error: CFBundleVersion mismatch"
    echo "  expected: $VERSION"
    echo "  actual:   $build_version"
    echo "Rebuild with: TERMSURF_VERSION=$VERSION scripts/build.sh all --release"
    exit 1
  fi
}

# Check release builds exist
for f in \
  "$REPO_DIR/target/release/web" \
  "$REPO_DIR/target/release/termsurf" \
  "$GHOSTBOARD_APP/Contents/MacOS/ghostboard" \
  "$REPO_DIR/target/release/roamium" \
  "$REPO_DIR/target/release/surfari" \
  "$SURFARI_LIB"; do
  if [ ! -f "$f" ]; then
    echo "Error: Release build not found: $f"
    echo "Run: scripts/build.sh all --release"
    exit 1
  fi
done

check_ghostboard_version

for artifact in "${SURFARI_RUNTIME_ARTIFACTS[@]}"; do
  artifact_source="$(surfari_runtime_artifact_source "$artifact")"
  if [ ! -e "$artifact_source" ]; then
    echo "Error: Surfari runtime artifact not found: $artifact_source"
    echo "Run: scripts/build.sh all --release"
    exit 1
  fi
done

# Clean and create staging directory
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR/roamium"
mkdir -p "$STAGING_DIR/surfari"
mkdir -p "$STAGING_DIR/gtui"

# Copy binaries
echo "==> Copying binaries..."
cp "$REPO_DIR/target/release/web" "$STAGING_DIR/"
cp "$REPO_DIR/target/release/termsurf" "$STAGING_DIR/"
cp "$REPO_DIR/target/release/roamium" "$STAGING_DIR/roamium/"
cp "$REPO_DIR/target/release/surfari" "$STAGING_DIR/surfari/"
cp -R "$REPO_DIR/gtui/app" "$STAGING_DIR/gtui/"

# Copy Chromium dylibs and resources
copy_roamium_runtime_resources "$CHROMIUM_OUT" "$STAGING_DIR/roamium"

# Copy Surfari WebKit runtime resources
echo "==> Copying Surfari runtime resources..."
cp "$SURFARI_LIB" "$STAGING_DIR/surfari/"
for artifact in "${SURFARI_RUNTIME_ARTIFACTS[@]}"; do
  cp -R "$(surfari_runtime_artifact_source "$artifact")" "$STAGING_DIR/surfari/"
done
cleanup_surfari_rpaths

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

if [ "${TERMSURF_RELEASE_PACKAGE_ONLY:-0}" = "1" ] ||
  [ "${TERMSURF_RELEASE_PUBLISH:-0}" != "1" ]; then
  echo "==> Package-only mode: skipping GitHub upload and Homebrew cask update."
  echo "==> Tarball: dist/$TARBALL_NAME"
  exit 0
fi

if [ ! -d "$PUBLIC_REPO/.git" ]; then
  echo "Error: Public repo checkout not found: $PUBLIC_REPO"
  echo "Clone git@github.com:termsurf/termsurf.git to $PUBLIC_REPO"
  exit 1
fi

if [ ! -d "$HOMEBREW_TAP_REPO/.git" ]; then
  echo "Error: Homebrew tap checkout not found: $HOMEBREW_TAP_REPO"
  echo "Clone git@github.com:termsurf/homebrew-termsurf.git to $HOMEBREW_TAP_REPO"
  exit 1
fi

if [ ! -f "$CASK_FILE" ]; then
  echo "Error: Homebrew cask not found: $CASK_FILE"
  exit 1
fi

cd "$HOMEBREW_TAP_REPO"
if [ "$(git rev-parse --abbrev-ref HEAD)" != "main" ]; then
  echo "==> Checking out Homebrew tap main branch..."
  git checkout main
fi

if [ -n "$(git status --short)" ]; then
  echo "Error: Homebrew tap has uncommitted changes: $HOMEBREW_TAP_REPO"
  echo "Commit, stash, or reset the tap before publishing."
  exit 1
fi

cd "$PUBLIC_REPO"
if [ "$(git rev-parse --abbrev-ref HEAD)" != "main" ]; then
  echo "==> Checking out public repo main branch..."
  git checkout main
fi

if [ -n "$(git status --short)" ]; then
  echo "Error: Public repo has uncommitted changes: $PUBLIC_REPO"
  echo "Commit the synced public source before publishing."
  exit 1
fi

if git rev-parse -q --verify "refs/tags/v${VERSION}" >/dev/null; then
  if [ "$(git rev-parse "refs/tags/v${VERSION}^{commit}")" != "$(git rev-parse HEAD)" ]; then
    echo "Error: v${VERSION} already exists and does not point at public repo HEAD"
    exit 1
  fi
else
  echo "==> Tagging public repo v${VERSION}..."
  git tag -a "v${VERSION}" -m "v${VERSION}"
fi

echo "==> Pushing public source release..."
git push origin main "v${VERSION}"

# Upload to GitHub (delete old release if it exists)
echo "==> Uploading to GitHub..."
cd "$REPO_DIR"
gh release delete "v${VERSION}" --repo "$PUBLIC_GITHUB_REPO" --yes 2>/dev/null || true
gh release create "v${VERSION}" "dist/${TARBALL_NAME}" \
  --repo "$PUBLIC_GITHUB_REPO" \
  --title "v${VERSION}" \
  --notes "v${VERSION}"

cd "$HOMEBREW_TAP_REPO"

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
