#!/bin/bash
# Build TermSurf 3.0 in Debug mode
#
# Usage:
#   ./scripts/build-debug.sh [--clean] [--open] [--open-web]
#
# Flags:
#   --clean     Clear build caches and do a fresh build
#   --open      Run termsurf-gui after building
#   --open-web  Run web CLI after building (for testing CEF)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
CEF_RS_DIR="$(dirname "$REPO_DIR")/cef-rs"

# Parse flags
CLEAN=false
OPEN=false
OPEN_WEB=false
for arg in "$@"; do
    case $arg in
        --clean) CLEAN=true ;;
        --open) OPEN=true ;;
        --open-web) OPEN_WEB=true ;;
    esac
done

# Clean if requested
if [ "$CLEAN" = true ]; then
    echo "=== Cleaning build caches ==="
    rm -rf "$REPO_DIR/target/debug"
    echo "Cleared target/debug"
fi

# Clean stale XPC service registration (prevents launchd from loading old binary)
launchctl bootout "gui/$(id -u)/com.termsurf.launcher" 2>/dev/null || true
rm -f /private/tmp/com.termsurf.launcher.plist

echo "=== Building TermSurf 3.0 (Debug) ==="

# 1. Build cef-rs helpers first (needed for bundle)
echo "Building CEF helpers..."
cd "$CEF_RS_DIR"
cargo build -p cef-osr
cargo run -p cef --bin bundle-cef-app -- cef-osr -o cef-osr.app

# Verify cef-osr.app was created
if [ ! -d "$CEF_RS_DIR/cef-osr.app" ]; then
    echo "ERROR: cef-osr.app not found at $CEF_RS_DIR/cef-osr.app"
    exit 1
fi

# 2. Build ts3 workspace
echo "Building workspace..."
cd "$REPO_DIR"
cargo build

# 3. Create app bundle
APP_BUNDLE="$REPO_DIR/target/debug/wezterm-gui.app"
echo "Creating app bundle at $APP_BUNDLE..."

rm -rf "$APP_BUNDLE"
mkdir -p "$APP_BUNDLE/Contents/MacOS"
mkdir -p "$APP_BUNDLE/Contents/Frameworks"
mkdir -p "$APP_BUNDLE/Contents/Resources"

# 4. Copy executables
cp "$REPO_DIR/target/debug/termsurf-gui" "$APP_BUNDLE/Contents/MacOS/"
cp "$REPO_DIR/target/debug/wezterm" "$APP_BUNDLE/Contents/MacOS/"
cp "$REPO_DIR/target/debug/web" "$APP_BUNDLE/Contents/MacOS/"

# 4b. Create TermSurf Profile Helper app bundle (proper macOS helper pattern)
echo "Creating TermSurf Profile Helper app..."
PROFILE_HELPER="$APP_BUNDLE/Contents/Frameworks/TermSurf Profile Helper.app"
mkdir -p "$PROFILE_HELPER/Contents/MacOS"
cp "$REPO_DIR/target/debug/termsurf-profile" "$PROFILE_HELPER/Contents/MacOS/"
cp "$REPO_DIR/termsurf-profile/helper-app/Info.plist" "$PROFILE_HELPER/Contents/"

# 5. Copy CEF framework
echo "Copying CEF framework..."
cp -R "$CEF_RS_DIR/cef-osr.app/Contents/Frameworks/Chromium Embedded Framework.framework" \
      "$APP_BUNDLE/Contents/Frameworks/"

# 6. Copy and rename CEF helper apps
echo "Copying CEF helper apps..."
for suffix in "" " (GPU)" " (Renderer)" " (Plugin)" " (Alerts)"; do
    src="$CEF_RS_DIR/cef-osr.app/Contents/Frameworks/cef-osr Helper${suffix}.app"
    dst="$APP_BUNDLE/Contents/Frameworks/WezTerm Helper${suffix}.app"
    if [ -d "$src" ]; then
        cp -R "$src" "$dst"
        # Update Info.plist to rename from cef-osr to WezTerm
        sed -i '' 's/cef-osr/WezTerm/g' "$dst/Contents/Info.plist"
        # Rename the binary inside the helper app
        mv "$dst/Contents/MacOS/cef-osr Helper${suffix}" "$dst/Contents/MacOS/WezTerm Helper${suffix}"
    else
        echo "WARNING: Helper not found: $src"
    fi
done

# 7. Copy XPC launcher service
echo "Copying XPC launcher service..."
mkdir -p "$APP_BUNDLE/Contents/XPCServices/com.termsurf.launcher.xpc/Contents/MacOS"
cp "$REPO_DIR/target/debug/termsurf-launcher" \
   "$APP_BUNDLE/Contents/XPCServices/com.termsurf.launcher.xpc/Contents/MacOS/"
cp "$REPO_DIR/termsurf-launcher/xpc-service/Info.plist" \
   "$APP_BUNDLE/Contents/XPCServices/com.termsurf.launcher.xpc/Contents/"

# 8. Create Info.plist
cat > "$APP_BUNDLE/Contents/Info.plist" << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>termsurf-gui</string>
    <key>CFBundleIdentifier</key>
    <string>org.wezfurlong.wezterm</string>
    <key>CFBundleName</key>
    <string>WezTerm</string>
    <key>CFBundleVersion</key>
    <string>1.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSEnvironment</key>
    <dict>
        <key>MallocNanoZone</key>
        <string>0</string>
    </dict>
</dict>
</plist>
PLIST

# 9. Sign the bundle
echo "Signing bundle..."
codesign --sign - --force --deep "$APP_BUNDLE"

# 10. Register XPC launcher as launchd Mach service
LAUNCHER_BIN="$APP_BUNDLE/Contents/XPCServices/com.termsurf.launcher.xpc/Contents/MacOS/termsurf-launcher"
PLIST_PATH="/tmp/com.termsurf.launcher.plist"

cat > "$PLIST_PATH" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.termsurf.launcher</string>
    <key>MachServices</key>
    <dict>
        <key>com.termsurf.launcher</key>
        <true/>
    </dict>
    <key>ProgramArguments</key>
    <array>
        <string>$LAUNCHER_BIN</string>
    </array>
</dict>
</plist>
EOF

launchctl bootstrap "gui/$(id -u)" "$PLIST_PATH"
echo "Registered com.termsurf.launcher with launchd"

echo ""
echo "=== Debug Build Complete ==="
echo "App bundle: $APP_BUNDLE"
echo "  Contents/MacOS/termsurf-gui                             (terminal)"
echo "  Contents/MacOS/wezterm                                  (CLI)"
echo "  Contents/MacOS/web                                      (web coordinator)"
echo "  Contents/Frameworks/TermSurf Profile Helper.app         (CEF profile server)"
echo ""

# Open if requested
if [ "$OPEN" = true ]; then
    echo "Running termsurf-gui..."
    open --stdout /tmp/termsurf-gui.log --stderr /tmp/termsurf-gui.log "$APP_BUNDLE"
    echo "Logs: /tmp/termsurf-gui.log"
fi

if [ "$OPEN_WEB" = true ]; then
    echo "Running web CLI..."
    "$APP_BUNDLE/Contents/MacOS/web"
fi
