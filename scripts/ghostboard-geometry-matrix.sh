#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCENARIO="${1:-initial-open}"
TS="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$ROOT/logs"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-ghostboard-geometry-${SCENARIO}.XXXXXX")"
APP="${TERMSURF_GHOSTBOARD_APP:-$ROOT/ghostboard/macos/build/Debug/TermSurf.app}"
APP_BIN="$APP/Contents/MacOS/termsurf"
WEB="${TERMSURF_WEB:-$ROOT/target/debug/web}"
ROAMIUM="${TERMSURF_ROAMIUM:-$ROOT/chromium/src/out/Default/roamium}"
URL="${TERMSURF_GEOMETRY_URL:-https://example.com}"
APP_LOG="$LOG_DIR/ghostboard-geometry-${SCENARIO}-app-${TS}.log"
HARNESS_LOG="$LOG_DIR/ghostboard-geometry-${SCENARIO}-harness-${TS}.log"
SCREENSHOT="$LOG_DIR/ghostboard-geometry-${SCENARIO}-screenshot-${TS}.png"
SCREENSHOT_GROW="$LOG_DIR/ghostboard-geometry-${SCENARIO}-grow-screenshot-${TS}.png"
SCREENSHOT_SHRINK="$LOG_DIR/ghostboard-geometry-${SCENARIO}-shrink-screenshot-${TS}.png"
SCREENSHOT_SPLIT="$LOG_DIR/ghostboard-geometry-${SCENARIO}-split-screenshot-${TS}.png"
ROAMIUM_TRACE="$LOG_DIR/ghostboard-geometry-${SCENARIO}-roamium-${TS}.log"
PID=""

mkdir -p "$LOG_DIR"

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

require_file() {
  [ -x "$1" ] || fail "missing executable: $1"
}

require_readable() {
  [ -r "$1" ] || fail "missing readable file: $1"
}

cleanup() {
  if [ -n "${PID:-}" ] && kill -0 "$PID" >/dev/null 2>&1; then
    kill "$PID" >/dev/null 2>&1 || true
    delay 0.5 || true
    kill -9 "$PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

wait_for_log() {
  local pattern="$1"
  local label="$2"
  local attempts="${3:-30}"
  for _ in $(seq 1 "$attempts"); do
    if grep -E "$pattern" "$APP_LOG" >/dev/null 2>&1; then
      log "PASS: observed $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

require_log() {
  local pattern="$1"
  local label="$2"
  if grep -E "$pattern" "$APP_LOG" >/dev/null 2>&1; then
    log "PASS: $label"
  else
    fail "missing $label"
  fi
}

require_log_after() {
  local start_line="$1"
  local pattern="$2"
  local label="$3"
  if tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "$pattern" >/dev/null 2>&1; then
    log "PASS: $label"
  else
    fail "missing $label"
  fi
}

require_trace() {
  local needle="$1"
  local label="$2"
  if grep -F "$needle" "$ROAMIUM_TRACE" >/dev/null 2>&1; then
    log "PASS: $label"
  else
    fail "missing $label"
  fi
}

trace_line_count() {
  if [ -r "$ROAMIUM_TRACE" ]; then
    wc -l <"$ROAMIUM_TRACE" | tr -d ' '
  else
    printf '0\n'
  fi
}

require_trace_after() {
  local start_line="$1"
  local needle="$2"
  local label="$3"
  if tail -n +"$((start_line + 1))" "$ROAMIUM_TRACE" | grep -F "$needle" >/dev/null 2>&1; then
    log "PASS: $label"
  else
    fail "missing $label"
  fi
}

require_text() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) log "PASS: $label" ;;
    *) fail "missing $label" ;;
  esac
}

log_line_count() {
  wc -l <"$APP_LOG" | tr -d ' '
}

extract_appkit_pixel() {
  printf '%s\n' "$1" | sed -E 's/.*appkit_pixel=([^ ]+).*/\1/'
}

extract_frame_size() {
  printf '%s\n' "$1" | sed -E 's/.*overlay_frame=\{\{[^}]+\}, \{([^,]+), ([^}]+)\}\}.*/\1x\2/'
}

extract_frame_x() {
  printf '%s\n' "$1" | sed -E 's/.*overlay_frame=\{\{([^,]+), [^}]+\}, \{[^}]+\}\}.*/\1/'
}

pair_width() {
  printf '%s\n' "$1" | awk -Fx '{print $1}'
}

