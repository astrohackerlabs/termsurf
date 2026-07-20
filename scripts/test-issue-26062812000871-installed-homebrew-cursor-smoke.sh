#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CASK_TOKEN="astrohacker"
INSTALLED_CASK_VERSION="$(brew list --cask --versions "$CASK_TOKEN" 2>/dev/null | awk '{print $2}')"
if [ -z "${TERMSURF_SMOKE_VERSION:-}" ]; then
  VERSION="${INSTALLED_CASK_VERSION:-}"
else
  VERSION="$TERMSURF_SMOKE_VERSION"
fi
[ -n "$VERSION" ] || { echo "FAIL: could not derive installed cask version" >&2; exit 1; }
RUN_ID="$(date +%Y%m%d-%H%M%S)"
START_EPOCH="$(date +%s)"
LOG_DIR="$ROOT/logs/issue-26062812000871-exp1-installed-homebrew-cursor"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-issue871-exp1.XXXXXX")"
SITE_DIR="$RUN_DIR/site"
APP="/Applications/Astrohacker Terminal.app"
APP_BIN="$APP/Contents/MacOS/ahterm"
WEB="/opt/homebrew/bin/ahweb"
WEBKIT_HELPER="/opt/homebrew/opt/astrohacker-terminal-ah-webkitd/ah-webkitd"
COMMAND="$RUN_DIR/run-web.sh"
APP_LOG="$LOG_DIR/app-$RUN_ID.log"
WEBTUI_TRACE="$LOG_DIR/webtui-$RUN_ID.log"
HARNESS_LOG="$LOG_DIR/harness-$RUN_ID.log"
PID=""
HTTP_PID=""

mkdir -p "$LOG_DIR" "$SITE_DIR"

log() {
  printf '%s\n' "$*" | tee -a "$HARNESS_LOG"
}

fail() {
  log "FAIL: $*"
  exit 1
}

delay() {
  osascript -e "delay ${1:-0.5}" >/dev/null
}

