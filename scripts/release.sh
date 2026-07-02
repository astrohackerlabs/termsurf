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
GIRLBAT_LIB="$REPO_DIR/girlbat/libtermsurf_ladybird/build/libtermsurf_ladybird.dylib"
LADYBIRD_RELEASE_OUT="$REPO_DIR/vendor/ladybird/Build/release"
LADYBIRD_RELEASE_APP_MACOS="$LADYBIRD_RELEASE_OUT/bin/Ladybird.app/Contents/MacOS"
LADYBIRD_RELEASE_APP_RESOURCES="$LADYBIRD_RELEASE_OUT/bin/Ladybird.app/Contents/Resources"
PUBLIC_REPO="${TERMSURF_PUBLIC_REPO:-$HOME/dev/termsurf}"
PUBLIC_GITHUB_REPO="${TERMSURF_PUBLIC_GITHUB_REPO:-termsurf/termsurf}"
HOMEBREW_TAP_REPO="${TERMSURF_HOMEBREW_TAP_REPO:-$HOME/dev/homebrew-termsurf}"
CASK_FILE="$HOMEBREW_TAP_REPO/Casks/termsurf.rb"
GIRLBAT_HELPER_ARTIFACTS=(
  ImageDecoder
  RequestServer
  WebContent
  WebWorker
)
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