pair_height() {
  printf '%s\n' "$1" | awk -Fx '{print $2}'
}

compare_pair() {
  local pair="$1"
  local ref="$2"
  local mode="$3"
  local width height ref_width ref_height
  width="$(pair_width "$pair")"
  height="$(pair_height "$pair")"
  ref_width="$(pair_width "$ref")"
  ref_height="$(pair_height "$ref")"
  awk \
    -v width="$width" \
    -v height="$height" \
    -v ref_width="$ref_width" \
    -v ref_height="$ref_height" \
    -v mode="$mode" \
    'BEGIN {
      if (mode == "gt") { exit !((width > ref_width) && (height > ref_height)) }
      if (mode == "lt") { exit !((width < ref_width) && (height < ref_height)) }
      exit 1
    }'
}

compare_split_right_pair() {
  local pair="$1"
  local ref="$2"
  local tolerance="$3"
  local width height ref_width ref_height
  width="$(pair_width "$pair")"
  height="$(pair_height "$pair")"
  ref_width="$(pair_width "$ref")"
  ref_height="$(pair_height "$ref")"
  awk \
    -v width="$width" \
    -v height="$height" \
    -v ref_width="$ref_width" \
    -v ref_height="$ref_height" \
    -v tolerance="$tolerance" \
    'BEGIN {
      delta = height - ref_height
      if (delta < 0) delta = -delta
      exit !((width < ref_width) && (delta <= tolerance))
    }'
}