cleanup() {
  if [ -n "${PID:-}" ] && kill -0 "$PID" >/dev/null 2>&1; then
    kill "$PID" >/dev/null 2>&1 || true
    delay 0.5 || true
    kill -9 "$PID" >/dev/null 2>&1 || true
  fi
  if [ -n "${HTTP_PID:-}" ] && kill -0 "$HTTP_PID" >/dev/null 2>&1; then
    kill "$HTTP_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

require_executable() {
  [ -x "$1" ] || fail "missing executable: $1"
}

require_unset() {
  local name="$1"
  if [ -n "${!name+x}" ]; then
    fail "$name must be unset for installed Homebrew cursor smoke"
  fi
}

line_count() {
  local file="$1"
  if [ -r "$file" ]; then
    wc -l <"$file" | tr -d ' '
  else
    printf '0\n'
  fi
}

wait_for_file_pattern_after() {
  local file="$1"
  local start_line="$2"
  local pattern="$3"
  local label="$4"
  local attempts="${5:-60}"
  for _ in $(seq 1 "$attempts"); do
    if tail -n +"$((start_line + 1))" "$file" 2>/dev/null | grep -E "$pattern" >/dev/null 2>&1; then
      log "PASS: $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

post_move_pair_until() {
  local x1="$1"
  local y1="$2"
  local x2="$3"
  local y2="$4"
  local file="$5"
  local start_line="$6"
  local pattern="$7"
  local label="$8"
  local attempts="${9:-15}"
  for _ in $(seq 1 "$attempts"); do
    swift "$ROOT/scripts/ghostty-app/inject.swift" move "$x1" "$y1" >>"$HARNESS_LOG" 2>&1
    delay 0.1
    swift "$ROOT/scripts/ghostty-app/inject.swift" move "$x2" "$y2" >>"$HARNESS_LOG" 2>&1
    if tail -n +"$((start_line + 1))" "$file" 2>/dev/null | grep -E "$pattern" >/dev/null 2>&1; then
      log "PASS: $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

extract_first_match() {
  local file="$1"
  local pattern="$2"
  grep -E "$pattern" "$file" | head -1 || true
}

extract_window_id() {
  printf '%s\n' "$1" | sed -E 's/.*identity=window_id:([0-9]+).*/\1/'
}

extract_frame_x() {
  printf '%s\n' "$1" | sed -E 's/.*overlay_frame=\{\{([^,]+), [^}]+\}, \{[^}]+\}\}.*/\1/'
}

extract_frame_y() {
  printf '%s\n' "$1" | sed -E 's/.*overlay_frame=\{\{[^,]+, ([^}]+)\}, \{[^}]+\}\}.*/\1/'
}

extract_root_frame_size() {
  printf '%s\n' "$1" | sed -E 's/.*root_frame=\{\{[^}]+\}, \{([^,]+), ([^}]+)\}\}.*/\1x\2/'
}

pair_height() {
  printf '%s\n' "$1" | awk -Fx '{print $2}'
}

exact_window_bounds() {
  local window_id="$1"
  swift - "$window_id" <<'SWIFT'
import CoreGraphics
import Foundation

let target = Int(CommandLine.arguments[1])!
guard let info = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] else {
    exit(1)
}

for window in info {
    guard let id = window[kCGWindowNumber as String] as? Int, id == target else { continue }
    let bounds = (window[kCGWindowBounds as String] as? [String: Any]) ?? [:]
    let x = Int((bounds["X"] as? Double) ?? 0)
    let y = Int((bounds["Y"] as? Double) ?? 0)
    let width = Int((bounds["Width"] as? Double) ?? 0)
    let height = Int((bounds["Height"] as? Double) ?? 0)
    print("\(id)\t\(x)\t\(y)\t\(width)\t\(height)")
    exit(0)
}

exit(1)
SWIFT
}

activate_pid() {
  local pid="$1"
  local label="$2"
  local front_pid
  front_pid="$(osascript \
    -e 'tell application "System Events" to set frontmost of first process whose unix id is '"$pid"' to true' \
    -e 'delay 0.25' \
    -e 'tell application "System Events" to unix id of first process whose frontmost is true')"
  if [ "$front_pid" != "$pid" ]; then
    fail "$label frontmost PID mismatch: got=$front_pid expected=$pid"
  fi
  log "PASS: $label frontmost pid=$front_pid"
}

global_point_for_web_point() {
  local win_line="$1"
  local present_line="$2"
  local web_x="$3"
  local web_y="$4"
  local _wid wx wy _ww wh frame_x frame_y root_frame_size root_height content_y_offset
  IFS=$'\t' read -r _wid wx wy _ww wh <<<"$win_line"
  frame_x="$(extract_frame_x "$present_line")"
  frame_y="$(extract_frame_y "$present_line")"
  root_frame_size="$(extract_root_frame_size "$present_line")"
  root_height="$(pair_height "$root_frame_size")"
  content_y_offset="$(awk -v wh="$wh" -v root_h="$root_height" 'BEGIN { print int(wh - root_h) }')"
  awk \
    -v wx="$wx" \
    -v wy="$wy" \
    -v content_y="$content_y_offset" \
    -v frame_x="$frame_x" \
    -v frame_y="$frame_y" \
    -v web_x="$web_x" \
    -v web_y="$web_y" \
    'BEGIN {
      print int(wx + frame_x + web_x + 0.5) "\t" int(wy + content_y + frame_y + web_y + 0.5)
    }'
}

require_unset TERMSURF_ROAMIUM_PATH
require_unset TERMSURF_SURFARI_PATH
require_unset TERMSURF_INSTALLED_ROAMIUM_PATH
require_unset TERMSURF_INSTALLED_SURFARI_PATH
require_unset DYLD_FRAMEWORK_PATH

require_unset ASTROHACKER_WEBKIT_PATH
require_unset TERMSURF_WEBKIT_PATH
require_unset TERMSURF_SURFARI_PATH
require_executable "$APP_BIN"
require_executable "$WEB"
require_executable "$WEBKIT_HELPER"

HTTP_PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
PY
)"
URL="http://127.0.0.1:${HTTP_PORT}/index.html"

cat >"$SITE_DIR/index.html" <<'HTML'
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>Issue 26062812000871 Installed Cursor Fixture</title>
    <style>
      html,
      body {
        margin: 0;
        width: 100%;
        height: 100%;
        background: white;
        color: #111;
        font: 18px -apple-system, BlinkMacSystemFont, sans-serif;
      }

      #background {
        position: absolute;
        left: 20px;
        top: 20px;
        width: 180px;
        height: 70px;
        background: #f3f3f3;
      }

      #link {
        position: absolute;
        left: 120px;
        top: 130px;
        font-size: 24px;
      }

      #text {
        position: absolute;
        left: 120px;
        top: 205px;
        width: 420px;
        height: 42px;
        margin: 0;
        line-height: 42px;
        cursor: text;
      }
    </style>
  </head>
  <body>
    <div id="background">background</div>
    <a id="link" href="https://example.com/">Issue 26062812000871 test link</a>
    <div id="text">Selectable text should expose the text cursor.</div>
  </body>