is_system_dylib_ref() {
  local ref="$1"
  case "$ref" in
    /usr/lib/* | /System/*) return 0 ;;
    *) return 1 ;;
  esac
}

otool_deps() {
  local file="$1"
  otool -L "$file" | awk 'NR > 1 { print $1 }'
}

otool_rpaths() {
  local file="$1"
  otool -l "$file" | awk '
    $1 == "cmd" && $2 == "LC_RPATH" { in_rpath = 1; next }
    in_rpath && $1 == "path" { print $2; in_rpath = 0 }
  '
}

path_in_list() {
  local needle="$1"
  shift
  local item
  for item in "$@"; do
    if [ "$item" = "$needle" ]; then
      return 0
    fi
  done
  return 1
}

contains_bad_release_path() {
  local path="$1"
  case "$path" in
    "$REPO_DIR"* | "$HOME/dev/"* | /opt/homebrew/Cellar/* | */vcpkg_installed/* | */vendor/ladybird/*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

resolve_dylib_ref() {
  local file="$1"
  local ref="$2"
  shift 2
  local loader_dir
  local candidate
  local rpath
  loader_dir="$(dirname "$file")"

  case "$ref" in
    /usr/lib/* | /System/*)
      return 1
      ;;
    /*)
      if [ -e "$ref" ]; then
        printf '%s\n' "$ref"
        return 0
      fi
      ;;
    @loader_path/*)
      candidate="$loader_dir/${ref#@loader_path/}"
      if [ -e "$candidate" ]; then
        printf '%s\n' "$candidate"
        return 0
      fi
      ;;
    @executable_path/*)
      candidate="$loader_dir/${ref#@executable_path/}"
      if [ -e "$candidate" ]; then
        printf '%s\n' "$candidate"
        return 0
      fi
      ;;
    @rpath/*)
      local suffix="${ref#@rpath/}"
      while IFS= read -r rpath; do
        [ -n "$rpath" ] || continue
        case "$rpath" in
          @loader_path/*)
            candidate="$loader_dir/${rpath#@loader_path/}/$suffix"
            ;;
          @executable_path/*)
            candidate="$loader_dir/${rpath#@executable_path/}/$suffix"
            ;;
          /*)
            candidate="$rpath/$suffix"
            ;;
          *)
            candidate=""
            ;;
        esac
        if [ -n "$candidate" ] && [ -e "$candidate" ]; then
          printf '%s\n' "$candidate"
          return 0
        fi
      done < <(otool_rpaths "$file")
      ;;
  esac

  local search_dir
  for search_dir in "$@"; do
    candidate="$search_dir/$(basename "$ref")"
    if [ -e "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  return 1
}

rewrite_macho_for_girlbat_package() {
  local file="$1"
  local expected_rpath="$2"
  local dep
  local rpath
  local basename_ref

  while IFS= read -r rpath; do
    [ -n "$rpath" ] || continue
    install_name_tool -delete_rpath "$rpath" "$file" 2>/dev/null || true
  done < <(otool_rpaths "$file")
  install_name_tool -add_rpath "$expected_rpath" "$file" 2>/dev/null || true

  if [[ "$file" == *.dylib ]]; then
    install_name_tool -id "@rpath/$(basename "$file")" "$file" 2>/dev/null || true
  fi

  while IFS= read -r dep; do
    [ -n "$dep" ] || continue
    if is_system_dylib_ref "$dep"; then
      continue
    fi
    basename_ref="$(basename "$dep")"
    if [ -f "$STAGING_DIR/girlbat/lib/$basename_ref" ]; then
      install_name_tool -change "$dep" "@rpath/$basename_ref" "$file" 2>/dev/null || true
    fi
  done < <(otool_deps "$file")
}

validate_girlbat_macho_file() {
  local file="$1"
  local expected_rpath="$2"
  local dep
  local rpath
  local saw_expected_rpath=0
  local basename_ref

  while IFS= read -r rpath; do
    [ -n "$rpath" ] || continue
    if [ "$rpath" = "$expected_rpath" ]; then
      saw_expected_rpath=1
      continue
    fi
    echo "Error: unexpected Girlbat rpath in $file: $rpath" >&2
    exit 1
  done < <(otool_rpaths "$file")

  if [ "$saw_expected_rpath" = "0" ]; then
    echo "Error: missing Girlbat rpath in $file: $expected_rpath" >&2
    exit 1
  fi

  while IFS= read -r dep; do
    [ -n "$dep" ] || continue
    if is_system_dylib_ref "$dep"; then
      continue
    fi
    if contains_bad_release_path "$dep"; then
      echo "Error: bad Girlbat dependency path in $file: $dep" >&2
      exit 1
    fi
    case "$dep" in
      @rpath/*)
        basename_ref="$(basename "$dep")"
        if [ ! -f "$STAGING_DIR/girlbat/lib/$basename_ref" ]; then
          echo "Error: Girlbat dependency not packaged for $file: $dep" >&2
          exit 1
        fi
        ;;
      *)
        echo "Error: non-system Girlbat dependency is not @rpath-relative in $file: $dep" >&2
        exit 1
        ;;
    esac
  done < <(otool_deps "$file")
}

copy_girlbat_dylib_closure() {
  local queue=("$@")
  local processed=()
  local search_dirs=(
    "$LADYBIRD_RELEASE_OUT/lib"
    "$LADYBIRD_RELEASE_OUT/bin"
    "$LADYBIRD_RELEASE_OUT/vcpkg_installed/arm64-osx-dynamic/lib"
    "$LADYBIRD_RELEASE_APP_MACOS"
    "$REPO_DIR/girlbat/libtermsurf_ladybird/build"
  )
  local file
  local dep
  local resolved
  local dest
  local basename_ref

  while [ "${#queue[@]}" -gt 0 ]; do
    file="${queue[0]}"
    queue=("${queue[@]:1}")
    if [ "${#processed[@]}" -gt 0 ] && path_in_list "$file" "${processed[@]}"; then
      continue
    fi
    processed+=("$file")

    while IFS= read -r dep; do
      [ -n "$dep" ] || continue
      if is_system_dylib_ref "$dep"; then
        continue
      fi
      resolved="$(resolve_dylib_ref "$file" "$dep" "${search_dirs[@]}")" || {
        echo "Error: unable to resolve Girlbat dependency $dep from $file" >&2
        exit 1
      }
      basename_ref="$(basename "$resolved")"
      dest="$STAGING_DIR/girlbat/lib/$basename_ref"
      if [ ! -f "$dest" ]; then
        cp "$resolved" "$dest"
        queue+=("$resolved")
      fi
    done < <(otool_deps "$file")
  done
}

copy_girlbat_runtime_resources() {
  local helper
  local helper_source
  local executable_sources=()

  if [ ! -f "$REPO_DIR/target/release/girlbat" ]; then
    echo "Error: Release build not found: $REPO_DIR/target/release/girlbat" >&2
    echo "Run: scripts/build.sh all --release" >&2
    exit 1
  fi
  if [ ! -f "$GIRLBAT_LIB" ]; then
    echo "Error: Girlbat ABI dylib not found: $GIRLBAT_LIB" >&2
    echo "Run: TERMSURF_LADYBIRD_BACKEND=real scripts/build.sh girlbat --release" >&2
    exit 1
  fi
  if [ ! -d "$LADYBIRD_RELEASE_APP_RESOURCES" ]; then
    echo "Error: Ladybird release resources not found: $LADYBIRD_RELEASE_APP_RESOURCES" >&2
    echo "Run: TERMSURF_LADYBIRD_BACKEND=real scripts/build.sh girlbat --release" >&2
    exit 1
  fi

  mkdir -p "$STAGING_DIR/girlbat/bin" "$STAGING_DIR/girlbat/lib" "$STAGING_DIR/girlbat/Resources"
  cp "$REPO_DIR/target/release/girlbat" "$STAGING_DIR/girlbat/bin/"
  cp "$GIRLBAT_LIB" "$STAGING_DIR/girlbat/lib/"
  cp -R "$LADYBIRD_RELEASE_APP_RESOURCES/." "$STAGING_DIR/girlbat/Resources/"
  if [ -d "$REPO_DIR/target/Resources" ]; then
    cp -R "$REPO_DIR/target/Resources/." "$STAGING_DIR/girlbat/Resources/"
  fi

  for helper in "${GIRLBAT_HELPER_ARTIFACTS[@]}"; do
    helper_source="$LADYBIRD_RELEASE_APP_MACOS/$helper"
    if [ ! -f "$helper_source" ]; then
      echo "Error: Ladybird helper not found: $helper_source" >&2
      exit 1
    fi
    cp "$helper_source" "$STAGING_DIR/girlbat/bin/"
    executable_sources+=("$helper_source")
  done

  for helper_source in "$LADYBIRD_RELEASE_APP_MACOS/Compositor" "$LADYBIRD_RELEASE_OUT/bin/Compositor"; do
    if [ -f "$helper_source" ]; then
      cp "$helper_source" "$STAGING_DIR/girlbat/bin/"
      executable_sources+=("$helper_source")
      break
    fi
  done

  copy_girlbat_dylib_closure \
    "$REPO_DIR/target/release/girlbat" \
    "$GIRLBAT_LIB" \
    "${executable_sources[@]}"

  local file
  for file in "$STAGING_DIR/girlbat/bin/"*; do
    [ -f "$file" ] || continue
    rewrite_macho_for_girlbat_package "$file" "@loader_path/../lib"
  done
  for file in "$STAGING_DIR/girlbat/lib/"*.dylib; do
    [ -f "$file" ] || continue
    rewrite_macho_for_girlbat_package "$file" "@loader_path"
  done

  for file in "$STAGING_DIR/girlbat/bin/"*; do
    [ -f "$file" ] || continue
    validate_girlbat_macho_file "$file" "@loader_path/../lib"
  done
  for file in "$STAGING_DIR/girlbat/lib/"*.dylib; do
    [ -f "$file" ] || continue
    validate_girlbat_macho_file "$file" "@loader_path"
  done

  local smoke_home
  local smoke_out
  local smoke_err
  local resource_root
  smoke_home="$(mktemp -d)"
  smoke_out="$(mktemp)"
  smoke_err="$(mktemp)"
  (
    cd /tmp
    HOME="$smoke_home" \
      XDG_CONFIG_HOME="$smoke_home/.config" \
      "$STAGING_DIR/girlbat/bin/girlbat" --termsurf-resource-root-smoke >"$smoke_out" 2>"$smoke_err"
  )
  resource_root="$(sed -n '1p' "$smoke_out")"
  if [ "$resource_root" != "$STAGING_DIR/girlbat/Resources" ]; then
    echo "Error: packaged Girlbat resource root mismatch" >&2
    echo "  expected: $STAGING_DIR/girlbat/Resources" >&2
    echo "  actual:   $resource_root" >&2
    cat "$smoke_err" >&2
    exit 1
  fi
  if ! grep -q 'runtime=libtermsurf_ladybird-ladybird' "$smoke_err"; then
    echo "Error: packaged Girlbat is not using the real Ladybird runtime" >&2
    cat "$smoke_err" >&2
    exit 1
  fi
  rm -rf "$smoke_home"
  rm -f "$smoke_out" "$smoke_err"
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
  "$REPO_DIR/target/release/girlbat" \
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
mkdir -p "$STAGING_DIR/girlbat"
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

# Copy Girlbat Ladybird runtime resources
echo "==> Copying Girlbat runtime resources..."
copy_girlbat_runtime_resources

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