wait_for_appkit_pixels_after() {
  local start_line="$1"
  local ref_pixel="$2"
  local mode="$3"
  local label="$4"
  local attempts="${5:-30}"
  local line pixel
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      pixel="$(extract_appkit_pixel "$line")"
      if [ -n "$pixel" ] && compare_pair "$pixel" "$ref_pixel" "$mode"; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E 'TermSurf geometry layer=appkit event=presented_pixels' || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_appkit_frame_after() {
  local start_line="$1"
  local ref_frame="$2"
  local mode="$3"
  local label="$4"
  local attempts="${5:-30}"
  local line frame_size
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      frame_size="$(extract_frame_size "$line")"
      if [ -n "$frame_size" ] && [ "$frame_size" != "$line" ] && compare_pair "$frame_size" "$ref_frame" "$mode"; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E 'TermSurf geometry layer=appkit event=presented ' || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_split_right_frame_after() {
  local start_line="$1"
  local pane_id="$2"
  local context_id="$3"
  local ref_frame="$4"
  local label="$5"
  local attempts="${6:-30}"
  local line frame_size
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      frame_size="$(extract_frame_size "$line")"
      if [ -n "$frame_size" ] && [ "$frame_size" != "$line" ] && compare_split_right_pair "$frame_size" "$ref_frame" 8; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_split_right_pixels_after() {
  local start_line="$1"
  local pane_id="$2"
  local context_id="$3"
  local ref_pixel="$4"
  local label="$5"
  local attempts="${6:-30}"
  local line pixel
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      pixel="$(extract_appkit_pixel "$line")"
      if [ -n "$pixel" ] && compare_split_right_pair "$pixel" "$ref_pixel" 16; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${pane_id} .*context_id=${context_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_hit_after() {
  local start_line="$1"
  local context_id="$2"
  local label="$3"
  local attempts="${4:-30}"
  local line
  for _ in $(seq 1 "$attempts"); do
    line="$(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=hit_test .*context_id=${context_id} .*hit=true .*web_point=\\{" | tail -1 || true)"
    if [ -n "$line" ]; then
      printf '%s\n' "$line"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_negative_hit_after() {
  local start_line="$1"
  local context_id="$2"
  local label="$3"
  delay 1
  if tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=hit_test .*context_id=${context_id} .*hit=true" >/dev/null 2>&1; then
    fail "$label routed to original browser context"
  fi
  log "PASS: $label did not route to original browser context"
}

window_bounds() {
  swift "$WINDOW_BOUNDS" "$WID"
}

set_window_size() {
  local width="$1"
  local height="$2"
  swift "$RESIZE_WINDOW" "$PID" 40 80 "$width" "$height"
}

click_window_center() {
  local bounds="$1"
  local label="$2"
  local bid bx by bw bh click_x click_y
  IFS=$'\t' read -r bid bx by bw bh <<<"$bounds"
  click_x=$((bx + bw / 2))
  click_y=$((by + bh / 2))
  log "${label}_input_point=${click_x},${click_y}"
  swift "$ROOT/scripts/ghostty-app/inject.swift" move "$click_x" "$click_y" >>"$HARNESS_LOG" 2>&1
  delay 0.25
  swift "$ROOT/scripts/ghostty-app/inject.swift" click "$click_x" "$click_y" left 1 >>"$HARNESS_LOG" 2>&1
}

click_global_point() {
  local x="$1"
  local y="$2"
  local label="$3"
  log "${label}_input_point=${x},${y}"
  swift "$ROOT/scripts/ghostty-app/inject.swift" move "$x" "$y" >>"$HARNESS_LOG" 2>&1
  delay 0.25
  swift "$ROOT/scripts/ghostty-app/inject.swift" click "$x" "$y" left 1 >>"$HARNESS_LOG" 2>&1
}

case "$SCENARIO" in
  initial-open|window-resize|split-right) ;;
  *)
    fail "unsupported scenario: $SCENARIO"
    ;;
esac

require_file "$APP_BIN"
require_file "$WEB"
require_file "$ROAMIUM"
require_readable "$ROOT/scripts/ghostty-app/inject.swift"
require_readable "$ROOT/scripts/ghostty-app/winid.swift"

COMMAND="$RUN_DIR/run-web.sh"
CONFIG="$RUN_DIR/config"
WINDOW_BOUNDS="$RUN_DIR/window-bounds.swift"
ACTIVATE_APP="$RUN_DIR/activate-app.swift"
RESIZE_WINDOW="$RUN_DIR/resize-window.swift"
cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
exec "$WEB" --browser "$ROAMIUM" "$URL"
EOF
chmod +x "$COMMAND"

cat >"$CONFIG" <<EOF
initial-command = direct:$COMMAND
EOF

if [ "$SCENARIO" = "split-right" ]; then
  cat >>"$CONFIG" <<'EOF'
keybind = ctrl+d=new_split:right
EOF
fi

cat >"$WINDOW_BOUNDS" <<'EOF'
import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 2,
      let target = Int(CommandLine.arguments[1]),
      let info = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]]
else {
    exit(2)
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
EOF

cat >"$ACTIVATE_APP" <<'EOF'
import AppKit
import Foundation

guard CommandLine.arguments.count == 2,
      let rawPID = Int32(CommandLine.arguments[1]),
      let app = NSRunningApplication(processIdentifier: pid_t(rawPID))
else {
    exit(2)
}

app.activate(options: [.activateAllWindows, .activateIgnoringOtherApps])
Thread.sleep(forTimeInterval: 0.5)
EOF

cat >"$RESIZE_WINDOW" <<'EOF'
import ApplicationServices
import Foundation

guard CommandLine.arguments.count == 6,
      let rawPID = Int32(CommandLine.arguments[1]),
      let x = Double(CommandLine.arguments[2]),
      let y = Double(CommandLine.arguments[3]),
      let width = Double(CommandLine.arguments[4]),
      let height = Double(CommandLine.arguments[5])
else {
    fputs("usage: resize-window.swift <pid> <x> <y> <width> <height>\n", stderr)
    exit(2)
}

guard AXIsProcessTrusted() else {
    fputs("accessibility permission is not trusted for window resize automation\n", stderr)
    exit(3)
}

let app = AXUIElementCreateApplication(pid_t(rawPID))
var windowsValue: CFTypeRef?
let windowsResult = AXUIElementCopyAttributeValue(
    app,
    kAXWindowsAttribute as CFString,
    &windowsValue
)

guard windowsResult == .success,
      let windows = windowsValue as? [AXUIElement],
      let window = windows.first
else {
    fputs("could not read target app windows: \(windowsResult.rawValue)\n", stderr)
    exit(4)
}

var position = CGPoint(x: x, y: y)
var size = CGSize(width: width, height: height)

guard let positionValue = AXValueCreate(.cgPoint, &position),
      let sizeValue = AXValueCreate(.cgSize, &size)
else {
    fputs("could not create accessibility values\n", stderr)
    exit(5)
}

let positionResult = AXUIElementSetAttributeValue(
    window,
    kAXPositionAttribute as CFString,
    positionValue
)
let sizeResult = AXUIElementSetAttributeValue(
    window,
    kAXSizeAttribute as CFString,
    sizeValue
)

guard positionResult == .success, sizeResult == .success else {
    fputs("resize failed position=\(positionResult.rawValue) size=\(sizeResult.rawValue)\n", stderr)
    exit(6)
}

Thread.sleep(forTimeInterval: 0.5)
EOF

log "scenario=$SCENARIO"
log "run_dir=$RUN_DIR"
log "app=$APP"
log "web=$WEB"
log "roamium=$ROAMIUM"
log "url=$URL"
log "app_log=$APP_LOG"
log "roamium_trace=$ROAMIUM_TRACE"
log "screenshot=$SCREENSHOT"
if [ "$SCENARIO" = "window-resize" ]; then
  log "grow_screenshot=$SCREENSHOT_GROW"
  log "shrink_screenshot=$SCREENSHOT_SHRINK"
fi
if [ "$SCENARIO" = "split-right" ]; then
  log "split_screenshot=$SCREENSHOT_SPLIT"
fi

GHOSTTY_CONFIG_PATH="$CONFIG" \
GHOSTTY_LOG=stderr \
TERMSURF_GEOMETRY_TRACE=1 \
TERMSURF_GEOMETRY_SCENARIO="$SCENARIO" \
TERMSURF_INPUT_TRACE=1 \
TERMSURF_PDF_INPUT_TRACE=1 \
TERMSURF_PDF_INPUT_TRACE_FILE="$ROAMIUM_TRACE" \
  "$APP_BIN" >"$APP_LOG" 2>&1 &
PID="$!"
log "pid=$PID"

wait_for_log 'TermSurf geometry layer=appkit event=presented ' "AppKit overlay presentation"

PRESENTED_LINE="$(grep -E 'TermSurf geometry layer=appkit event=presented ' "$APP_LOG" | tail -1)"
WID="$(printf '%s\n' "$PRESENTED_LINE" | sed -E 's/.*identity=window_id:([^ ]+) .*/\1/')"
case "$WID" in
  ''|*[!0-9]*) fail "failed to extract numeric AppKit window id from presented geometry: $PRESENTED_LINE" ;;
esac
log "presented_window_id=$WID"

swift "$ACTIVATE_APP" "$PID" >>"$HARNESS_LOG" 2>&1 || fail "failed to activate pid=$PID"
delay 0.5

WIN_LINE="$(window_bounds)" || fail "failed to resolve bounds for window id=$WID"
IFS=$'\t' read -r WID WX WY WW WH <<<"$WIN_LINE"
log "window=$WIN_LINE"

screencapture -x -o -l"$WID" "$SCREENSHOT"
log "screenshot_exit=$?"

click_window_center "$WIN_LINE" "initial"
delay 1

require_log 'TermSurf geometry layer=zig' "Zig geometry record"
require_log 'TermSurf geometry layer=bridge' "bridge geometry record"
require_log 'TermSurf geometry layer=appkit event=presented ' "AppKit presented geometry record"
require_log 'TermSurf geometry layer=appkit event=hit_test .*hit=true' "AppKit hit-test geometry record"
require_log "scenario=${SCENARIO}" "scenario id in geometry records"

TAB_READY_LINE="$(grep -E 'TermSurf geometry layer=zig event=tab_ready' "$APP_LOG" | tail -1)"
CA_CONTEXT_LINE="$(grep -E 'TermSurf geometry layer=zig event=ca_context' "$APP_LOG" | tail -1)"
ZIG_PRESENT_LINE="$(grep -E 'TermSurf geometry layer=zig event=present_overlay_call' "$APP_LOG" | tail -1)"
BRIDGE_PRESENT_LINE="$(grep -E 'TermSurf geometry layer=bridge event=present_target_found' "$APP_LOG" | tail -1)"
APPKIT_PRESENT_LINE="$(grep -E 'TermSurf geometry layer=appkit event=presented ' "$APP_LOG" | tail -1)"
APPKIT_PIXELS_LINE="$(grep -E 'TermSurf geometry layer=appkit event=presented_pixels' "$APP_LOG" | tail -1)"
HIT_TEST_LINE="$(grep -E 'TermSurf geometry layer=appkit event=hit_test .*hit=true' "$APP_LOG" | tail -1)"

[ -n "$TAB_READY_LINE" ] || fail "missing Zig tab_ready geometry line"
[ -n "$CA_CONTEXT_LINE" ] || fail "missing Zig ca_context geometry line"
[ -n "$ZIG_PRESENT_LINE" ] || fail "missing Zig present_overlay_call geometry line"
[ -n "$BRIDGE_PRESENT_LINE" ] || fail "missing bridge present_target_found geometry line"
[ -n "$APPKIT_PRESENT_LINE" ] || fail "missing AppKit presented geometry line"
[ -n "$APPKIT_PIXELS_LINE" ] || fail "missing AppKit presented-pixels geometry line"
[ -n "$HIT_TEST_LINE" ] || fail "missing AppKit hit-test geometry line"

PANE_ID="$(printf '%s\n' "$CA_CONTEXT_LINE" | sed -E 's/.*pane_id:([^ ]+).*/\1/')"
[ -n "$PANE_ID" ] || fail "could not extract pane id"
BROWSER_TAB_ID="$(printf '%s\n' "$CA_CONTEXT_LINE" | sed -E 's/.*browser_tab_id:([^ ]+).*/\1/')"
case "$BROWSER_TAB_ID" in
  ''|unknown:*) fail "could not extract concrete browser tab id from Zig ca_context" ;;