</html>
HTML

python3 -m http.server "$HTTP_PORT" --bind 127.0.0.1 --directory "$SITE_DIR" >>"$HARNESS_LOG" 2>&1 &
HTTP_PID="$!"
for _ in $(seq 1 30); do
  if python3 - "$URL" <<'PY' >/dev/null 2>&1
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY
  then
    break
  fi
  delay 0.25
done

python3 - "$URL" <<'PY' >/dev/null 2>&1 || fail "HTTP fixture did not become ready"
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY

cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
set -euo pipefail
export TERMSURF_WEBTUI_STATE_TRACE_FILE="$WEBTUI_TRACE"
exec "$WEB" --browser webkit "$URL"
EOF
chmod +x "$COMMAND"

log "run_id=$RUN_ID"
log "version=$VERSION"
cli_version="$("$APP_BIN" +version 2>&1 | sed -n '1p')"
log "cli_version=$cli_version"
[ "$cli_version" = "Astrohacker Terminal $VERSION" ] || fail "CLI version mismatch: $cli_version"
brew_version="$(brew list --cask --versions "$CASK_TOKEN")"
log "brew_version=$brew_version"
[ "$brew_version" = "$CASK_TOKEN $VERSION" ] || fail "brew version mismatch: $brew_version"
log "started_at_epoch=$START_EPOCH"
log "app_bin=$APP_BIN"
log "web=$WEB"
log "webkit_helper=$WEBKIT_HELPER"
log "url=$URL"
log "app_log=$APP_LOG"
log "webtui_trace=$WEBTUI_TRACE"

env \
  -u TERMSURF_ROAMIUM_PATH \
  -u TERMSURF_SURFARI_PATH \
  -u TERMSURF_INSTALLED_ROAMIUM_PATH \
  -u TERMSURF_INSTALLED_SURFARI_PATH \
  -u DYLD_FRAMEWORK_PATH \
  GHOSTTY_LOG=stderr \
  TERMSURF_GEOMETRY_TRACE=1 \
  TERMSURF_GEOMETRY_SCENARIO="issue871-exp1-installed-cursor" \
  TERMSURF_INPUT_TRACE=1 \
  TERMSURF_SURFARI_CURSOR_TRACE=1 \
  "$APP_BIN" \
  --window-save-state=never \
  --confirm-close-surface=false \
  --initial-command="direct:$COMMAND" >"$APP_LOG" 2>&1 &
PID="$!"
log "pid=$PID"

START_LINE="$(line_count "$APP_LOG")"
wait_for_file_pattern_after "$APP_LOG" "$START_LINE" "SetOverlay: pane_id=.* browser=webkit url=${URL}" "web requested webkit overlay" 90
wait_for_file_pattern_after "$APP_LOG" "$START_LINE" "SetOverlay: named browser resolved browser=webkit installed_path=${WEBKIT_HELPER}" "webkit resolved to installed Homebrew binary" 90
wait_for_file_pattern_after "$APP_LOG" "$START_LINE" "browser spawn runtime env browser=webkit DYLD_FRAMEWORK_PATH=/opt/homebrew/opt/astrohacker-terminal-ah-webkitd" "Astrohacker Terminal supplied installed WebKit runtime" 90
wait_for_file_pattern_after "$APP_LOG" "$START_LINE" "spawned browser path=${WEBKIT_HELPER} .* browser=webkit " "Astrohacker Terminal spawned installed WebKit binary" 90
wait_for_file_pattern_after "$APP_LOG" "$START_LINE" "BrowserReady: pane_id=.* browser=webkit" "Astrohacker Terminal emitted webkit BrowserReady" 160
wait_for_file_pattern_after "$APP_LOG" "$START_LINE" "TermSurf geometry layer=appkit event=presented " "AppKit presented overlay" 90

BROWSER_READY_LINE="$(extract_first_match "$APP_LOG" "BrowserReady: pane_id=.* browser=webkit")"
PANE_ID="$(printf '%s\n' "$BROWSER_READY_LINE" | sed -E 's/.*pane_id=([^ ]+) tab_id=.*/\1/')"
BROWSER_TAB_ID="$(printf '%s\n' "$BROWSER_READY_LINE" | sed -E 's/.*tab_id=([0-9]+) socket=.*/\1/')"
case "$PANE_ID" in
  '' | "$BROWSER_READY_LINE") fail "could not extract pane id from BrowserReady: $BROWSER_READY_LINE" ;;
