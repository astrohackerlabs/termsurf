#!/usr/bin/env bash

SURFARI_REQUIRED_RUNTIME_RESOURCES=(
  "WebKit.framework"
  "WebCore.framework"
  "JavaScriptCore.framework"
  "WebKitLegacy.framework"
  "WebInspectorUI.framework"
  "WebGPU.framework"
  "libANGLE-shared.dylib"
  "libWebKitSwift.dylib"
  "libwebrtc.dylib"
  "com.apple.WebKit.GPU.xpc"
  "com.apple.WebKit.Model.xpc"
  "com.apple.WebKit.Networking.xpc"
  "com.apple.WebKit.WebContent.CaptivePortal.xpc"
  "com.apple.WebKit.WebContent.Development.xpc"
  "com.apple.WebKit.WebContent.EnhancedSecurity.xpc"
  "com.apple.WebKit.WebContent.xpc"
)

surfari_framework_binary() {
  local framework="$1"
  local name
  name="$(basename "$framework" .framework)"
  printf '%s/Versions/A/%s\n' "$framework" "$name"
}

surfari_xpc_executable() {
  local xpc="$1"
  local plist="$xpc/Contents/Info.plist"

  if [ ! -f "$plist" ]; then
    echo "Error: missing XPC Info.plist: $plist" >&2
    return 1
  fi

  local executable
  executable="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist")"
  printf '%s/Contents/MacOS/%s\n' "$xpc" "$executable"
}