esac
CONTEXT_ID="$(printf '%s\n' "$ZIG_PRESENT_LINE" | sed -E 's/.*context_id=([0-9]+).*/\1/')"
[ -n "$CONTEXT_ID" ] || fail "could not extract context id"
GRID="$(printf '%s\n' "$ZIG_PRESENT_LINE" | sed -E 's/.*grid=([^ ]+).*/\1/')"
[ -n "$GRID" ] || fail "could not extract Zig overlay grid"
BROWSER_PIXEL="$(printf '%s\n' "$ZIG_PRESENT_LINE" | sed -E 's/.*browser_pixel=([^ ]+).*/\1/')"
[ -n "$BROWSER_PIXEL" ] || fail "could not extract Zig browser pixel size"
OVERLAY_FRAME="$(printf '%s\n' "$APPKIT_PRESENT_LINE" | sed -E 's/.*overlay_frame=([^ ]+ [^ ]+ [^ ]+ [^ ]+) root_frame=.*/\1/')"
[ -n "$OVERLAY_FRAME" ] && [ "$OVERLAY_FRAME" != "none" ] || fail "could not extract AppKit overlay frame"
OVERLAY_FRAME_SIZE="$(extract_frame_size "$APPKIT_PRESENT_LINE")"
[ -n "$OVERLAY_FRAME_SIZE" ] && [ "$OVERLAY_FRAME_SIZE" != "$APPKIT_PRESENT_LINE" ] || fail "could not extract AppKit overlay frame size"
OVERLAY_FRAME_X="$(extract_frame_x "$APPKIT_PRESENT_LINE")"
[ -n "$OVERLAY_FRAME_X" ] && [ "$OVERLAY_FRAME_X" != "$APPKIT_PRESENT_LINE" ] || fail "could not extract AppKit overlay frame x"
APPKIT_PIXEL="$(printf '%s\n' "$APPKIT_PIXELS_LINE" | sed -E 's/.*appkit_pixel=([^ ]+).*/\1/')"
[ -n "$APPKIT_PIXEL" ] || fail "could not extract AppKit presented pixel size"
APPKIT_PIXEL_WIDTH="${APPKIT_PIXEL%x*}"
APPKIT_PIXEL_HEIGHT="${APPKIT_PIXEL#*x}"

