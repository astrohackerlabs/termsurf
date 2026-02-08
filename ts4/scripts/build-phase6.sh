#!/bin/bash
# Build and run Phase 6: XPC between Swift window, Rust terminal, and C++ browser
#
# Usage:
#   ./scripts/build-phase6.sh [--clean]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TS4_DIR="$(dirname "$SCRIPT_DIR")"

# Parse flags
CLEAN=false
for arg in "$@"; do
    case $arg in
        --clean) CLEAN=true ;;
    esac
done

# Clean stale XPC service registrations
echo "=== Cleaning stale XPC services ==="
launchctl bootout "gui/$(id -u)/com.termsurf.ts4.terminal" 2>/dev/null || true
launchctl bootout "gui/$(id -u)/com.termsurf.ts4.browser" 2>/dev/null || true
rm -f /tmp/com.termsurf.ts4.terminal.plist
rm -f /tmp/com.termsurf.ts4.browser.plist
rm -f /tmp/termsurf-terminal.log
rm -f /tmp/termsurf-browser.log

if [ "$CLEAN" = true ]; then
    echo "=== Cleaning build caches ==="
    rm -rf "$TS4_DIR/target/debug"
    rm -rf "$TS4_DIR/termsurf-window/.build"
fi

# 1. Build Rust terminal
echo ""
echo "=== Building Rust terminal ==="
cd "$TS4_DIR"
cargo build -p termsurf-terminal

TERMINAL_BIN="$TS4_DIR/target/debug/termsurf-terminal"
echo "Built: $TERMINAL_BIN"

# 2. Build C++ browser
echo ""
echo "=== Building C++ browser ==="
cd "$TS4_DIR/termsurf-browser"
make

BROWSER_BIN="$TS4_DIR/termsurf-browser/termsurf-browser"
echo "Built: $BROWSER_BIN"

# 3. Build Swift window
echo ""
echo "=== Building Swift window ==="
cd "$TS4_DIR/termsurf-window"
swift build

WINDOW_BIN="$TS4_DIR/termsurf-window/.build/debug/termsurf-window"
echo "Built: $WINDOW_BIN"

# 4. Register XPC services with launchd

echo ""
echo "=== Registering XPC services ==="

# Terminal service
PLIST_PATH="/tmp/com.termsurf.ts4.terminal.plist"
cat > "$PLIST_PATH" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.termsurf.ts4.terminal</string>
    <key>MachServices</key>
    <dict>
        <key>com.termsurf.ts4.terminal</key>
        <true/>
    </dict>
    <key>ProgramArguments</key>
    <array>
        <string>$TERMINAL_BIN</string>
    </array>
    <key>StandardOutPath</key>
    <string>/tmp/termsurf-terminal.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/termsurf-terminal.log</string>
</dict>
</plist>
EOF
launchctl bootstrap "gui/$(id -u)" "$PLIST_PATH"
echo "Registered com.termsurf.ts4.terminal"

# Browser service
PLIST_PATH="/tmp/com.termsurf.ts4.browser.plist"
cat > "$PLIST_PATH" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.termsurf.ts4.browser</string>
    <key>MachServices</key>
    <dict>
        <key>com.termsurf.ts4.browser</key>
        <true/>
    </dict>
    <key>ProgramArguments</key>
    <array>
        <string>$BROWSER_BIN</string>
    </array>
    <key>StandardOutPath</key>
    <string>/tmp/termsurf-browser.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/termsurf-browser.log</string>
</dict>
</plist>
EOF
launchctl bootstrap "gui/$(id -u)" "$PLIST_PATH"
echo "Registered com.termsurf.ts4.browser"

# 5. Run Swift window
echo ""
echo "=== Running Swift window ==="
echo "Terminal logs: /tmp/termsurf-terminal.log"
echo "Browser logs:  /tmp/termsurf-browser.log"
echo ""
"$WINDOW_BIN"