materialize_webkit_bundle_symlinks() {
  local destination="$1"
  local webkit_framework="$destination/WebKit.framework/Versions/A"

  local symlink
  for symlink in \
    "$webkit_framework/Frameworks/libWebKitSwift.dylib" \
    "$webkit_framework/XPCServices"/*.xpc; do
    [ -L "$symlink" ] || continue

    local target
    target="$destination/$(basename "$symlink")"
    if [ ! -e "$target" ]; then
      echo "Error: Surfari WebKit bundle symlink target missing: $target" >&2
      return 1
    fi

    rm "$symlink"
    cp -R "$target" "$symlink"
  done
}

copy_surfari_runtime_resources() {
  local webkit_build="$1"
  local destination="$2"

  mkdir -p "$destination"

  echo "==> Copying Surfari WebKit runtime resources..."
  local resource
  for resource in "${SURFARI_REQUIRED_RUNTIME_RESOURCES[@]}"; do
    local source_path="$webkit_build/$resource"
    local destination_path="$destination/$resource"

    if [ ! -e "$source_path" ]; then
      echo "Error: Required Surfari runtime resource missing: $source_path" >&2
      echo "Run: webkit/src/Tools/Scripts/build-webkit --debug" >&2
      return 1
    fi

    rm -rf "$destination_path"
    cp -R "$source_path" "$destination_path"
  done

  # The WebKit debug build currently ships this dangling framework-level
  # symlink. It is not needed at runtime and breaks strict bundle verification
  # after packaging.
  if [ -L "$destination/WebCore.framework/Frameworks" ] &&
    [ ! -e "$destination/WebCore.framework/Frameworks" ]; then
    rm "$destination/WebCore.framework/Frameworks"
  fi

  materialize_webkit_bundle_symlinks "$destination"
}

rewrite_surfari_runtime_paths() {
  local webkit_build="$1"
  local destination="$2"
  local bridge="$destination/libtermsurf_webkit.dylib"

  if [ ! -f "$bridge" ]; then
    echo "Error: Surfari bridge dylib missing: $bridge" >&2
    return 1
  fi

  install_name_tool -delete_rpath "$webkit_build" "$bridge" 2>/dev/null || true
  install_name_tool -add_rpath "@loader_path" "$bridge" 2>/dev/null || true

  install_name_tool -id "@rpath/libtermsurf_webkit.dylib" "$bridge"

  local framework
  for framework in WebKit.framework WebCore.framework JavaScriptCore.framework WebKitLegacy.framework WebInspectorUI.framework WebGPU.framework; do
    local binary
    binary="$(surfari_framework_binary "$destination/$framework")"
    local name="${framework%.framework}"
    install_name_tool -id "@rpath/$framework/Versions/A/$name" "$binary"
    install_name_tool -add_rpath "@loader_path/../../.." "$binary" 2>/dev/null || true
  done

  local artifacts=(
    "$bridge"
    "$destination/WebKit.framework/Versions/A/WebKit"
    "$destination/WebCore.framework/Versions/A/WebCore"
    "$destination/WebKitLegacy.framework/Versions/A/WebKitLegacy"
    "$destination/WebGPU.framework/Versions/A/WebGPU"
    "$destination/libANGLE-shared.dylib"
    "$destination/libWebKitSwift.dylib"
    "$destination/WebKit.framework/Versions/A/Frameworks/libWebKitSwift.dylib"
    "$destination/libwebrtc.dylib"
  )

  local artifact
  for artifact in "${artifacts[@]}"; do
    [ -f "$artifact" ] || continue
    install_name_tool \
      -change /System/Library/Frameworks/WebKit.framework/Versions/A/WebKit \
      @rpath/WebKit.framework/Versions/A/WebKit \
      "$artifact" 2>/dev/null || true
    install_name_tool \
      -change /System/Library/Frameworks/WebKit.framework/Versions/A/Frameworks/WebCore.framework/Versions/A/WebCore \
      @rpath/WebCore.framework/Versions/A/WebCore \
      "$artifact" 2>/dev/null || true
    install_name_tool \
      -change /System/Library/Frameworks/JavaScriptCore.framework/Versions/A/JavaScriptCore \
      @rpath/JavaScriptCore.framework/Versions/A/JavaScriptCore \
      "$artifact" 2>/dev/null || true
    install_name_tool \
      -change /System/Library/Frameworks/WebKit.framework/Versions/A/Frameworks/WebKitLegacy.framework/Versions/A/WebKitLegacy \
      @rpath/WebKitLegacy.framework/Versions/A/WebKitLegacy \
      "$artifact" 2>/dev/null || true
    install_name_tool \
      -change /System/Library/PrivateFrameworks/WebInspectorUI.framework/Versions/A/WebInspectorUI \
      @rpath/WebInspectorUI.framework/Versions/A/WebInspectorUI \
      "$artifact" 2>/dev/null || true
    install_name_tool \
      -change /System/Library/PrivateFrameworks/WebGPU.framework/Versions/A/WebGPU \
      @rpath/WebGPU.framework/Versions/A/WebGPU \
      "$artifact" 2>/dev/null || true
  done

  for artifact in \
    "$destination/libWebKitSwift.dylib" \
    "$destination/WebKit.framework/Versions/A/Frameworks/libWebKitSwift.dylib"; do
    [ -f "$artifact" ] || continue
    install_name_tool \
      -id @rpath/WebKit.framework/Versions/A/Frameworks/libWebKitSwift.dylib \
      "$artifact"
  done

  local xpc
  for xpc in \
    "$destination"/*.xpc \
    "$destination/WebKit.framework/Versions/A/XPCServices"/*.xpc; do
    [ -d "$xpc" ] || continue
    local executable
    executable="$(surfari_xpc_executable "$xpc")"
    install_name_tool -add_rpath "@loader_path/../../.." "$executable" 2>/dev/null || true
    install_name_tool \
      -change /System/Library/Frameworks/WebKit.framework/Versions/A/WebKit \
      @rpath/WebKit.framework/Versions/A/WebKit \
      "$executable" 2>/dev/null || true
  done
}

sign_surfari_runtime_artifacts() {
  local destination="$1"

  local artifact
  for artifact in surfari libtermsurf_webkit.dylib "${SURFARI_REQUIRED_RUNTIME_RESOURCES[@]}"; do
    local path="$destination/$artifact"
    [ -e "$path" ] || continue
    codesign --force --deep --sign - "$path"
  done
}