log "correlation_pane_id=$PANE_ID"
log "correlation_browser_tab_id=$BROWSER_TAB_ID"
log "correlation_context_id=$CONTEXT_ID"
log "correlation_grid=$GRID"
log "correlation_browser_pixel=$BROWSER_PIXEL"
log "correlation_overlay_frame=$OVERLAY_FRAME"
log "correlation_overlay_frame_size=$OVERLAY_FRAME_SIZE"
log "correlation_overlay_frame_x=$OVERLAY_FRAME_X"
log "correlation_appkit_pixel=$APPKIT_PIXEL"
log "correlation_scenario=$SCENARIO"
log "correlation_timestamp=$TS"
log "correlation_app_log=$APP_LOG"
log "correlation_harness_log=$HARNESS_LOG"
log "correlation_screenshot=$SCREENSHOT"

require_text "$TAB_READY_LINE" "pane_id:${PANE_ID}" "Zig tab_ready shares pane id"
require_text "$TAB_READY_LINE" "browser_tab_id:${BROWSER_TAB_ID}" "Zig tab_ready shares browser tab id"
require_text "$CA_CONTEXT_LINE" "pane_id:${PANE_ID}" "Zig ca_context shares pane id"
require_text "$CA_CONTEXT_LINE" "browser_tab_id:${BROWSER_TAB_ID}" "Zig ca_context shares browser tab id"
require_text "$CA_CONTEXT_LINE" "grid=${GRID}" "Zig ca_context shares grid"
require_text "$CA_CONTEXT_LINE" "browser_pixel=${BROWSER_PIXEL}" "Zig ca_context shares browser pixel"
require_text "$CA_CONTEXT_LINE" "context_id=${CONTEXT_ID}" "Zig ca_context shares context"
require_log "TermSurf geometry layer=bridge .*pane_id:${PANE_ID}" "bridge shares pane id"
require_log "TermSurf geometry layer=appkit .*pane_id:${PANE_ID}" "AppKit shares pane id"
require_text "$BRIDGE_PRESENT_LINE" "grid=${GRID}" "bridge shares grid"
require_text "$BRIDGE_PRESENT_LINE" "browser_pixel=${BROWSER_PIXEL}" "bridge shares browser pixel"
require_text "$BRIDGE_PRESENT_LINE" "context_id=${CONTEXT_ID}" "bridge shares context"
require_text "$APPKIT_PRESENT_LINE" "grid=${GRID}" "AppKit shares grid"
require_text "$APPKIT_PRESENT_LINE" "browser_pixel=${BROWSER_PIXEL}" "AppKit shares browser pixel"
require_text "$APPKIT_PRESENT_LINE" "context_id=${CONTEXT_ID}" "AppKit shares context"
require_text "$APPKIT_PIXELS_LINE" "appkit_pixel=${APPKIT_PIXEL}" "AppKit reports presented pixel size"
require_log "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${APPKIT_PIXEL}" "Zig records AppKit presented pixel size"
require_log "TermSurf geometry layer=zig event=appkit_corrective_resize .*pane_id:${PANE_ID} .*appkit_pixel=${APPKIT_PIXEL}" "Zig sends corrective resize for AppKit pixel size"
require_log "TermSurf geometry layer=appkit .*context_id=${CONTEXT_ID}" "AppKit shares context id"
require_text "$HIT_TEST_LINE" "context_id=${CONTEXT_ID}" "hit-test shares context"
require_text "$HIT_TEST_LINE" "hit=true" "hit-test is inside overlay"
require_text "$HIT_TEST_LINE" "web_point={" "hit-test includes webview-relative point"
require_log "TermSurf geometry .*scenario=${SCENARIO}" "timestamped run contains scenario id"
require_log 'window_id:[^ ]+ surface_id:[^ ]+ selected_tab_id:[^ ]+ pane_id:[^ ]+ browser_tab_id:[^ ]+' "canonical identity tuple fields"
require_readable "$ROAMIUM_TRACE"
require_trace "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${APPKIT_PIXEL_WIDTH} pixel_height=${APPKIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Roamium applied resize to AppKit pixel size via ts_set_view_size"