esac
case "$BROWSER_TAB_ID" in
  '' | *[!0-9]*) fail "could not extract tab id from BrowserReady: $BROWSER_READY_LINE" ;;
esac

PRESENTED_LINE="$(extract_first_match "$APP_LOG" "TermSurf geometry layer=appkit event=presented .*pane_id:${PANE_ID}")"
[ -n "$PRESENTED_LINE" ] || fail "missing AppKit presented line for pane $PANE_ID"
PRESENTED_WINDOW_ID="$(extract_window_id "$PRESENTED_LINE")"
WIN_LINE="$(exact_window_bounds "$PRESENTED_WINDOW_ID")" || fail "failed to resolve presented window bounds"
log "pane_id=$PANE_ID"
log "browser_tab_id=$BROWSER_TAB_ID"
log "presented_window_bounds=$WIN_LINE"

activate_pid "$PID" "pre-browse Astrohacker Terminal activation"
MODE_START="$(line_count "$APP_LOG")"
swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
wait_for_file_pattern_after "$APP_LOG" "$MODE_START" "ModeChanged: pane_id=${PANE_ID} browsing=true" "webtui entered Browse mode" 45
activate_pid "$PID" "post-browse Astrohacker Terminal activation"

read -r BG_X BG_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$PRESENTED_LINE" 45 45)"
read -r BG2_X BG2_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$PRESENTED_LINE" 50 48)"
read -r LINK_X LINK_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$PRESENTED_LINE" 160 145)"
read -r LINK2_X LINK2_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$PRESENTED_LINE" 170 148)"
read -r TEXT_X TEXT_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$PRESENTED_LINE" 160 218)"
read -r TEXT2_X TEXT2_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$PRESENTED_LINE" 170 220)"
log "points background=${BG_X},${BG_Y} link=${LINK_X},${LINK_Y} text=${TEXT_X},${TEXT_Y}"

BG_START="$(line_count "$APP_LOG")"
# type=0 may have already been emitted at BrowserReady; only wait if missing
if ! grep -E "CursorChanged: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} cursor_type=0" "$APP_LOG" >/dev/null 2>&1; then
  post_move_pair_until "$BG_X" "$BG_Y" "$BG2_X" "$BG2_Y" "$APP_LOG" "$BG_START" "CursorChanged: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} cursor_type=0" "background cursor changed to arrow"
fi
log "PASS: background cursor changed to arrow"
if ! grep -E "SetCursor: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} cursor_type=0" "$APP_LOG" >/dev/null 2>&1; then
  wait_for_file_pattern_after "$APP_LOG" "$BG_START" "SetCursor: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} cursor_type=0" "background cursor applied as arrow" 45
else
  log "PASS: background cursor applied as arrow"
fi

LINK_START="$(line_count "$APP_LOG")"
post_move_pair_until "$LINK_X" "$LINK_Y" "$LINK2_X" "$LINK2_Y" "$APP_LOG" "$LINK_START" "CursorChanged: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} cursor_type=2" "link cursor changed to pointing hand"
wait_for_file_pattern_after "$APP_LOG" "$LINK_START" "SetCursor: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} cursor_type=2" "link cursor applied as pointing hand" 45
wait_for_file_pattern_after "$APP_LOG" "$LINK_START" "TermSurf cursor set pane_id=${PANE_ID} cursor_type=2" "AppKit applied pointing-hand cursor" 45

TEXT_START="$(line_count "$APP_LOG")"
post_move_pair_until "$TEXT_X" "$TEXT_Y" "$TEXT2_X" "$TEXT2_Y" "$APP_LOG" "$TEXT_START" "CursorChanged: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} cursor_type=3" "text cursor changed to I-beam"
wait_for_file_pattern_after "$APP_LOG" "$TEXT_START" "SetCursor: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} cursor_type=3" "text cursor applied as I-beam" 45

RETURN_START="$(line_count "$APP_LOG")"
post_move_pair_until "$BG_X" "$BG_Y" "$BG2_X" "$BG2_Y" "$APP_LOG" "$RETURN_START" "CursorChanged: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} cursor_type=0" "cursor returned to arrow"
wait_for_file_pattern_after "$APP_LOG" "$RETURN_START" "SetCursor: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} cursor_type=0" "return cursor applied as arrow" 45

FINISH_EPOCH="$(date +%s)"
DURATION="$((FINISH_EPOCH - START_EPOCH))"
log "finished_at_epoch=$FINISH_EPOCH"
log "duration_seconds=$DURATION"
log "PASS: issue 871 installed Homebrew Surfari cursor smoke"
