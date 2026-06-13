#!/usr/bin/env bash
# Issue 802 / Exp 14 — launch the debug Roastty app and wait until its window is up. Pairs
# with stop-app.sh. Prints the main process PID. Use start -> drive/capture -> stop in one
# flow so the app is never left running on the user's screen across turns.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
if [ -n "${ROASTTY_APP:-}" ]; then
  APP="$ROASTTY_APP"
else
  XCODE_APP="$ROOT/roastty/macos/build/Build/Products/Debug/Roastty.app"
  FLAT_APP="$ROOT/roastty/macos/build/Debug/Roastty.app"
  if [ -d "$XCODE_APP" ] && { [ ! -d "$FLAT_APP" ] || [ "$XCODE_APP" -nt "$FLAT_APP" ]; }; then
    APP="$XCODE_APP"
  else
    APP="$FLAT_APP"
  fi
fi
[ -d "$APP" ] || { echo "app not built: $APP" >&2; exit 1; }

open "$APP"
for _ in $(seq 1 20); do
  pid=$(pgrep -f "$APP/Contents/MacOS/roastty" | head -1 || true)
  [ -n "$pid" ] && { osascript -e 'delay 1' >/dev/null 2>&1; echo "$pid"; exit 0; }
  osascript -e 'delay 0.5' >/dev/null 2>&1
done
echo "launch timed out (no roastty PID under the build path — likely crashed on init)" >&2
exit 1