if [ "$SCENARIO" = "window-resize" ]; then
  INITIAL_PIXEL="$APPKIT_PIXEL"
  INITIAL_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  INITIAL_WW="$WW"
  INITIAL_WH="$WH"

  GROW_WIDTH=$((INITIAL_WW + 320))
  GROW_HEIGHT=$((INITIAL_WH + 220))
  log "resize_grow_target=${GROW_WIDTH}x${GROW_HEIGHT}"
  GROW_START_LINE="$(log_line_count)"
  set_window_size "$GROW_WIDTH" "$GROW_HEIGHT" >>"$HARNESS_LOG" 2>&1 || fail "failed to grow window via System Events"
  delay 1
  GROW_WIN_LINE="$(window_bounds)" || fail "failed to resolve grown window bounds for window id=$WID"
  log "grow_window=$GROW_WIN_LINE"
  GROW_PRESENT_LINE="$(wait_for_appkit_frame_after "$GROW_START_LINE" "$INITIAL_FRAME_SIZE" gt "grown AppKit overlay frame")"
  GROW_PIXELS_LINE="$(wait_for_appkit_pixels_after "$GROW_START_LINE" "$INITIAL_PIXEL" gt "grown AppKit presented pixels")"
  GROW_FRAME_SIZE="$(extract_frame_size "$GROW_PRESENT_LINE")"
  GROW_PIXEL="$(extract_appkit_pixel "$GROW_PIXELS_LINE")"
  GROW_PIXEL_WIDTH="${GROW_PIXEL%x*}"
  GROW_PIXEL_HEIGHT="${GROW_PIXEL#*x}"
  log "PASS: observed grown AppKit overlay frame overlay_frame_size=$GROW_FRAME_SIZE"
  log "PASS: observed grown AppKit presented pixels appkit_pixel=$GROW_PIXEL"
  log "grow_overlay_frame_size=$GROW_FRAME_SIZE"
  log "grow_appkit_pixel=$GROW_PIXEL"
  require_log_after "$GROW_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${GROW_PIXEL}" "Zig records grown AppKit presented pixel size"
  require_trace "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${GROW_PIXEL_WIDTH} pixel_height=${GROW_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Roamium applied grow resize to AppKit pixel size via ts_set_view_size"
  screencapture -x -o -l"$WID" "$SCREENSHOT_GROW"
  log "grow_screenshot_exit=$?"
  GROW_HIT_START_LINE="$(log_line_count)"
  click_window_center "$GROW_WIN_LINE" "grow"
  GROW_HIT_LINE="$(wait_for_hit_after "$GROW_HIT_START_LINE" "$CONTEXT_ID" "grown AppKit hit-test")"
  log "PASS: observed grown AppKit hit-test"
  require_text "$GROW_HIT_LINE" "overlay_frame=" "grown hit-test includes current overlay frame"
  require_text "$GROW_HIT_LINE" "web_point={" "grown hit-test includes webview-relative point"

  SHRINK_WIDTH=$((INITIAL_WW + 80))
  SHRINK_HEIGHT=$((INITIAL_WH + 60))
  log "resize_shrink_target=${SHRINK_WIDTH}x${SHRINK_HEIGHT}"
  SHRINK_START_LINE="$(log_line_count)"
  set_window_size "$SHRINK_WIDTH" "$SHRINK_HEIGHT" >>"$HARNESS_LOG" 2>&1 || fail "failed to shrink window via System Events"
  delay 1
  SHRINK_WIN_LINE="$(window_bounds)" || fail "failed to resolve shrunken window bounds for window id=$WID"
  log "shrink_window=$SHRINK_WIN_LINE"
  SHRINK_PRESENT_LINE="$(wait_for_appkit_frame_after "$SHRINK_START_LINE" "$GROW_FRAME_SIZE" lt "shrunken AppKit overlay frame")"
  SHRINK_PIXELS_LINE="$(wait_for_appkit_pixels_after "$SHRINK_START_LINE" "$GROW_PIXEL" lt "shrunken AppKit presented pixels")"
  SHRINK_FRAME_SIZE="$(extract_frame_size "$SHRINK_PRESENT_LINE")"
  SHRINK_PIXEL="$(extract_appkit_pixel "$SHRINK_PIXELS_LINE")"
  SHRINK_PIXEL_WIDTH="${SHRINK_PIXEL%x*}"
  SHRINK_PIXEL_HEIGHT="${SHRINK_PIXEL#*x}"
  log "PASS: observed shrunken AppKit overlay frame overlay_frame_size=$SHRINK_FRAME_SIZE"
  log "PASS: observed shrunken AppKit presented pixels appkit_pixel=$SHRINK_PIXEL"
  log "shrink_overlay_frame_size=$SHRINK_FRAME_SIZE"
  log "shrink_appkit_pixel=$SHRINK_PIXEL"
  require_log_after "$SHRINK_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${SHRINK_PIXEL}" "Zig records shrunken AppKit presented pixel size"
  require_trace "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${SHRINK_PIXEL_WIDTH} pixel_height=${SHRINK_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Roamium applied shrink resize to AppKit pixel size via ts_set_view_size"
  screencapture -x -o -l"$WID" "$SCREENSHOT_SHRINK"
  log "shrink_screenshot_exit=$?"
  SHRINK_HIT_START_LINE="$(log_line_count)"
  click_window_center "$SHRINK_WIN_LINE" "shrink"
  SHRINK_HIT_LINE="$(wait_for_hit_after "$SHRINK_HIT_START_LINE" "$CONTEXT_ID" "shrunken AppKit hit-test")"
  log "PASS: observed shrunken AppKit hit-test"
  require_text "$SHRINK_HIT_LINE" "overlay_frame=" "shrunken hit-test includes current overlay frame"
  require_text "$SHRINK_HIT_LINE" "web_point={" "shrunken hit-test includes webview-relative point"
fi

if [ "$SCENARIO" = "split-right" ]; then
  SPLIT_START_LINE="$(log_line_count)"
  SPLIT_TRACE_START_LINE="$(trace_line_count)"
  log "split_keybind=ctrl+d=new_split:right"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 2 control >>"$HARNESS_LOG" 2>&1
  delay 1

  SPLIT_PRESENT_LINE="$(wait_for_split_right_frame_after "$SPLIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$OVERLAY_FRAME_SIZE" "split-right AppKit overlay frame")"
  SPLIT_PIXELS_LINE="$(wait_for_split_right_pixels_after "$SPLIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$APPKIT_PIXEL" "split-right AppKit presented pixels")"
  SPLIT_FRAME_SIZE="$(extract_frame_size "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_X="$(extract_frame_x "$SPLIT_PRESENT_LINE")"
  SPLIT_PIXEL="$(extract_appkit_pixel "$SPLIT_PIXELS_LINE")"
  SPLIT_PIXEL_WIDTH="${SPLIT_PIXEL%x*}"
  SPLIT_PIXEL_HEIGHT="${SPLIT_PIXEL#*x}"
  log "PASS: observed split-right AppKit overlay frame overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "PASS: observed split-right AppKit presented pixels appkit_pixel=$SPLIT_PIXEL"
  log "split_overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "split_overlay_frame_x=$SPLIT_FRAME_X"
  log "split_appkit_pixel=$SPLIT_PIXEL"
  require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${SPLIT_PIXEL}" "Zig records split-right AppKit presented pixel size"
  require_trace_after "$SPLIT_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${SPLIT_PIXEL_WIDTH} pixel_height=${SPLIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Roamium applied split-right resize to AppKit pixel size via ts_set_view_size"
  screencapture -x -o -l"$WID" "$SCREENSHOT_SPLIT"
  log "split_screenshot_exit=$?"

  SPLIT_WIN_LINE="$(window_bounds)" || fail "failed to resolve split window bounds for window id=$WID"
  IFS=$'\t' read -r _SPLIT_WID SPLIT_WX SPLIT_WY SPLIT_WW SPLIT_WH <<<"$SPLIT_WIN_LINE"
  SPLIT_FRAME_WIDTH="$(pair_width "$SPLIT_FRAME_SIZE")"
  INITIAL_FRAME_WIDTH="$(pair_width "$OVERLAY_FRAME_SIZE")"
  SPLIT_INSIDE_X="$(awk -v wx="$SPLIT_WX" -v frame_x="$SPLIT_FRAME_X" -v frame_w="$SPLIT_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  SPLIT_INSIDE_Y=$((SPLIT_WY + SPLIT_WH / 2))
  SPLIT_HIT_START_LINE="$(log_line_count)"
  click_global_point "$SPLIT_INSIDE_X" "$SPLIT_INSIDE_Y" "split_inside"
  SPLIT_HIT_LINE="$(wait_for_hit_after "$SPLIT_HIT_START_LINE" "$CONTEXT_ID" "split-right AppKit hit-test")"
  log "PASS: observed split-right AppKit hit-test"
  require_text "$SPLIT_HIT_LINE" "overlay_frame=" "split-right hit-test includes current overlay frame"
  require_text "$SPLIT_HIT_LINE" "web_point={" "split-right hit-test includes webview-relative point"

  SPLIT_NEGATIVE_X="$(awk -v wx="$SPLIT_WX" -v frame_x="$SPLIT_FRAME_X" -v frame_w="$SPLIT_FRAME_WIDTH" -v old_w="$INITIAL_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + frame_w + ((old_w - frame_w) / 2) + 0.5) }')"
  SPLIT_NEGATIVE_Y="$SPLIT_INSIDE_Y"
  SPLIT_NEGATIVE_START_LINE="$(log_line_count)"
  click_global_point "$SPLIT_NEGATIVE_X" "$SPLIT_NEGATIVE_Y" "split_sibling_negative"
  wait_for_negative_hit_after "$SPLIT_NEGATIVE_START_LINE" "$CONTEXT_ID" "split-right sibling-pane negative hit-test"
fi

log "PASS: scenario $SCENARIO"
