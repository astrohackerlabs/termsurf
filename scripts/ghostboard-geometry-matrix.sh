#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCENARIO="${1:-initial-open}"
TS="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$ROOT/logs"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-ghostboard-geometry-${SCENARIO}.XXXXXX")"
APP="${TERMSURF_GHOSTBOARD_APP:-$ROOT/forks/ghostty/macos/build/Debug/Astrohacker Terminal.app}"
if [ "$SCENARIO" = "installed-chromium-release-launch" ] && [ -z "${TERMSURF_GHOSTBOARD_APP:-}" ]; then
  APP="$ROOT/forks/ghostty/macos/build/Release/Astrohacker Terminal.app"
fi
APP_BIN="$APP/Contents/MacOS/ahterm"
WEB="${TERMSURF_WEB:-$ROOT/rust/target/debug/ahweb}"
CHROMIUM="${ASTROHACKER_CHROMIUM:-$ROOT/forks/chromium/src/out/Default/ah-chromiumd}"
INSTALLED_CHROMIUM="${TERMSURF_INSTALLED_CHROMIUM:-$CHROMIUM}"
CHROMIUM_PATH_FOR_APP="$CHROMIUM"
POINTER_DRIVER="${TERMSURF_GEOMETRY_POINTER_DRIVER:-cgevent}"
URL="${TERMSURF_GEOMETRY_URL:-https://example.com}"
BORDER_CONFIG_CASE="${TERMSURF_SPLIT_BORDER_CONFIG_CASE:-enabled}"
HELLO_CONFIG_HOMEPAGE="${TERMSURF_HELLO_CONFIG_HOMEPAGE:-https://example.net/issue-26061712000815-homepage}"
HELLO_CONFIG_SECOND_BROWSER="${TERMSURF_HELLO_CONFIG_SECOND_BROWSER:-webkit}"
URL_B="${TERMSURF_GEOMETRY_SECOND_URL:-https://example.org}"
URL_C="${TERMSURF_GEOMETRY_THIRD_URL:-https://example.net}"
APP_LOG="$LOG_DIR/ghostboard-geometry-${SCENARIO}-app-${TS}.log"
HARNESS_LOG="$LOG_DIR/ghostboard-geometry-${SCENARIO}-harness-${TS}.log"
SCREENSHOT="$LOG_DIR/ghostboard-geometry-${SCENARIO}-screenshot-${TS}.png"
SCREENSHOT_GROW="$LOG_DIR/ghostboard-geometry-${SCENARIO}-grow-screenshot-${TS}.png"
SCREENSHOT_SHRINK="$LOG_DIR/ghostboard-geometry-${SCENARIO}-shrink-screenshot-${TS}.png"
SCREENSHOT_SPLIT="$LOG_DIR/ghostboard-geometry-${SCENARIO}-split-screenshot-${TS}.png"
SCREENSHOT_ZOOM="$LOG_DIR/ghostboard-geometry-${SCENARIO}-zoom-screenshot-${TS}.png"
SCREENSHOT_UNZOOM="$LOG_DIR/ghostboard-geometry-${SCENARIO}-unzoom-screenshot-${TS}.png"
SCREENSHOT_CLOSE="$LOG_DIR/ghostboard-geometry-${SCENARIO}-close-screenshot-${TS}.png"
SCREENSHOT_TAB_NEW="$LOG_DIR/ghostboard-geometry-${SCENARIO}-new-tab-screenshot-${TS}.png"
SCREENSHOT_TAB_BACK="$LOG_DIR/ghostboard-geometry-${SCENARIO}-back-tab-screenshot-${TS}.png"
SCREENSHOT_TAB_BROWSER_B="$LOG_DIR/ghostboard-geometry-${SCENARIO}-browser-b-screenshot-${TS}.png"
SCREENSHOT_TAB_BROWSER_A_RESTORED="$LOG_DIR/ghostboard-geometry-${SCENARIO}-browser-a-restored-screenshot-${TS}.png"
SCREENSHOT_TAB_BROWSER_B_RESTORED="$LOG_DIR/ghostboard-geometry-${SCENARIO}-browser-b-restored-screenshot-${TS}.png"
SCREENSHOT_TAB_AFTER_CLOSE="$LOG_DIR/ghostboard-geometry-${SCENARIO}-after-close-screenshot-${TS}.png"
SCREENSHOT_WINDOW_B="$LOG_DIR/ghostboard-geometry-${SCENARIO}-window-b-screenshot-${TS}.png"
SCREENSHOT_WINDOW_C="$LOG_DIR/ghostboard-geometry-${SCENARIO}-window-c-screenshot-${TS}.png"
SCREENSHOT_WINDOW_A_RESTORED="$LOG_DIR/ghostboard-geometry-${SCENARIO}-window-a-restored-screenshot-${TS}.png"
SCREENSHOT_WINDOW_B_RESTORED="$LOG_DIR/ghostboard-geometry-${SCENARIO}-window-b-restored-screenshot-${TS}.png"
SCREENSHOT_DISPLAY_MOVED="$LOG_DIR/ghostboard-geometry-${SCENARIO}-moved-screenshot-${TS}.png"
SCREENSHOT_DISPLAY_RETURNED="$LOG_DIR/ghostboard-geometry-${SCENARIO}-returned-screenshot-${TS}.png"
SCREENSHOT_FULLSCREEN="$LOG_DIR/ghostboard-geometry-${SCENARIO}-fullscreen-screenshot-${TS}.png"
SCREENSHOT_UNFULLSCREEN="$LOG_DIR/ghostboard-geometry-${SCENARIO}-unfullscreen-screenshot-${TS}.png"
SCREENSHOT_MINIMIZE_RESTORED="$LOG_DIR/ghostboard-geometry-${SCENARIO}-minimize-restored-screenshot-${TS}.png"
SCREENSHOT_HIDE_RESTORED="$LOG_DIR/ghostboard-geometry-${SCENARIO}-hide-restored-screenshot-${TS}.png"
SCREENSHOT_FONT_INCREASE="$LOG_DIR/ghostboard-geometry-${SCENARIO}-font-increase-screenshot-${TS}.png"
SCREENSHOT_FONT_DECREASE="$LOG_DIR/ghostboard-geometry-${SCENARIO}-font-decrease-screenshot-${TS}.png"
SCREENSHOT_TUI_SHRINK="$LOG_DIR/ghostboard-geometry-${SCENARIO}-tui-shrink-screenshot-${TS}.png"
SCREENSHOT_TUI_RESET="$LOG_DIR/ghostboard-geometry-${SCENARIO}-tui-reset-screenshot-${TS}.png"
SCREENSHOT_SCROLLBACK_UP="$LOG_DIR/ghostboard-geometry-${SCENARIO}-scrollback-up-screenshot-${TS}.png"
SCREENSHOT_SCROLLBACK_BOTTOM="$LOG_DIR/ghostboard-geometry-${SCENARIO}-scrollback-bottom-screenshot-${TS}.png"
SCREENSHOT_NAVIGATED="$LOG_DIR/ghostboard-geometry-${SCENARIO}-navigated-screenshot-${TS}.png"
SCREENSHOT_DEVTOOLS_SPLIT="$LOG_DIR/ghostboard-geometry-${SCENARIO}-devtools-split-screenshot-${TS}.png"
SCREENSHOT_BROWSER_STATE_HOVER="$LOG_DIR/ghostboard-geometry-${SCENARIO}-hover-screenshot-${TS}.png"
SCREENSHOT_BROWSER_STATE_RELOAD="$LOG_DIR/ghostboard-geometry-${SCENARIO}-reload-screenshot-${TS}.png"
SCREENSHOT_BROWSER_STATE_BLANK="$LOG_DIR/ghostboard-geometry-${SCENARIO}-blank-screenshot-${TS}.png"
CHROMIUM_TRACE="$LOG_DIR/ghostboard-geometry-${SCENARIO}-chromium-${TS}.log"
WEBTUI_STATE_TRACE="$LOG_DIR/ghostboard-geometry-${SCENARIO}-webtui-${TS}.log"
SIBLING_ALIVE_COMMAND="$RUN_DIR/sibling-alive-command.txt"
SIBLING_FOCUS_COMMAND="$RUN_DIR/sibling-focus-command.txt"
BROWSER_FOCUS_COMMAND="$RUN_DIR/browser-focus-command.txt"
TUI_VIEWPORT_SHRINK_COMMAND="$RUN_DIR/tui-viewport-shrink-command.txt"
TUI_VIEWPORT_RESET_COMMAND="$RUN_DIR/tui-viewport-reset-command.txt"
NAVIGATION_APPEND_COMMAND="$RUN_DIR/navigation-append-command.txt"
DEVTOOLS_COMMAND="$RUN_DIR/devtools-command.txt"
DEVTOOLS_QUERY_PROBE="$RUN_DIR/devtools-query-probe.py"
DEVTOOLS_OVERLAY_PROBE="$RUN_DIR/devtools-overlay-probe.py"
STATE_WEB_ROOT="$RUN_DIR/browser-state-site"
DIALOG_WEB_ROOT="$RUN_DIR/javascript-dialog-site"
DIALOG_TYPE_COMMAND="$RUN_DIR/dialog-type-command.txt"
AUTH_WEB_ROOT="$RUN_DIR/http-auth-site"
AUTH_TYPE_COMMAND="$RUN_DIR/http-auth-type-command.txt"
CRASH_WEB_ROOT="$RUN_DIR/renderer-crash-site"
CRASH_TYPE_COMMAND="$RUN_DIR/renderer-crash-type-command.txt"
COLOR_WEB_ROOT="$RUN_DIR/color-scheme-site"
COLOR_TYPE_COMMAND="$RUN_DIR/color-scheme-type-command.txt"
COPY_URL_WEB_ROOT="$RUN_DIR/copy-current-url-site"
INPUT_WEB_ROOT="$RUN_DIR/browser-input-site"
INPUT_TYPE_COMMAND="$RUN_DIR/browser-input-type-command.txt"
CLIPBOARD_WEB_ROOT="$RUN_DIR/webapp-clipboard-site"
CLIPBOARD_ORIGINAL="$RUN_DIR/original-clipboard.txt"
PROFILE_WEB_ROOT="$RUN_DIR/multi-profile-site"
NEW_TAB_COMMAND_LOG="$RUN_DIR/new-tab-command.log"
NEW_TAB_MARKER_COMMAND="$RUN_DIR/new-tab-marker-command.txt"
SECOND_BROWSER_COMMAND="$RUN_DIR/second-browser-command.txt"
THIRD_BROWSER_COMMAND="$RUN_DIR/third-browser-command.txt"
BROWSER_B_TUI_PID_FILE="$RUN_DIR/browser-b-web.pid"
BROWSER_B_TUI_WRAPPER="$RUN_DIR/browser-b-web-wrapper.sh"
PID=""
HTTP_PID=""

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

pointer_move() {
  local x="$1"
  local y="$2"
  local label="$3"
  case "$POINTER_DRIVER" in
    cgevent|system-events)
      if [ "$POINTER_DRIVER" = "system-events" ]; then
        log "${label}_pointer_move_driver=cgevent fallback=system-events-has-no-move-only"
      fi
      swift "$ROOT/scripts/ghostty-app/inject.swift" move "$x" "$y" >>"$HARNESS_LOG" 2>&1
      ;;
    *)
      fail "unsupported TERMSURF_GEOMETRY_POINTER_DRIVER: $POINTER_DRIVER"
      ;;
  esac
}

pointer_click() {
  local x="$1"
  local y="$2"
  local label="$3"
  case "$POINTER_DRIVER" in
    cgevent)
      swift "$ROOT/scripts/ghostty-app/inject.swift" click "$x" "$y" left 1 >>"$HARNESS_LOG" 2>&1
      ;;
    system-events)
      log "${label}_pointer_click_driver=system-events"
      osascript -e "tell application \"System Events\" to click at {$x, $y}" >>"$HARNESS_LOG" 2>&1
      ;;
    *)
      fail "unsupported TERMSURF_GEOMETRY_POINTER_DRIVER: $POINTER_DRIVER"
      ;;
  esac
}

require_file() {
  [ -x "$1" ] || fail "missing executable: $1"
}

require_readable() {
  [ -r "$1" ] || fail "missing readable file: $1"
}

cleanup() {
  if [ -f "$CLIPBOARD_ORIGINAL" ]; then
    pbcopy <"$CLIPBOARD_ORIGINAL" >/dev/null 2>&1 || true
  fi
  if [ -n "${HTTP_PID:-}" ] && kill -0 "$HTTP_PID" >/dev/null 2>&1; then
    kill "$HTTP_PID" >/dev/null 2>&1 || true
  fi
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

require_no_log_after() {
  local start_line="$1"
  local pattern="$2"
  local label="$3"
  if tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "$pattern" >/dev/null 2>&1; then
    fail "$label"
  else
    log "PASS: $label"
  fi
}

send_open_split_message() {
  local socket_path="$1"
  local pane_id="$2"
  local direction="$3"
  local command="$4"
  python3 - "$socket_path" "$pane_id" "$direction" "$command" <<'PY'
import socket
import struct
import sys


def varint(value):
    out = bytearray()
    while value >= 0x80:
        out.append((value & 0x7F) | 0x80)
        value >>= 7
    out.append(value)
    return bytes(out)


def string_field(number, value):
    data = value.encode("utf-8")
    return varint((number << 3) | 2) + varint(len(data)) + data


socket_path, pane_id, direction, command = sys.argv[1:]
open_split = (
    string_field(1, pane_id)
    + string_field(2, direction)
    + string_field(3, command)
)
message = varint((21 << 3) | 2) + varint(len(open_split)) + open_split

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
try:
    sock.connect(socket_path)
    sock.sendall(struct.pack("<I", len(message)) + message)
finally:
    sock.close()
PY
}

wait_for_log_after() {
  local start_line="$1"
  local pattern="$2"
  local label="$3"
  local attempts="${4:-30}"
  for _ in $(seq 1 "$attempts"); do
    if tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "$pattern" >/dev/null 2>&1; then
      log "PASS: $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

try_wait_log_after() {
  local start_line="$1"
  local pattern="$2"
  local attempts="${3:-10}"
  for _ in $(seq 1 "$attempts"); do
    if tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "$pattern" >/dev/null 2>&1; then
      return 0
    fi
    delay 1
  done
  return 1
}

LOG_MATCH_LINE=0
wait_for_log_after_ordered() {
  local start_line="$1"
  local pattern="$2"
  local label="$3"
  local attempts="${4:-30}"
  local match=""
  local relative_line=""
  for _ in $(seq 1 "$attempts"); do
    match="$(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -n -E "$pattern" | head -n 1 || true)"
    if [ -n "$match" ]; then
      relative_line="${match%%:*}"
      LOG_MATCH_LINE=$((start_line + relative_line))
      log "PASS: $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

require_trace() {
  local needle="$1"
  local label="$2"
  if grep -F "$needle" "$CHROMIUM_TRACE" >/dev/null 2>&1; then
    log "PASS: $label"
  else
    fail "missing $label"
  fi
}

wait_for_state_trace() {
  local pattern="$1"
  local label="$2"
  local attempts="${3:-30}"
  for _ in $(seq 1 "$attempts"); do
    if grep -E "$pattern" "$WEBTUI_STATE_TRACE" >/dev/null 2>&1; then
      log "PASS: observed $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

try_wait_state_trace() {
  local pattern="$1"
  local attempts="${2:-10}"
  for _ in $(seq 1 "$attempts"); do
    if grep -E "$pattern" "$WEBTUI_STATE_TRACE" >/dev/null 2>&1; then
      return 0
    fi
    delay 1
  done
  return 1
}

has_state_trace() {
  local pattern="$1"
  grep -E "$pattern" "$WEBTUI_STATE_TRACE" >/dev/null 2>&1
}

state_trace_line_count() {
  if [ -r "$WEBTUI_STATE_TRACE" ]; then
    wc -l <"$WEBTUI_STATE_TRACE" | tr -d ' '
  else
    printf '0\n'
  fi
}

wait_for_state_trace_after() {
  local start_line="$1"
  local pattern="$2"
  local label="$3"
  local attempts="${4:-30}"
  for _ in $(seq 1 "$attempts"); do
    if tail -n +"$((start_line + 1))" "$WEBTUI_STATE_TRACE" | grep -E "$pattern" >/dev/null 2>&1; then
      log "PASS: observed $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

require_no_state_trace_after() {
  local start_line="$1"
  local pattern="$2"
  local label="$3"
  if tail -n +"$((start_line + 1))" "$WEBTUI_STATE_TRACE" | grep -E "$pattern" >/dev/null 2>&1; then
    fail "$label"
  else
    log "PASS: $label"
  fi
}

require_state_trace() {
  local pattern="$1"
  local label="$2"
  if grep -E "$pattern" "$WEBTUI_STATE_TRACE" >/dev/null 2>&1; then
    log "PASS: $label"
  else
    fail "missing $label"
  fi
}

assert_png_pixel_near_white() {
  local image="$1"
  local x="$2"
  local y="$3"
  local label="$4"
  local sample
  sample="$(python3 - "$image" "$x" "$y" <<'PY'
import struct
import sys
import zlib

path = sys.argv[1]
x = int(float(sys.argv[2]))
y = int(float(sys.argv[3]))
data = open(path, "rb").read()
if data[:8] != b"\x89PNG\r\n\x1a\n":
    raise SystemExit("not a png")
pos = 8
width = height = bit_depth = color_type = interlace = None
idat = bytearray()
while pos < len(data):
    length = struct.unpack(">I", data[pos:pos + 4])[0]
    chunk_type = data[pos + 4:pos + 8]
    chunk_data = data[pos + 8:pos + 8 + length]
    pos += 12 + length
    if chunk_type == b"IHDR":
        width, height, bit_depth, color_type, _compression, _filter, interlace = struct.unpack(">IIBBBBB", chunk_data)
    elif chunk_type == b"IDAT":
        idat.extend(chunk_data)
    elif chunk_type == b"IEND":
        break

if bit_depth != 8 or color_type not in (2, 6) or interlace != 0:
    raise SystemExit(f"unsupported png bit_depth={bit_depth} color_type={color_type} interlace={interlace}")
if x < 0 or y < 0 or x >= width or y >= height:
    raise SystemExit(f"sample outside image x={x} y={y} size={width}x{height}")

bpp = 4 if color_type == 6 else 3
stride = width * bpp
raw = zlib.decompress(bytes(idat))
rows = []
offset = 0
prev = bytearray(stride)
for _row_index in range(height):
    filter_type = raw[offset]
    offset += 1
    row = bytearray(raw[offset:offset + stride])
    offset += stride
    for i in range(stride):
        left = row[i - bpp] if i >= bpp else 0
        up = prev[i]
        up_left = prev[i - bpp] if i >= bpp else 0
        if filter_type == 1:
            row[i] = (row[i] + left) & 0xFF
        elif filter_type == 2:
            row[i] = (row[i] + up) & 0xFF
        elif filter_type == 3:
            row[i] = (row[i] + ((left + up) // 2)) & 0xFF
        elif filter_type == 4:
            p = left + up - up_left
            pa = abs(p - left)
            pb = abs(p - up)
            pc = abs(p - up_left)
            predictor = left if pa <= pb and pa <= pc else up if pb <= pc else up_left
            row[i] = (row[i] + predictor) & 0xFF
        elif filter_type != 0:
            raise SystemExit(f"unsupported png filter={filter_type}")
    rows.append(row)
    prev = row

idx = x * bpp
r, g, b = rows[y][idx:idx + 3]
print(f"{r},{g},{b}")
if not (r >= 245 and g >= 245 and b >= 245):
    raise SystemExit(f"pixel not white enough: {r},{g},{b}")
PY
)"
  log "PASS: $label pixel=$sample"
}

trace_line_count() {
  if [ -r "$CHROMIUM_TRACE" ]; then
    wc -l <"$CHROMIUM_TRACE" | tr -d ' '
  else
    printf '0\n'
  fi
}

require_trace_after() {
  local start_line="$1"
  local needle="$2"
  local label="$3"
  if tail -n +"$((start_line + 1))" "$CHROMIUM_TRACE" | grep -F "$needle" >/dev/null 2>&1; then
    log "PASS: $label"
  else
    fail "missing $label"
  fi
}

has_trace() {
  local needle="$1"
  grep -F "$needle" "$CHROMIUM_TRACE" >/dev/null 2>&1
}

wait_for_trace_regex() {
  local pattern="$1"
  local label="$2"
  local attempts="${3:-30}"
  for _ in $(seq 1 "$attempts"); do
    if grep -E "$pattern" "$CHROMIUM_TRACE" >/dev/null 2>&1; then
      log "PASS: observed $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

require_no_trace_after() {
  local start_line="$1"
  local needle="$2"
  local label="$3"
  if tail -n +"$((start_line + 1))" "$CHROMIUM_TRACE" | grep -F "$needle" >/dev/null 2>&1; then
    fail "$label"
  fi
  log "PASS: $label"
}

wait_for_trace_line_after() {
  local start_line="$1"
  local pattern="$2"
  local label="$3"
  local attempts="${4:-30}"
  local line
  for _ in $(seq 1 "$attempts"); do
    line="$(tail -n +"$((start_line + 1))" "$CHROMIUM_TRACE" |
      grep -E "$pattern" |
      tail -1 || true)"
    if [ -n "$line" ]; then
      printf '%s\n' "$line"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

optional_trace_line_after() {
  local start_line="$1"
  local pattern="$2"
  tail -n +"$((start_line + 1))" "$CHROMIUM_TRACE" |
    grep -E "$pattern" |
    tail -1 || true
}

extract_app_tab_id() {
  printf '%s\n' "$1" | sed -E 's/.*tab_id=([0-9]+).*/\1/'
}

wait_for_mouse_down_after() {
  local start_line="$1"
  local tab_id="$2"
  local pane_id="$3"
  local label="$4"
  wait_for_trace_line_after \
    "$start_line" \
    "mouse-event tab=${tab_id} pane=${pane_id} ffi=ts_forward_mouse_event type=down button=left coords=\\([0-9.-]+, [0-9.-]+\\)" \
    "$label" \
    30
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

extract_selected_tab_id() {
  printf '%s\n' "$1" | sed -E 's/.*selected_tab_id:([^ ]+) .*/\1/'
}

extract_pane_id() {
  printf '%s\n' "$1" | sed -E 's/.*pane_id[:=]([^ ]+).*/\1/'
}

extract_surface_id() {
  printf '%s\n' "$1" | sed -E 's/.*surface_id:([^ ]+) .*/\1/'
}

extract_browser_tab_id() {
  printf '%s\n' "$1" | sed -E 's/.*browser_tab_id:([^ ]+) .*/\1/'
}

extract_context_id() {
  printf '%s\n' "$1" | sed -E 's/.*context_id=([^ ]+).*/\1/'
}

extract_grid() {
  printf '%s\n' "$1" | sed -E 's/.*grid=([^ ]+).*/\1/'
}

extract_grid_width() {
  printf '%s\n' "$1" | sed -E 's/^([0-9]+)x[0-9]+.*/\1/'
}

extract_grid_height() {
  printf '%s\n' "$1" | sed -E 's/^[0-9]+x([0-9]+).*/\1/'
}

extract_scrollback_row() {
  printf '%s\n' "$1" | sed -E 's/.*derived_row=([^ ]+).*/\1/'
}

extract_scrollback_total() {
  printf '%s\n' "$1" | sed -E 's/.*scrollbar_total=([^ ]+).*/\1/'
}

extract_scrollback_len() {
  printf '%s\n' "$1" | sed -E 's/.*scrollbar_len=([^ ]+).*/\1/'
}

extract_cell_size() {
  printf '%s\n' "$1" | sed -E 's/.*cell=([^ ]+).*/\1/'
}

extract_overlay_frame() {
  printf '%s\n' "$1" | sed -E 's/.*overlay_frame=(\{\{[^}]+\}, \{[^}]+\}\}).*/\1/'
}

extract_frame_size() {
  printf '%s\n' "$1" | sed -E 's/.*overlay_frame=\{\{[^}]+\}, \{([^,]+), ([^}]+)\}\}.*/\1x\2/'
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

extract_root_frame_x() {
  printf '%s\n' "$1" | sed -E 's/.*root_frame=\{\{([^,]+), [^}]+\}, \{[^}]+\}\}.*/\1/'
}

extract_root_frame_y() {
  printf '%s\n' "$1" | sed -E 's/.*root_frame=\{\{[^,]+, ([^}]+)\}, \{[^}]+\}\}.*/\1/'
}

extract_web_point() {
  printf '%s\n' "$1" | sed -E 's/.*web_point=\{([^,]+), ([^}]+)\}.*/\1,\2/'
}

extract_top_point() {
  printf '%s\n' "$1" | sed -E 's/.*top_point=\{([^,]+), ([^}]+)\}.*/\1,\2/'
}

point_y() {
  printf '%s\n' "$1" | awk -F, '{print $2}'
}

extract_mouse_coords() {
  printf '%s\n' "$1" | sed -E 's/.*coords=\(([^,]+), ([^)]+)\).*/\1,\2/'
}

extract_backing_scale() {
  printf '%s\n' "$1" | sed -E 's/.*backing_scale=([^ ]+).*/\1/'
}

appkit_pixel_from_geometry_line() {
  local line="$1"
  local frame_size
  local width
  local height
  local scale
  frame_size="$(extract_frame_size "$line")"
  width="$(pair_width "$frame_size")"
  height="$(pair_height "$frame_size")"
  scale="$(extract_backing_scale "$line")"
  awk -v width="$width" -v height="$height" -v scale="$scale" \
    'BEGIN { printf "%dx%d\n", int((width * scale) + 0.5), int((height * scale) + 0.5) }'
}

pair_width() {
  printf '%s\n' "$1" | awk -Fx '{print $1}'
}

pair_height() {
  printf '%s\n' "$1" | awk -Fx '{print $2}'
}

click_point_for_frame() {
  local win_line="$1"
  local present_line="$2"
  local _wid wx wy ww wh
  local frame_size frame_x frame_y frame_width frame_height root_frame_size root_height content_y_offset
  IFS=$'\t' read -r _wid wx wy ww wh <<<"$win_line"
  frame_size="$(extract_frame_size "$present_line")"
  frame_x="$(extract_frame_x "$present_line")"
  frame_y="$(extract_frame_y "$present_line")"
  frame_width="$(pair_width "$frame_size")"
  frame_height="$(pair_height "$frame_size")"
  root_frame_size="$(extract_root_frame_size "$present_line")"
  root_height="$(pair_height "$root_frame_size")"
  content_y_offset="$(awk -v wh="$wh" -v root_h="$root_height" 'BEGIN { print int(wh - root_h) }')"
  awk \
    -v wx="$wx" \
    -v wy="$wy" \
    -v content_y="$content_y_offset" \
    -v frame_x="$frame_x" \
    -v frame_y="$frame_y" \
    -v frame_width="$frame_width" \
    -v frame_height="$frame_height" \
    'BEGIN {
      print int(wx + frame_x + (frame_width / 2) + 0.5) "\t" int(wy + content_y + frame_y + (frame_height / 2) + 0.5)
    }'
}

global_point_for_web_point() {
  local win_line="$1"
  local present_line="$2"
  local web_x="$3"
  local web_y="$4"
  local _wid wx wy ww wh
  local frame_x frame_y root_frame_size root_height content_y_offset
  IFS=$'\t' read -r _wid wx wy ww wh <<<"$win_line"
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

global_point_inside_web_frame() {
  local win_line="$1"
  local present_line="$2"
  local web_x="${3:-120}"
  local web_y="${4:-120}"
  local frame_size frame_width frame_height
  frame_size="$(extract_frame_size "$present_line")"
  frame_width="$(pair_width "$frame_size")"
  frame_height="$(pair_height "$frame_size")"
  web_x="$(awk -v x="$web_x" -v width="$frame_width" 'BEGIN { if (x < 4) x = 4; if (x > width - 4) x = width / 2; print int(x + 0.5) }')"
  web_y="$(awk -v y="$web_y" -v height="$frame_height" 'BEGIN { if (y < 4) y = 4; if (y > height - 4) y = height / 2; print int(y + 0.5) }')"
  global_point_for_web_point "$win_line" "$present_line" "$web_x" "$web_y"
}

global_point_below_web_frame() {
  local win_line="$1"
  local present_line="$2"
  local local_x="${3:-120}"
  local _wid wx wy ww wh
  local frame_size frame_x frame_y frame_width frame_height root_frame_size root_height content_y_offset
  IFS=$'\t' read -r _wid wx wy ww wh <<<"$win_line"
  frame_size="$(extract_frame_size "$present_line")"
  frame_x="$(extract_frame_x "$present_line")"
  frame_y="$(extract_frame_y "$present_line")"
  frame_width="$(pair_width "$frame_size")"
  frame_height="$(pair_height "$frame_size")"
  root_frame_size="$(extract_root_frame_size "$present_line")"
  root_height="$(pair_height "$root_frame_size")"
  content_y_offset="$(awk -v wh="$wh" -v root_h="$root_height" 'BEGIN { print int(wh - root_h) }')"
  awk \
    -v wx="$wx" \
    -v wy="$wy" \
    -v content_y="$content_y_offset" \
    -v frame_x="$frame_x" \
    -v frame_y="$frame_y" \
    -v frame_width="$frame_width" \
    -v frame_height="$frame_height" \
    -v root_height="$root_height" \
    -v local_x="$local_x" \
    'BEGIN {
      x = local_x
      if (x < 4) x = 4
      if (x > frame_width - 4) x = frame_width / 2
      y = frame_y + frame_height + 24
      if (y > root_height - 4) y = frame_y - 8
      if (y < 4) y = 4
      print int(wx + frame_x + x + 0.5) "\t" int(wy + content_y + y + 0.5)
    }'
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

compare_pair_width() {
  local pair="$1"
  local ref="$2"
  local mode="$3"
  local width ref_width
  width="$(pair_width "$pair")"
  ref_width="$(pair_width "$ref")"
  awk \
    -v width="$width" \
    -v ref_width="$ref_width" \
    -v mode="$mode" \
    'BEGIN {
      if (mode == "gt") { exit !(width > ref_width) }
      if (mode == "lt") { exit !(width < ref_width) }
      exit 1
    }'
}

wait_for_appkit_frame_width_after() {
  local start_line="$1"
  local ref_frame="$2"
  local mode="$3"
  local label="$4"
  local attempts="${5:-30}"
  local line frame_size
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      frame_size="$(extract_frame_size "$line")"
      if [ -n "$frame_size" ] && [ "$frame_size" != "$line" ] && compare_pair_width "$frame_size" "$ref_frame" "$mode"; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E 'TermSurf geometry layer=appkit event=presented ' || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_appkit_pixels_width_after() {
  local start_line="$1"
  local ref_pixel="$2"
  local mode="$3"
  local label="$4"
  local attempts="${5:-30}"
  local line pixel
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      pixel="$(extract_appkit_pixel "$line")"
      if [ -n "$pixel" ] && compare_pair_width "$pixel" "$ref_pixel" "$mode"; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E 'TermSurf geometry layer=appkit event=presented_pixels' || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

compare_points_within_tolerance() {
  local actual="$1"
  local expected="$2"
  local tolerance="$3"
  awk \
    -v actual="$actual" \
    -v expected="$expected" \
    -v tolerance="$tolerance" \
    'BEGIN {
      split(actual, a, ",")
      split(expected, e, ",")
      dx = a[1] - e[1]
      dy = a[2] - e[2]
      if (dx < 0) dx = -dx
      if (dy < 0) dy = -dy
      exit !((dx <= tolerance) && (dy <= tolerance))
    }'
}

assert_mouse_click_matches_hit() {
  local hit_line="$1"
  local mouse_line="$2"
  local label="$3"
  local web_point
  local mouse_coords
  web_point="$(extract_web_point "$hit_line")"
  mouse_coords="$(extract_mouse_coords "$mouse_line")"
  [ -n "$web_point" ] && [ "$web_point" != "$hit_line" ] || fail "$label missing AppKit web_point"
  [ -n "$mouse_coords" ] && [ "$mouse_coords" != "$mouse_line" ] || fail "$label missing Chromium mouse coords"
  if compare_points_within_tolerance "$mouse_coords" "$web_point" 1; then
    log "PASS: $label Chromium mouse coords match AppKit web_point within 1 CSS pixel web_point=$web_point mouse_coords=$mouse_coords"
  else
    fail "$label Chromium mouse coords mismatch: web_point=$web_point mouse_coords=$mouse_coords"
  fi
}

assert_mouse_after_click() {
  local x="$1"
  local y="$2"
  local click_label="$3"
  local context_id="$4"
  local tab_id="$5"
  local pane_id="$6"
  local expected_frame="$7"
  local assert_label="$8"
  local log_start
  local trace_start
  local hit_line
  local mouse_line
  log_start="$(log_line_count)"
  trace_start="$(trace_line_count)"
  click_global_point "$x" "$y" "$click_label"
  hit_line="$(wait_for_hit_after "$log_start" "$context_id" "$assert_label AppKit hit-test")"
  require_text "$hit_line" "overlay_frame=${expected_frame}" "$assert_label hit-test uses current AppKit frame"
  require_text "$hit_line" "web_point={" "$assert_label hit-test includes webview-relative point"
  mouse_line="$(wait_for_mouse_down_after "$trace_start" "$tab_id" "$pane_id" "$assert_label Chromium mouse down")"
  assert_mouse_click_matches_hit "$hit_line" "$mouse_line" "$assert_label"
}

prime_mouse_focus() {
  local x="$1"
  local y="$2"
  local click_label="$3"
  local context_id="$4"
  local expected_frame="$5"
  local assert_label="$6"
  local log_start
  local hit_line
  log_start="$(log_line_count)"
  click_global_point "$x" "$y" "$click_label"
  hit_line="$(wait_for_hit_after "$log_start" "$context_id" "$assert_label focus-prime AppKit hit-test")"
  require_text "$hit_line" "overlay_frame=${expected_frame}" "$assert_label focus-prime uses current AppKit frame"
  delay 0.5
}

assert_stale_click_misses_browser() {
  local x="$1"
  local y="$2"
  local click_label="$3"
  local context_id="$4"
  local tab_id="$5"
  local pane_id="$6"
  local assert_label="$7"
  local log_start
  local trace_start
  log_start="$(log_line_count)"
  trace_start="$(trace_line_count)"
  click_global_point "$x" "$y" "$click_label"
  wait_for_negative_hit_after "$log_start" "$context_id" "$assert_label AppKit stale-coordinate hit-test" require-hit-false 5
  require_no_trace_after "$trace_start" "mouse-event tab=${tab_id} pane=${pane_id}" "$assert_label stale coordinate did not reach Chromium mouse input"
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

compare_split_down_pair() {
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
      delta = width - ref_width
      if (delta < 0) delta = -delta
      exit !((height < ref_height) && (delta <= tolerance))
    }'
}

compare_split_right_resize_pair() {
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
      exit !((width > ref_width) && (delta <= tolerance))
    }'
}

compare_split_right_equalize_pair() {
  local pair="$1"
  local target="$2"
  local unequal="$3"
  local tolerance="$4"
  local width height target_width target_height unequal_width
  width="$(pair_width "$pair")"
  height="$(pair_height "$pair")"
  target_width="$(pair_width "$target")"
  target_height="$(pair_height "$target")"
  unequal_width="$(pair_width "$unequal")"
  awk \
    -v width="$width" \
    -v height="$height" \
    -v target_width="$target_width" \
    -v target_height="$target_height" \
    -v unequal_width="$unequal_width" \
    -v tolerance="$tolerance" \
    'BEGIN {
      width_delta = width - target_width
      if (width_delta < 0) width_delta = -width_delta
      height_delta = height - target_height
      if (height_delta < 0) height_delta = -height_delta
      exit !((width < unequal_width) && (width_delta <= tolerance) && (height_delta <= tolerance))
    }'
}

compare_split_right_zoom_pair() {
  local pair="$1"
  local target="$2"
  local split="$3"
  local tolerance="$4"
  local width height target_width target_height split_width
  width="$(pair_width "$pair")"
  height="$(pair_height "$pair")"
  target_width="$(pair_width "$target")"
  target_height="$(pair_height "$target")"
  split_width="$(pair_width "$split")"
  awk \
    -v width="$width" \
    -v height="$height" \
    -v target_width="$target_width" \
    -v target_height="$target_height" \
    -v split_width="$split_width" \
    -v tolerance="$tolerance" \
    'BEGIN {
      width_delta = width - target_width
      if (width_delta < 0) width_delta = -width_delta
      height_delta = height - target_height
      if (height_delta < 0) height_delta = -height_delta
      exit !((width > split_width) && (width_delta <= tolerance) && (height_delta <= tolerance))
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
  local tolerance="${7:-8}"
  local line frame_size
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      frame_size="$(extract_frame_size "$line")"
      if [ -n "$frame_size" ] && [ "$frame_size" != "$line" ] && compare_split_right_pair "$frame_size" "$ref_frame" "$tolerance"; then
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
  local tolerance="${7:-16}"
  local line pixel
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      pixel="$(extract_appkit_pixel "$line")"
      if [ -n "$pixel" ] && compare_split_right_pair "$pixel" "$ref_pixel" "$tolerance"; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${pane_id} .*context_id=${context_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_split_down_frame_after() {
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
      if [ -n "$frame_size" ] && [ "$frame_size" != "$line" ] && compare_split_down_pair "$frame_size" "$ref_frame" 8; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_split_down_pixels_after() {
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
      if [ -n "$pixel" ] && compare_split_down_pair "$pixel" "$ref_pixel" 16; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${pane_id} .*context_id=${context_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_split_right_resize_frame_after() {
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
      if [ -n "$frame_size" ] && [ "$frame_size" != "$line" ] && compare_split_right_resize_pair "$frame_size" "$ref_frame" 8; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_split_right_resize_pixels_after() {
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
      if [ -n "$pixel" ] && compare_split_right_resize_pair "$pixel" "$ref_pixel" 16; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${pane_id} .*context_id=${context_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_split_right_equalize_frame_after() {
  local start_line="$1"
  local pane_id="$2"
  local context_id="$3"
  local target_frame="$4"
  local unequal_frame="$5"
  local label="$6"
  local attempts="${7:-30}"
  local line frame_size
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      frame_size="$(extract_frame_size "$line")"
      if [ -n "$frame_size" ] && [ "$frame_size" != "$line" ] && compare_split_right_equalize_pair "$frame_size" "$target_frame" "$unequal_frame" 8; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_split_right_equalize_pixels_after() {
  local start_line="$1"
  local pane_id="$2"
  local context_id="$3"
  local target_pixel="$4"
  local unequal_pixel="$5"
  local label="$6"
  local attempts="${7:-30}"
  local line pixel
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      pixel="$(extract_appkit_pixel "$line")"
      if [ -n "$pixel" ] && compare_split_right_equalize_pair "$pixel" "$target_pixel" "$unequal_pixel" 16; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${pane_id} .*context_id=${context_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_split_right_zoom_frame_after() {
  local start_line="$1"
  local pane_id="$2"
  local context_id="$3"
  local target_frame="$4"
  local split_frame="$5"
  local label="$6"
  local attempts="${7:-30}"
  local line frame_size
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      frame_size="$(extract_frame_size "$line")"
      if [ -n "$frame_size" ] && [ "$frame_size" != "$line" ] && compare_split_right_zoom_pair "$frame_size" "$target_frame" "$split_frame" 8; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_split_right_zoom_pixels_after() {
  local start_line="$1"
  local pane_id="$2"
  local context_id="$3"
  local target_pixel="$4"
  local split_pixel="$5"
  local label="$6"
  local attempts="${7:-30}"
  local line pixel
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      pixel="$(extract_appkit_pixel "$line")"
      if [ -n "$pixel" ] && compare_split_right_zoom_pair "$pixel" "$target_pixel" "$split_pixel" 16; then
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
  local mode="${4:-require-hit-false}"
  local attempts="${5:-30}"
  local line
  for _ in $(seq 1 "$attempts"); do
    if tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=hit_test .*context_id=${context_id} .*hit=true" >/dev/null 2>&1; then
      fail "$label routed to original browser context"
    fi
    line="$(tail -n +"$((start_line + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=hit_test .*context_id=${context_id} .*hit=false" | tail -1 || true)"
    if [ -n "$line" ] && printf '%s\n' "$line" | grep -F 'overlay_frame={' >/dev/null 2>&1; then
      printf '%s\n' "$line"
      log "PASS: observed $label with explicit hit=false"
      return 0
    fi
    delay 1
  done
  if [ "$mode" = "allow-absent" ]; then
    log "PASS: $label did not route to original browser context"
  else
    fail "timed out waiting for $label explicit hit=false"
  fi
}

require_no_different_appkit_frame_after() {
  local start_line="$1"
  local pane_id="$2"
  local context_id="$3"
  local expected_frame="$4"
  local label="$5"
  local line
  line="$(tail -n +"$((start_line + 1))" "$APP_LOG" |
    grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}" |
    grep -Fv "overlay_frame=$expected_frame" |
    tail -1 || true)"
  if [ -n "$line" ]; then
    fail "$label changed frame: $line"
  fi
  log "PASS: $label"
}

require_no_different_appkit_pixels_after() {
  local start_line="$1"
  local pane_id="$2"
  local context_id="$3"
  local expected_pixels="$4"
  local label="$5"
  local line
  line="$(tail -n +"$((start_line + 1))" "$APP_LOG" |
    grep -E "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${pane_id} .*context_id=${context_id}" |
    grep -Fv "appkit_pixel=$expected_pixels" |
    tail -1 || true)"
  if [ -n "$line" ]; then
    fail "$label changed pixels: $line"
  fi
  log "PASS: $label"
}

wait_for_line_after() {
  local start_line="$1"
  local pattern="$2"
  local label="$3"
  local attempts="${4:-30}"
  local line
  for _ in $(seq 1 "$attempts"); do
    line="$(tail -n +"$((start_line + 1))" "$APP_LOG" |
      grep -E "$pattern" |
      tail -1 || true)"
    if [ -n "$line" ]; then
      printf '%s\n' "$line"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_different_zig_event_after() {
  local start_line="$1"
  local event="$2"
  local old_pane_id="$3"
  local label="$4"
  local attempts="${5:-30}"
  local line
  for _ in $(seq 1 "$attempts"); do
    line="$(tail -n +"$((start_line + 1))" "$APP_LOG" |
      grep -E "TermSurf geometry layer=zig event=${event} " |
      grep -Fv "pane_id:${old_pane_id}" |
      tail -1 || true)"
    if [ -n "$line" ]; then
      printf '%s\n' "$line"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_exact_appkit_frame_after() {
  local start_line="$1"
  local pane_id="$2"
  local context_id="$3"
  local expected_frame="$4"
  local label="$5"
  local attempts="${6:-30}"
  local line
  for _ in $(seq 1 "$attempts"); do
    line="$(tail -n +"$((start_line + 1))" "$APP_LOG" |
      grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}" |
      grep -F "overlay_frame=$expected_frame" |
      tail -1 || true)"
    if [ -n "$line" ]; then
      printf '%s\n' "$line"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_exact_appkit_pixels_after() {
  local start_line="$1"
  local pane_id="$2"
  local context_id="$3"
  local expected_pixel="$4"
  local label="$5"
  local attempts="${6:-30}"
  local line
  for _ in $(seq 1 "$attempts"); do
    line="$(tail -n +"$((start_line + 1))" "$APP_LOG" |
      grep -E "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${pane_id} .*context_id=${context_id}" |
      grep -F "appkit_pixel=$expected_pixel" |
      tail -1 || true)"
    if [ -n "$line" ]; then
      printf '%s\n' "$line"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_changed_appkit_frame_after() {
  local start_line="$1"
  local pane_id="$2"
  local context_id="$3"
  local expected_frame="$4"
  local label="$5"
  local attempts="${6:-30}"
  local line frame
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      frame="$(extract_overlay_frame "$line")"
      if [ -n "$frame" ] && [ "$frame" != "$line" ] && [ "$frame" != "$expected_frame" ]; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" |
      grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_changed_appkit_pixels_after() {
  local start_line="$1"
  local pane_id="$2"
  local context_id="$3"
  local expected_pixel="$4"
  local label="$5"
  local attempts="${6:-30}"
  local line pixel
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      pixel="$(extract_appkit_pixel "$line")"
      if [ -n "$pixel" ] && [ "$pixel" != "$line" ] && [ "$pixel" != "$expected_pixel" ]; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" |
      grep -E "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${pane_id} .*context_id=${context_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_scrollback_row_less_after() {
  local start_line="$1"
  local pane_id="$2"
  local ref_row="$3"
  local label="$4"
  local attempts="${5:-30}"
  local line row
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      row="$(extract_scrollback_row "$line")"
      case "$row" in
        ''|unknown:*|*[!0-9]*) continue ;;
      esac
      if [ "$row" -lt "$ref_row" ]; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" |
      grep -E "TermSurf geometry layer=scrollview .*pane_id:${pane_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_scrollback_row_at_least_after() {
  local start_line="$1"
  local pane_id="$2"
  local ref_row="$3"
  local label="$4"
  local attempts="${5:-30}"
  local line row
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      row="$(extract_scrollback_row "$line")"
      case "$row" in
        ''|unknown:*|*[!0-9]*) continue ;;
      esac
      if [ "$row" -ge "$ref_row" ]; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" |
      grep -E "TermSurf geometry layer=scrollview .*pane_id:${pane_id}" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

require_no_scrollback_row_at_least_after() {
  local start_line="$1"
  local pane_id="$2"
  local ref_row="$3"
  local label="$4"
  local line row
  while IFS= read -r line; do
    row="$(extract_scrollback_row "$line")"
    case "$row" in
      ''|unknown:*|*[!0-9]*) continue ;;
    esac
    if [ "$row" -ge "$ref_row" ]; then
      fail "$label: saw row=$row >= ref=$ref_row in line: $line"
    fi
  done < <(tail -n +"$((start_line + 1))" "$APP_LOG" |
    grep -E "TermSurf geometry layer=scrollview .*pane_id:${pane_id}" || true)
  log "PASS: $label"
}

wait_for_selected_tab_change_after() {
  local start_line="$1"
  local selected_tab_id="$2"
  local label="$3"
  local attempts="${4:-30}"
  local line changed_id
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      changed_id="$(extract_selected_tab_id "$line")"
      case "$changed_id" in
        ""|"$line"|"$selected_tab_id"|unknown:*|-1) ;;
        *)
          printf '%s\n' "$line"
          return 0
          ;;
      esac
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" |
      grep -E "TermSurf geometry layer=appkit event=.*selected_tab_id:" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_selected_tab_not_in_after() {
  local start_line="$1"
  local label="$2"
  local attempts="$3"
  shift 3
  local line changed_id excluded excluded_id
  for _ in $(seq 1 "$attempts"); do
    while IFS= read -r line; do
      changed_id="$(extract_selected_tab_id "$line")"
      case "$changed_id" in
        ""|"$line"|unknown:*|-1) continue ;;
      esac
      excluded=false
      for excluded_id in "$@"; do
        if [ "$changed_id" = "$excluded_id" ]; then
          excluded=true
          break
        fi
      done
      if [ "$excluded" = false ]; then
        printf '%s\n' "$line"
        return 0
      fi
    done < <(tail -n +"$((start_line + 1))" "$APP_LOG" |
      grep -E "TermSurf geometry layer=(appkit|scrollview) .*selected_tab_id:" || true)
    delay 1
  done
  fail "timed out waiting for $label"
}

wait_for_selected_tab_id_after() {
  local start_line="$1"
  local selected_tab_id="$2"
  local label="$3"
  local attempts="${4:-30}"
  local line
  for _ in $(seq 1 "$attempts"); do
    line="$(tail -n +"$((start_line + 1))" "$APP_LOG" |
      grep -E "TermSurf geometry layer=appkit event=.*selected_tab_id:${selected_tab_id}" |
      tail -1 || true)"
    if [ -n "$line" ]; then
      printf '%s\n' "$line"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

window_bounds() {
  swift "$WINDOW_BOUNDS" "$WID"
}

window_bounds_for() {
  swift "$WINDOW_BOUNDS" "$1"
}

app_windows() {
  swift "$APP_WINDOWS" "$PID"
}

focus_window_bounds() {
  local bounds="$1"
  local bid bx by bw bh
  IFS=$'\t' read -r bid bx by bw bh <<<"$bounds"
  swift "$FOCUS_WINDOW" "$PID" "$bx" "$by" "$bw" "$bh"
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
  pointer_move "$click_x" "$click_y" "$label"
  delay 0.25
  pointer_click "$click_x" "$click_y" "$label"
}

move_window_center() {
  local bounds="$1"
  local label="$2"
  local bid bx by bw bh move_x move_y nudge_x nudge_y
  IFS=$'\t' read -r bid bx by bw bh <<<"$bounds"
  move_x=$((bx + bw / 2))
  move_y=$((by + bh / 2))
  nudge_x=$((bx + 10))
  nudge_y=$((by + 10))
  log "${label}_input_point=${move_x},${move_y}"
  pointer_move "$nudge_x" "$nudge_y" "${label}_nudge"
  delay 0.1
  pointer_move "$move_x" "$move_y" "$label"
  delay 0.5
}

click_global_point() {
  local x="$1"
  local y="$2"
  local label="$3"
  log "${label}_input_point=${x},${y}"
  pointer_move "$x" "$y" "$label"
  delay 0.25
  pointer_click "$x" "$y" "$label"
}

move_global_point() {
  local x="$1"
  local y="$2"
  local label="$3"
  log "${label}_input_point=${x},${y}"
  pointer_move "$x" "$y" "$label"
}

click_negative_global_point() {
  local x="$1"
  local y="$2"
  local label="$3"
  log "${label}_input_point=${x},${y}"
  pointer_move "$x" "$y" "$label"
  delay 0.75
  NEGATIVE_HIT_START_LINE="$(log_line_count)"
  pointer_click "$x" "$y" "$label"
}

click_browser_frame_center() {
  local bounds="$1"
  local frame_x="$2"
  local frame_y="$3"
  local frame_size="$4"
  local label="$5"
  local bid bx by bw bh click_x click_y
  IFS=$'\t' read -r bid bx by bw bh <<<"$bounds"
  click_x="$(awk -v wx="$bx" -v fx="$frame_x" -v fs="$frame_size" 'BEGIN { split(fs, parts, "x"); print int(wx + fx + (parts[1] / 2) + 0.5) }')"
  click_y="$(awk -v wy="$by" -v fy="$frame_y" -v fs="$frame_size" 'BEGIN { split(fs, parts, "x"); print int(wy + fy + (parts[2] / 2) + 0.5) }')"
  click_global_point "$click_x" "$click_y" "$label"
}

enter_browser_browse() {
  local label="$1"
  local pane_id="$2"
  local browser_tab_id="$3"
  local mode_start_line
  local mode_trace_start_line
  mode_start_line="$(log_line_count)"
  mode_trace_start_line="$(trace_line_count)"
  log "${label}_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  if try_wait_log_after "$mode_start_line" "ModeChanged: pane_id=${pane_id} browsing=true" 10; then
    log "PASS: $label webtui entered browse mode"
    require_trace_after "$mode_trace_start_line" "focus-changed tab=${browser_tab_id} pane=${pane_id} ffi=ts_set_focus focused=true" "Chromium observed $label focus=true after browse mode"
  elif tail -n +"$((mode_start_line + 1))" "$APP_LOG" | grep -E "KeyEvent: pane_id=${pane_id} tab_id=${browser_tab_id} type=down .*windows_key_code=13" >/dev/null 2>&1 &&
    has_trace "focus-changed tab=${browser_tab_id} pane=${pane_id} ffi=ts_set_focus focused=true"; then
    log "PASS: $label webtui was already in browse mode"
  else
    fail "timed out waiting for $label webtui entered browse mode"
  fi
}

leave_browser_browse() {
  local label="$1"
  local pane_id="$2"
  local browser_tab_id="$3"
  local control_start_line
  local control_trace_start_line
  control_start_line="$(log_line_count)"
  control_trace_start_line="$(trace_line_count)"
  log "${label}_control_key=escape=Mode::Control"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$control_start_line" "ModeChanged: pane_id=${pane_id} browsing=false" "$label webtui returned to control mode"
  require_trace_after "$control_trace_start_line" "focus-changed tab=${browser_tab_id} pane=${pane_id} ffi=ts_set_focus focused=false" "Chromium observed $label focus=false after control mode"
}

edit_url_replace() {
  local label="$1"
  local target_url="$2"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1 || true
  delay 0.25
  log "${label}_edit_key=shift+a=edit-url-end"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 0 shift >>"$HARNESS_LOG" 2>&1
  delay 0.5
  for _ in $(seq 1 180); do
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 51 >>"$HARNESS_LOG" 2>&1
  done
  printf '%s' "$target_url" >"$CRASH_TYPE_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$CRASH_TYPE_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
}

run_webtui_command() {
  local label="$1"
  local command="$2"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1 || true
  delay 0.25
  printf ':%s' "$command" >"$COLOR_TYPE_COMMAND"
  log "${label}_command=:${command}"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$COLOR_TYPE_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
}

type_marker_require_only() {
  local label="$1"
  local marker="$2"
  local active_tab_id="$3"
  local active_pane_id="$4"
  local inactive_tab_id_1="${5:-}"
  local inactive_pane_id_1="${6:-}"
  local inactive_tab_id_2="${7:-}"
  local inactive_pane_id_2="${8:-}"
  local key_start_line
  key_start_line="$(trace_line_count)"
  printf '%s\n' "$marker" >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$key_start_line" "key-event tab=${active_tab_id} pane=${active_pane_id}" "$label keyboard marker reached active browser"
  if [ -n "$inactive_tab_id_1" ] && [ -n "$inactive_pane_id_1" ]; then
    require_no_trace_after "$key_start_line" "key-event tab=${inactive_tab_id_1} pane=${inactive_pane_id_1}" "$label keyboard marker did not reach inactive browser 1"
  fi
  if [ -n "$inactive_tab_id_2" ] && [ -n "$inactive_pane_id_2" ]; then
    require_no_trace_after "$key_start_line" "key-event tab=${inactive_tab_id_2} pane=${inactive_pane_id_2}" "$label keyboard marker did not reach inactive browser 2"
  fi
}

assert_gui_active_cycle() {
  local label="$1"
  local active_tab_id="$2"
  local active_pane_id="$3"
  local inactive_tab_id="$4"
  local inactive_pane_id="$5"
  local app_start_line
  local trace_start_line
  local hide_result
  local show_result

  app_start_line="$(log_line_count)"
  trace_start_line="$(trace_line_count)"
  log "${label}_hide_show=deactivate-reactivate"
  hide_result="$(swift "$HIDE_APP" "$PID" hide >>"$HARNESS_LOG" 2>&1; tail -1 "$HARNESS_LOG")"
  [ "$hide_result" = "hidden" ] || fail "$label app hide did not report hidden: $hide_result"
  log "PASS: $label app hide reported hidden"
  delay 1
  show_result="$(swift "$HIDE_APP" "$PID" show >>"$HARNESS_LOG" 2>&1; tail -1 "$HARNESS_LOG")"
  [ "$show_result" = "visible" ] || fail "$label app show did not report visible: $show_result"
  log "PASS: $label app show reported visible"
  delay 1

  require_log_after "$app_start_line" "SetGuiActive: tab_id=0 active=false reason=gui_deactivated" "$label Ghostboard sent inactive broadcast"
  require_trace_after "$trace_start_line" "set-gui-active tab=0 active=false reason=gui_deactivated" "$label Chromium received inactive broadcast"
  require_log_after "$app_start_line" "SetGuiActive: pane_id=${active_pane_id} tab_id=${active_tab_id} active=true reason=gui_activated" "$label Ghostboard sent active state to focused browser"
  require_trace_after "$trace_start_line" "set-gui-active tab=${active_tab_id} pane=${active_pane_id} active=true reason=gui_activated target_count=1" "$label Chromium received active state for focused browser"
  require_no_trace_after "$trace_start_line" "set-gui-active tab=${inactive_tab_id} pane=${inactive_pane_id} active=true reason=gui_activated" "$label Chromium did not receive stale active state for unfocused browser"
}

devtools_probe() {
  local socket_path="$1"
  local pane_id="$2"
  local tab_id="$3"
  local profile="$4"
  local browser="$5"
  python3 "$DEVTOOLS_QUERY_PROBE" "$socket_path" "$pane_id" "$tab_id" "$profile" "$browser"
}

devtools_overlay_probe() {
  local socket_path="$1"
  local pane_id="$2"
  local tab_id="$3"
  local profile="$4"
  local browser="$5"
  python3 "$DEVTOOLS_OVERLAY_PROBE" "$socket_path" "$pane_id" "$tab_id" "$profile" "$browser"
}

case "$SCENARIO" in
  initial-open|launch-discovery-contract|named-chromium-debug-launch|named-chromium-invalid-env|installed-chromium-release-launch|hello-config-homepage|hello-config-browser-list|hello-empty-browser-list|ghostboard-config-paths|browser-state-smoke|browser-command-navigation|back-navigation-focus-theft|javascript-dialog-smoke|http-auth-smoke|renderer-crash-smoke|color-scheme-smoke|copy-current-url-smoke|browser-input-granularity|click-driven-browse-mode|webapp-clipboard-copy|multi-profile-isolation|same-profile-server-lifecycle|tui-disconnect-reconnect|visible-profile-identity|two-browser-split-routing|window-resize|performance-window-resize|split-right|split-right-border-config|performance-split-right|split-down|split-right-resize|split-right-equalize|split-right-zoom|split-right-close-sibling|split-right-close-browser-pane|split-right-focus-switch|new-terminal-tab-visibility|open-browser-in-new-tab|close-browser-tab|open-browser-in-new-window|multiple-windows-with-browsers|display-move-backing-scale|fullscreen-unfullscreen|minimize-hide-restore|font-size-cell-metrics|tui-overlay-resize-command|terminal-scrollback-movement|browser-navigation-geometry|devtools-split-geometry|devtools-singleton-guard|mouse-after-geometry-change|keyboard-after-tab-window-switch|gui-active-multi-tab) ;;
  *)
    fail "unsupported scenario: $SCENARIO"
    ;;
esac

RESOLVER_ONLY_SCENARIO=0
if [ "$SCENARIO" = "named-chromium-debug-launch" ] || [ "$SCENARIO" = "installed-chromium-release-launch" ]; then
  RESOLVER_ONLY_SCENARIO=1
fi

SKIP_INITIAL_HIT_TEST=0
if [ "$RESOLVER_ONLY_SCENARIO" = "1" ] ||
  [ "$SCENARIO" = "performance-window-resize" ] ||
  [ "$SCENARIO" = "performance-split-right" ] ||
  [ "$SCENARIO" = "browser-command-navigation" ] ||
  [ "$SCENARIO" = "back-navigation-focus-theft" ] ||
  [ "$SCENARIO" = "click-driven-browse-mode" ] ||
  [ "$SCENARIO" = "browser-input-granularity" ]; then
  SKIP_INITIAL_HIT_TEST=1
fi

require_file "$APP_BIN"
require_file "$WEB"
require_file "$CHROMIUM"
if [ "$SCENARIO" = "installed-chromium-release-launch" ]; then
  require_file "$INSTALLED_CHROMIUM"
fi
require_readable "$ROOT/scripts/ghostty-app/inject.swift"
require_readable "$ROOT/scripts/ghostty-app/winid.swift"

if [ "$SCENARIO" = "browser-state-smoke" ] || [ "$SCENARIO" = "browser-command-navigation" ] || [ "$SCENARIO" = "back-navigation-focus-theft" ]; then
  STATE_HTTP_PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
PY
)"
  mkdir -p "$STATE_WEB_ROOT"
  cat >"$STATE_WEB_ROOT/index.html" <<'EOF'
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>Issue 26061712000816 State Smoke Initial</title>
    <style>
      html,
      body {
        margin: 0;
        background: #ffffff;
        color: #111111;
        font: 16px -apple-system, BlinkMacSystemFont, sans-serif;
      }
      #white-sample {
        position: absolute;
        left: 20px;
        top: 210px;
        width: 80px;
        height: 80px;
        background: #ffffff;
      }
      a,
      button {
        position: absolute;
        left: 32px;
      }
      #hover-link {
        top: 32px;
      }
      #blank-link {
        top: 82px;
      }
      #reload-count {
        position: absolute;
        left: 32px;
        top: 132px;
      }
    </style>
  </head>
  <body>
    <a id="hover-link" href="/hover-target.html">ISSUE816 hover target</a>
    <a id="blank-link" href="/blank-target.html" target="_blank">ISSUE816 blank target</a>
    <div id="reload-count"></div>
    <div id="white-sample"></div>
    <script>
      const count = Number(sessionStorage.getItem("issue816ReloadCount") || "0") + 1;
      sessionStorage.setItem("issue816ReloadCount", String(count));
      document.getElementById("reload-count").textContent = `ISSUE816_RELOAD_COUNT_${count}`;
      console.log(`ISSUE816_CONSOLE_MARKER_${count}`);
      window.addEventListener("load", () => {
        document.title = `Issue 26061712000816 State Smoke Updated ${count}`;
        document.body.dataset.loaded = "true";
      });
    </script>
  </body>
</html>
EOF
  cat >"$STATE_WEB_ROOT/hover-target.html" <<'EOF'
<!doctype html>
<html>
  <head><title>Issue 26061712000816 Hover Target</title></head>
  <body>ISSUE816_HOVER_TARGET</body>
</html>
EOF
  cat >"$STATE_WEB_ROOT/blank-target.html" <<'EOF'
<!doctype html>
<html>
  <head><title>Issue 26061712000816 Blank Target</title></head>
  <body>ISSUE816_BLANK_TARGET</body>
</html>
EOF
  python3 -m http.server "$STATE_HTTP_PORT" --bind 127.0.0.1 --directory "$STATE_WEB_ROOT" >>"$HARNESS_LOG" 2>&1 &
  HTTP_PID="$!"
  URL="http://127.0.0.1:${STATE_HTTP_PORT}/index.html"
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
  python3 - "$URL" <<'PY' >/dev/null 2>&1 || fail "browser-state HTTP fixture did not become ready"
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY
  log "browser_state_web_root=$STATE_WEB_ROOT"
  log "browser_state_http_pid=$HTTP_PID"
  log "browser_state_url=$URL"
fi

if [ "$SCENARIO" = "javascript-dialog-smoke" ]; then
  DIALOG_HTTP_PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
PY
)"
  mkdir -p "$DIALOG_WEB_ROOT"
  cat >"$DIALOG_WEB_ROOT/initial-alert.html" <<'EOF'
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>Issue 26061712000816 Dialog Initial Pending</title>
    <style>
      html,
      body {
        margin: 0;
        background: #ffffff;
        color: #111111;
        font: 16px -apple-system, BlinkMacSystemFont, sans-serif;
      }
      button,
      a {
        position: absolute;
        left: 32px;
        min-width: 220px;
        height: 28px;
      }
      #alert-button { top: 70px; }
      #confirm-accept-button { top: 120px; }
      #confirm-cancel-button { top: 170px; }
      #prompt-accept-button { top: 220px; }
      #prompt-cancel-button { top: 270px; }
      #beforeunload-arm-button { top: 320px; }
      #beforeunload-leave-link { top: 370px; line-height: 28px; }
      #result {
        position: absolute;
        left: 32px;
        top: 20px;
      }
    </style>
    <script>
      function record(name, value) {
        const marker = `ISSUE816_DIALOG_RESULT case=${name} value=${value}`;
        console.log(marker);
        document.title = `Issue 26061712000816 Dialog ${name} ${value}`;
        const result = document.getElementById("result");
        if (result) result.textContent = marker;
      }

      alert("ISSUE816_INITIAL_ALERT");
      window.addEventListener("DOMContentLoaded", () => {
        record("initial-alert", "resumed");
      });
    </script>
  </head>
  <body>
    <div id="result">ISSUE816_DIALOG_READY</div>
    <button id="alert-button" onclick="alert('ISSUE816_ALERT'); record('alert', 'resumed')">alert</button>
    <button id="confirm-accept-button" onclick="record('confirm-accept', String(confirm('ISSUE816_CONFIRM_ACCEPT')))">confirm accept</button>
    <button id="confirm-cancel-button" onclick="record('confirm-cancel', String(confirm('ISSUE816_CONFIRM_CANCEL')))">confirm cancel</button>
    <button id="prompt-accept-button" onclick="record('prompt-accept', String(prompt('ISSUE816_PROMPT_ACCEPT', 'seed')))">prompt accept</button>
    <button id="prompt-cancel-button" onclick="record('prompt-cancel', String(prompt('ISSUE816_PROMPT_CANCEL', 'seed')))">prompt cancel</button>
    <button id="beforeunload-arm-button" onclick="window.onbeforeunload = (event) => { event.preventDefault(); event.returnValue = 'ISSUE816_BEFOREUNLOAD'; return 'ISSUE816_BEFOREUNLOAD'; }; record('beforeunload-armed', 'true')">arm beforeunload</button>
    <a id="beforeunload-leave-link" href="/away.html">leave page</a>
  </body>
</html>
EOF
  cat >"$DIALOG_WEB_ROOT/away.html" <<'EOF'
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>Issue 26061712000816 Dialog away loaded</title>
    <script>
      console.log("ISSUE816_DIALOG_RESULT case=beforeunload-proceed value=away");
    </script>
  </head>
  <body>ISSUE816_DIALOG_AWAY</body>
</html>
EOF
  python3 -m http.server "$DIALOG_HTTP_PORT" --bind 127.0.0.1 --directory "$DIALOG_WEB_ROOT" >>"$HARNESS_LOG" 2>&1 &
  HTTP_PID="$!"
  URL="http://127.0.0.1:${DIALOG_HTTP_PORT}/initial-alert.html"
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
  python3 - "$URL" <<'PY' >/dev/null 2>&1 || fail "JavaScript dialog HTTP fixture did not become ready"
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY
  log "javascript_dialog_web_root=$DIALOG_WEB_ROOT"
  log "javascript_dialog_http_pid=$HTTP_PID"
  log "javascript_dialog_url=$URL"
fi

if [ "$SCENARIO" = "http-auth-smoke" ]; then
  AUTH_HTTP_PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
PY
)"
  mkdir -p "$AUTH_WEB_ROOT"
  cat >"$AUTH_WEB_ROOT/server.py" <<'PY'
import base64
import http.server
import socketserver
import sys

PORT = int(sys.argv[1])
EXPECTED = "Basic " + base64.b64encode(b"user:passwd").decode("ascii")

class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        sys.stderr.write("%s\n" % (fmt % args))

    def send_html(self, title, body):
        data = f"""<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>{title}</title>
    <style>
      body {{ margin: 0; font: 16px -apple-system, BlinkMacSystemFont, sans-serif; }}
      a {{ position: absolute; left: 32px; }}
      #cancel-link {{ top: 48px; }}
      #public-link {{ top: 98px; }}
    </style>
    <script>console.log("{body}");</script>
  </head>
  <body>
    <div>{body}</div>
    <a id="cancel-link" href="/auth-cancel/cancel.html">ISSUE816_AUTH_CANCEL_LINK</a>
    <a id="public-link" href="/public.html">ISSUE816_AUTH_PUBLIC_LINK</a>
  </body>
</html>
""".encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path.startswith("/auth/") or self.path.startswith("/auth-cancel/"):
            if self.headers.get("Authorization") != EXPECTED:
                realm = "Issue816Cancel" if self.path.startswith("/auth-cancel/") else "Issue816Auth"
                self.send_response(401)
                self.send_header("WWW-Authenticate", f'Basic realm="{realm}"')
                self.send_header("Content-Length", "0")
                self.end_headers()
                return
            marker = "ISSUE816_AUTH_CANCEL_UNEXPECTED_SUCCESS" if self.path.startswith("/auth-cancel/") else "ISSUE816_AUTH_SUCCESS"
            title = "Issue 26061712000816 Auth Cancel Unexpected Success" if self.path.startswith("/auth-cancel/") else "Issue 26061712000816 Auth Success"
            self.send_html(title, marker)
            return
        if self.path.startswith("/public.html"):
            self.send_html("Issue 26061712000816 Auth Public", "ISSUE816_AUTH_PUBLIC")
            return
        self.send_html("Issue 26061712000816 Auth Index", "ISSUE816_AUTH_INDEX")

with socketserver.TCPServer(("127.0.0.1", PORT), Handler) as httpd:
    httpd.serve_forever()
PY
  python3 "$AUTH_WEB_ROOT/server.py" "$AUTH_HTTP_PORT" >>"$HARNESS_LOG" 2>&1 &
  HTTP_PID="$!"
  URL="http://127.0.0.1:${AUTH_HTTP_PORT}/auth/success.html"
  for _ in $(seq 1 30); do
    if python3 - "http://127.0.0.1:${AUTH_HTTP_PORT}/public.html" <<'PY' >/dev/null 2>&1
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
  python3 - "http://127.0.0.1:${AUTH_HTTP_PORT}/public.html" <<'PY' >/dev/null 2>&1 || fail "HTTP auth fixture did not become ready"
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY
  log "http_auth_web_root=$AUTH_WEB_ROOT"
  log "http_auth_http_pid=$HTTP_PID"
  log "http_auth_url=$URL"
fi

if [ "$SCENARIO" = "renderer-crash-smoke" ]; then
  CRASH_HTTP_PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
PY
)"
  mkdir -p "$CRASH_WEB_ROOT"
  cat >"$CRASH_WEB_ROOT/index.html" <<'EOF'
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>Issue 26061712000816 Crash Initial</title>
    <style>
      html,
      body {
        margin: 0;
        background: #ffffff;
        color: #111111;
        font: 16px -apple-system, BlinkMacSystemFont, sans-serif;
      }
    </style>
    <script>
      console.log("ISSUE816_CRASH_INITIAL_READY");
      window.addEventListener("load", () => {
        document.title = "Issue 26061712000816 Crash Initial Ready";
      });
    </script>
  </head>
  <body>ISSUE816_CRASH_INITIAL</body>
</html>
EOF
  cat >"$CRASH_WEB_ROOT/recovery.html" <<'EOF'
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>Issue 26061712000816 Crash Recovery</title>
    <style>
      html,
      body {
        margin: 0;
        background: #ffffff;
        color: #111111;
        font: 16px -apple-system, BlinkMacSystemFont, sans-serif;
      }
    </style>
    <script>
      console.log("ISSUE816_CRASH_RECOVERY");
      window.addEventListener("load", () => {
        document.body.dataset.issue816Recovery = "ready";
      });
    </script>
  </head>
  <body>ISSUE816_CRASH_RECOVERY_BODY</body>
</html>
EOF
  python3 -m http.server "$CRASH_HTTP_PORT" --bind 127.0.0.1 --directory "$CRASH_WEB_ROOT" >>"$HARNESS_LOG" 2>&1 &
  HTTP_PID="$!"
  URL="http://127.0.0.1:${CRASH_HTTP_PORT}/index.html"
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
  python3 - "$URL" <<'PY' >/dev/null 2>&1 || fail "renderer crash HTTP fixture did not become ready"
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY
  log "renderer_crash_web_root=$CRASH_WEB_ROOT"
  log "renderer_crash_http_pid=$HTTP_PID"
  log "renderer_crash_url=$URL"
fi

if [ "$SCENARIO" = "color-scheme-smoke" ]; then
  COLOR_HTTP_PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
PY
)"
  mkdir -p "$COLOR_WEB_ROOT"
  cat >"$COLOR_WEB_ROOT/index.html" <<'EOF'
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>Issue 26061712000816 Color Initial</title>
    <style>
      html,
      body {
        margin: 0;
        background: Canvas;
        color: CanvasText;
        font: 16px -apple-system, BlinkMacSystemFont, sans-serif;
      }
    </style>
    <script>
      const query = window.matchMedia("(prefers-color-scheme: dark)");
      let seq = 0;

      function report(label) {
        const scheme = query.matches ? "dark" : "light";
        seq += 1;
        const marker = `ISSUE816_COLOR_SCHEME seq=${seq} label=${label} scheme=${scheme}`;
        console.log(marker);
        document.title = `Issue 26061712000816 Color ${label} ${scheme} ${seq}`;
        document.body.dataset.scheme = scheme;
      }

      query.addEventListener("change", () => report("change"));
      window.addEventListener("load", () => {
        report("ready");
        setInterval(() => report("poll"), 1000);
      });
    </script>
  </head>
  <body>ISSUE816_COLOR_SCHEME_BODY</body>
</html>
EOF
  python3 -m http.server "$COLOR_HTTP_PORT" --bind 127.0.0.1 --directory "$COLOR_WEB_ROOT" >>"$HARNESS_LOG" 2>&1 &
  HTTP_PID="$!"
  URL="http://127.0.0.1:${COLOR_HTTP_PORT}/index.html"
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
  python3 - "$URL" <<'PY' >/dev/null 2>&1 || fail "color scheme HTTP fixture did not become ready"
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY
  log "color_scheme_web_root=$COLOR_WEB_ROOT"
  log "color_scheme_http_pid=$HTTP_PID"
  log "color_scheme_url=$URL"
fi

if [ "$SCENARIO" = "copy-current-url-smoke" ]; then
  COPY_URL_HTTP_PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
PY
)"
  mkdir -p "$COPY_URL_WEB_ROOT"
  cat >"$COPY_URL_WEB_ROOT/index.html" <<'EOF'
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>Issue 26061712000816 Copy URL Initial</title>
    <style>
      html,
      body {
        margin: 0;
        background: #ffffff;
        color: #111111;
        font: 16px -apple-system, BlinkMacSystemFont, sans-serif;
      }
    </style>
    <script>
      console.log(`ISSUE816_COPY_URL_READY ${window.location.href}`);
      window.addEventListener("load", () => {
        document.title = "Issue 26061712000816 Copy URL Ready";
      });
    </script>
  </head>
  <body>ISSUE816_COPY_URL_BODY</body>
</html>
EOF
  cp "$COPY_URL_WEB_ROOT/index.html" "$COPY_URL_WEB_ROOT/copy-${TS}.html"
  python3 -m http.server "$COPY_URL_HTTP_PORT" --bind 127.0.0.1 --directory "$COPY_URL_WEB_ROOT" >>"$HARNESS_LOG" 2>&1 &
  HTTP_PID="$!"
  URL="http://127.0.0.1:${COPY_URL_HTTP_PORT}/copy-${TS}.html"
  for _ in $(seq 1 30); do
    if python3 - "http://127.0.0.1:${COPY_URL_HTTP_PORT}/index.html" <<'PY' >/dev/null 2>&1
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
  python3 - "http://127.0.0.1:${COPY_URL_HTTP_PORT}/index.html" <<'PY' >/dev/null 2>&1 || fail "copy-current-url HTTP fixture did not become ready"
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY
  log "copy_current_url_web_root=$COPY_URL_WEB_ROOT"
  log "copy_current_url_http_pid=$HTTP_PID"
  log "copy_current_url_url=$URL"
fi

if [ "$SCENARIO" = "browser-input-granularity" ] || [ "$SCENARIO" = "click-driven-browse-mode" ]; then
  INPUT_HTTP_PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
PY
)"
  mkdir -p "$INPUT_WEB_ROOT"
  cat >"$INPUT_WEB_ROOT/index.html" <<'EOF'
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>Issue 26061712000817 Input Initial</title>
    <style>
      html,
      body {
        margin: 0;
        background: #ffffff;
        color: #111111;
        font: 16px -apple-system, BlinkMacSystemFont, sans-serif;
      }
      #text-input {
        position: absolute;
        left: 60px;
        top: 60px;
        width: 320px;
        height: 32px;
        font: 18px -apple-system, BlinkMacSystemFont, sans-serif;
      }
      #next-control {
        position: absolute;
        left: 60px;
        top: 112px;
        width: 220px;
        height: 32px;
      }
      #click-zone {
        position: absolute;
        left: 60px;
        top: 168px;
        width: 320px;
        height: 48px;
        border: 1px solid #333333;
        line-height: 48px;
        user-select: none;
      }
      #drag-input {
        position: absolute;
        left: 60px;
        top: 248px;
        width: 420px;
        height: 36px;
        font: 18px -apple-system, BlinkMacSystemFont, sans-serif;
        line-height: 36px;
      }
      #result {
        position: absolute;
        left: 60px;
        top: 320px;
      }
    </style>
    <script>
      let seq = 0;

      function safe(value) {
        return String(value).replace(/[^A-Za-z0-9_.:-]/g, "_");
      }

      function report(kind, detail = "") {
        seq += 1;
        const marker = `ISSUE817_INPUT seq=${seq} kind=${kind} ${detail}`.trim();
        console.log(marker);
        document.title = `Issue 26061712000817 Input ${kind} ${seq}`;
        const result = document.getElementById("result");
        if (result) result.textContent = marker;
      }

      function inputState(label) {
        const input = document.getElementById("text-input");
        return `label=${label} value=${safe(input.value)} start=${input.selectionStart} end=${input.selectionEnd} active=${safe(document.activeElement.id)}`;
      }

      window.addEventListener("DOMContentLoaded", () => {
        const input = document.getElementById("text-input");
        const next = document.getElementById("next-control");
        const clickZone = document.getElementById("click-zone");
        const dragInput = document.getElementById("drag-input");

        input.addEventListener("focus", () => report("focus", inputState("input-focus")));
        input.addEventListener("click", () => report("input-click", inputState("input-click")));
        input.addEventListener("input", () => report("input", inputState("input")));
        input.addEventListener("keyup", (event) => {
          report("keyup", `${inputState(safe(event.key))} key=${safe(event.key)}`);
        });
        next.addEventListener("focus", () => report("focus", `active=${safe(document.activeElement.id)}`));
        next.addEventListener("click", () => report("enter", `active=${safe(document.activeElement.id)}`));

        clickZone.addEventListener("click", (event) => {
          report("click", `detail=${event.detail} shift=${event.shiftKey} meta=${event.metaKey} alt=${event.altKey} ctrl=${event.ctrlKey}`);
        });
        clickZone.addEventListener("dblclick", (event) => {
          report("dblclick", `detail=${event.detail}`);
        });

        dragInput.addEventListener("mouseup", () => {
          setTimeout(() => {
            const text = dragInput.value.substring(dragInput.selectionStart, dragInput.selectionEnd);
            report("selection", `text=${safe(text)} start=${dragInput.selectionStart} end=${dragInput.selectionEnd} active=${safe(document.activeElement.id)}`);
          }, 0);
        });

        document.addEventListener("copy", () => {
          setTimeout(() => {
            const active = document.activeElement;
            let text = window.getSelection().toString();
            if (active && active.id === "drag-input") {
              text = active.value.substring(active.selectionStart, active.selectionEnd);
            }
            report("copy", `text=${safe(text)} active=${safe(active ? active.id : "")}`);
          }, 0);
        });

        report("ready", "url=" + safe(window.location.href));
      });
    </script>
  </head>
  <body>
    <input id="text-input" autocomplete="off" spellcheck="false" />
    <button id="next-control">ISSUE817 next control</button>
    <div id="click-zone">ISSUE817_CLICK_ZONE</div>
    <input id="drag-input" readonly value="ISSUE817_BROWSER_DRAG_TEXT" />
    <div id="result">ISSUE817_INPUT_BOOT</div>
  </body>
</html>
EOF
  python3 -m http.server "$INPUT_HTTP_PORT" --bind 127.0.0.1 --directory "$INPUT_WEB_ROOT" >>"$HARNESS_LOG" 2>&1 &
  HTTP_PID="$!"
  URL="http://127.0.0.1:${INPUT_HTTP_PORT}/index.html"
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
  python3 - "$URL" <<'PY' >/dev/null 2>&1 || fail "browser-input HTTP fixture did not become ready"
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY
  log "browser_input_web_root=$INPUT_WEB_ROOT"
  log "browser_input_http_pid=$HTTP_PID"
  log "browser_input_url=$URL"
fi

if [ "$SCENARIO" = "webapp-clipboard-copy" ]; then
  CLIPBOARD_HTTP_PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
PY
)"
  mkdir -p "$CLIPBOARD_WEB_ROOT"
  cat >"$CLIPBOARD_WEB_ROOT/index.html" <<'EOF'
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>Issue 26062412000840 Clipboard Initial</title>
    <style>
      html,
      body {
        margin: 0;
        background: #ffffff;
        color: #111111;
        font: 16px -apple-system, BlinkMacSystemFont, sans-serif;
      }
      button {
        position: absolute;
        left: 60px;
        width: 360px;
        height: 40px;
        font: 16px -apple-system, BlinkMacSystemFont, sans-serif;
      }
      #clipboard-api-copy {
        top: 60px;
      }
      #exec-command-copy {
        top: 120px;
      }
      #fallback-input {
        position: absolute;
        left: 60px;
        top: 180px;
        width: 360px;
        height: 32px;
        font: 16px -apple-system, BlinkMacSystemFont, sans-serif;
      }
      #result {
        position: absolute;
        left: 60px;
        top: 240px;
      }
    </style>
    <script>
      let seq = 0;
      const clipboardToken = "ISSUE840_CLIPBOARD_API_TOKEN";
      const execToken = "ISSUE840_EXEC_COMMAND_TOKEN";

      function safe(value) {
        return String(value).replace(/[^A-Za-z0-9_.:-]/g, "_");
      }

      function report(kind, detail = "") {
        seq += 1;
        const marker = `ISSUE840_CLIPBOARD seq=${seq} kind=${kind} ${detail}`.trim();
        console.log(marker);
        document.title = `Issue 26062412000840 Clipboard ${kind} ${seq}`;
        const result = document.getElementById("result");
        if (result) result.textContent = marker;
      }

      async function clipboardApiCopy() {
        report("clipboard-api-start", `token=${clipboardToken} active=${safe(document.activeElement.id)}`);
        try {
          await navigator.clipboard.writeText(clipboardToken);
          report("clipboard-api-success", `token=${clipboardToken}`);
        } catch (error) {
          report("clipboard-api-error", `name=${safe(error && error.name)} message=${safe(error && error.message)}`);
        }
      }

      function execCommandCopy() {
        report("exec-command-start", `token=${execToken} active=${safe(document.activeElement.id)}`);
        const input = document.getElementById("fallback-input");
        input.value = execToken;
        input.focus();
        input.select();
        try {
          const ok = document.execCommand("copy");
          report(ok ? "exec-command-success" : "exec-command-false", `token=${execToken}`);
        } catch (error) {
          report("exec-command-error", `name=${safe(error && error.name)} message=${safe(error && error.message)}`);
        }
      }

      window.addEventListener("DOMContentLoaded", () => {
        document.getElementById("clipboard-api-copy").addEventListener("click", clipboardApiCopy);
        document.getElementById("exec-command-copy").addEventListener("click", execCommandCopy);
        document.addEventListener("copy", () => {
          const active = document.activeElement;
          const selected = active && "selectionStart" in active
            ? active.value.substring(active.selectionStart, active.selectionEnd)
            : window.getSelection().toString();
          setTimeout(() => report("copy-event", `text=${safe(selected)} active=${safe(active ? active.id : "")}`), 0);
        });
        report("ready", `secure=${window.isSecureContext} clipboard=${Boolean(navigator.clipboard)}`);
      });
    </script>
  </head>
  <body>
    <button id="clipboard-api-copy">Copy with navigator.clipboard.writeText</button>
    <button id="exec-command-copy">Copy with document.execCommand</button>
    <input id="fallback-input" autocomplete="off" spellcheck="false" />
    <div id="result">ISSUE840_CLIPBOARD_BOOT</div>
  </body>
</html>
EOF
  python3 -m http.server "$CLIPBOARD_HTTP_PORT" --bind 127.0.0.1 --directory "$CLIPBOARD_WEB_ROOT" >>"$HARNESS_LOG" 2>&1 &
  HTTP_PID="$!"
  URL="http://127.0.0.1:${CLIPBOARD_HTTP_PORT}/index.html"
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
  python3 - "$URL" <<'PY' >/dev/null 2>&1 || fail "webapp clipboard HTTP fixture did not become ready"
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY
  log "webapp_clipboard_web_root=$CLIPBOARD_WEB_ROOT"
  log "webapp_clipboard_http_pid=$HTTP_PID"
  log "webapp_clipboard_url=$URL"
fi

if [ "$SCENARIO" = "multi-profile-isolation" ]; then
  PROFILE_HTTP_PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
PY
)"
  mkdir -p "$PROFILE_WEB_ROOT"
  cat >"$PROFILE_WEB_ROOT/index.html" <<'EOF'
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>Issue 26061712000818 Profile Fixture</title>
  </head>
  <body>
    <input id="profile-input" value="ISSUE818_PROFILE_INPUT">
    <script>
      const params = new URLSearchParams(location.search);
      const profile = params.get("profile") || "unknown";
      const key = "issue818-profile-marker";
      const before = localStorage.getItem(key) || "none";
      localStorage.setItem(key, profile);
      const after = localStorage.getItem(key) || "none";
      console.log(`ISSUE818_PROFILE_STORAGE profile=${profile} before=${before} after=${after}`);
      document.title = `Issue 26061712000818 Profile ${profile} before ${before} after ${after}`;
      document.getElementById("profile-input").focus();
    </script>
  </body>
</html>
EOF
  python3 -m http.server "$PROFILE_HTTP_PORT" --bind 127.0.0.1 --directory "$PROFILE_WEB_ROOT" >>"$HARNESS_LOG" 2>&1 &
  HTTP_PID="$!"
  PROFILE_URL_A="http://127.0.0.1:${PROFILE_HTTP_PORT}/index.html?profile=profilea"
  PROFILE_URL_B="http://127.0.0.1:${PROFILE_HTTP_PORT}/index.html?profile=profileb"
  URL="$PROFILE_URL_A"
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
  python3 - "$URL" <<'PY' >/dev/null 2>&1 || fail "multi-profile HTTP fixture did not become ready"
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY
  log "profile_web_root=$PROFILE_WEB_ROOT"
  log "profile_http_pid=$HTTP_PID"
  log "profile_url_a=$PROFILE_URL_A"
  log "profile_url_b=$PROFILE_URL_B"
fi

COMMAND="$RUN_DIR/run-web.sh"
CONFIG="$RUN_DIR/config"
RELEASE_XDG_CONFIG_HOME="$RUN_DIR/release-xdg"
WINDOW_BOUNDS="$RUN_DIR/window-bounds.swift"
APP_WINDOWS="$RUN_DIR/app-windows.swift"
DISPLAY_INVENTORY="$RUN_DIR/display-inventory.swift"
ACTIVATE_APP="$RUN_DIR/activate-app.swift"
FOCUS_WINDOW="$RUN_DIR/focus-window.swift"
FULLSCREEN_WINDOW="$RUN_DIR/fullscreen-window.swift"
MINIMIZE_WINDOW="$RUN_DIR/minimize-window.swift"
HIDE_APP="$RUN_DIR/hide-app.swift"
RESIZE_WINDOW="$RUN_DIR/resize-window.swift"
cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
exec "$WEB" --browser "$CHROMIUM" "$URL"
EOF
chmod +x "$COMMAND"

if [ "$SCENARIO" = "multi-profile-isolation" ]; then
  cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
exec "$WEB" --browser "$CHROMIUM" --profile profilea "$PROFILE_URL_A"
EOF
  chmod +x "$COMMAND"
fi

if [ "$SCENARIO" = "named-chromium-debug-launch" ] || [ "$SCENARIO" = "named-chromium-invalid-env" ] || [ "$SCENARIO" = "installed-chromium-release-launch" ]; then
  cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
exec "$WEB" "$URL"
EOF
  chmod +x "$COMMAND"
fi

if [ "$SCENARIO" = "hello-config-homepage" ]; then
  cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
exec "$WEB"
EOF
  chmod +x "$COMMAND"
fi

if [ "$SCENARIO" = "ghostboard-config-paths" ]; then
  cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
exec "$WEB"
EOF
  chmod +x "$COMMAND"
fi

if [ "$SCENARIO" = "hello-config-browser-list" ] || [ "$SCENARIO" = "hello-empty-browser-list" ]; then
  cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
exec "$WEB" "$URL"
EOF
  chmod +x "$COMMAND"
fi

if [ "$SCENARIO" = "named-chromium-invalid-env" ]; then
  CHROMIUM_PATH_FOR_APP="chromium"
fi

if [ "$SCENARIO" = "launch-discovery-contract" ]; then
  NAMED_COMMAND="$RUN_DIR/run-web-named.sh"
  INVALID_ENV_COMMAND="$RUN_DIR/run-web-invalid-env.sh"
  cat >"$NAMED_COMMAND" <<EOF
#!/usr/bin/env bash
exec "$WEB" "$URL"
EOF
  cat >"$INVALID_ENV_COMMAND" <<EOF
#!/usr/bin/env bash
exec "$WEB" "$URL"
EOF
  chmod +x "$NAMED_COMMAND" "$INVALID_ENV_COMMAND"
  log "scenario=$SCENARIO"
  log "run_dir=$RUN_DIR"
  log "app=$APP"
  log "web=$WEB"
  log "chromium=$CHROMIUM"
  log "url=$URL"
  log "absolute_command=$COMMAND"
  log "named_command=$NAMED_COMMAND"
  log "invalid_env_command=$INVALID_ENV_COMMAND"
  log "named_chromium_env=$CHROMIUM"
  log "invalid_chromium_env=chromium"
  grep -F -- "exec \"$WEB\" --browser \"$CHROMIUM\" \"$URL\"" "$COMMAND" >/dev/null 2>&1 || fail "absolute launch command does not use explicit --browser path"
  log "PASS: absolute launch command uses explicit --browser path"
  grep -F -- "--browser" "$NAMED_COMMAND" >/dev/null 2>&1 && fail "named launch command unexpectedly contains --browser"
  grep -F -- "exec \"$WEB\" \"$URL\"" "$NAMED_COMMAND" >/dev/null 2>&1 || fail "named launch command does not omit --browser"
  log "PASS: named/default launch command omits --browser"
  [ "${CHROMIUM:0:1}" = "/" ] || fail "debug Chromium path is not absolute: $CHROMIUM"
  log "PASS: named Chromium debug env is absolute"
  grep -F -- "--browser" "$INVALID_ENV_COMMAND" >/dev/null 2>&1 && fail "invalid-env command unexpectedly contains --browser"
  [ "chromium" != "${CHROMIUM}" ] || fail "invalid-env sentinel equals debug Chromium helper path"
  case "chromium" in
    /*) fail "invalid-env sentinel is unexpectedly absolute" ;;
  esac
  log "PASS: invalid named Chromium env sentinel is relative"
  log "PASS: scenario launch-discovery-contract"
  exit 0
fi

if [ "$SCENARIO" = "terminal-scrollback-movement" ]; then
  cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
for i in \$(seq 1 180); do
  printf 'ISSUE809_EXP22_SCROLLBACK_HISTORY_%03d\\n' "\$i"
done
exec "$WEB" --primary-screen --browser "$CHROMIUM" "$URL"
EOF
  chmod +x "$COMMAND"
fi

if [ "$SCENARIO" = "new-terminal-tab-visibility" ] || [ "$SCENARIO" = "open-browser-in-new-tab" ] || [ "$SCENARIO" = "close-browser-tab" ] || [ "$SCENARIO" = "same-profile-server-lifecycle" ] || [ "$SCENARIO" = "tui-disconnect-reconnect" ] || [ "$SCENARIO" = "visible-profile-identity" ] || [ "$SCENARIO" = "open-browser-in-new-window" ] || [ "$SCENARIO" = "multiple-windows-with-browsers" ] || [ "$SCENARIO" = "keyboard-after-tab-window-switch" ] || [ "$SCENARIO" = "gui-active-multi-tab" ] || [ "$SCENARIO" = "devtools-singleton-guard" ] || [ "$SCENARIO" = "multi-profile-isolation" ]; then
  FIRST_RUN_MARKER="$RUN_DIR/first-web-ran"
  cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
set -euo pipefail
if mkdir "$FIRST_RUN_MARKER" 2>/dev/null; then
  exec "$WEB" --browser "$CHROMIUM" "$URL"
fi
printf 'new-tab-command invocation pid=%s\\n' "\$\$" >>"$NEW_TAB_COMMAND_LOG"
exec /bin/zsh -f -c 'printf "ISSUE809_EXP12_NEW_TAB_READY\\n"; while :; do sleep 60; done'
EOF
  chmod +x "$COMMAND"
fi

if [ "$SCENARIO" = "multi-profile-isolation" ]; then
  cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
set -euo pipefail
if mkdir "$FIRST_RUN_MARKER" 2>/dev/null; then
  exec "$WEB" --browser "$CHROMIUM" --profile profilea "$PROFILE_URL_A"
fi
printf 'new-tab-command invocation pid=%s\\n' "\$\$" >>"$NEW_TAB_COMMAND_LOG"
exec /bin/zsh -f -c 'printf "ISSUE809_EXP12_NEW_TAB_READY\\n"; while :; do sleep 60; done'
EOF
  chmod +x "$COMMAND"
fi

if [ "$SCENARIO" = "same-profile-server-lifecycle" ] || [ "$SCENARIO" = "tui-disconnect-reconnect" ]; then
  cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
set -euo pipefail
if mkdir "$FIRST_RUN_MARKER" 2>/dev/null; then
  exec "$WEB" --browser "$CHROMIUM" --profile default "$URL"
fi
printf 'new-tab-command invocation pid=%s\\n' "\$\$" >>"$NEW_TAB_COMMAND_LOG"
exec /bin/zsh -f -c 'printf "ISSUE809_EXP12_NEW_TAB_READY\\n"; while :; do sleep 60; done'
EOF
  chmod +x "$COMMAND"
fi

cat >"$CONFIG" <<EOF
window-save-state = never
initial-command = direct:$COMMAND
EOF

if [ "$SCENARIO" = "hello-config-homepage" ]; then
  cat >>"$CONFIG" <<EOF
homepage = "$HELLO_CONFIG_HOMEPAGE"
EOF
fi

if [ "$SCENARIO" = "hello-config-browser-list" ]; then
  cat >>"$CONFIG" <<EOF
browser = chromium
browser = $HELLO_CONFIG_SECOND_BROWSER
EOF
fi

if [ "$SCENARIO" = "split-right-border-config" ]; then
  case "$BORDER_CONFIG_CASE" in
    enabled)
      cat >>"$CONFIG" <<'EOF'
focused-split-border-color = 7dcfff
unfocused-split-border-color = 565f89
split-border-width = 2
unfocused-split-saturation = 0.5
EOF
      ;;
    clamp)
      cat >>"$CONFIG" <<'EOF'
focused-split-border-color = 7dcfff
unfocused-split-border-color = 565f89
split-border-width = 99
unfocused-split-saturation = 99
EOF
      ;;
    disabled)
      cat >>"$CONFIG" <<'EOF'
focused-split-border-color = 7dcfff
unfocused-split-border-color = 565f89
split-border-width = 0
unfocused-split-saturation = 0.5
EOF
      ;;
    missing-focused)
      cat >>"$CONFIG" <<'EOF'
unfocused-split-border-color = 565f89
split-border-width = 2
unfocused-split-saturation = 0.5
EOF
      ;;
    missing-unfocused)
      cat >>"$CONFIG" <<'EOF'
focused-split-border-color = 7dcfff
split-border-width = 2
unfocused-split-saturation = 0.5
EOF
      ;;
    *)
      fail "unsupported TERMSURF_SPLIT_BORDER_CONFIG_CASE: $BORDER_CONFIG_CASE"
      ;;
  esac
fi

if [ "$SCENARIO" = "hello-empty-browser-list" ]; then
  cat >>"$CONFIG" <<'EOF'
browser = ""
EOF
fi

if [ "$SCENARIO" != "ghostboard-config-paths" ]; then
  mkdir -p "$RELEASE_XDG_CONFIG_HOME/astrohacker/terminal"
  cp "$CONFIG" "$RELEASE_XDG_CONFIG_HOME/astrohacker/terminal/config"
fi

if [ "$SCENARIO" = "ghostboard-config-paths" ]; then
  write_config_path_case_config() {
    local path="$1"
    local command="$2"
    local homepage="$3"
    mkdir -p "$(dirname "$path")"
    cat >"$path" <<EOF
window-save-state = never
initial-command = direct:$command
homepage = "$homepage"
EOF
  }

  wait_for_config_path_log() {
    local needle="$1"
    local label="$2"
    local attempts="${3:-30}"
    for _ in $(seq 1 "$attempts"); do
      if grep -F -- "$needle" "$APP_LOG" >/dev/null 2>&1; then
        log "PASS: $label"
        return 0
      fi
      delay 1
    done
    fail "timed out waiting for $label"
  }

  require_config_path_log() {
    local needle="$1"
    local label="$2"
    if grep -F -- "$needle" "$APP_LOG" >/dev/null 2>&1; then
      log "PASS: $label"
    else
      fail "missing $label"
    fi
  }

  require_no_config_path_log() {
    local pattern="$1"
    local label="$2"
    if grep -E "$pattern" "$APP_LOG" >/dev/null 2>&1; then
      fail "$label"
    fi
    log "PASS: $label"
  }

  run_config_path_case() {
    local label="$1"
    local mode="$2"
    local expected_path="$3"
    local expected_homepage="$4"
    local case_dir="$RUN_DIR/config-paths-$label"
    local case_home="$case_dir/home"
    local case_xdg="$case_dir/xdg"
    local case_command="$case_dir/run-web.sh"
    local explicit_config="$case_dir/explicit-config"
    local xdg_astrohacker_terminal_config="$case_xdg/astrohacker/terminal/config"
    local xdg_ghostty_config="$case_xdg/ghostty/config.ghostty"
    local app_support_ghostty_config="$case_home/Library/Application Support/com.mitchellh.ghostty/config.ghostty"
    local app_support_termsurf_config_ghostty="$case_home/Library/Application Support/com.termsurf/config.ghostty"
    local app_support_termsurf_config="$case_home/Library/Application Support/com.termsurf/config"
    local app_support_ghostboard_config_ghostty="$case_home/Library/Application Support/com.termsurf.ghostboard/config.ghostty"
    local app_support_ghostboard_config="$case_home/Library/Application Support/com.termsurf.ghostboard/config"
    local sentinel_prefix="https://example.net/issue819-exp5-$label"

    mkdir -p "$case_dir"
    cat >"$case_command" <<EOF
#!/usr/bin/env bash
exec "$WEB"
EOF
    chmod +x "$case_command"

    write_config_path_case_config "$explicit_config" "$case_command" "$sentinel_prefix-explicit"
    if [ "$mode" != "no-xdg" ]; then
      write_config_path_case_config "$xdg_astrohacker_terminal_config" "$case_command" "$sentinel_prefix-xdg-astrohacker-terminal"
    fi
    write_config_path_case_config "$xdg_ghostty_config" "$case_command" "$sentinel_prefix-xdg-ghostty"
    write_config_path_case_config "$app_support_ghostty_config" "$case_command" "$sentinel_prefix-appsupport-ghostty"
    write_config_path_case_config "$app_support_termsurf_config_ghostty" "$case_command" "$sentinel_prefix-appsupport-termsurf-config-ghostty"
    write_config_path_case_config "$app_support_termsurf_config" "$case_command" "$sentinel_prefix-appsupport-termsurf-config"
    write_config_path_case_config "$app_support_ghostboard_config_ghostty" "$case_command" "$sentinel_prefix-appsupport-ghostboard-config-ghostty"
    write_config_path_case_config "$app_support_ghostboard_config" "$case_command" "$sentinel_prefix-appsupport-ghostboard-config"

    APP_LOG="$LOG_DIR/ghostboard-geometry-${SCENARIO}-${label}-app-${TS}.log"
    WEBTUI_STATE_TRACE="$LOG_DIR/ghostboard-geometry-${SCENARIO}-${label}-webtui-${TS}.log"
    CHROMIUM_TRACE="$LOG_DIR/ghostboard-geometry-${SCENARIO}-${label}-chromium-${TS}.log"
    log "config_path_case=$label"
    log "config_path_case_app_log=$APP_LOG"
    log "config_path_case_expected_path=$expected_path"
    log "config_path_case_expected_homepage=$expected_homepage"

    if [ "$mode" = "explicit" ]; then
      env \
        HOME="$case_home" \
        XDG_CONFIG_HOME="$case_xdg" \
        GHOSTTY_CONFIG_PATH="$explicit_config" \
        GHOSTTY_LOG=stderr \
        TERMSURF_GEOMETRY_TRACE=1 \
        TERMSURF_GEOMETRY_SCENARIO="${SCENARIO}-${label}" \
        ASTROHACKER_CHROMIUM_PATH="$CHROMIUM_PATH_FOR_APP" \
        TERMSURF_WEBTUI_STATE_TRACE_FILE="$WEBTUI_STATE_TRACE" \
        TERMSURF_INPUT_TRACE=1 \
        TERMSURF_PDF_INPUT_TRACE=1 \
        TERMSURF_PDF_INPUT_TRACE_FILE="$CHROMIUM_TRACE" \
        "$APP_BIN" >"$APP_LOG" 2>&1 &
    else
      env \
        HOME="$case_home" \
        XDG_CONFIG_HOME="$case_xdg" \
        GHOSTTY_LOG=stderr \
        TERMSURF_GEOMETRY_TRACE=1 \
        TERMSURF_GEOMETRY_SCENARIO="${SCENARIO}-${label}" \
        ASTROHACKER_CHROMIUM_PATH="$CHROMIUM_PATH_FOR_APP" \
        TERMSURF_WEBTUI_STATE_TRACE_FILE="$WEBTUI_STATE_TRACE" \
        TERMSURF_INPUT_TRACE=1 \
        TERMSURF_PDF_INPUT_TRACE=1 \
        TERMSURF_PDF_INPUT_TRACE_FILE="$CHROMIUM_TRACE" \
        "$APP_BIN" >"$APP_LOG" 2>&1 &
    fi
    PID="$!"
    log "config_path_case_pid=$PID"

    if [ "$mode" = "no-xdg" ]; then
      delay 3
      if grep -F -- "reading configuration file path=" "$APP_LOG" >/dev/null 2>&1; then
        fail "$label unexpectedly loaded a config file"
      fi
      log "PASS: $label did not load a config file when current XDG config was absent"
      wait_for_config_path_log "creating template config file: path=$expected_path" "$label created default XDG config template" 10
      require_no_config_path_log "reading configuration file path=.*(ghostty/config\\.ghostty|Application Support)" "$label did not load inherited Ghostty or Application Support fallback paths"
      if grep -F -- "$sentinel_prefix-" "$APP_LOG" >/dev/null 2>&1; then
        fail "$label leaked a fallback sentinel homepage into app logs"
      fi
      log "PASS: $label did not consume any fallback sentinel homepage"
    else
      wait_for_config_path_log "reading configuration file path=$expected_path" "$label loaded expected config path" 45
      wait_for_config_path_log "TermSurf Hello config homepage=$expected_homepage browsers=chromium" "$label loaded expected homepage" 45
      wait_for_config_path_log "TermSurf HelloReply sent homepage=$expected_homepage browsers=chromium" "$label sent expected HelloReply homepage" 45
      wait_for_log "SetOverlay: pane_id=.* profile=default browser=chromium url=${expected_homepage}" "$label webtui consumed expected homepage" 45

      require_no_config_path_log "reading configuration file path=.*(ghostty/config\\.ghostty|Application Support)" "$label did not load inherited Ghostty or Application Support config paths"
      require_config_path_log "SetOverlay: named browser resolved browser=chromium env=ASTROHACKER_CHROMIUM_PATH path=${CHROMIUM}" "$label used debug Chromium helper resolver"
    fi

    kill "$PID" >/dev/null 2>&1 || true
    delay 0.5
    kill -9 "$PID" >/dev/null 2>&1 || true
    PID=""
  }

  log "scenario=$SCENARIO"
  log "run_dir=$RUN_DIR"
  log "app=$APP"
  log "web=$WEB"
  log "chromium=$CHROMIUM"
  run_config_path_case "explicit-env" "explicit" "$RUN_DIR/config-paths-explicit-env/explicit-config" "https://example.net/issue819-exp5-explicit-env-explicit"
  run_config_path_case "xdg-default" "default" "$RUN_DIR/config-paths-xdg-default/xdg/astrohacker/terminal/config" "https://example.net/issue819-exp5-xdg-default-xdg-astrohacker-terminal"
  run_config_path_case "no-current-xdg" "no-xdg" "$RUN_DIR/config-paths-no-current-xdg/xdg/astrohacker/terminal/config" ""
  log "PASS: scenario ghostboard-config-paths"
  exit 0
fi

if [ "$SCENARIO" = "new-terminal-tab-visibility" ] || [ "$SCENARIO" = "open-browser-in-new-tab" ] || [ "$SCENARIO" = "close-browser-tab" ] || [ "$SCENARIO" = "same-profile-server-lifecycle" ] || [ "$SCENARIO" = "tui-disconnect-reconnect" ] || [ "$SCENARIO" = "visible-profile-identity" ] || [ "$SCENARIO" = "keyboard-after-tab-window-switch" ] || [ "$SCENARIO" = "gui-active-multi-tab" ] || [ "$SCENARIO" = "devtools-singleton-guard" ] || [ "$SCENARIO" = "multi-profile-isolation" ]; then
  cat >>"$CONFIG" <<'EOF'
keybind = ctrl+t=new_tab
keybind = ctrl+1=goto_tab:1
keybind = ctrl+2=goto_tab:2
keybind = ctrl+p=previous_tab
keybind = ctrl+n=next_tab
EOF
fi

if [ "$SCENARIO" = "close-browser-tab" ] || [ "$SCENARIO" = "same-profile-server-lifecycle" ]; then
  cat >>"$CONFIG" <<'EOF'
confirm-close-surface = false
keybind = ctrl+w=close_tab
EOF
fi

if [ "$SCENARIO" = "open-browser-in-new-window" ] || [ "$SCENARIO" = "multiple-windows-with-browsers" ] || [ "$SCENARIO" = "keyboard-after-tab-window-switch" ]; then
  cat >>"$CONFIG" <<'EOF'
keybind = ctrl+b=new_window
EOF
fi

if [ "$SCENARIO" = "font-size-cell-metrics" ]; then
  cat >>"$CONFIG" <<'EOF'
keybind = ctrl+u=increase_font_size:2
keybind = ctrl+y=decrease_font_size:2
EOF
fi

if [ "$SCENARIO" = "terminal-scrollback-movement" ]; then
  cat >>"$CONFIG" <<'EOF'
scroll-to-bottom = no-keystroke, no-output
keybind = ctrl+u=scroll_page_up
keybind = ctrl+y=scroll_page_down
keybind = ctrl+b=scroll_to_bottom
EOF
fi

if [ "$SCENARIO" = "split-right" ] || [ "$SCENARIO" = "split-right-border-config" ] || [ "$SCENARIO" = "split-right-resize" ] || [ "$SCENARIO" = "split-right-equalize" ] || [ "$SCENARIO" = "split-right-zoom" ] || [ "$SCENARIO" = "split-right-close-sibling" ] || [ "$SCENARIO" = "split-right-focus-switch" ] || [ "$SCENARIO" = "two-browser-split-routing" ] || [ "$SCENARIO" = "mouse-after-geometry-change" ]; then
  cat >>"$CONFIG" <<'EOF'
keybind = ctrl+d=new_split:right
EOF
fi

if [ "$SCENARIO" = "split-right-close-sibling" ] || [ "$SCENARIO" = "two-browser-split-routing" ]; then
  cat >>"$CONFIG" <<'EOF'
confirm-close-surface = false
keybind = ctrl+k=close_surface
EOF
fi

if [ "$SCENARIO" = "split-right-close-browser-pane" ]; then
  cat >>"$CONFIG" <<'EOF'
confirm-close-surface = false
keybind = ctrl+d=new_split:right
keybind = ctrl+k=close_surface
EOF
fi

if [ "$SCENARIO" = "devtools-singleton-guard" ]; then
  cat >>"$CONFIG" <<'EOF'
confirm-close-surface = false
keybind = ctrl+k=close_surface
EOF
fi

if [ "$SCENARIO" = "split-right-resize" ] || [ "$SCENARIO" = "split-right-equalize" ] || [ "$SCENARIO" = "mouse-after-geometry-change" ]; then
  cat >>"$CONFIG" <<'EOF'
keybind = ctrl+l=resize_split:right,20
EOF
fi

if [ "$SCENARIO" = "split-right-equalize" ] || [ "$SCENARIO" = "mouse-after-geometry-change" ]; then
  cat >>"$CONFIG" <<'EOF'
keybind = ctrl+e=equalize_splits
EOF
fi

if [ "$SCENARIO" = "split-right-zoom" ]; then
  cat >>"$CONFIG" <<'EOF'
keybind = ctrl+z=toggle_split_zoom
EOF
fi

if [ "$SCENARIO" = "split-down" ]; then
  cat >>"$CONFIG" <<'EOF'
keybind = ctrl+j=new_split:down
EOF
fi

cat >"$DEVTOOLS_QUERY_PROBE" <<'PY'
#!/usr/bin/env python3
import socket
import struct
import sys


def varint(value):
    out = bytearray()
    value = int(value)
    while value >= 0x80:
        out.append((value & 0x7F) | 0x80)
        value >>= 7
    out.append(value)
    return bytes(out)


def read_varint(data, offset):
    value = 0
    shift = 0
    while True:
        if offset >= len(data):
            raise ValueError("truncated varint")
        byte = data[offset]
        offset += 1
        value |= (byte & 0x7F) << shift
        if byte < 0x80:
            return value, offset
        shift += 7


def field_varint(field, value):
    return varint((field << 3) | 0) + varint(value)


def field_string(field, value):
    payload = value.encode()
    return varint((field << 3) | 2) + varint(len(payload)) + payload


def read_exact(conn, size):
    chunks = bytearray()
    while len(chunks) < size:
        chunk = conn.recv(size - len(chunks))
        if not chunk:
            raise EOFError("socket closed")
        chunks.extend(chunk)
    return bytes(chunks)


def decode_reply(payload):
    reply = {"tab": 0, "error": "", "browser": "", "profile": ""}
    offset = 0
    while offset < len(payload):
        tag, offset = read_varint(payload, offset)
        field = tag >> 3
        wire = tag & 7
        if wire == 0:
            value, offset = read_varint(payload, offset)
            if field == 1:
                reply["tab"] = value
        elif wire == 2:
            size, offset = read_varint(payload, offset)
            value = payload[offset:offset + size]
            offset += size
            if field == 2:
                reply["error"] = value.decode()
            elif field == 3:
                reply["browser"] = value.decode()
            elif field == 4:
                reply["profile"] = value.decode()
        else:
            raise ValueError(f"unsupported reply wire type {wire}")
    return reply


def main():
    socket_path, pane_id, tab_id, profile, browser = sys.argv[1:6]
    request = b"".join(
        [
            field_string(1, pane_id),
            field_varint(2, int(tab_id)),
            field_string(3, profile),
            field_string(4, browser),
        ]
    )
    wrapper = varint((27 << 3) | 2) + varint(len(request)) + request

    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as conn:
        conn.connect(socket_path)
        conn.sendall(struct.pack("<I", len(wrapper)) + wrapper)
        size = struct.unpack("<I", read_exact(conn, 4))[0]
        data = read_exact(conn, size)

    offset = 0
    while offset < len(data):
        tag, offset = read_varint(data, offset)
        field = tag >> 3
        wire = tag & 7
        if wire != 2:
            raise ValueError(f"unexpected wrapper wire type {wire}")
        size, offset = read_varint(data, offset)
        payload = data[offset:offset + size]
        offset += size
        if field == 28:
            reply = decode_reply(payload)
            print(f"tab={reply['tab']}\terror={reply['error']}\tbrowser={reply['browser']}\tprofile={reply['profile']}")
            return
    raise ValueError("QueryDevtoolsReply not found")


if __name__ == "__main__":
    main()
PY
chmod +x "$DEVTOOLS_QUERY_PROBE"

cat >"$DEVTOOLS_OVERLAY_PROBE" <<'PY'
#!/usr/bin/env python3
import socket
import struct
import sys


def varint(value):
    out = bytearray()
    value = int(value)
    while value >= 0x80:
        out.append((value & 0x7F) | 0x80)
        value >>= 7
    out.append(value)
    return bytes(out)


def field_varint(field, value):
    return varint((field << 3) | 0) + varint(value)


def field_string(field, value):
    payload = value.encode()
    return varint((field << 3) | 2) + varint(len(payload)) + payload


def main():
    socket_path, pane_id, tab_id, profile, browser = sys.argv[1:6]
    request = b"".join(
        [
            field_string(1, pane_id),
            field_varint(2, 0),
            field_varint(3, 0),
            field_varint(4, 80),
            field_varint(5, 24),
            field_string(6, profile),
            field_varint(7, 1),
            field_varint(8, int(tab_id)),
            field_string(9, browser),
        ]
    )
    wrapper = varint((20 << 3) | 2) + varint(len(request)) + request

    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as conn:
        conn.connect(socket_path)
        conn.sendall(struct.pack("<I", len(wrapper)) + wrapper)


if __name__ == "__main__":
    main()
PY
chmod +x "$DEVTOOLS_OVERLAY_PROBE"

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

cat >"$APP_WINDOWS" <<'EOF'
import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 2,
      let targetPID = Int(CommandLine.arguments[1]),
      let info = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]]
else {
    exit(2)
}

for window in info {
    let pid = (window[kCGWindowOwnerPID as String] as? Int) ?? -1
    let layer = (window[kCGWindowLayer as String] as? Int) ?? -1
    let onscreen = (window[kCGWindowIsOnscreen as String] as? Bool) ?? false
    guard pid == targetPID, layer == 0, onscreen else { continue }
    guard let id = window[kCGWindowNumber as String] as? Int else { continue }
    let bounds = (window[kCGWindowBounds as String] as? [String: Any]) ?? [:]
    let x = Int((bounds["X"] as? Double) ?? 0)
    let y = Int((bounds["Y"] as? Double) ?? 0)
    let width = Int((bounds["Width"] as? Double) ?? 0)
    let height = Int((bounds["Height"] as? Double) ?? 0)
    guard width >= 50, height >= 50 else { continue }
    print("\(id)\t\(x)\t\(y)\t\(width)\t\(height)")
}
EOF

cat >"$DISPLAY_INVENTORY" <<'EOF'
import AppKit
import Foundation

for screen in NSScreen.screens {
    let id = (screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.intValue ?? -1
    let frame = screen.frame
    let visible = screen.visibleFrame
    let scale = screen.backingScaleFactor
    let isMain = screen == NSScreen.main
    print("\(id)\t\(Int(frame.origin.x))\t\(Int(frame.origin.y))\t\(Int(frame.size.width))\t\(Int(frame.size.height))\t\(Int(visible.origin.x))\t\(Int(visible.origin.y))\t\(Int(visible.size.width))\t\(Int(visible.size.height))\t\(scale)\t\(isMain ? "main" : "secondary")")
}
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

cat >"$FOCUS_WINDOW" <<'EOF'
import ApplicationServices
import Foundation

guard CommandLine.arguments.count == 6,
      let rawPID = Int32(CommandLine.arguments[1]),
      let targetX = Double(CommandLine.arguments[2]),
      let targetY = Double(CommandLine.arguments[3]),
      let targetWidth = Double(CommandLine.arguments[4]),
      let targetHeight = Double(CommandLine.arguments[5])
else {
    fputs("usage: focus-window.swift <pid> <x> <y> <width> <height>\n", stderr)
    exit(2)
}

guard AXIsProcessTrusted() else {
    fputs("accessibility permission is not trusted for window focus automation\n", stderr)
    exit(3)
}

func point(_ value: CFTypeRef?) -> CGPoint? {
    guard let value, CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
    var result = CGPoint.zero
    guard AXValueGetValue(value as! AXValue, .cgPoint, &result) else { return nil }
    return result
}

func size(_ value: CFTypeRef?) -> CGSize? {
    guard let value, CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
    var result = CGSize.zero
    guard AXValueGetValue(value as! AXValue, .cgSize, &result) else { return nil }
    return result
}

let app = AXUIElementCreateApplication(pid_t(rawPID))
var windowsValue: CFTypeRef?
let windowsResult = AXUIElementCopyAttributeValue(
    app,
    kAXWindowsAttribute as CFString,
    &windowsValue
)

guard windowsResult == .success,
      let windows = windowsValue as? [AXUIElement]
else {
    fputs("could not read target app windows: \(windowsResult.rawValue)\n", stderr)
    exit(4)
}

for window in windows {
    var positionValue: CFTypeRef?
    var sizeValue: CFTypeRef?
    guard AXUIElementCopyAttributeValue(window, kAXPositionAttribute as CFString, &positionValue) == .success,
          AXUIElementCopyAttributeValue(window, kAXSizeAttribute as CFString, &sizeValue) == .success,
          let position = point(positionValue),
          let windowSize = size(sizeValue)
    else { continue }

    let positionMatches = abs(Double(position.x) - targetX) <= 4 &&
        abs(Double(position.y) - targetY) <= 4
    let sizeMatches = abs(Double(windowSize.width) - targetWidth) <= 8 &&
        abs(Double(windowSize.height) - targetHeight) <= 8
    guard positionMatches && sizeMatches else { continue }

    _ = AXUIElementPerformAction(window, kAXRaiseAction as CFString)
    _ = AXUIElementSetAttributeValue(app, kAXFocusedWindowAttribute as CFString, window)
    _ = AXUIElementSetAttributeValue(window, kAXMainAttribute as CFString, kCFBooleanTrue)
    _ = AXUIElementSetAttributeValue(window, kAXFocusedAttribute as CFString, kCFBooleanTrue)
    Thread.sleep(forTimeInterval: 0.5)
    exit(0)
}

fputs("no matching accessibility window for bounds \(targetX),\(targetY) \(targetWidth)x\(targetHeight)\n", stderr)
exit(5)
EOF

cat >"$FULLSCREEN_WINDOW" <<'EOF'
import ApplicationServices
import Foundation

guard CommandLine.arguments.count == 7,
      let rawPID = Int32(CommandLine.arguments[1]),
      let targetX = Double(CommandLine.arguments[2]),
      let targetY = Double(CommandLine.arguments[3]),
      let targetWidth = Double(CommandLine.arguments[4]),
      let targetHeight = Double(CommandLine.arguments[5])
else {
    fputs("usage: fullscreen-window.swift <pid> <x> <y> <width> <height> <enter|exit|state>\n", stderr)
    exit(2)
}

let mode = CommandLine.arguments[6]
guard mode == "enter" || mode == "exit" || mode == "state" else {
    fputs("mode must be enter, exit, or state\n", stderr)
    exit(2)
}

guard AXIsProcessTrusted() else {
    fputs("accessibility permission is not trusted for fullscreen automation\n", stderr)
    exit(3)
}

func point(_ value: CFTypeRef?) -> CGPoint? {
    guard let value, CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
    var result = CGPoint.zero
    guard AXValueGetValue(value as! AXValue, .cgPoint, &result) else { return nil }
    return result
}

func size(_ value: CFTypeRef?) -> CGSize? {
    guard let value, CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
    var result = CGSize.zero
    guard AXValueGetValue(value as! AXValue, .cgSize, &result) else { return nil }
    return result
}

let app = AXUIElementCreateApplication(pid_t(rawPID))
var windowsValue: CFTypeRef?
let windowsResult = AXUIElementCopyAttributeValue(
    app,
    kAXWindowsAttribute as CFString,
    &windowsValue
)

guard windowsResult == .success,
      let windows = windowsValue as? [AXUIElement]
else {
    fputs("could not read target app windows: \(windowsResult.rawValue)\n", stderr)
    exit(4)
}

for window in windows {
    var positionValue: CFTypeRef?
    var sizeValue: CFTypeRef?
    guard AXUIElementCopyAttributeValue(window, kAXPositionAttribute as CFString, &positionValue) == .success,
          AXUIElementCopyAttributeValue(window, kAXSizeAttribute as CFString, &sizeValue) == .success,
          let position = point(positionValue),
          let windowSize = size(sizeValue)
    else { continue }

    let positionMatches = abs(Double(position.x) - targetX) <= 12 &&
        abs(Double(position.y) - targetY) <= 12
    let sizeMatches = abs(Double(windowSize.width) - targetWidth) <= 16 &&
        abs(Double(windowSize.height) - targetHeight) <= 16
    guard positionMatches && sizeMatches else { continue }

    var fullScreenValue: CFTypeRef?
    let stateResult = AXUIElementCopyAttributeValue(window, "AXFullScreen" as CFString, &fullScreenValue)
    let isFullscreen = stateResult == .success && ((fullScreenValue as? Bool) ?? false)
    if mode == "state" {
        print(isFullscreen ? "true" : "false")
        exit(0)
    }

    let target = mode == "enter"
    if isFullscreen != target {
        let setResult = AXUIElementSetAttributeValue(
            window,
            "AXFullScreen" as CFString,
            target ? kCFBooleanTrue : kCFBooleanFalse
        )
        guard setResult == .success else {
            fputs("failed to set AXFullScreen: \(setResult.rawValue)\n", stderr)
            exit(5)
        }
    }

    Thread.sleep(forTimeInterval: 1.5)
    var afterValue: CFTypeRef?
    let afterResult = AXUIElementCopyAttributeValue(window, "AXFullScreen" as CFString, &afterValue)
    guard afterResult == .success else {
        fputs("failed to read AXFullScreen after set: \(afterResult.rawValue)\n", stderr)
        exit(6)
    }
    print(((afterValue as? Bool) ?? false) ? "true" : "false")
    exit(0)
}

fputs("no matching accessibility window for bounds \(targetX),\(targetY) \(targetWidth)x\(targetHeight)\n", stderr)
exit(7)
EOF

cat >"$MINIMIZE_WINDOW" <<'EOF'
import ApplicationServices
import Foundation

guard CommandLine.arguments.count == 7,
      let rawPID = Int32(CommandLine.arguments[1]),
      let targetX = Double(CommandLine.arguments[2]),
      let targetY = Double(CommandLine.arguments[3]),
      let targetWidth = Double(CommandLine.arguments[4]),
      let targetHeight = Double(CommandLine.arguments[5])
else {
    fputs("usage: minimize-window.swift <pid> <x> <y> <width> <height> <minimize|restore|state>\n", stderr)
    exit(2)
}

let mode = CommandLine.arguments[6]
guard mode == "minimize" || mode == "restore" || mode == "state" else {
    fputs("mode must be minimize, restore, or state\n", stderr)
    exit(2)
}

guard AXIsProcessTrusted() else {
    fputs("accessibility permission is not trusted for minimize automation\n", stderr)
    exit(3)
}

func point(_ value: CFTypeRef?) -> CGPoint? {
    guard let value, CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
    var result = CGPoint.zero
    guard AXValueGetValue(value as! AXValue, .cgPoint, &result) else { return nil }
    return result
}

func size(_ value: CFTypeRef?) -> CGSize? {
    guard let value, CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
    var result = CGSize.zero
    guard AXValueGetValue(value as! AXValue, .cgSize, &result) else { return nil }
    return result
}

let app = AXUIElementCreateApplication(pid_t(rawPID))
var windowsValue: CFTypeRef?
let windowsResult = AXUIElementCopyAttributeValue(
    app,
    kAXWindowsAttribute as CFString,
    &windowsValue
)

guard windowsResult == .success,
      let windows = windowsValue as? [AXUIElement]
else {
    fputs("could not read target app windows: \(windowsResult.rawValue)\n", stderr)
    exit(4)
}

for window in windows {
    var positionValue: CFTypeRef?
    var sizeValue: CFTypeRef?
    guard AXUIElementCopyAttributeValue(window, kAXPositionAttribute as CFString, &positionValue) == .success,
          AXUIElementCopyAttributeValue(window, kAXSizeAttribute as CFString, &sizeValue) == .success,
          let position = point(positionValue),
          let windowSize = size(sizeValue)
    else { continue }

    let positionMatches = abs(Double(position.x) - targetX) <= 16 &&
        abs(Double(position.y) - targetY) <= 16
    let sizeMatches = abs(Double(windowSize.width) - targetWidth) <= 16 &&
        abs(Double(windowSize.height) - targetHeight) <= 16
    guard positionMatches && sizeMatches else { continue }

    var minimizedValue: CFTypeRef?
    let stateResult = AXUIElementCopyAttributeValue(window, kAXMinimizedAttribute as CFString, &minimizedValue)
    let isMinimized = stateResult == .success && ((minimizedValue as? Bool) ?? false)
    if mode == "state" {
        print(isMinimized ? "true" : "false")
        exit(0)
    }

    let target = mode == "minimize"
    if isMinimized != target {
        let setResult = AXUIElementSetAttributeValue(
            window,
            kAXMinimizedAttribute as CFString,
            target ? kCFBooleanTrue : kCFBooleanFalse
        )
        guard setResult == .success else {
            fputs("failed to set AXMinimized: \(setResult.rawValue)\n", stderr)
            exit(5)
        }
    }

    Thread.sleep(forTimeInterval: 1.0)
    var afterValue: CFTypeRef?
    let afterResult = AXUIElementCopyAttributeValue(window, kAXMinimizedAttribute as CFString, &afterValue)
    guard afterResult == .success else {
        fputs("failed to read AXMinimized after set: \(afterResult.rawValue)\n", stderr)
        exit(6)
    }
    print(((afterValue as? Bool) ?? false) ? "true" : "false")
    exit(0)
}

fputs("no matching accessibility window for bounds \(targetX),\(targetY) \(targetWidth)x\(targetHeight)\n", stderr)
exit(7)
EOF

cat >"$HIDE_APP" <<'EOF'
import AppKit
import Foundation

guard CommandLine.arguments.count == 3,
      let rawPID = Int32(CommandLine.arguments[1]),
      let app = NSRunningApplication(processIdentifier: pid_t(rawPID))
else {
    fputs("usage: hide-app.swift <pid> <hide|show>\n", stderr)
    exit(2)
}

let mode = CommandLine.arguments[2]
func setVisibleWithSystemEvents(_ visible: Bool) -> Bool {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
    let state = visible ? "true" : "false"
    process.arguments = [
        "-e",
        "tell application \"System Events\" to set visible of (first application process whose unix id is \(rawPID)) to \(state)"
    ]
    do {
        try process.run()
        process.waitUntilExit()
        return process.terminationStatus == 0
    } catch {
        return false
    }
}

if mode == "hide" {
    app.activate(options: [.activateAllWindows, .activateIgnoringOtherApps])
    Thread.sleep(forTimeInterval: 0.5)
    guard app.hide() || setVisibleWithSystemEvents(false) else {
        fputs("failed to hide app through AppKit and System Events\n", stderr)
        exit(3)
    }
    Thread.sleep(forTimeInterval: 1.0)
    print("hidden")
} else if mode == "show" {
    guard app.unhide() || setVisibleWithSystemEvents(true) else {
        fputs("failed to unhide app through AppKit and System Events\n", stderr)
        exit(4)
    }
    app.activate(options: [.activateAllWindows, .activateIgnoringOtherApps])
    Thread.sleep(forTimeInterval: 1.0)
    print("visible")
} else {
    fputs("mode must be hide or show\n", stderr)
    exit(2)
}
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
log "chromium=$CHROMIUM"
log "pointer_driver=$POINTER_DRIVER"
log "url=$URL"
if [ "$SCENARIO" = "hello-config-homepage" ]; then
  log "hello_config_homepage=$HELLO_CONFIG_HOMEPAGE"
fi
if [ "$SCENARIO" = "hello-config-browser-list" ]; then
  log "hello_config_browsers=chromium,$HELLO_CONFIG_SECOND_BROWSER"
fi
log "app_log=$APP_LOG"
log "chromium_trace=$CHROMIUM_TRACE"
log "screenshot=$SCREENSHOT"
if [ "$SCENARIO" = "browser-state-smoke" ] || [ "$SCENARIO" = "browser-command-navigation" ] || [ "$SCENARIO" = "back-navigation-focus-theft" ] || [ "$SCENARIO" = "click-driven-browse-mode" ]; then
  log "webtui_state_trace=$WEBTUI_STATE_TRACE"
fi
if [ "$SCENARIO" = "browser-state-smoke" ]; then
  log "hover_screenshot=$SCREENSHOT_BROWSER_STATE_HOVER"
  log "reload_screenshot=$SCREENSHOT_BROWSER_STATE_RELOAD"
  log "blank_screenshot=$SCREENSHOT_BROWSER_STATE_BLANK"
fi
if [ "$SCENARIO" = "javascript-dialog-smoke" ]; then
  log "webtui_state_trace=$WEBTUI_STATE_TRACE"
fi
if [ "$SCENARIO" = "http-auth-smoke" ]; then
  log "webtui_state_trace=$WEBTUI_STATE_TRACE"
fi
if [ "$SCENARIO" = "renderer-crash-smoke" ]; then
  log "webtui_state_trace=$WEBTUI_STATE_TRACE"
fi
if [ "$SCENARIO" = "color-scheme-smoke" ]; then
  log "webtui_state_trace=$WEBTUI_STATE_TRACE"
fi
if [ "$SCENARIO" = "copy-current-url-smoke" ]; then
  log "webtui_state_trace=$WEBTUI_STATE_TRACE"
fi
if [ "$SCENARIO" = "webapp-clipboard-copy" ]; then
  log "webtui_state_trace=$WEBTUI_STATE_TRACE"
fi
if [ "$SCENARIO" = "browser-input-granularity" ]; then
  log "webtui_state_trace=$WEBTUI_STATE_TRACE"
  log "browser_input_type_command=$INPUT_TYPE_COMMAND"
fi
if [ "$SCENARIO" = "click-driven-browse-mode" ]; then
  log "webtui_state_trace=$WEBTUI_STATE_TRACE"
  log "browser_input_type_command=$INPUT_TYPE_COMMAND"
  log "scenario_feature=click-driven-browse-mode"
fi
if [ "$SCENARIO" = "visible-profile-identity" ]; then
  log "webtui_state_trace=$WEBTUI_STATE_TRACE"
fi
if [ "$SCENARIO" = "window-resize" ] || [ "$SCENARIO" = "performance-window-resize" ]; then
  log "grow_screenshot=$SCREENSHOT_GROW"
  log "shrink_screenshot=$SCREENSHOT_SHRINK"
fi
if [ "$SCENARIO" = "split-right" ] || [ "$SCENARIO" = "split-right-border-config" ] || [ "$SCENARIO" = "split-down" ] || [ "$SCENARIO" = "split-right-resize" ] || [ "$SCENARIO" = "split-right-equalize" ]; then
  log "split_screenshot=$SCREENSHOT_SPLIT"
fi
if [ "$SCENARIO" = "split-right-border-config" ]; then
  log "split_border_config_case=$BORDER_CONFIG_CASE"
fi
if [ "$SCENARIO" = "split-right-zoom" ]; then
  log "zoom_screenshot=$SCREENSHOT_ZOOM"
  log "unzoom_screenshot=$SCREENSHOT_UNZOOM"
fi
if [ "$SCENARIO" = "split-right-close-sibling" ]; then
  log "close_screenshot=$SCREENSHOT_CLOSE"
fi
if [ "$SCENARIO" = "split-right-close-browser-pane" ]; then
  log "close_screenshot=$SCREENSHOT_CLOSE"
  log "sibling_alive_command=$SIBLING_ALIVE_COMMAND"
fi
if [ "$SCENARIO" = "split-right-focus-switch" ]; then
  log "split_screenshot=$SCREENSHOT_SPLIT"
  log "sibling_focus_command=$SIBLING_FOCUS_COMMAND"
  log "browser_focus_command=$BROWSER_FOCUS_COMMAND"
fi
if [ "$SCENARIO" = "new-terminal-tab-visibility" ]; then
  log "new_tab_screenshot=$SCREENSHOT_TAB_NEW"
  log "back_tab_screenshot=$SCREENSHOT_TAB_BACK"
  log "new_tab_command_log=$NEW_TAB_COMMAND_LOG"
  log "new_tab_marker_command=$NEW_TAB_MARKER_COMMAND"
fi
if [ "$SCENARIO" = "open-browser-in-new-tab" ] || [ "$SCENARIO" = "close-browser-tab" ] || [ "$SCENARIO" = "same-profile-server-lifecycle" ] || [ "$SCENARIO" = "gui-active-multi-tab" ] || [ "$SCENARIO" = "multi-profile-isolation" ]; then
  log "new_tab_screenshot=$SCREENSHOT_TAB_NEW"
  log "browser_b_screenshot=$SCREENSHOT_TAB_BROWSER_B"
  log "browser_a_restored_screenshot=$SCREENSHOT_TAB_BROWSER_A_RESTORED"
  log "browser_b_restored_screenshot=$SCREENSHOT_TAB_BROWSER_B_RESTORED"
  log "after_close_screenshot=$SCREENSHOT_TAB_AFTER_CLOSE"
  log "second_browser_command=$SECOND_BROWSER_COMMAND"
fi
if [ "$SCENARIO" = "open-browser-in-new-window" ]; then
  log "window_b_screenshot=$SCREENSHOT_WINDOW_B"
  log "window_a_restored_screenshot=$SCREENSHOT_WINDOW_A_RESTORED"
  log "window_b_command=$SECOND_BROWSER_COMMAND"
  log "first_run_wrapper_command_log=$NEW_TAB_COMMAND_LOG"
fi
if [ "$SCENARIO" = "multiple-windows-with-browsers" ]; then
  log "window_b_screenshot=$SCREENSHOT_WINDOW_B"
  log "window_c_screenshot=$SCREENSHOT_WINDOW_C"
  log "window_a_restored_screenshot=$SCREENSHOT_WINDOW_A_RESTORED"
  log "window_b_restored_screenshot=$SCREENSHOT_WINDOW_B_RESTORED"
  log "window_b_command=$SECOND_BROWSER_COMMAND"
  log "window_c_command=$THIRD_BROWSER_COMMAND"
  log "first_run_wrapper_command_log=$NEW_TAB_COMMAND_LOG"
fi
if [ "$SCENARIO" = "keyboard-after-tab-window-switch" ]; then
  log "new_tab_screenshot=$SCREENSHOT_TAB_NEW"
  log "browser_b_screenshot=$SCREENSHOT_TAB_BROWSER_B"
  log "browser_a_restored_screenshot=$SCREENSHOT_TAB_BROWSER_A_RESTORED"
  log "window_c_screenshot=$SCREENSHOT_WINDOW_C"
  log "window_a_restored_screenshot=$SCREENSHOT_WINDOW_A_RESTORED"
  log "second_browser_command=$SECOND_BROWSER_COMMAND"
  log "third_browser_command=$THIRD_BROWSER_COMMAND"
  log "first_run_wrapper_command_log=$NEW_TAB_COMMAND_LOG"
  log "new_tab_marker_command=$NEW_TAB_MARKER_COMMAND"
fi
if [ "$SCENARIO" = "display-move-backing-scale" ]; then
  log "display_moved_screenshot=$SCREENSHOT_DISPLAY_MOVED"
  log "display_returned_screenshot=$SCREENSHOT_DISPLAY_RETURNED"
fi
if [ "$SCENARIO" = "fullscreen-unfullscreen" ]; then
  log "fullscreen_screenshot=$SCREENSHOT_FULLSCREEN"
  log "unfullscreen_screenshot=$SCREENSHOT_UNFULLSCREEN"
fi
if [ "$SCENARIO" = "minimize-hide-restore" ]; then
  log "minimize_restored_screenshot=$SCREENSHOT_MINIMIZE_RESTORED"
  log "hide_restored_screenshot=$SCREENSHOT_HIDE_RESTORED"
fi
if [ "$SCENARIO" = "font-size-cell-metrics" ]; then
  log "font_increase_screenshot=$SCREENSHOT_FONT_INCREASE"
  log "font_decrease_screenshot=$SCREENSHOT_FONT_DECREASE"
fi
if [ "$SCENARIO" = "tui-overlay-resize-command" ]; then
  log "tui_shrink_screenshot=$SCREENSHOT_TUI_SHRINK"
  log "tui_reset_screenshot=$SCREENSHOT_TUI_RESET"
  log "tui_viewport_shrink_command=$TUI_VIEWPORT_SHRINK_COMMAND"
  log "tui_viewport_reset_command=$TUI_VIEWPORT_RESET_COMMAND"
fi
if [ "$SCENARIO" = "terminal-scrollback-movement" ]; then
  log "scrollback_up_screenshot=$SCREENSHOT_SCROLLBACK_UP"
  log "scrollback_bottom_screenshot=$SCREENSHOT_SCROLLBACK_BOTTOM"
fi
if [ "$SCENARIO" = "browser-navigation-geometry" ]; then
  log "navigated_screenshot=$SCREENSHOT_NAVIGATED"
  log "navigation_append_command=$NAVIGATION_APPEND_COMMAND"
fi
if [ "$SCENARIO" = "devtools-split-geometry" ] || [ "$SCENARIO" = "devtools-singleton-guard" ]; then
  log "devtools_split_screenshot=$SCREENSHOT_DEVTOOLS_SPLIT"
  log "devtools_command=$DEVTOOLS_COMMAND"
  log "devtools_query_probe=$DEVTOOLS_QUERY_PROBE"
  log "devtools_overlay_probe=$DEVTOOLS_OVERLAY_PROBE"
fi
if [ "$SCENARIO" = "mouse-after-geometry-change" ]; then
  log "mouse_grow_screenshot=$SCREENSHOT_GROW"
  log "mouse_shrink_screenshot=$SCREENSHOT_SHRINK"
  log "mouse_split_screenshot=$SCREENSHOT_SPLIT"
  log "mouse_tui_shrink_screenshot=$SCREENSHOT_TUI_SHRINK"
  log "mouse_tui_reset_screenshot=$SCREENSHOT_TUI_RESET"
fi

if [ "$SCENARIO" = "installed-chromium-release-launch" ]; then
  XDG_CONFIG_HOME="$RELEASE_XDG_CONFIG_HOME" \
  GHOSTTY_LOG=stderr \
  TERMSURF_GEOMETRY_TRACE=1 \
  TERMSURF_GEOMETRY_SCENARIO="$SCENARIO" \
  TERMSURF_DEVTOOLS_RESERVATION_TIMEOUT_MS=1000 \
  TERMSURF_INSTALLED_ROAMIUM_PATH="$INSTALLED_CHROMIUM" \
  TERMSURF_WEBTUI_STATE_TRACE_FILE="$WEBTUI_STATE_TRACE" \
  TERMSURF_INPUT_TRACE=1 \
  TERMSURF_PDF_INPUT_TRACE=1 \
  TERMSURF_PDF_INPUT_TRACE_FILE="$CHROMIUM_TRACE" \
    "$APP_BIN" >"$APP_LOG" 2>&1 &
else
  XDG_CONFIG_HOME="$RELEASE_XDG_CONFIG_HOME" \
  GHOSTTY_CONFIG_PATH="$CONFIG" \
  GHOSTTY_LOG=stderr \
  TERMSURF_GEOMETRY_TRACE=1 \
  TERMSURF_GEOMETRY_SCENARIO="$SCENARIO" \
  TERMSURF_DEVTOOLS_RESERVATION_TIMEOUT_MS=1000 \
  ASTROHACKER_CHROMIUM_PATH="$CHROMIUM_PATH_FOR_APP" \
  TERMSURF_WEBTUI_STATE_TRACE_FILE="$WEBTUI_STATE_TRACE" \
  TERMSURF_INPUT_TRACE=1 \
  TERMSURF_PDF_INPUT_TRACE=1 \
  TERMSURF_PDF_INPUT_TRACE_FILE="$CHROMIUM_TRACE" \
    "$APP_BIN" >"$APP_LOG" 2>&1 &
fi
PID="$!"
log "pid=$PID"

if [ "$SCENARIO" = "named-chromium-invalid-env" ]; then
  wait_for_log "TermSurf message decoded type=HelloRequest" "HelloRequest over TERMSURF_SOCKET" 45
  wait_for_log "SetOverlay: pane_id=.* profile=default browser=chromium url=${URL}" "named Chromium SetOverlay with invalid env" 45
  wait_for_log "SetOverlay: named browser unresolved browser=chromium env=ASTROHACKER_CHROMIUM_PATH value=chromium" "clear named Chromium invalid-env failure" 45
  if grep -E "SetOverlay: created pending server key=default/chromium" "$APP_LOG" >/dev/null 2>&1; then
    fail "invalid named Chromium env left a pending default/chromium server"
  fi
  log "PASS: invalid named Chromium env did not create a pending server"
  if grep -E "spawned browser path=" "$APP_LOG" >/dev/null 2>&1; then
    fail "invalid named Chromium env spawned a browser"
  fi
  log "PASS: invalid named Chromium env did not spawn a browser"
  log "PASS: scenario named-chromium-invalid-env"
  exit 0
fi

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

if [ "$SKIP_INITIAL_HIT_TEST" = "0" ]; then
  move_window_center "$WIN_LINE" "initial"
  read -r INITIAL_FOCUS_X INITIAL_FOCUS_Y <<<"$(global_point_inside_web_frame "$WIN_LINE" "$PRESENTED_LINE" 120 120)"
  click_global_point "$INITIAL_FOCUS_X" "$INITIAL_FOCUS_Y" "initial_terminal_focus"
  delay 1
else
  log "PASS: scenario skipped initial pointer prime"
fi

require_log 'TermSurf geometry layer=zig' "Zig geometry record"
require_log 'TermSurf geometry layer=bridge' "bridge geometry record"
require_log 'TermSurf geometry layer=appkit event=presented ' "AppKit presented geometry record"
if [ "$SKIP_INITIAL_HIT_TEST" = "0" ]; then
  wait_for_log 'TermSurf geometry layer=appkit event=hit_test .*hit=true' "AppKit hit-test geometry record" 5
else
  log "PASS: scenario skipped generic initial hit-test prerequisite"
fi
require_log "scenario=${SCENARIO}" "scenario id in geometry records"

CA_CONTEXT_LINE="$(grep -E 'TermSurf geometry layer=zig event=ca_context' "$APP_LOG" | tail -1)"
ZIG_PRESENT_LINE="$(grep -E 'TermSurf geometry layer=zig event=present_overlay_call' "$APP_LOG" | tail -1)"
BRIDGE_PRESENT_LINE="$(grep -E 'TermSurf geometry layer=bridge event=present_target_found' "$APP_LOG" | tail -1)"
APPKIT_PRESENT_LINE="$(grep -E 'TermSurf geometry layer=appkit event=presented ' "$APP_LOG" | tail -1)"
APPKIT_PIXELS_LINE="$(grep -E 'TermSurf geometry layer=appkit event=presented_pixels' "$APP_LOG" | tail -1)"
HIT_TEST_LINE="$(grep -E 'TermSurf geometry layer=appkit event=hit_test .*hit=true' "$APP_LOG" | tail -1 || true)"

[ -n "$CA_CONTEXT_LINE" ] || fail "missing Zig ca_context geometry line"
[ -n "$ZIG_PRESENT_LINE" ] || fail "missing Zig present_overlay_call geometry line"
[ -n "$BRIDGE_PRESENT_LINE" ] || fail "missing bridge present_target_found geometry line"
[ -n "$APPKIT_PRESENT_LINE" ] || fail "missing AppKit presented geometry line"
[ -n "$APPKIT_PIXELS_LINE" ] || fail "missing AppKit presented-pixels geometry line"
if [ "$SKIP_INITIAL_HIT_TEST" = "0" ]; then
  [ -n "$HIT_TEST_LINE" ] || fail "missing AppKit hit-test geometry line"
fi

PANE_ID="$(printf '%s\n' "$CA_CONTEXT_LINE" | sed -E 's/.*pane_id:([^ ]+).*/\1/')"
[ -n "$PANE_ID" ] || fail "could not extract pane id"
BROWSER_TAB_ID="$(printf '%s\n' "$CA_CONTEXT_LINE" | sed -E 's/.*browser_tab_id:([^ ]+).*/\1/')"
case "$BROWSER_TAB_ID" in
  ''|unknown:*) fail "could not extract concrete browser tab id from Zig ca_context" ;;
esac
TAB_READY_LINE="$(grep -E "TermSurf geometry layer=zig event=tab_ready .*pane_id:${PANE_ID} .*browser_tab_id:${BROWSER_TAB_ID}" "$APP_LOG" | tail -1 || true)"
if [ -z "$TAB_READY_LINE" ] && grep -E "TabReady: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID}" "$APP_LOG" >/dev/null 2>&1; then
  TAB_READY_LINE="pane_id:${PANE_ID} browser_tab_id:${BROWSER_TAB_ID} note=tab-ready-log-fallback"
fi
[ -n "$TAB_READY_LINE" ] || fail "missing Zig tab_ready geometry line for pane id and browser tab id"
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
OVERLAY_FRAME_Y="$(extract_frame_y "$APPKIT_PRESENT_LINE")"
[ -n "$OVERLAY_FRAME_Y" ] && [ "$OVERLAY_FRAME_Y" != "$APPKIT_PRESENT_LINE" ] || fail "could not extract AppKit overlay frame y"
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
log "correlation_overlay_frame_y=$OVERLAY_FRAME_Y"
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
if [ "$SKIP_INITIAL_HIT_TEST" = "0" ]; then
  require_text "$HIT_TEST_LINE" "context_id=${CONTEXT_ID}" "hit-test shares context"
  require_text "$HIT_TEST_LINE" "hit=true" "hit-test is inside overlay"
  require_text "$HIT_TEST_LINE" "web_point={" "hit-test includes webview-relative point"
fi
require_log "TermSurf geometry .*scenario=${SCENARIO}" "timestamped run contains scenario id"
require_log 'window_id:[^ ]+ surface_id:[^ ]+ selected_tab_id:[^ ]+ pane_id:[^ ]+ browser_tab_id:[^ ]+' "canonical identity tuple fields"
require_readable "$CHROMIUM_TRACE"
require_trace "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${APPKIT_PIXEL_WIDTH} pixel_height=${APPKIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied resize to AppKit pixel size via ts_set_view_size"

if [ "$SCENARIO" = "browser-command-navigation" ]; then
  STATE_HOVER_URL="http://127.0.0.1:${STATE_HTTP_PORT}/hover-target.html"

  wait_for_state_trace "event=url_changed.*url=${URL}" "webtui URL state changed to command-navigation fixture URL" 45
  wait_for_state_trace "event=loading_state.*state=done" "webtui command-navigation initial load finished" 45

  CONTROL_LOG_START_LINE="$(log_line_count)"
  CONTROL_TRACE_START_LINE="$(trace_line_count)"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1 || true
  delay 0.5
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 33 command >>"$HARNESS_LOG" 2>&1
  delay 0.5
  require_no_log_after "$CONTROL_LOG_START_LINE" "perform_key_equivalent_browser_forwarded .*key_code=33" "pre-browse Cmd+[ did not log browser forwarding"
  if tail -n +"$((CONTROL_TRACE_START_LINE + 1))" "$CHROMIUM_TRACE" |
    grep -E "key-event tab=${BROWSER_TAB_ID} pane=${PANE_ID} .*type=down .*windows_key_code=219 .*modifiers=8" >/dev/null 2>&1; then
    fail "pre-browse Cmd+[ reached browser"
  fi
  log "PASS: pre-browse Cmd+[ did not reach browser"

  COMMAND_MODE_TRACE_START_LINE="$(trace_line_count)"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  require_trace_after "$COMMAND_MODE_TRACE_START_LINE" "focus-changed tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed command-navigation browse focus=true"

  IFS=$'\t' read -r COMMAND_NAV_X COMMAND_NAV_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$APPKIT_PRESENT_LINE" 90 44)"
  COMMAND_NAV_STATE_START_LINE="$(state_trace_line_count)"
  click_global_point "$COMMAND_NAV_X" "$COMMAND_NAV_Y" "command_navigation_hover_link"
  wait_for_state_trace_after "$COMMAND_NAV_STATE_START_LINE" "event=url_changed[[:space:]]+url=${STATE_HOVER_URL}" "browser-command-navigation reached second URL" 45

  COMMAND_BACK_LOG_START_LINE="$(log_line_count)"
  COMMAND_BACK_TRACE_START_LINE="$(trace_line_count)"
  COMMAND_BACK_STATE_START_LINE="$(state_trace_line_count)"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 33 command >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$COMMAND_BACK_LOG_START_LINE" "perform_key_equivalent_browser_forwarded .*key_code=33 .*modifiers=8" "Cmd+[ key equivalent forwarded to browser" 15
  wait_for_log_after "$COMMAND_BACK_LOG_START_LINE" "KeyEvent: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} type=down windows_key_code=219 .*modifiers=8" "Zig forwarded Cmd+[ as browser Back key event" 15
  wait_for_trace_line_after "$COMMAND_BACK_TRACE_START_LINE" "key-event tab=${BROWSER_TAB_ID} pane=${PANE_ID} .*type=down .*windows_key_code=219 .*modifiers=8" "Chromium received Cmd+[ browser Back key event" 15 >/dev/null
  wait_for_state_trace_after "$COMMAND_BACK_STATE_START_LINE" "event=url_changed[[:space:]]+url=${URL}" "Cmd+[ navigated back to original URL" 45

  COMMAND_FORWARD_LOG_START_LINE="$(log_line_count)"
  COMMAND_FORWARD_TRACE_START_LINE="$(trace_line_count)"
  COMMAND_FORWARD_STATE_START_LINE="$(state_trace_line_count)"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 30 command >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$COMMAND_FORWARD_LOG_START_LINE" "perform_key_equivalent_browser_forwarded .*key_code=30 .*modifiers=8" "Cmd+] key equivalent forwarded to browser" 15
  wait_for_log_after "$COMMAND_FORWARD_LOG_START_LINE" "KeyEvent: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} type=down windows_key_code=221 .*modifiers=8" "Zig forwarded Cmd+] as browser Forward key event" 15
  wait_for_trace_line_after "$COMMAND_FORWARD_TRACE_START_LINE" "key-event tab=${BROWSER_TAB_ID} pane=${PANE_ID} .*type=down .*windows_key_code=221 .*modifiers=8" "Chromium received Cmd+] browser Forward key event" 15 >/dev/null
  wait_for_state_trace_after "$COMMAND_FORWARD_STATE_START_LINE" "event=url_changed[[:space:]]+url=${STATE_HOVER_URL}" "Cmd+] navigated forward to second URL" 45
fi

if [ "$SCENARIO" = "back-navigation-focus-theft" ]; then
  STATE_HOVER_URL="http://127.0.0.1:${STATE_HTTP_PORT}/hover-target.html"

  wait_for_state_trace "event=url_changed.*url=${URL}" "webtui focus-theft fixture URL" 45
  wait_for_state_trace "event=loading_state.*state=done" "webtui focus-theft initial load finished" 45

  FOCUS_THEFT_MODE_LOG_START_LINE="$(log_line_count)"
  FOCUS_THEFT_MODE_TRACE_START_LINE="$(trace_line_count)"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_trace_line_after "$FOCUS_THEFT_MODE_TRACE_START_LINE" "focus-changed tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed focus-theft browse focus=true" 15 >/dev/null
  wait_for_log_after "$FOCUS_THEFT_MODE_LOG_START_LINE" "termsurf-activation stage=ActivateContents" "Chromium recorded Browse-mode ActivateContents activation trace" 15
  wait_for_log_after "$FOCUS_THEFT_MODE_LOG_START_LINE" "termsurf-activation stage=ActivateContents .*after_activate .*hidden=1 .*shell_key=0 .*shell_main=0" "Chromium hidden shell stayed non-key/non-main during Browse activation" 15

  IFS=$'\t' read -r FOCUS_THEFT_LINK_X FOCUS_THEFT_LINK_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$APPKIT_PRESENT_LINE" 90 44)"
  FOCUS_THEFT_FIRST_NAV_STATE_START_LINE="$(state_trace_line_count)"
  click_global_point "$FOCUS_THEFT_LINK_X" "$FOCUS_THEFT_LINK_Y" "focus_theft_initial_link"
  wait_for_state_trace_after "$FOCUS_THEFT_FIRST_NAV_STATE_START_LINE" "event=url_changed[[:space:]]+url=${STATE_HOVER_URL}" "focus-theft initial link click reached second URL" 45

  FOCUS_THEFT_BACK_LOG_START_LINE="$(log_line_count)"
  FOCUS_THEFT_BACK_TRACE_START_LINE="$(trace_line_count)"
  FOCUS_THEFT_BACK_STATE_START_LINE="$(state_trace_line_count)"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 33 command >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$FOCUS_THEFT_BACK_LOG_START_LINE" "perform_key_equivalent_browser_forwarded .*key_code=33 .*modifiers=8" "focus-theft Cmd+[ key equivalent forwarded to browser" 15
  wait_for_trace_line_after "$FOCUS_THEFT_BACK_TRACE_START_LINE" "key-event tab=${BROWSER_TAB_ID} pane=${PANE_ID} .*type=down .*windows_key_code=219 .*modifiers=8" "focus-theft Chromium received Cmd+[ browser Back key event" 15 >/dev/null
  wait_for_state_trace_after "$FOCUS_THEFT_BACK_STATE_START_LINE" "event=url_changed[[:space:]]+url=${URL}" "focus-theft Cmd+[ navigated back to original URL" 45
  require_no_log_after "$FOCUS_THEFT_BACK_LOG_START_LINE" "Pane focus changed: pane_id=${PANE_ID} focused=false" "Ghostboard pane did not lose focus after Cmd+["

  FOCUS_THEFT_SECOND_CLICK_TRACE_START_LINE="$(trace_line_count)"
  FOCUS_THEFT_SECOND_CLICK_LOG_START_LINE="$(log_line_count)"
  FOCUS_THEFT_SECOND_CLICK_STATE_START_LINE="$(state_trace_line_count)"
  click_global_point "$FOCUS_THEFT_LINK_X" "$FOCUS_THEFT_LINK_Y" "focus_theft_post_back_single_link"
  wait_for_trace_line_after "$FOCUS_THEFT_SECOND_CLICK_TRACE_START_LINE" "mouse-event tab=${BROWSER_TAB_ID} pane=${PANE_ID} .*type=down .*button=left .*click_count=1 .*modifiers=0" "post-Back single link click reached Chromium as unmodified click_count=1" 15 >/dev/null
  wait_for_state_trace_after "$FOCUS_THEFT_SECOND_CLICK_STATE_START_LINE" "event=url_changed[[:space:]]+url=${STATE_HOVER_URL}" "post-Back single link click navigated to second URL" 45
  require_no_log_after "$FOCUS_THEFT_SECOND_CLICK_LOG_START_LINE" "Pane focus changed: pane_id=${PANE_ID} focused=false" "Ghostboard pane did not lose focus after post-Back single link click"

  read -r FOCUS_THEFT_CONTROL_X FOCUS_THEFT_CONTROL_Y <<<"$(global_point_below_web_frame "$WIN_LINE" "$APPKIT_PRESENT_LINE" 120)"
  FOCUS_THEFT_CONTROL_LOG_START_LINE="$(log_line_count)"
  FOCUS_THEFT_CONTROL_TRACE_START_LINE="$(trace_line_count)"
  FOCUS_THEFT_CONTROL_STATE_START_LINE="$(state_trace_line_count)"
  click_global_point "$FOCUS_THEFT_CONTROL_X" "$FOCUS_THEFT_CONTROL_Y" "focus_theft_post_back_control"
  wait_for_log_after "$FOCUS_THEFT_CONTROL_LOG_START_LINE" "ClickModeChanged: pane_id=${PANE_ID} browsing=false reason=overlay_miss" "post-Back control-panel click left Browse mode once" 45
  wait_for_state_trace_after "$FOCUS_THEFT_CONTROL_STATE_START_LINE" "event=mode_changed.*source=gui.*browsing=false.*mode=control" "webtui received post-Back GUI-originated Control mode" 45
  wait_for_trace_line_after "$FOCUS_THEFT_CONTROL_TRACE_START_LINE" "focus-changed tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed post-Back control click focus=false" 15 >/dev/null
  require_no_log_after "$FOCUS_THEFT_CONTROL_LOG_START_LINE" "Pane focus changed: pane_id=${PANE_ID} focused=false" "Ghostboard pane stayed focused after post-Back control click"
fi

if [ "$SCENARIO" = "browser-state-smoke" ]; then
  STATE_HOVER_URL="http://127.0.0.1:${STATE_HTTP_PORT}/hover-target.html"
  STATE_BLANK_URL="http://127.0.0.1:${STATE_HTTP_PORT}/blank-target.html"

  wait_for_state_trace "event=url_changed.*url=${URL}" "webtui URL state changed to fixture URL" 45
  wait_for_state_trace "event=loading_state.*state=loading" "webtui initial loading state started" 45
  wait_for_state_trace "event=loading_state.*state=done" "webtui loading state finished" 45
  wait_for_state_trace "event=title_changed.*title=Issue 26061712000816 State Smoke Updated 1" "webtui received updated page title" 45
  wait_for_state_trace "event=console_message.*message=ISSUE816_CONSOLE_MARKER_1" "webtui received console marker" 45
  require_state_trace "event=render_state.*title=Issue 26061712000816 State Smoke Updated 1" "webtui render state includes updated title"
  require_state_trace "event=render_state.*latest_console=.*ISSUE816_CONSOLE_MARKER_1" "webtui render state includes console marker"

  STATE_WIN_LINE="$(window_bounds)" || fail "failed to resolve browser-state window bounds"
  IFS=$'\t' read -r _STATE_WID STATE_WX STATE_WY _STATE_WW _STATE_WH <<<"$STATE_WIN_LINE"
  IFS=$'\t' read -r HOVER_X HOVER_Y <<<"$(global_point_for_web_point "$STATE_WIN_LINE" "$APPKIT_PRESENT_LINE" 90 44)"
  move_global_point "$HOVER_X" "$HOVER_Y" "browser_state_hover"
  wait_for_state_trace "event=target_url_changed.*url=${STATE_HOVER_URL}" "webtui hover target URL" 45
  require_state_trace "event=render_state.*target_url=${STATE_HOVER_URL}" "webtui render state includes hover target URL"
  screencapture -x -o -l"$WID" "$SCREENSHOT_BROWSER_STATE_HOVER"
  log "hover_screenshot_exit=$?"

  IFS=$'\t' read -r WHITE_X WHITE_Y <<<"$(global_point_for_web_point "$STATE_WIN_LINE" "$APPKIT_PRESENT_LINE" 60 250)"
  WHITE_SAMPLE_X=$((WHITE_X - STATE_WX))
  WHITE_SAMPLE_Y=$((WHITE_Y - STATE_WY))
  assert_png_pixel_near_white "$SCREENSHOT_BROWSER_STATE_HOVER" "$WHITE_SAMPLE_X" "$WHITE_SAMPLE_Y" "browser-state white viewport background"

  log "browser_state_reload_key=cmd+r=browser_refresh"
  enter_browser_browse "browser_state_reload" "$PANE_ID" "$BROWSER_TAB_ID"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 15 command >>"$HARNESS_LOG" 2>&1
  if try_wait_state_trace "event=console_message.*message=ISSUE816_CONSOLE_MARKER_2" 20; then
    log "PASS: webtui received reload console marker"
    wait_for_state_trace "event=title_changed.*title=Issue 26061712000816 State Smoke Updated 2" "webtui received reload title update" 45
    require_state_trace "event=render_state.*title=Issue 26061712000816 State Smoke Updated 2" "webtui render state includes reload title"
  else
    log "PARTIAL: Cmd-R in Browse mode did not produce a reload console marker"
  fi
  screencapture -x -o -l"$WID" "$SCREENSHOT_BROWSER_STATE_RELOAD"
  log "reload_screenshot_exit=$?"

  IFS=$'\t' read -r BLANK_X BLANK_Y <<<"$(global_point_for_web_point "$STATE_WIN_LINE" "$APPKIT_PRESENT_LINE" 90 94)"
  BLANK_TRACE_START_LINE="$(state_trace_line_count)"
  click_global_point "$BLANK_X" "$BLANK_Y" "browser_state_blank_target"
  wait_for_state_trace_after "$BLANK_TRACE_START_LINE" "event=url_changed[[:space:]]+url=${STATE_BLANK_URL}" "webtui target blank URL navigation" 45
  wait_for_state_trace_after "$BLANK_TRACE_START_LINE" "event=title_changed[[:space:]]+title=Issue 26061712000816 Blank Target" "webtui target blank title" 45
  screencapture -x -o -l"$WID" "$SCREENSHOT_BROWSER_STATE_BLANK"
  log "blank_screenshot_exit=$?"
fi

if [ "$SCENARIO" = "multi-profile-isolation" ]; then
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_FRAME_X="$OVERLAY_FRAME_X"
  A_FRAME_Y="$OVERLAY_FRAME_Y"
  A_PIXEL="$APPKIT_PIXEL"
  log "profile_a_selected_tab_id=$A_SELECTED_TAB_ID"
  log "profile_a_pane_id=$A_PANE_ID"
  log "profile_a_browser_tab_id=$A_BROWSER_TAB_ID"
  log "profile_a_context_id=$A_CONTEXT_ID"

  require_log "SetOverlay: pane_id=${A_PANE_ID} profile=profilea browser=${CHROMIUM}" "profile A SetOverlay uses profilea and absolute Chromium path"
  require_log "SetOverlay: created pending server key=profilea/${CHROMIUM} pane_count=1" "profile A created profile-scoped server key"
  A_SPAWN_LINE="$(grep -E "spawned browser path=${CHROMIUM} pid=[0-9]+ profile=profilea .*--user-data-dir=.*chromium-profiles/profilea" "$APP_LOG" | tail -1 || true)"
  [ -n "$A_SPAWN_LINE" ] || fail "missing profile A spawn line with profile-specific user-data-dir"
  A_SPAWN_PID="$(printf '%s\n' "$A_SPAWN_LINE" | sed -E 's/.* pid=([0-9]+) profile=.*/\1/')"
  [ -n "$A_SPAWN_PID" ] || fail "failed to extract profile A Chromium pid"
  log "profile_a_spawn_pid=$A_SPAWN_PID"
  log "PASS: profile A spawn used profile-specific user-data-dir"
  wait_for_state_trace "event=console_message.*message=ISSUE818_PROFILE_STORAGE profile=profilea before=none after=profilea" "profile A initial same-origin storage marker" 45

  B_TAB_START_LINE="$(log_line_count)"
  B_TAB_TRACE_START_LINE="$(trace_line_count)"
  B_TAB_STATE_START_LINE="$(state_trace_line_count)"
  log "profile_b_new_tab_keybind=ctrl+t=new_tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 17 control >>"$HARNESS_LOG" 2>&1
  delay 1
  require_log_after "$B_TAB_START_LINE" "dispatching action target=surface action=.new_tab" "profile B native tab action dispatched"
  require_log_after "$B_TAB_START_LINE" 'starting command command=`/usr/bin/login`' "profile B native tab started login shell"
  if [ -s "$NEW_TAB_COMMAND_LOG" ]; then
    fail "profile B native tab inherited the first-run web wrapper"
  fi
  log "PASS: profile B native tab did not inherit first-run web wrapper"
  A_TABBED_PRESENT_LINE="$(wait_for_changed_appkit_frame_after "$B_TAB_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "profile A geometry adjusted for native tab bar")"
  A_TABBED_PIXELS_LINE="$(wait_for_changed_appkit_pixels_after "$B_TAB_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "profile A AppKit pixels adjusted for native tab bar")"
  A_FRAME="$(extract_overlay_frame "$A_TABBED_PRESENT_LINE")"
  A_FRAME_SIZE="$(extract_frame_size "$A_TABBED_PRESENT_LINE")"
  A_FRAME_X="$(extract_frame_x "$A_TABBED_PRESENT_LINE")"
  A_FRAME_Y="$(extract_frame_y "$A_TABBED_PRESENT_LINE")"
  A_PIXEL="$(extract_appkit_pixel "$A_TABBED_PIXELS_LINE")"
  log "profile_a_tabbed_overlay_frame=$A_FRAME"
  log "profile_a_tabbed_appkit_pixel=$A_PIXEL"

  log "profile_b_select_tab2_keybind=ctrl+2=goto_tab:2"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 19 control >>"$HARNESS_LOG" 2>&1
  delay 1
  B_SELECTED_LINE="$(wait_for_selected_tab_change_after "$B_TAB_START_LINE" "$A_SELECTED_TAB_ID" "profile B tab selected")"
  B_SELECTED_TAB_ID="$(extract_selected_tab_id "$B_SELECTED_LINE")"
  [ -n "$B_SELECTED_TAB_ID" ] || fail "failed to extract profile B selected tab id"
  log "profile_b_selected_tab_id=$B_SELECTED_TAB_ID"

  printf '"%s" --browser "%s" --profile profileb "%s"' "$WEB" "$CHROMIUM" "$PROFILE_URL_B" >"$SECOND_BROWSER_COMMAND"
  log "profile_b_command=$(cat "$SECOND_BROWSER_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$SECOND_BROWSER_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1

  B_SET_LINE="$(wait_for_line_after "$B_TAB_START_LINE" "SetOverlay: pane_id=[^ ]+ profile=profileb browser=${CHROMIUM}" "profile B SetOverlay" 60)"
  B_PANE_ID="$(printf '%s\n' "$B_SET_LINE" | sed -E 's/.*SetOverlay: pane_id=([^ ]+) .*/\1/')"
  [ -n "$B_PANE_ID" ] || fail "failed to extract profile B pane id"
  [ "$B_PANE_ID" != "$A_PANE_ID" ] || fail "profile B reused profile A pane id"
  wait_for_log_after "$B_TAB_START_LINE" "SetOverlay: created pending server key=profileb/${CHROMIUM} pane_count=1" "profile B created profile-scoped server key" 60
  B_SPAWN_LINE="$(tail -n +"$((B_TAB_START_LINE + 1))" "$APP_LOG" | grep -E "spawned browser path=${CHROMIUM} pid=[0-9]+ profile=profileb .*--user-data-dir=.*chromium-profiles/profileb" | tail -1 || true)"
  [ -n "$B_SPAWN_LINE" ] || fail "missing profile B spawn line with profile-specific user-data-dir"
  B_SPAWN_PID="$(printf '%s\n' "$B_SPAWN_LINE" | sed -E 's/.* pid=([0-9]+) profile=.*/\1/')"
  [ -n "$B_SPAWN_PID" ] || fail "failed to extract profile B Chromium pid"
  [ "$B_SPAWN_PID" != "$A_SPAWN_PID" ] || fail "profile B reused profile A Chromium pid"
  log "profile_b_spawn_pid=$B_SPAWN_PID"
  log "PASS: profile B spawn used profile-specific user-data-dir"

  B_CA_CONTEXT_LINE="$(wait_for_line_after "$B_TAB_START_LINE" "TermSurf geometry layer=zig event=ca_context .*pane_id:${B_PANE_ID}" "profile B Zig ca_context" 60)"
  B_BROWSER_TAB_ID="$(extract_browser_tab_id "$B_CA_CONTEXT_LINE")"
  B_CONTEXT_ID="$(extract_context_id "$B_CA_CONTEXT_LINE")"
  [ -n "$B_BROWSER_TAB_ID" ] || fail "failed to extract profile B browser tab id"
  [ -n "$B_CONTEXT_ID" ] || fail "failed to extract profile B context id"
  [ "$B_CONTEXT_ID" != "$A_CONTEXT_ID" ] || fail "profile B reused profile A CA/context id"
  log "profile_b_pane_id=$B_PANE_ID"
  log "profile_b_browser_tab_id=$B_BROWSER_TAB_ID"
  log "profile_b_context_id=$B_CONTEXT_ID"
  if [ "$B_BROWSER_TAB_ID" = "$A_BROWSER_TAB_ID" ]; then
    log "profile_browser_tab_ids_are_process_local=true"
  fi

  B_APPKIT_PRESENT_LINE="$(wait_for_line_after "$B_TAB_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${B_PANE_ID} .*context_id=${B_CONTEXT_ID}" "profile B AppKit presentation" 60)"
  B_APPKIT_PIXELS_LINE="$(wait_for_line_after "$B_TAB_START_LINE" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${B_PANE_ID} .*context_id=${B_CONTEXT_ID}" "profile B AppKit pixels" 60)"
  B_SELECTED_PRESENTED_TAB_ID="$(extract_selected_tab_id "$B_APPKIT_PRESENT_LINE")"
  B_FRAME="$(extract_overlay_frame "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_SIZE="$(extract_frame_size "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_X="$(extract_frame_x "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_Y="$(extract_frame_y "$B_APPKIT_PRESENT_LINE")"
  B_PIXEL="$(extract_appkit_pixel "$B_APPKIT_PIXELS_LINE")"
  [ "$B_SELECTED_PRESENTED_TAB_ID" = "$B_SELECTED_TAB_ID" ] || fail "profile B selected tab id mismatch: expected=$B_SELECTED_TAB_ID actual=$B_SELECTED_PRESENTED_TAB_ID"
  log "profile_b_overlay_frame=$B_FRAME"
  log "profile_b_appkit_pixel=$B_PIXEL"
  B_PIXEL_WIDTH="${B_PIXEL%x*}"
  B_PIXEL_HEIGHT="${B_PIXEL#*x}"
  require_trace_after "$B_TAB_TRACE_START_LINE" "resize tab_id=${B_BROWSER_TAB_ID} pane_id=${B_PANE_ID} pixel_width=${B_PIXEL_WIDTH} pixel_height=${B_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied profile B resize to AppKit pixel size"
  wait_for_state_trace_after "$B_TAB_STATE_START_LINE" "event=console_message.*message=ISSUE818_PROFILE_STORAGE profile=profileb before=none after=profileb" "profile B initial same-origin storage marker" 45

  B_WIN_LINE="$(window_bounds)" || fail "failed to resolve profile B window bounds"
  IFS=$'\t' read -r _B_WID B_WX B_WY _B_WW _B_WH <<<"$B_WIN_LINE"
  B_CLICK_X="$(awk -v wx="$B_WX" -v frame_x="$B_FRAME_X" -v frame_size="$B_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
  B_CLICK_Y="$(awk -v wy="$B_WY" -v frame_y="$B_FRAME_Y" -v frame_size="$B_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"
  B_HIT_START_LINE="$(log_line_count)"
  click_global_point "$B_CLICK_X" "$B_CLICK_Y" "profile_b_area"
  B_HIT_LINE="$(wait_for_hit_after "$B_HIT_START_LINE" "$B_CONTEXT_ID" "profile B hit-test")"
  require_text "$B_HIT_LINE" "selected_tab_id:${B_SELECTED_TAB_ID}" "profile B hit-test has selected tab id"
  require_text "$B_HIT_LINE" "overlay_frame=${B_FRAME}" "profile B hit-test uses profile B frame"

  enter_browser_browse "profile_b" "$B_PANE_ID" "$B_BROWSER_TAB_ID"
  B_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE818_PROFILE_B_KEY\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$B_KEY_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "profile B keyboard marker reached profile B"
  require_no_trace_after "$B_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "profile B keyboard marker did not reach profile A"
  leave_browser_browse "profile_b" "$B_PANE_ID" "$B_BROWSER_TAB_ID"

  A_RETURN_START_LINE="$(log_line_count)"
  A_RETURN_STATE_START_LINE="$(state_trace_line_count)"
  log "profile_a_return_keybind=ctrl+p=previous_tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 35 control >>"$HARNESS_LOG" 2>&1
  delay 1
  wait_for_log_after "$A_RETURN_START_LINE" "Pane focus changed: pane_id=${A_PANE_ID} focused=true" "profile A pane focused again after tab switch"
  A_RETURN_WIN_LINE="$(window_bounds)" || fail "failed to resolve profile A return window bounds"
  IFS=$'\t' read -r _A_WID A_WX A_WY _A_WW _A_WH <<<"$A_RETURN_WIN_LINE"
  A_CLICK_X="$(awk -v wx="$A_WX" -v frame_x="$A_FRAME_X" -v frame_size="$A_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
  A_CLICK_Y="$(awk -v wy="$A_WY" -v frame_y="$A_FRAME_Y" -v frame_size="$A_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"
  A_HIT_START_LINE="$(log_line_count)"
  click_global_point "$A_CLICK_X" "$A_CLICK_Y" "profile_a_return_area"
  A_HIT_LINE="$(wait_for_hit_after "$A_HIT_START_LINE" "$A_CONTEXT_ID" "profile A return hit-test")"
  require_text "$A_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "profile A return hit-test has selected tab id"
  require_text "$A_HIT_LINE" "overlay_frame=${A_FRAME}" "profile A return hit-test uses profile A frame"
  enter_browser_browse "profile_a_return" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
  log "profile_a_reload_key=cmd+r"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 15 command >>"$HARNESS_LOG" 2>&1
  wait_for_state_trace_after "$A_RETURN_STATE_START_LINE" "event=console_message.*message=ISSUE818_PROFILE_STORAGE profile=profilea before=profilea after=profilea" "profile A same-origin storage marker after profile B" 45

  A_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE818_PROFILE_A_KEY\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$A_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "profile A keyboard marker reached profile A"
  require_no_trace_after "$A_KEY_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "profile A keyboard marker did not reach profile B"
  leave_browser_browse "profile_a_return" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
fi

if [ "$SCENARIO" = "tui-disconnect-reconnect" ]; then
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  [ -n "$A_SELECTED_TAB_ID" ] || fail "could not extract browser A selected tab id"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  log "browser_a_selected_tab_id=$A_SELECTED_TAB_ID"
  log "browser_a_pane_id=$A_PANE_ID"
  log "browser_a_browser_tab_id=$A_BROWSER_TAB_ID"
  log "browser_a_context_id=$A_CONTEXT_ID"

  require_log "SetOverlay: pane_id=${A_PANE_ID} profile=default browser=${CHROMIUM}" "browser A SetOverlay uses default profile and absolute Chromium path"
  require_log "SetOverlay: created pending server key=default/${CHROMIUM} pane_count=1" "browser A created default profile server"
  SHARED_SPAWN_LINE="$(grep -E "spawned browser path=${CHROMIUM} pid=[0-9]+ profile=default .*--user-data-dir=.*chromium-profiles/default" "$APP_LOG" | tail -1 || true)"
  [ -n "$SHARED_SPAWN_LINE" ] || fail "missing default profile Chromium spawn line"
  SHARED_SPAWN_PID="$(printf '%s\n' "$SHARED_SPAWN_LINE" | sed -E 's/.* pid=([0-9]+) profile=.*/\1/')"
  [ -n "$SHARED_SPAWN_PID" ] || fail "failed to extract shared Chromium pid"
  log "reconnect_shared_spawn_pid=$SHARED_SPAWN_PID"

  cat >"$BROWSER_B_TUI_WRAPPER" <<EOF
#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "\$\$" >"$BROWSER_B_TUI_PID_FILE"
exec "$WEB" --browser "$CHROMIUM" --profile default "$URL_B"
EOF
  chmod +x "$BROWSER_B_TUI_WRAPPER"

  B_TAB_START_LINE="$(log_line_count)"
  B_TAB_TRACE_START_LINE="$(trace_line_count)"
  log "reconnect_browser_b_new_tab_keybind=ctrl+t=new_tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 17 control >>"$HARNESS_LOG" 2>&1
  delay 1
  require_log_after "$B_TAB_START_LINE" "dispatching action target=surface action=.new_tab" "browser B native tab action dispatched"
  require_log_after "$B_TAB_START_LINE" 'starting command command=`/usr/bin/login`' "browser B native tab started login shell"

  log "reconnect_browser_b_select_tab2_keybind=ctrl+2=goto_tab:2"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 19 control >>"$HARNESS_LOG" 2>&1
  delay 1
  B_SELECTED_LINE="$(wait_for_selected_tab_change_after "$B_TAB_START_LINE" "$A_SELECTED_TAB_ID" "browser B tab selected")"
  B_SELECTED_TAB_ID="$(extract_selected_tab_id "$B_SELECTED_LINE")"
  [ -n "$B_SELECTED_TAB_ID" ] || fail "failed to extract browser B selected tab id"
  [ "$B_SELECTED_TAB_ID" != "$A_SELECTED_TAB_ID" ] || fail "browser B reused browser A native tab id"
  log "browser_b_selected_tab_id=$B_SELECTED_TAB_ID"

  printf '"%s"' "$BROWSER_B_TUI_WRAPPER" >"$SECOND_BROWSER_COMMAND"
  log "browser_b_wrapper_command=$(cat "$SECOND_BROWSER_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$SECOND_BROWSER_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  for _ in $(seq 1 30); do
    [ -s "$BROWSER_B_TUI_PID_FILE" ] && break
    delay 1
  done
  [ -s "$BROWSER_B_TUI_PID_FILE" ] || fail "browser B wrapper did not record a web TUI pid"
  B_TUI_PID="$(cat "$BROWSER_B_TUI_PID_FILE")"
  case "$B_TUI_PID" in
    ''|*[!0-9]*) fail "invalid browser B web TUI pid: $B_TUI_PID" ;;
  esac
  if ! ps -p "$B_TUI_PID" -o command= | grep -F -- "$URL_B" >/dev/null 2>&1; then
    fail "browser B recorded pid does not match URL_B command"
  fi
  log "browser_b_tui_pid=$B_TUI_PID"

  B_SET_LINE="$(wait_for_line_after "$B_TAB_START_LINE" "SetOverlay: pane_id=[^ ]+ profile=default browser=${CHROMIUM} url=${URL_B}" "browser B SetOverlay uses default profile and absolute Chromium path" 60)"
  B_PANE_ID="$(printf '%s\n' "$B_SET_LINE" | sed -E 's/.*SetOverlay: pane_id=([^ ]+) .*/\1/')"
  [ -n "$B_PANE_ID" ] || fail "failed to extract browser B pane id"
  [ "$B_PANE_ID" != "$A_PANE_ID" ] || fail "browser B reused browser A pane id"
  wait_for_log_after "$B_TAB_START_LINE" "SetOverlay: reused pending server key=default/${CHROMIUM} pane_count=2 has_fd=true" "browser B reused shared default profile server" 60
  if tail -n +"$((B_TAB_START_LINE + 1))" "$APP_LOG" | grep -E "spawned browser path=${CHROMIUM} pid=[0-9]+ profile=default" >/dev/null 2>&1; then
    fail "browser B spawned a second default profile Chromium process"
  fi
  kill -0 "$SHARED_SPAWN_PID" >/dev/null 2>&1 || fail "shared Chromium pid died while browser B was opening"

  B_CA_CONTEXT_LINE="$(wait_for_line_after "$B_TAB_START_LINE" "TermSurf geometry layer=zig event=ca_context .*pane_id:${B_PANE_ID}" "browser B Zig ca_context" 60)"
  B_BROWSER_TAB_ID="$(extract_browser_tab_id "$B_CA_CONTEXT_LINE")"
  B_CONTEXT_ID="$(extract_context_id "$B_CA_CONTEXT_LINE")"
  [ -n "$B_BROWSER_TAB_ID" ] || fail "failed to extract browser B browser tab id"
  [ -n "$B_CONTEXT_ID" ] || fail "failed to extract browser B context id"
  [ "$B_BROWSER_TAB_ID" != "$A_BROWSER_TAB_ID" ] || fail "browser B reused browser A tab id"
  [ "$B_CONTEXT_ID" != "$A_CONTEXT_ID" ] || fail "browser B reused browser A context id"
  log "browser_b_pane_id=$B_PANE_ID"
  log "browser_b_browser_tab_id=$B_BROWSER_TAB_ID"
  log "browser_b_context_id=$B_CONTEXT_ID"

  B_DISCONNECT_START_LINE="$(log_line_count)"
  B_DISCONNECT_TRACE_START_LINE="$(trace_line_count)"
  log "browser_b_tui_disconnect_kill_pid=$B_TUI_PID"
  kill "$B_TUI_PID" >/dev/null 2>&1 || fail "failed to terminate browser B web TUI pid=$B_TUI_PID"
  for _ in $(seq 1 20); do
    ! kill -0 "$B_TUI_PID" >/dev/null 2>&1 && break
    delay 0.5
  done
  if kill -0 "$B_TUI_PID" >/dev/null 2>&1; then
    kill -9 "$B_TUI_PID" >/dev/null 2>&1 || true
  fi

  wait_for_log_after "$B_DISCONNECT_START_LINE" "TUI disconnect cleanup: pane_id=${B_PANE_ID} tab_id=${B_BROWSER_TAB_ID}" "Ghostboard ran TUI disconnect cleanup for browser B" 45
  require_log_after "$B_DISCONNECT_START_LINE" "CloseTab: pane_id=${B_PANE_ID} tab_id=${B_BROWSER_TAB_ID}" "Ghostboard sent CloseTab for browser B TUI disconnect"
  require_trace_after "$B_DISCONNECT_TRACE_START_LINE" "close-tab tab_id=${B_BROWSER_TAB_ID} pane_id=${B_PANE_ID} result=destroying ffi=ts_destroy_web_contents" "Chromium destroyed browser B after TUI disconnect"
  require_trace_after "$B_DISCONNECT_TRACE_START_LINE" "close-tab tab_id=${B_BROWSER_TAB_ID} result=removed" "Chromium removed browser B after TUI disconnect"
  kill -0 "$SHARED_SPAWN_PID" >/dev/null 2>&1 || fail "shared Chromium pid died after browser B TUI disconnect while browser A remained alive"
  require_no_trace_after "$B_DISCONNECT_TRACE_START_LINE" "close-tab result=no-tabs-remaining" "shared Chromium stayed alive after browser B TUI disconnect"

  log "reconnect_browser_a_select_tab1_keybind=ctrl+1=goto_tab:1"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 18 control >>"$HARNESS_LOG" 2>&1
  delay 1
  enter_browser_browse "reconnect_browser_a_after_b_disconnect" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
  type_marker_require_only "reconnect_browser_a_after_b_disconnect" "ISSUE818_EXP6_BROWSER_A_AFTER_B_DISCONNECT" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$B_BROWSER_TAB_ID" "$B_PANE_ID"
  leave_browser_browse "reconnect_browser_a_after_b_disconnect" "$A_PANE_ID" "$A_BROWSER_TAB_ID"

  C_TAB_START_LINE="$(log_line_count)"
  C_TAB_TRACE_START_LINE="$(trace_line_count)"
  log "reconnect_browser_c_new_tab_keybind=ctrl+t=new_tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 17 control >>"$HARNESS_LOG" 2>&1
  delay 1
  require_log_after "$C_TAB_START_LINE" "dispatching action target=surface action=.new_tab" "browser C native tab action dispatched"
  require_log_after "$C_TAB_START_LINE" 'starting command command=`/usr/bin/login`' "browser C native tab started login shell"
  C_SELECTED_LINE="$(wait_for_selected_tab_not_in_after "$C_TAB_START_LINE" "browser C tab selected" 30 "$A_SELECTED_TAB_ID" "$B_SELECTED_TAB_ID")"
  C_SELECTED_TAB_ID="$(extract_selected_tab_id "$C_SELECTED_LINE")"
  [ -n "$C_SELECTED_TAB_ID" ] || fail "failed to extract browser C selected tab id"
  [ "$C_SELECTED_TAB_ID" != "$A_SELECTED_TAB_ID" ] || fail "browser C reused browser A native tab id"
  [ "$C_SELECTED_TAB_ID" != "$B_SELECTED_TAB_ID" ] || fail "browser C reused browser B native tab id"
  log "browser_c_selected_tab_id=$C_SELECTED_TAB_ID"

  printf '"%s" --browser "%s" --profile default "%s"' "$WEB" "$CHROMIUM" "$URL_C" >"$THIRD_BROWSER_COMMAND"
  log "browser_c_command=$(cat "$THIRD_BROWSER_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$THIRD_BROWSER_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  C_SET_LINE="$(wait_for_line_after "$C_TAB_START_LINE" "SetOverlay: pane_id=[^ ]+ profile=default browser=${CHROMIUM} url=${URL_C}" "browser C SetOverlay uses default profile and absolute Chromium path" 60)"
  C_PANE_ID="$(printf '%s\n' "$C_SET_LINE" | sed -E 's/.*SetOverlay: pane_id=([^ ]+) .*/\1/')"
  [ -n "$C_PANE_ID" ] || fail "failed to extract browser C pane id"
  [ "$C_PANE_ID" != "$A_PANE_ID" ] || fail "browser C reused browser A pane id"
  [ "$C_PANE_ID" != "$B_PANE_ID" ] || fail "browser C reused disconnected browser B pane id"
  wait_for_log_after "$C_TAB_START_LINE" "SetOverlay: reused pending server key=default/${CHROMIUM} pane_count=2 has_fd=true" "browser C reused warm default profile server" 60
  if tail -n +"$((C_TAB_START_LINE + 1))" "$APP_LOG" | grep -E "spawned browser path=${CHROMIUM} pid=[0-9]+ profile=default" >/dev/null 2>&1; then
    fail "browser C spawned a second default profile Chromium process"
  fi
  kill -0 "$SHARED_SPAWN_PID" >/dev/null 2>&1 || fail "shared Chromium pid died while browser C was opening"

  C_CA_CONTEXT_LINE="$(wait_for_line_after "$C_TAB_START_LINE" "TermSurf geometry layer=zig event=ca_context .*pane_id:${C_PANE_ID}" "browser C Zig ca_context" 60)"
  C_BROWSER_TAB_ID="$(extract_browser_tab_id "$C_CA_CONTEXT_LINE")"
  C_CONTEXT_ID="$(extract_context_id "$C_CA_CONTEXT_LINE")"
  [ -n "$C_BROWSER_TAB_ID" ] || fail "failed to extract browser C browser tab id"
  [ -n "$C_CONTEXT_ID" ] || fail "failed to extract browser C context id"
  [ "$C_BROWSER_TAB_ID" != "$A_BROWSER_TAB_ID" ] || fail "browser C reused browser A tab id"
  [ "$C_BROWSER_TAB_ID" != "$B_BROWSER_TAB_ID" ] || fail "browser C reused disconnected browser B tab id"
  [ "$C_CONTEXT_ID" != "$A_CONTEXT_ID" ] || fail "browser C reused browser A context id"
  [ "$C_CONTEXT_ID" != "$B_CONTEXT_ID" ] || fail "browser C reused disconnected browser B context id"
  log "browser_c_pane_id=$C_PANE_ID"
  log "browser_c_browser_tab_id=$C_BROWSER_TAB_ID"
  log "browser_c_context_id=$C_CONTEXT_ID"

  enter_browser_browse "reconnect_browser_c" "$C_PANE_ID" "$C_BROWSER_TAB_ID"
  type_marker_require_only "reconnect_browser_c" "ISSUE818_EXP6_BROWSER_C" "$C_BROWSER_TAB_ID" "$C_PANE_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$B_BROWSER_TAB_ID" "$B_PANE_ID"
  leave_browser_browse "reconnect_browser_c" "$C_PANE_ID" "$C_BROWSER_TAB_ID"

  log "reconnect_browser_a_select_tab1_after_c_keybind=ctrl+1=goto_tab:1"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 18 control >>"$HARNESS_LOG" 2>&1
  delay 1
  enter_browser_browse "reconnect_browser_a_after_c" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
  type_marker_require_only "reconnect_browser_a_after_c" "ISSUE818_EXP6_BROWSER_A_AFTER_C" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$B_BROWSER_TAB_ID" "$B_PANE_ID" "$C_BROWSER_TAB_ID" "$C_PANE_ID"
  leave_browser_browse "reconnect_browser_a_after_c" "$A_PANE_ID" "$A_BROWSER_TAB_ID"

  require_no_trace_after "$B_DISCONNECT_TRACE_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "disconnected browser B received no later keyboard input"
  log "PASS: scenario tui-disconnect-reconnect"
fi

if [ "$SCENARIO" = "visible-profile-identity" ]; then
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  [ -n "$A_SELECTED_TAB_ID" ] || fail "could not extract browser A selected tab id"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  log "identity_browser_a_selected_tab_id=$A_SELECTED_TAB_ID"
  log "identity_browser_a_pane_id=$A_PANE_ID"
  log "identity_browser_a_browser_tab_id=$A_BROWSER_TAB_ID"
  log "identity_browser_a_context_id=$A_CONTEXT_ID"

  require_log "SetOverlay: pane_id=${A_PANE_ID} profile=default browser=${CHROMIUM}" "browser A SetOverlay uses default profile and absolute Chromium path"
  require_trace "tab-ready tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} inspected_tab_id=0" "browser A Chromium tab-ready identity"
  wait_for_state_trace "event=render_state.*identity_label=chromium/default#${A_BROWSER_TAB_ID}.*browser_label=chromium.*profile=default.*is_devtools=false.*current_tab_id=${A_BROWSER_TAB_ID}.*inspected_tab_id=-1" "browser A visible default-profile identity label" 45

  B_TAB_START_LINE="$(log_line_count)"
  B_TAB_TRACE_START_LINE="$(trace_line_count)"
  B_TAB_STATE_START_LINE="$(state_trace_line_count)"
  log "identity_browser_b_new_tab_keybind=ctrl+t=new_tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 17 control >>"$HARNESS_LOG" 2>&1
  delay 1
  require_log_after "$B_TAB_START_LINE" "dispatching action target=surface action=.new_tab" "browser B identity native tab action dispatched"
  require_log_after "$B_TAB_START_LINE" 'starting command command=`/usr/bin/login`' "browser B identity native tab started login shell"

  log "identity_browser_b_select_tab2_keybind=ctrl+2=goto_tab:2"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 19 control >>"$HARNESS_LOG" 2>&1
  delay 1
  B_SELECTED_LINE="$(wait_for_selected_tab_change_after "$B_TAB_START_LINE" "$A_SELECTED_TAB_ID" "browser B identity tab selected")"
  B_SELECTED_TAB_ID="$(extract_selected_tab_id "$B_SELECTED_LINE")"
  [ -n "$B_SELECTED_TAB_ID" ] || fail "failed to extract browser B selected tab id"
  [ "$B_SELECTED_TAB_ID" != "$A_SELECTED_TAB_ID" ] || fail "browser B reused browser A selected tab id"
  log "identity_browser_b_selected_tab_id=$B_SELECTED_TAB_ID"

  printf '"%s" --browser "%s" --profile profilea "%s"' "$WEB" "$CHROMIUM" "$URL_B" >"$SECOND_BROWSER_COMMAND"
  log "identity_browser_b_command=$(cat "$SECOND_BROWSER_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$SECOND_BROWSER_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1

  B_SET_LINE="$(wait_for_line_after "$B_TAB_START_LINE" "SetOverlay: pane_id=[^ ]+ profile=profilea browser=${CHROMIUM} url=${URL_B}" "browser B identity SetOverlay" 60)"
  B_PANE_ID="$(printf '%s\n' "$B_SET_LINE" | sed -E 's/.*SetOverlay: pane_id=([^ ]+) .*/\1/')"
  [ -n "$B_PANE_ID" ] || fail "failed to extract browser B pane id"
  [ "$B_PANE_ID" != "$A_PANE_ID" ] || fail "browser B reused browser A pane id"
  B_TAB_READY_LINE="$(wait_for_trace_line_after "$B_TAB_TRACE_START_LINE" "tab-ready tab=[0-9]+ pane=${B_PANE_ID} inspected_tab_id=0" "browser B Chromium tab-ready identity" 60)"
  B_BROWSER_TAB_ID="$(printf '%s\n' "$B_TAB_READY_LINE" | sed -E 's/.*tab-ready tab=([0-9]+) .*/\1/')"
  [ -n "$B_BROWSER_TAB_ID" ] || fail "failed to extract browser B browser tab id"
  log "identity_browser_b_pane_id=$B_PANE_ID"
  log "identity_browser_b_browser_tab_id=$B_BROWSER_TAB_ID"
  wait_for_state_trace_after "$B_TAB_STATE_START_LINE" "event=render_state.*identity_label=chromium/profilea#${B_BROWSER_TAB_ID}.*browser_label=chromium.*profile=profilea.*is_devtools=false.*current_tab_id=${B_BROWSER_TAB_ID}.*inspected_tab_id=-1" "browser B visible profilea identity label" 45

  DEVTOOLS_START_LINE="$(log_line_count)"
  DEVTOOLS_TRACE_START_LINE="$(trace_line_count)"
  DEVTOOLS_STATE_START_LINE="$(state_trace_line_count)"
  printf ':devtools right' >"$DEVTOOLS_COMMAND"
  log "identity_devtools_command_text=$(cat "$DEVTOOLS_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$DEVTOOLS_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  delay 1

  wait_for_log_after "$DEVTOOLS_START_LINE" "TermSurf QueryDevtoolsRequest pane_id=${B_PANE_ID} inspected_tab_id=${B_BROWSER_TAB_ID}" "identity DevTools query request for browser B" 45
  wait_for_log_after "$DEVTOOLS_START_LINE" "TermSurf QueryDevtoolsReply sent error=" "identity DevTools query reply for browser B" 45
  DEVTOOLS_SET_LINE="$(wait_for_line_after "$DEVTOOLS_START_LINE" "SetDevtoolsOverlay: pane_id=[^ ]+ profile=profilea browser=${CHROMIUM} inspected_tab_id=${B_BROWSER_TAB_ID}" "identity DevTools SetDevtoolsOverlay" 60)"
  DT_PANE_ID="$(printf '%s\n' "$DEVTOOLS_SET_LINE" | sed -E 's/.*SetDevtoolsOverlay: pane_id=([^ ]+) .*/\1/')"
  [ -n "$DT_PANE_ID" ] || fail "failed to extract identity DevTools pane id"
  [ "$DT_PANE_ID" != "$B_PANE_ID" ] || fail "identity DevTools reused browser B pane id"
  wait_for_log_after "$DEVTOOLS_START_LINE" "CreateDevtoolsTab: pane_id=${DT_PANE_ID} inspected_tab_id=${B_BROWSER_TAB_ID}" "Ghostboard sent CreateDevtoolsTab for identity DevTools pane" 60
  DT_TAB_READY_LINE="$(wait_for_trace_line_after "$DEVTOOLS_TRACE_START_LINE" "tab-ready tab=[0-9]+ pane=${DT_PANE_ID} inspected_tab_id=${B_BROWSER_TAB_ID}" "Chromium reported identity DevTools tab ready" 60)"
  DT_BROWSER_TAB_ID="$(printf '%s\n' "$DT_TAB_READY_LINE" | sed -E 's/.*tab-ready tab=([0-9]+) .*/\1/')"
  [ -n "$DT_BROWSER_TAB_ID" ] || fail "failed to extract identity DevTools browser tab id"
  [ "$DT_BROWSER_TAB_ID" != "$B_BROWSER_TAB_ID" ] || fail "identity DevTools browser tab id reused inspected browser tab id"
  log "identity_devtools_pane_id=$DT_PANE_ID"
  log "identity_devtools_browser_tab_id=$DT_BROWSER_TAB_ID"
  wait_for_state_trace_after "$DEVTOOLS_STATE_START_LINE" "event=render_state.*identity_label=chromium/profilea#${B_BROWSER_TAB_ID}.*browser_label=chromium.*profile=profilea.*is_devtools=true.*current_tab_id=${DT_BROWSER_TAB_ID}.*inspected_tab_id=${B_BROWSER_TAB_ID}" "DevTools visible inspected-tab profile identity label" 45

  log "PASS: scenario visible-profile-identity"
fi

if [ "$SCENARIO" = "javascript-dialog-smoke" ]; then
  DIALOG_AWAY_URL="http://127.0.0.1:${DIALOG_HTTP_PORT}/away.html"

  dialog_request_id() {
    local start_line="$1"
    local pattern="$2"
    tail -n +"$((start_line + 1))" "$WEBTUI_STATE_TRACE" \
      | grep -E "$pattern" \
      | tail -1 \
      | sed -E 's/.*request_id=([0-9]+).*/\1/'
  }

  press_dialog_key() {
    local key="$1"
    local label="$2"
    case "$key" in
      enter) swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1 ;;
      esc) swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1 ;;
      y) swift "$ROOT/scripts/ghostty-app/inject.swift" key 16 >>"$HARNESS_LOG" 2>&1 ;;
      n) swift "$ROOT/scripts/ghostty-app/inject.swift" key 45 >>"$HARNESS_LOG" 2>&1 ;;
      *) fail "unsupported dialog key: $key" ;;
    esac
    log "javascript_dialog_key_${label}=$key"
  }

  run_dialog_button_case() {
    local label="$1"
    local web_y="$2"
    local dialog_type="$3"
    local message="$4"
    local reply_key="$5"
    local accepted="$6"
    local prompt_text="$7"
    local result_value="$8"
    local start_line request_id click_x click_y

    start_line="$(state_trace_line_count)"
    IFS=$'\t' read -r click_x click_y <<<"$(global_point_for_web_point "$DIALOG_WIN_LINE" "$APPKIT_PRESENT_LINE" 150 "$web_y")"
    click_global_point "$click_x" "$click_y" "javascript_dialog_${label}"
    wait_for_state_trace_after "$start_line" "event=javascript_dialog_request.*dialog_type=${dialog_type}.*message=${message}" "webtui ${label} dialog request" 45
    request_id="$(dialog_request_id "$start_line" "event=javascript_dialog_request.*dialog_type=${dialog_type}.*message=${message}")"
    [ -n "$request_id" ] || fail "missing request id for ${label}"
    wait_for_trace_regex "javascript-dialog-request tab=${BROWSER_TAB_ID} .*request_id=${request_id} type=${dialog_type} .*message=${message}" "Chromium ${label} dialog request" 45
    if [ -n "$prompt_text" ]; then
      printf '%s' "$prompt_text" >"$DIALOG_TYPE_COMMAND"
      swift "$ROOT/scripts/ghostty-app/inject.swift" type "$DIALOG_TYPE_COMMAND" >>"$HARNESS_LOG" 2>&1
      log "javascript_dialog_type_${label}=$prompt_text"
    fi
    press_dialog_key "$reply_key" "$label"
    wait_for_state_trace_after "$start_line" "event=javascript_dialog_reply.*request_id=${request_id}.*accepted=${accepted}" "webtui ${label} dialog reply" 45
    wait_for_trace_regex "javascript-dialog-reply tab=${BROWSER_TAB_ID} .*request_id=${request_id} accepted=${accepted} ok=true" "Chromium ${label} dialog reply" 45
    wait_for_state_trace_after "$start_line" "event=console_message.*message=ISSUE816_DIALOG_RESULT case=${label} value=${result_value}" "page observed ${label} result" 45
    wait_for_state_trace_after "$start_line" "event=title_changed.*title=Issue 26061712000816 Dialog ${label} ${result_value}" "webtui received ${label} title" 45
  }

  wait_for_state_trace "event=javascript_dialog_request.*dialog_type=alert.*message=ISSUE816_INITIAL_ALERT" "webtui initial alert dialog request" 45
  INITIAL_DIALOG_REQUEST_ID="$(dialog_request_id 0 "event=javascript_dialog_request.*dialog_type=alert.*message=ISSUE816_INITIAL_ALERT")"
  [ -n "$INITIAL_DIALOG_REQUEST_ID" ] || fail "missing request id for initial alert"
  wait_for_trace_regex "javascript-dialog-request tab=${BROWSER_TAB_ID} .*request_id=${INITIAL_DIALOG_REQUEST_ID} type=alert .*message=ISSUE816_INITIAL_ALERT" "Chromium initial alert request" 45
  press_dialog_key enter "initial_alert"
  wait_for_state_trace "event=javascript_dialog_reply.*request_id=${INITIAL_DIALOG_REQUEST_ID}.*accepted=true" "webtui initial alert reply" 45
  wait_for_trace_regex "javascript-dialog-reply tab=${BROWSER_TAB_ID} .*request_id=${INITIAL_DIALOG_REQUEST_ID} accepted=true ok=true" "Chromium initial alert reply" 45
  wait_for_state_trace "event=console_message.*message=ISSUE816_DIALOG_RESULT case=initial-alert value=resumed" "page observed initial alert result" 45
  wait_for_state_trace "event=title_changed.*title=Issue 26061712000816 Dialog initial-alert resumed" "webtui received initial alert title" 45

  DIALOG_WIN_LINE="$(window_bounds)" || fail "failed to resolve JavaScript dialog window bounds"
  run_dialog_button_case "alert" 84 "alert" "ISSUE816_ALERT" "enter" "true" "" "resumed"
  run_dialog_button_case "confirm-accept" 134 "confirm" "ISSUE816_CONFIRM_ACCEPT" "y" "true" "" "true"
  run_dialog_button_case "confirm-cancel" 184 "confirm" "ISSUE816_CONFIRM_CANCEL" "n" "false" "" "false"
  run_dialog_button_case "prompt-accept" 234 "prompt" "ISSUE816_PROMPT_ACCEPT" "enter" "true" "-typed" "seed-typed"
  run_dialog_button_case "prompt-cancel" 284 "prompt" "ISSUE816_PROMPT_CANCEL" "esc" "false" "" "null"

  BEFOREUNLOAD_ARM_START_LINE="$(state_trace_line_count)"
  IFS=$'\t' read -r BEFOREUNLOAD_ARM_X BEFOREUNLOAD_ARM_Y <<<"$(global_point_for_web_point "$DIALOG_WIN_LINE" "$APPKIT_PRESENT_LINE" 150 334)"
  click_global_point "$BEFOREUNLOAD_ARM_X" "$BEFOREUNLOAD_ARM_Y" "javascript_dialog_beforeunload_arm"
  wait_for_state_trace_after "$BEFOREUNLOAD_ARM_START_LINE" "event=console_message.*message=ISSUE816_DIALOG_RESULT case=beforeunload-armed value=true" "page observed beforeunload activation" 45
  wait_for_state_trace_after "$BEFOREUNLOAD_ARM_START_LINE" "event=title_changed.*title=Issue 26061712000816 Dialog beforeunload-armed true" "webtui received beforeunload armed title" 45

  BEFOREUNLOAD_CANCEL_START_LINE="$(state_trace_line_count)"
  IFS=$'\t' read -r BEFOREUNLOAD_LEAVE_X BEFOREUNLOAD_LEAVE_Y <<<"$(global_point_for_web_point "$DIALOG_WIN_LINE" "$APPKIT_PRESENT_LINE" 150 384)"
  click_global_point "$BEFOREUNLOAD_LEAVE_X" "$BEFOREUNLOAD_LEAVE_Y" "javascript_dialog_beforeunload_stay"
  wait_for_state_trace_after "$BEFOREUNLOAD_CANCEL_START_LINE" "event=javascript_dialog_request.*dialog_type=beforeunload" "webtui beforeunload stay request" 45
  BEFOREUNLOAD_CANCEL_REQUEST_ID="$(dialog_request_id "$BEFOREUNLOAD_CANCEL_START_LINE" "event=javascript_dialog_request.*dialog_type=beforeunload")"
  [ -n "$BEFOREUNLOAD_CANCEL_REQUEST_ID" ] || fail "missing request id for beforeunload stay"
  wait_for_trace_regex "javascript-dialog-request tab=${BROWSER_TAB_ID} .*request_id=${BEFOREUNLOAD_CANCEL_REQUEST_ID} type=beforeunload" "Chromium beforeunload stay request" 45
  press_dialog_key n "beforeunload_stay"
  wait_for_state_trace_after "$BEFOREUNLOAD_CANCEL_START_LINE" "event=javascript_dialog_reply.*request_id=${BEFOREUNLOAD_CANCEL_REQUEST_ID}.*accepted=false" "webtui beforeunload stay reply" 45
  wait_for_trace_regex "javascript-dialog-reply tab=${BROWSER_TAB_ID} .*request_id=${BEFOREUNLOAD_CANCEL_REQUEST_ID} accepted=false ok=true" "Chromium beforeunload stay reply" 45
  delay 2
  require_no_state_trace_after "$BEFOREUNLOAD_CANCEL_START_LINE" "event=url_changed[[:space:]]+url=${DIALOG_AWAY_URL}" "beforeunload stay did not emit away URL"
  require_no_state_trace_after "$BEFOREUNLOAD_CANCEL_START_LINE" "event=title_changed.*title=Issue 26061712000816 Dialog away loaded" "beforeunload stay did not navigate away"

  BEFOREUNLOAD_PROCEED_START_LINE="$(state_trace_line_count)"
  click_global_point "$BEFOREUNLOAD_LEAVE_X" "$BEFOREUNLOAD_LEAVE_Y" "javascript_dialog_beforeunload_proceed"
  wait_for_state_trace_after "$BEFOREUNLOAD_PROCEED_START_LINE" "event=javascript_dialog_request.*dialog_type=beforeunload" "webtui beforeunload proceed request" 45
  BEFOREUNLOAD_PROCEED_REQUEST_ID="$(dialog_request_id "$BEFOREUNLOAD_PROCEED_START_LINE" "event=javascript_dialog_request.*dialog_type=beforeunload")"
  [ -n "$BEFOREUNLOAD_PROCEED_REQUEST_ID" ] || fail "missing request id for beforeunload proceed"
  wait_for_trace_regex "javascript-dialog-request tab=${BROWSER_TAB_ID} .*request_id=${BEFOREUNLOAD_PROCEED_REQUEST_ID} type=beforeunload" "Chromium beforeunload proceed request" 45
  press_dialog_key y "beforeunload_proceed"
  wait_for_state_trace_after "$BEFOREUNLOAD_PROCEED_START_LINE" "event=javascript_dialog_reply.*request_id=${BEFOREUNLOAD_PROCEED_REQUEST_ID}.*accepted=true" "webtui beforeunload proceed reply" 45
  wait_for_trace_regex "javascript-dialog-reply tab=${BROWSER_TAB_ID} .*request_id=${BEFOREUNLOAD_PROCEED_REQUEST_ID} accepted=true ok=true" "Chromium beforeunload proceed reply" 45
  wait_for_state_trace_after "$BEFOREUNLOAD_PROCEED_START_LINE" "event=url_changed[[:space:]]+url=${DIALOG_AWAY_URL}" "beforeunload proceed navigated away" 45
  wait_for_state_trace_after "$BEFOREUNLOAD_PROCEED_START_LINE" "event=title_changed[[:space:]]+title=Issue 26061712000816 Dialog away loaded" "beforeunload proceed loaded away title" 45
  wait_for_state_trace_after "$BEFOREUNLOAD_PROCEED_START_LINE" "event=console_message.*message=ISSUE816_DIALOG_RESULT case=beforeunload-proceed value=away" "page observed beforeunload proceed result" 45
fi

if [ "$SCENARIO" = "http-auth-smoke" ]; then
  auth_request_id() {
    local start_line="$1"
    local pattern="$2"
    tail -n +"$((start_line + 1))" "$WEBTUI_STATE_TRACE" \
      | grep -E "$pattern" \
      | tail -1 \
      | sed -E 's/.*request_id=([0-9]+).*/\1/'
  }

  type_auth_text() {
    local value="$1"
    local label="$2"
    printf '%s' "$value" >"$AUTH_TYPE_COMMAND"
    swift "$ROOT/scripts/ghostty-app/inject.swift" type "$AUTH_TYPE_COMMAND" >>"$HARNESS_LOG" 2>&1
    log "http_auth_type_${label}_len=${#value}"
  }

  wait_for_state_trace "event=http_auth_request.*auth_scheme=basic.*realm=Issue816Auth" "webtui HTTP auth request" 45
  AUTH_SUCCESS_REQUEST_ID="$(auth_request_id 0 "event=http_auth_request.*auth_scheme=basic.*realm=Issue816Auth")"
  [ -n "$AUTH_SUCCESS_REQUEST_ID" ] || fail "missing request id for HTTP auth success"
  wait_for_trace_regex "http-auth-request tab=${BROWSER_TAB_ID} .*request_id=${AUTH_SUCCESS_REQUEST_ID} .*scheme=basic .*realm=Issue816Auth" "Chromium HTTP auth request" 45
  type_auth_text "user" "username"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  log "http_auth_key_username=enter"
  type_auth_text "passwd" "password"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  log "http_auth_key_password=enter"
  wait_for_state_trace "event=http_auth_reply.*request_id=${AUTH_SUCCESS_REQUEST_ID}.*accepted=true.*username=user.*password_len=6" "webtui HTTP auth success reply" 45
  wait_for_trace_regex "http-auth-reply tab=${BROWSER_TAB_ID} .*request_id=${AUTH_SUCCESS_REQUEST_ID} accepted=true username=user password_len=6 ok=true" "Chromium HTTP auth success reply" 45
  wait_for_state_trace "event=title_changed.*title=Issue 26061712000816 Auth Success" "authenticated page title" 45
  wait_for_state_trace "event=console_message.*message=ISSUE816_AUTH_SUCCESS" "authenticated page success marker" 45

  AUTH_WIN_LINE="$(window_bounds)" || fail "failed to resolve HTTP auth window bounds"
  AUTH_CANCEL_START_LINE="$(state_trace_line_count)"
  AUTH_CANCEL_URL="http://127.0.0.1:${AUTH_HTTP_PORT}/auth-cancel/cancel.html"
  IFS=$'\t' read -r AUTH_CANCEL_X AUTH_CANCEL_Y <<<"$(global_point_for_web_point "$AUTH_WIN_LINE" "$APPKIT_PRESENT_LINE" 140 58)"
  click_global_point "$AUTH_CANCEL_X" "$AUTH_CANCEL_Y" "http_auth_cancel_link"
  wait_for_state_trace_after "$AUTH_CANCEL_START_LINE" "event=http_auth_request.*url=${AUTH_CANCEL_URL}.*auth_scheme=basic.*realm=Issue816Cancel" "webtui HTTP auth cancel request" 45
  AUTH_CANCEL_REQUEST_ID="$(auth_request_id "$AUTH_CANCEL_START_LINE" "event=http_auth_request.*url=${AUTH_CANCEL_URL}.*auth_scheme=basic.*realm=Issue816Cancel")"
  [ -n "$AUTH_CANCEL_REQUEST_ID" ] || fail "missing request id for HTTP auth cancel"
  wait_for_trace_regex "http-auth-request tab=${BROWSER_TAB_ID} .*request_id=${AUTH_CANCEL_REQUEST_ID} .*scheme=basic .*realm=Issue816Cancel" "Chromium HTTP auth cancel request" 45
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  log "http_auth_key_cancel=esc"
  wait_for_state_trace_after "$AUTH_CANCEL_START_LINE" "event=http_auth_reply.*request_id=${AUTH_CANCEL_REQUEST_ID}.*accepted=false.*password_len=0" "webtui HTTP auth cancel reply" 45
  wait_for_trace_regex "http-auth-reply tab=${BROWSER_TAB_ID} .*request_id=${AUTH_CANCEL_REQUEST_ID} accepted=false username= password_len=0 ok=true" "Chromium HTTP auth cancel reply" 45
  require_no_state_trace_after "$AUTH_CANCEL_START_LINE" "event=console_message.*message=ISSUE816_AUTH_CANCEL_UNEXPECTED_SUCCESS" "HTTP auth cancel did not authenticate protected page"

  AUTH_PUBLIC_START_LINE="$(state_trace_line_count)"
  AUTH_PUBLIC_URL="http://127.0.0.1:${AUTH_HTTP_PORT}/public.html"
  log "http_auth_public_edit_key=shift+a=edit-url-end"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 0 shift >>"$HARNESS_LOG" 2>&1
  delay 0.5
  for _ in $(seq 1 120); do
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 51 >>"$HARNESS_LOG" 2>&1
  done
  printf '%s' "$AUTH_PUBLIC_URL" >"$AUTH_TYPE_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$AUTH_TYPE_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_state_trace_after "$AUTH_PUBLIC_START_LINE" "event=url_changed[[:space:]]+url=${AUTH_PUBLIC_URL}" "public navigation after auth cancel" 45
  wait_for_state_trace_after "$AUTH_PUBLIC_START_LINE" "event=title_changed.*title=Issue 26061712000816 Auth Public" "public page title after auth cancel" 45
  wait_for_state_trace_after "$AUTH_PUBLIC_START_LINE" "event=console_message.*message=ISSUE816_AUTH_PUBLIC" "public page marker after auth cancel" 45

  if grep -F "passwd" "$APP_LOG" "$CHROMIUM_TRACE" "$WEBTUI_STATE_TRACE" "$HARNESS_LOG" >/dev/null 2>&1; then
    fail "HTTP auth password leaked into logs or traces"
  fi
  log "PASS: HTTP auth password did not appear in logs or traces"
fi

if [ "$SCENARIO" = "renderer-crash-smoke" ]; then
  CRASH_URL="chrome://crash/"
  CRASH_RECOVERY_URL="http://127.0.0.1:${CRASH_HTTP_PORT}/recovery.html"

  wait_for_state_trace "event=url_changed.*url=${URL}" "webtui initial crash fixture URL" 45
  wait_for_state_trace "event=title_changed.*title=Issue 26061712000816 Crash Initial Ready" "webtui initial crash fixture title" 45
  wait_for_state_trace "event=console_message.*message=ISSUE816_CRASH_INITIAL_READY" "webtui initial crash fixture console marker" 45

  CRASH_TRACE_START_LINE="$(trace_line_count)"
  CRASH_STATE_START_LINE="$(state_trace_line_count)"
  log "renderer_crash_navigate_url=${CRASH_URL}"
  edit_url_replace "renderer_crash" "$CRASH_URL"

  CRASH_TRACE_LINE="$(wait_for_trace_line_after \
    "$CRASH_TRACE_START_LINE" \
    "renderer-crashed tab=${BROWSER_TAB_ID} pane=${PANE_ID} status=crashed code=[0-9-]+ url=chrome://crash/ can_reload=true" \
    "Chromium renderer crash event" \
    60)"
  log "PASS: Chromium renderer crash event: $CRASH_TRACE_LINE"
  wait_for_state_trace_after "$CRASH_STATE_START_LINE" "event=renderer_crashed.*tab_id=${BROWSER_TAB_ID}.*status=crashed.*url=chrome://crash/.*can_reload=true" "webtui renderer crash state event" 60
  wait_for_state_trace_after "$CRASH_STATE_START_LINE" "event=render_state.*loading_bar_active=false.*renderer_crash_active=true.*renderer_crash_tab_id=${BROWSER_TAB_ID}.*renderer_crash_status=crashed" "webtui render state shows active crash without stuck loading" 60

  CRASH_EVENT_LINE="$(
    grep -nE "event=renderer_crashed.*tab_id=${BROWSER_TAB_ID}.*status=crashed.*url=chrome://crash/.*can_reload=true" "$WEBTUI_STATE_TRACE" |
      tail -1 |
      cut -d: -f1
  )"
  case "$CRASH_EVENT_LINE" in
    ''|*[!0-9]*) fail "could not locate webtui renderer crash event line" ;;
  esac
  delay 2
  require_no_state_trace_after "$CRASH_EVENT_LINE" "event=render_state.*renderer_crash_active=false" "stale post-crash events did not clear crash state before recovery"
  require_no_state_trace_after "$CRASH_EVENT_LINE" "event=loading_state.*state=loading" "stale post-crash events did not restart loading before recovery"

  RECOVERY_TRACE_START_LINE="$(trace_line_count)"
  RECOVERY_STATE_START_LINE="$(state_trace_line_count)"
  log "renderer_crash_recovery_url=${CRASH_RECOVERY_URL}"
  edit_url_replace "renderer_crash_recovery" "$CRASH_RECOVERY_URL"
  wait_for_trace_line_after "$RECOVERY_TRACE_START_LINE" "navigate tab=${BROWSER_TAB_ID} pane=${PANE_ID} url=${CRASH_RECOVERY_URL}" "Chromium recovery navigation" 60 >/dev/null
  wait_for_state_trace_after "$RECOVERY_STATE_START_LINE" "event=url_changed[[:space:]]+url=${CRASH_RECOVERY_URL}" "webtui recovery URL" 60
  wait_for_state_trace_after "$RECOVERY_STATE_START_LINE" "event=title_changed.*title=Issue 26061712000816 Crash Recovery" "webtui recovery title" 60
  wait_for_state_trace_after "$RECOVERY_STATE_START_LINE" "event=console_message.*message=ISSUE816_CRASH_RECOVERY" "webtui recovery console marker" 60
  wait_for_state_trace_after "$RECOVERY_STATE_START_LINE" "event=render_state.*title=Issue 26061712000816 Crash Recovery.*loading_bar_active=false.*renderer_crash_active=false.*latest_console=ISSUE816_CRASH_RECOVERY" "webtui render state cleared crash after recovery"
  require_trace_after "$RECOVERY_TRACE_START_LINE" "title-changed tab=${BROWSER_TAB_ID} pane=${PANE_ID} title=Issue 26061712000816 Crash Recovery" "Chromium stayed alive through recovery"
fi

if [ "$SCENARIO" = "color-scheme-smoke" ]; then
  assert_color_scheme_command() {
    local label="$1"
    local command="$2"
    local action="$3"
    local scheme="$4"
    local dark="$5"
    local source="$6"
    local trace_start_line state_start_line

    trace_start_line="$(trace_line_count)"
    state_start_line="$(state_trace_line_count)"
    run_webtui_command "$label" "$command"
    wait_for_state_trace_after "$state_start_line" "event=color_scheme_command.*action=${action}.*scheme=${scheme}.*dark=${dark}.*source=${source}.*tab_id=${BROWSER_TAB_ID}" "webtui accepted ${label} color scheme command" 45
    require_trace_after "$trace_start_line" "set-color-scheme tab=${BROWSER_TAB_ID} pane=${PANE_ID} dark=${dark} ffi=ts_set_color_scheme" "Chromium received ${label} SetColorScheme"
    wait_for_state_trace_after "$state_start_line" "event=console_message.*message=ISSUE816_COLOR_SCHEME .*scheme=${scheme}" "page observed ${label} ${scheme} media scheme" 45
    wait_for_state_trace_after "$state_start_line" "event=title_changed.*title=Issue 26061712000816 Color .* ${scheme}" "webtui received ${label} ${scheme} title" 45
  }

  wait_for_state_trace "event=url_changed.*url=${URL}" "webtui initial color fixture URL" 45
  wait_for_trace_regex "create-tab pane=${PANE_ID} .*dark=true" "Chromium initial CreateTab used system dark scheme" 45
  wait_for_state_trace "event=console_message.*message=ISSUE816_COLOR_SCHEME .*label=ready" "webtui initial color fixture ready marker" 45
  wait_for_state_trace "event=render_state.*browser_ready=true" "webtui color fixture browser ready render state" 45

  assert_color_scheme_command "dark_off" "dark off" "off" "light" "false" "explicit-off"
  assert_color_scheme_command "dark_on" "dark on" "on" "dark" "true" "explicit-on"
  assert_color_scheme_command "dark_system" "dark system" "system" "dark" "true" "macos-defaults"
fi

if [ "$SCENARIO" = "copy-current-url-smoke" ]; then
  wait_for_state_trace "event=url_changed.*url=${URL}" "webtui copy URL fixture URL" 45
  wait_for_state_trace "event=title_changed.*title=Issue 26061712000816 Copy URL Ready" "webtui copy URL fixture title" 45
  wait_for_state_trace "event=console_message.*message=ISSUE816_COPY_URL_READY ${URL}" "webtui copy URL fixture ready marker" 45

  COPY_URL_SENTINEL="ISSUE816_COPY_URL_SENTINEL_${TS}"
  printf '%s' "$COPY_URL_SENTINEL" | pbcopy
  CLIPBOARD_BEFORE="$(pbpaste)"
  [ "$CLIPBOARD_BEFORE" = "$COPY_URL_SENTINEL" ] || fail "failed to set clipboard sentinel before copy-current-url"
  log "copy_current_url_clipboard_before=$CLIPBOARD_BEFORE"

  COPY_APP_START_LINE="$(log_line_count)"
  log "copy_current_url_key=cmd+c"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 8 command >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$COPY_APP_START_LINE" "CopyCurrentUrl: pane_id=${PANE_ID} url=${URL}" "Ghostboard selected current URL for Control-mode copy" 45
  wait_for_log_after "$COPY_APP_START_LINE" "TermSurf geometry layer=appkit event=copy_current_url .*pane_id:${PANE_ID} .*note=pane_id=${PANE_ID} url=${URL} mode=control" "AppKit copied current URL in Control mode" 45
  wait_for_log_after "$COPY_APP_START_LINE" "TermSurf geometry layer=appkit event=copy_current_url_feedback .*pane_id:${PANE_ID} .*note=message=URL copied" "AppKit showed copy-current-URL feedback" 45

  for _ in $(seq 1 20); do
    CLIPBOARD_AFTER="$(pbpaste)"
    if [ "$CLIPBOARD_AFTER" = "$URL" ]; then
      break
    fi
    delay 0.25
  done
  [ "$CLIPBOARD_AFTER" = "$URL" ] || fail "clipboard after copy-current-url did not match current URL: got=$CLIPBOARD_AFTER expected=$URL"
  [ "$CLIPBOARD_AFTER" != "$COPY_URL_SENTINEL" ] || fail "copy-current-url left clipboard sentinel unchanged"
  log "PASS: copy-current-url clipboard matched current URL"

  enter_browser_browse "copy_current_url_browse_guard" "$PANE_ID" "$BROWSER_TAB_ID"
  BROWSE_COPY_SENTINEL="ISSUE816_COPY_URL_BROWSE_SENTINEL_${TS}"
  printf '%s' "$BROWSE_COPY_SENTINEL" | pbcopy
  BROWSE_CLIPBOARD_BEFORE="$(pbpaste)"
  [ "$BROWSE_CLIPBOARD_BEFORE" = "$BROWSE_COPY_SENTINEL" ] || fail "failed to set clipboard sentinel before Browse-mode Cmd+C"

  BROWSE_COPY_APP_START_LINE="$(log_line_count)"
  log "copy_current_url_browse_key=cmd+c"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 8 command >>"$HARNESS_LOG" 2>&1
  delay 1
  require_no_log_after "$BROWSE_COPY_APP_START_LINE" "CopyCurrentUrl: pane_id=${PANE_ID} url=${URL}" "Browse-mode Cmd+C did not select TermSurf current URL"
  require_no_log_after "$BROWSE_COPY_APP_START_LINE" "TermSurf geometry layer=appkit event=copy_current_url .*pane_id:${PANE_ID}" "Browse-mode Cmd+C did not run AppKit copy-current-URL"
  BROWSE_CLIPBOARD_AFTER="$(pbpaste)"
  [ "$BROWSE_CLIPBOARD_AFTER" = "$BROWSE_COPY_SENTINEL" ] || fail "Browse-mode Cmd+C overwrote clipboard: got=$BROWSE_CLIPBOARD_AFTER expected=$BROWSE_COPY_SENTINEL"
  log "PASS: Browse-mode Cmd+C left copy-current-URL guard inactive"
fi

if [ "$SCENARIO" = "webapp-clipboard-copy" ]; then
  wait_for_state_trace "event=url_changed.*url=${URL}" "webapp clipboard fixture URL" 45
  wait_for_state_trace "event=console_message.*message=ISSUE840_CLIPBOARD .*kind=ready .*secure=true .*clipboard=true" "webapp clipboard fixture ready marker" 45

  pbpaste >"$CLIPBOARD_ORIGINAL" || true
  CLIPBOARD_SENTINEL="ISSUE840_CLIPBOARD_SENTINEL_${TS}"
  CLIPBOARD_API_TOKEN="ISSUE840_CLIPBOARD_API_TOKEN"
  EXEC_COMMAND_TOKEN="ISSUE840_EXEC_COMMAND_TOKEN"
  printf '%s' "$CLIPBOARD_SENTINEL" | pbcopy
  CLIPBOARD_BEFORE="$(pbpaste)"
  [ "$CLIPBOARD_BEFORE" = "$CLIPBOARD_SENTINEL" ] || fail "failed to set clipboard sentinel before webapp copy"
  log "webapp_clipboard_before=$CLIPBOARD_BEFORE"

  read -r CLIPBOARD_API_X CLIPBOARD_API_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$APPKIT_PRESENT_LINE" 180 80)"
  read -r EXEC_COMMAND_X EXEC_COMMAND_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$APPKIT_PRESENT_LINE" 180 140)"

  API_APP_START_LINE="$(log_line_count)"
  API_TRACE_START_LINE="$(trace_line_count)"
  API_STATE_START_LINE="$(state_trace_line_count)"
  click_global_point "$CLIPBOARD_API_X" "$CLIPBOARD_API_Y" "webapp_clipboard_api_button"
  wait_for_state_trace_after "$API_STATE_START_LINE" "event=console_message.*message=ISSUE840_CLIPBOARD .*kind=clipboard-api-start .*token=${CLIPBOARD_API_TOKEN}" "page started Clipboard API copy handler" 45
  wait_for_log_after "$API_APP_START_LINE" "ClickModeChanged: pane_id=${PANE_ID} browsing=true reason=overlay_click" "Ghostboard entered Browse from webapp clipboard click" 45
  require_trace_after "$API_TRACE_START_LINE" "mouse-event tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_forward_mouse_event type=down button=left" "Chromium received webapp clipboard click"

  API_CLIPBOARD_AFTER="$CLIPBOARD_SENTINEL"
  for _ in $(seq 1 20); do
    API_CLIPBOARD_AFTER="$(pbpaste)"
    if [ "$API_CLIPBOARD_AFTER" = "$CLIPBOARD_API_TOKEN" ]; then
      break
    fi
    delay 0.25
  done

  if [ "$API_CLIPBOARD_AFTER" = "$CLIPBOARD_API_TOKEN" ]; then
    wait_for_state_trace_after "$API_STATE_START_LINE" "event=console_message.*message=ISSUE840_CLIPBOARD .*kind=clipboard-api-success .*token=${CLIPBOARD_API_TOKEN}" "page reported Clipboard API success" 45
    log "PASS: webapp Clipboard API wrote expected token to pbpaste"
    log "PASS: scenario webapp-clipboard-copy"
  else
    wait_for_state_trace_after "$API_STATE_START_LINE" "event=console_message.*message=ISSUE840_CLIPBOARD .*kind=clipboard-api-(success|error)" "page reported Clipboard API result" 45
    log "WARN: webapp Clipboard API did not update pbpaste: got=$API_CLIPBOARD_AFTER expected=$CLIPBOARD_API_TOKEN"

    printf '%s' "$CLIPBOARD_SENTINEL" | pbcopy
    EXEC_STATE_START_LINE="$(state_trace_line_count)"
    click_global_point "$EXEC_COMMAND_X" "$EXEC_COMMAND_Y" "webapp_exec_command_button"
    wait_for_state_trace_after "$EXEC_STATE_START_LINE" "event=console_message.*message=ISSUE840_CLIPBOARD .*kind=exec-command-start .*token=${EXEC_COMMAND_TOKEN}" "page started execCommand copy handler" 45
    wait_for_state_trace_after "$EXEC_STATE_START_LINE" "event=console_message.*message=ISSUE840_CLIPBOARD .*kind=exec-command-(success|false|error)" "page reported execCommand copy result" 45

    EXEC_CLIPBOARD_AFTER="$CLIPBOARD_SENTINEL"
    for _ in $(seq 1 20); do
      EXEC_CLIPBOARD_AFTER="$(pbpaste)"
      if [ "$EXEC_CLIPBOARD_AFTER" = "$EXEC_COMMAND_TOKEN" ]; then
        break
      fi
      delay 0.25
    done

    if [ "$EXEC_CLIPBOARD_AFTER" = "$EXEC_COMMAND_TOKEN" ]; then
      fail "webapp clipboard partial: Clipboard API failed but execCommand copy reached pbpaste"
    fi
    fail "webapp clipboard failed: neither Clipboard API nor execCommand copy reached pbpaste; api_after=$API_CLIPBOARD_AFTER exec_after=$EXEC_CLIPBOARD_AFTER sentinel=$CLIPBOARD_SENTINEL"
  fi
fi

if [ "$SCENARIO" = "browser-input-granularity" ]; then
  wait_for_state_trace "event=url_changed.*url=${URL}" "webtui browser-input fixture URL" 45
  wait_for_state_trace "event=console_message.*message=ISSUE817_INPUT .*kind=ready" "browser-input fixture ready marker" 45

  enter_browser_browse "browser_input" "$PANE_ID" "$BROWSER_TAB_ID"

  read -r INPUT_X INPUT_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$APPKIT_PRESENT_LINE" 120 78)"
  read -r CLICK_X CLICK_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$APPKIT_PRESENT_LINE" 120 192)"
  read -r DRAG_START_X DRAG_START_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$APPKIT_PRESENT_LINE" 65 266)"
  read -r DRAG_END_X DRAG_END_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$APPKIT_PRESENT_LINE" 360 266)"

  INPUT_FOCUS_START_LINE="$(state_trace_line_count)"
  click_global_point "$INPUT_X" "$INPUT_Y" "browser_input_text_field"
  wait_for_state_trace_after "$INPUT_FOCUS_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=focus .*active=text-input" "page focused text input" 45

  INPUT_TOKEN="issue817"
  printf '%s' "$INPUT_TOKEN" >"$INPUT_TYPE_COMMAND"
  INPUT_TYPE_START_LINE="$(state_trace_line_count)"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$INPUT_TYPE_COMMAND" >>"$HARNESS_LOG" 2>&1
  wait_for_state_trace_after "$INPUT_TYPE_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=input .*value=${INPUT_TOKEN} .*start=8 .*end=8 .*active=text-input" "page received typed token" 45

  LEFT_START_LINE="$(state_trace_line_count)"
  log "browser_input_special_key=left-arrow"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 123 >>"$HARNESS_LOG" 2>&1
  wait_for_state_trace_after "$LEFT_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=keyup .*label=ArrowLeft .*value=${INPUT_TOKEN} .*start=7 .*end=7 .*key=ArrowLeft" "page observed left-arrow caret movement" 45

  INSERT_START_LINE="$(state_trace_line_count)"
  printf 'x' >"$INPUT_TYPE_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$INPUT_TYPE_COMMAND" >>"$HARNESS_LOG" 2>&1
  wait_for_state_trace_after "$INSERT_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=input .*value=issue81x7 .*start=8 .*end=8 .*active=text-input" "page inserted character at caret" 45

  BACKSPACE_START_LINE="$(state_trace_line_count)"
  log "browser_input_special_key=backspace"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 51 >>"$HARNESS_LOG" 2>&1
  wait_for_state_trace_after "$BACKSPACE_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=input .*value=${INPUT_TOKEN} .*start=7 .*end=7 .*active=text-input" "page observed backspace deletion" 45

  TAB_START_LINE="$(state_trace_line_count)"
  log "browser_input_special_key=tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 48 >>"$HARNESS_LOG" 2>&1
  wait_for_state_trace_after "$TAB_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=focus .*active=next-control" "page observed tab focus movement" 45

  ENTER_START_LINE="$(state_trace_line_count)"
  log "browser_input_special_key=enter"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_state_trace_after "$ENTER_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=enter .*active=next-control" "page observed enter activation" 45

  SINGLE_CLICK_START_LINE="$(state_trace_line_count)"
  click_global_point "$CLICK_X" "$CLICK_Y" "browser_input_single_click"
  wait_for_state_trace_after "$SINGLE_CLICK_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=click .*detail=1 .*shift=false" "page observed single click" 45

  delay 1
  DOUBLE_CLICK_START_LINE="$(state_trace_line_count)"
  log "browser_input_double_click_point=${CLICK_X},${CLICK_Y}"
  swift "$ROOT/scripts/ghostty-app/inject.swift" move "$CLICK_X" "$CLICK_Y" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" click "$CLICK_X" "$CLICK_Y" left 2 >>"$HARNESS_LOG" 2>&1
  wait_for_state_trace_after "$DOUBLE_CLICK_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=click .*detail=2" "page observed double-click detail" 45
  wait_for_state_trace_after "$DOUBLE_CLICK_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=dblclick .*detail=2" "page observed dblclick event" 45

  delay 1
  MODIFIER_CLICK_START_LINE="$(state_trace_line_count)"
  log "browser_input_modifier_click_point=${CLICK_X},${CLICK_Y}"
  swift "$ROOT/scripts/ghostty-app/inject.swift" move "$CLICK_X" "$CLICK_Y" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" click "$CLICK_X" "$CLICK_Y" left 1 shift >>"$HARNESS_LOG" 2>&1
  wait_for_state_trace_after "$MODIFIER_CLICK_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=click .*detail=1 .*shift=true" "page observed modifier click" 45

  delay 1
  TRIPLE_CLICK_START_LINE="$(state_trace_line_count)"
  log "browser_input_triple_click_point=${CLICK_X},${CLICK_Y}"
  swift "$ROOT/scripts/ghostty-app/inject.swift" move "$CLICK_X" "$CLICK_Y" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" click "$CLICK_X" "$CLICK_Y" left 3 >>"$HARNESS_LOG" 2>&1
  wait_for_state_trace_after "$TRIPLE_CLICK_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=click .*detail=3" "page observed triple-click detail" 45

  DRAG_START_LINE="$(state_trace_line_count)"
  DRAG_APP_START_LINE="$(log_line_count)"
  log "browser_input_drag_points=${DRAG_START_X},${DRAG_START_Y}-${DRAG_END_X},${DRAG_END_Y}"
  swift "$ROOT/scripts/ghostty-app/inject.swift" drag "$DRAG_START_X" "$DRAG_START_Y" "$DRAG_END_X" "$DRAG_END_Y" >>"$HARNESS_LOG" 2>&1
  wait_for_log_after_ordered "$DRAG_APP_START_LINE" "TermSurf geometry layer=appkit event=mouse_forwarded .*pane_id:${PANE_ID} .*note=kind=event .*type=down .*button=left .*terminal_fallback=false" "browser drag down consumed by TermSurf overlay"
  DRAG_APP_DOWN_LINE="$LOG_MATCH_LINE"
  wait_for_log_after_ordered "$DRAG_APP_DOWN_LINE" "TermSurf geometry layer=appkit event=mouse_forwarded .*pane_id:${PANE_ID} .*note=kind=move .*ns_event=6 .*modifiers=.*64 .*terminal_fallback=false" "browser drag move consumed by TermSurf overlay"
  DRAG_APP_MOVE_LINE="$LOG_MATCH_LINE"
  wait_for_log_after_ordered "$DRAG_APP_MOVE_LINE" "TermSurf geometry layer=appkit event=mouse_forwarded .*pane_id:${PANE_ID} .*note=kind=event .*type=up .*button=left .*terminal_fallback=false" "browser drag up consumed by TermSurf overlay"
  wait_for_state_trace_after "$DRAG_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=selection .*text=ISSUE817_BROWSER_DRAG_TEXT" "page observed browser drag selection" 45

  BROWSER_SELECTION_SENTINEL="ISSUE817_TERMINAL_SELECTION_SENTINEL_${TS}"
  printf '%s' "$BROWSER_SELECTION_SENTINEL" | pbcopy
  BROWSER_SELECTION_CLIPBOARD_BEFORE="$(pbpaste)"
  [ "$BROWSER_SELECTION_CLIPBOARD_BEFORE" = "$BROWSER_SELECTION_SENTINEL" ] || fail "failed to set clipboard sentinel before browser selection copy"
  SELECTION_COPY_START_LINE="$(state_trace_line_count)"
  log "browser_input_selection_copy_key=cmd+c"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 8 command >>"$HARNESS_LOG" 2>&1
  wait_for_state_trace_after "$SELECTION_COPY_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=copy .*text=ISSUE817_BROWSER_DRAG_TEXT" "page observed browser selection copy" 45
  for _ in $(seq 1 20); do
    BROWSER_SELECTION_CLIPBOARD_AFTER="$(pbpaste)"
    if [ "$BROWSER_SELECTION_CLIPBOARD_AFTER" = "ISSUE817_BROWSER_DRAG_TEXT" ]; then
      break
    fi
    delay 0.25
  done
  [ "$BROWSER_SELECTION_CLIPBOARD_AFTER" = "ISSUE817_BROWSER_DRAG_TEXT" ] || fail "browser selection copy did not win clipboard: got=$BROWSER_SELECTION_CLIPBOARD_AFTER"
  log "PASS: browser drag selection copied browser text, proving terminal selection did not intercept Browse-mode Cmd+C"

  leave_browser_browse "browser_input" "$PANE_ID" "$BROWSER_TAB_ID"
fi

if [ "$SCENARIO" = "click-driven-browse-mode" ]; then
  wait_for_state_trace "event=url_changed.*url=${URL}" "click-driven browser-input fixture URL" 45
  wait_for_state_trace "event=console_message.*message=ISSUE817_INPUT .*kind=ready" "click-driven browser-input fixture ready marker" 45

  if grep -E "ModeChanged: pane_id=${PANE_ID} browsing=true" "$APP_LOG" >/dev/null 2>&1; then
    leave_browser_browse "click_driven_reset" "$PANE_ID" "$BROWSER_TAB_ID"
  fi

  read -r INPUT_X INPUT_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$APPKIT_PRESENT_LINE" 120 78)"
  read -r OUTSIDE_X OUTSIDE_Y <<<"$(global_point_below_web_frame "$WIN_LINE" "$APPKIT_PRESENT_LINE" 120)"

  CLICK_MODE_APP_START_LINE="$(log_line_count)"
  CLICK_MODE_TRACE_START_LINE="$(trace_line_count)"
  CLICK_MODE_STATE_START_LINE="$(state_trace_line_count)"
  click_global_point "$INPUT_X" "$INPUT_Y" "click_driven_browser_input"
  wait_for_state_trace_after "$CLICK_MODE_STATE_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=input-click .*active=text-input" "page observed click-driven input click" 45
  wait_for_log_after "$CLICK_MODE_APP_START_LINE" "ClickModeChanged: pane_id=${PANE_ID} browsing=true reason=overlay_click" "Ghostboard entered Browse from overlay click" 45
  wait_for_log_after "$CLICK_MODE_APP_START_LINE" "ModeChanged: pane_id=${PANE_ID} browsing=true .*source=gui" "Ghostboard sent GUI-originated Browse mode to webtui" 45
  require_trace_after "$CLICK_MODE_TRACE_START_LINE" "focus-changed tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed click-driven focus=true"
  wait_for_state_trace_after "$CLICK_MODE_STATE_START_LINE" "event=mode_changed.*source=gui.*browsing=true.*mode=browse" "webtui received GUI-originated Browse mode" 45

  INPUT_TOKEN="issue838"
  printf '%s' "$INPUT_TOKEN" >"$INPUT_TYPE_COMMAND"
  INPUT_TYPE_START_LINE="$(state_trace_line_count)"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$INPUT_TYPE_COMMAND" >>"$HARNESS_LOG" 2>&1
  wait_for_state_trace_after "$INPUT_TYPE_START_LINE" "event=console_message.*message=ISSUE817_INPUT .*kind=input .*value=${INPUT_TOKEN} .*start=8 .*end=8 .*active=text-input" "page received typed token after click-driven Browse mode" 45

  MISS_APP_START_LINE="$(log_line_count)"
  MISS_TRACE_START_LINE="$(trace_line_count)"
  MISS_STATE_START_LINE="$(state_trace_line_count)"
  click_global_point "$OUTSIDE_X" "$OUTSIDE_Y" "click_driven_outside_overlay"
  wait_for_log_after "$MISS_APP_START_LINE" "ClickModeChanged: pane_id=${PANE_ID} browsing=false reason=overlay_miss" "Ghostboard left Browse from non-overlay click" 45
  wait_for_log_after "$MISS_APP_START_LINE" "ModeChanged: pane_id=${PANE_ID} browsing=false .*source=gui" "Ghostboard sent GUI-originated Control mode to webtui" 45
  require_trace_after "$MISS_TRACE_START_LINE" "focus-changed tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed click-driven focus=false"
  wait_for_state_trace_after "$MISS_STATE_START_LINE" "event=mode_changed.*source=gui.*browsing=false.*mode=control" "webtui received GUI-originated Control mode" 45
fi

if [ "$SCENARIO" = "named-chromium-debug-launch" ]; then
  if grep -F -- "--browser" "$COMMAND" >/dev/null 2>&1; then
    fail "named Chromium debug launch command unexpectedly contains --browser"
  fi
  log "PASS: named Chromium debug launch command omits --browser"
  require_log "TermSurf message decoded type=HelloRequest" "named Chromium webtui discovered TERMSURF_SOCKET"
  require_log "TermSurf HelloReply sent homepage=https://astrohacker.com/welcome browsers=chromium" "Ghostboard sent HelloReply homepage and browser defaults"
  require_log "SetOverlay: pane_id=${PANE_ID} profile=default browser=chromium url=${URL}" "Ghostboard received named Chromium SetOverlay"
  require_log "SetOverlay: named browser resolved browser=chromium env=ASTROHACKER_CHROMIUM_PATH path=${CHROMIUM}" "Ghostboard resolved named Chromium to debug path"
  require_log "spawned browser path=${CHROMIUM} pid=[0-9]+ profile=default" "Ghostboard spawned debug Chromium path"
  require_log "BrowserReady: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} socket=.* browser=chromium" "BrowserReady preserved named Chromium key"
  if grep -E "spawned browser path=(/usr/local/chromium|/usr/local/bin/chromium|/opt/homebrew/opt/astrohacker-terminal-ah-chromiumd)" "$APP_LOG" >/dev/null 2>&1; then
    fail "named Chromium debug launch used a stale installed Chromium path"
  fi
  log "PASS: named Chromium debug launch did not use a stale installed path"
fi

if [ "$SCENARIO" = "installed-chromium-release-launch" ]; then
  if grep -F -- "--browser" "$COMMAND" >/dev/null 2>&1; then
    fail "installed Chromium release launch command unexpectedly contains --browser"
  fi
  log "PASS: installed Chromium release launch command omits --browser"
  require_log "TermSurf message decoded type=HelloRequest" "release webtui discovered TERMSURF_SOCKET"
  require_log "SetOverlay: pane_id=${PANE_ID} profile=default browser=chromium url=${URL}" "release named Chromium SetOverlay"
  if grep -F "env=ASTROHACKER_CHROMIUM_PATH path=" "$APP_LOG" >/dev/null 2>&1; then
    fail "release installed Chromium scenario unexpectedly resolved through ASTROHACKER_CHROMIUM_PATH"
  fi
  log "PASS: release installed Chromium scenario did not resolve through ASTROHACKER_CHROMIUM_PATH"
  require_log "SetOverlay: named browser resolved browser=chromium env=TERMSURF_INSTALLED_ROAMIUM_PATH path=${INSTALLED_CHROMIUM}" "release Ghostboard resolved Chromium through installed override"
  require_log "spawned browser path=${INSTALLED_CHROMIUM} pid=[0-9]+ profile=default" "release Ghostboard spawned installed override Chromium path"
  require_log "BrowserReady: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} socket=.* browser=chromium" "release BrowserReady preserved named Chromium key"
fi

if [ "$SCENARIO" = "hello-config-homepage" ]; then
  if grep -F -- "--browser" "$COMMAND" >/dev/null 2>&1; then
    fail "hello config homepage command unexpectedly contains --browser"
  fi
  log "PASS: hello config homepage command omits --browser"
  if grep -F -- "$URL" "$COMMAND" >/dev/null 2>&1; then
    fail "hello config homepage command unexpectedly contains positional URL"
  fi
  log "PASS: hello config homepage command omits positional URL"
  require_log "TermSurf Hello config homepage=${HELLO_CONFIG_HOMEPAGE}" "Ghostboard loaded configured HelloReply homepage"
  require_log "TermSurf HelloReply sent homepage=${HELLO_CONFIG_HOMEPAGE} browsers=chromium" "Ghostboard sent configured HelloReply homepage"
  require_log "SetOverlay: pane_id=${PANE_ID} profile=default browser=chromium url=${HELLO_CONFIG_HOMEPAGE}" "webtui consumed configured homepage from HelloReply"
  require_log "BrowserReady: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} socket=.* browser=chromium" "BrowserReady preserved named Chromium key with configured homepage"
fi

if [ "$SCENARIO" = "hello-config-browser-list" ]; then
  if grep -F -- "--browser" "$COMMAND" >/dev/null 2>&1; then
    fail "hello config browser-list command unexpectedly contains --browser"
  fi
  log "PASS: hello config browser-list command omits --browser"
  require_log "TermSurf Hello config homepage=https://astrohacker.com/welcome browsers=chromium,${HELLO_CONFIG_SECOND_BROWSER}" "Ghostboard loaded configured HelloReply browser list"
  require_log "TermSurf HelloReply sent homepage=https://astrohacker.com/welcome browsers=chromium,${HELLO_CONFIG_SECOND_BROWSER}" "Ghostboard sent configured HelloReply browser list"
  require_log "SetOverlay: pane_id=${PANE_ID} profile=default browser=chromium url=${URL}" "webtui consumed first configured browser from HelloReply"
  require_log "BrowserReady: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} socket=.* browser=chromium" "BrowserReady preserved first configured browser key"
fi

if [ "$SCENARIO" = "hello-empty-browser-list" ]; then
  if grep -F -- "--browser" "$COMMAND" >/dev/null 2>&1; then
    fail "hello empty browser-list command unexpectedly contains --browser"
  fi
  log "PASS: hello empty browser-list command omits --browser"
  require_log "TermSurf Hello config homepage=https://astrohacker.com/welcome browsers=chromium" "Ghostboard fell back to default HelloReply browser list"
  require_log "TermSurf HelloReply sent homepage=https://astrohacker.com/welcome browsers=chromium" "Ghostboard sent fallback HelloReply browser list"
  require_log "SetOverlay: pane_id=${PANE_ID} profile=default browser=chromium url=${URL}" "webtui consumed fallback browser from HelloReply"
  require_log "BrowserReady: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID} socket=.* browser=chromium" "BrowserReady preserved fallback browser key"
fi

if [ "$SCENARIO" = "display-move-backing-scale" ]; then
  A_WINDOW_ID="$WID"
  A_SURFACE_ID="$(extract_surface_id "$APPKIT_PRESENT_LINE")"
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_FRAME_X="$OVERLAY_FRAME_X"
  A_FRAME_Y="$OVERLAY_FRAME_Y"
  A_ROOT_FRAME_SIZE="$(extract_root_frame_size "$APPKIT_PRESENT_LINE")"
  A_PIXEL="$APPKIT_PIXEL"
  A_BACKING_SCALE="$(extract_backing_scale "$APPKIT_PRESENT_LINE")"
  log "browser_a_window_id=$A_WINDOW_ID"
  log "browser_a_surface_id=$A_SURFACE_ID"
  log "browser_a_selected_tab_id=$A_SELECTED_TAB_ID"
  log "browser_a_pane_id=$A_PANE_ID"
  log "browser_a_browser_tab_id=$A_BROWSER_TAB_ID"
  log "browser_a_context_id=$A_CONTEXT_ID"
  log "browser_a_backing_scale=$A_BACKING_SCALE"

  DISPLAY_LINES="$(swift "$DISPLAY_INVENTORY")"
  DISPLAY_COUNT="$(printf '%s\n' "$DISPLAY_LINES" | awk 'NF { count++ } END { print count + 0 }')"
  log "display_count=$DISPLAY_COUNT"
  while IFS= read -r display_line; do
    [ -n "$display_line" ] && log "display=$display_line"
  done <<EOF
$DISPLAY_LINES
EOF

  if [ "$DISPLAY_COUNT" -lt 2 ]; then
    log "PARTIAL: only one display is available; cross-display move cannot run in this VM"
    screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_DISPLAY_MOVED"
    log "single_display_screenshot_exit=$?"

    SINGLE_HIT_START_LINE="$(log_line_count)"
    click_window_center "$WIN_LINE" "single_display_browser_area"
    SINGLE_HIT_LINE="$(wait_for_hit_after "$SINGLE_HIT_START_LINE" "$A_CONTEXT_ID" "single-display browser hit-test")"
    require_text "$SINGLE_HIT_LINE" "window_id:${A_WINDOW_ID}" "single-display hit-test has window id"
    require_text "$SINGLE_HIT_LINE" "surface_id:${A_SURFACE_ID}" "single-display hit-test has surface id"
    require_text "$SINGLE_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "single-display hit-test has selected tab id"
    require_text "$SINGLE_HIT_LINE" "overlay_frame=${A_FRAME}" "single-display hit-test uses AppKit frame"
    require_text "$SINGLE_HIT_LINE" "web_point={" "single-display hit-test includes webview-relative point"

    SINGLE_MODE_START_LINE="$(log_line_count)"
    SINGLE_MODE_TRACE_START_LINE="$(trace_line_count)"
    SINGLE_MODE_PRE_TRACE_FOCUS_TRUE_COUNT="$(awk -v tab="$A_BROWSER_TAB_ID" -v pane="$A_PANE_ID" 'index($0, "focus-changed tab=" tab " pane=" pane " ffi=ts_set_focus focused=true") { count++ } END { print count + 0 }' "$CHROMIUM_TRACE")"
    SINGLE_MODE_PRE_TRACE_FOCUS_FALSE_COUNT="$(awk -v tab="$A_BROWSER_TAB_ID" -v pane="$A_PANE_ID" 'index($0, "focus-changed tab=" tab " pane=" pane " ffi=ts_set_focus focused=false") { count++ } END { print count + 0 }' "$CHROMIUM_TRACE")"
    SINGLE_MODE_PRE_TRACE_LAST_FOCUS="$(rg -n "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} .*focused=" "$CHROMIUM_TRACE" | tail -1 || true)"
    log "single_display_trace_start_line=$SINGLE_MODE_TRACE_START_LINE"
    log "single_display_pre_enter_focus_true_count=$SINGLE_MODE_PRE_TRACE_FOCUS_TRUE_COUNT"
    log "single_display_pre_enter_focus_false_count=$SINGLE_MODE_PRE_TRACE_FOCUS_FALSE_COUNT"
    if [ -n "$SINGLE_MODE_PRE_TRACE_LAST_FOCUS" ]; then
      log "single_display_pre_enter_last_focus=$SINGLE_MODE_PRE_TRACE_LAST_FOCUS"
    else
      log "single_display_pre_enter_last_focus=<none>"
    fi
    log "single_display_mode_key=enter=Mode::Browse"
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
    SINGLE_MODE_CHANGED_APP_LINE="$(wait_for_line_after "$SINGLE_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "single-display webtui entered browse mode")"
    log "PASS: single-display webtui entered browse mode"
    SINGLE_MODE_FOCUS_APP_LINE="$(wait_for_line_after "$SINGLE_MODE_START_LINE" "FocusChanged: pane_id=${A_PANE_ID} tab_id=${A_BROWSER_TAB_ID} focused=true" "single-display Ghostboard emitted focus=true after browse mode")"
    log "PASS: single-display Ghostboard emitted focus=true after browse mode"
    SINGLE_MODE_KEY_APP_LINE="$(rg -n "key_down scenario=${SCENARIO} .*pane_id:${A_PANE_ID} .*note=key_code=36 " "$APP_LOG" | tail -1 || true)"
    SINGLE_MODE_POST_TRACE_FOCUS_TRUE_COUNT="$(awk -v tab="$A_BROWSER_TAB_ID" -v pane="$A_PANE_ID" 'index($0, "focus-changed tab=" tab " pane=" pane " ffi=ts_set_focus focused=true") { count++ } END { print count + 0 }' "$CHROMIUM_TRACE")"
    SINGLE_MODE_POST_TRACE_FOCUS_FALSE_COUNT="$(awk -v tab="$A_BROWSER_TAB_ID" -v pane="$A_PANE_ID" 'index($0, "focus-changed tab=" tab " pane=" pane " ffi=ts_set_focus focused=false") { count++ } END { print count + 0 }' "$CHROMIUM_TRACE")"
    SINGLE_MODE_POST_TRACE_LAST_FOCUS="$(rg -n "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} .*focused=" "$CHROMIUM_TRACE" | tail -1 || true)"
    SINGLE_MODE_TRACE_FOCUS_TRUE_DELTA="$((SINGLE_MODE_POST_TRACE_FOCUS_TRUE_COUNT - SINGLE_MODE_PRE_TRACE_FOCUS_TRUE_COUNT))"
    SINGLE_MODE_TRACE_FOCUS_FALSE_DELTA="$((SINGLE_MODE_POST_TRACE_FOCUS_FALSE_COUNT - SINGLE_MODE_PRE_TRACE_FOCUS_FALSE_COUNT))"
    log "single_display_mode_app_log_line=$SINGLE_MODE_CHANGED_APP_LINE"
    log "single_display_focus_app_log_line=$SINGLE_MODE_FOCUS_APP_LINE"
    if [ -n "$SINGLE_MODE_KEY_APP_LINE" ]; then
      log "single_display_enter_key_appkit_log_line=$SINGLE_MODE_KEY_APP_LINE"
    else
      log "single_display_enter_key_appkit_log_line=<none>"
    fi
    log "single_display_post_enter_focus_true_count=$SINGLE_MODE_POST_TRACE_FOCUS_TRUE_COUNT"
    log "single_display_post_enter_focus_false_count=$SINGLE_MODE_POST_TRACE_FOCUS_FALSE_COUNT"
    log "single_display_focus_true_delta=$SINGLE_MODE_TRACE_FOCUS_TRUE_DELTA"
    log "single_display_focus_false_delta=$SINGLE_MODE_TRACE_FOCUS_FALSE_DELTA"
    if [ -n "$SINGLE_MODE_POST_TRACE_LAST_FOCUS" ]; then
      log "single_display_post_enter_last_focus=$SINGLE_MODE_POST_TRACE_LAST_FOCUS"
    else
      log "single_display_post_enter_last_focus=<none>"
    fi
    require_trace_after "$SINGLE_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed focus=true on single display"

    SINGLE_KEY_START_LINE="$(trace_line_count)"
    printf 'ISSUE809_EXP17_SINGLE_DISPLAY\n' >"$BROWSER_FOCUS_COMMAND"
    swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
    require_trace_after "$SINGLE_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "single-display keyboard marker reached browser"
    log "PASS: scenario display-move-backing-scale partial-single-display"
    exit 0
  fi

  ORIGINAL_WIN_LINE="$WIN_LINE"
  IFS=$'\t' read -r _ORIG_WID ORIG_WX ORIG_WY ORIG_WW ORIG_WH <<<"$ORIGINAL_WIN_LINE"
  ORIG_CENTER_X="$(awk -v x="$ORIG_WX" -v w="$ORIG_WW" 'BEGIN { print int(x + (w / 2)) }')"
  ORIG_CENTER_Y="$(awk -v y="$ORIG_WY" -v h="$ORIG_WH" 'BEGIN { print int(y + (h / 2)) }')"
  SOURCE_DISPLAY_LINE="$(printf '%s\n' "$DISPLAY_LINES" | awk -F '\t' -v x="$ORIG_CENTER_X" -v y="$ORIG_CENTER_Y" '$2 <= x && x < ($2 + $4) && $3 <= y && y < ($3 + $5) { print; exit }')"
  [ -n "$SOURCE_DISPLAY_LINE" ] || fail "could not identify source display for window center"
  SOURCE_DISPLAY_ID="$(printf '%s\n' "$SOURCE_DISPLAY_LINE" | awk -F '\t' '{print $1}')"
  SOURCE_SCALE="$(printf '%s\n' "$SOURCE_DISPLAY_LINE" | awk -F '\t' '{print $10}')"
  DEST_DISPLAY_LINE="$(printf '%s\n' "$DISPLAY_LINES" | awk -F '\t' -v source="$SOURCE_DISPLAY_ID" '$1 != source { print; exit }')"
  [ -n "$DEST_DISPLAY_LINE" ] || fail "could not identify destination display"
  IFS=$'\t' read -r DEST_DISPLAY_ID DEST_X DEST_Y DEST_W DEST_H DEST_VX DEST_VY DEST_VW DEST_VH DEST_SCALE _DEST_KIND <<<"$DEST_DISPLAY_LINE"
  log "source_display=$SOURCE_DISPLAY_LINE"
  log "destination_display=$DEST_DISPLAY_LINE"

  MOVE_START_LINE="$(log_line_count)"
  MOVE_TRACE_START_LINE="$(trace_line_count)"
  DEST_WINDOW_X="$(awk -v x="$DEST_VX" 'BEGIN { print int(x + 40) }')"
  DEST_WINDOW_Y="$(awk -v y="$DEST_VY" 'BEGIN { print int(y + 40) }')"
  swift "$RESIZE_WINDOW" "$PID" "$DEST_WINDOW_X" "$DEST_WINDOW_Y" "$ORIG_WW" "$ORIG_WH" >>"$HARNESS_LOG" 2>&1
  delay 2
  MOVED_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve moved window bounds"
  log "moved_window=$MOVED_WIN_LINE"
  IFS=$'\t' read -r _MOVED_WID MOVED_WX MOVED_WY MOVED_WW MOVED_WH <<<"$MOVED_WIN_LINE"
  MOVED_CENTER_X="$(awk -v x="$MOVED_WX" -v w="$MOVED_WW" 'BEGIN { print int(x + (w / 2)) }')"
  MOVED_CENTER_Y="$(awk -v y="$MOVED_WY" -v h="$MOVED_WH" 'BEGIN { print int(y + (h / 2)) }')"
  awk -F '\t' -v x="$MOVED_CENTER_X" -v y="$MOVED_CENTER_Y" '{
    exit !(($2 <= x) && (x < ($2 + $4)) && ($3 <= y) && (y < ($3 + $5)))
  }' <<EOF || fail "moved window center is not inside destination display"
$DEST_DISPLAY_LINE
EOF
  log "PASS: moved window center is inside destination display"
  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_DISPLAY_MOVED"
  log "display_moved_screenshot_exit=$?"
  MOVED_PRESENT_LINE="$(wait_for_line_after "$MOVE_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${A_PANE_ID} .*context_id=${A_CONTEXT_ID}" "display-moved AppKit presentation")"
  MOVED_PIXELS_LINE="$(wait_for_line_after "$MOVE_START_LINE" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${A_PANE_ID} .*context_id=${A_CONTEXT_ID}" "display-moved AppKit pixels")"
  MOVED_FRAME="$(extract_overlay_frame "$MOVED_PRESENT_LINE")"
  MOVED_FRAME_SIZE="$(extract_frame_size "$MOVED_PRESENT_LINE")"
  MOVED_SCALE="$(extract_backing_scale "$MOVED_PRESENT_LINE")"
  MOVED_PIXEL="$(extract_appkit_pixel "$MOVED_PIXELS_LINE")"
  [ "$MOVED_FRAME" = "$A_FRAME" ] || fail "browser AppKit frame changed after display move: expected=$A_FRAME actual=$MOVED_FRAME"
  [ "$MOVED_SCALE" = "$DEST_SCALE" ] || fail "display-moved backing scale mismatch: expected=$DEST_SCALE actual=$MOVED_SCALE"
  MOVED_EXPECTED_PIXEL="$(awk -v size="$MOVED_FRAME_SIZE" -v scale="$MOVED_SCALE" 'BEGIN { split(size, parts, "x"); printf "%dx%d", int((parts[1] * scale) + 0.5), int((parts[2] * scale) + 0.5) }')"
  [ "$MOVED_PIXEL" = "$MOVED_EXPECTED_PIXEL" ] || fail "display-moved AppKit pixel mismatch: expected=$MOVED_EXPECTED_PIXEL actual=$MOVED_PIXEL"
  log "PASS: display-moved AppKit frame, pixels, and backing scale match destination display"
  if [ "$MOVED_PIXEL" != "$A_PIXEL" ]; then
    MOVED_PIXEL_WIDTH="${MOVED_PIXEL%x*}"
    MOVED_PIXEL_HEIGHT="${MOVED_PIXEL#*x}"
    require_trace_after "$MOVE_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${MOVED_PIXEL_WIDTH} pixel_height=${MOVED_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied display-moved resize to AppKit pixel size"
  else
    log "PASS: display-moved AppKit pixels unchanged; no Chromium resize required"
  fi

  MOVED_ROOT_HEIGHT="$(pair_height "$A_ROOT_FRAME_SIZE")"
  MOVED_CONTENT_Y_OFFSET="$(awk -v wh="$MOVED_WH" -v root_h="$MOVED_ROOT_HEIGHT" 'BEGIN { print int(wh - root_h) }')"
  MOVED_X="$(awk -v wx="$MOVED_WX" -v frame_x="$A_FRAME_X" 'BEGIN { print int(wx + frame_x + 4) }')"
  MOVED_Y="$(awk -v wy="$MOVED_WY" -v content_y="$MOVED_CONTENT_Y_OFFSET" -v frame_y="$A_FRAME_Y" 'BEGIN { print int(wy + content_y + frame_y + 4) }')"
  MOVED_HIT_START_LINE="$(log_line_count)"
  click_global_point "$MOVED_X" "$MOVED_Y" "display_moved_browser_area"
  MOVED_HIT_LINE="$(wait_for_hit_after "$MOVED_HIT_START_LINE" "$A_CONTEXT_ID" "display-moved browser hit-test")"
  require_text "$MOVED_HIT_LINE" "window_id:${A_WINDOW_ID}" "display-moved hit-test has window id"
  require_text "$MOVED_HIT_LINE" "surface_id:${A_SURFACE_ID}" "display-moved hit-test has surface id"
  require_text "$MOVED_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "display-moved hit-test has selected tab id"
  require_text "$MOVED_HIT_LINE" "overlay_frame=${A_FRAME}" "display-moved hit-test uses AppKit frame"
  require_text "$MOVED_HIT_LINE" "web_point={" "display-moved hit-test includes webview-relative point"

  MOVED_MODE_START_LINE="$(log_line_count)"
  MOVED_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "display_moved_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$MOVED_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "display-moved webtui entered browse mode"
  require_trace_after "$MOVED_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed focus=true after display move"
  MOVED_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP17_MOVED_DISPLAY\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$MOVED_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "display-moved keyboard marker reached browser"

  RETURN_CONTROL_START_LINE="$(log_line_count)"
  RETURN_CONTROL_TRACE_START_LINE="$(trace_line_count)"
  log "display_moved_control_key=escape=Mode::Control"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$RETURN_CONTROL_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=false" "display-moved webtui returned to control mode"
  require_trace_after "$RETURN_CONTROL_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed focus=false before return move"

  RETURN_START_LINE="$(log_line_count)"
  RETURN_TRACE_START_LINE="$(trace_line_count)"
  swift "$RESIZE_WINDOW" "$PID" "$ORIG_WX" "$ORIG_WY" "$ORIG_WW" "$ORIG_WH" >>"$HARNESS_LOG" 2>&1
  delay 2
  RETURNED_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve returned window bounds"
  log "returned_window=$RETURNED_WIN_LINE"
  IFS=$'\t' read -r _RETURNED_WID RETURNED_WX RETURNED_WY RETURNED_WW RETURNED_WH <<<"$RETURNED_WIN_LINE"
  RETURNED_CENTER_X="$(awk -v x="$RETURNED_WX" -v w="$RETURNED_WW" 'BEGIN { print int(x + (w / 2)) }')"
  RETURNED_CENTER_Y="$(awk -v y="$RETURNED_WY" -v h="$RETURNED_WH" 'BEGIN { print int(y + (h / 2)) }')"
  awk -F '\t' -v x="$RETURNED_CENTER_X" -v y="$RETURNED_CENTER_Y" '{
    exit !(($2 <= x) && (x < ($2 + $4)) && ($3 <= y) && (y < ($3 + $5)))
  }' <<EOF || fail "returned window center is not inside source display"
$SOURCE_DISPLAY_LINE
EOF
  log "PASS: returned window center is inside source display"
  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_DISPLAY_RETURNED"
  log "display_returned_screenshot_exit=$?"
  RETURNED_PRESENT_LINE="$(wait_for_line_after "$RETURN_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${A_PANE_ID} .*context_id=${A_CONTEXT_ID}" "display-returned AppKit presentation")"
  RETURNED_PIXELS_LINE="$(wait_for_line_after "$RETURN_START_LINE" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${A_PANE_ID} .*context_id=${A_CONTEXT_ID}" "display-returned AppKit pixels")"
  RETURNED_FRAME="$(extract_overlay_frame "$RETURNED_PRESENT_LINE")"
  RETURNED_FRAME_SIZE="$(extract_frame_size "$RETURNED_PRESENT_LINE")"
  RETURNED_SCALE="$(extract_backing_scale "$RETURNED_PRESENT_LINE")"
  RETURNED_PIXEL="$(extract_appkit_pixel "$RETURNED_PIXELS_LINE")"
  [ "$RETURNED_FRAME" = "$A_FRAME" ] || fail "browser AppKit frame changed after display return: expected=$A_FRAME actual=$RETURNED_FRAME"
  [ "$RETURNED_SCALE" = "$SOURCE_SCALE" ] || fail "display-returned backing scale mismatch: expected=$SOURCE_SCALE actual=$RETURNED_SCALE"
  RETURNED_EXPECTED_PIXEL="$(awk -v size="$RETURNED_FRAME_SIZE" -v scale="$RETURNED_SCALE" 'BEGIN { split(size, parts, "x"); printf "%dx%d", int((parts[1] * scale) + 0.5), int((parts[2] * scale) + 0.5) }')"
  [ "$RETURNED_PIXEL" = "$RETURNED_EXPECTED_PIXEL" ] || fail "display-returned AppKit pixel mismatch: expected=$RETURNED_EXPECTED_PIXEL actual=$RETURNED_PIXEL"
  log "PASS: display-returned AppKit frame, pixels, and backing scale match source display"
  if [ "$RETURNED_PIXEL" != "$MOVED_PIXEL" ]; then
    RETURNED_PIXEL_WIDTH="${RETURNED_PIXEL%x*}"
    RETURNED_PIXEL_HEIGHT="${RETURNED_PIXEL#*x}"
    require_trace_after "$RETURN_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${RETURNED_PIXEL_WIDTH} pixel_height=${RETURNED_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied display-returned resize to AppKit pixel size"
  else
    log "PASS: display-returned AppKit pixels unchanged; no Chromium resize required"
  fi

  RETURN_HIT_START_LINE="$(log_line_count)"
  click_window_center "$RETURNED_WIN_LINE" "display_returned_browser_area"
  RETURN_HIT_LINE="$(wait_for_hit_after "$RETURN_HIT_START_LINE" "$A_CONTEXT_ID" "display-returned browser hit-test")"
  require_text "$RETURN_HIT_LINE" "window_id:${A_WINDOW_ID}" "display-returned hit-test has window id"
  require_text "$RETURN_HIT_LINE" "surface_id:${A_SURFACE_ID}" "display-returned hit-test has surface id"
  require_text "$RETURN_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "display-returned hit-test has selected tab id"
  require_text "$RETURN_HIT_LINE" "overlay_frame=${A_FRAME}" "display-returned hit-test uses AppKit frame"
  require_text "$RETURN_HIT_LINE" "web_point={" "display-returned hit-test includes webview-relative point"

  [ "$MOVE_TRACE_START_LINE" -lt "$MOVED_MODE_TRACE_START_LINE" ] || fail "trace boundaries for display move were not monotonic"
  [ "$MOVED_MODE_TRACE_START_LINE" -lt "$RETURN_TRACE_START_LINE" ] || fail "trace boundaries for display return were not monotonic"
fi

if [ "$SCENARIO" = "fullscreen-unfullscreen" ]; then
  A_WINDOW_ID="$WID"
  A_SURFACE_ID="$(extract_surface_id "$APPKIT_PRESENT_LINE")"
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_FRAME_X="$OVERLAY_FRAME_X"
  A_FRAME_Y="$OVERLAY_FRAME_Y"
  A_PIXEL="$APPKIT_PIXEL"
  A_BACKING_SCALE="$(extract_backing_scale "$APPKIT_PRESENT_LINE")"
  log "browser_a_window_id=$A_WINDOW_ID"
  log "browser_a_surface_id=$A_SURFACE_ID"
  log "browser_a_selected_tab_id=$A_SELECTED_TAB_ID"
  log "browser_a_pane_id=$A_PANE_ID"
  log "browser_a_browser_tab_id=$A_BROWSER_TAB_ID"
  log "browser_a_context_id=$A_CONTEXT_ID"
  log "browser_a_backing_scale=$A_BACKING_SCALE"

  FULLSCREEN_START_LINE="$(log_line_count)"
  FULLSCREEN_TRACE_START_LINE="$(trace_line_count)"
  FULLSCREEN_STATE="$(swift "$FULLSCREEN_WINDOW" "$PID" "$WX" "$WY" "$WW" "$WH" enter >>"$HARNESS_LOG" 2>&1; tail -1 "$HARNESS_LOG")"
  [ "$FULLSCREEN_STATE" = "true" ] || fail "AXFullScreen did not become true after entering fullscreen: $FULLSCREEN_STATE"
  log "PASS: AXFullScreen true after enter"
  delay 4

  FULLSCREEN_WIN_LINE="$(app_windows | awk -F '\t' -v old="$A_WINDOW_ID" '$1 == old { print; found=1 } END { if (!found) exit 1 }' || true)"
  if [ -z "$FULLSCREEN_WIN_LINE" ]; then
    FULLSCREEN_WIN_LINE="$(app_windows | head -1 || true)"
  fi
  [ -n "$FULLSCREEN_WIN_LINE" ] || fail "could not resolve fullscreen Ghostboard window"
  IFS=$'\t' read -r FULLSCREEN_WINDOW_ID FULLSCREEN_WX FULLSCREEN_WY FULLSCREEN_WW FULLSCREEN_WH <<<"$FULLSCREEN_WIN_LINE"
  log "fullscreen_window=$FULLSCREEN_WIN_LINE"

  FULLSCREEN_PRESENT_LINE="$(wait_for_line_after "$FULLSCREEN_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${A_PANE_ID} .*context_id=${A_CONTEXT_ID}" "fullscreen AppKit presentation" 45)"
  FULLSCREEN_PIXELS_LINE="$(wait_for_line_after "$FULLSCREEN_START_LINE" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${A_PANE_ID} .*context_id=${A_CONTEXT_ID}" "fullscreen AppKit pixels" 45)"
  FULLSCREEN_PRESENT_WINDOW_ID="$(printf '%s\n' "$FULLSCREEN_PRESENT_LINE" | sed -E 's/.*window_id:([^ ]+) .*/\1/')"
  FULLSCREEN_SURFACE_ID="$(extract_surface_id "$FULLSCREEN_PRESENT_LINE")"
  FULLSCREEN_SELECTED_TAB_ID="$(extract_selected_tab_id "$FULLSCREEN_PRESENT_LINE")"
  FULLSCREEN_FRAME="$(extract_overlay_frame "$FULLSCREEN_PRESENT_LINE")"
  FULLSCREEN_FRAME_SIZE="$(extract_frame_size "$FULLSCREEN_PRESENT_LINE")"
  FULLSCREEN_FRAME_X="$(extract_frame_x "$FULLSCREEN_PRESENT_LINE")"
  FULLSCREEN_FRAME_Y="$(extract_frame_y "$FULLSCREEN_PRESENT_LINE")"
  FULLSCREEN_SCALE="$(extract_backing_scale "$FULLSCREEN_PRESENT_LINE")"
  FULLSCREEN_PIXEL="$(extract_appkit_pixel "$FULLSCREEN_PIXELS_LINE")"
  [ "$FULLSCREEN_PRESENT_WINDOW_ID" = "$FULLSCREEN_WINDOW_ID" ] || fail "fullscreen AppKit window id mismatch: current=$FULLSCREEN_WINDOW_ID presented=$FULLSCREEN_PRESENT_WINDOW_ID"
  [ "$FULLSCREEN_SURFACE_ID" = "$A_SURFACE_ID" ] || fail "fullscreen surface id changed"
  [ "$FULLSCREEN_SELECTED_TAB_ID" = "$A_SELECTED_TAB_ID" ] || fail "fullscreen selected tab id changed"
  [ "$FULLSCREEN_PIXEL" != "$A_PIXEL" ] || fail "fullscreen AppKit pixels did not change from baseline"
  compare_pair "$FULLSCREEN_FRAME_SIZE" "$A_FRAME_SIZE" gt || fail "fullscreen frame did not grow from baseline: baseline=$A_FRAME_SIZE fullscreen=$FULLSCREEN_FRAME_SIZE"
  log "fullscreen_window_id=$FULLSCREEN_PRESENT_WINDOW_ID"
  log "fullscreen_overlay_frame=$FULLSCREEN_FRAME"
  log "fullscreen_overlay_frame_size=$FULLSCREEN_FRAME_SIZE"
  log "fullscreen_appkit_pixel=$FULLSCREEN_PIXEL"
  log "fullscreen_backing_scale=$FULLSCREEN_SCALE"

  FULLSCREEN_PIXEL_WIDTH="${FULLSCREEN_PIXEL%x*}"
  FULLSCREEN_PIXEL_HEIGHT="${FULLSCREEN_PIXEL#*x}"
  require_trace_after "$FULLSCREEN_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${FULLSCREEN_PIXEL_WIDTH} pixel_height=${FULLSCREEN_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied fullscreen resize to AppKit pixel size"

  screencapture -x -o -l"$FULLSCREEN_WINDOW_ID" "$SCREENSHOT_FULLSCREEN"
  log "fullscreen_screenshot_exit=$?"

  FULLSCREEN_CLICK_X="$(awk -v wx="$FULLSCREEN_WX" -v frame_x="$FULLSCREEN_FRAME_X" -v frame_size="$FULLSCREEN_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
  FULLSCREEN_CLICK_Y="$(awk -v wy="$FULLSCREEN_WY" -v frame_y="$FULLSCREEN_FRAME_Y" -v frame_size="$FULLSCREEN_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"
  FULLSCREEN_HIT_START_LINE="$(log_line_count)"
  click_global_point "$FULLSCREEN_CLICK_X" "$FULLSCREEN_CLICK_Y" "fullscreen_browser_area"
  FULLSCREEN_HIT_LINE="$(wait_for_hit_after "$FULLSCREEN_HIT_START_LINE" "$A_CONTEXT_ID" "fullscreen browser hit-test")"
  require_text "$FULLSCREEN_HIT_LINE" "window_id:${FULLSCREEN_WINDOW_ID}" "fullscreen hit-test has current window id"
  require_text "$FULLSCREEN_HIT_LINE" "surface_id:${A_SURFACE_ID}" "fullscreen hit-test has surface id"
  require_text "$FULLSCREEN_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "fullscreen hit-test has selected tab id"
  require_text "$FULLSCREEN_HIT_LINE" "overlay_frame=${FULLSCREEN_FRAME}" "fullscreen hit-test uses fullscreen frame"
  require_text "$FULLSCREEN_HIT_LINE" "web_point={" "fullscreen hit-test includes webview-relative point"

  FULLSCREEN_MODE_START_LINE="$(log_line_count)"
  FULLSCREEN_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "fullscreen_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$FULLSCREEN_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "fullscreen webtui entered browse mode"
  require_trace_after "$FULLSCREEN_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed focus=true after fullscreen"

  FULLSCREEN_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP18_FULLSCREEN\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$FULLSCREEN_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "fullscreen keyboard marker reached browser"

  FULLSCREEN_CONTROL_START_LINE="$(log_line_count)"
  FULLSCREEN_CONTROL_TRACE_START_LINE="$(trace_line_count)"
  log "fullscreen_control_key=escape=Mode::Control"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$FULLSCREEN_CONTROL_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=false" "fullscreen webtui returned to control mode"
  require_trace_after "$FULLSCREEN_CONTROL_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed focus=false before unfullscreen"

  UNFULLSCREEN_START_LINE="$(log_line_count)"
  UNFULLSCREEN_TRACE_START_LINE="$(trace_line_count)"
  UNFULLSCREEN_STATE="$(swift "$FULLSCREEN_WINDOW" "$PID" "$FULLSCREEN_WX" "$FULLSCREEN_WY" "$FULLSCREEN_WW" "$FULLSCREEN_WH" exit >>"$HARNESS_LOG" 2>&1; tail -1 "$HARNESS_LOG")"
  [ "$UNFULLSCREEN_STATE" = "false" ] || fail "AXFullScreen did not become false after exiting fullscreen: $UNFULLSCREEN_STATE"
  log "PASS: AXFullScreen false after exit"
  delay 4

  UNFULLSCREEN_WIN_LINE="$(app_windows | awk -F '\t' -v old="$A_WINDOW_ID" '$1 == old { print; found=1 } END { if (!found) exit 1 }' || true)"
  if [ -z "$UNFULLSCREEN_WIN_LINE" ]; then
    UNFULLSCREEN_WIN_LINE="$(app_windows | head -1 || true)"
  fi
  [ -n "$UNFULLSCREEN_WIN_LINE" ] || fail "could not resolve unfullscreen Ghostboard window"
  IFS=$'\t' read -r UNFULLSCREEN_WINDOW_ID UNFULLSCREEN_WX UNFULLSCREEN_WY UNFULLSCREEN_WW UNFULLSCREEN_WH <<<"$UNFULLSCREEN_WIN_LINE"
  log "unfullscreen_window=$UNFULLSCREEN_WIN_LINE"

  UNFULLSCREEN_PRESENT_LINE="$(wait_for_line_after "$UNFULLSCREEN_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${A_PANE_ID} .*context_id=${A_CONTEXT_ID}" "unfullscreen AppKit presentation" 45)"
  UNFULLSCREEN_PIXELS_LINE="$(wait_for_line_after "$UNFULLSCREEN_START_LINE" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${A_PANE_ID} .*context_id=${A_CONTEXT_ID}" "unfullscreen AppKit pixels" 45)"
  UNFULLSCREEN_PRESENT_WINDOW_ID="$(printf '%s\n' "$UNFULLSCREEN_PRESENT_LINE" | sed -E 's/.*window_id:([^ ]+) .*/\1/')"
  UNFULLSCREEN_SURFACE_ID="$(extract_surface_id "$UNFULLSCREEN_PRESENT_LINE")"
  UNFULLSCREEN_SELECTED_TAB_ID="$(extract_selected_tab_id "$UNFULLSCREEN_PRESENT_LINE")"
  UNFULLSCREEN_FRAME="$(extract_overlay_frame "$UNFULLSCREEN_PRESENT_LINE")"
  UNFULLSCREEN_FRAME_SIZE="$(extract_frame_size "$UNFULLSCREEN_PRESENT_LINE")"
  UNFULLSCREEN_SCALE="$(extract_backing_scale "$UNFULLSCREEN_PRESENT_LINE")"
  UNFULLSCREEN_PIXEL="$(extract_appkit_pixel "$UNFULLSCREEN_PIXELS_LINE")"
  [ "$UNFULLSCREEN_PRESENT_WINDOW_ID" = "$UNFULLSCREEN_WINDOW_ID" ] || fail "unfullscreen AppKit window id mismatch: current=$UNFULLSCREEN_WINDOW_ID presented=$UNFULLSCREEN_PRESENT_WINDOW_ID"
  [ "$UNFULLSCREEN_SURFACE_ID" = "$A_SURFACE_ID" ] || fail "unfullscreen surface id changed"
  [ "$UNFULLSCREEN_SELECTED_TAB_ID" = "$A_SELECTED_TAB_ID" ] || fail "unfullscreen selected tab id changed"
  [ "$UNFULLSCREEN_PIXEL" != "$FULLSCREEN_PIXEL" ] || fail "unfullscreen AppKit pixels did not change from fullscreen"
  log "unfullscreen_window_id=$UNFULLSCREEN_PRESENT_WINDOW_ID"
  log "unfullscreen_overlay_frame=$UNFULLSCREEN_FRAME"
  log "unfullscreen_overlay_frame_size=$UNFULLSCREEN_FRAME_SIZE"
  log "unfullscreen_appkit_pixel=$UNFULLSCREEN_PIXEL"
  log "unfullscreen_backing_scale=$UNFULLSCREEN_SCALE"

  UNFULLSCREEN_PIXEL_WIDTH="${UNFULLSCREEN_PIXEL%x*}"
  UNFULLSCREEN_PIXEL_HEIGHT="${UNFULLSCREEN_PIXEL#*x}"
  require_trace_after "$UNFULLSCREEN_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${UNFULLSCREEN_PIXEL_WIDTH} pixel_height=${UNFULLSCREEN_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied unfullscreen resize to AppKit pixel size"

  screencapture -x -o -l"$UNFULLSCREEN_WINDOW_ID" "$SCREENSHOT_UNFULLSCREEN"
  log "unfullscreen_screenshot_exit=$?"

  UNFULLSCREEN_HIT_START_LINE="$(log_line_count)"
  click_window_center "$UNFULLSCREEN_WIN_LINE" "unfullscreen_browser_area"
  UNFULLSCREEN_HIT_LINE="$(wait_for_hit_after "$UNFULLSCREEN_HIT_START_LINE" "$A_CONTEXT_ID" "unfullscreen browser hit-test")"
  require_text "$UNFULLSCREEN_HIT_LINE" "window_id:${UNFULLSCREEN_WINDOW_ID}" "unfullscreen hit-test has current window id"
  require_text "$UNFULLSCREEN_HIT_LINE" "surface_id:${A_SURFACE_ID}" "unfullscreen hit-test has surface id"
  require_text "$UNFULLSCREEN_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "unfullscreen hit-test has selected tab id"
  require_text "$UNFULLSCREEN_HIT_LINE" "overlay_frame=${UNFULLSCREEN_FRAME}" "unfullscreen hit-test uses unfullscreen frame"
  require_text "$UNFULLSCREEN_HIT_LINE" "web_point={" "unfullscreen hit-test includes webview-relative point"

  UNFULLSCREEN_MODE_START_LINE="$(log_line_count)"
  UNFULLSCREEN_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "unfullscreen_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$UNFULLSCREEN_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "unfullscreen webtui entered browse mode"
  require_trace_after "$UNFULLSCREEN_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed focus=true after unfullscreen"

  UNFULLSCREEN_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP18_UNFULLSCREEN\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$UNFULLSCREEN_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "unfullscreen keyboard marker reached browser"

  [ "$FULLSCREEN_TRACE_START_LINE" -lt "$FULLSCREEN_MODE_TRACE_START_LINE" ] || fail "trace boundaries for fullscreen were not monotonic"
  [ "$FULLSCREEN_MODE_TRACE_START_LINE" -lt "$UNFULLSCREEN_TRACE_START_LINE" ] || fail "trace boundaries for unfullscreen were not monotonic"
fi

if [ "$SCENARIO" = "minimize-hide-restore" ]; then
  A_WINDOW_ID="$WID"
  A_SURFACE_ID="$(extract_surface_id "$APPKIT_PRESENT_LINE")"
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_FRAME_X="$OVERLAY_FRAME_X"
  A_FRAME_Y="$OVERLAY_FRAME_Y"
  A_ROOT_FRAME_SIZE="$(extract_root_frame_size "$APPKIT_PRESENT_LINE")"
  A_PIXEL="$APPKIT_PIXEL"
  A_BACKING_SCALE="$(extract_backing_scale "$APPKIT_PRESENT_LINE")"
  log "browser_a_window_id=$A_WINDOW_ID"
  log "browser_a_surface_id=$A_SURFACE_ID"
  log "browser_a_selected_tab_id=$A_SELECTED_TAB_ID"
  log "browser_a_pane_id=$A_PANE_ID"
  log "browser_a_browser_tab_id=$A_BROWSER_TAB_ID"
  log "browser_a_context_id=$A_CONTEXT_ID"
  log "browser_a_backing_scale=$A_BACKING_SCALE"

  FORMER_CLICK_X="$(awk -v wx="$WX" -v frame_x="$A_FRAME_X" -v frame_size="$A_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
  FORMER_CLICK_Y="$(awk -v wy="$WY" -v frame_y="$A_FRAME_Y" -v frame_size="$A_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"

  MINIMIZE_START_LINE="$(log_line_count)"
  MINIMIZE_STATE="$(swift "$MINIMIZE_WINDOW" "$PID" "$WX" "$WY" "$WW" "$WH" minimize >>"$HARNESS_LOG" 2>&1; tail -1 "$HARNESS_LOG")"
  [ "$MINIMIZE_STATE" = "true" ] || fail "AXMinimized did not become true after minimize: $MINIMIZE_STATE"
  log "PASS: AXMinimized true after minimize"
  delay 2
  if app_windows | awk -F '\t' -v wid="$A_WINDOW_ID" '$1 == wid { found=1 } END { exit found ? 0 : 1 }'; then
    fail "minimized window remained visible in onscreen CG window list"
  fi
  log "PASS: minimized window absent from onscreen CG window list"

  MINIMIZED_HIT_START_LINE="$(log_line_count)"
  click_global_point "$FORMER_CLICK_X" "$FORMER_CLICK_Y" "minimized_former_browser_area"
  wait_for_negative_hit_after "$MINIMIZED_HIT_START_LINE" "$A_CONTEXT_ID" "minimized former browser area hit-test" allow-absent 3

  RESTORE_START_LINE="$(log_line_count)"
  RESTORE_STATE="$(swift "$MINIMIZE_WINDOW" "$PID" "$WX" "$WY" "$WW" "$WH" restore >>"$HARNESS_LOG" 2>&1; tail -1 "$HARNESS_LOG")"
  [ "$RESTORE_STATE" = "false" ] || fail "AXMinimized did not become false after restore: $RESTORE_STATE"
  log "PASS: AXMinimized false after restore"
  delay 2
  RESTORE_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve restored window bounds"
  log "minimize_restored_window=$RESTORE_WIN_LINE"
  IFS=$'\t' read -r RESTORE_WINDOW_ID RESTORE_WX RESTORE_WY RESTORE_WW RESTORE_WH <<<"$RESTORE_WIN_LINE"
  [ "$RESTORE_WINDOW_ID" = "$A_WINDOW_ID" ] || fail "minimize restore window id changed unexpectedly: expected=$A_WINDOW_ID actual=$RESTORE_WINDOW_ID"
  RESTORE_BACKING_LINE="$(wait_for_line_after "$RESTORE_START_LINE" "TermSurf geometry layer=appkit event=backing_properties_changed .*pane_id:${A_PANE_ID} .*context_id=${A_CONTEXT_ID}" "minimize-restored AppKit backing properties" 45)"
  RESTORE_PRESENT_WINDOW_ID="$(printf '%s\n' "$RESTORE_BACKING_LINE" | sed -E 's/.*window_id:([^ ]+) .*/\1/')"
  RESTORE_SURFACE_ID="$(extract_surface_id "$RESTORE_BACKING_LINE")"
  RESTORE_SELECTED_TAB_ID="$(extract_selected_tab_id "$RESTORE_BACKING_LINE")"
  RESTORE_FRAME="$(extract_overlay_frame "$RESTORE_BACKING_LINE")"
  RESTORE_SCALE="$(extract_backing_scale "$RESTORE_BACKING_LINE")"
  RESTORE_PIXEL="$(appkit_pixel_from_geometry_line "$RESTORE_BACKING_LINE")"
  [ "$RESTORE_PRESENT_WINDOW_ID" = "$A_WINDOW_ID" ] || fail "minimize-restored AppKit window id mismatch"
  [ "$RESTORE_SURFACE_ID" = "$A_SURFACE_ID" ] || fail "minimize-restored surface id changed"
  [ "$RESTORE_SELECTED_TAB_ID" = "$A_SELECTED_TAB_ID" ] || fail "minimize-restored selected tab id changed"
  [ "$RESTORE_FRAME" = "$A_FRAME" ] || fail "minimize-restored frame mismatch: expected=$A_FRAME actual=$RESTORE_FRAME"
  [ "$RESTORE_SCALE" = "$A_BACKING_SCALE" ] || fail "minimize-restored backing scale mismatch: expected=$A_BACKING_SCALE actual=$RESTORE_SCALE"
  [ "$RESTORE_PIXEL" = "$A_PIXEL" ] || fail "minimize-restored current AppKit pixel mismatch: expected=$A_PIXEL actual=$RESTORE_PIXEL"
  log "PASS: minimize-restored current AppKit pixel matched baseline"
  require_no_different_appkit_frame_after "$RESTORE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "minimize-restored AppKit frame stayed stable"
  require_no_different_appkit_pixels_after "$RESTORE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "minimize-restored AppKit pixels stayed stable"

  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_MINIMIZE_RESTORED"
  log "minimize_restored_screenshot_exit=$?"
  RESTORE_HIT_START_LINE="$(log_line_count)"
  click_window_center "$RESTORE_WIN_LINE" "minimize_restored_browser_area"
  RESTORE_HIT_LINE="$(wait_for_hit_after "$RESTORE_HIT_START_LINE" "$A_CONTEXT_ID" "minimize-restored browser hit-test")"
  require_text "$RESTORE_HIT_LINE" "window_id:${A_WINDOW_ID}" "minimize-restored hit-test has window id"
  require_text "$RESTORE_HIT_LINE" "surface_id:${A_SURFACE_ID}" "minimize-restored hit-test has surface id"
  require_text "$RESTORE_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "minimize-restored hit-test has selected tab id"
  require_text "$RESTORE_HIT_LINE" "overlay_frame=${A_FRAME}" "minimize-restored hit-test uses AppKit frame"
  require_text "$RESTORE_HIT_LINE" "web_point={" "minimize-restored hit-test includes webview-relative point"

  RESTORE_MODE_START_LINE="$(log_line_count)"
  RESTORE_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "minimize_restored_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$RESTORE_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "minimize-restored webtui entered browse mode"
  require_trace_after "$RESTORE_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed focus=true after minimize restore"
  RESTORE_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP19_MINIMIZE_RESTORE\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$RESTORE_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "minimize-restored keyboard marker reached browser"

  RESTORE_CONTROL_START_LINE="$(log_line_count)"
  RESTORE_CONTROL_TRACE_START_LINE="$(trace_line_count)"
  log "minimize_restored_control_key=escape=Mode::Control"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$RESTORE_CONTROL_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=false" "minimize-restored webtui returned to control mode"
  require_trace_after "$RESTORE_CONTROL_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed focus=false before hide"

  HIDE_START_LINE="$(log_line_count)"
  HIDE_RESULT="$(swift "$HIDE_APP" "$PID" hide >>"$HARNESS_LOG" 2>&1; tail -1 "$HARNESS_LOG")"
  [ "$HIDE_RESULT" = "hidden" ] || fail "app hide did not report hidden: $HIDE_RESULT"
  log "PASS: app hide reported hidden"
  delay 2
  if app_windows | awk -F '\t' -v wid="$A_WINDOW_ID" '$1 == wid { found=1 } END { exit found ? 0 : 1 }'; then
    fail "hidden window remained visible in onscreen CG window list"
  fi
  log "PASS: hidden window absent from onscreen CG window list"

  HIDDEN_HIT_START_LINE="$(log_line_count)"
  click_global_point "$FORMER_CLICK_X" "$FORMER_CLICK_Y" "hidden_former_browser_area"
  wait_for_negative_hit_after "$HIDDEN_HIT_START_LINE" "$A_CONTEXT_ID" "hidden former browser area hit-test" allow-absent 3

  SHOW_START_LINE="$(log_line_count)"
  SHOW_RESULT="$(swift "$HIDE_APP" "$PID" show >>"$HARNESS_LOG" 2>&1; tail -1 "$HARNESS_LOG")"
  [ "$SHOW_RESULT" = "visible" ] || fail "app show did not report visible: $SHOW_RESULT"
  log "PASS: app show reported visible"
  delay 2
  SHOW_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve unhidden window bounds"
  log "hide_restored_window=$SHOW_WIN_LINE"
  IFS=$'\t' read -r SHOW_WINDOW_ID SHOW_WX SHOW_WY SHOW_WW SHOW_WH <<<"$SHOW_WIN_LINE"
  [ "$SHOW_WINDOW_ID" = "$A_WINDOW_ID" ] || fail "hide restore window id changed unexpectedly: expected=$A_WINDOW_ID actual=$SHOW_WINDOW_ID"
  require_no_different_appkit_frame_after "$SHOW_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "hide-restored AppKit frame stayed stable"
  require_no_different_appkit_pixels_after "$SHOW_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "hide-restored AppKit pixels stayed stable"

  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_HIDE_RESTORED"
  log "hide_restored_screenshot_exit=$?"
  SHOW_HIT_START_LINE="$(log_line_count)"
  click_window_center "$SHOW_WIN_LINE" "hide_restored_browser_area"
  SHOW_HIT_LINE="$(wait_for_hit_after "$SHOW_HIT_START_LINE" "$A_CONTEXT_ID" "hide-restored browser hit-test")"
  require_text "$SHOW_HIT_LINE" "window_id:${A_WINDOW_ID}" "hide-restored hit-test has window id"
  require_text "$SHOW_HIT_LINE" "surface_id:${A_SURFACE_ID}" "hide-restored hit-test has surface id"
  require_text "$SHOW_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "hide-restored hit-test has selected tab id"
  require_text "$SHOW_HIT_LINE" "overlay_frame=${A_FRAME}" "hide-restored hit-test uses AppKit frame"
  require_text "$SHOW_HIT_LINE" "backing_scale=${A_BACKING_SCALE}" "hide-restored hit-test has backing scale"
  require_text "$SHOW_HIT_LINE" "web_point={" "hide-restored hit-test includes webview-relative point"
  SHOW_PIXEL="$(appkit_pixel_from_geometry_line "$SHOW_HIT_LINE")"
  [ "$SHOW_PIXEL" = "$A_PIXEL" ] || fail "hide-restored current AppKit pixel mismatch: expected=$A_PIXEL actual=$SHOW_PIXEL"
  log "PASS: hide-restored current AppKit pixel matched baseline"

  SHOW_MODE_START_LINE="$(log_line_count)"
  SHOW_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "hide_restored_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$SHOW_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "hide-restored webtui entered browse mode"
  require_trace_after "$SHOW_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed focus=true after hide restore"
  SHOW_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP19_HIDE_RESTORE\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$SHOW_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "hide-restored keyboard marker reached browser"
fi

if [ "$SCENARIO" = "open-browser-in-new-window" ]; then
  A_WINDOW_ID="$WID"
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_FRAME_X="$OVERLAY_FRAME_X"
  A_FRAME_Y="$OVERLAY_FRAME_Y"
  A_ROOT_FRAME_SIZE="$(extract_root_frame_size "$APPKIT_PRESENT_LINE")"
  A_PIXEL="$APPKIT_PIXEL"
  log "browser_a_window_id=$A_WINDOW_ID"
  log "browser_a_selected_tab_id=$A_SELECTED_TAB_ID"
  log "browser_a_pane_id=$A_PANE_ID"
  log "browser_a_browser_tab_id=$A_BROWSER_TAB_ID"
  log "browser_a_context_id=$A_CONTEXT_ID"

  NEW_WINDOW_START_LINE="$(log_line_count)"
  NEW_WINDOW_TRACE_START_LINE="$(trace_line_count)"
  log "new_window_keybind=ctrl+b=new_window"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 11 control >>"$HARNESS_LOG" 2>&1
  delay 2

  require_log_after "$NEW_WINDOW_START_LINE" "dispatching action target=surface action=.new_window" "new window action dispatched"
  require_log_after "$NEW_WINDOW_START_LINE" 'starting command command=`/usr/bin/login`' "new window started plain login shell"
  if [ -s "$NEW_TAB_COMMAND_LOG" ]; then
    fail "new window unexpectedly inherited and ran the first-run web wrapper"
  fi
  log "PASS: new window did not inherit the first-run web wrapper"

  B_WIN_LINE=""
  for _ in $(seq 1 30); do
    B_WIN_LINE="$(app_windows | awk -F '\t' -v old="$A_WINDOW_ID" '$1 != old { print; exit }' || true)"
    if [ -n "$B_WIN_LINE" ]; then
      break
    fi
    delay 1
  done
  [ -n "$B_WIN_LINE" ] || fail "timed out waiting for second Ghostboard window"
  IFS=$'\t' read -r B_WINDOW_ID B_WX B_WY B_WW B_WH <<<"$B_WIN_LINE"
  [ "$B_WINDOW_ID" != "$A_WINDOW_ID" ] || fail "new window reused browser A window id"
  log "window_b=$B_WIN_LINE"

  click_window_center "$B_WIN_LINE" "window_b_shell"
  delay 1

  BROWSER_B_START_LINE="$(log_line_count)"
  BROWSER_B_TRACE_START_LINE="$(trace_line_count)"
  printf '"%s" --browser "%s" "%s"' "$WEB" "$CHROMIUM" "$URL_B" >"$SECOND_BROWSER_COMMAND"
  log "browser_b_command=$(cat "$SECOND_BROWSER_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$SECOND_BROWSER_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1

  B_CA_CONTEXT_LINE="$(wait_for_different_zig_event_after "$BROWSER_B_START_LINE" "ca_context" "$A_PANE_ID" "browser B Zig ca_context")"
  B_PANE_ID="$(extract_pane_id "$B_CA_CONTEXT_LINE")"
  B_BROWSER_TAB_ID="$(extract_browser_tab_id "$B_CA_CONTEXT_LINE")"
  B_CONTEXT_ID="$(extract_context_id "$B_CA_CONTEXT_LINE")"
  [ -n "$B_PANE_ID" ] || fail "could not extract browser B pane id"
  [ -n "$B_BROWSER_TAB_ID" ] || fail "could not extract browser B tab id"
  [ -n "$B_CONTEXT_ID" ] || fail "could not extract browser B context id"
  [ "$B_PANE_ID" != "$A_PANE_ID" ] || fail "browser B reused browser A pane id"
  [ "$B_BROWSER_TAB_ID" != "$A_BROWSER_TAB_ID" ] || fail "browser B reused browser A tab id"
  [ "$B_CONTEXT_ID" != "$A_CONTEXT_ID" ] || fail "browser B reused browser A CA/context id"
  log "browser_b_pane_id=$B_PANE_ID"
  log "browser_b_browser_tab_id=$B_BROWSER_TAB_ID"
  log "browser_b_context_id=$B_CONTEXT_ID"

  B_APPKIT_PRESENT_LINE="$(wait_for_line_after "$BROWSER_B_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${B_PANE_ID} .*context_id=${B_CONTEXT_ID}" "browser B AppKit presentation")"
  B_APPKIT_PIXELS_LINE="$(wait_for_line_after "$BROWSER_B_START_LINE" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${B_PANE_ID} .*context_id=${B_CONTEXT_ID}" "browser B AppKit pixels")"
  B_PRESENT_WINDOW_ID="$(printf '%s\n' "$B_APPKIT_PRESENT_LINE" | sed -E 's/.*window_id:([^ ]+) .*/\1/')"
  B_SELECTED_TAB_ID="$(extract_selected_tab_id "$B_APPKIT_PRESENT_LINE")"
  B_FRAME="$(extract_overlay_frame "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_SIZE="$(extract_frame_size "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_X="$(extract_frame_x "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_Y="$(extract_frame_y "$B_APPKIT_PRESENT_LINE")"
  B_PIXEL="$(extract_appkit_pixel "$B_APPKIT_PIXELS_LINE")"
  [ "$B_PRESENT_WINDOW_ID" = "$B_WINDOW_ID" ] || fail "browser B AppKit window id mismatch: expected=$B_WINDOW_ID actual=$B_PRESENT_WINDOW_ID"
  [ "$B_SELECTED_TAB_ID" = "$B_WINDOW_ID" ] || fail "browser B selected tab id mismatch: expected=$B_WINDOW_ID actual=$B_SELECTED_TAB_ID"
  log "browser_b_window_id=$B_PRESENT_WINDOW_ID"
  log "browser_b_selected_tab_id=$B_SELECTED_TAB_ID"
  log "browser_b_overlay_frame=$B_FRAME"
  log "browser_b_overlay_frame_size=$B_FRAME_SIZE"
  log "browser_b_appkit_pixel=$B_PIXEL"

  B_PIXEL_WIDTH="${B_PIXEL%x*}"
  B_PIXEL_HEIGHT="${B_PIXEL#*x}"
  require_trace_after "$BROWSER_B_TRACE_START_LINE" "resize tab_id=${B_BROWSER_TAB_ID} pane_id=${B_PANE_ID} pixel_width=${B_PIXEL_WIDTH} pixel_height=${B_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied browser B resize to AppKit pixel size"

  if tail -n +"$((BROWSER_B_START_LINE + 1))" "$APP_LOG" |
    grep -E "TermSurf geometry layer=appkit event=presented .*window_id:${B_WINDOW_ID} .*pane_id:${A_PANE_ID} .*context_id=${A_CONTEXT_ID} .*visible=true" >/dev/null 2>&1; then
    fail "browser A overlay was presented as visible in window B"
  fi
  log "PASS: browser A was not presented as visible in window B"

  screencapture -x -o -l"$B_WINDOW_ID" "$SCREENSHOT_WINDOW_B"
  log "window_b_screenshot_exit=$?"

  B_CLICK_X="$(awk -v wx="$B_WX" -v frame_x="$B_FRAME_X" -v frame_size="$B_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
  B_CLICK_Y="$(awk -v wy="$B_WY" -v frame_y="$B_FRAME_Y" -v frame_size="$B_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"
  B_HIT_START_LINE="$(log_line_count)"
  click_global_point "$B_CLICK_X" "$B_CLICK_Y" "browser_b_window_area"
  B_HIT_LINE="$(wait_for_hit_after "$B_HIT_START_LINE" "$B_CONTEXT_ID" "browser B hit-test in window B")"
  require_text "$B_HIT_LINE" "window_id:${B_WINDOW_ID}" "browser B hit-test has window B id"
  require_text "$B_HIT_LINE" "selected_tab_id:${B_SELECTED_TAB_ID}" "browser B hit-test has window B selected tab id"
  require_text "$B_HIT_LINE" "overlay_frame=${B_FRAME}" "browser B hit-test uses browser B frame"
  require_text "$B_HIT_LINE" "web_point={" "browser B hit-test includes webview-relative point"

  B_MODE_START_LINE="$(log_line_count)"
  B_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "browser_b_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$B_MODE_START_LINE" "ModeChanged: pane_id=${B_PANE_ID} browsing=true" "browser B webtui entered browse mode"
  require_trace_after "$B_MODE_TRACE_START_LINE" "focus-changed tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed browser B focus=true after browse mode"

  B_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP15_BROWSER_B_WINDOW\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$B_KEY_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser B keyboard marker reached browser B"
  require_no_trace_after "$B_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser B keyboard marker did not reach browser A"

  B_CONTROL_START_LINE="$(log_line_count)"
  B_CONTROL_TRACE_START_LINE="$(trace_line_count)"
  log "browser_b_control_key=escape=Mode::Control"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$B_CONTROL_START_LINE" "ModeChanged: pane_id=${B_PANE_ID} browsing=false" "browser B webtui returned to control mode"
  require_trace_after "$B_CONTROL_TRACE_START_LINE" "focus-changed tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed browser B focus=false after control mode"

  A_RESTORE_START_LINE="$(log_line_count)"
  A_RESTORE_TRACE_START_LINE="$(trace_line_count)"
  A_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve browser A window bounds for window id=$A_WINDOW_ID"
  focus_window_bounds "$A_WIN_LINE" >>"$HARNESS_LOG" 2>&1 || fail "failed to focus browser A window via accessibility"
  IFS=$'\t' read -r _A_WID A_WX A_WY A_WW A_WH <<<"$A_WIN_LINE"
  A_ROOT_HEIGHT="$(pair_height "$A_ROOT_FRAME_SIZE")"
  A_CONTENT_Y_OFFSET="$(awk -v wh="$A_WH" -v root_h="$A_ROOT_HEIGHT" 'BEGIN { print int(wh - root_h) }')"
  A_VISIBLE_X="$(awk -v wx="$A_WX" -v frame_x="$A_FRAME_X" 'BEGIN { print int(wx + frame_x + 4) }')"
  A_VISIBLE_Y="$(awk -v wy="$A_WY" -v content_y="$A_CONTENT_Y_OFFSET" -v frame_y="$A_FRAME_Y" 'BEGIN { print int(wy + content_y + frame_y + 4) }')"
  A_HIT_START_LINE="$(log_line_count)"
  click_global_point "$A_VISIBLE_X" "$A_VISIBLE_Y" "window_a_visible_edge_restore"
  A_HIT_LINE="$(wait_for_hit_after "$A_HIT_START_LINE" "$A_CONTEXT_ID" "browser A restored visible hit-test in window A")"
  require_no_different_appkit_frame_after "$A_RESTORE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "browser A kept AppKit frame after returning to window A"
  require_no_different_appkit_pixels_after "$A_RESTORE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "browser A kept AppKit pixels after returning to window A"

  require_text "$A_HIT_LINE" "window_id:${A_WINDOW_ID}" "browser A restored hit-test has window A id"
  require_text "$A_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "browser A restored hit-test has original selected tab id"
  require_text "$A_HIT_LINE" "overlay_frame=${A_FRAME}" "browser A restored hit-test uses browser A frame"
  require_text "$A_HIT_LINE" "web_point={" "browser A restored hit-test includes webview-relative point"

  A_MODE_START_LINE="$(log_line_count)"
  A_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "browser_a_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$A_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "browser A webtui entered browse mode after window restore"
  require_trace_after "$A_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed browser A focus=true after window restore"

  A_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP15_BROWSER_A_WINDOW\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$A_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser A keyboard marker reached browser A"
  require_no_trace_after "$A_KEY_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser A keyboard marker did not reach browser B"

  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_WINDOW_A_RESTORED"
  log "window_a_restored_screenshot_exit=$?"

  [ "$NEW_WINDOW_TRACE_START_LINE" -lt "$BROWSER_B_TRACE_START_LINE" ] || fail "trace boundaries for browser B window open were not monotonic"
  [ "$BROWSER_B_TRACE_START_LINE" -lt "$A_RESTORE_TRACE_START_LINE" ] || fail "trace boundaries for browser A window restore were not monotonic"
fi

if [ "$SCENARIO" = "multiple-windows-with-browsers" ]; then
  A_WINDOW_ID="$WID"
  A_SURFACE_ID="$(extract_surface_id "$APPKIT_PRESENT_LINE")"
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_FRAME_X="$OVERLAY_FRAME_X"
  A_FRAME_Y="$OVERLAY_FRAME_Y"
  A_ROOT_FRAME_SIZE="$(extract_root_frame_size "$APPKIT_PRESENT_LINE")"
  A_PIXEL="$APPKIT_PIXEL"
  log "browser_a_window_id=$A_WINDOW_ID"
  log "browser_a_surface_id=$A_SURFACE_ID"
  log "browser_a_selected_tab_id=$A_SELECTED_TAB_ID"
  log "browser_a_pane_id=$A_PANE_ID"
  log "browser_a_browser_tab_id=$A_BROWSER_TAB_ID"
  log "browser_a_context_id=$A_CONTEXT_ID"

  NEW_WINDOW_B_START_LINE="$(log_line_count)"
  NEW_WINDOW_B_TRACE_START_LINE="$(trace_line_count)"
  log "new_window_b_keybind=ctrl+b=new_window"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 11 control >>"$HARNESS_LOG" 2>&1
  delay 2

  require_log_after "$NEW_WINDOW_B_START_LINE" "dispatching action target=surface action=.new_window" "window B new-window action dispatched"
  require_log_after "$NEW_WINDOW_B_START_LINE" 'starting command command=`/usr/bin/login`' "window B started plain login shell"
  if [ -s "$NEW_TAB_COMMAND_LOG" ]; then
    fail "window B unexpectedly inherited and ran the first-run web wrapper"
  fi
  log "PASS: window B did not inherit the first-run web wrapper"

  B_WIN_LINE=""
  for _ in $(seq 1 30); do
    B_WIN_LINE="$(app_windows | awk -F '\t' -v old="$A_WINDOW_ID" '$1 != old { print; exit }' || true)"
    if [ -n "$B_WIN_LINE" ]; then
      break
    fi
    delay 1
  done
  [ -n "$B_WIN_LINE" ] || fail "timed out waiting for second Ghostboard window"
  IFS=$'\t' read -r B_WINDOW_ID B_WX B_WY B_WW B_WH <<<"$B_WIN_LINE"
  [ "$B_WINDOW_ID" != "$A_WINDOW_ID" ] || fail "window B reused browser A window id"
  log "window_b=$B_WIN_LINE"

  click_window_center "$B_WIN_LINE" "window_b_shell"
  delay 1

  BROWSER_B_START_LINE="$(log_line_count)"
  BROWSER_B_TRACE_START_LINE="$(trace_line_count)"
  printf '"%s" --browser "%s" "%s"' "$WEB" "$CHROMIUM" "$URL_B" >"$SECOND_BROWSER_COMMAND"
  log "browser_b_command=$(cat "$SECOND_BROWSER_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$SECOND_BROWSER_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1

  B_CA_CONTEXT_LINE="$(wait_for_different_zig_event_after "$BROWSER_B_START_LINE" "ca_context" "$A_PANE_ID" "browser B Zig ca_context")"
  B_PANE_ID="$(extract_pane_id "$B_CA_CONTEXT_LINE")"
  B_BROWSER_TAB_ID="$(extract_browser_tab_id "$B_CA_CONTEXT_LINE")"
  B_CONTEXT_ID="$(extract_context_id "$B_CA_CONTEXT_LINE")"
  [ -n "$B_PANE_ID" ] || fail "could not extract browser B pane id"
  [ -n "$B_BROWSER_TAB_ID" ] || fail "could not extract browser B tab id"
  [ -n "$B_CONTEXT_ID" ] || fail "could not extract browser B context id"
  [ "$B_PANE_ID" != "$A_PANE_ID" ] || fail "browser B reused browser A pane id"
  [ "$B_BROWSER_TAB_ID" != "$A_BROWSER_TAB_ID" ] || fail "browser B reused browser A tab id"
  [ "$B_CONTEXT_ID" != "$A_CONTEXT_ID" ] || fail "browser B reused browser A CA/context id"
  log "browser_b_pane_id=$B_PANE_ID"
  log "browser_b_browser_tab_id=$B_BROWSER_TAB_ID"
  log "browser_b_context_id=$B_CONTEXT_ID"

  B_APPKIT_PRESENT_LINE="$(wait_for_line_after "$BROWSER_B_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${B_PANE_ID} .*context_id=${B_CONTEXT_ID}" "browser B AppKit presentation")"
  B_APPKIT_PIXELS_LINE="$(wait_for_line_after "$BROWSER_B_START_LINE" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${B_PANE_ID} .*context_id=${B_CONTEXT_ID}" "browser B AppKit pixels")"
  B_PRESENT_WINDOW_ID="$(printf '%s\n' "$B_APPKIT_PRESENT_LINE" | sed -E 's/.*window_id:([^ ]+) .*/\1/')"
  B_SURFACE_ID="$(extract_surface_id "$B_APPKIT_PRESENT_LINE")"
  B_SELECTED_TAB_ID="$(extract_selected_tab_id "$B_APPKIT_PRESENT_LINE")"
  B_FRAME="$(extract_overlay_frame "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_SIZE="$(extract_frame_size "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_X="$(extract_frame_x "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_Y="$(extract_frame_y "$B_APPKIT_PRESENT_LINE")"
  B_ROOT_FRAME_SIZE="$(extract_root_frame_size "$B_APPKIT_PRESENT_LINE")"
  B_PIXEL="$(extract_appkit_pixel "$B_APPKIT_PIXELS_LINE")"
  [ "$B_PRESENT_WINDOW_ID" = "$B_WINDOW_ID" ] || fail "browser B AppKit window id mismatch: expected=$B_WINDOW_ID actual=$B_PRESENT_WINDOW_ID"
  [ -n "$B_SURFACE_ID" ] || fail "could not extract browser B AppKit surface id"
  [ "$B_SURFACE_ID" != "$A_SURFACE_ID" ] || fail "browser B reused browser A surface id"
  [ "$B_SELECTED_TAB_ID" = "$B_WINDOW_ID" ] || fail "browser B selected tab id mismatch: expected=$B_WINDOW_ID actual=$B_SELECTED_TAB_ID"
  log "browser_b_window_id=$B_PRESENT_WINDOW_ID"
  log "browser_b_surface_id=$B_SURFACE_ID"
  log "browser_b_selected_tab_id=$B_SELECTED_TAB_ID"
  log "browser_b_overlay_frame=$B_FRAME"
  log "browser_b_appkit_pixel=$B_PIXEL"

  B_PIXEL_WIDTH="${B_PIXEL%x*}"
  B_PIXEL_HEIGHT="${B_PIXEL#*x}"
  require_trace_after "$BROWSER_B_TRACE_START_LINE" "resize tab_id=${B_BROWSER_TAB_ID} pane_id=${B_PANE_ID} pixel_width=${B_PIXEL_WIDTH} pixel_height=${B_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied browser B resize to AppKit pixel size"

  NEW_WINDOW_C_START_LINE="$(log_line_count)"
  NEW_WINDOW_C_TRACE_START_LINE="$(trace_line_count)"
  log "new_window_c_keybind=ctrl+b=new_window"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 11 control >>"$HARNESS_LOG" 2>&1
  delay 2

  require_log_after "$NEW_WINDOW_C_START_LINE" "dispatching action target=surface action=.new_window" "window C new-window action dispatched"
  require_log_after "$NEW_WINDOW_C_START_LINE" 'starting command command=`/usr/bin/login`' "window C started plain login shell"
  if [ -s "$NEW_TAB_COMMAND_LOG" ]; then
    fail "window C unexpectedly inherited and ran the first-run web wrapper"
  fi
  log "PASS: window C did not inherit the first-run web wrapper"

  C_WIN_LINE=""
  for _ in $(seq 1 30); do
    C_WIN_LINE="$(app_windows | awk -F '\t' -v a="$A_WINDOW_ID" -v b="$B_WINDOW_ID" '$1 != a && $1 != b { print; exit }' || true)"
    if [ -n "$C_WIN_LINE" ]; then
      break
    fi
    delay 1
  done
  [ -n "$C_WIN_LINE" ] || fail "timed out waiting for third Ghostboard window"
  IFS=$'\t' read -r C_WINDOW_ID C_WX C_WY C_WW C_WH <<<"$C_WIN_LINE"
  [ "$C_WINDOW_ID" != "$A_WINDOW_ID" ] || fail "window C reused browser A window id"
  [ "$C_WINDOW_ID" != "$B_WINDOW_ID" ] || fail "window C reused browser B window id"
  log "window_c=$C_WIN_LINE"

  click_window_center "$C_WIN_LINE" "window_c_shell"
  delay 1

  BROWSER_C_START_LINE="$(log_line_count)"
  BROWSER_C_TRACE_START_LINE="$(trace_line_count)"
  printf '"%s" --browser "%s" "%s"' "$WEB" "$CHROMIUM" "$URL_C" >"$THIRD_BROWSER_COMMAND"
  log "browser_c_command=$(cat "$THIRD_BROWSER_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$THIRD_BROWSER_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1

  C_CA_CONTEXT_LINE="$(wait_for_different_zig_event_after "$BROWSER_C_START_LINE" "ca_context" "$B_PANE_ID" "browser C Zig ca_context")"
  C_PANE_ID="$(extract_pane_id "$C_CA_CONTEXT_LINE")"
  C_BROWSER_TAB_ID="$(extract_browser_tab_id "$C_CA_CONTEXT_LINE")"
  C_CONTEXT_ID="$(extract_context_id "$C_CA_CONTEXT_LINE")"
  [ -n "$C_PANE_ID" ] || fail "could not extract browser C pane id"
  [ -n "$C_BROWSER_TAB_ID" ] || fail "could not extract browser C tab id"
  [ -n "$C_CONTEXT_ID" ] || fail "could not extract browser C context id"
  [ "$C_PANE_ID" != "$A_PANE_ID" ] || fail "browser C reused browser A pane id"
  [ "$C_PANE_ID" != "$B_PANE_ID" ] || fail "browser C reused browser B pane id"
  [ "$C_BROWSER_TAB_ID" != "$A_BROWSER_TAB_ID" ] || fail "browser C reused browser A tab id"
  [ "$C_BROWSER_TAB_ID" != "$B_BROWSER_TAB_ID" ] || fail "browser C reused browser B tab id"
  [ "$C_CONTEXT_ID" != "$A_CONTEXT_ID" ] || fail "browser C reused browser A CA/context id"
  [ "$C_CONTEXT_ID" != "$B_CONTEXT_ID" ] || fail "browser C reused browser B CA/context id"
  log "browser_c_pane_id=$C_PANE_ID"
  log "browser_c_browser_tab_id=$C_BROWSER_TAB_ID"
  log "browser_c_context_id=$C_CONTEXT_ID"

  C_APPKIT_PRESENT_LINE="$(wait_for_line_after "$BROWSER_C_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${C_PANE_ID} .*context_id=${C_CONTEXT_ID}" "browser C AppKit presentation")"
  C_APPKIT_PIXELS_LINE="$(wait_for_line_after "$BROWSER_C_START_LINE" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${C_PANE_ID} .*context_id=${C_CONTEXT_ID}" "browser C AppKit pixels")"
  C_PRESENT_WINDOW_ID="$(printf '%s\n' "$C_APPKIT_PRESENT_LINE" | sed -E 's/.*window_id:([^ ]+) .*/\1/')"
  C_SURFACE_ID="$(extract_surface_id "$C_APPKIT_PRESENT_LINE")"
  C_SELECTED_TAB_ID="$(extract_selected_tab_id "$C_APPKIT_PRESENT_LINE")"
  C_FRAME="$(extract_overlay_frame "$C_APPKIT_PRESENT_LINE")"
  C_FRAME_SIZE="$(extract_frame_size "$C_APPKIT_PRESENT_LINE")"
  C_FRAME_X="$(extract_frame_x "$C_APPKIT_PRESENT_LINE")"
  C_FRAME_Y="$(extract_frame_y "$C_APPKIT_PRESENT_LINE")"
  C_ROOT_FRAME_SIZE="$(extract_root_frame_size "$C_APPKIT_PRESENT_LINE")"
  C_PIXEL="$(extract_appkit_pixel "$C_APPKIT_PIXELS_LINE")"
  [ "$C_PRESENT_WINDOW_ID" = "$C_WINDOW_ID" ] || fail "browser C AppKit window id mismatch: expected=$C_WINDOW_ID actual=$C_PRESENT_WINDOW_ID"
  [ -n "$C_SURFACE_ID" ] || fail "could not extract browser C AppKit surface id"
  [ "$C_SURFACE_ID" != "$A_SURFACE_ID" ] || fail "browser C reused browser A surface id"
  [ "$C_SURFACE_ID" != "$B_SURFACE_ID" ] || fail "browser C reused browser B surface id"
  [ "$C_SELECTED_TAB_ID" = "$C_WINDOW_ID" ] || fail "browser C selected tab id mismatch: expected=$C_WINDOW_ID actual=$C_SELECTED_TAB_ID"
  log "browser_c_window_id=$C_PRESENT_WINDOW_ID"
  log "browser_c_surface_id=$C_SURFACE_ID"
  log "browser_c_selected_tab_id=$C_SELECTED_TAB_ID"
  log "browser_c_overlay_frame=$C_FRAME"
  log "browser_c_appkit_pixel=$C_PIXEL"

  C_PIXEL_WIDTH="${C_PIXEL%x*}"
  C_PIXEL_HEIGHT="${C_PIXEL#*x}"
  require_trace_after "$BROWSER_C_TRACE_START_LINE" "resize tab_id=${C_BROWSER_TAB_ID} pane_id=${C_PANE_ID} pixel_width=${C_PIXEL_WIDTH} pixel_height=${C_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied browser C resize to AppKit pixel size"

  if tail -n +"$((BROWSER_C_START_LINE + 1))" "$APP_LOG" |
    grep -E "TermSurf geometry layer=appkit event=presented .*window_id:${C_WINDOW_ID} .*pane_id:${A_PANE_ID} .*context_id=${A_CONTEXT_ID} .*visible=true" >/dev/null 2>&1; then
    fail "browser A overlay was presented as visible in window C"
  fi
  log "PASS: browser A was not presented as visible in window C"
  if tail -n +"$((BROWSER_C_START_LINE + 1))" "$APP_LOG" |
    grep -E "TermSurf geometry layer=appkit event=presented .*window_id:${C_WINDOW_ID} .*pane_id:${B_PANE_ID} .*context_id=${B_CONTEXT_ID} .*visible=true" >/dev/null 2>&1; then
    fail "browser B overlay was presented as visible in window C"
  fi
  log "PASS: browser B was not presented as visible in window C"

  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_WINDOW_A_RESTORED"
  log "window_a_alive_screenshot_exit=$?"
  screencapture -x -o -l"$B_WINDOW_ID" "$SCREENSHOT_WINDOW_B"
  log "window_b_screenshot_exit=$?"
  screencapture -x -o -l"$C_WINDOW_ID" "$SCREENSHOT_WINDOW_C"
  log "window_c_screenshot_exit=$?"

  C_CLICK_X="$(awk -v wx="$C_WX" -v frame_x="$C_FRAME_X" -v frame_size="$C_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
  C_CLICK_Y="$(awk -v wy="$C_WY" -v frame_y="$C_FRAME_Y" -v frame_size="$C_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"
  C_HIT_START_LINE="$(log_line_count)"
  click_global_point "$C_CLICK_X" "$C_CLICK_Y" "browser_c_window_area"
  C_HIT_LINE="$(wait_for_hit_after "$C_HIT_START_LINE" "$C_CONTEXT_ID" "browser C hit-test in window C")"
  require_text "$C_HIT_LINE" "window_id:${C_WINDOW_ID}" "browser C hit-test has window C id"
  require_text "$C_HIT_LINE" "surface_id:${C_SURFACE_ID}" "browser C hit-test has surface C id"
  require_text "$C_HIT_LINE" "selected_tab_id:${C_SELECTED_TAB_ID}" "browser C hit-test has window C selected tab id"
  require_text "$C_HIT_LINE" "overlay_frame=${C_FRAME}" "browser C hit-test uses browser C frame"
  require_text "$C_HIT_LINE" "web_point={" "browser C hit-test includes webview-relative point"

  C_MODE_START_LINE="$(log_line_count)"
  C_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "browser_c_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$C_MODE_START_LINE" "ModeChanged: pane_id=${C_PANE_ID} browsing=true" "browser C webtui entered browse mode"
  require_trace_after "$C_MODE_TRACE_START_LINE" "focus-changed tab=${C_BROWSER_TAB_ID} pane=${C_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed browser C focus=true after browse mode"

  C_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP16_BROWSER_C_WINDOW\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$C_KEY_START_LINE" "key-event tab=${C_BROWSER_TAB_ID} pane=${C_PANE_ID}" "browser C keyboard marker reached browser C"
  require_no_trace_after "$C_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser C keyboard marker did not reach browser A"
  require_no_trace_after "$C_KEY_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser C keyboard marker did not reach browser B"

  C_CONTROL_START_LINE="$(log_line_count)"
  C_CONTROL_TRACE_START_LINE="$(trace_line_count)"
  log "browser_c_control_key=escape=Mode::Control"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$C_CONTROL_START_LINE" "ModeChanged: pane_id=${C_PANE_ID} browsing=false" "browser C webtui returned to control mode"
  require_trace_after "$C_CONTROL_TRACE_START_LINE" "focus-changed tab=${C_BROWSER_TAB_ID} pane=${C_PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed browser C focus=false after control mode"

  B_RESTORE_START_LINE="$(log_line_count)"
  B_RESTORE_TRACE_START_LINE="$(trace_line_count)"
  B_WIN_LINE="$(window_bounds_for "$B_WINDOW_ID")" || fail "failed to resolve browser B window bounds for window id=$B_WINDOW_ID"
  focus_window_bounds "$B_WIN_LINE" >>"$HARNESS_LOG" 2>&1 || fail "failed to focus browser B window via accessibility"
  IFS=$'\t' read -r _B_WID B_WX B_WY B_WW B_WH <<<"$B_WIN_LINE"
  B_ROOT_HEIGHT="$(pair_height "$B_ROOT_FRAME_SIZE")"
  B_CONTENT_Y_OFFSET="$(awk -v wh="$B_WH" -v root_h="$B_ROOT_HEIGHT" 'BEGIN { print int(wh - root_h) }')"
  B_VISIBLE_X="$(awk -v wx="$B_WX" -v frame_x="$B_FRAME_X" 'BEGIN { print int(wx + frame_x + 4) }')"
  B_VISIBLE_Y="$(awk -v wy="$B_WY" -v content_y="$B_CONTENT_Y_OFFSET" -v frame_y="$B_FRAME_Y" 'BEGIN { print int(wy + content_y + frame_y + 4) }')"
  B_HIT_START_LINE="$(log_line_count)"
  click_global_point "$B_VISIBLE_X" "$B_VISIBLE_Y" "window_b_visible_edge_restore"
  B_HIT_LINE="$(wait_for_hit_after "$B_HIT_START_LINE" "$B_CONTEXT_ID" "browser B restored visible hit-test in window B")"
  require_no_different_appkit_frame_after "$B_RESTORE_START_LINE" "$B_PANE_ID" "$B_CONTEXT_ID" "$B_FRAME" "browser B kept AppKit frame after returning to window B"
  require_no_different_appkit_pixels_after "$B_RESTORE_START_LINE" "$B_PANE_ID" "$B_CONTEXT_ID" "$B_PIXEL" "browser B kept AppKit pixels after returning to window B"
  require_text "$B_HIT_LINE" "window_id:${B_WINDOW_ID}" "browser B restored hit-test has window B id"
  require_text "$B_HIT_LINE" "surface_id:${B_SURFACE_ID}" "browser B restored hit-test has surface B id"
  require_text "$B_HIT_LINE" "selected_tab_id:${B_SELECTED_TAB_ID}" "browser B restored hit-test has selected tab B id"
  require_text "$B_HIT_LINE" "overlay_frame=${B_FRAME}" "browser B restored hit-test uses browser B frame"
  require_text "$B_HIT_LINE" "web_point={" "browser B restored hit-test includes webview-relative point"

  B_MODE_START_LINE="$(log_line_count)"
  B_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "browser_b_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$B_MODE_START_LINE" "ModeChanged: pane_id=${B_PANE_ID} browsing=true" "browser B webtui entered browse mode after window restore"
  require_trace_after "$B_MODE_TRACE_START_LINE" "focus-changed tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed browser B focus=true after window restore"

  B_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP16_BROWSER_B_WINDOW\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$B_KEY_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser B keyboard marker reached browser B"
  require_no_trace_after "$B_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser B keyboard marker did not reach browser A"
  require_no_trace_after "$B_KEY_START_LINE" "key-event tab=${C_BROWSER_TAB_ID} pane=${C_PANE_ID}" "browser B keyboard marker did not reach browser C"

  B_CONTROL_START_LINE="$(log_line_count)"
  B_CONTROL_TRACE_START_LINE="$(trace_line_count)"
  log "browser_b_control_key=escape=Mode::Control"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$B_CONTROL_START_LINE" "ModeChanged: pane_id=${B_PANE_ID} browsing=false" "browser B webtui returned to control mode"
  require_trace_after "$B_CONTROL_TRACE_START_LINE" "focus-changed tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed browser B focus=false after control mode"
  screencapture -x -o -l"$B_WINDOW_ID" "$SCREENSHOT_WINDOW_B_RESTORED"
  log "window_b_restored_screenshot_exit=$?"

  A_RESTORE_START_LINE="$(log_line_count)"
  A_RESTORE_TRACE_START_LINE="$(trace_line_count)"
  A_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve browser A window bounds for window id=$A_WINDOW_ID"
  focus_window_bounds "$A_WIN_LINE" >>"$HARNESS_LOG" 2>&1 || fail "failed to focus browser A window via accessibility"
  IFS=$'\t' read -r _A_WID A_WX A_WY A_WW A_WH <<<"$A_WIN_LINE"
  A_ROOT_HEIGHT="$(pair_height "$A_ROOT_FRAME_SIZE")"
  A_CONTENT_Y_OFFSET="$(awk -v wh="$A_WH" -v root_h="$A_ROOT_HEIGHT" 'BEGIN { print int(wh - root_h) }')"
  A_VISIBLE_X="$(awk -v wx="$A_WX" -v frame_x="$A_FRAME_X" 'BEGIN { print int(wx + frame_x + 4) }')"
  A_VISIBLE_Y="$(awk -v wy="$A_WY" -v content_y="$A_CONTENT_Y_OFFSET" -v frame_y="$A_FRAME_Y" 'BEGIN { print int(wy + content_y + frame_y + 4) }')"
  A_HIT_START_LINE="$(log_line_count)"
  click_global_point "$A_VISIBLE_X" "$A_VISIBLE_Y" "window_a_visible_edge_restore"
  A_HIT_LINE="$(wait_for_hit_after "$A_HIT_START_LINE" "$A_CONTEXT_ID" "browser A restored visible hit-test in window A")"
  require_no_different_appkit_frame_after "$A_RESTORE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "browser A kept AppKit frame after returning to window A"
  require_no_different_appkit_pixels_after "$A_RESTORE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "browser A kept AppKit pixels after returning to window A"
  require_text "$A_HIT_LINE" "window_id:${A_WINDOW_ID}" "browser A restored hit-test has window A id"
  require_text "$A_HIT_LINE" "surface_id:${A_SURFACE_ID}" "browser A restored hit-test has surface A id"
  require_text "$A_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "browser A restored hit-test has selected tab A id"
  require_text "$A_HIT_LINE" "overlay_frame=${A_FRAME}" "browser A restored hit-test uses browser A frame"
  require_text "$A_HIT_LINE" "web_point={" "browser A restored hit-test includes webview-relative point"

  A_MODE_START_LINE="$(log_line_count)"
  A_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "browser_a_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$A_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "browser A webtui entered browse mode after window restore"
  require_trace_after "$A_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed browser A focus=true after window restore"

  A_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP16_BROWSER_A_WINDOW\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$A_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser A keyboard marker reached browser A"
  require_no_trace_after "$A_KEY_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser A keyboard marker did not reach browser B"
  require_no_trace_after "$A_KEY_START_LINE" "key-event tab=${C_BROWSER_TAB_ID} pane=${C_PANE_ID}" "browser A keyboard marker did not reach browser C"

  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_WINDOW_A_RESTORED"
  log "window_a_restored_screenshot_exit=$?"

  [ "$NEW_WINDOW_B_TRACE_START_LINE" -lt "$BROWSER_B_TRACE_START_LINE" ] || fail "trace boundaries for browser B window open were not monotonic"
  [ "$BROWSER_B_TRACE_START_LINE" -lt "$NEW_WINDOW_C_TRACE_START_LINE" ] || fail "trace boundaries for window C open were not monotonic"
  [ "$NEW_WINDOW_C_TRACE_START_LINE" -lt "$BROWSER_C_TRACE_START_LINE" ] || fail "trace boundaries for browser C open were not monotonic"
  [ "$BROWSER_C_TRACE_START_LINE" -lt "$B_RESTORE_TRACE_START_LINE" ] || fail "trace boundaries for browser B window restore were not monotonic"
  [ "$B_RESTORE_TRACE_START_LINE" -lt "$A_RESTORE_TRACE_START_LINE" ] || fail "trace boundaries for browser A window restore were not monotonic"
fi

if [ "$SCENARIO" = "new-terminal-tab-visibility" ]; then
  BASE_SELECTED_TAB_ID="$(printf '%s\n' "$APPKIT_PRESENT_LINE" | sed -E 's/.*selected_tab_id:([^ ]+) .*/\1/')"
  [ -n "$BASE_SELECTED_TAB_ID" ] || fail "could not extract baseline selected tab id"
  BASE_FRAME="$OVERLAY_FRAME"
  BASE_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  BASE_FRAME_X="$OVERLAY_FRAME_X"
  BASE_FRAME_Y="$OVERLAY_FRAME_Y"
  BASE_PIXEL="$APPKIT_PIXEL"
  log "baseline_selected_tab_id=$BASE_SELECTED_TAB_ID"

  NEW_TAB_START_LINE="$(log_line_count)"
  NEW_TAB_TRACE_START_LINE="$(trace_line_count)"
  log "new_tab_keybind=ctrl+t=new_tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 17 control >>"$HARNESS_LOG" 2>&1
  delay 2

  require_log_after "$NEW_TAB_START_LINE" "dispatching action target=surface action=.new_tab" "new terminal tab action dispatched"
  require_log_after "$NEW_TAB_START_LINE" 'starting command command=`/usr/bin/login`' "new terminal tab started plain login shell"
  if [ -s "$NEW_TAB_COMMAND_LOG" ]; then
    fail "new terminal tab unexpectedly inherited and ran the first-run web wrapper"
  fi
  log "PASS: new terminal tab did not inherit the first-run web wrapper"

  TABBED_PRESENT_LINE="$(wait_for_changed_appkit_frame_after "$NEW_TAB_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$BASE_FRAME" "browser tab geometry adjusted for native tab bar")"
  TABBED_PIXELS_LINE="$(wait_for_changed_appkit_pixels_after "$NEW_TAB_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$BASE_PIXEL" "browser tab AppKit pixels adjusted for native tab bar")"
  TABBED_FRAME="$(extract_overlay_frame "$TABBED_PRESENT_LINE")"
  TABBED_FRAME_SIZE="$(extract_frame_size "$TABBED_PRESENT_LINE")"
  TABBED_FRAME_X="$(extract_frame_x "$TABBED_PRESENT_LINE")"
  TABBED_FRAME_Y="$(extract_frame_y "$TABBED_PRESENT_LINE")"
  TABBED_PIXEL="$(extract_appkit_pixel "$TABBED_PIXELS_LINE")"
  log "tabbed_overlay_frame=$TABBED_FRAME"
  log "tabbed_overlay_frame_size=$TABBED_FRAME_SIZE"
  log "tabbed_appkit_pixel=$TABBED_PIXEL"

  NEW_TAB_SELECT_START_LINE="$(log_line_count)"
  log "select_new_tab_keybind=ctrl+2=goto_tab:2"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 19 control >>"$HARNESS_LOG" 2>&1
  delay 1

  NEW_SELECTED_TAB_LINE="$(wait_for_selected_tab_change_after "$NEW_TAB_SELECT_START_LINE" "$BASE_SELECTED_TAB_ID" "new terminal tab selected")"
  NEW_SELECTED_TAB_ID="$(extract_selected_tab_id "$NEW_SELECTED_TAB_LINE")"
  [ -n "$NEW_SELECTED_TAB_ID" ] || fail "could not extract selected tab id for new terminal tab"
  log "PASS: new terminal tab selected"
  log "new_selected_tab_id=$NEW_SELECTED_TAB_ID"

  if tail -n +"$((NEW_TAB_START_LINE + 1))" "$APP_LOG" |
    grep -E "TermSurf geometry layer=zig event=(tab_ready|ca_context) " |
    grep -Fv "pane_id:${PANE_ID}" >/dev/null 2>&1; then
    fail "new terminal tab created a second browser pane/context"
  fi
  log "PASS: new terminal tab did not create a second browser pane/context"
  if tail -n +"$((NEW_TAB_TRACE_START_LINE + 1))" "$CHROMIUM_TRACE" |
    grep -E "resize tab_id=|title-changed tab=|key-event tab=|mouse-event tab=|mouse-move tab=" |
    grep -Fv "pane_id=${PANE_ID}" |
    grep -Fv "pane=${PANE_ID}" >/dev/null 2>&1; then
    fail "Chromium trace shows activity for a second browser context after new tab"
  fi
  log "PASS: Chromium trace shows no second browser context after new tab"

  screencapture -x -o -l"$NEW_SELECTED_TAB_ID" "$SCREENSHOT_TAB_NEW"
  log "new_tab_screenshot_exit=$?"

  if tail -n +"$((NEW_TAB_SELECT_START_LINE + 1))" "$APP_LOG" |
    grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${PANE_ID} .*context_id=${CONTEXT_ID} .*visible=true .*selected_tab_id:${NEW_SELECTED_TAB_ID}" >/dev/null 2>&1; then
    fail "original browser overlay was presented as visible in the selected new tab"
  fi
  log "PASS: original browser overlay was not freshly presented as visible in the selected new tab"

  TAB_WIN_LINE="$(window_bounds_for "$NEW_SELECTED_TAB_ID")" || fail "failed to resolve new-tab window bounds for window id=$NEW_SELECTED_TAB_ID"
  IFS=$'\t' read -r _TAB_WID TAB_WX TAB_WY TAB_WW TAB_WH <<<"$TAB_WIN_LINE"
  TAB_BROWSER_X="$(awk -v wx="$TAB_WX" -v frame_x="$TABBED_FRAME_X" -v frame_size="$TABBED_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
  TAB_BROWSER_Y="$(awk -v wy="$TAB_WY" -v frame_y="$TABBED_FRAME_Y" -v frame_size="$TABBED_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"

  click_negative_global_point "$TAB_BROWSER_X" "$TAB_BROWSER_Y" "new_tab_former_browser_area"
  wait_for_negative_hit_after "$NEGATIVE_HIT_START_LINE" "$CONTEXT_ID" "new terminal tab former browser area negative hit-test" allow-absent

  NEW_TAB_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP12_NEW_TAB_TERMINAL\n' >"$NEW_TAB_MARKER_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$NEW_TAB_MARKER_COMMAND" >>"$HARNESS_LOG" 2>&1
  delay 1
  require_no_trace_after "$NEW_TAB_KEY_START_LINE" "key-event tab=${BROWSER_TAB_ID} pane=${PANE_ID}" "new terminal tab keyboard marker did not reach original browser context"

  SWITCH_BACK_START_LINE="$(log_line_count)"
  SWITCH_BACK_TRACE_START_LINE="$(trace_line_count)"
  log "switch_back_keybind=ctrl+p=previous_tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 35 control >>"$HARNESS_LOG" 2>&1
  delay 1

  wait_for_log_after "$SWITCH_BACK_START_LINE" "Pane focus changed: pane_id=${PANE_ID} focused=true" "original browser pane focused again after tab switch"
  require_no_different_appkit_frame_after "$SWITCH_BACK_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$TABBED_FRAME" "browser tab kept tab-bar-adjusted AppKit frame after switch back"
  require_no_different_appkit_pixels_after "$SWITCH_BACK_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$TABBED_PIXEL" "browser tab kept tab-bar-adjusted AppKit pixels after switch back"

  BROWSER_RESTORE_HIT_START_LINE="$(log_line_count)"
  TAB_WIN_LINE="$(window_bounds_for "$BASE_SELECTED_TAB_ID")" || fail "failed to resolve restored browser-tab window bounds for window id=$BASE_SELECTED_TAB_ID"
  IFS=$'\t' read -r _TAB_WID TAB_WX TAB_WY TAB_WW TAB_WH <<<"$TAB_WIN_LINE"
  TAB_BROWSER_X="$(awk -v wx="$TAB_WX" -v frame_x="$TABBED_FRAME_X" -v frame_size="$TABBED_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
  TAB_BROWSER_Y="$(awk -v wy="$TAB_WY" -v frame_y="$TABBED_FRAME_Y" -v frame_size="$TABBED_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"
  click_global_point "$TAB_BROWSER_X" "$TAB_BROWSER_Y" "restored_browser_area"
  RESTORE_HIT_LINE="$(wait_for_hit_after "$BROWSER_RESTORE_HIT_START_LINE" "$CONTEXT_ID" "restored browser tab hit-test")"
  RESTORE_HIT_FRAME_SIZE="$(extract_frame_size "$RESTORE_HIT_LINE")"
  RESTORE_HIT_FRAME_X="$(extract_frame_x "$RESTORE_HIT_LINE")"
  RESTORE_HIT_FRAME_Y="$(extract_frame_y "$RESTORE_HIT_LINE")"
  require_text "$RESTORE_HIT_LINE" "selected_tab_id:${BASE_SELECTED_TAB_ID}" "restored browser hit-test has original selected tab id"
  [ "$RESTORE_HIT_FRAME_SIZE" = "$TABBED_FRAME_SIZE" ] || fail "restored hit-test frame size changed: expected=$TABBED_FRAME_SIZE actual=$RESTORE_HIT_FRAME_SIZE"
  [ "$RESTORE_HIT_FRAME_X" = "$TABBED_FRAME_X" ] || fail "restored hit-test frame x changed: expected=$TABBED_FRAME_X actual=$RESTORE_HIT_FRAME_X"
  [ "$RESTORE_HIT_FRAME_Y" = "$TABBED_FRAME_Y" ] || fail "restored hit-test frame y changed: expected=$TABBED_FRAME_Y actual=$RESTORE_HIT_FRAME_Y"
  require_text "$RESTORE_HIT_LINE" "web_point={" "restored browser hit-test includes webview-relative point"
  log "PASS: restored browser hit-test uses tab-bar-adjusted overlay frame"

  BROWSER_MODE_START_LINE="$(log_line_count)"
  BROWSER_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "restored_browser_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$BROWSER_MODE_START_LINE" "ModeChanged: pane_id=${PANE_ID} browsing=true" "webtui entered browse mode after tab restore"
  require_trace_after "$BROWSER_MODE_TRACE_START_LINE" "focus-changed tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed restored browser pane focus=true after browse mode"

  BROWSER_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP12_BROWSER_RESTORED\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  BROWSER_KEY_SEEN=""
  for _ in $(seq 1 10); do
    if tail -n +"$((BROWSER_KEY_START_LINE + 1))" "$CHROMIUM_TRACE" | grep -F "key-event tab=${BROWSER_TAB_ID} pane=${PANE_ID}" >/dev/null 2>&1; then
      BROWSER_KEY_SEEN="1"
      break
    fi
    delay 1
  done
  [ -n "$BROWSER_KEY_SEEN" ] || fail "restored browser tab keyboard marker did not reach original browser context"
  log "PASS: restored browser tab keyboard marker reached original browser context"

  screencapture -x -o -l"$WID" "$SCREENSHOT_TAB_BACK"
  log "back_tab_screenshot_exit=$?"
fi

if [ "$SCENARIO" = "open-browser-in-new-tab" ] || [ "$SCENARIO" = "close-browser-tab" ] || [ "$SCENARIO" = "same-profile-server-lifecycle" ] || [ "$SCENARIO" = "gui-active-multi-tab" ]; then
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  [ -n "$A_SELECTED_TAB_ID" ] || fail "could not extract browser A selected tab id"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_FRAME_X="$OVERLAY_FRAME_X"
  A_FRAME_Y="$OVERLAY_FRAME_Y"
  A_PIXEL="$APPKIT_PIXEL"
  log "browser_a_selected_tab_id=$A_SELECTED_TAB_ID"
  log "browser_a_pane_id=$A_PANE_ID"
  log "browser_a_browser_tab_id=$A_BROWSER_TAB_ID"
  log "browser_a_context_id=$A_CONTEXT_ID"

  if [ "$SCENARIO" = "same-profile-server-lifecycle" ]; then
    require_log "SetOverlay: pane_id=${A_PANE_ID} profile=default browser=${CHROMIUM}" "browser A SetOverlay uses default profile and absolute Chromium path"
    require_log "SetOverlay: created pending server key=default/${CHROMIUM} pane_count=1" "browser A created default profile server"
    SHARED_SPAWN_LINE="$(grep -E "spawned browser path=${CHROMIUM} pid=[0-9]+ profile=default .*--user-data-dir=.*chromium-profiles/default" "$APP_LOG" | tail -1 || true)"
    [ -n "$SHARED_SPAWN_LINE" ] || fail "missing default profile Chromium spawn line"
    SHARED_SPAWN_PID="$(printf '%s\n' "$SHARED_SPAWN_LINE" | sed -E 's/.* pid=([0-9]+) profile=.*/\1/')"
    [ -n "$SHARED_SPAWN_PID" ] || fail "failed to extract shared Chromium pid"
    log "same_profile_shared_spawn_pid=$SHARED_SPAWN_PID"
    log "PASS: browser A spawned shared default profile server"
  fi

  NEW_TAB_START_LINE="$(log_line_count)"
  NEW_TAB_TRACE_START_LINE="$(trace_line_count)"
  log "new_tab_keybind=ctrl+t=new_tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 17 control >>"$HARNESS_LOG" 2>&1
  delay 2

  require_log_after "$NEW_TAB_START_LINE" "dispatching action target=surface action=.new_tab" "new terminal tab action dispatched"
  require_log_after "$NEW_TAB_START_LINE" 'starting command command=`/usr/bin/login`' "new terminal tab started plain login shell"
  if [ -s "$NEW_TAB_COMMAND_LOG" ]; then
    fail "new terminal tab unexpectedly inherited and ran the first-run web wrapper"
  fi
  log "PASS: new terminal tab did not inherit the first-run web wrapper"

  A_TABBED_PRESENT_LINE="$(wait_for_changed_appkit_frame_after "$NEW_TAB_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "browser A geometry adjusted for native tab bar")"
  A_TABBED_PIXELS_LINE="$(wait_for_changed_appkit_pixels_after "$NEW_TAB_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "browser A AppKit pixels adjusted for native tab bar")"
  A_TABBED_FRAME="$(extract_overlay_frame "$A_TABBED_PRESENT_LINE")"
  A_TABBED_FRAME_SIZE="$(extract_frame_size "$A_TABBED_PRESENT_LINE")"
  A_TABBED_FRAME_X="$(extract_frame_x "$A_TABBED_PRESENT_LINE")"
  A_TABBED_FRAME_Y="$(extract_frame_y "$A_TABBED_PRESENT_LINE")"
  A_TABBED_PIXEL="$(extract_appkit_pixel "$A_TABBED_PIXELS_LINE")"
  log "browser_a_tabbed_overlay_frame=$A_TABBED_FRAME"
  log "browser_a_tabbed_overlay_frame_size=$A_TABBED_FRAME_SIZE"
  log "browser_a_tabbed_appkit_pixel=$A_TABBED_PIXEL"

  TAB2_SELECT_START_LINE="$(log_line_count)"
  log "select_tab2_keybind=ctrl+2=goto_tab:2"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 19 control >>"$HARNESS_LOG" 2>&1
  delay 1

  TAB2_SELECTED_LINE="$(wait_for_selected_tab_change_after "$TAB2_SELECT_START_LINE" "$A_SELECTED_TAB_ID" "tab 2 selected")"
  TAB2_SELECTED_TAB_ID="$(extract_selected_tab_id "$TAB2_SELECTED_LINE")"
  [ -n "$TAB2_SELECTED_TAB_ID" ] || fail "could not extract tab 2 selected tab id"
  log "PASS: tab 2 selected"
  log "tab2_selected_tab_id=$TAB2_SELECTED_TAB_ID"

  screencapture -x -o -l"$TAB2_SELECTED_TAB_ID" "$SCREENSHOT_TAB_NEW"
  log "new_tab_screenshot_exit=$?"

  if tail -n +"$((TAB2_SELECT_START_LINE + 1))" "$APP_LOG" |
    grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${A_PANE_ID} .*context_id=${A_CONTEXT_ID} .*visible=true .*selected_tab_id:${TAB2_SELECTED_TAB_ID}" >/dev/null 2>&1; then
    fail "browser A overlay was presented as visible in selected tab 2"
  fi
  log "PASS: browser A was not freshly presented as visible in selected tab 2"

  TAB2_WIN_LINE="$(window_bounds_for "$TAB2_SELECTED_TAB_ID")" || fail "failed to resolve tab 2 window bounds for window id=$TAB2_SELECTED_TAB_ID"
  IFS=$'\t' read -r _TAB2_WID TAB2_WX TAB2_WY TAB2_WW TAB2_WH <<<"$TAB2_WIN_LINE"
  TAB2_A_X="$(awk -v wx="$TAB2_WX" -v frame_x="$A_TABBED_FRAME_X" -v frame_size="$A_TABBED_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
  TAB2_A_Y="$(awk -v wy="$TAB2_WY" -v frame_y="$A_TABBED_FRAME_Y" -v frame_size="$A_TABBED_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"
  click_negative_global_point "$TAB2_A_X" "$TAB2_A_Y" "tab2_former_browser_a_area"
  wait_for_negative_hit_after "$NEGATIVE_HIT_START_LINE" "$A_CONTEXT_ID" "tab 2 former browser A area negative hit-test" allow-absent

  TAB2_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP13_TAB2_TERMINAL\n' >"$NEW_TAB_MARKER_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$NEW_TAB_MARKER_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  delay 1
  require_no_trace_after "$TAB2_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "tab 2 terminal keyboard marker did not reach browser A"

  BROWSER_B_START_LINE="$(log_line_count)"
  BROWSER_B_TRACE_START_LINE="$(trace_line_count)"
  if [ "$SCENARIO" = "same-profile-server-lifecycle" ]; then
    printf '"%s" --browser "%s" --profile default "%s"' "$WEB" "$CHROMIUM" "$URL_B" >"$SECOND_BROWSER_COMMAND"
  else
    printf '"%s" --browser "%s" "%s"' "$WEB" "$CHROMIUM" "$URL_B" >"$SECOND_BROWSER_COMMAND"
  fi
  log "browser_b_command=$(cat "$SECOND_BROWSER_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$SECOND_BROWSER_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1

  if [ "$SCENARIO" = "same-profile-server-lifecycle" ]; then
    wait_for_log_after "$BROWSER_B_START_LINE" "SetOverlay: pane_id=[^ ]+ profile=default browser=${CHROMIUM}" "browser B SetOverlay uses default profile and absolute Chromium path"
    wait_for_log_after "$BROWSER_B_START_LINE" "SetOverlay: reused pending server key=default/${CHROMIUM} pane_count=2 has_fd=true" "browser B reused shared default profile server"
    if tail -n +"$((BROWSER_B_START_LINE + 1))" "$APP_LOG" | grep -E "spawned browser path=${CHROMIUM} pid=[0-9]+ profile=default" >/dev/null 2>&1; then
      fail "browser B spawned a second default profile Chromium process"
    fi
    kill -0 "$SHARED_SPAWN_PID" >/dev/null 2>&1 || fail "shared Chromium pid died while browser B was opening"
    log "PASS: browser B reused shared Chromium pid without a second spawn"
  fi

  B_CA_CONTEXT_LINE="$(wait_for_different_zig_event_after "$BROWSER_B_START_LINE" "ca_context" "$A_PANE_ID" "browser B Zig ca_context")"
  B_PANE_ID="$(extract_pane_id "$B_CA_CONTEXT_LINE")"
  B_BROWSER_TAB_ID="$(extract_browser_tab_id "$B_CA_CONTEXT_LINE")"
  B_CONTEXT_ID="$(extract_context_id "$B_CA_CONTEXT_LINE")"
  [ -n "$B_PANE_ID" ] || fail "could not extract browser B pane id"
  [ -n "$B_BROWSER_TAB_ID" ] || fail "could not extract browser B tab id"
  [ -n "$B_CONTEXT_ID" ] || fail "could not extract browser B context id"
  [ "$B_PANE_ID" != "$A_PANE_ID" ] || fail "browser B reused browser A pane id"
  [ "$B_BROWSER_TAB_ID" != "$A_BROWSER_TAB_ID" ] || fail "browser B reused browser A tab id"
  [ "$B_CONTEXT_ID" != "$A_CONTEXT_ID" ] || fail "browser B reused browser A CA/context id"
  log "browser_b_pane_id=$B_PANE_ID"
  log "browser_b_browser_tab_id=$B_BROWSER_TAB_ID"
  log "browser_b_context_id=$B_CONTEXT_ID"

  B_APPKIT_PRESENT_LINE="$(wait_for_line_after "$BROWSER_B_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${B_PANE_ID} .*context_id=${B_CONTEXT_ID}" "browser B AppKit presentation")"
  B_APPKIT_PIXELS_LINE="$(wait_for_line_after "$BROWSER_B_START_LINE" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${B_PANE_ID} .*context_id=${B_CONTEXT_ID}" "browser B AppKit pixels")"
  B_SELECTED_TAB_ID="$(extract_selected_tab_id "$B_APPKIT_PRESENT_LINE")"
  B_FRAME="$(extract_overlay_frame "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_SIZE="$(extract_frame_size "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_X="$(extract_frame_x "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_Y="$(extract_frame_y "$B_APPKIT_PRESENT_LINE")"
  B_PIXEL="$(extract_appkit_pixel "$B_APPKIT_PIXELS_LINE")"
  [ "$B_SELECTED_TAB_ID" = "$TAB2_SELECTED_TAB_ID" ] || fail "browser B selected tab mismatch: expected=$TAB2_SELECTED_TAB_ID actual=$B_SELECTED_TAB_ID"
  log "browser_b_selected_tab_id=$B_SELECTED_TAB_ID"
  log "browser_b_overlay_frame=$B_FRAME"
  log "browser_b_overlay_frame_size=$B_FRAME_SIZE"
  log "browser_b_appkit_pixel=$B_PIXEL"

  B_PIXEL_WIDTH="${B_PIXEL%x*}"
  B_PIXEL_HEIGHT="${B_PIXEL#*x}"
  require_trace_after "$BROWSER_B_TRACE_START_LINE" "resize tab_id=${B_BROWSER_TAB_ID} pane_id=${B_PANE_ID} pixel_width=${B_PIXEL_WIDTH} pixel_height=${B_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied browser B resize to AppKit pixel size"

  if tail -n +"$((BROWSER_B_START_LINE + 1))" "$APP_LOG" |
    grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${A_PANE_ID} .*context_id=${A_CONTEXT_ID} .*visible=true .*selected_tab_id:${TAB2_SELECTED_TAB_ID}" >/dev/null 2>&1; then
    fail "browser A overlay was presented as visible in tab 2 after browser B opened"
  fi
  log "PASS: browser A stayed hidden after browser B opened in tab 2"

  screencapture -x -o -l"$TAB2_SELECTED_TAB_ID" "$SCREENSHOT_TAB_BROWSER_B"
  log "browser_b_screenshot_exit=$?"

  B_CLICK_X="$(awk -v wx="$TAB2_WX" -v frame_x="$B_FRAME_X" -v frame_size="$B_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
  B_CLICK_Y="$(awk -v wy="$TAB2_WY" -v frame_y="$B_FRAME_Y" -v frame_size="$B_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"
  B_HIT_START_LINE="$(log_line_count)"
  click_global_point "$B_CLICK_X" "$B_CLICK_Y" "browser_b_area"
  B_HIT_LINE="$(wait_for_hit_after "$B_HIT_START_LINE" "$B_CONTEXT_ID" "browser B hit-test")"
  require_text "$B_HIT_LINE" "selected_tab_id:${TAB2_SELECTED_TAB_ID}" "browser B hit-test has tab 2 selected tab id"
  require_text "$B_HIT_LINE" "overlay_frame=${B_FRAME}" "browser B hit-test uses browser B frame"
  require_text "$B_HIT_LINE" "web_point={" "browser B hit-test includes webview-relative point"

  B_MODE_START_LINE="$(log_line_count)"
  B_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "browser_b_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$B_MODE_START_LINE" "ModeChanged: pane_id=${B_PANE_ID} browsing=true" "browser B webtui entered browse mode"
  require_trace_after "$B_MODE_TRACE_START_LINE" "focus-changed tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed browser B focus=true after browse mode"

  B_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP13_BROWSER_B\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$B_KEY_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser B keyboard marker reached browser B"
  require_no_trace_after "$B_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser B keyboard marker did not reach browser A"

  B_CONTROL_START_LINE="$(log_line_count)"
  B_CONTROL_TRACE_START_LINE="$(trace_line_count)"
  log "browser_b_control_key=escape=Mode::Control"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$B_CONTROL_START_LINE" "ModeChanged: pane_id=${B_PANE_ID} browsing=false" "browser B webtui returned to control mode"
  require_trace_after "$B_CONTROL_TRACE_START_LINE" "focus-changed tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed browser B focus=false after control mode"

  if [ "$SCENARIO" = "close-browser-tab" ] || [ "$SCENARIO" = "same-profile-server-lifecycle" ]; then
    CLOSE_TAB_START_LINE="$(log_line_count)"
    CLOSE_TAB_TRACE_START_LINE="$(trace_line_count)"
    log "close_tab_keybind=ctrl+w=close_tab"
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 13 control >>"$HARNESS_LOG" 2>&1
    delay 1

    CLOSE_SELECTED_LINE="$(wait_for_selected_tab_change_after "$CLOSE_TAB_START_LINE" "$TAB2_SELECTED_TAB_ID" "browser B native tab closed/selection changed")"
    CLOSE_SELECTED_TAB_ID="$(extract_selected_tab_id "$CLOSE_SELECTED_LINE")"
    [ "$CLOSE_SELECTED_TAB_ID" = "$A_SELECTED_TAB_ID" ] || fail "closing browser B tab selected unexpected tab: expected=$A_SELECTED_TAB_ID actual=$CLOSE_SELECTED_TAB_ID"
    log "PASS: closing browser B tab selected browser A tab"
    log "close_selected_tab_id=$CLOSE_SELECTED_TAB_ID"

    A_AFTER_CLOSE_PRESENT_LINE="$(wait_for_changed_appkit_frame_after "$CLOSE_TAB_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_TABBED_FRAME" "browser A geometry restored after browser B tab close")"
    A_AFTER_CLOSE_PIXELS_LINE="$(wait_for_changed_appkit_pixels_after "$CLOSE_TAB_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_TABBED_PIXEL" "browser A AppKit pixels restored after browser B tab close")"
    A_AFTER_CLOSE_FRAME="$(extract_overlay_frame "$A_AFTER_CLOSE_PRESENT_LINE")"
    A_AFTER_CLOSE_FRAME_SIZE="$(extract_frame_size "$A_AFTER_CLOSE_PRESENT_LINE")"
    A_AFTER_CLOSE_FRAME_X="$(extract_frame_x "$A_AFTER_CLOSE_PRESENT_LINE")"
    A_AFTER_CLOSE_FRAME_Y="$(extract_frame_y "$A_AFTER_CLOSE_PRESENT_LINE")"
    A_AFTER_CLOSE_PIXEL="$(extract_appkit_pixel "$A_AFTER_CLOSE_PIXELS_LINE")"
    A_AFTER_CLOSE_PIXEL_WIDTH="${A_AFTER_CLOSE_PIXEL%x*}"
    A_AFTER_CLOSE_PIXEL_HEIGHT="${A_AFTER_CLOSE_PIXEL#*x}"
    log "browser_a_after_close_overlay_frame=$A_AFTER_CLOSE_FRAME"
    log "browser_a_after_close_overlay_frame_size=$A_AFTER_CLOSE_FRAME_SIZE"
    log "browser_a_after_close_appkit_pixel=$A_AFTER_CLOSE_PIXEL"
    require_trace_after "$CLOSE_TAB_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${A_AFTER_CLOSE_PIXEL_WIDTH} pixel_height=${A_AFTER_CLOSE_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium resized browser A after browser B tab close"

    CLEAR_OVERLAY_SEEN=""
    for _ in $(seq 1 30); do
      if tail -n +"$((CLOSE_TAB_START_LINE + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=zig event=clear_overlay_call .*pane_id:${B_PANE_ID}" >/dev/null 2>&1; then
        CLEAR_OVERLAY_SEEN="1"
        break
      fi
      if tail -n +"$((CLOSE_TAB_TRACE_START_LINE + 1))" "$CHROMIUM_TRACE" | grep -F "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" >/dev/null 2>&1; then
        fail "Control-W was forwarded to browser B input before close_tab cleanup"
      fi
      delay 1
    done
    [ -n "$CLEAR_OVERLAY_SEEN" ] || fail "timed out waiting for Zig records browser B clear_overlay_call after tab close"
    log "PASS: Zig records browser B clear_overlay_call after tab close"

    wait_for_log_after "$CLOSE_TAB_START_LINE" "TermSurf geometry layer=bridge event=clear_request .*pane_id:${B_PANE_ID}" "Swift bridge records browser B clear_request after tab close"

    CLEAR_RESULT=""
    for _ in $(seq 1 30); do
      if tail -n +"$((CLOSE_TAB_START_LINE + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=bridge event=clear_target_found .*pane_id:${B_PANE_ID}" >/dev/null 2>&1 &&
        tail -n +"$((CLOSE_TAB_START_LINE + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=clear .*pane_id:${B_PANE_ID}" >/dev/null 2>&1; then
        CLEAR_RESULT="target-found"
        break
      fi
      if tail -n +"$((CLOSE_TAB_START_LINE + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=bridge event=clear_rejected .*pane_id:${B_PANE_ID} .*note=no-surface" >/dev/null 2>&1; then
        CLEAR_RESULT="surface-already-gone"
        break
      fi
      delay 1
    done
    [ -n "$CLEAR_RESULT" ] || fail "missing AppKit clear or bridge no-surface cleanup evidence after browser B tab close"
    log "PASS: observed browser B tab-close clear result clear_result=$CLEAR_RESULT"

    require_log_after "$CLOSE_TAB_START_LINE" "CloseTab: pane_id=${B_PANE_ID} tab_id=${B_BROWSER_TAB_ID}" "Zig records CloseTab for browser B after tab close"
    require_trace_after "$CLOSE_TAB_TRACE_START_LINE" "close-tab tab_id=${B_BROWSER_TAB_ID} pane_id=${B_PANE_ID} result=destroying ffi=ts_destroy_web_contents" "Chromium received CloseTab and destroyed browser B"
    require_trace_after "$CLOSE_TAB_TRACE_START_LINE" "close-tab tab_id=${B_BROWSER_TAB_ID} result=removed" "Chromium removed closed browser B tab"

    SELECT_CLOSED_START_LINE="$(log_line_count)"
    log "select_closed_tab_keybind=ctrl+2=goto_tab:2"
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 19 control >>"$HARNESS_LOG" 2>&1
    delay 1
    if tail -n +"$((SELECT_CLOSED_START_LINE + 1))" "$APP_LOG" |
      grep -E "TermSurf geometry layer=appkit event=.*selected_tab_id:${TAB2_SELECTED_TAB_ID}" >/dev/null 2>&1; then
      fail "closed browser B native tab was selectable after close"
    fi
    log "PASS: closed browser B native tab was not selectable by ctrl+2"

    require_no_trace_after "$CLOSE_TAB_TRACE_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "close-tab and closed-tab probes did not reach browser B input"

    TAB1_WIN_LINE="$(window_bounds_for "$A_SELECTED_TAB_ID")" || fail "failed to resolve browser A window bounds after tab close for window id=$A_SELECTED_TAB_ID"
    IFS=$'\t' read -r _TAB1_WID TAB1_WX TAB1_WY TAB1_WW TAB1_WH <<<"$TAB1_WIN_LINE"
    A_CLICK_X="$(awk -v wx="$TAB1_WX" -v frame_x="$A_AFTER_CLOSE_FRAME_X" -v frame_size="$A_AFTER_CLOSE_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
    A_CLICK_Y="$(awk -v wy="$TAB1_WY" -v frame_y="$A_AFTER_CLOSE_FRAME_Y" -v frame_size="$A_AFTER_CLOSE_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"
    A_AFTER_CLOSE_HIT_START_LINE="$(log_line_count)"
    click_global_point "$A_CLICK_X" "$A_CLICK_Y" "browser_a_after_tab_close_area"
    A_AFTER_CLOSE_HIT_LINE="$(wait_for_hit_after "$A_AFTER_CLOSE_HIT_START_LINE" "$A_CONTEXT_ID" "browser A after tab-close hit-test")"
    require_text "$A_AFTER_CLOSE_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "browser A after tab-close hit-test has tab 1 selected tab id"
    require_text "$A_AFTER_CLOSE_HIT_LINE" "overlay_frame=${A_AFTER_CLOSE_FRAME}" "browser A after tab-close hit-test uses browser A post-close frame"
    require_text "$A_AFTER_CLOSE_HIT_LINE" "web_point={" "browser A after tab-close hit-test includes webview-relative point"

    A_AFTER_CLOSE_MODE_START_LINE="$(log_line_count)"
    A_AFTER_CLOSE_MODE_TRACE_START_LINE="$(trace_line_count)"
    log "browser_a_after_tab_close_mode_key=enter=Mode::Browse"
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
    wait_for_log_after "$A_AFTER_CLOSE_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "browser A entered browse mode after browser B tab close"
    require_trace_after "$A_AFTER_CLOSE_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed browser A focus=true after browser B tab close"

    A_AFTER_CLOSE_KEY_START_LINE="$(trace_line_count)"
    printf 'ISSUE809_EXP14_BROWSER_A_AFTER_CLOSE\n' >"$BROWSER_FOCUS_COMMAND"
    swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
    require_trace_after "$A_AFTER_CLOSE_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser A keyboard marker reached browser A after browser B tab close"
    require_no_trace_after "$A_AFTER_CLOSE_KEY_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser A keyboard marker did not reach closed browser B"
    if [ "$SCENARIO" = "same-profile-server-lifecycle" ]; then
      leave_browser_browse "browser_a_after_browser_b_close" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
    fi

    screencapture -x -o -l"$A_SELECTED_TAB_ID" "$SCREENSHOT_TAB_AFTER_CLOSE"
    log "after_close_screenshot_exit=$?"

    [ "$NEW_TAB_TRACE_START_LINE" -lt "$BROWSER_B_TRACE_START_LINE" ] || fail "trace boundaries for browser B open were not monotonic"
    [ "$BROWSER_B_TRACE_START_LINE" -lt "$CLOSE_TAB_TRACE_START_LINE" ] || fail "trace boundaries for browser B close were not monotonic"

    if [ "$SCENARIO" = "same-profile-server-lifecycle" ]; then
      kill -0 "$SHARED_SPAWN_PID" >/dev/null 2>&1 || fail "shared Chromium pid died after browser B close while browser A remained alive"
      if tail -n +"$((CLOSE_TAB_START_LINE + 1))" "$APP_LOG" | grep -E "spawned browser path=${CHROMIUM} pid=[0-9]+ profile=default" >/dev/null 2>&1; then
        fail "closing browser B caused an unexpected default profile Chromium respawn"
      fi
      log "PASS: shared Chromium pid survived browser B close without respawn"

      C_TAB_START_LINE="$(log_line_count)"
      C_TAB_TRACE_START_LINE="$(trace_line_count)"
      log "browser_c_new_tab_keybind=ctrl+t=new_tab"
      swift "$ROOT/scripts/ghostty-app/inject.swift" key 17 control >>"$HARNESS_LOG" 2>&1
      delay 1
      require_log_after "$C_TAB_START_LINE" "dispatching action target=surface action=.new_tab" "browser C native tab action dispatched"
      require_log_after "$C_TAB_START_LINE" 'starting command command=`/usr/bin/login`' "browser C native tab started login shell"
      if [ -s "$NEW_TAB_COMMAND_LOG" ]; then
        fail "browser C native tab inherited the first-run web wrapper"
      fi
      log "PASS: browser C native tab did not inherit first-run web wrapper"

      log "browser_c_select_tab2_keybind=ctrl+2=goto_tab:2"
      swift "$ROOT/scripts/ghostty-app/inject.swift" key 19 control >>"$HARNESS_LOG" 2>&1
      delay 1
      C_SELECTED_LINE="$(wait_for_selected_tab_change_after "$C_TAB_START_LINE" "$A_SELECTED_TAB_ID" "browser C tab selected")"
      C_SELECTED_TAB_ID="$(extract_selected_tab_id "$C_SELECTED_LINE")"
      [ -n "$C_SELECTED_TAB_ID" ] || fail "failed to extract browser C selected tab id"
      [ "$C_SELECTED_TAB_ID" != "$A_SELECTED_TAB_ID" ] || fail "browser C reused browser A native tab id"
      [ "$C_SELECTED_TAB_ID" != "$TAB2_SELECTED_TAB_ID" ] || fail "browser C reused closed browser B native tab id"
      log "browser_c_selected_tab_id=$C_SELECTED_TAB_ID"

      printf '"%s" --browser "%s" --profile default "%s"' "$WEB" "$CHROMIUM" "$URL_C" >"$THIRD_BROWSER_COMMAND"
      log "browser_c_command=$(cat "$THIRD_BROWSER_COMMAND")"
      swift "$ROOT/scripts/ghostty-app/inject.swift" type "$THIRD_BROWSER_COMMAND" >>"$HARNESS_LOG" 2>&1
      swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1

      C_SET_LINE="$(wait_for_line_after "$C_TAB_START_LINE" "SetOverlay: pane_id=[^ ]+ profile=default browser=${CHROMIUM} url=${URL_C}" "browser C SetOverlay uses default profile and absolute Chromium path")"
      C_SET_PANE_ID="$(printf '%s\n' "$C_SET_LINE" | sed -E 's/.*SetOverlay: pane_id=([^ ]+) .*/\1/')"
      [ -n "$C_SET_PANE_ID" ] || fail "failed to extract browser C SetOverlay pane id"
      [ "$C_SET_PANE_ID" != "$A_PANE_ID" ] || fail "browser C SetOverlay matched browser A pane id"
      [ "$C_SET_PANE_ID" != "$B_PANE_ID" ] || fail "browser C SetOverlay matched closed browser B pane id"
      wait_for_log_after "$C_TAB_START_LINE" "SetOverlay: reused pending server key=default/${CHROMIUM} pane_count=2 has_fd=true" "browser C reused shared default profile server"
      if tail -n +"$((C_TAB_START_LINE + 1))" "$APP_LOG" | grep -E "spawned browser path=${CHROMIUM} pid=[0-9]+ profile=default" >/dev/null 2>&1; then
        fail "browser C spawned a second default profile Chromium process"
      fi
      kill -0 "$SHARED_SPAWN_PID" >/dev/null 2>&1 || fail "shared Chromium pid died while browser C was opening"

      C_CA_CONTEXT_LINE="$(wait_for_different_zig_event_after "$C_TAB_START_LINE" "ca_context" "$A_PANE_ID" "browser C Zig ca_context")"
      C_PANE_ID="$(extract_pane_id "$C_CA_CONTEXT_LINE")"
      C_BROWSER_TAB_ID="$(extract_browser_tab_id "$C_CA_CONTEXT_LINE")"
      C_CONTEXT_ID="$(extract_context_id "$C_CA_CONTEXT_LINE")"
      [ -n "$C_PANE_ID" ] || fail "could not extract browser C pane id"
      [ -n "$C_BROWSER_TAB_ID" ] || fail "could not extract browser C tab id"
      [ -n "$C_CONTEXT_ID" ] || fail "could not extract browser C context id"
      [ "$C_PANE_ID" != "$A_PANE_ID" ] || fail "browser C reused browser A pane id"
      [ "$C_PANE_ID" != "$B_PANE_ID" ] || fail "browser C reused closed browser B pane id"
      [ "$C_BROWSER_TAB_ID" != "$A_BROWSER_TAB_ID" ] || fail "browser C reused browser A tab id"
      [ "$C_BROWSER_TAB_ID" != "$B_BROWSER_TAB_ID" ] || fail "browser C reused closed browser B tab id"
      [ "$C_CONTEXT_ID" != "$A_CONTEXT_ID" ] || fail "browser C reused browser A CA/context id"
      [ "$C_CONTEXT_ID" != "$B_CONTEXT_ID" ] || fail "browser C reused closed browser B CA/context id"
      log "browser_c_pane_id=$C_PANE_ID"
      log "browser_c_browser_tab_id=$C_BROWSER_TAB_ID"
      log "browser_c_context_id=$C_CONTEXT_ID"

      C_APPKIT_PRESENT_LINE="$(wait_for_line_after "$C_TAB_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${C_PANE_ID} .*context_id=${C_CONTEXT_ID}" "browser C AppKit presentation")"
      C_SELECTED_PRESENTED_TAB_ID="$(extract_selected_tab_id "$C_APPKIT_PRESENT_LINE")"
      C_FRAME="$(extract_overlay_frame "$C_APPKIT_PRESENT_LINE")"
      C_FRAME_SIZE="$(extract_frame_size "$C_APPKIT_PRESENT_LINE")"
      C_FRAME_X="$(extract_frame_x "$C_APPKIT_PRESENT_LINE")"
      C_FRAME_Y="$(extract_frame_y "$C_APPKIT_PRESENT_LINE")"
      [ "$C_SELECTED_PRESENTED_TAB_ID" = "$C_SELECTED_TAB_ID" ] || fail "browser C selected tab mismatch: expected=$C_SELECTED_TAB_ID actual=$C_SELECTED_PRESENTED_TAB_ID"

      TABC_WIN_LINE="$(window_bounds_for "$C_SELECTED_TAB_ID")" || fail "failed to resolve browser C window bounds for window id=$C_SELECTED_TAB_ID"
      IFS=$'\t' read -r _TABC_WID TABC_WX TABC_WY TABC_WW TABC_WH <<<"$TABC_WIN_LINE"
      C_CLICK_X="$(awk -v wx="$TABC_WX" -v frame_x="$C_FRAME_X" -v frame_size="$C_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
      C_CLICK_Y="$(awk -v wy="$TABC_WY" -v frame_y="$C_FRAME_Y" -v frame_size="$C_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"
      C_HIT_START_LINE="$(log_line_count)"
      click_global_point "$C_CLICK_X" "$C_CLICK_Y" "browser_c_area"
      C_HIT_LINE="$(wait_for_hit_after "$C_HIT_START_LINE" "$C_CONTEXT_ID" "browser C hit-test")"
      require_text "$C_HIT_LINE" "selected_tab_id:${C_SELECTED_TAB_ID}" "browser C hit-test has browser C selected tab id"
      require_text "$C_HIT_LINE" "overlay_frame=${C_FRAME}" "browser C hit-test uses browser C frame"

      enter_browser_browse "browser_c" "$C_PANE_ID" "$C_BROWSER_TAB_ID"
      C_KEY_START_LINE="$(trace_line_count)"
      printf 'ISSUE818_EXP3_BROWSER_C\n' >"$BROWSER_FOCUS_COMMAND"
      swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
      require_trace_after "$C_KEY_START_LINE" "key-event tab=${C_BROWSER_TAB_ID} pane=${C_PANE_ID}" "browser C keyboard marker reached browser C"
      require_no_trace_after "$C_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser C keyboard marker did not reach browser A"
      require_no_trace_after "$C_KEY_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser C keyboard marker did not reach closed browser B"
      leave_browser_browse "browser_c" "$C_PANE_ID" "$C_BROWSER_TAB_ID"

      C_CLOSE_START_LINE="$(log_line_count)"
      C_CLOSE_TRACE_START_LINE="$(trace_line_count)"
      log "browser_c_close_keybind=ctrl+w=close_tab"
      swift "$ROOT/scripts/ghostty-app/inject.swift" key 13 control >>"$HARNESS_LOG" 2>&1
      delay 1
      require_log_after "$C_CLOSE_START_LINE" "CloseTab: pane_id=${C_PANE_ID} tab_id=${C_BROWSER_TAB_ID}" "Zig records CloseTab for browser C after tab close"
      require_trace_after "$C_CLOSE_TRACE_START_LINE" "close-tab tab_id=${C_BROWSER_TAB_ID} pane_id=${C_PANE_ID} result=destroying ffi=ts_destroy_web_contents" "Chromium received CloseTab and destroyed browser C"
      require_trace_after "$C_CLOSE_TRACE_START_LINE" "close-tab tab_id=${C_BROWSER_TAB_ID} result=removed" "Chromium removed closed browser C tab"
      kill -0 "$SHARED_SPAWN_PID" >/dev/null 2>&1 || fail "shared Chromium pid died after browser C close while browser A remained alive"
      log "PASS: shared Chromium pid survived browser C close"

      require_log_after "$C_CLOSE_START_LINE" "Pane focus changed: pane_id=${A_PANE_ID} focused=true" "browser A pane focused after browser C close"
      TAB1_AFTER_C_WIN_LINE="$(window_bounds_for "$A_SELECTED_TAB_ID")" || fail "failed to resolve browser A window bounds after browser C close for window id=$A_SELECTED_TAB_ID"
      IFS=$'\t' read -r _TAB1_AFTER_C_WID TAB1_AFTER_C_WX TAB1_AFTER_C_WY TAB1_AFTER_C_WW TAB1_AFTER_C_WH <<<"$TAB1_AFTER_C_WIN_LINE"
      A_AFTER_C_X="$(awk -v wx="$TAB1_AFTER_C_WX" -v frame_x="$A_AFTER_CLOSE_FRAME_X" -v frame_size="$A_AFTER_CLOSE_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
      A_AFTER_C_Y="$(awk -v wy="$TAB1_AFTER_C_WY" -v frame_y="$A_AFTER_CLOSE_FRAME_Y" -v frame_size="$A_AFTER_CLOSE_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"
      A_AFTER_C_HIT_START_LINE="$(log_line_count)"
      click_global_point "$A_AFTER_C_X" "$A_AFTER_C_Y" "browser_a_after_browser_c_close_area"
      A_AFTER_C_HIT_LINE="$(wait_for_hit_after "$A_AFTER_C_HIT_START_LINE" "$A_CONTEXT_ID" "browser A after browser C close hit-test")"
      require_text "$A_AFTER_C_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "browser A after browser C close hit-test has tab 1 selected tab id"
      require_text "$A_AFTER_C_HIT_LINE" "overlay_frame=${A_AFTER_CLOSE_FRAME}" "browser A after browser C close hit-test uses browser A restored frame"
      enter_browser_browse "browser_a_after_browser_c_close" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
      A_AFTER_C_KEY_START_LINE="$(trace_line_count)"
      printf 'ISSUE818_EXP3_BROWSER_A_AFTER_C\n' >"$BROWSER_FOCUS_COMMAND"
      swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
      require_trace_after "$A_AFTER_C_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser A keyboard marker reached browser A after browser C close"
      require_no_trace_after "$A_AFTER_C_KEY_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser A keyboard marker did not reach closed browser B after browser C close"
      require_no_trace_after "$A_AFTER_C_KEY_START_LINE" "key-event tab=${C_BROWSER_TAB_ID} pane=${C_PANE_ID}" "browser A keyboard marker did not reach closed browser C"
      leave_browser_browse "browser_a_after_browser_c_close" "$A_PANE_ID" "$A_BROWSER_TAB_ID"

      A_FINAL_START_LINE="$(log_line_count)"
      log "browser_a_final_close_keybind=ctrl+w=close_tab"
      swift "$ROOT/scripts/ghostty-app/inject.swift" key 13 control >>"$HARNESS_LOG" 2>&1
      delay 2
      require_log_after "$A_FINAL_START_LINE" "CloseTab: pane_id=${A_PANE_ID} tab_id=${A_BROWSER_TAB_ID}" "Zig records CloseTab for final browser A close"
      for _ in $(seq 1 15); do
        if ! kill -0 "$SHARED_SPAWN_PID" >/dev/null 2>&1; then
          log "PASS: shared Chromium pid exited after final browser close"
          SHARED_PID_EXITED="1"
          break
        fi
        delay 1
      done
      [ "${SHARED_PID_EXITED:-}" = "1" ] || fail "shared Chromium pid remained alive after final browser close"
    fi
  fi

  if [ "$SCENARIO" = "open-browser-in-new-tab" ] || [ "$SCENARIO" = "gui-active-multi-tab" ]; then
  SWITCH_A_START_LINE="$(log_line_count)"
  SWITCH_A_TRACE_START_LINE="$(trace_line_count)"
  log "switch_to_browser_a_keybind=ctrl+p=previous_tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 35 control >>"$HARNESS_LOG" 2>&1
  delay 1
  wait_for_log_after "$SWITCH_A_START_LINE" "Pane focus changed: pane_id=${A_PANE_ID} focused=true" "browser A pane focused again"
  require_no_different_appkit_frame_after "$SWITCH_A_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_TABBED_FRAME" "browser A kept tab-bar-adjusted AppKit frame after tab restore"
  require_no_different_appkit_pixels_after "$SWITCH_A_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_TABBED_PIXEL" "browser A kept tab-bar-adjusted AppKit pixels after tab restore"

  TAB1_WIN_LINE="$(window_bounds_for "$A_SELECTED_TAB_ID")" || fail "failed to resolve tab 1 window bounds for window id=$A_SELECTED_TAB_ID"
  IFS=$'\t' read -r _TAB1_WID TAB1_WX TAB1_WY TAB1_WW TAB1_WH <<<"$TAB1_WIN_LINE"
  A_CLICK_X="$(awk -v wx="$TAB1_WX" -v frame_x="$A_TABBED_FRAME_X" -v frame_size="$A_TABBED_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
  A_CLICK_Y="$(awk -v wy="$TAB1_WY" -v frame_y="$A_TABBED_FRAME_Y" -v frame_size="$A_TABBED_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"
  A_RESTORE_HIT_START_LINE="$(log_line_count)"
  click_global_point "$A_CLICK_X" "$A_CLICK_Y" "browser_a_restored_area"
  A_RESTORE_HIT_LINE="$(wait_for_hit_after "$A_RESTORE_HIT_START_LINE" "$A_CONTEXT_ID" "browser A restored hit-test")"
  require_text "$A_RESTORE_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "browser A restored hit-test has tab 1 selected tab id"
  require_text "$A_RESTORE_HIT_LINE" "overlay_frame=${A_TABBED_FRAME}" "browser A restored hit-test uses browser A frame"
  require_text "$A_RESTORE_HIT_LINE" "web_point={" "browser A restored hit-test includes webview-relative point"

  A_MODE_START_LINE="$(log_line_count)"
  A_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "browser_a_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$A_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "browser A webtui entered browse mode"
  require_trace_after "$A_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed browser A focus=true after browse mode"

  A_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP13_BROWSER_A\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$A_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser A keyboard marker reached browser A"
  require_no_trace_after "$A_KEY_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser A keyboard marker did not reach browser B"
  screencapture -x -o -l"$A_SELECTED_TAB_ID" "$SCREENSHOT_TAB_BROWSER_A_RESTORED"
  log "browser_a_restored_screenshot_exit=$?"

  A_CONTROL_START_LINE="$(log_line_count)"
  A_CONTROL_TRACE_START_LINE="$(trace_line_count)"
  log "browser_a_control_key=escape=Mode::Control"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$A_CONTROL_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=false" "browser A webtui returned to control mode"
  require_trace_after "$A_CONTROL_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed browser A focus=false after control mode"

  SWITCH_B_START_LINE="$(log_line_count)"
  SWITCH_B_TRACE_START_LINE="$(trace_line_count)"
  log "switch_to_browser_b_keybind=ctrl+n=next_tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 45 control >>"$HARNESS_LOG" 2>&1
  delay 1
  wait_for_log_after "$SWITCH_B_START_LINE" "Pane focus changed: pane_id=${B_PANE_ID} focused=true" "browser B pane focused again"
  require_no_different_appkit_frame_after "$SWITCH_B_START_LINE" "$B_PANE_ID" "$B_CONTEXT_ID" "$B_FRAME" "browser B kept AppKit frame after tab restore"
  require_no_different_appkit_pixels_after "$SWITCH_B_START_LINE" "$B_PANE_ID" "$B_CONTEXT_ID" "$B_PIXEL" "browser B kept AppKit pixels after tab restore"

  TAB2_WIN_LINE="$(window_bounds_for "$TAB2_SELECTED_TAB_ID")" || fail "failed to resolve restored tab 2 window bounds for window id=$TAB2_SELECTED_TAB_ID"
  IFS=$'\t' read -r _TAB2_WID TAB2_WX TAB2_WY TAB2_WW TAB2_WH <<<"$TAB2_WIN_LINE"
  B_CLICK_X="$(awk -v wx="$TAB2_WX" -v frame_x="$B_FRAME_X" -v frame_size="$B_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wx + frame_x + (parts[1] / 2) + 0.5) }')"
  B_CLICK_Y="$(awk -v wy="$TAB2_WY" -v frame_y="$B_FRAME_Y" -v frame_size="$B_FRAME_SIZE" 'BEGIN { split(frame_size, parts, "x"); print int(wy + frame_y + (parts[2] / 2) + 0.5) }')"
  B_RESTORE_HIT_START_LINE="$(log_line_count)"
  click_global_point "$B_CLICK_X" "$B_CLICK_Y" "browser_b_restored_area"
  B_RESTORE_HIT_LINE="$(wait_for_hit_after "$B_RESTORE_HIT_START_LINE" "$B_CONTEXT_ID" "browser B restored hit-test")"
  require_text "$B_RESTORE_HIT_LINE" "selected_tab_id:${TAB2_SELECTED_TAB_ID}" "browser B restored hit-test has tab 2 selected tab id"
  require_text "$B_RESTORE_HIT_LINE" "overlay_frame=${B_FRAME}" "browser B restored hit-test uses browser B frame"
  require_text "$B_RESTORE_HIT_LINE" "web_point={" "browser B restored hit-test includes webview-relative point"

  B_RESTORE_MODE_START_LINE="$(log_line_count)"
  B_RESTORE_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "browser_b_restored_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$B_RESTORE_MODE_START_LINE" "ModeChanged: pane_id=${B_PANE_ID} browsing=true" "browser B restored webtui entered browse mode"
  require_trace_after "$B_RESTORE_MODE_TRACE_START_LINE" "focus-changed tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed browser B restored focus=true after browse mode"

  B_RESTORE_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP13_BROWSER_B_RESTORED\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$B_RESTORE_KEY_START_LINE" "key-event tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser B restored keyboard marker reached browser B"
  require_no_trace_after "$B_RESTORE_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser B restored keyboard marker did not reach browser A"
  screencapture -x -o -l"$TAB2_SELECTED_TAB_ID" "$SCREENSHOT_TAB_BROWSER_B_RESTORED"
  log "browser_b_restored_screenshot_exit=$?"

  if [ "$SCENARIO" = "gui-active-multi-tab" ]; then
    assert_gui_active_cycle "gui_active_browser_b" "$B_BROWSER_TAB_ID" "$B_PANE_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID"
    type_marker_require_only "gui-active browser B after activation" "ISSUE812_EXP2_BROWSER_B_ACTIVE" "$B_BROWSER_TAB_ID" "$B_PANE_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID"
    leave_browser_browse "gui_active_browser_b" "$B_PANE_ID" "$B_BROWSER_TAB_ID"

    GUI_ACTIVE_SWITCH_A_START_LINE="$(log_line_count)"
    log "gui_active_switch_to_browser_a_keybind=ctrl+p=previous_tab"
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 35 control >>"$HARNESS_LOG" 2>&1
    delay 1
    wait_for_log_after "$GUI_ACTIVE_SWITCH_A_START_LINE" "Pane focus changed: pane_id=${A_PANE_ID} focused=true" "gui-active browser A pane focused"
    require_no_different_appkit_frame_after "$GUI_ACTIVE_SWITCH_A_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_TABBED_FRAME" "gui-active browser A kept tab-bar-adjusted AppKit frame"
    require_no_different_appkit_pixels_after "$GUI_ACTIVE_SWITCH_A_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_TABBED_PIXEL" "gui-active browser A kept tab-bar-adjusted AppKit pixels"

    TAB1_WIN_LINE="$(window_bounds_for "$A_SELECTED_TAB_ID")" || fail "failed to resolve gui-active browser A window bounds for window id=$A_SELECTED_TAB_ID"
    A_GUI_CLICK_START_LINE="$(log_line_count)"
    click_browser_frame_center "$TAB1_WIN_LINE" "$A_TABBED_FRAME_X" "$A_TABBED_FRAME_Y" "$A_TABBED_FRAME_SIZE" "gui_active_browser_a_area"
    A_GUI_HIT_LINE="$(wait_for_hit_after "$A_GUI_CLICK_START_LINE" "$A_CONTEXT_ID" "gui-active browser A hit-test")"
    require_text "$A_GUI_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "gui-active browser A hit-test has tab 1 selected tab id"
    require_text "$A_GUI_HIT_LINE" "overlay_frame=${A_TABBED_FRAME}" "gui-active browser A hit-test uses browser A frame"
    enter_browser_browse "gui_active_browser_a" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
    assert_gui_active_cycle "gui_active_browser_a" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$B_BROWSER_TAB_ID" "$B_PANE_ID"
    type_marker_require_only "gui-active browser A after activation" "ISSUE812_EXP2_BROWSER_A_ACTIVE" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$B_BROWSER_TAB_ID" "$B_PANE_ID"
    leave_browser_browse "gui_active_browser_a" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
  fi

  [ "$BROWSER_B_TRACE_START_LINE" -lt "$SWITCH_B_TRACE_START_LINE" ] || fail "trace boundaries for browser B restore were not monotonic"
  [ "$NEW_TAB_TRACE_START_LINE" -lt "$BROWSER_B_TRACE_START_LINE" ] || fail "trace boundaries for browser B open were not monotonic"
  fi
fi

if [ "$SCENARIO" = "keyboard-after-tab-window-switch" ]; then
  A_WINDOW_ID="$WID"
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  [ -n "$A_SELECTED_TAB_ID" ] || fail "could not extract browser A selected tab id"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_FRAME_X="$OVERLAY_FRAME_X"
  A_FRAME_Y="$OVERLAY_FRAME_Y"
  A_PIXEL="$APPKIT_PIXEL"
  log "keyboard_browser_a_window_id=$A_WINDOW_ID"
  log "keyboard_browser_a_selected_tab_id=$A_SELECTED_TAB_ID"
  log "keyboard_browser_a_pane_id=$A_PANE_ID"
  log "keyboard_browser_a_browser_tab_id=$A_BROWSER_TAB_ID"
  log "keyboard_browser_a_context_id=$A_CONTEXT_ID"

  NEW_TAB_START_LINE="$(log_line_count)"
  NEW_TAB_TRACE_START_LINE="$(trace_line_count)"
  log "keyboard_new_tab_keybind=ctrl+t=new_tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 17 control >>"$HARNESS_LOG" 2>&1
  delay 2

  require_log_after "$NEW_TAB_START_LINE" "dispatching action target=surface action=.new_tab" "keyboard switch new terminal tab action dispatched"
  require_log_after "$NEW_TAB_START_LINE" 'starting command command=`/usr/bin/login`' "keyboard switch new terminal tab started plain login shell"
  if [ -s "$NEW_TAB_COMMAND_LOG" ]; then
    fail "keyboard switch new terminal tab unexpectedly inherited and ran the first-run web wrapper"
  fi
  log "PASS: keyboard switch new terminal tab did not inherit the first-run web wrapper"

  A_TABBED_PRESENT_LINE="$(wait_for_changed_appkit_frame_after "$NEW_TAB_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "keyboard switch browser A geometry adjusted for native tab bar")"
  A_TABBED_PIXELS_LINE="$(wait_for_changed_appkit_pixels_after "$NEW_TAB_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "keyboard switch browser A AppKit pixels adjusted for native tab bar")"
  A_TABBED_FRAME="$(extract_overlay_frame "$A_TABBED_PRESENT_LINE")"
  A_TABBED_FRAME_SIZE="$(extract_frame_size "$A_TABBED_PRESENT_LINE")"
  A_TABBED_FRAME_X="$(extract_frame_x "$A_TABBED_PRESENT_LINE")"
  A_TABBED_FRAME_Y="$(extract_frame_y "$A_TABBED_PRESENT_LINE")"
  A_TABBED_PIXEL="$(extract_appkit_pixel "$A_TABBED_PIXELS_LINE")"
  log "keyboard_browser_a_tabbed_overlay_frame=$A_TABBED_FRAME"
  log "keyboard_browser_a_tabbed_overlay_frame_size=$A_TABBED_FRAME_SIZE"
  log "keyboard_browser_a_tabbed_appkit_pixel=$A_TABBED_PIXEL"

  TAB2_SELECT_START_LINE="$(log_line_count)"
  log "keyboard_select_tab2_keybind=ctrl+2=goto_tab:2"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 19 control >>"$HARNESS_LOG" 2>&1
  delay 1

  TAB2_SELECTED_LINE="$(wait_for_selected_tab_change_after "$TAB2_SELECT_START_LINE" "$A_SELECTED_TAB_ID" "keyboard switch plain terminal tab selected")"
  TAB2_SELECTED_TAB_ID="$(extract_selected_tab_id "$TAB2_SELECTED_LINE")"
  [ -n "$TAB2_SELECTED_TAB_ID" ] || fail "could not extract selected tab id for keyboard switch tab 2"
  log "keyboard_tab2_selected_tab_id=$TAB2_SELECTED_TAB_ID"

  screencapture -x -o -l"$TAB2_SELECTED_TAB_ID" "$SCREENSHOT_TAB_NEW"
  log "keyboard_new_tab_screenshot_exit=$?"

  PLAIN_TAB_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP26_PLAIN_TERMINAL_TAB\n' >"$NEW_TAB_MARKER_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$NEW_TAB_MARKER_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  delay 1
  require_no_trace_after "$PLAIN_TAB_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "plain terminal tab keyboard marker did not reach browser A"

  SWITCH_A_FROM_PLAIN_START_LINE="$(log_line_count)"
  log "keyboard_switch_a_from_plain_keybind=ctrl+p=previous_tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 35 control >>"$HARNESS_LOG" 2>&1
  delay 1
  wait_for_log_after "$SWITCH_A_FROM_PLAIN_START_LINE" "Pane focus changed: pane_id=${A_PANE_ID} focused=true" "keyboard switch browser A pane focused after returning from plain tab"
  require_no_different_appkit_frame_after "$SWITCH_A_FROM_PLAIN_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_TABBED_FRAME" "keyboard switch browser A kept tab-bar-adjusted AppKit frame after plain-tab return"
  require_no_different_appkit_pixels_after "$SWITCH_A_FROM_PLAIN_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_TABBED_PIXEL" "keyboard switch browser A kept tab-bar-adjusted AppKit pixels after plain-tab return"

  TAB1_WIN_LINE="$(window_bounds_for "$A_SELECTED_TAB_ID")" || fail "failed to resolve browser A tab window bounds for window id=$A_SELECTED_TAB_ID"
  A_CLICK_START_LINE="$(log_line_count)"
  click_browser_frame_center "$TAB1_WIN_LINE" "$A_TABBED_FRAME_X" "$A_TABBED_FRAME_Y" "$A_TABBED_FRAME_SIZE" "keyboard_browser_a_after_plain_tab"
  A_HIT_LINE="$(wait_for_hit_after "$A_CLICK_START_LINE" "$A_CONTEXT_ID" "keyboard switch browser A hit-test after plain-tab return")"
  require_text "$A_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "keyboard switch browser A hit-test has tab 1 selected tab id"
  require_text "$A_HIT_LINE" "overlay_frame=${A_TABBED_FRAME}" "keyboard switch browser A hit-test uses tabbed frame"
  enter_browser_browse "keyboard_browser_a_after_plain_tab" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
  type_marker_require_only "keyboard browser A after plain-tab return" "ISSUE809_EXP26_BROWSER_A_AFTER_PLAIN_TAB" "$A_BROWSER_TAB_ID" "$A_PANE_ID"
  leave_browser_browse "keyboard_browser_a_after_plain_tab" "$A_PANE_ID" "$A_BROWSER_TAB_ID"

  SWITCH_TAB2_START_LINE="$(log_line_count)"
  log "keyboard_switch_tab2_for_browser_b_keybind=ctrl+2=goto_tab:2"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 19 control >>"$HARNESS_LOG" 2>&1
  delay 1
  require_log_after "$SWITCH_TAB2_START_LINE" "dispatching action target=surface action=.goto_tab value=@enumFromInt\\(2\\)" "keyboard switch tab 2 goto action dispatched before browser B"

  BROWSER_B_START_LINE="$(log_line_count)"
  BROWSER_B_TRACE_START_LINE="$(trace_line_count)"
  printf '"%s" --browser "%s" "%s"' "$WEB" "$CHROMIUM" "$URL_B" >"$SECOND_BROWSER_COMMAND"
  log "keyboard_browser_b_command=$(cat "$SECOND_BROWSER_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$SECOND_BROWSER_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1

  B_CA_CONTEXT_LINE="$(wait_for_different_zig_event_after "$BROWSER_B_START_LINE" "ca_context" "$A_PANE_ID" "keyboard switch browser B Zig ca_context")"
  B_PANE_ID="$(extract_pane_id "$B_CA_CONTEXT_LINE")"
  B_BROWSER_TAB_ID="$(extract_browser_tab_id "$B_CA_CONTEXT_LINE")"
  B_CONTEXT_ID="$(extract_context_id "$B_CA_CONTEXT_LINE")"
  [ -n "$B_PANE_ID" ] || fail "could not extract keyboard switch browser B pane id"
  [ -n "$B_BROWSER_TAB_ID" ] || fail "could not extract keyboard switch browser B tab id"
  [ -n "$B_CONTEXT_ID" ] || fail "could not extract keyboard switch browser B context id"
  [ "$B_PANE_ID" != "$A_PANE_ID" ] || fail "keyboard switch browser B reused browser A pane id"
  [ "$B_BROWSER_TAB_ID" != "$A_BROWSER_TAB_ID" ] || fail "keyboard switch browser B reused browser A tab id"
  [ "$B_CONTEXT_ID" != "$A_CONTEXT_ID" ] || fail "keyboard switch browser B reused browser A CA/context id"
  log "keyboard_browser_b_pane_id=$B_PANE_ID"
  log "keyboard_browser_b_browser_tab_id=$B_BROWSER_TAB_ID"
  log "keyboard_browser_b_context_id=$B_CONTEXT_ID"

  B_APPKIT_PRESENT_LINE="$(wait_for_line_after "$BROWSER_B_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${B_PANE_ID} .*context_id=${B_CONTEXT_ID}" "keyboard switch browser B AppKit presentation")"
  B_APPKIT_PIXELS_LINE="$(wait_for_line_after "$BROWSER_B_START_LINE" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${B_PANE_ID} .*context_id=${B_CONTEXT_ID}" "keyboard switch browser B AppKit pixels")"
  B_SELECTED_TAB_ID="$(extract_selected_tab_id "$B_APPKIT_PRESENT_LINE")"
  B_FRAME="$(extract_overlay_frame "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_SIZE="$(extract_frame_size "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_X="$(extract_frame_x "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_Y="$(extract_frame_y "$B_APPKIT_PRESENT_LINE")"
  B_PIXEL="$(extract_appkit_pixel "$B_APPKIT_PIXELS_LINE")"
  [ "$B_SELECTED_TAB_ID" = "$TAB2_SELECTED_TAB_ID" ] || fail "keyboard switch browser B selected tab mismatch: expected=$TAB2_SELECTED_TAB_ID actual=$B_SELECTED_TAB_ID"
  log "keyboard_browser_b_selected_tab_id=$B_SELECTED_TAB_ID"
  log "keyboard_browser_b_overlay_frame=$B_FRAME"
  log "keyboard_browser_b_overlay_frame_size=$B_FRAME_SIZE"
  log "keyboard_browser_b_appkit_pixel=$B_PIXEL"

  B_PIXEL_WIDTH="${B_PIXEL%x*}"
  B_PIXEL_HEIGHT="${B_PIXEL#*x}"
  require_trace_after "$BROWSER_B_TRACE_START_LINE" "resize tab_id=${B_BROWSER_TAB_ID} pane_id=${B_PANE_ID} pixel_width=${B_PIXEL_WIDTH} pixel_height=${B_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied keyboard switch browser B resize to AppKit pixel size"

  TAB2_WIN_LINE="$(window_bounds_for "$TAB2_SELECTED_TAB_ID")" || fail "failed to resolve browser B tab window bounds for window id=$TAB2_SELECTED_TAB_ID"
  B_CLICK_START_LINE="$(log_line_count)"
  click_browser_frame_center "$TAB2_WIN_LINE" "$B_FRAME_X" "$B_FRAME_Y" "$B_FRAME_SIZE" "keyboard_browser_b_tab"
  B_HIT_LINE="$(wait_for_hit_after "$B_CLICK_START_LINE" "$B_CONTEXT_ID" "keyboard switch browser B hit-test")"
  require_text "$B_HIT_LINE" "selected_tab_id:${TAB2_SELECTED_TAB_ID}" "keyboard switch browser B hit-test has tab 2 selected tab id"
  require_text "$B_HIT_LINE" "overlay_frame=${B_FRAME}" "keyboard switch browser B hit-test uses browser B frame"
  enter_browser_browse "keyboard_browser_b_tab" "$B_PANE_ID" "$B_BROWSER_TAB_ID"
  type_marker_require_only "keyboard browser B tab" "ISSUE809_EXP26_BROWSER_B_TAB" "$B_BROWSER_TAB_ID" "$B_PANE_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID"
  leave_browser_browse "keyboard_browser_b_tab" "$B_PANE_ID" "$B_BROWSER_TAB_ID"
  screencapture -x -o -l"$TAB2_SELECTED_TAB_ID" "$SCREENSHOT_TAB_BROWSER_B"
  log "keyboard_browser_b_screenshot_exit=$?"

  SWITCH_A_FROM_B_START_LINE="$(log_line_count)"
  log "keyboard_switch_a_from_b_keybind=ctrl+p=previous_tab"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 35 control >>"$HARNESS_LOG" 2>&1
  delay 1
  wait_for_log_after "$SWITCH_A_FROM_B_START_LINE" "Pane focus changed: pane_id=${A_PANE_ID} focused=true" "keyboard switch browser A pane focused after returning from browser B"
  require_no_different_appkit_frame_after "$SWITCH_A_FROM_B_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_TABBED_FRAME" "keyboard switch browser A kept frame after browser B tab"
  require_no_different_appkit_pixels_after "$SWITCH_A_FROM_B_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_TABBED_PIXEL" "keyboard switch browser A kept pixels after browser B tab"
  TAB1_WIN_LINE="$(window_bounds_for "$A_SELECTED_TAB_ID")" || fail "failed to resolve restored browser A window bounds for window id=$A_SELECTED_TAB_ID"
  A_FROM_B_CLICK_START_LINE="$(log_line_count)"
  click_browser_frame_center "$TAB1_WIN_LINE" "$A_TABBED_FRAME_X" "$A_TABBED_FRAME_Y" "$A_TABBED_FRAME_SIZE" "keyboard_browser_a_after_browser_b"
  A_FROM_B_HIT_LINE="$(wait_for_hit_after "$A_FROM_B_CLICK_START_LINE" "$A_CONTEXT_ID" "keyboard switch browser A hit-test after browser B")"
  require_text "$A_FROM_B_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "keyboard switch browser A after browser B hit-test has tab 1 selected tab id"
  require_text "$A_FROM_B_HIT_LINE" "overlay_frame=${A_TABBED_FRAME}" "keyboard switch browser A after browser B hit-test uses browser A frame"
  enter_browser_browse "keyboard_browser_a_after_browser_b" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
  type_marker_require_only "keyboard browser A after browser B" "ISSUE809_EXP26_BROWSER_A_AFTER_BROWSER_B" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$B_BROWSER_TAB_ID" "$B_PANE_ID"
  leave_browser_browse "keyboard_browser_a_after_browser_b" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
  screencapture -x -o -l"$A_SELECTED_TAB_ID" "$SCREENSHOT_TAB_BROWSER_A_RESTORED"
  log "keyboard_browser_a_restored_screenshot_exit=$?"

  NEW_WINDOW_C_START_LINE="$(log_line_count)"
  log "keyboard_new_window_c_keybind=ctrl+b=new_window"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 11 control >>"$HARNESS_LOG" 2>&1
  delay 2

  require_log_after "$NEW_WINDOW_C_START_LINE" "dispatching action target=surface action=.new_window" "keyboard switch window C new-window action dispatched"
  require_log_after "$NEW_WINDOW_C_START_LINE" 'starting command command=`/usr/bin/login`' "keyboard switch window C started plain login shell"
  C_WIN_LINE=""
  for _ in $(seq 1 30); do
    C_WIN_LINE="$(app_windows | awk -F '\t' -v old="$A_WINDOW_ID" '$1 != old { print; exit }' || true)"
    if [ -n "$C_WIN_LINE" ]; then
      break
    fi
    delay 1
  done
  [ -n "$C_WIN_LINE" ] || fail "timed out waiting for keyboard switch second Ghostboard window"
  IFS=$'\t' read -r C_WINDOW_ID C_WX C_WY C_WW C_WH <<<"$C_WIN_LINE"
  [ "$C_WINDOW_ID" != "$A_WINDOW_ID" ] || fail "keyboard switch window C reused browser A window id"
  log "keyboard_window_c=$C_WIN_LINE"

  click_window_center "$C_WIN_LINE" "keyboard_window_c_shell"
  delay 1

  BROWSER_C_START_LINE="$(log_line_count)"
  BROWSER_C_TRACE_START_LINE="$(trace_line_count)"
  printf '"%s" --browser "%s" "%s"' "$WEB" "$CHROMIUM" "$URL_C" >"$THIRD_BROWSER_COMMAND"
  log "keyboard_browser_c_command=$(cat "$THIRD_BROWSER_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$THIRD_BROWSER_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1

  C_CA_CONTEXT_LINE="$(wait_for_different_zig_event_after "$BROWSER_C_START_LINE" "ca_context" "$A_PANE_ID" "keyboard switch browser C Zig ca_context")"
  C_PANE_ID="$(extract_pane_id "$C_CA_CONTEXT_LINE")"
  C_BROWSER_TAB_ID="$(extract_browser_tab_id "$C_CA_CONTEXT_LINE")"
  C_CONTEXT_ID="$(extract_context_id "$C_CA_CONTEXT_LINE")"
  [ -n "$C_PANE_ID" ] || fail "could not extract keyboard switch browser C pane id"
  [ -n "$C_BROWSER_TAB_ID" ] || fail "could not extract keyboard switch browser C tab id"
  [ -n "$C_CONTEXT_ID" ] || fail "could not extract keyboard switch browser C context id"
  [ "$C_PANE_ID" != "$A_PANE_ID" ] || fail "keyboard switch browser C reused browser A pane id"
  [ "$C_PANE_ID" != "$B_PANE_ID" ] || fail "keyboard switch browser C reused browser B pane id"
  [ "$C_BROWSER_TAB_ID" != "$A_BROWSER_TAB_ID" ] || fail "keyboard switch browser C reused browser A tab id"
  [ "$C_BROWSER_TAB_ID" != "$B_BROWSER_TAB_ID" ] || fail "keyboard switch browser C reused browser B tab id"
  [ "$C_CONTEXT_ID" != "$A_CONTEXT_ID" ] || fail "keyboard switch browser C reused browser A CA/context id"
  [ "$C_CONTEXT_ID" != "$B_CONTEXT_ID" ] || fail "keyboard switch browser C reused browser B CA/context id"
  log "keyboard_browser_c_pane_id=$C_PANE_ID"
  log "keyboard_browser_c_browser_tab_id=$C_BROWSER_TAB_ID"
  log "keyboard_browser_c_context_id=$C_CONTEXT_ID"

  C_APPKIT_PRESENT_LINE="$(wait_for_line_after "$BROWSER_C_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${C_PANE_ID} .*context_id=${C_CONTEXT_ID}" "keyboard switch browser C AppKit presentation")"
  C_APPKIT_PIXELS_LINE="$(wait_for_line_after "$BROWSER_C_START_LINE" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${C_PANE_ID} .*context_id=${C_CONTEXT_ID}" "keyboard switch browser C AppKit pixels")"
  C_PRESENT_WINDOW_ID="$(printf '%s\n' "$C_APPKIT_PRESENT_LINE" | sed -E 's/.*window_id:([^ ]+) .*/\1/')"
  C_SELECTED_TAB_ID="$(extract_selected_tab_id "$C_APPKIT_PRESENT_LINE")"
  C_FRAME="$(extract_overlay_frame "$C_APPKIT_PRESENT_LINE")"
  C_FRAME_SIZE="$(extract_frame_size "$C_APPKIT_PRESENT_LINE")"
  C_FRAME_X="$(extract_frame_x "$C_APPKIT_PRESENT_LINE")"
  C_FRAME_Y="$(extract_frame_y "$C_APPKIT_PRESENT_LINE")"
  C_PIXEL="$(extract_appkit_pixel "$C_APPKIT_PIXELS_LINE")"
  [ "$C_PRESENT_WINDOW_ID" = "$C_WINDOW_ID" ] || fail "keyboard switch browser C AppKit window id mismatch: expected=$C_WINDOW_ID actual=$C_PRESENT_WINDOW_ID"
  [ "$C_SELECTED_TAB_ID" = "$C_WINDOW_ID" ] || fail "keyboard switch browser C selected tab id mismatch: expected=$C_WINDOW_ID actual=$C_SELECTED_TAB_ID"
  log "keyboard_browser_c_selected_tab_id=$C_SELECTED_TAB_ID"
  log "keyboard_browser_c_overlay_frame=$C_FRAME"
  log "keyboard_browser_c_overlay_frame_size=$C_FRAME_SIZE"
  log "keyboard_browser_c_appkit_pixel=$C_PIXEL"

  C_PIXEL_WIDTH="${C_PIXEL%x*}"
  C_PIXEL_HEIGHT="${C_PIXEL#*x}"
  require_trace_after "$BROWSER_C_TRACE_START_LINE" "resize tab_id=${C_BROWSER_TAB_ID} pane_id=${C_PANE_ID} pixel_width=${C_PIXEL_WIDTH} pixel_height=${C_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied keyboard switch browser C resize to AppKit pixel size"

  C_CLICK_START_LINE="$(log_line_count)"
  click_browser_frame_center "$C_WIN_LINE" "$C_FRAME_X" "$C_FRAME_Y" "$C_FRAME_SIZE" "keyboard_browser_c_window"
  C_HIT_LINE="$(wait_for_hit_after "$C_CLICK_START_LINE" "$C_CONTEXT_ID" "keyboard switch browser C hit-test")"
  require_text "$C_HIT_LINE" "window_id:${C_WINDOW_ID}" "keyboard switch browser C hit-test has window C id"
  require_text "$C_HIT_LINE" "selected_tab_id:${C_SELECTED_TAB_ID}" "keyboard switch browser C hit-test has selected tab C id"
  require_text "$C_HIT_LINE" "overlay_frame=${C_FRAME}" "keyboard switch browser C hit-test uses browser C frame"
  enter_browser_browse "keyboard_browser_c_window" "$C_PANE_ID" "$C_BROWSER_TAB_ID"
  type_marker_require_only "keyboard browser C window" "ISSUE809_EXP26_BROWSER_C_WINDOW" "$C_BROWSER_TAB_ID" "$C_PANE_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$B_BROWSER_TAB_ID" "$B_PANE_ID"
  leave_browser_browse "keyboard_browser_c_window" "$C_PANE_ID" "$C_BROWSER_TAB_ID"
  screencapture -x -o -l"$C_WINDOW_ID" "$SCREENSHOT_WINDOW_C"
  log "keyboard_browser_c_screenshot_exit=$?"

  A_WINDOW_RESTORE_START_LINE="$(log_line_count)"
  A_WIN_LINE="$(window_bounds_for "$A_SELECTED_TAB_ID")" || fail "failed to resolve browser A window bounds after window C for window id=$A_SELECTED_TAB_ID"
  focus_window_bounds "$A_WIN_LINE" >>"$HARNESS_LOG" 2>&1 || fail "failed to focus browser A window after window C"
  delay 1
  log "keyboard_select_tab1_after_window_c_keybind=ctrl+1=goto_tab:1"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 18 control >>"$HARNESS_LOG" 2>&1
  delay 1
  wait_for_log_after "$A_WINDOW_RESTORE_START_LINE" "Pane focus changed: pane_id=${A_PANE_ID} focused=true" "keyboard switch browser A pane focused after returning from window C"
  require_no_different_appkit_frame_after "$A_WINDOW_RESTORE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_TABBED_FRAME" "keyboard switch browser A kept frame after window C"
  require_no_different_appkit_pixels_after "$A_WINDOW_RESTORE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_TABBED_PIXEL" "keyboard switch browser A kept pixels after window C"

  A_AFTER_WINDOW_CLICK_START_LINE="$(log_line_count)"
  A_WIN_LINE="$(window_bounds_for "$A_SELECTED_TAB_ID")" || fail "failed to resolve restored browser A window bounds after window C"
  click_browser_frame_center "$A_WIN_LINE" "$A_TABBED_FRAME_X" "$A_TABBED_FRAME_Y" "$A_TABBED_FRAME_SIZE" "keyboard_browser_a_after_window_c"
  A_AFTER_WINDOW_HIT_LINE="$(wait_for_hit_after "$A_AFTER_WINDOW_CLICK_START_LINE" "$A_CONTEXT_ID" "keyboard switch browser A hit-test after window C")"
  require_text "$A_AFTER_WINDOW_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "keyboard switch browser A after window C hit-test has tab 1 selected tab id"
  require_text "$A_AFTER_WINDOW_HIT_LINE" "overlay_frame=${A_TABBED_FRAME}" "keyboard switch browser A after window C hit-test uses browser A frame"
  enter_browser_browse "keyboard_browser_a_after_window_c" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
  type_marker_require_only "keyboard browser A after window C" "ISSUE809_EXP26_BROWSER_A_AFTER_WINDOW_C" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$B_BROWSER_TAB_ID" "$B_PANE_ID" "$C_BROWSER_TAB_ID" "$C_PANE_ID"
  screencapture -x -o -l"$A_SELECTED_TAB_ID" "$SCREENSHOT_WINDOW_A_RESTORED"
  log "keyboard_window_a_restored_screenshot_exit=$?"

  [ "$NEW_TAB_TRACE_START_LINE" -lt "$BROWSER_B_TRACE_START_LINE" ] || fail "keyboard switch trace boundaries for browser B open were not monotonic"
  [ "$BROWSER_B_TRACE_START_LINE" -lt "$BROWSER_C_TRACE_START_LINE" ] || fail "keyboard switch trace boundaries for browser C open were not monotonic"
fi

if [ "$SCENARIO" = "window-resize" ] || [ "$SCENARIO" = "performance-window-resize" ]; then
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
  require_trace "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${GROW_PIXEL_WIDTH} pixel_height=${GROW_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied grow resize to AppKit pixel size via ts_set_view_size"
  screencapture -x -o -l"$WID" "$SCREENSHOT_GROW"
  log "grow_screenshot_exit=$?"
  if [ "$SCENARIO" = "window-resize" ]; then
    GROW_HIT_START_LINE="$(log_line_count)"
    click_window_center "$GROW_WIN_LINE" "grow"
    GROW_HIT_LINE="$(wait_for_hit_after "$GROW_HIT_START_LINE" "$CONTEXT_ID" "grown AppKit hit-test")"
    log "PASS: observed grown AppKit hit-test"
    require_text "$GROW_HIT_LINE" "overlay_frame=" "grown hit-test includes current overlay frame"
    require_text "$GROW_HIT_LINE" "web_point={" "grown hit-test includes webview-relative point"
  else
    log "PASS: performance resize skipped grown pointer hit-test"
  fi

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
  require_trace "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${SHRINK_PIXEL_WIDTH} pixel_height=${SHRINK_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied shrink resize to AppKit pixel size via ts_set_view_size"
  screencapture -x -o -l"$WID" "$SCREENSHOT_SHRINK"
  log "shrink_screenshot_exit=$?"
  if [ "$SCENARIO" = "window-resize" ]; then
    SHRINK_HIT_START_LINE="$(log_line_count)"
    click_window_center "$SHRINK_WIN_LINE" "shrink"
    SHRINK_HIT_LINE="$(wait_for_hit_after "$SHRINK_HIT_START_LINE" "$CONTEXT_ID" "shrunken AppKit hit-test")"
    log "PASS: observed shrunken AppKit hit-test"
    require_text "$SHRINK_HIT_LINE" "overlay_frame=" "shrunken hit-test includes current overlay frame"
    require_text "$SHRINK_HIT_LINE" "web_point={" "shrunken hit-test includes webview-relative point"
  else
    log "PASS: performance resize skipped shrunken pointer hit-test"
  fi
fi

if [ "$SCENARIO" = "mouse-after-geometry-change" ]; then
  A_WINDOW_ID="$WID"
  A_SURFACE_ID="$(extract_surface_id "$APPKIT_PRESENT_LINE")"
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_GRID="$(extract_grid "$APPKIT_PRESENT_LINE")"
  A_GRID_WIDTH="$(extract_grid_width "$A_GRID")"
  A_GRID_HEIGHT="$(extract_grid_height "$A_GRID")"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_FRAME_WIDTH="$(pair_width "$A_FRAME_SIZE")"
  A_FRAME_HEIGHT="$(pair_height "$A_FRAME_SIZE")"
  A_PIXEL="$APPKIT_PIXEL"
  log "mouse_baseline_window_id=$A_WINDOW_ID"
  log "mouse_baseline_surface_id=$A_SURFACE_ID"
  log "mouse_baseline_selected_tab_id=$A_SELECTED_TAB_ID"
  log "mouse_baseline_pane_id=$A_PANE_ID"
  log "mouse_baseline_browser_tab_id=$A_BROWSER_TAB_ID"
  log "mouse_baseline_context_id=$A_CONTEXT_ID"
  log "mouse_baseline_grid=$A_GRID"
  log "mouse_baseline_frame=$A_FRAME"
  log "mouse_baseline_appkit_pixel=$A_PIXEL"

  if awk -v width="$A_FRAME_WIDTH" 'BEGIN { exit !(width > 1300) }'; then
    NORMALIZE_START_LINE="$(log_line_count)"
    NORMALIZE_TRACE_START_LINE="$(trace_line_count)"
    log "mouse_normalize_window_target=1280x992"
    set_window_size 1280 992 >>"$HARNESS_LOG" 2>&1 || fail "failed to normalize mouse scenario window via System Events"
    delay 1
    NORMALIZE_PRESENT_LINE="$(wait_for_appkit_frame_width_after "$NORMALIZE_START_LINE" "$A_FRAME_SIZE" lt "mouse normalized AppKit overlay frame")"
    NORMALIZE_PIXELS_LINE="$(wait_for_appkit_pixels_width_after "$NORMALIZE_START_LINE" "$A_PIXEL" lt "mouse normalized AppKit presented pixels")"
    A_FRAME="$(extract_overlay_frame "$NORMALIZE_PRESENT_LINE")"
    A_FRAME_SIZE="$(extract_frame_size "$NORMALIZE_PRESENT_LINE")"
    A_FRAME_WIDTH="$(pair_width "$A_FRAME_SIZE")"
    A_FRAME_HEIGHT="$(pair_height "$A_FRAME_SIZE")"
    A_GRID="$(extract_grid "$NORMALIZE_PRESENT_LINE")"
    A_GRID_WIDTH="$(extract_grid_width "$A_GRID")"
    A_GRID_HEIGHT="$(extract_grid_height "$A_GRID")"
    A_PIXEL="$(extract_appkit_pixel "$NORMALIZE_PIXELS_LINE")"
    A_PIXEL_WIDTH="${A_PIXEL%x*}"
    A_PIXEL_HEIGHT="${A_PIXEL#*x}"
    WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve normalized mouse window bounds"
    IFS=$'\t' read -r WID WX WY WW WH <<<"$WIN_LINE"
    log "mouse_normalized_window=$WIN_LINE"
    log "mouse_normalized_grid=$A_GRID"
    log "mouse_normalized_frame=$A_FRAME"
    log "mouse_normalized_appkit_pixel=$A_PIXEL"
    require_trace_after "$NORMALIZE_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${A_PIXEL_WIDTH} pixel_height=${A_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied mouse normalization resize"
  fi

  INITIAL_WW="$WW"
  INITIAL_WH="$WH"
  GROW_WIDTH=$((INITIAL_WW + 320))
  GROW_HEIGHT=$((INITIAL_WH + 220))
  GROW_START_LINE="$(log_line_count)"
  GROW_TRACE_START_LINE="$(trace_line_count)"
  log "mouse_resize_grow_target=${GROW_WIDTH}x${GROW_HEIGHT}"
  set_window_size "$GROW_WIDTH" "$GROW_HEIGHT" >>"$HARNESS_LOG" 2>&1 || fail "failed to grow window via System Events"
  delay 1
  GROW_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve grown window bounds"
  log "mouse_grow_window=$GROW_WIN_LINE"
  GROW_PRESENT_LINE="$(wait_for_appkit_frame_width_after "$GROW_START_LINE" "$A_FRAME_SIZE" gt "mouse width-grown AppKit overlay frame")"
  GROW_PIXELS_LINE="$(wait_for_appkit_pixels_width_after "$GROW_START_LINE" "$A_PIXEL" gt "mouse width-grown AppKit presented pixels")"
  GROW_FRAME="$(extract_overlay_frame "$GROW_PRESENT_LINE")"
  GROW_FRAME_SIZE="$(extract_frame_size "$GROW_PRESENT_LINE")"
  GROW_PIXEL="$(extract_appkit_pixel "$GROW_PIXELS_LINE")"
  GROW_PIXEL_WIDTH="${GROW_PIXEL%x*}"
  GROW_PIXEL_HEIGHT="${GROW_PIXEL#*x}"
  log "mouse_grow_frame=$GROW_FRAME"
  log "mouse_grow_appkit_pixel=$GROW_PIXEL"
  require_log_after "$GROW_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${A_PANE_ID} .*appkit_pixel=${GROW_PIXEL}" "Zig records mouse grown AppKit presented pixel size"
  require_trace_after "$GROW_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${GROW_PIXEL_WIDTH} pixel_height=${GROW_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied mouse grow resize"
  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_GROW"
  log "mouse_grow_screenshot_exit=$?"
  IFS=$'\t' read -r GROW_CLICK_X GROW_CLICK_Y <<<"$(click_point_for_frame "$GROW_WIN_LINE" "$GROW_PRESENT_LINE")"
  assert_mouse_after_click "$GROW_CLICK_X" "$GROW_CLICK_Y" "mouse_grow_inside" "$A_CONTEXT_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$GROW_FRAME" "mouse grown overlay"

  SHRINK_WIDTH=$((INITIAL_WW + 80))
  SHRINK_HEIGHT=$((INITIAL_WH - 60))
  [ "$SHRINK_HEIGHT" -ge 700 ] || SHRINK_HEIGHT=700
  SHRINK_START_LINE="$(log_line_count)"
  SHRINK_TRACE_START_LINE="$(trace_line_count)"
  log "mouse_resize_shrink_target=${SHRINK_WIDTH}x${SHRINK_HEIGHT}"
  set_window_size "$SHRINK_WIDTH" "$SHRINK_HEIGHT" >>"$HARNESS_LOG" 2>&1 || fail "failed to shrink window via System Events"
  delay 1
  SHRINK_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve shrunken window bounds"
  log "mouse_shrink_window=$SHRINK_WIN_LINE"
  SHRINK_PRESENT_LINE="$(wait_for_appkit_frame_after "$SHRINK_START_LINE" "$GROW_FRAME_SIZE" lt "mouse shrunken AppKit overlay frame")"
  SHRINK_PIXELS_LINE="$(wait_for_appkit_pixels_after "$SHRINK_START_LINE" "$GROW_PIXEL" lt "mouse shrunken AppKit presented pixels")"
  SHRINK_GRID="$(extract_grid "$SHRINK_PRESENT_LINE")"
  SHRINK_GRID_WIDTH="$(extract_grid_width "$SHRINK_GRID")"
  SHRINK_FRAME="$(extract_overlay_frame "$SHRINK_PRESENT_LINE")"
  SHRINK_FRAME_SIZE="$(extract_frame_size "$SHRINK_PRESENT_LINE")"
  SHRINK_PIXEL="$(extract_appkit_pixel "$SHRINK_PIXELS_LINE")"
  SHRINK_PIXEL_WIDTH="${SHRINK_PIXEL%x*}"
  SHRINK_PIXEL_HEIGHT="${SHRINK_PIXEL#*x}"
  log "mouse_shrink_frame=$SHRINK_FRAME"
  log "mouse_shrink_grid=$SHRINK_GRID"
  log "mouse_shrink_appkit_pixel=$SHRINK_PIXEL"
  require_log_after "$SHRINK_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${A_PANE_ID} .*appkit_pixel=${SHRINK_PIXEL}" "Zig records mouse shrunken AppKit presented pixel size"
  require_trace_after "$SHRINK_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${SHRINK_PIXEL_WIDTH} pixel_height=${SHRINK_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied mouse shrink resize"
  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_SHRINK"
  log "mouse_shrink_screenshot_exit=$?"
  IFS=$'\t' read -r SHRINK_CLICK_X SHRINK_CLICK_Y <<<"$(click_point_for_frame "$SHRINK_WIN_LINE" "$SHRINK_PRESENT_LINE")"
  assert_mouse_after_click "$SHRINK_CLICK_X" "$SHRINK_CLICK_Y" "mouse_shrink_inside" "$A_CONTEXT_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$SHRINK_FRAME" "mouse shrunken overlay"

  SHRINK_ROWS=12
  [ "$A_GRID_HEIGHT" -gt "$SHRINK_ROWS" ] || fail "baseline grid height is too small for mouse TUI shrink test: baseline=$A_GRID"
  TUI_SHRINK_START_LINE="$(log_line_count)"
  TUI_SHRINK_TRACE_START_LINE="$(trace_line_count)"
  printf ':viewport height %s' "$SHRINK_ROWS" >"$TUI_VIEWPORT_SHRINK_COMMAND"
  log "mouse_tui_viewport_shrink_command_text=$(cat "$TUI_VIEWPORT_SHRINK_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$TUI_VIEWPORT_SHRINK_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  delay 1
  TUI_SHRINK_SET_OVERLAY_LINE="$(wait_for_line_after "$TUI_SHRINK_START_LINE" "TermSurf geometry layer=zig event=set_overlay_update .*pane_id:${A_PANE_ID} .*browser_tab_id:${A_BROWSER_TAB_ID} .*grid=${SHRINK_GRID_WIDTH}x${SHRINK_ROWS}\\+1\\+1 .*context_id=${A_CONTEXT_ID}" "mouse TUI shrink SetOverlay update" 45)"
  TUI_SHRINK_PRESENT_LINE="$(wait_for_changed_appkit_frame_after "$TUI_SHRINK_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$SHRINK_FRAME" "mouse TUI-shrunken AppKit overlay frame" 45)"
  TUI_SHRINK_PIXELS_LINE="$(wait_for_changed_appkit_pixels_after "$TUI_SHRINK_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$SHRINK_PIXEL" "mouse TUI-shrunken AppKit pixels" 45)"
  TUI_SHRINK_FRAME="$(extract_overlay_frame "$TUI_SHRINK_PRESENT_LINE")"
  TUI_SHRINK_FRAME_SIZE="$(extract_frame_size "$TUI_SHRINK_PRESENT_LINE")"
  TUI_SHRINK_FRAME_HEIGHT="$(pair_height "$TUI_SHRINK_FRAME_SIZE")"
  TUI_SHRINK_PIXEL="$(extract_appkit_pixel "$TUI_SHRINK_PIXELS_LINE")"
  TUI_SHRINK_PIXEL_WIDTH="${TUI_SHRINK_PIXEL%x*}"
  TUI_SHRINK_PIXEL_HEIGHT="${TUI_SHRINK_PIXEL#*x}"
  [ "$TUI_SHRINK_FRAME_HEIGHT" -lt "$(pair_height "$SHRINK_FRAME_SIZE")" ] || fail "mouse TUI shrink frame height did not shrink"
  log "mouse_tui_shrink_set_overlay=$TUI_SHRINK_SET_OVERLAY_LINE"
  log "mouse_tui_shrink_frame=$TUI_SHRINK_FRAME"
  log "mouse_tui_shrink_appkit_pixel=$TUI_SHRINK_PIXEL"
  require_trace_after "$TUI_SHRINK_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${TUI_SHRINK_PIXEL_WIDTH} pixel_height=${TUI_SHRINK_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied mouse TUI shrink resize"
  TUI_SHRINK_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve mouse TUI-shrunken window bounds"
  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_TUI_SHRINK"
  log "mouse_tui_shrink_screenshot_exit=$?"
  IFS=$'\t' read -r TUI_SHRINK_CLICK_X TUI_SHRINK_CLICK_Y <<<"$(click_point_for_frame "$TUI_SHRINK_WIN_LINE" "$TUI_SHRINK_PRESENT_LINE")"
  assert_mouse_after_click "$TUI_SHRINK_CLICK_X" "$TUI_SHRINK_CLICK_Y" "mouse_tui_shrink_inside" "$A_CONTEXT_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$TUI_SHRINK_FRAME" "mouse TUI-shrunken overlay"

  IFS=$'\t' read -r _TUI_WID TUI_WX TUI_WY TUI_WW TUI_WH <<<"$TUI_SHRINK_WIN_LINE"
  TUI_SHRINK_FRAME_Y="$(extract_frame_y "$TUI_SHRINK_PRESENT_LINE")"
  TUI_SHRINK_ROOT_HEIGHT="$(pair_height "$(extract_root_frame_size "$TUI_SHRINK_PRESENT_LINE")")"
  TUI_SHRINK_CONTENT_Y_OFFSET="$(awk -v wh="$TUI_WH" -v root_h="$TUI_SHRINK_ROOT_HEIGHT" 'BEGIN { print int(wh - root_h) }')"
  TUI_STALE_X="$TUI_SHRINK_CLICK_X"
  TUI_STALE_Y="$(awk -v wy="$TUI_WY" -v content_y="$TUI_SHRINK_CONTENT_Y_OFFSET" -v frame_y="$TUI_SHRINK_FRAME_Y" -v shrink_h="$TUI_SHRINK_FRAME_HEIGHT" -v old_h="$(pair_height "$SHRINK_FRAME_SIZE")" 'BEGIN { print int(wy + content_y + frame_y + shrink_h + ((old_h - shrink_h) / 2) + 0.5) }')"
  assert_stale_click_misses_browser "$TUI_STALE_X" "$TUI_STALE_Y" "mouse_tui_shrink_stale_lower" "$A_CONTEXT_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "mouse TUI-shrunken former lower area"

  TUI_RESET_START_LINE="$(log_line_count)"
  TUI_RESET_TRACE_START_LINE="$(trace_line_count)"
  printf ':viewport reset' >"$TUI_VIEWPORT_RESET_COMMAND"
  log "mouse_tui_viewport_reset_command_text=$(cat "$TUI_VIEWPORT_RESET_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$TUI_VIEWPORT_RESET_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  delay 1
  TUI_RESET_PRESENT_LINE="$(wait_for_exact_appkit_frame_after "$TUI_RESET_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$SHRINK_FRAME" "mouse TUI-reset AppKit overlay frame" 45)"
  TUI_RESET_PIXELS_LINE="$(wait_for_exact_appkit_pixels_after "$TUI_RESET_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$SHRINK_PIXEL" "mouse TUI-reset AppKit pixels" 45)"
  TUI_RESET_FRAME="$(extract_overlay_frame "$TUI_RESET_PRESENT_LINE")"
  TUI_RESET_FRAME_SIZE="$(extract_frame_size "$TUI_RESET_PRESENT_LINE")"
  TUI_RESET_PIXEL="$(extract_appkit_pixel "$TUI_RESET_PIXELS_LINE")"
  TUI_RESET_PIXEL_WIDTH="${TUI_RESET_PIXEL%x*}"
  TUI_RESET_PIXEL_HEIGHT="${TUI_RESET_PIXEL#*x}"
  log "mouse_tui_reset_frame=$TUI_RESET_FRAME"
  log "mouse_tui_reset_appkit_pixel=$TUI_RESET_PIXEL"
  require_trace_after "$TUI_RESET_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${TUI_RESET_PIXEL_WIDTH} pixel_height=${TUI_RESET_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied mouse TUI reset resize"
  TUI_RESET_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve mouse TUI-reset window bounds"
  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_TUI_RESET"
  log "mouse_tui_reset_screenshot_exit=$?"
  IFS=$'\t' read -r TUI_RESET_CLICK_X TUI_RESET_CLICK_Y <<<"$(click_point_for_frame "$TUI_RESET_WIN_LINE" "$TUI_RESET_PRESENT_LINE")"
  assert_mouse_after_click "$TUI_RESET_CLICK_X" "$TUI_RESET_CLICK_Y" "mouse_tui_reset_inside" "$A_CONTEXT_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$TUI_RESET_FRAME" "mouse TUI-reset overlay"

  SPLIT_START_LINE="$(log_line_count)"
  SPLIT_TRACE_START_LINE="$(trace_line_count)"
  log "mouse_split_keybind=ctrl+d=new_split:right"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 2 control >>"$HARNESS_LOG" 2>&1
  delay 1
  SPLIT_PRESENT_LINE="$(wait_for_split_right_frame_after "$SPLIT_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$TUI_RESET_FRAME_SIZE" "mouse split-right AppKit overlay frame")"
  SPLIT_PIXELS_LINE="$(wait_for_split_right_pixels_after "$SPLIT_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$TUI_RESET_PIXEL" "mouse split-right AppKit pixels")"
  SPLIT_FRAME="$(extract_overlay_frame "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_SIZE="$(extract_frame_size "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_X="$(extract_frame_x "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_WIDTH="$(pair_width "$SPLIT_FRAME_SIZE")"
  SPLIT_PIXEL="$(extract_appkit_pixel "$SPLIT_PIXELS_LINE")"
  SPLIT_PIXEL_WIDTH="${SPLIT_PIXEL%x*}"
  SPLIT_PIXEL_HEIGHT="${SPLIT_PIXEL#*x}"
  log "mouse_split_frame=$SPLIT_FRAME"
  log "mouse_split_appkit_pixel=$SPLIT_PIXEL"
  require_trace_after "$SPLIT_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${SPLIT_PIXEL_WIDTH} pixel_height=${SPLIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied mouse split-right resize"
  SPLIT_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve mouse split window bounds"
  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_SPLIT"
  log "mouse_split_screenshot_exit=$?"
  IFS=$'\t' read -r SPLIT_CLICK_X SPLIT_CLICK_Y <<<"$(click_point_for_frame "$SPLIT_WIN_LINE" "$SPLIT_PRESENT_LINE")"
  prime_mouse_focus "$SPLIT_CLICK_X" "$SPLIT_CLICK_Y" "mouse_split_focus_prime" "$A_CONTEXT_ID" "$SPLIT_FRAME" "mouse split-right overlay"
  assert_mouse_after_click "$SPLIT_CLICK_X" "$SPLIT_CLICK_Y" "mouse_split_inside" "$A_CONTEXT_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$SPLIT_FRAME" "mouse split-right overlay"
  IFS=$'\t' read -r _SPLIT_WID SPLIT_WX SPLIT_WY SPLIT_WW SPLIT_WH <<<"$SPLIT_WIN_LINE"
  SPLIT_STALE_X="$(awk -v wx="$SPLIT_WX" -v frame_x="$SPLIT_FRAME_X" -v frame_w="$SPLIT_FRAME_WIDTH" -v old_w="$(pair_width "$TUI_RESET_FRAME_SIZE")" 'BEGIN { print int(wx + frame_x + frame_w + ((old_w - frame_w) / 2) + 0.5) }')"
  assert_stale_click_misses_browser "$SPLIT_STALE_X" "$SPLIT_CLICK_Y" "mouse_split_stale_sibling" "$A_CONTEXT_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "mouse split-right stale sibling area"

  DIVIDER_START_LINE="$(log_line_count)"
  DIVIDER_TRACE_START_LINE="$(trace_line_count)"
  log "mouse_resize_split_keybind=ctrl+l=resize_split:right,20"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 37 control >>"$HARNESS_LOG" 2>&1
  delay 1
  DIVIDER_PRESENT_LINE="$(wait_for_split_right_resize_frame_after "$DIVIDER_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$SPLIT_FRAME_SIZE" "mouse divider-resized AppKit overlay frame")"
  DIVIDER_PIXELS_LINE="$(wait_for_split_right_resize_pixels_after "$DIVIDER_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$SPLIT_PIXEL" "mouse divider-resized AppKit pixels")"
  DIVIDER_FRAME="$(extract_overlay_frame "$DIVIDER_PRESENT_LINE")"
  DIVIDER_FRAME_SIZE="$(extract_frame_size "$DIVIDER_PRESENT_LINE")"
  DIVIDER_PIXEL="$(extract_appkit_pixel "$DIVIDER_PIXELS_LINE")"
  DIVIDER_PIXEL_WIDTH="${DIVIDER_PIXEL%x*}"
  DIVIDER_PIXEL_HEIGHT="${DIVIDER_PIXEL#*x}"
  log "mouse_divider_frame=$DIVIDER_FRAME"
  log "mouse_divider_appkit_pixel=$DIVIDER_PIXEL"
  require_trace_after "$DIVIDER_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${DIVIDER_PIXEL_WIDTH} pixel_height=${DIVIDER_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied mouse divider resize"
  DIVIDER_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve mouse divider window bounds"
  IFS=$'\t' read -r DIVIDER_CLICK_X DIVIDER_CLICK_Y <<<"$(click_point_for_frame "$DIVIDER_WIN_LINE" "$DIVIDER_PRESENT_LINE")"
  prime_mouse_focus "$DIVIDER_CLICK_X" "$DIVIDER_CLICK_Y" "mouse_divider_focus_prime" "$A_CONTEXT_ID" "$DIVIDER_FRAME" "mouse divider-resized overlay"
  assert_mouse_after_click "$DIVIDER_CLICK_X" "$DIVIDER_CLICK_Y" "mouse_divider_inside" "$A_CONTEXT_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$DIVIDER_FRAME" "mouse divider-resized overlay"

  EQUALIZE_START_LINE="$(log_line_count)"
  EQUALIZE_TRACE_START_LINE="$(trace_line_count)"
  log "mouse_equalize_keybind=ctrl+e=equalize_splits"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 14 control >>"$HARNESS_LOG" 2>&1
  delay 1
  EQUALIZE_PRESENT_LINE="$(wait_for_split_right_equalize_frame_after "$EQUALIZE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$SPLIT_FRAME_SIZE" "$DIVIDER_FRAME_SIZE" "mouse equalized AppKit overlay frame")"
  EQUALIZE_PIXELS_LINE="$(wait_for_split_right_equalize_pixels_after "$EQUALIZE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$SPLIT_PIXEL" "$DIVIDER_PIXEL" "mouse equalized AppKit pixels")"
  EQUALIZE_FRAME="$(extract_overlay_frame "$EQUALIZE_PRESENT_LINE")"
  EQUALIZE_PIXEL="$(extract_appkit_pixel "$EQUALIZE_PIXELS_LINE")"
  EQUALIZE_PIXEL_WIDTH="${EQUALIZE_PIXEL%x*}"
  EQUALIZE_PIXEL_HEIGHT="${EQUALIZE_PIXEL#*x}"
  log "mouse_equalize_frame=$EQUALIZE_FRAME"
  log "mouse_equalize_appkit_pixel=$EQUALIZE_PIXEL"
  require_trace_after "$EQUALIZE_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${EQUALIZE_PIXEL_WIDTH} pixel_height=${EQUALIZE_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied mouse equalize resize"
  EQUALIZE_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve mouse equalized window bounds"
  IFS=$'\t' read -r EQUALIZE_CLICK_X EQUALIZE_CLICK_Y <<<"$(click_point_for_frame "$EQUALIZE_WIN_LINE" "$EQUALIZE_PRESENT_LINE")"
  prime_mouse_focus "$EQUALIZE_CLICK_X" "$EQUALIZE_CLICK_Y" "mouse_equalize_focus_prime" "$A_CONTEXT_ID" "$EQUALIZE_FRAME" "mouse equalized overlay"
  assert_mouse_after_click "$EQUALIZE_CLICK_X" "$EQUALIZE_CLICK_Y" "mouse_equalize_inside" "$A_CONTEXT_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$EQUALIZE_FRAME" "mouse equalized overlay"

  [ "$GROW_TRACE_START_LINE" -lt "$SHRINK_TRACE_START_LINE" ] || fail "trace boundaries for mouse grow/shrink were not monotonic"
  [ "$SHRINK_TRACE_START_LINE" -lt "$TUI_SHRINK_TRACE_START_LINE" ] || fail "trace boundaries for mouse shrink/TUI shrink were not monotonic"
  [ "$TUI_SHRINK_TRACE_START_LINE" -lt "$TUI_RESET_TRACE_START_LINE" ] || fail "trace boundaries for mouse TUI shrink/reset were not monotonic"
  [ "$TUI_RESET_TRACE_START_LINE" -lt "$SPLIT_TRACE_START_LINE" ] || fail "trace boundaries for mouse TUI reset/split were not monotonic"
  [ "$SPLIT_TRACE_START_LINE" -lt "$DIVIDER_TRACE_START_LINE" ] || fail "trace boundaries for mouse split/divider were not monotonic"
  [ "$DIVIDER_TRACE_START_LINE" -lt "$EQUALIZE_TRACE_START_LINE" ] || fail "trace boundaries for mouse divider/equalize were not monotonic"
fi

if [ "$SCENARIO" = "font-size-cell-metrics" ]; then
  A_WINDOW_ID="$WID"
  A_SURFACE_ID="$(extract_surface_id "$APPKIT_PRESENT_LINE")"
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_GRID="$(extract_grid "$APPKIT_PRESENT_LINE")"
  A_CELL="$(extract_cell_size "$APPKIT_PRESENT_LINE")"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_PIXEL="$APPKIT_PIXEL"
  A_BACKING_SCALE="$(extract_backing_scale "$APPKIT_PRESENT_LINE")"
  log "font_baseline_window_id=$A_WINDOW_ID"
  log "font_baseline_surface_id=$A_SURFACE_ID"
  log "font_baseline_selected_tab_id=$A_SELECTED_TAB_ID"
  log "font_baseline_pane_id=$A_PANE_ID"
  log "font_baseline_browser_tab_id=$A_BROWSER_TAB_ID"
  log "font_baseline_context_id=$A_CONTEXT_ID"
  log "font_baseline_grid=$A_GRID"
  log "font_baseline_cell=$A_CELL"
  log "font_baseline_frame=$A_FRAME"
  log "font_baseline_appkit_pixel=$A_PIXEL"
  log "font_baseline_backing_scale=$A_BACKING_SCALE"

  INCREASE_START_LINE="$(log_line_count)"
  INCREASE_TRACE_START_LINE="$(trace_line_count)"
  log "font_increase_keybind=ctrl+u=increase_font_size:2"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 32 control >>"$HARNESS_LOG" 2>&1
  delay 1
  INCREASE_PRESENT_LINE="$(wait_for_changed_appkit_frame_after "$INCREASE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "font-increased AppKit overlay frame" 45)"
  INCREASE_PIXELS_LINE="$(wait_for_changed_appkit_pixels_after "$INCREASE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "font-increased AppKit pixels" 45)"
  INCREASE_WINDOW_ID="$(printf '%s\n' "$INCREASE_PRESENT_LINE" | sed -E 's/.*window_id:([^ ]+) .*/\1/')"
  INCREASE_SURFACE_ID="$(extract_surface_id "$INCREASE_PRESENT_LINE")"
  INCREASE_SELECTED_TAB_ID="$(extract_selected_tab_id "$INCREASE_PRESENT_LINE")"
  INCREASE_GRID="$(extract_grid "$INCREASE_PRESENT_LINE")"
  INCREASE_CELL="$(extract_cell_size "$INCREASE_PRESENT_LINE")"
  INCREASE_FRAME="$(extract_overlay_frame "$INCREASE_PRESENT_LINE")"
  INCREASE_PIXEL="$(extract_appkit_pixel "$INCREASE_PIXELS_LINE")"
  INCREASE_SCALE="$(extract_backing_scale "$INCREASE_PRESENT_LINE")"
  INCREASE_CURRENT_PIXEL="$(appkit_pixel_from_geometry_line "$INCREASE_PRESENT_LINE")"
  [ "$INCREASE_WINDOW_ID" = "$A_WINDOW_ID" ] || fail "font-increase window id changed unexpectedly: expected=$A_WINDOW_ID actual=$INCREASE_WINDOW_ID"
  [ "$INCREASE_SURFACE_ID" = "$A_SURFACE_ID" ] || fail "font-increase surface id changed"
  [ "$INCREASE_SELECTED_TAB_ID" = "$A_SELECTED_TAB_ID" ] || fail "font-increase selected tab id changed"
  [ "$INCREASE_GRID" != "$A_GRID" ] || fail "font-increase grid did not change: $INCREASE_GRID"
  [ "$INCREASE_CELL" != "$A_CELL" ] || fail "font-increase cell size did not change: $INCREASE_CELL"
  [ "$INCREASE_SCALE" = "$A_BACKING_SCALE" ] || fail "font-increase backing scale mismatch: expected=$A_BACKING_SCALE actual=$INCREASE_SCALE"
  [ "$INCREASE_CURRENT_PIXEL" = "$INCREASE_PIXEL" ] || fail "font-increase current frame-derived pixel mismatch: expected=$INCREASE_PIXEL actual=$INCREASE_CURRENT_PIXEL"
  log "font_increase_grid=$INCREASE_GRID"
  log "font_increase_cell=$INCREASE_CELL"
  log "font_increase_frame=$INCREASE_FRAME"
  log "font_increase_appkit_pixel=$INCREASE_PIXEL"
  log "PASS: font-increase current AppKit pixel matched presented pixels"
  INCREASE_PIXEL_WIDTH="${INCREASE_PIXEL%x*}"
  INCREASE_PIXEL_HEIGHT="${INCREASE_PIXEL#*x}"
  require_log_after "$INCREASE_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${A_PANE_ID} .*appkit_pixel=${INCREASE_PIXEL}" "Zig records font-increased AppKit presented pixel size"
  require_trace_after "$INCREASE_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${INCREASE_PIXEL_WIDTH} pixel_height=${INCREASE_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied font-increase resize to AppKit pixel size"

  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_FONT_INCREASE"
  log "font_increase_screenshot_exit=$?"
  INCREASE_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve font-increased window bounds"
  INCREASE_HIT_START_LINE="$(log_line_count)"
  click_window_center "$INCREASE_WIN_LINE" "font_increase_browser_area"
  INCREASE_HIT_LINE="$(wait_for_hit_after "$INCREASE_HIT_START_LINE" "$A_CONTEXT_ID" "font-increased browser hit-test")"
  require_text "$INCREASE_HIT_LINE" "window_id:${A_WINDOW_ID}" "font-increased hit-test has window id"
  require_text "$INCREASE_HIT_LINE" "surface_id:${A_SURFACE_ID}" "font-increased hit-test has surface id"
  require_text "$INCREASE_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "font-increased hit-test has selected tab id"
  require_text "$INCREASE_HIT_LINE" "overlay_frame=${INCREASE_FRAME}" "font-increased hit-test uses current AppKit frame"
  require_text "$INCREASE_HIT_LINE" "web_point={" "font-increased hit-test includes webview-relative point"

  INCREASE_MODE_START_LINE="$(log_line_count)"
  INCREASE_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "font_increase_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$INCREASE_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "font-increased webtui entered browse mode"
  require_trace_after "$INCREASE_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed focus=true after font increase"
  INCREASE_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP20_FONT_INCREASE\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$INCREASE_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "font-increased keyboard marker reached browser"

  INCREASE_CONTROL_START_LINE="$(log_line_count)"
  INCREASE_CONTROL_TRACE_START_LINE="$(trace_line_count)"
  log "font_increase_control_key=escape=Mode::Control"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$INCREASE_CONTROL_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=false" "font-increased webtui returned to control mode"
  require_trace_after "$INCREASE_CONTROL_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed focus=false before font decrease"

  DECREASE_START_LINE="$(log_line_count)"
  DECREASE_TRACE_START_LINE="$(trace_line_count)"
  log "font_decrease_keybind=ctrl+y=decrease_font_size:2"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 16 control >>"$HARNESS_LOG" 2>&1
  delay 1
  DECREASE_PRESENT_LINE="$(wait_for_exact_appkit_frame_after "$DECREASE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "font-decreased AppKit overlay frame" 45)"
  DECREASE_PIXELS_LINE="$(wait_for_exact_appkit_pixels_after "$DECREASE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "font-decreased AppKit pixels" 45)"
  DECREASE_WINDOW_ID="$(printf '%s\n' "$DECREASE_PRESENT_LINE" | sed -E 's/.*window_id:([^ ]+) .*/\1/')"
  DECREASE_SURFACE_ID="$(extract_surface_id "$DECREASE_PRESENT_LINE")"
  DECREASE_SELECTED_TAB_ID="$(extract_selected_tab_id "$DECREASE_PRESENT_LINE")"
  DECREASE_GRID="$(extract_grid "$DECREASE_PRESENT_LINE")"
  DECREASE_CELL="$(extract_cell_size "$DECREASE_PRESENT_LINE")"
  DECREASE_FRAME="$(extract_overlay_frame "$DECREASE_PRESENT_LINE")"
  DECREASE_PIXEL="$(extract_appkit_pixel "$DECREASE_PIXELS_LINE")"
  DECREASE_SCALE="$(extract_backing_scale "$DECREASE_PRESENT_LINE")"
  DECREASE_CURRENT_PIXEL="$(appkit_pixel_from_geometry_line "$DECREASE_PRESENT_LINE")"
  [ "$DECREASE_WINDOW_ID" = "$A_WINDOW_ID" ] || fail "font-decrease window id changed unexpectedly: expected=$A_WINDOW_ID actual=$DECREASE_WINDOW_ID"
  [ "$DECREASE_SURFACE_ID" = "$A_SURFACE_ID" ] || fail "font-decrease surface id changed"
  [ "$DECREASE_SELECTED_TAB_ID" = "$A_SELECTED_TAB_ID" ] || fail "font-decrease selected tab id changed"
  [ "$DECREASE_GRID" = "$A_GRID" ] || fail "font-decrease grid did not return to baseline: expected=$A_GRID actual=$DECREASE_GRID"
  [ "$DECREASE_CELL" = "$A_CELL" ] || fail "font-decrease cell did not return to baseline: expected=$A_CELL actual=$DECREASE_CELL"
  [ "$DECREASE_SCALE" = "$A_BACKING_SCALE" ] || fail "font-decrease backing scale mismatch: expected=$A_BACKING_SCALE actual=$DECREASE_SCALE"
  [ "$DECREASE_CURRENT_PIXEL" = "$DECREASE_PIXEL" ] || fail "font-decrease current frame-derived pixel mismatch: expected=$DECREASE_PIXEL actual=$DECREASE_CURRENT_PIXEL"
  log "font_decrease_grid=$DECREASE_GRID"
  log "font_decrease_cell=$DECREASE_CELL"
  log "font_decrease_frame=$DECREASE_FRAME"
  log "font_decrease_appkit_pixel=$DECREASE_PIXEL"
  log "PASS: font-decrease current AppKit pixel matched presented pixels"
  DECREASE_PIXEL_WIDTH="${DECREASE_PIXEL%x*}"
  DECREASE_PIXEL_HEIGHT="${DECREASE_PIXEL#*x}"
  require_log_after "$DECREASE_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${A_PANE_ID} .*appkit_pixel=${DECREASE_PIXEL}" "Zig records font-decreased AppKit presented pixel size"
  require_trace_after "$DECREASE_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${DECREASE_PIXEL_WIDTH} pixel_height=${DECREASE_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied font-decrease resize to AppKit pixel size"

  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_FONT_DECREASE"
  log "font_decrease_screenshot_exit=$?"
  DECREASE_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve font-decreased window bounds"
  DECREASE_HIT_START_LINE="$(log_line_count)"
  click_window_center "$DECREASE_WIN_LINE" "font_decrease_browser_area"
  DECREASE_HIT_LINE="$(wait_for_hit_after "$DECREASE_HIT_START_LINE" "$A_CONTEXT_ID" "font-decreased browser hit-test")"
  require_text "$DECREASE_HIT_LINE" "window_id:${A_WINDOW_ID}" "font-decreased hit-test has window id"
  require_text "$DECREASE_HIT_LINE" "surface_id:${A_SURFACE_ID}" "font-decreased hit-test has surface id"
  require_text "$DECREASE_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "font-decreased hit-test has selected tab id"
  require_text "$DECREASE_HIT_LINE" "overlay_frame=${DECREASE_FRAME}" "font-decreased hit-test uses current AppKit frame"
  require_text "$DECREASE_HIT_LINE" "web_point={" "font-decreased hit-test includes webview-relative point"

  DECREASE_MODE_START_LINE="$(log_line_count)"
  DECREASE_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "font_decrease_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$DECREASE_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "font-decreased webtui entered browse mode"
  require_trace_after "$DECREASE_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed focus=true after font decrease"
  DECREASE_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP20_FONT_DECREASE\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$DECREASE_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "font-decreased keyboard marker reached browser"
fi

if [ "$SCENARIO" = "tui-overlay-resize-command" ]; then
  A_WINDOW_ID="$WID"
  A_SURFACE_ID="$(extract_surface_id "$APPKIT_PRESENT_LINE")"
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_GRID="$(extract_grid "$APPKIT_PRESENT_LINE")"
  A_GRID_WIDTH="$(extract_grid_width "$A_GRID")"
  A_GRID_HEIGHT="$(extract_grid_height "$A_GRID")"
  A_CELL="$(extract_cell_size "$APPKIT_PRESENT_LINE")"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_FRAME_X="$OVERLAY_FRAME_X"
  A_FRAME_Y="$OVERLAY_FRAME_Y"
  A_FRAME_WIDTH="$(pair_width "$A_FRAME_SIZE")"
  A_FRAME_HEIGHT="$(pair_height "$A_FRAME_SIZE")"
  A_ROOT_FRAME_SIZE="$(extract_root_frame_size "$APPKIT_PRESENT_LINE")"
  A_PIXEL="$APPKIT_PIXEL"
  A_PIXEL_HEIGHT="${A_PIXEL#*x}"
  A_BACKING_SCALE="$(extract_backing_scale "$APPKIT_PRESENT_LINE")"
  log "tui_baseline_window_id=$A_WINDOW_ID"
  log "tui_baseline_surface_id=$A_SURFACE_ID"
  log "tui_baseline_selected_tab_id=$A_SELECTED_TAB_ID"
  log "tui_baseline_pane_id=$A_PANE_ID"
  log "tui_baseline_browser_tab_id=$A_BROWSER_TAB_ID"
  log "tui_baseline_context_id=$A_CONTEXT_ID"
  log "tui_baseline_grid=$A_GRID"
  log "tui_baseline_cell=$A_CELL"
  log "tui_baseline_frame=$A_FRAME"
  log "tui_baseline_appkit_pixel=$A_PIXEL"
  log "tui_baseline_backing_scale=$A_BACKING_SCALE"

  SHRINK_ROWS=12
  [ "$A_GRID_HEIGHT" -gt "$SHRINK_ROWS" ] || fail "baseline grid height is too small for TUI shrink test: baseline=$A_GRID"
  SHRINK_START_LINE="$(log_line_count)"
  SHRINK_TRACE_START_LINE="$(trace_line_count)"
  printf ':viewport height %s' "$SHRINK_ROWS" >"$TUI_VIEWPORT_SHRINK_COMMAND"
  log "tui_viewport_shrink_command_text=$(cat "$TUI_VIEWPORT_SHRINK_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$TUI_VIEWPORT_SHRINK_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  delay 1

  SHRINK_SET_OVERLAY_LINE="$(wait_for_line_after "$SHRINK_START_LINE" "TermSurf geometry layer=zig event=set_overlay_update .*pane_id:${A_PANE_ID} .*browser_tab_id:${A_BROWSER_TAB_ID} .*grid=${A_GRID_WIDTH}x${SHRINK_ROWS}\\+1\\+1 .*context_id=${A_CONTEXT_ID}" "TUI shrink SetOverlay update" 45)"
  SHRINK_PRESENT_LINE="$(wait_for_changed_appkit_frame_after "$SHRINK_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "TUI-shrunken AppKit overlay frame" 45)"
  SHRINK_PIXELS_LINE="$(wait_for_changed_appkit_pixels_after "$SHRINK_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "TUI-shrunken AppKit pixels" 45)"
  SHRINK_WINDOW_ID="$(printf '%s\n' "$SHRINK_PRESENT_LINE" | sed -E 's/.*window_id:([^ ]+) .*/\1/')"
  SHRINK_SURFACE_ID="$(extract_surface_id "$SHRINK_PRESENT_LINE")"
  SHRINK_SELECTED_TAB_ID="$(extract_selected_tab_id "$SHRINK_PRESENT_LINE")"
  SHRINK_GRID="$(extract_grid "$SHRINK_PRESENT_LINE")"
  SHRINK_GRID_WIDTH="$(extract_grid_width "$SHRINK_GRID")"
  SHRINK_GRID_HEIGHT="$(extract_grid_height "$SHRINK_GRID")"
  SHRINK_FRAME="$(extract_overlay_frame "$SHRINK_PRESENT_LINE")"
  SHRINK_FRAME_SIZE="$(extract_frame_size "$SHRINK_PRESENT_LINE")"
  SHRINK_FRAME_X="$(extract_frame_x "$SHRINK_PRESENT_LINE")"
  SHRINK_FRAME_Y="$(extract_frame_y "$SHRINK_PRESENT_LINE")"
  SHRINK_FRAME_WIDTH="$(pair_width "$SHRINK_FRAME_SIZE")"
  SHRINK_FRAME_HEIGHT="$(pair_height "$SHRINK_FRAME_SIZE")"
  SHRINK_PIXEL="$(extract_appkit_pixel "$SHRINK_PIXELS_LINE")"
  SHRINK_PIXEL_WIDTH="${SHRINK_PIXEL%x*}"
  SHRINK_PIXEL_HEIGHT="${SHRINK_PIXEL#*x}"
  SHRINK_SCALE="$(extract_backing_scale "$SHRINK_PRESENT_LINE")"
  SHRINK_CURRENT_PIXEL="$(appkit_pixel_from_geometry_line "$SHRINK_PRESENT_LINE")"
  [ "$SHRINK_WINDOW_ID" = "$A_WINDOW_ID" ] || fail "TUI shrink window id changed: expected=$A_WINDOW_ID actual=$SHRINK_WINDOW_ID"
  [ "$SHRINK_SURFACE_ID" = "$A_SURFACE_ID" ] || fail "TUI shrink surface id changed"
  [ "$SHRINK_SELECTED_TAB_ID" = "$A_SELECTED_TAB_ID" ] || fail "TUI shrink selected tab id changed"
  [ "$SHRINK_GRID_WIDTH" = "$A_GRID_WIDTH" ] || fail "TUI shrink grid width changed unexpectedly: expected=$A_GRID_WIDTH actual=$SHRINK_GRID_WIDTH"
  [ "$SHRINK_GRID_HEIGHT" = "$SHRINK_ROWS" ] || fail "TUI shrink grid height mismatch: expected=$SHRINK_ROWS actual=$SHRINK_GRID_HEIGHT"
  [ "$SHRINK_SCALE" = "$A_BACKING_SCALE" ] || fail "TUI shrink backing scale mismatch: expected=$A_BACKING_SCALE actual=$SHRINK_SCALE"
  [ "$SHRINK_CURRENT_PIXEL" = "$SHRINK_PIXEL" ] || fail "TUI shrink current frame-derived pixel mismatch: expected=$SHRINK_PIXEL actual=$SHRINK_CURRENT_PIXEL"
  [ "$SHRINK_FRAME_HEIGHT" -lt "$A_FRAME_HEIGHT" ] || fail "TUI shrink frame height did not shrink: baseline=$A_FRAME_SIZE shrink=$SHRINK_FRAME_SIZE"
  [ "$SHRINK_PIXEL_HEIGHT" -lt "$A_PIXEL_HEIGHT" ] || fail "TUI shrink AppKit pixel height did not shrink: baseline=$A_PIXEL shrink=$SHRINK_PIXEL"
  log "tui_shrink_set_overlay=$SHRINK_SET_OVERLAY_LINE"
  log "tui_shrink_grid=$SHRINK_GRID"
  log "tui_shrink_frame=$SHRINK_FRAME"
  log "tui_shrink_appkit_pixel=$SHRINK_PIXEL"
  log "PASS: TUI command shrank SetOverlay, AppKit frame, and AppKit pixels"
  require_log_after "$SHRINK_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${A_PANE_ID} .*appkit_pixel=${SHRINK_PIXEL}" "Zig records TUI-shrunken AppKit presented pixel size"
  require_trace_after "$SHRINK_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${SHRINK_PIXEL_WIDTH} pixel_height=${SHRINK_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied TUI shrink resize to AppKit pixel size"
  SHRINK_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve TUI-shrunken window bounds"
  [ "$SHRINK_WIN_LINE" = "$WIN_LINE" ] || fail "TUI shrink changed window bounds: baseline=$WIN_LINE shrink=$SHRINK_WIN_LINE"
  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_TUI_SHRINK"
  log "tui_shrink_screenshot_exit=$?"

  IFS=$'\t' read -r _SHRINK_WID SHRINK_WX SHRINK_WY SHRINK_WW SHRINK_WH <<<"$SHRINK_WIN_LINE"
  SHRINK_ROOT_HEIGHT="$(pair_height "$A_ROOT_FRAME_SIZE")"
  SHRINK_CONTENT_Y_OFFSET="$(awk -v wh="$SHRINK_WH" -v root_h="$SHRINK_ROOT_HEIGHT" 'BEGIN { print int(wh - root_h) }')"
  SHRINK_INSIDE_X="$(awk -v wx="$SHRINK_WX" -v frame_x="$SHRINK_FRAME_X" -v frame_w="$SHRINK_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  SHRINK_INSIDE_Y="$(awk -v wy="$SHRINK_WY" -v content_y="$SHRINK_CONTENT_Y_OFFSET" -v frame_y="$SHRINK_FRAME_Y" -v frame_h="$SHRINK_FRAME_HEIGHT" 'BEGIN { print int(wy + content_y + frame_y + (frame_h / 2) + 0.5) }')"
  SHRINK_HIT_START_LINE="$(log_line_count)"
  click_global_point "$SHRINK_INSIDE_X" "$SHRINK_INSIDE_Y" "tui_shrink_inside"
  SHRINK_HIT_LINE="$(wait_for_hit_after "$SHRINK_HIT_START_LINE" "$A_CONTEXT_ID" "TUI-shrunken browser hit-test")"
  require_text "$SHRINK_HIT_LINE" "window_id:${A_WINDOW_ID}" "TUI-shrunken hit-test has window id"
  require_text "$SHRINK_HIT_LINE" "surface_id:${A_SURFACE_ID}" "TUI-shrunken hit-test has surface id"
  require_text "$SHRINK_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "TUI-shrunken hit-test has selected tab id"
  require_text "$SHRINK_HIT_LINE" "overlay_frame=${SHRINK_FRAME}" "TUI-shrunken hit-test uses current AppKit frame"
  require_text "$SHRINK_HIT_LINE" "web_point={" "TUI-shrunken hit-test includes webview-relative point"

  FORMER_LOWER_X="$SHRINK_INSIDE_X"
  FORMER_LOWER_Y="$(awk -v wy="$SHRINK_WY" -v content_y="$SHRINK_CONTENT_Y_OFFSET" -v frame_y="$A_FRAME_Y" -v shrink_h="$SHRINK_FRAME_HEIGHT" -v baseline_h="$A_FRAME_HEIGHT" 'BEGIN { print int(wy + content_y + frame_y + shrink_h + ((baseline_h - shrink_h) / 2) + 0.5) }')"
  click_negative_global_point "$FORMER_LOWER_X" "$FORMER_LOWER_Y" "tui_shrink_former_lower_browser_area"
  wait_for_negative_hit_after "$NEGATIVE_HIT_START_LINE" "$A_CONTEXT_ID" "TUI-shrunken former lower browser area hit-test" allow-absent

  SHRINK_MODE_START_LINE="$(log_line_count)"
  SHRINK_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "tui_shrink_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$SHRINK_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "TUI-shrunken webtui entered browse mode"
  require_trace_after "$SHRINK_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed focus=true after TUI shrink"
  SHRINK_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP21_TUI_SHRINK\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$SHRINK_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "TUI-shrunken keyboard marker reached browser"

  SHRINK_CONTROL_START_LINE="$(log_line_count)"
  SHRINK_CONTROL_TRACE_START_LINE="$(trace_line_count)"
  log "tui_shrink_control_key=escape=Mode::Control"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$SHRINK_CONTROL_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=false" "TUI-shrunken webtui returned to control mode"
  require_trace_after "$SHRINK_CONTROL_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed focus=false before TUI reset"

  RESET_START_LINE="$(log_line_count)"
  RESET_TRACE_START_LINE="$(trace_line_count)"
  printf ':viewport reset' >"$TUI_VIEWPORT_RESET_COMMAND"
  log "tui_viewport_reset_command_text=$(cat "$TUI_VIEWPORT_RESET_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$TUI_VIEWPORT_RESET_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  delay 1

  RESET_SET_OVERLAY_LINE="$(wait_for_line_after "$RESET_START_LINE" "TermSurf geometry layer=zig event=set_overlay_update .*pane_id:${A_PANE_ID} .*browser_tab_id:${A_BROWSER_TAB_ID} .*grid=${A_GRID_WIDTH}x${A_GRID_HEIGHT}\\+1\\+1 .*context_id=${A_CONTEXT_ID}" "TUI reset SetOverlay update" 45)"
  RESET_PRESENT_LINE="$(wait_for_exact_appkit_frame_after "$RESET_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "TUI-reset AppKit overlay frame" 45)"
  RESET_PIXELS_LINE="$(wait_for_exact_appkit_pixels_after "$RESET_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "TUI-reset AppKit pixels" 45)"
  RESET_WINDOW_ID="$(printf '%s\n' "$RESET_PRESENT_LINE" | sed -E 's/.*window_id:([^ ]+) .*/\1/')"
  RESET_SURFACE_ID="$(extract_surface_id "$RESET_PRESENT_LINE")"
  RESET_SELECTED_TAB_ID="$(extract_selected_tab_id "$RESET_PRESENT_LINE")"
  RESET_GRID="$(extract_grid "$RESET_PRESENT_LINE")"
  RESET_FRAME="$(extract_overlay_frame "$RESET_PRESENT_LINE")"
  RESET_PIXEL="$(extract_appkit_pixel "$RESET_PIXELS_LINE")"
  RESET_PIXEL_WIDTH="${RESET_PIXEL%x*}"
  RESET_PIXEL_HEIGHT="${RESET_PIXEL#*x}"
  RESET_SCALE="$(extract_backing_scale "$RESET_PRESENT_LINE")"
  RESET_CURRENT_PIXEL="$(appkit_pixel_from_geometry_line "$RESET_PRESENT_LINE")"
  [ "$RESET_WINDOW_ID" = "$A_WINDOW_ID" ] || fail "TUI reset window id changed: expected=$A_WINDOW_ID actual=$RESET_WINDOW_ID"
  [ "$RESET_SURFACE_ID" = "$A_SURFACE_ID" ] || fail "TUI reset surface id changed"
  [ "$RESET_SELECTED_TAB_ID" = "$A_SELECTED_TAB_ID" ] || fail "TUI reset selected tab id changed"
  [ "$RESET_GRID" = "$A_GRID" ] || fail "TUI reset grid mismatch: expected=$A_GRID actual=$RESET_GRID"
  [ "$RESET_PIXEL" = "$A_PIXEL" ] || fail "TUI reset AppKit pixels did not return to baseline: expected=$A_PIXEL actual=$RESET_PIXEL"
  [ "$RESET_SCALE" = "$A_BACKING_SCALE" ] || fail "TUI reset backing scale mismatch: expected=$A_BACKING_SCALE actual=$RESET_SCALE"
  [ "$RESET_CURRENT_PIXEL" = "$RESET_PIXEL" ] || fail "TUI reset current frame-derived pixel mismatch: expected=$RESET_PIXEL actual=$RESET_CURRENT_PIXEL"
  log "tui_reset_set_overlay=$RESET_SET_OVERLAY_LINE"
  log "tui_reset_grid=$RESET_GRID"
  log "tui_reset_frame=$RESET_FRAME"
  log "tui_reset_appkit_pixel=$RESET_PIXEL"
  log "PASS: TUI reset returned SetOverlay, AppKit frame, and AppKit pixels to baseline"
  require_trace_after "$RESET_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${RESET_PIXEL_WIDTH} pixel_height=${RESET_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied TUI reset resize to AppKit pixel size"
  RESET_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve TUI-reset window bounds"
  [ "$RESET_WIN_LINE" = "$WIN_LINE" ] || fail "TUI reset changed window bounds: baseline=$WIN_LINE reset=$RESET_WIN_LINE"
  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_TUI_RESET"
  log "tui_reset_screenshot_exit=$?"

  RESET_HIT_START_LINE="$(log_line_count)"
  click_window_center "$RESET_WIN_LINE" "tui_reset_browser_area"
  RESET_HIT_LINE="$(wait_for_hit_after "$RESET_HIT_START_LINE" "$A_CONTEXT_ID" "TUI-reset browser hit-test")"
  require_text "$RESET_HIT_LINE" "window_id:${A_WINDOW_ID}" "TUI-reset hit-test has window id"
  require_text "$RESET_HIT_LINE" "surface_id:${A_SURFACE_ID}" "TUI-reset hit-test has surface id"
  require_text "$RESET_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "TUI-reset hit-test has selected tab id"
  require_text "$RESET_HIT_LINE" "overlay_frame=${A_FRAME}" "TUI-reset hit-test uses baseline AppKit frame"
  require_text "$RESET_HIT_LINE" "web_point={" "TUI-reset hit-test includes webview-relative point"

  RESET_MODE_START_LINE="$(log_line_count)"
  RESET_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "tui_reset_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$RESET_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "TUI-reset webtui entered browse mode"
  require_trace_after "$RESET_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed focus=true after TUI reset"
  RESET_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP21_TUI_RESET\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$RESET_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "TUI-reset keyboard marker reached browser"

  [ "$SHRINK_TRACE_START_LINE" -lt "$SHRINK_MODE_TRACE_START_LINE" ] || fail "trace boundaries for TUI shrink were not monotonic"
  [ "$SHRINK_MODE_TRACE_START_LINE" -lt "$RESET_TRACE_START_LINE" ] || fail "trace boundaries for TUI reset were not monotonic"
fi

if [ "$SCENARIO" = "terminal-scrollback-movement" ]; then
  A_WINDOW_ID="$WID"
  A_SURFACE_ID="$(extract_surface_id "$APPKIT_PRESENT_LINE")"
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_GRID="$(extract_grid "$APPKIT_PRESENT_LINE")"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_FRAME_X="$OVERLAY_FRAME_X"
  A_FRAME_Y="$OVERLAY_FRAME_Y"
  A_FRAME_WIDTH="$(pair_width "$A_FRAME_SIZE")"
  A_FRAME_HEIGHT="$(pair_height "$A_FRAME_SIZE")"
  A_ROOT_FRAME_SIZE="$(extract_root_frame_size "$APPKIT_PRESENT_LINE")"
  A_PIXEL="$APPKIT_PIXEL"
  A_BACKING_SCALE="$(extract_backing_scale "$APPKIT_PRESENT_LINE")"
  log "scrollback_baseline_window_id=$A_WINDOW_ID"
  log "scrollback_baseline_surface_id=$A_SURFACE_ID"
  log "scrollback_baseline_selected_tab_id=$A_SELECTED_TAB_ID"
  log "scrollback_baseline_pane_id=$A_PANE_ID"
  log "scrollback_baseline_browser_tab_id=$A_BROWSER_TAB_ID"
  log "scrollback_baseline_context_id=$A_CONTEXT_ID"
  log "scrollback_baseline_grid=$A_GRID"
  log "scrollback_baseline_frame=$A_FRAME"
  log "scrollback_baseline_appkit_pixel=$A_PIXEL"
  log "scrollback_baseline_backing_scale=$A_BACKING_SCALE"

  BASE_SCROLL_LINE="$(wait_for_line_after 0 "TermSurf geometry layer=scrollview .*pane_id:${A_PANE_ID} .*scrollbar_total=[0-9]+ .*scrollbar_offset=[0-9]+ .*scrollbar_len=[0-9]+ .*derived_row=[0-9]+" "baseline scrollback visible-rect evidence" 45)"
  BASE_SCROLL_ROW="$(extract_scrollback_row "$BASE_SCROLL_LINE")"
  BASE_SCROLL_TOTAL="$(extract_scrollback_total "$BASE_SCROLL_LINE")"
  BASE_SCROLL_LEN="$(extract_scrollback_len "$BASE_SCROLL_LINE")"
  [ "$BASE_SCROLL_TOTAL" -gt "$BASE_SCROLL_LEN" ] || fail "scrollback total is not larger than viewport length: total=$BASE_SCROLL_TOTAL len=$BASE_SCROLL_LEN"
  [ "$BASE_SCROLL_ROW" -gt 0 ] || fail "baseline scrollback row did not prove primary-screen history: row=$BASE_SCROLL_ROW"
  log "scrollback_baseline_evidence=$BASE_SCROLL_LINE"
  log "scrollback_baseline_row=$BASE_SCROLL_ROW"
  log "scrollback_total=$BASE_SCROLL_TOTAL"
  log "scrollback_len=$BASE_SCROLL_LEN"

  SCROLL_UP_START_LINE="$(log_line_count)"
  SCROLL_UP_TRACE_START_LINE="$(trace_line_count)"
  log "scrollback_up_keybind=ctrl+u=scroll_page_up"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 32 control >>"$HARNESS_LOG" 2>&1
  delay 1
  SCROLLED_LINE="$(wait_for_scrollback_row_less_after "$SCROLL_UP_START_LINE" "$A_PANE_ID" "$BASE_SCROLL_ROW" "scrolled-back visible row" 45)"
  SCROLLED_ROW="$(extract_scrollback_row "$SCROLLED_LINE")"
  log "scrollback_scrolled_evidence=$SCROLLED_LINE"
  log "scrollback_scrolled_row=$SCROLLED_ROW"
  require_no_different_appkit_frame_after "$SCROLL_UP_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "scrolled-back AppKit frame stayed stable"
  require_no_different_appkit_pixels_after "$SCROLL_UP_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "scrolled-back AppKit pixels stayed stable"
  require_no_trace_after "$SCROLL_UP_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID}" "scrolled-back terminal scrollback did not resize Chromium"
  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_SCROLLBACK_UP"
  log "scrollback_up_screenshot_exit=$?"

  SCROLL_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve scrolled-back window bounds"
  [ "$SCROLL_WIN_LINE" = "$WIN_LINE" ] || fail "scrollback movement changed window bounds: baseline=$WIN_LINE scrolled=$SCROLL_WIN_LINE"
  IFS=$'\t' read -r _SCROLL_WID SCROLL_WX SCROLL_WY SCROLL_WW SCROLL_WH <<<"$SCROLL_WIN_LINE"
  SCROLL_ROOT_HEIGHT="$(pair_height "$A_ROOT_FRAME_SIZE")"
  SCROLL_CONTENT_Y_OFFSET="$(awk -v wh="$SCROLL_WH" -v root_h="$SCROLL_ROOT_HEIGHT" 'BEGIN { print int(wh - root_h) }')"
  SCROLL_INSIDE_X="$(awk -v wx="$SCROLL_WX" -v frame_x="$A_FRAME_X" -v frame_w="$A_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  SCROLL_INSIDE_Y="$(awk -v wy="$SCROLL_WY" -v content_y="$SCROLL_CONTENT_Y_OFFSET" -v frame_y="$A_FRAME_Y" -v frame_h="$A_FRAME_HEIGHT" 'BEGIN { print int(wy + content_y + frame_y + (frame_h / 2) + 0.5) }')"
  SCROLL_HIT_START_LINE="$(log_line_count)"
  click_global_point "$SCROLL_INSIDE_X" "$SCROLL_INSIDE_Y" "scrollback_inside"
  SCROLL_HIT_LINE="$(wait_for_hit_after "$SCROLL_HIT_START_LINE" "$A_CONTEXT_ID" "scrolled-back browser hit-test")"
  require_text "$SCROLL_HIT_LINE" "window_id:${A_WINDOW_ID}" "scrolled-back hit-test has window id"
  require_text "$SCROLL_HIT_LINE" "surface_id:${A_SURFACE_ID}" "scrolled-back hit-test has surface id"
  require_text "$SCROLL_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "scrolled-back hit-test has selected tab id"
  require_text "$SCROLL_HIT_LINE" "overlay_frame=${A_FRAME}" "scrolled-back hit-test uses baseline AppKit frame"
  require_text "$SCROLL_HIT_LINE" "web_point={" "scrolled-back hit-test includes webview-relative point"

  OUTSIDE_X="$SCROLL_INSIDE_X"
  OUTSIDE_Y="$(awk -v wy="$SCROLL_WY" -v content_y="$SCROLL_CONTENT_Y_OFFSET" -v frame_y="$A_FRAME_Y" -v frame_h="$A_FRAME_HEIGHT" 'BEGIN { print int(wy + content_y + frame_y + frame_h + 20) }')"
  click_negative_global_point "$OUTSIDE_X" "$OUTSIDE_Y" "scrollback_outside_overlay"
  wait_for_negative_hit_after "$NEGATIVE_HIT_START_LINE" "$A_CONTEXT_ID" "scrolled-back outside-overlay hit-test"

  SCROLL_MODE_START_LINE="$(log_line_count)"
  SCROLL_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "scrollback_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$SCROLL_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "scrolled-back webtui entered browse mode"
  require_trace_after "$SCROLL_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed focus=true while scrolled back"
  SCROLL_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP22_SCROLLBACK_UP\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$SCROLL_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "scrolled-back keyboard marker reached browser"
  require_no_scrollback_row_at_least_after "$SCROLL_MODE_START_LINE" "$A_PANE_ID" "$BASE_SCROLL_ROW" "keyboard while scrolled back did not force terminal to bottom"

  SCROLL_CONTROL_START_LINE="$(log_line_count)"
  SCROLL_CONTROL_TRACE_START_LINE="$(trace_line_count)"
  log "scrollback_control_key=escape=Mode::Control"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$SCROLL_CONTROL_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=false" "scrolled-back webtui returned to control mode"
  require_trace_after "$SCROLL_CONTROL_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed focus=false before scrollback bottom"

  BOTTOM_START_LINE="$(log_line_count)"
  BOTTOM_TRACE_START_LINE="$(trace_line_count)"
  log "scrollback_bottom_keybind=ctrl+b=scroll_to_bottom"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 11 control >>"$HARNESS_LOG" 2>&1
  delay 1
  BOTTOM_LINE="$(wait_for_scrollback_row_at_least_after "$BOTTOM_START_LINE" "$A_PANE_ID" "$BASE_SCROLL_ROW" "returned-to-bottom visible row" 45)"
  BOTTOM_ROW="$(extract_scrollback_row "$BOTTOM_LINE")"
  log "scrollback_bottom_evidence=$BOTTOM_LINE"
  log "scrollback_bottom_row=$BOTTOM_ROW"
  require_no_different_appkit_frame_after "$BOTTOM_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "returned-to-bottom AppKit frame stayed stable"
  require_no_different_appkit_pixels_after "$BOTTOM_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "returned-to-bottom AppKit pixels stayed stable"
  require_no_trace_after "$BOTTOM_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID}" "returned-to-bottom terminal scrollback did not resize Chromium"
  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_SCROLLBACK_BOTTOM"
  log "scrollback_bottom_screenshot_exit=$?"

  BOTTOM_HIT_START_LINE="$(log_line_count)"
  click_window_center "$SCROLL_WIN_LINE" "scrollback_bottom_browser_area"
  BOTTOM_HIT_LINE="$(wait_for_hit_after "$BOTTOM_HIT_START_LINE" "$A_CONTEXT_ID" "returned-to-bottom browser hit-test")"
  require_text "$BOTTOM_HIT_LINE" "window_id:${A_WINDOW_ID}" "returned-to-bottom hit-test has window id"
  require_text "$BOTTOM_HIT_LINE" "surface_id:${A_SURFACE_ID}" "returned-to-bottom hit-test has surface id"
  require_text "$BOTTOM_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "returned-to-bottom hit-test has selected tab id"
  require_text "$BOTTOM_HIT_LINE" "overlay_frame=${A_FRAME}" "returned-to-bottom hit-test uses baseline AppKit frame"
  require_text "$BOTTOM_HIT_LINE" "web_point={" "returned-to-bottom hit-test includes webview-relative point"

  BOTTOM_MODE_START_LINE="$(log_line_count)"
  BOTTOM_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "scrollback_bottom_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$BOTTOM_MODE_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "returned-to-bottom webtui entered browse mode"
  require_trace_after "$BOTTOM_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed focus=true after return to bottom"
  BOTTOM_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP22_SCROLLBACK_BOTTOM\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$BOTTOM_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "returned-to-bottom keyboard marker reached browser"

  [ "$SCROLL_UP_TRACE_START_LINE" -lt "$SCROLL_MODE_TRACE_START_LINE" ] || fail "trace boundaries for scrollback-up were not monotonic"
  [ "$SCROLL_MODE_TRACE_START_LINE" -lt "$BOTTOM_TRACE_START_LINE" ] || fail "trace boundaries for scrollback-bottom were not monotonic"
fi

if [ "$SCENARIO" = "browser-navigation-geometry" ]; then
  A_WINDOW_ID="$WID"
  A_SURFACE_ID="$(extract_surface_id "$APPKIT_PRESENT_LINE")"
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_GRID="$(extract_grid "$APPKIT_PRESENT_LINE")"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_FRAME_X="$OVERLAY_FRAME_X"
  A_FRAME_Y="$OVERLAY_FRAME_Y"
  A_FRAME_WIDTH="$(pair_width "$A_FRAME_SIZE")"
  A_FRAME_HEIGHT="$(pair_height "$A_FRAME_SIZE")"
  A_ROOT_FRAME_SIZE="$(extract_root_frame_size "$APPKIT_PRESENT_LINE")"
  A_PIXEL="$APPKIT_PIXEL"
  A_PIXEL_WIDTH="${A_PIXEL%x*}"
  A_PIXEL_HEIGHT="${A_PIXEL#*x}"
  A_BACKING_SCALE="$(extract_backing_scale "$APPKIT_PRESENT_LINE")"
  log "navigation_baseline_window_id=$A_WINDOW_ID"
  log "navigation_baseline_surface_id=$A_SURFACE_ID"
  log "navigation_baseline_selected_tab_id=$A_SELECTED_TAB_ID"
  log "navigation_baseline_pane_id=$A_PANE_ID"
  log "navigation_baseline_browser_tab_id=$A_BROWSER_TAB_ID"
  log "navigation_baseline_context_id=$A_CONTEXT_ID"
  log "navigation_baseline_grid=$A_GRID"
  log "navigation_baseline_frame=$A_FRAME"
  log "navigation_baseline_appkit_pixel=$A_PIXEL"
  log "navigation_baseline_backing_scale=$A_BACKING_SCALE"

  NAV_MARKER="termsurf_issue809_exp23=${TS}"
  printf '?%s' "$NAV_MARKER" >"$NAVIGATION_APPEND_COMMAND"
  log "navigation_append_command_text=$(cat "$NAVIGATION_APPEND_COMMAND")"
  NAV_START_LINE="$(log_line_count)"
  NAV_TRACE_START_LINE="$(trace_line_count)"
  NAV_STATE_START_LINE="$(state_trace_line_count)"
  log "navigation_app_start_line=$NAV_START_LINE"
  log "navigation_trace_start_line=$NAV_TRACE_START_LINE"
  log "navigation_state_start_line=$NAV_STATE_START_LINE"
  log "navigation_edit_key=shift+a=edit-url-end"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 0 shift >>"$HARNESS_LOG" 2>&1
  delay 0.5
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$NAVIGATION_APPEND_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  delay 1

  wait_for_state_trace_after "$NAV_STATE_START_LINE" "event=url_changed[[:space:]]+url=.*${NAV_MARKER}" "webtui URL state contains navigation marker" 45
  NAV_STATE_MARKER_LINE="$(tail -n +"$((NAV_STATE_START_LINE + 1))" "$WEBTUI_STATE_TRACE" | grep -n -E "event=url_changed[[:space:]]+url=.*${NAV_MARKER}" | tail -1 || true)"
  NAV_APP_URL_CHANGED_LINE="$(wait_for_line_after "$NAV_START_LINE" "TermSurf message decoded type=UrlChanged" "Ghostboard decoded UrlChanged after browser navigation" 45)"
  NAV_APP_MARKER_LINE="$(wait_for_line_after "$NAV_START_LINE" "navigation-throttles .*url=.*${NAV_MARKER}" "app Chromium navigation log contains marker" 45)"
  NAV_APP_NAVIGATE_LINE="$(tail -n +"$((NAV_START_LINE + 1))" "$APP_LOG" | grep -n "TermSurf message decoded type=Navigate" | head -1 || true)"
  NAV_TRACE_NAVIGATE_LINE="$(tail -n +"$((NAV_TRACE_START_LINE + 1))" "$CHROMIUM_TRACE" | grep -n -F "navigate tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} url=" | head -1 || true)"
  NAV_TRACE_MARKER_LINE="$(tail -n +"$((NAV_TRACE_START_LINE + 1))" "$CHROMIUM_TRACE" | grep -n -F "$NAV_MARKER" | head -1 || true)"
  NAV_TRACE_URL_CHANGED_LINE="$(tail -n +"$((NAV_TRACE_START_LINE + 1))" "$CHROMIUM_TRACE" | grep -n -F "url-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} url=" | head -1 || true)"
  log "navigation_state_marker_line=${NAV_STATE_MARKER_LINE:-<none>}"
  log "navigation_app_url_changed_line=$NAV_APP_URL_CHANGED_LINE"
  log "navigation_app_marker_line=$NAV_APP_MARKER_LINE"
  log "navigation_app_decoded_navigate_line=${NAV_APP_NAVIGATE_LINE:-<none>}"
  log "navigation_chromium_navigate_line=${NAV_TRACE_NAVIGATE_LINE:-<none>}"
  log "navigation_chromium_marker_line=${NAV_TRACE_MARKER_LINE:-<none>}"
  log "navigation_chromium_url_changed_line=${NAV_TRACE_URL_CHANGED_LINE:-<none>}"
  require_text "$NAV_STATE_MARKER_LINE" "$NAV_MARKER" "webtui URL state includes navigation marker"
  require_text "$NAV_APP_MARKER_LINE" "$NAV_MARKER" "app Chromium navigation log includes marker"
  wait_for_log_after "$NAV_START_LINE" "ModeChanged: pane_id=${A_PANE_ID} browsing=true" "webtui returned to browse mode after navigation" 45
  require_no_different_appkit_frame_after "$NAV_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME" "browser navigation AppKit frame stayed stable"
  require_no_different_appkit_pixels_after "$NAV_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "browser navigation AppKit pixels stayed stable"
  require_no_trace_after "$NAV_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID}" "browser navigation did not resize Chromium"
  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_NAVIGATED"
  log "navigation_screenshot_exit=$?"

  NAV_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve post-navigation window bounds"
  [ "$NAV_WIN_LINE" = "$WIN_LINE" ] || fail "browser navigation changed window bounds: baseline=$WIN_LINE navigated=$NAV_WIN_LINE"
  IFS=$'\t' read -r _NAV_WID NAV_WX NAV_WY NAV_WW NAV_WH <<<"$NAV_WIN_LINE"
  NAV_ROOT_HEIGHT="$(pair_height "$A_ROOT_FRAME_SIZE")"
  NAV_CONTENT_Y_OFFSET="$(awk -v wh="$NAV_WH" -v root_h="$NAV_ROOT_HEIGHT" 'BEGIN { print int(wh - root_h) }')"
  NAV_INSIDE_X="$(awk -v wx="$NAV_WX" -v frame_x="$A_FRAME_X" -v frame_w="$A_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  NAV_INSIDE_Y="$(awk -v wy="$NAV_WY" -v content_y="$NAV_CONTENT_Y_OFFSET" -v frame_y="$A_FRAME_Y" -v frame_h="$A_FRAME_HEIGHT" 'BEGIN { print int(wy + content_y + frame_y + (frame_h / 2) + 0.5) }')"
  NAV_HIT_START_LINE="$(log_line_count)"
  click_global_point "$NAV_INSIDE_X" "$NAV_INSIDE_Y" "navigation_inside"
  NAV_HIT_LINE="$(wait_for_hit_after "$NAV_HIT_START_LINE" "$A_CONTEXT_ID" "post-navigation browser hit-test")"
  require_text "$NAV_HIT_LINE" "window_id:${A_WINDOW_ID}" "post-navigation hit-test has window id"
  require_text "$NAV_HIT_LINE" "surface_id:${A_SURFACE_ID}" "post-navigation hit-test has surface id"
  require_text "$NAV_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "post-navigation hit-test has selected tab id"
  require_text "$NAV_HIT_LINE" "overlay_frame=${A_FRAME}" "post-navigation hit-test uses baseline AppKit frame"
  require_text "$NAV_HIT_LINE" "web_point={" "post-navigation hit-test includes webview-relative point"

  NAV_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP23_NAVIGATION\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$NAV_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "post-navigation keyboard marker reached browser"
fi

if [ "$SCENARIO" = "devtools-split-geometry" ] || [ "$SCENARIO" = "devtools-singleton-guard" ]; then
  A_WINDOW_ID="$WID"
  A_SURFACE_ID="$(extract_surface_id "$APPKIT_PRESENT_LINE")"
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_GRID="$(extract_grid "$APPKIT_PRESENT_LINE")"
  A_FRAME="$OVERLAY_FRAME"
  A_FRAME_SIZE="$OVERLAY_FRAME_SIZE"
  A_PIXEL="$APPKIT_PIXEL"
  A_BACKING_SCALE="$(extract_backing_scale "$APPKIT_PRESENT_LINE")"
  log "devtools_normal_baseline_window_id=$A_WINDOW_ID"
  log "devtools_normal_baseline_surface_id=$A_SURFACE_ID"
  log "devtools_normal_baseline_selected_tab_id=$A_SELECTED_TAB_ID"
  log "devtools_normal_baseline_pane_id=$A_PANE_ID"
  log "devtools_normal_baseline_browser_tab_id=$A_BROWSER_TAB_ID"
  log "devtools_normal_baseline_context_id=$A_CONTEXT_ID"
  log "devtools_normal_baseline_grid=$A_GRID"
  log "devtools_normal_baseline_frame=$A_FRAME"
  log "devtools_normal_baseline_appkit_pixel=$A_PIXEL"
  log "devtools_normal_baseline_backing_scale=$A_BACKING_SCALE"

  if [ "$SCENARIO" = "devtools-singleton-guard" ]; then
    GUI_SOCKET="$(sed -nE 's/.*TermSurf socket listening on (.*)$/\1/p' "$APP_LOG" | tail -1)"
    [ -S "$GUI_SOCKET" ] || fail "failed to resolve Ghostboard TermSurf socket: $GUI_SOCKET"
    log "devtools_gui_socket=$GUI_SOCKET"

    INFLIGHT_START_LINE="$(log_line_count)"
    INFLIGHT_FIRST_REPLY="$(devtools_probe "$GUI_SOCKET" "$A_PANE_ID" "$A_BROWSER_TAB_ID" "default" "$CHROMIUM")"
    log "devtools_inflight_first_reply=$INFLIGHT_FIRST_REPLY"
    case "$INFLIGHT_FIRST_REPLY" in
      *"tab=${A_BROWSER_TAB_ID}"*$'\t'"error="*) ;;
      *) fail "in-flight first query did not succeed: $INFLIGHT_FIRST_REPLY" ;;
    esac
    require_log_after "$INFLIGHT_START_LINE" "DevTools reservation: profile=default browser=${CHROMIUM} inspected_tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} timeout_ms=1000" "Ghostboard reserved in-flight DevTools target"

    INFLIGHT_DUPLICATE_REPLY="$(devtools_probe "$GUI_SOCKET" "$A_PANE_ID" "$A_BROWSER_TAB_ID" "default" "$CHROMIUM")"
    log "devtools_inflight_duplicate_reply=$INFLIGHT_DUPLICATE_REPLY"
    case "$INFLIGHT_DUPLICATE_REPLY" in
      *"error=Tab ${A_BROWSER_TAB_ID} already has DevTools open in ${CHROMIUM}/default"*) ;;
      *) fail "in-flight duplicate query was not rejected: $INFLIGHT_DUPLICATE_REPLY" ;;
    esac
    require_no_log_after "$INFLIGHT_START_LINE" "OpenSplit: pane_id=${A_PANE_ID} direction=right" "in-flight duplicate direct query did not open a split"
    require_no_log_after "$INFLIGHT_START_LINE" "SetDevtoolsOverlay: pane_id=.*inspected_tab_id=${A_BROWSER_TAB_ID}" "in-flight duplicate direct query did not create a DevTools overlay"
    require_no_log_after "$INFLIGHT_START_LINE" "CreateDevtoolsTab: pane_id=.*inspected_tab_id=${A_BROWSER_TAB_ID}" "in-flight duplicate direct query did not create a DevTools tab"

    delay 2
    INFLIGHT_TIMEOUT_REPLY="$(devtools_probe "$GUI_SOCKET" "$A_PANE_ID" "$A_BROWSER_TAB_ID" "default" "$CHROMIUM")"
    log "devtools_inflight_timeout_reply=$INFLIGHT_TIMEOUT_REPLY"
    case "$INFLIGHT_TIMEOUT_REPLY" in
      *"tab=${A_BROWSER_TAB_ID}"*$'\t'"error="*) ;;
      *) fail "query did not succeed after abandoned reservation timeout: $INFLIGHT_TIMEOUT_REPLY" ;;
    esac
    require_log_after "$INFLIGHT_START_LINE" "DevTools reservation expired: profile=default browser=${CHROMIUM} inspected_tab_id=${A_BROWSER_TAB_ID}" "Ghostboard expired abandoned DevTools reservation"
    delay 2
  fi

  DEVTOOLS_START_LINE="$(log_line_count)"
  DEVTOOLS_TRACE_START_LINE="$(trace_line_count)"
  DEVTOOLS_STATE_START_LINE="$(state_trace_line_count)"
  printf ':devtools right' >"$DEVTOOLS_COMMAND"
  log "devtools_command_text=$(cat "$DEVTOOLS_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$DEVTOOLS_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  delay 1

  wait_for_log_after "$DEVTOOLS_START_LINE" "TermSurf QueryDevtoolsRequest pane_id=${A_PANE_ID} inspected_tab_id=${A_BROWSER_TAB_ID}" "DevTools query request for normal browser tab" 45
  wait_for_log_after "$DEVTOOLS_START_LINE" "TermSurf QueryDevtoolsReply sent" "DevTools query reply" 45
  wait_for_log_after "$DEVTOOLS_START_LINE" "OpenSplit: pane_id=${A_PANE_ID} direction=right" "DevTools split opened to the right" 45
  DEVTOOLS_SET_LINE="$(wait_for_line_after "$DEVTOOLS_START_LINE" "SetDevtoolsOverlay: pane_id=[^ ]+ .*inspected_tab_id=${A_BROWSER_TAB_ID}" "DevTools SetDevtoolsOverlay" 60)"
  DT_PANE_ID="$(printf '%s\n' "$DEVTOOLS_SET_LINE" | sed -E 's/.*SetDevtoolsOverlay: pane_id=([^ ]+) .*/\1/')"
  [ -n "$DT_PANE_ID" ] || fail "failed to extract DevTools pane id"
  [ "$DT_PANE_ID" != "$A_PANE_ID" ] || fail "DevTools pane id reused normal browser pane id"
  log "devtools_pane_id=$DT_PANE_ID"
  wait_for_log_after "$DEVTOOLS_START_LINE" "CreateDevtoolsTab: pane_id=${DT_PANE_ID} inspected_tab_id=${A_BROWSER_TAB_ID}" "Ghostboard sent CreateDevtoolsTab for DevTools pane" 60

  A_SPLIT_PRESENT_LINE="$(wait_for_split_right_frame_after "$DEVTOOLS_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_FRAME_SIZE" "normal browser split-resized AppKit overlay frame" 60)"
  A_SPLIT_PIXELS_LINE="$(wait_for_split_right_pixels_after "$DEVTOOLS_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_PIXEL" "normal browser split-resized AppKit pixels" 60)"
  A_SPLIT_FRAME="$(extract_overlay_frame "$A_SPLIT_PRESENT_LINE")"
  A_SPLIT_FRAME_SIZE="$(extract_frame_size "$A_SPLIT_PRESENT_LINE")"
  A_SPLIT_FRAME_X="$(extract_frame_x "$A_SPLIT_PRESENT_LINE")"
  A_SPLIT_FRAME_Y="$(extract_frame_y "$A_SPLIT_PRESENT_LINE")"
  A_SPLIT_FRAME_WIDTH="$(pair_width "$A_SPLIT_FRAME_SIZE")"
  A_SPLIT_FRAME_HEIGHT="$(pair_height "$A_SPLIT_FRAME_SIZE")"
  A_SPLIT_ROOT_FRAME_WIDTH="$(pair_width "$(extract_root_frame_size "$A_SPLIT_PRESENT_LINE")")"
  A_SPLIT_PIXEL="$(extract_appkit_pixel "$A_SPLIT_PIXELS_LINE")"
  A_SPLIT_PIXEL_WIDTH="${A_SPLIT_PIXEL%x*}"
  A_SPLIT_PIXEL_HEIGHT="${A_SPLIT_PIXEL#*x}"
  log "devtools_normal_split_frame=$A_SPLIT_FRAME"
  log "devtools_normal_split_appkit_pixel=$A_SPLIT_PIXEL"
  require_trace_after "$DEVTOOLS_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${A_SPLIT_PIXEL_WIDTH} pixel_height=${A_SPLIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium resized normal browser after DevTools split"

  DT_CREATE_TRACE_LINE="$(optional_trace_line_after "$DEVTOOLS_TRACE_START_LINE" "create-devtools-tab pane=${DT_PANE_ID} inspected_tab_id=${A_BROWSER_TAB_ID} pixel_width=[0-9]+ pixel_height=[0-9]+")"
  DT_TAB_READY_TRACE_LINE="$(optional_trace_line_after "$DEVTOOLS_TRACE_START_LINE" "tab-ready tab=[0-9]+ pane=${DT_PANE_ID} inspected_tab_id=${A_BROWSER_TAB_ID}")"
  DT_TAB_READY_APP_LINE="$(wait_for_line_after "$DEVTOOLS_START_LINE" "TabReady: pane_id=${DT_PANE_ID} tab_id=[0-9]+" "Ghostboard mapped DevTools TabReady" 60)"
  DT_BROWSER_TAB_ID="$(extract_app_tab_id "$DT_TAB_READY_APP_LINE")"
  [ -n "$DT_BROWSER_TAB_ID" ] || fail "failed to extract DevTools browser tab id"
  [ "$DT_BROWSER_TAB_ID" != "$A_BROWSER_TAB_ID" ] || fail "DevTools browser tab id reused normal browser tab id"
  wait_for_state_trace_after "$DEVTOOLS_STATE_START_LINE" "event=render_state.*is_devtools=true.*current_tab_id=${DT_BROWSER_TAB_ID}.*inspected_tab_id=${A_BROWSER_TAB_ID}" "webtui mapped DevTools render state" 60
  log "devtools_create_trace=${DT_CREATE_TRACE_LINE:-none}"
  log "devtools_tab_ready_trace=${DT_TAB_READY_TRACE_LINE:-none}"
  log "devtools_tab_ready_app=$DT_TAB_READY_APP_LINE"
  log "devtools_browser_tab_id=$DT_BROWSER_TAB_ID"
  DT_CA_CONTEXT_APP_LINE="$(wait_for_line_after "$DEVTOOLS_START_LINE" "CaContext: tab_id=${DT_BROWSER_TAB_ID} pane_id=${DT_PANE_ID} context_id=[0-9]+" "Ghostboard received DevTools CA context" 60)"

  DT_PRESENT_LINE="$(wait_for_line_after "$DEVTOOLS_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${DT_PANE_ID} .*overlay_frame=\\{\\{.*context_id=[1-9][0-9]*" "DevTools AppKit overlay frame" 60)"
  DT_CONTEXT_ID="$(printf '%s\n' "$DT_PRESENT_LINE" | sed -E 's/.*context_id=([0-9]+) .*/\1/')"
  [ -n "$DT_CONTEXT_ID" ] || fail "failed to extract DevTools context id"
  [ "$DT_CONTEXT_ID" != "$A_CONTEXT_ID" ] || fail "DevTools context id reused normal browser context id"
  DT_SURFACE_ID="$(extract_surface_id "$DT_PRESENT_LINE")"
  DT_SELECTED_TAB_ID="$(extract_selected_tab_id "$DT_PRESENT_LINE")"
  DT_GRID="$(extract_grid "$DT_PRESENT_LINE")"
  DT_FRAME="$(extract_overlay_frame "$DT_PRESENT_LINE")"
  DT_FRAME_SIZE="$(extract_frame_size "$DT_PRESENT_LINE")"
  DT_FRAME_X="$(extract_frame_x "$DT_PRESENT_LINE")"
  DT_FRAME_Y="$(extract_frame_y "$DT_PRESENT_LINE")"
  DT_FRAME_WIDTH="$(pair_width "$DT_FRAME_SIZE")"
  DT_FRAME_HEIGHT="$(pair_height "$DT_FRAME_SIZE")"
  DT_BACKING_SCALE="$(extract_backing_scale "$DT_PRESENT_LINE")"
  DT_PIXELS_LINE="$(wait_for_line_after "$DEVTOOLS_START_LINE" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${DT_PANE_ID} .*appkit_pixel=[0-9]+x[0-9]+ .*context_id=${DT_CONTEXT_ID}" "DevTools AppKit presented pixels" 60)"
  DT_PIXEL="$(extract_appkit_pixel "$DT_PIXELS_LINE")"
  DT_PIXEL_WIDTH="${DT_PIXEL%x*}"
  DT_PIXEL_HEIGHT="${DT_PIXEL#*x}"
  log "devtools_surface_id=$DT_SURFACE_ID"
  log "devtools_selected_tab_id=$DT_SELECTED_TAB_ID"
  log "devtools_context_id=$DT_CONTEXT_ID"
  log "devtools_grid=$DT_GRID"
  log "devtools_frame=$DT_FRAME"
  log "devtools_appkit_pixel=$DT_PIXEL"
  log "devtools_backing_scale=$DT_BACKING_SCALE"
  require_text "$DT_CA_CONTEXT_APP_LINE" "context_id=${DT_CONTEXT_ID}" "DevTools CA context matches AppKit context"
  require_trace_after "$DEVTOOLS_TRACE_START_LINE" "resize tab_id=${DT_BROWSER_TAB_ID} pane_id=${DT_PANE_ID} pixel_width=${DT_PIXEL_WIDTH} pixel_height=${DT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium resized DevTools to AppKit pixel size"
  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_DEVTOOLS_SPLIT"
  log "devtools_split_screenshot_exit=$?"

  DT_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve DevTools window bounds"
  IFS=$'\t' read -r _DT_WID DT_WX DT_WY DT_WW DT_WH <<<"$DT_WIN_LINE"
  DT_ROOT_HEIGHT="$(pair_height "$(extract_root_frame_size "$DT_PRESENT_LINE")")"
  DT_CONTENT_Y_OFFSET="$(awk -v wh="$DT_WH" -v root_h="$DT_ROOT_HEIGHT" 'BEGIN { print int(wh - root_h) }')"
  A_SPLIT_INSIDE_X="$(awk -v wx="$DT_WX" -v frame_x="$A_SPLIT_FRAME_X" -v frame_w="$A_SPLIT_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  A_SPLIT_INSIDE_Y="$(awk -v wy="$DT_WY" -v content_y="$DT_CONTENT_Y_OFFSET" -v frame_y="$A_SPLIT_FRAME_Y" -v frame_h="$A_SPLIT_FRAME_HEIGHT" 'BEGIN { print int(wy + content_y + frame_y + (frame_h / 2) + 0.5) }')"
  DT_INSIDE_X="$(awk -v wx="$DT_WX" -v surface_x="$A_SPLIT_ROOT_FRAME_WIDTH" -v frame_x="$DT_FRAME_X" -v frame_w="$DT_FRAME_WIDTH" 'BEGIN { print int(wx + surface_x + frame_x + (frame_w / 2) + 0.5) }')"
  DT_INSIDE_Y="$(awk -v wy="$DT_WY" -v content_y="$DT_CONTENT_Y_OFFSET" -v frame_y="$DT_FRAME_Y" -v frame_h="$DT_FRAME_HEIGHT" 'BEGIN { print int(wy + content_y + frame_y + (frame_h / 2) + 0.5) }')"

  A_HIT_START_LINE="$(log_line_count)"
  A_HIT_TRACE_START_LINE="$(trace_line_count)"
  click_global_point "$A_SPLIT_INSIDE_X" "$A_SPLIT_INSIDE_Y" "devtools_normal_inside"
  A_HIT_LINE="$(wait_for_hit_after "$A_HIT_START_LINE" "$A_CONTEXT_ID" "normal browser hit-test after DevTools split")"
  require_text "$A_HIT_LINE" "overlay_frame=${A_SPLIT_FRAME}" "normal browser hit-test uses split frame"
  require_no_trace_after "$A_HIT_TRACE_START_LINE" "mouse-event tab=${DT_BROWSER_TAB_ID} pane=${DT_PANE_ID}" "normal browser click did not route to DevTools"

  DT_HIT_START_LINE="$(log_line_count)"
  DT_HIT_TRACE_START_LINE="$(trace_line_count)"
  click_global_point "$DT_INSIDE_X" "$DT_INSIDE_Y" "devtools_inside"
  DT_HIT_LINE="$(wait_for_hit_after "$DT_HIT_START_LINE" "$DT_CONTEXT_ID" "DevTools hit-test")"
  require_text "$DT_HIT_LINE" "overlay_frame=${DT_FRAME}" "DevTools hit-test uses DevTools frame"
  require_trace_after "$DT_HIT_TRACE_START_LINE" "mouse-move tab=${DT_BROWSER_TAB_ID} pane=${DT_PANE_ID}" "DevTools pointer move reached DevTools browser"
  require_no_trace_after "$DT_HIT_TRACE_START_LINE" "mouse-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "DevTools click did not route to normal browser"

  DT_MODE_TRACE_START_LINE="$(trace_line_count)"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  require_trace_after "$DT_MODE_TRACE_START_LINE" "focus-changed tab=${DT_BROWSER_TAB_ID} pane=${DT_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed DevTools focus=true"

  DT_CLICK_TRACE_START_LINE="$(trace_line_count)"
  click_global_point "$DT_INSIDE_X" "$DT_INSIDE_Y" "devtools_inside_focused"
  require_trace_after "$DT_CLICK_TRACE_START_LINE" "mouse-event tab=${DT_BROWSER_TAB_ID} pane=${DT_PANE_ID}" "focused DevTools click reached DevTools browser"
  require_no_trace_after "$DT_CLICK_TRACE_START_LINE" "mouse-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "focused DevTools click did not route to normal browser"

  DT_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP24_DEVTOOLS\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$DT_KEY_START_LINE" "key-event tab=${DT_BROWSER_TAB_ID} pane=${DT_PANE_ID}" "DevTools keyboard marker reached DevTools"
  require_no_trace_after "$DT_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "DevTools keyboard marker did not reach normal browser"

  swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
  A_MODE_TRACE_START_LINE="$(trace_line_count)"
  click_global_point "$A_SPLIT_INSIDE_X" "$A_SPLIT_INSIDE_Y" "devtools_normal_refocus"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  require_trace_after "$A_MODE_TRACE_START_LINE" "focus-changed tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed normal browser focus=true after DevTools"
  A_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP24_NORMAL\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  require_trace_after "$A_KEY_START_LINE" "key-event tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "normal browser keyboard marker reached normal browser"
  require_no_trace_after "$A_KEY_START_LINE" "key-event tab=${DT_BROWSER_TAB_ID} pane=${DT_PANE_ID}" "normal browser keyboard marker did not reach DevTools"

  if [ "$SCENARIO" = "devtools-singleton-guard" ]; then
    leave_browser_browse "devtools_singleton_normal_after_first" "$A_PANE_ID" "$A_BROWSER_TAB_ID"

    DIRECT_SET_START_LINE="$(log_line_count)"
    DIRECT_SET_PANE_ID="direct-duplicate-${A_BROWSER_TAB_ID}"
    log "devtools_direct_duplicate_set_pane_id=$DIRECT_SET_PANE_ID"
    devtools_overlay_probe "$GUI_SOCKET" "$DIRECT_SET_PANE_ID" "$A_BROWSER_TAB_ID" "default" "$CHROMIUM"
    wait_for_log_after "$DIRECT_SET_START_LINE" "SetDevtoolsOverlay: pane_id=${DIRECT_SET_PANE_ID} profile=default browser=${CHROMIUM} inspected_tab_id=${A_BROWSER_TAB_ID}" "direct duplicate SetDevtoolsOverlay reached Ghostboard" 45
    wait_for_log_after "$DIRECT_SET_START_LINE" "SetDevtoolsOverlay: duplicate target rejected pane_id=${DIRECT_SET_PANE_ID} existing_pane_id=${DT_PANE_ID} profile=default browser=${CHROMIUM} inspected_tab_id=${A_BROWSER_TAB_ID}" "direct duplicate SetDevtoolsOverlay rejected" 45
    require_no_log_after "$DIRECT_SET_START_LINE" "CreateDevtoolsTab: pane_id=${DIRECT_SET_PANE_ID} inspected_tab_id=${A_BROWSER_TAB_ID}" "direct duplicate SetDevtoolsOverlay did not create a DevTools tab"

    LIVE_DUPLICATE_START_LINE="$(log_line_count)"
    printf ':devtools right' >"$DEVTOOLS_COMMAND"
    log "devtools_live_duplicate_command_text=$(cat "$DEVTOOLS_COMMAND")"
    swift "$ROOT/scripts/ghostty-app/inject.swift" type "$DEVTOOLS_COMMAND" >>"$HARNESS_LOG" 2>&1
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
    wait_for_log_after "$LIVE_DUPLICATE_START_LINE" "TermSurf QueryDevtoolsRequest pane_id=${A_PANE_ID} inspected_tab_id=${A_BROWSER_TAB_ID}" "live duplicate DevTools query request for browser A" 45
    wait_for_log_after "$LIVE_DUPLICATE_START_LINE" "TermSurf QueryDevtoolsReply sent error=Tab ${A_BROWSER_TAB_ID} already has DevTools open in ${CHROMIUM}/default" "live duplicate DevTools query rejected" 45
    require_no_log_after "$LIVE_DUPLICATE_START_LINE" "OpenSplit: pane_id=${A_PANE_ID} direction=right" "live duplicate DevTools query did not open a split"
    require_no_log_after "$LIVE_DUPLICATE_START_LINE" "SetDevtoolsOverlay: pane_id=.*inspected_tab_id=${A_BROWSER_TAB_ID}" "live duplicate DevTools query did not create another overlay"
    require_no_log_after "$LIVE_DUPLICATE_START_LINE" "CreateDevtoolsTab: pane_id=.*inspected_tab_id=${A_BROWSER_TAB_ID}" "live duplicate DevTools query did not create another DevTools tab"

    DEVTOOLS_CLOSE_START_LINE="$(log_line_count)"
    DEVTOOLS_CLOSE_TRACE_START_LINE="$(trace_line_count)"
    click_global_point "$DT_INSIDE_X" "$DT_INSIDE_Y" "devtools_singleton_focus_before_close"
    log "devtools_singleton_close_keybind=ctrl+k=close_surface"
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 40 control >>"$HARNESS_LOG" 2>&1
    delay 1
    require_log_after "$DEVTOOLS_CLOSE_START_LINE" "Pane close cleanup: pane_id=${DT_PANE_ID} tab_id=${DT_BROWSER_TAB_ID}" "Ghostboard cleaned up first DevTools pane"
    require_log_after "$DEVTOOLS_CLOSE_START_LINE" "CloseTab: pane_id=${DT_PANE_ID} tab_id=${DT_BROWSER_TAB_ID}" "Ghostboard sent CloseTab for first DevTools pane"
    require_trace_after "$DEVTOOLS_CLOSE_TRACE_START_LINE" "close-tab tab_id=${DT_BROWSER_TAB_ID} pane_id=${DT_PANE_ID} result=destroying ffi=ts_destroy_web_contents" "Chromium destroyed first DevTools tab"
    require_trace_after "$DEVTOOLS_CLOSE_TRACE_START_LINE" "close-tab tab_id=${DT_BROWSER_TAB_ID} result=removed" "Chromium removed first DevTools tab"

    REOPEN_START_LINE="$(log_line_count)"
    REOPEN_TRACE_START_LINE="$(trace_line_count)"
    click_global_point "$A_SPLIT_INSIDE_X" "$A_SPLIT_INSIDE_Y" "devtools_singleton_normal_before_reopen"
    log "devtools_singleton_normal_before_reopen_control_key=escape"
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
    delay 1
    printf ':devtools right' >"$DEVTOOLS_COMMAND"
    log "devtools_reopen_command_text=$(cat "$DEVTOOLS_COMMAND")"
    swift "$ROOT/scripts/ghostty-app/inject.swift" type "$DEVTOOLS_COMMAND" >>"$HARNESS_LOG" 2>&1
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
    wait_for_log_after "$REOPEN_START_LINE" "TermSurf QueryDevtoolsRequest pane_id=${A_PANE_ID} inspected_tab_id=${A_BROWSER_TAB_ID}" "reopen DevTools query request for browser A" 45
    wait_for_log_after "$REOPEN_START_LINE" "TermSurf QueryDevtoolsReply sent error=" "reopen DevTools query succeeded" 45
    wait_for_log_after "$REOPEN_START_LINE" "OpenSplit: pane_id=${A_PANE_ID} direction=right" "reopen DevTools split opened to the right" 45
    DT2_SET_LINE="$(wait_for_line_after "$REOPEN_START_LINE" "SetDevtoolsOverlay: pane_id=[^ ]+ .*inspected_tab_id=${A_BROWSER_TAB_ID}" "reopen DevTools SetDevtoolsOverlay" 60)"
    DT2_PANE_ID="$(printf '%s\n' "$DT2_SET_LINE" | sed -E 's/.*SetDevtoolsOverlay: pane_id=([^ ]+) .*/\1/')"
    [ -n "$DT2_PANE_ID" ] || fail "failed to extract reopened DevTools pane id"
    [ "$DT2_PANE_ID" != "$DT_PANE_ID" ] || fail "reopened DevTools reused the closed DevTools pane id"
    log "devtools_reopened_pane_id=$DT2_PANE_ID"
    wait_for_log_after "$REOPEN_START_LINE" "CreateDevtoolsTab: pane_id=${DT2_PANE_ID} inspected_tab_id=${A_BROWSER_TAB_ID}" "Ghostboard sent CreateDevtoolsTab for reopened DevTools pane" 60
    DT2_TAB_READY_TRACE_LINE="$(optional_trace_line_after "$REOPEN_TRACE_START_LINE" "tab-ready tab=[0-9]+ pane=${DT2_PANE_ID} inspected_tab_id=${A_BROWSER_TAB_ID}")"
    DT2_TAB_READY_APP_LINE="$(wait_for_line_after "$REOPEN_START_LINE" "TabReady: pane_id=${DT2_PANE_ID} tab_id=[0-9]+" "Ghostboard mapped reopened DevTools TabReady" 60)"
    DT2_BROWSER_TAB_ID="$(extract_app_tab_id "$DT2_TAB_READY_APP_LINE")"
    [ -n "$DT2_BROWSER_TAB_ID" ] || fail "failed to extract reopened DevTools browser tab id"
    log "devtools_reopened_tab_ready_trace=${DT2_TAB_READY_TRACE_LINE:-none}"
    log "devtools_reopened_tab_ready_app=$DT2_TAB_READY_APP_LINE"
    log "devtools_reopened_browser_tab_id=$DT2_BROWSER_TAB_ID"
    wait_for_line_after "$REOPEN_START_LINE" "CaContext: tab_id=${DT2_BROWSER_TAB_ID} pane_id=${DT2_PANE_ID} context_id=[0-9]+" "Ghostboard received reopened DevTools CA context" 60

    B_TAB_START_LINE="$(log_line_count)"
    B_TAB_TRACE_START_LINE="$(trace_line_count)"
    log "devtools_singleton_new_tab_keybind=ctrl+t=new_tab"
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 17 control >>"$HARNESS_LOG" 2>&1
    delay 1
    require_log_after "$B_TAB_START_LINE" "dispatching action target=surface action=.new_tab" "DevTools singleton browser B native tab action dispatched"
    require_log_after "$B_TAB_START_LINE" 'starting command command=`/usr/bin/login`' "DevTools singleton browser B native tab started login shell"
    printf '"%s" --browser "%s" "%s"' "$WEB" "$CHROMIUM" "$URL_B" >"$SECOND_BROWSER_COMMAND"
    log "devtools_singleton_browser_b_command=$(cat "$SECOND_BROWSER_COMMAND")"
    swift "$ROOT/scripts/ghostty-app/inject.swift" type "$SECOND_BROWSER_COMMAND" >>"$HARNESS_LOG" 2>&1
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
    B_SET_LINE="$(wait_for_line_after "$B_TAB_START_LINE" "SetOverlay: pane_id=[^ ]+ profile=default browser=${CHROMIUM} url=${URL_B}" "DevTools singleton browser B SetOverlay" 60)"
    B_PANE_ID="$(printf '%s\n' "$B_SET_LINE" | sed -E 's/.*SetOverlay: pane_id=([^ ]+) .*/\1/')"
    [ -n "$B_PANE_ID" ] || fail "failed to extract DevTools singleton browser B pane id"
    [ "$B_PANE_ID" != "$A_PANE_ID" ] || fail "DevTools singleton browser B reused browser A pane id"
    log "devtools_singleton_browser_b_pane_id=$B_PANE_ID"
    B_TAB_READY_TRACE_LINE="$(optional_trace_line_after "$B_TAB_TRACE_START_LINE" "tab-ready tab=[0-9]+ pane=${B_PANE_ID} inspected_tab_id=0")"
    B_TAB_READY_APP_LINE="$(wait_for_line_after "$B_TAB_START_LINE" "TabReady: pane_id=${B_PANE_ID} tab_id=[0-9]+" "Ghostboard mapped DevTools singleton browser B TabReady" 60)"
    B_BROWSER_TAB_ID="$(extract_app_tab_id "$B_TAB_READY_APP_LINE")"
    [ -n "$B_BROWSER_TAB_ID" ] || fail "failed to extract DevTools singleton browser B tab id"
    [ "$B_BROWSER_TAB_ID" != "$A_BROWSER_TAB_ID" ] || fail "DevTools singleton browser B reused browser A tab id"
    log "devtools_singleton_browser_b_tab_ready_trace=${B_TAB_READY_TRACE_LINE:-none}"
    log "devtools_singleton_browser_b_tab_ready_app=$B_TAB_READY_APP_LINE"
    log "devtools_singleton_browser_b_tab_id=$B_BROWSER_TAB_ID"
    log "devtools_singleton_browser_b_before_devtools_control_key=escape"
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 53 >>"$HARNESS_LOG" 2>&1
    delay 1

    B_DEVTOOLS_START_LINE="$(log_line_count)"
    B_DEVTOOLS_TRACE_START_LINE="$(trace_line_count)"
    printf ':devtools right' >"$DEVTOOLS_COMMAND"
    log "devtools_singleton_browser_b_devtools_command=$(cat "$DEVTOOLS_COMMAND")"
    swift "$ROOT/scripts/ghostty-app/inject.swift" type "$DEVTOOLS_COMMAND" >>"$HARNESS_LOG" 2>&1
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
    wait_for_log_after "$B_DEVTOOLS_START_LINE" "TermSurf QueryDevtoolsRequest pane_id=${B_PANE_ID} inspected_tab_id=${B_BROWSER_TAB_ID}" "browser B DevTools query request while browser A DevTools is open" 45
    wait_for_log_after "$B_DEVTOOLS_START_LINE" "TermSurf QueryDevtoolsReply sent error=" "browser B DevTools query succeeded while browser A DevTools is open" 45
    wait_for_log_after "$B_DEVTOOLS_START_LINE" "OpenSplit: pane_id=${B_PANE_ID} direction=right" "browser B DevTools split opened while browser A DevTools is open" 45
    B_DT_SET_LINE="$(wait_for_line_after "$B_DEVTOOLS_START_LINE" "SetDevtoolsOverlay: pane_id=[^ ]+ .*inspected_tab_id=${B_BROWSER_TAB_ID}" "browser B DevTools SetDevtoolsOverlay" 60)"
    B_DT_PANE_ID="$(printf '%s\n' "$B_DT_SET_LINE" | sed -E 's/.*SetDevtoolsOverlay: pane_id=([^ ]+) .*/\1/')"
    [ -n "$B_DT_PANE_ID" ] || fail "failed to extract browser B DevTools pane id"
    log "devtools_singleton_browser_b_devtools_pane_id=$B_DT_PANE_ID"
    wait_for_log_after "$B_DEVTOOLS_START_LINE" "CreateDevtoolsTab: pane_id=${B_DT_PANE_ID} inspected_tab_id=${B_BROWSER_TAB_ID}" "Ghostboard sent CreateDevtoolsTab for browser B DevTools pane" 60
    B_DT_TAB_READY_TRACE_LINE="$(optional_trace_line_after "$B_DEVTOOLS_TRACE_START_LINE" "tab-ready tab=[0-9]+ pane=${B_DT_PANE_ID} inspected_tab_id=${B_BROWSER_TAB_ID}")"
    B_DT_TAB_READY_APP_LINE="$(wait_for_line_after "$B_DEVTOOLS_START_LINE" "TabReady: pane_id=${B_DT_PANE_ID} tab_id=[0-9]+" "Ghostboard mapped browser B DevTools TabReady" 60)"
    B_DT_BROWSER_TAB_ID="$(extract_app_tab_id "$B_DT_TAB_READY_APP_LINE")"
    [ -n "$B_DT_BROWSER_TAB_ID" ] || fail "failed to extract browser B DevTools browser tab id"
    log "devtools_singleton_browser_b_devtools_tab_ready_trace=${B_DT_TAB_READY_TRACE_LINE:-none}"
    log "devtools_singleton_browser_b_devtools_tab_ready_app=$B_DT_TAB_READY_APP_LINE"
    log "devtools_singleton_browser_b_devtools_tab_id=$B_DT_BROWSER_TAB_ID"
    wait_for_line_after "$B_DEVTOOLS_START_LINE" "CaContext: tab_id=${B_DT_BROWSER_TAB_ID} pane_id=${B_DT_PANE_ID} context_id=[0-9]+" "Ghostboard received browser B DevTools CA context" 60
  fi
fi

if [ "$SCENARIO" = "two-browser-split-routing" ]; then
  A_WINDOW_ID="$WID"
  A_SURFACE_ID="$(extract_surface_id "$APPKIT_PRESENT_LINE")"
  A_SELECTED_TAB_ID="$(extract_selected_tab_id "$APPKIT_PRESENT_LINE")"
  A_PANE_ID="$PANE_ID"
  A_BROWSER_TAB_ID="$BROWSER_TAB_ID"
  A_CONTEXT_ID="$CONTEXT_ID"
  A_BASE_FRAME="$OVERLAY_FRAME"
  A_BASE_PIXEL="$APPKIT_PIXEL"
  log "browser_a_window_id=$A_WINDOW_ID"
  log "browser_a_surface_id=$A_SURFACE_ID"
  log "browser_a_selected_tab_id=$A_SELECTED_TAB_ID"
  log "browser_a_pane_id=$A_PANE_ID"
  log "browser_a_browser_tab_id=$A_BROWSER_TAB_ID"
  log "browser_a_context_id=$A_CONTEXT_ID"

  require_log "SetOverlay: created pending server key=default/${CHROMIUM} pane_count=1" "browser A created default-profile server"
  A_SPAWN_LINE="$(grep -E "spawned browser path=${CHROMIUM} pid=[0-9]+ profile=default" "$APP_LOG" | tail -1 || true)"
  [ -n "$A_SPAWN_LINE" ] || fail "missing browser A Chromium spawn line"
  A_SPAWN_PID="$(printf '%s\n' "$A_SPAWN_LINE" | sed -E 's/.* pid=([0-9]+) profile=.*/\1/')"
  [ -n "$A_SPAWN_PID" ] || fail "failed to extract browser A Chromium pid"
  log "browser_a_spawn_pid=$A_SPAWN_PID"

  SPLIT_START_LINE="$(log_line_count)"
  SPLIT_TRACE_START_LINE="$(trace_line_count)"
  log "split_keybind=ctrl+d=new_split:right"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 2 control >>"$HARNESS_LOG" 2>&1
  delay 1

  require_log_after "$SPLIT_START_LINE" "dispatching action target=surface action=.new_split" "two-browser split action dispatched"
  require_log_after "$SPLIT_START_LINE" 'starting command command=`/usr/bin/login`' "two-browser split sibling started login shell"
  A_SPLIT_PRESENT_LINE="$(wait_for_split_right_frame_after "$SPLIT_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$OVERLAY_FRAME_SIZE" "browser A split AppKit overlay frame")"
  A_SPLIT_PIXELS_LINE="$(wait_for_split_right_pixels_after "$SPLIT_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$APPKIT_PIXEL" "browser A split AppKit pixels")"
  A_SPLIT_FRAME="$(extract_overlay_frame "$A_SPLIT_PRESENT_LINE")"
  A_SPLIT_FRAME_SIZE="$(extract_frame_size "$A_SPLIT_PRESENT_LINE")"
  A_SPLIT_FRAME_X="$(extract_frame_x "$A_SPLIT_PRESENT_LINE")"
  A_SPLIT_FRAME_Y="$(extract_frame_y "$A_SPLIT_PRESENT_LINE")"
  A_SPLIT_ROOT_X="$(extract_root_frame_x "$A_SPLIT_PRESENT_LINE")"
  A_SPLIT_ROOT_Y="$(extract_root_frame_y "$A_SPLIT_PRESENT_LINE")"
  A_SPLIT_ROOT_SIZE="$(extract_root_frame_size "$A_SPLIT_PRESENT_LINE")"
  A_SPLIT_PIXEL="$(extract_appkit_pixel "$A_SPLIT_PIXELS_LINE")"
  A_SPLIT_PIXEL_WIDTH="${A_SPLIT_PIXEL%x*}"
  A_SPLIT_PIXEL_HEIGHT="${A_SPLIT_PIXEL#*x}"
  log "browser_a_split_overlay_frame=$A_SPLIT_FRAME"
  log "browser_a_split_root_x=$A_SPLIT_ROOT_X"
  log "browser_a_split_root_y=$A_SPLIT_ROOT_Y"
  log "browser_a_split_root_size=$A_SPLIT_ROOT_SIZE"
  log "browser_a_split_appkit_pixel=$A_SPLIT_PIXEL"
  require_trace_after "$SPLIT_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${A_SPLIT_PIXEL_WIDTH} pixel_height=${A_SPLIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium resized browser A to split AppKit pixel size"

  BROWSER_B_START_LINE="$(log_line_count)"
  BROWSER_B_TRACE_START_LINE="$(trace_line_count)"
  printf '"%s" --browser "%s" --profile default "%s"' "$WEB" "$CHROMIUM" "$URL_B" >"$SECOND_BROWSER_COMMAND"
  log "browser_b_split_command=$(cat "$SECOND_BROWSER_COMMAND")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$SECOND_BROWSER_COMMAND" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1

  B_SET_LINE="$(wait_for_line_after "$BROWSER_B_START_LINE" "SetOverlay: pane_id=[^ ]+ profile=default browser=${CHROMIUM} url=${URL_B}" "browser B split SetOverlay" 60)"
  B_PANE_ID="$(printf '%s\n' "$B_SET_LINE" | sed -E 's/.*SetOverlay: pane_id=([^ ]+) .*/\1/')"
  [ -n "$B_PANE_ID" ] || fail "failed to extract browser B pane id"
  [ "$B_PANE_ID" != "$A_PANE_ID" ] || fail "browser B reused browser A pane id"
  log "browser_b_pane_id=$B_PANE_ID"
  wait_for_log_after "$BROWSER_B_START_LINE" "SetOverlay: reused pending server key=default/${CHROMIUM} pane_count=2 has_fd=true" "browser B reused default-profile server" 60
  if tail -n +"$((BROWSER_B_START_LINE + 1))" "$APP_LOG" |
    grep -E "spawned browser path=${CHROMIUM} pid=[0-9]+ profile=default" >/dev/null 2>&1; then
    fail "browser B spawned a second default-profile Chromium process"
  fi
  log "PASS: browser B did not spawn a second default-profile Chromium process"

  B_CA_CONTEXT_LINE="$(wait_for_line_after "$BROWSER_B_START_LINE" "TermSurf geometry layer=zig event=ca_context .*pane_id:${B_PANE_ID}" "browser B split Zig ca_context" 60)"
  B_BROWSER_TAB_ID="$(extract_browser_tab_id "$B_CA_CONTEXT_LINE")"
  B_CONTEXT_ID="$(extract_context_id "$B_CA_CONTEXT_LINE")"
  [ -n "$B_BROWSER_TAB_ID" ] || fail "failed to extract browser B browser tab id"
  [ -n "$B_CONTEXT_ID" ] || fail "failed to extract browser B context id"
  [ "$B_BROWSER_TAB_ID" != "$A_BROWSER_TAB_ID" ] || fail "browser B reused browser A browser tab id"
  [ "$B_CONTEXT_ID" != "$A_CONTEXT_ID" ] || fail "browser B reused browser A CA/context id"
  log "browser_b_browser_tab_id=$B_BROWSER_TAB_ID"
  log "browser_b_context_id=$B_CONTEXT_ID"

  B_APPKIT_PRESENT_LINE="$(wait_for_line_after "$BROWSER_B_START_LINE" "TermSurf geometry layer=appkit event=presented .*pane_id:${B_PANE_ID} .*context_id=${B_CONTEXT_ID}" "browser B split AppKit presentation" 60)"
  B_APPKIT_PIXELS_LINE="$(wait_for_line_after "$BROWSER_B_START_LINE" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${B_PANE_ID} .*context_id=${B_CONTEXT_ID}" "browser B split AppKit pixels" 60)"
  B_PRESENT_WINDOW_ID="$(printf '%s\n' "$B_APPKIT_PRESENT_LINE" | sed -E 's/.*window_id:([^ ]+) .*/\1/')"
  B_SURFACE_ID="$(extract_surface_id "$B_APPKIT_PRESENT_LINE")"
  B_SELECTED_TAB_ID="$(extract_selected_tab_id "$B_APPKIT_PRESENT_LINE")"
  B_FRAME="$(extract_overlay_frame "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_SIZE="$(extract_frame_size "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_X="$(extract_frame_x "$B_APPKIT_PRESENT_LINE")"
  B_FRAME_Y="$(extract_frame_y "$B_APPKIT_PRESENT_LINE")"
  B_ROOT_X="$(extract_root_frame_x "$B_APPKIT_PRESENT_LINE")"
  B_ROOT_Y="$(extract_root_frame_y "$B_APPKIT_PRESENT_LINE")"
  B_ROOT_SIZE="$(extract_root_frame_size "$B_APPKIT_PRESENT_LINE")"
  B_PIXEL="$(extract_appkit_pixel "$B_APPKIT_PIXELS_LINE")"
  [ "$B_PRESENT_WINDOW_ID" = "$A_WINDOW_ID" ] || fail "browser B AppKit window id mismatch: expected=$A_WINDOW_ID actual=$B_PRESENT_WINDOW_ID"
  [ -n "$B_SURFACE_ID" ] || fail "failed to extract browser B surface id"
  [ "$B_SURFACE_ID" != "$A_SURFACE_ID" ] || fail "browser B reused browser A terminal surface id"
  [ "$B_SELECTED_TAB_ID" = "$A_SELECTED_TAB_ID" ] || fail "browser B selected tab id mismatch: expected=$A_SELECTED_TAB_ID actual=$B_SELECTED_TAB_ID"
  log "browser_b_surface_id=$B_SURFACE_ID"
  log "browser_b_selected_tab_id=$B_SELECTED_TAB_ID"
  log "browser_b_overlay_frame=$B_FRAME"
  log "browser_b_root_x=$B_ROOT_X"
  log "browser_b_root_y=$B_ROOT_Y"
  log "browser_b_root_size=$B_ROOT_SIZE"
  log "browser_b_appkit_pixel=$B_PIXEL"

  B_PIXEL_WIDTH="${B_PIXEL%x*}"
  B_PIXEL_HEIGHT="${B_PIXEL#*x}"
  require_trace_after "$BROWSER_B_TRACE_START_LINE" "resize tab_id=${B_BROWSER_TAB_ID} pane_id=${B_PANE_ID} pixel_width=${B_PIXEL_WIDTH} pixel_height=${B_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium resized browser B to split AppKit pixel size"

  A_SPLIT_ROOT_WIDTH="$(pair_width "$A_SPLIT_ROOT_SIZE")"
  A_GLOBAL_FRAME_X="$(awk -v root="$A_SPLIT_ROOT_X" -v frame="$A_SPLIT_FRAME_X" 'BEGIN { print root + frame }')"
  B_GLOBAL_FRAME_X="$(awk -v surface_x="$A_SPLIT_ROOT_WIDTH" -v frame="$B_FRAME_X" 'BEGIN { print surface_x + frame }')"
  A_GLOBAL_FRAME_WIDTH="$(pair_width "$A_SPLIT_FRAME_SIZE")"
  B_GLOBAL_FRAME_WIDTH="$(pair_width "$B_FRAME_SIZE")"
  awk \
    -v ax="$A_GLOBAL_FRAME_X" \
    -v aw="$A_GLOBAL_FRAME_WIDTH" \
    -v bx="$B_GLOBAL_FRAME_X" \
    -v bw="$B_GLOBAL_FRAME_WIDTH" \
    'BEGIN { exit !((ax + aw <= bx) || (bx + bw <= ax)) }' ||
    fail "browser split overlay frames overlap: a_x=$A_GLOBAL_FRAME_X a_w=$A_GLOBAL_FRAME_WIDTH b_x=$B_GLOBAL_FRAME_X b_w=$B_GLOBAL_FRAME_WIDTH"
  log "PASS: browser split overlay frames do not overlap"

  SPLIT_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve two-browser split window bounds"
  IFS=$'\t' read -r _SPLIT_WID SPLIT_WX SPLIT_WY SPLIT_WW SPLIT_WH <<<"$SPLIT_WIN_LINE"
  A_SPLIT_ROOT_HEIGHT="$(pair_height "$A_SPLIT_ROOT_SIZE")"
  SPLIT_CONTENT_Y_OFFSET="$(awk -v wh="$SPLIT_WH" -v root_h="$A_SPLIT_ROOT_HEIGHT" 'BEGIN { print int(wh - root_h) }')"
  A_SPLIT_FRAME_WIDTH="$(pair_width "$A_SPLIT_FRAME_SIZE")"
  A_SPLIT_FRAME_HEIGHT="$(pair_height "$A_SPLIT_FRAME_SIZE")"
  B_FRAME_WIDTH="$(pair_width "$B_FRAME_SIZE")"
  B_FRAME_HEIGHT="$(pair_height "$B_FRAME_SIZE")"
  A_CLICK_X="$(awk -v wx="$SPLIT_WX" -v root_x="$A_SPLIT_ROOT_X" -v frame_x="$A_SPLIT_FRAME_X" -v frame_w="$A_SPLIT_FRAME_WIDTH" 'BEGIN { print int(wx + root_x + frame_x + (frame_w / 2) + 0.5) }')"
  A_CLICK_Y="$(awk -v wy="$SPLIT_WY" -v content_y="$SPLIT_CONTENT_Y_OFFSET" -v root_y="$A_SPLIT_ROOT_Y" -v frame_y="$A_SPLIT_FRAME_Y" -v frame_h="$A_SPLIT_FRAME_HEIGHT" 'BEGIN { print int(wy + content_y + root_y + frame_y + (frame_h / 2) + 0.5) }')"
  B_CLICK_X="$(awk -v wx="$SPLIT_WX" -v surface_x="$A_SPLIT_ROOT_WIDTH" -v frame_x="$B_FRAME_X" -v frame_w="$B_FRAME_WIDTH" 'BEGIN { print int(wx + surface_x + frame_x + (frame_w / 2) + 0.5) }')"
  B_CLICK_Y="$(awk -v wy="$SPLIT_WY" -v content_y="$SPLIT_CONTENT_Y_OFFSET" -v root_y="$B_ROOT_Y" -v frame_y="$B_FRAME_Y" -v frame_h="$B_FRAME_HEIGHT" 'BEGIN { print int(wy + content_y + root_y + frame_y + (frame_h / 2) + 0.5) }')"
  log "browser_a_split_click=${A_CLICK_X},${A_CLICK_Y}"
  log "browser_b_split_click=${B_CLICK_X},${B_CLICK_Y}"

  A_HIT_START_LINE="$(log_line_count)"
  A_HIT_TRACE_START_LINE="$(trace_line_count)"
  click_global_point "$A_CLICK_X" "$A_CLICK_Y" "two_browser_split_a"
  A_HIT_LINE="$(wait_for_hit_after "$A_HIT_START_LINE" "$A_CONTEXT_ID" "browser A split hit-test")"
  require_text "$A_HIT_LINE" "pane_id:${A_PANE_ID}" "browser A hit-test has browser A pane id"
  require_text "$A_HIT_LINE" "selected_tab_id:${A_SELECTED_TAB_ID}" "browser A hit-test has selected tab id"
  require_text "$A_HIT_LINE" "overlay_frame=${A_SPLIT_FRAME}" "browser A hit-test uses browser A split frame"
  require_trace_after "$A_HIT_TRACE_START_LINE" "mouse-move tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser A pointer move reached browser A"
  require_no_trace_after "$A_HIT_TRACE_START_LINE" "mouse-move tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser A pointer move did not route to browser B"

  enter_browser_browse "two_browser_split_a" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
  type_marker_require_only "two_browser_split_a" "ISSUE818_EXP5_BROWSER_A" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$B_BROWSER_TAB_ID" "$B_PANE_ID"
  leave_browser_browse "two_browser_split_a" "$A_PANE_ID" "$A_BROWSER_TAB_ID"

  B_HIT_START_LINE="$(log_line_count)"
  B_HIT_TRACE_START_LINE="$(trace_line_count)"
  click_global_point "$B_CLICK_X" "$B_CLICK_Y" "two_browser_split_b"
  B_HIT_LINE="$(wait_for_hit_after "$B_HIT_START_LINE" "$B_CONTEXT_ID" "browser B split hit-test")"
  require_text "$B_HIT_LINE" "pane_id:${B_PANE_ID}" "browser B hit-test has browser B pane id"
  require_text "$B_HIT_LINE" "selected_tab_id:${B_SELECTED_TAB_ID}" "browser B hit-test has selected tab id"
  require_text "$B_HIT_LINE" "overlay_frame=${B_FRAME}" "browser B hit-test uses browser B split frame"
  require_trace_after "$B_HIT_TRACE_START_LINE" "mouse-move tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser B pointer move reached browser B"
  require_no_trace_after "$B_HIT_TRACE_START_LINE" "mouse-move tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser B pointer move did not route to browser A"

  enter_browser_browse "two_browser_split_b" "$B_PANE_ID" "$B_BROWSER_TAB_ID"
  type_marker_require_only "two_browser_split_b" "ISSUE818_EXP5_BROWSER_B" "$B_BROWSER_TAB_ID" "$B_PANE_ID" "$A_BROWSER_TAB_ID" "$A_PANE_ID"
  leave_browser_browse "two_browser_split_b" "$B_PANE_ID" "$B_BROWSER_TAB_ID"

  B_CLOSE_START_LINE="$(log_line_count)"
  B_CLOSE_TRACE_START_LINE="$(trace_line_count)"
  log "browser_b_close_keybind=ctrl+k=close_surface"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 40 control >>"$HARNESS_LOG" 2>&1
  delay 1
  require_log_after "$B_CLOSE_START_LINE" "Pane close cleanup: pane_id=${B_PANE_ID} tab_id=${B_BROWSER_TAB_ID}" "Ghostboard cleaned up browser B split pane"
  require_log_after "$B_CLOSE_START_LINE" "CloseTab: pane_id=${B_PANE_ID} tab_id=${B_BROWSER_TAB_ID}" "Ghostboard sent CloseTab for browser B split pane"
  require_trace_after "$B_CLOSE_TRACE_START_LINE" "close-tab tab_id=${B_BROWSER_TAB_ID} pane_id=${B_PANE_ID} result=destroying ffi=ts_destroy_web_contents" "Chromium destroyed browser B split tab"
  require_trace_after "$B_CLOSE_TRACE_START_LINE" "close-tab tab_id=${B_BROWSER_TAB_ID} result=removed" "Chromium removed browser B split tab"
  require_no_trace_after "$B_CLOSE_TRACE_START_LINE" "close-tab result=no-tabs-remaining" "shared Chromium stayed alive after closing browser B"

  A_AFTER_CLOSE_PRESENT_LINE="$(wait_for_split_right_zoom_frame_after "$B_CLOSE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$OVERLAY_FRAME_SIZE" "$A_SPLIT_FRAME_SIZE" "browser A expanded after browser B split close")"
  A_AFTER_CLOSE_PIXELS_LINE="$(wait_for_split_right_zoom_pixels_after "$B_CLOSE_START_LINE" "$A_PANE_ID" "$A_CONTEXT_ID" "$A_BASE_PIXEL" "$A_SPLIT_PIXEL" "browser A pixels expanded after browser B split close")"
  A_AFTER_CLOSE_FRAME="$(extract_overlay_frame "$A_AFTER_CLOSE_PRESENT_LINE")"
  A_AFTER_CLOSE_FRAME_SIZE="$(extract_frame_size "$A_AFTER_CLOSE_PRESENT_LINE")"
  A_AFTER_CLOSE_FRAME_X="$(extract_frame_x "$A_AFTER_CLOSE_PRESENT_LINE")"
  A_AFTER_CLOSE_FRAME_Y="$(extract_frame_y "$A_AFTER_CLOSE_PRESENT_LINE")"
  A_AFTER_CLOSE_PIXEL="$(extract_appkit_pixel "$A_AFTER_CLOSE_PIXELS_LINE")"
  A_AFTER_CLOSE_PIXEL_WIDTH="${A_AFTER_CLOSE_PIXEL%x*}"
  A_AFTER_CLOSE_PIXEL_HEIGHT="${A_AFTER_CLOSE_PIXEL#*x}"
  log "browser_a_after_close_overlay_frame=$A_AFTER_CLOSE_FRAME"
  log "browser_a_after_close_appkit_pixel=$A_AFTER_CLOSE_PIXEL"
  require_trace_after "$B_CLOSE_TRACE_START_LINE" "resize tab_id=${A_BROWSER_TAB_ID} pane_id=${A_PANE_ID} pixel_width=${A_AFTER_CLOSE_PIXEL_WIDTH} pixel_height=${A_AFTER_CLOSE_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium resized browser A after browser B close"

  screencapture -x -o -l"$A_WINDOW_ID" "$SCREENSHOT_CLOSE"
  log "two_browser_split_close_screenshot_exit=$?"

  A_AFTER_CLOSE_WIN_LINE="$(window_bounds_for "$A_WINDOW_ID")" || fail "failed to resolve browser A window after browser B close"
  A_AFTER_CLOSE_CLICK="$(click_point_for_frame "$A_AFTER_CLOSE_WIN_LINE" "$A_AFTER_CLOSE_PRESENT_LINE")"
  IFS=$'\t' read -r A_AFTER_CLOSE_X A_AFTER_CLOSE_Y <<<"$A_AFTER_CLOSE_CLICK"
  A_POST_HIT_START_LINE="$(log_line_count)"
  A_POST_HIT_TRACE_START_LINE="$(trace_line_count)"
  click_global_point "$A_AFTER_CLOSE_X" "$A_AFTER_CLOSE_Y" "two_browser_split_a_after_b_close"
  A_POST_HIT_LINE="$(wait_for_hit_after "$A_POST_HIT_START_LINE" "$A_CONTEXT_ID" "browser A hit-test after browser B close")"
  require_text "$A_POST_HIT_LINE" "pane_id:${A_PANE_ID}" "browser A post-close hit-test has browser A pane id"
  require_trace_after "$A_POST_HIT_TRACE_START_LINE" "mouse-move tab=${A_BROWSER_TAB_ID} pane=${A_PANE_ID}" "browser A post-close pointer move reached browser A"
  require_no_trace_after "$A_POST_HIT_TRACE_START_LINE" "mouse-move tab=${B_BROWSER_TAB_ID} pane=${B_PANE_ID}" "browser A post-close pointer move did not route to closed browser B"

  enter_browser_browse "two_browser_split_a_after_b_close" "$A_PANE_ID" "$A_BROWSER_TAB_ID"
  type_marker_require_only "two_browser_split_a_after_b_close" "ISSUE818_EXP5_BROWSER_A_AFTER_CLOSE" "$A_BROWSER_TAB_ID" "$A_PANE_ID" "$B_BROWSER_TAB_ID" "$B_PANE_ID"
  leave_browser_browse "two_browser_split_a_after_b_close" "$A_PANE_ID" "$A_BROWSER_TAB_ID"

  require_no_log_after "$B_CLOSE_START_LINE" "SetOverlay: pane_id=${B_PANE_ID}" "closed browser B did not recreate a live overlay"
fi

if [ "$SCENARIO" = "split-right" ] || [ "$SCENARIO" = "split-right-border-config" ] || [ "$SCENARIO" = "performance-split-right" ]; then
  SPLIT_START_LINE="$(log_line_count)"
  SPLIT_TRACE_START_LINE="$(trace_line_count)"
  if [ "$SCENARIO" = "performance-split-right" ]; then
    GUI_SOCKET="$(sed -nE 's/.*TermSurf socket listening on (.*)$/\1/p' "$APP_LOG" | tail -1)"
    [ -S "$GUI_SOCKET" ] || fail "failed to resolve Ghostboard TermSurf socket: $GUI_SOCKET"
    PERFORMANCE_SPLIT_COMMAND="/bin/sh -lc 'sleep 60'"
    log "performance_split_socket=$GUI_SOCKET"
    log "performance_split_protocol=OpenSplit direction=right command=${PERFORMANCE_SPLIT_COMMAND}"
    send_open_split_message "$GUI_SOCKET" "$PANE_ID" "right" "$PERFORMANCE_SPLIT_COMMAND" >>"$HARNESS_LOG" 2>&1
    wait_for_log_after "$SPLIT_START_LINE" "OpenSplit: pane_id=${PANE_ID} direction=right" "performance split OpenSplit handled" 30
  else
    log "split_keybind=ctrl+d=new_split:right"
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 2 control >>"$HARNESS_LOG" 2>&1
  fi
  delay 1

  if [ "$SCENARIO" = "split-right-border-config" ]; then
    if head -n "$SPLIT_START_LINE" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=split_border_draw" >/dev/null 2>&1; then
      fail "single-pane baseline drew a split pane border before split"
    fi
    log "PASS: single-pane baseline did not draw split pane border"
  fi

  SPLIT_FRAME_TOLERANCE=8
  SPLIT_PIXEL_TOLERANCE=16
  if [ "$SCENARIO" = "split-right-border-config" ]; then
    SPLIT_FRAME_TOLERANCE=24
    SPLIT_PIXEL_TOLERANCE=48
  fi
  SPLIT_PRESENT_LINE="$(wait_for_split_right_frame_after "$SPLIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$OVERLAY_FRAME_SIZE" "split-right AppKit overlay frame" 30 "$SPLIT_FRAME_TOLERANCE")"
  SPLIT_PIXELS_LINE="$(wait_for_split_right_pixels_after "$SPLIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$APPKIT_PIXEL" "split-right AppKit presented pixels" 30 "$SPLIT_PIXEL_TOLERANCE")"
  SPLIT_FRAME_SIZE="$(extract_frame_size "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_X="$(extract_frame_x "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_WIDTH="$(pair_width "$SPLIT_FRAME_SIZE")"
  SPLIT_PIXEL="$(extract_appkit_pixel "$SPLIT_PIXELS_LINE")"
  SPLIT_PIXEL_WIDTH="${SPLIT_PIXEL%x*}"
  SPLIT_PIXEL_HEIGHT="${SPLIT_PIXEL#*x}"
  log "PASS: observed split-right AppKit overlay frame overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "PASS: observed split-right AppKit presented pixels appkit_pixel=$SPLIT_PIXEL"
  log "split_overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "split_overlay_frame_x=$SPLIT_FRAME_X"
  log "split_appkit_pixel=$SPLIT_PIXEL"
  require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${SPLIT_PIXEL}" "Zig records split-right AppKit presented pixel size"
  require_trace_after "$SPLIT_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${SPLIT_PIXEL_WIDTH} pixel_height=${SPLIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-right resize to AppKit pixel size via ts_set_view_size"
  screencapture -x -o -l"$WID" "$SCREENSHOT_SPLIT"
  log "split_screenshot_exit=$?"

  if [ "$SCENARIO" = "split-right-border-config" ]; then
    case "$BORDER_CONFIG_CASE" in
      enabled)
        require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=appkit event=split_border_config .*is_split=true .*focused=true .*border_width=2 .*focused_color_present=true .*unfocused_color_present=true .*saturation=0.5 .*border_drawn=true .*border_color_key=focused-split-border-color" "focused split border config bridged enabled values"
        require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=appkit event=split_border_config .*is_split=true .*focused=false .*border_width=2 .*focused_color_present=true .*unfocused_color_present=true .*saturation=0.5 .*border_drawn=true .*border_color_key=unfocused-split-border-color" "unfocused split border config bridged enabled values"
        require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=appkit event=split_border_draw .*focused=true .*border_width=2 .*border_color_key=focused-split-border-color .*hit_testing=false" "focused split border draw logged with hit testing disabled"
        require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=appkit event=split_border_draw .*focused=false .*border_width=2 .*border_color_key=unfocused-split-border-color .*hit_testing=false" "unfocused split border draw logged with hit testing disabled"
        ;;
      clamp)
        require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=appkit event=split_border_config .*is_split=true .*border_width=10 .*focused_color_present=true .*unfocused_color_present=true .*saturation=1 .*border_drawn=true" "split border config bridged clamped values"
        require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=appkit event=split_border_draw .*border_width=10 .*hit_testing=false" "clamped split border draw logged with hit testing disabled"
        ;;
      disabled)
        require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=appkit event=split_border_config .*is_split=true .*border_width=0 .*focused_color_present=true .*unfocused_color_present=true .*saturation=0.5 .*border_drawn=false" "disabled split border config bridged width zero"
        require_no_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=appkit event=split_border_draw" "disabled split border did not draw"
        ;;
      missing-focused)
        require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=appkit event=split_border_config .*is_split=true .*focused=true .*border_width=2 .*focused_color_present=false .*unfocused_color_present=true .*saturation=0.5 .*border_drawn=false .*border_color_key=focused-split-border-color" "missing focused color prevents focused border draw"
        require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=appkit event=split_border_draw .*focused=false .*border_width=2 .*border_color_key=unfocused-split-border-color .*hit_testing=false" "missing focused color still allows unfocused border draw"
        ;;
      missing-unfocused)
        require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=appkit event=split_border_draw .*focused=true .*border_width=2 .*border_color_key=focused-split-border-color .*hit_testing=false" "missing unfocused color still allows focused border draw"
        require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=appkit event=split_border_config .*is_split=true .*focused=false .*border_width=2 .*focused_color_present=true .*unfocused_color_present=false .*saturation=0.5 .*border_drawn=false .*border_color_key=unfocused-split-border-color" "missing unfocused color prevents unfocused border draw"
        ;;
    esac
  fi

  if [ "$SCENARIO" = "split-right" ] || [ "$SCENARIO" = "split-right-border-config" ]; then
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

    if [ "$SCENARIO" = "split-right-border-config" ] && [ "$BORDER_CONFIG_CASE" = "disabled" ]; then
      log "PASS: disabled split border skipped moving sibling negative hit-test; no border was drawn to intercept input"
    else
      SPLIT_NEGATIVE_X="$(awk -v wx="$SPLIT_WX" -v frame_x="$SPLIT_FRAME_X" -v frame_w="$SPLIT_FRAME_WIDTH" -v old_w="$INITIAL_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + frame_w + ((old_w - frame_w) / 2) + 0.5) }')"
      SPLIT_NEGATIVE_Y="$SPLIT_INSIDE_Y"
      click_negative_global_point "$SPLIT_NEGATIVE_X" "$SPLIT_NEGATIVE_Y" "split_sibling_negative"
      wait_for_negative_hit_after "$NEGATIVE_HIT_START_LINE" "$CONTEXT_ID" "split-right sibling-pane negative hit-test" allow-absent
    fi
  else
    log "PASS: performance split skipped pointer hit-test assertions"
  fi
fi

if [ "$SCENARIO" = "split-right-focus-switch" ]; then
  SPLIT_START_LINE="$(log_line_count)"
  SPLIT_TRACE_START_LINE="$(trace_line_count)"
  log "split_keybind=ctrl+d=new_split:right"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 2 control >>"$HARNESS_LOG" 2>&1
  delay 1

  SPLIT_PRESENT_LINE="$(wait_for_split_right_frame_after "$SPLIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$OVERLAY_FRAME_SIZE" "split-right AppKit overlay frame")"
  SPLIT_PIXELS_LINE="$(wait_for_split_right_pixels_after "$SPLIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$APPKIT_PIXEL" "split-right AppKit presented pixels")"
  SPLIT_FRAME="$(printf '%s\n' "$SPLIT_PRESENT_LINE" | sed -E 's/.*overlay_frame=(\{\{[^}]+\}, \{[^}]+\}\}).*/\1/')"
  SPLIT_FRAME_SIZE="$(extract_frame_size "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_X="$(extract_frame_x "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_Y="$(extract_frame_y "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_WIDTH="$(pair_width "$SPLIT_FRAME_SIZE")"
  SPLIT_PIXEL="$(extract_appkit_pixel "$SPLIT_PIXELS_LINE")"
  SPLIT_PIXEL_WIDTH="${SPLIT_PIXEL%x*}"
  SPLIT_PIXEL_HEIGHT="${SPLIT_PIXEL#*x}"
  log "PASS: observed split-right AppKit overlay frame overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "PASS: observed split-right AppKit presented pixels appkit_pixel=$SPLIT_PIXEL"
  log "split_overlay_frame=$SPLIT_FRAME"
  log "split_overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "split_overlay_frame_x=$SPLIT_FRAME_X"
  log "split_overlay_frame_y=$SPLIT_FRAME_Y"
  log "split_appkit_pixel=$SPLIT_PIXEL"
  require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${SPLIT_PIXEL}" "Zig records split-right AppKit presented pixel size"
  require_trace_after "$SPLIT_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${SPLIT_PIXEL_WIDTH} pixel_height=${SPLIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-right resize to AppKit pixel size via ts_set_view_size"

  SPLIT_WIN_LINE="$(window_bounds)" || fail "failed to resolve split window bounds for window id=$WID"
  IFS=$'\t' read -r _SPLIT_WID SPLIT_WX SPLIT_WY SPLIT_WW SPLIT_WH <<<"$SPLIT_WIN_LINE"
  INITIAL_FRAME_WIDTH="$(pair_width "$OVERLAY_FRAME_SIZE")"
  BROWSER_FOCUS_X="$(awk -v wx="$SPLIT_WX" -v frame_x="$SPLIT_FRAME_X" -v frame_w="$SPLIT_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  BROWSER_FOCUS_Y=$((SPLIT_WY + SPLIT_WH / 2))
  BROWSER_PANE_FOCUS_X="$BROWSER_FOCUS_X"
  BROWSER_PANE_FOCUS_Y=$((SPLIT_WY + SPLIT_WH - 40))
  SIBLING_FOCUS_X="$(awk -v wx="$SPLIT_WX" -v frame_x="$SPLIT_FRAME_X" -v split_w="$SPLIT_FRAME_WIDTH" -v initial_w="$INITIAL_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + split_w + ((initial_w - split_w) / 2) + 0.5) }')"
  SIBLING_FOCUS_Y="$BROWSER_FOCUS_Y"

  BROWSER_PRIME_TRACE_START_LINE="$(trace_line_count)"
  click_global_point "$BROWSER_PANE_FOCUS_X" "$BROWSER_PANE_FOCUS_Y" "browser_prime_terminal_focus"

  BROWSER_PRIME_HIT_START_LINE="$(log_line_count)"
  click_global_point "$BROWSER_FOCUS_X" "$BROWSER_FOCUS_Y" "browser_prime_focus"
  BROWSER_PRIME_HIT_LINE="$(wait_for_hit_after "$BROWSER_PRIME_HIT_START_LINE" "$CONTEXT_ID" "same-tab browser prime focus hit-test")"
  require_text "$BROWSER_PRIME_HIT_LINE" "web_point={" "browser prime focus hit-test includes webview-relative point"
  require_no_trace_after "$BROWSER_PRIME_TRACE_START_LINE" "focus-changed tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_set_focus focused=true" "browser pane focus in Control mode did not focus Chromium before Browse mode"

  SIBLING_FOCUS_TRACE_START_LINE="$(trace_line_count)"
  click_negative_global_point "$SIBLING_FOCUS_X" "$SIBLING_FOCUS_Y" "sibling_focus"
  wait_for_negative_hit_after "$NEGATIVE_HIT_START_LINE" "$CONTEXT_ID" "same-tab sibling focus negative hit-test" allow-absent

  SIBLING_KEY_START_LINE="$(log_line_count)"
  printf 'ISSUE809_EXP11_SIBLING_FOCUS\n' >"$SIBLING_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$SIBLING_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  SIBLING_KEY_SEEN=""
  for _ in $(seq 1 10); do
    if tail -n +"$((SIBLING_KEY_START_LINE + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=key_down .*overlay_frame=none .*visible=false .*focused=true" >/dev/null 2>&1; then
      SIBLING_KEY_SEEN="1"
      break
    fi
    delay 1
  done
  [ -n "$SIBLING_KEY_SEEN" ] || fail "sibling terminal pane did not receive keyboard events after focus switch"
  log "PASS: sibling terminal pane received keyboard events after focus switch"
  require_no_trace_after "$SIBLING_FOCUS_TRACE_START_LINE" "key-event tab=${BROWSER_TAB_ID} pane=${PANE_ID}" "sibling terminal marker did not reach original browser context"
  require_trace_after "$SIBLING_FOCUS_TRACE_START_LINE" "focus-changed tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_set_focus focused=false" "Chromium observed original browser pane focus=false after sibling focus"
  require_no_different_appkit_frame_after "$SIBLING_KEY_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$SPLIT_FRAME" "sibling focus kept original browser AppKit frame unchanged"
  require_no_different_appkit_pixels_after "$SIBLING_KEY_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$SPLIT_PIXEL" "sibling focus kept original browser AppKit pixels unchanged"

  BROWSER_REFOCUS_TRACE_START_LINE="$(trace_line_count)"
  click_global_point "$BROWSER_PANE_FOCUS_X" "$BROWSER_PANE_FOCUS_Y" "browser_refocus_terminal_focus"

  BROWSER_REFOCUS_HIT_START_LINE="$(log_line_count)"
  click_global_point "$BROWSER_FOCUS_X" "$BROWSER_FOCUS_Y" "browser_refocus"
  BROWSER_REFOCUS_HIT_LINE="$(wait_for_hit_after "$BROWSER_REFOCUS_HIT_START_LINE" "$CONTEXT_ID" "same-tab browser refocus hit-test")"
  BROWSER_REFOCUS_FRAME_SIZE="$(extract_frame_size "$BROWSER_REFOCUS_HIT_LINE")"
  BROWSER_REFOCUS_FRAME_X="$(extract_frame_x "$BROWSER_REFOCUS_HIT_LINE")"
  BROWSER_REFOCUS_FRAME_Y="$(extract_frame_y "$BROWSER_REFOCUS_HIT_LINE")"
  [ "$BROWSER_REFOCUS_FRAME_SIZE" = "$SPLIT_FRAME_SIZE" ] || fail "browser refocus hit-test frame size changed: expected=$SPLIT_FRAME_SIZE actual=$BROWSER_REFOCUS_FRAME_SIZE"
  [ "$BROWSER_REFOCUS_FRAME_X" = "$SPLIT_FRAME_X" ] || fail "browser refocus hit-test frame x changed: expected=$SPLIT_FRAME_X actual=$BROWSER_REFOCUS_FRAME_X"
  [ "$BROWSER_REFOCUS_FRAME_Y" = "$SPLIT_FRAME_Y" ] || fail "browser refocus hit-test frame y changed: expected=$SPLIT_FRAME_Y actual=$BROWSER_REFOCUS_FRAME_Y"
  log "PASS: browser refocus hit-test uses split baseline overlay frame"
  require_text "$BROWSER_REFOCUS_HIT_LINE" "web_point={" "browser refocus hit-test includes webview-relative point"
  require_no_different_appkit_frame_after "$BROWSER_REFOCUS_HIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$SPLIT_FRAME" "browser refocus kept original browser AppKit frame unchanged"
  require_no_different_appkit_pixels_after "$BROWSER_REFOCUS_HIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$SPLIT_PIXEL" "browser refocus kept original browser AppKit pixels unchanged"
  require_no_trace_after "$BROWSER_REFOCUS_TRACE_START_LINE" "focus-changed tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_set_focus focused=true" "browser refocus in Control mode did not focus Chromium before Browse mode"

  BROWSER_MODE_START_LINE="$(log_line_count)"
  BROWSER_MODE_TRACE_START_LINE="$(trace_line_count)"
  log "browser_refocus_mode_key=enter=Mode::Browse"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  wait_for_log_after "$BROWSER_MODE_START_LINE" "ModeChanged: pane_id=${PANE_ID} browsing=true" "webtui entered browse mode after browser refocus"
  require_trace_after "$BROWSER_MODE_TRACE_START_LINE" "focus-changed tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_set_focus focused=true" "Chromium observed original browser pane focus=true after browse mode"

  BROWSER_KEY_START_LINE="$(trace_line_count)"
  printf 'ISSUE809_EXP11_BROWSER_REFOCUS\n' >"$BROWSER_FOCUS_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$BROWSER_FOCUS_COMMAND" >>"$HARNESS_LOG" 2>&1
  BROWSER_KEY_SEEN=""
  for _ in $(seq 1 10); do
    if tail -n +"$((BROWSER_KEY_START_LINE + 1))" "$CHROMIUM_TRACE" | grep -F "key-event tab=${BROWSER_TAB_ID} pane=${PANE_ID}" >/dev/null 2>&1; then
      BROWSER_KEY_SEEN="1"
      break
    fi
    delay 1
  done
  [ -n "$BROWSER_KEY_SEEN" ] || fail "browser refocus keyboard marker did not reach original browser context"
  log "PASS: browser refocus keyboard marker reached original browser context"

  screencapture -x -o -l"$WID" "$SCREENSHOT_SPLIT"
  log "focus_switch_screenshot_exit=$?"
fi

if [ "$SCENARIO" = "split-right-resize" ]; then
  SPLIT_START_LINE="$(log_line_count)"
  SPLIT_TRACE_START_LINE="$(trace_line_count)"
  log "split_keybind=ctrl+d=new_split:right"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 2 control >>"$HARNESS_LOG" 2>&1
  delay 1

  SPLIT_PRESENT_LINE="$(wait_for_split_right_frame_after "$SPLIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$OVERLAY_FRAME_SIZE" "split-right AppKit overlay frame")"
  SPLIT_PIXELS_LINE="$(wait_for_split_right_pixels_after "$SPLIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$APPKIT_PIXEL" "split-right AppKit presented pixels")"
  SPLIT_FRAME_SIZE="$(extract_frame_size "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_X="$(extract_frame_x "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_WIDTH="$(pair_width "$SPLIT_FRAME_SIZE")"
  SPLIT_PIXEL="$(extract_appkit_pixel "$SPLIT_PIXELS_LINE")"
  SPLIT_PIXEL_WIDTH="${SPLIT_PIXEL%x*}"
  SPLIT_PIXEL_HEIGHT="${SPLIT_PIXEL#*x}"
  log "PASS: observed split-right AppKit overlay frame overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "PASS: observed split-right AppKit presented pixels appkit_pixel=$SPLIT_PIXEL"
  log "split_overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "split_overlay_frame_x=$SPLIT_FRAME_X"
  log "split_appkit_pixel=$SPLIT_PIXEL"
  require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${SPLIT_PIXEL}" "Zig records split-right AppKit presented pixel size"
  require_trace_after "$SPLIT_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${SPLIT_PIXEL_WIDTH} pixel_height=${SPLIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-right resize to AppKit pixel size via ts_set_view_size"

  DIVIDER_START_LINE="$(log_line_count)"
  DIVIDER_TRACE_START_LINE="$(trace_line_count)"
  log "resize_split_keybind=ctrl+l=resize_split:right,20"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 37 control >>"$HARNESS_LOG" 2>&1
  delay 1

  DIVIDER_PRESENT_LINE="$(wait_for_split_right_resize_frame_after "$DIVIDER_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$SPLIT_FRAME_SIZE" "split-right divider-resized AppKit overlay frame")"
  DIVIDER_PIXELS_LINE="$(wait_for_split_right_resize_pixels_after "$DIVIDER_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$SPLIT_PIXEL" "split-right divider-resized AppKit presented pixels")"
  DIVIDER_FRAME_SIZE="$(extract_frame_size "$DIVIDER_PRESENT_LINE")"
  DIVIDER_FRAME_X="$(extract_frame_x "$DIVIDER_PRESENT_LINE")"
  DIVIDER_PIXEL="$(extract_appkit_pixel "$DIVIDER_PIXELS_LINE")"
  DIVIDER_PIXEL_WIDTH="${DIVIDER_PIXEL%x*}"
  DIVIDER_PIXEL_HEIGHT="${DIVIDER_PIXEL#*x}"
  log "PASS: observed split-right divider-resized AppKit overlay frame overlay_frame_size=$DIVIDER_FRAME_SIZE"
  log "PASS: observed split-right divider-resized AppKit presented pixels appkit_pixel=$DIVIDER_PIXEL"
  log "divider_overlay_frame_size=$DIVIDER_FRAME_SIZE"
  log "divider_overlay_frame_x=$DIVIDER_FRAME_X"
  log "divider_appkit_pixel=$DIVIDER_PIXEL"
  require_log_after "$DIVIDER_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${DIVIDER_PIXEL}" "Zig records split-right divider-resized AppKit presented pixel size"
  require_trace_after "$DIVIDER_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${DIVIDER_PIXEL_WIDTH} pixel_height=${DIVIDER_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-right divider resize to AppKit pixel size via ts_set_view_size"
  screencapture -x -o -l"$WID" "$SCREENSHOT_SPLIT"
  log "divider_screenshot_exit=$?"

  DIVIDER_WIN_LINE="$(window_bounds)" || fail "failed to resolve divider-resized window bounds for window id=$WID"
  IFS=$'\t' read -r _DIVIDER_WID DIVIDER_WX DIVIDER_WY DIVIDER_WW DIVIDER_WH <<<"$DIVIDER_WIN_LINE"
  DIVIDER_FRAME_WIDTH="$(pair_width "$DIVIDER_FRAME_SIZE")"
  DIVIDER_INSIDE_X="$(awk -v wx="$DIVIDER_WX" -v frame_x="$DIVIDER_FRAME_X" -v frame_w="$DIVIDER_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  DIVIDER_INSIDE_Y=$((DIVIDER_WY + DIVIDER_WH / 2))
  DIVIDER_HIT_START_LINE="$(log_line_count)"
  click_global_point "$DIVIDER_INSIDE_X" "$DIVIDER_INSIDE_Y" "divider_inside"
  DIVIDER_HIT_LINE="$(wait_for_hit_after "$DIVIDER_HIT_START_LINE" "$CONTEXT_ID" "split-right divider-resized AppKit hit-test")"
  log "PASS: observed split-right divider-resized AppKit hit-test"
  require_text "$DIVIDER_HIT_LINE" "overlay_frame=" "split-right divider-resized hit-test includes current overlay frame"
  require_text "$DIVIDER_HIT_LINE" "web_point={" "split-right divider-resized hit-test includes webview-relative point"

  DIVIDER_NEGATIVE_X="$(awk -v wx="$DIVIDER_WX" -v frame_x="$DIVIDER_FRAME_X" -v frame_w="$DIVIDER_FRAME_WIDTH" -v ww="$DIVIDER_WW" 'BEGIN { print int(wx + frame_x + frame_w + ((ww - frame_x - frame_w) / 2) + 0.5) }')"
  DIVIDER_NEGATIVE_Y="$DIVIDER_INSIDE_Y"
  click_negative_global_point "$DIVIDER_NEGATIVE_X" "$DIVIDER_NEGATIVE_Y" "divider_sibling_negative"
  wait_for_negative_hit_after "$NEGATIVE_HIT_START_LINE" "$CONTEXT_ID" "split-right divider-resized sibling-pane negative hit-test" allow-absent
fi

if [ "$SCENARIO" = "split-right-equalize" ]; then
  SPLIT_START_LINE="$(log_line_count)"
  SPLIT_TRACE_START_LINE="$(trace_line_count)"
  log "split_keybind=ctrl+d=new_split:right"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 2 control >>"$HARNESS_LOG" 2>&1
  delay 1

  SPLIT_PRESENT_LINE="$(wait_for_split_right_frame_after "$SPLIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$OVERLAY_FRAME_SIZE" "split-right AppKit overlay frame")"
  SPLIT_PIXELS_LINE="$(wait_for_split_right_pixels_after "$SPLIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$APPKIT_PIXEL" "split-right AppKit presented pixels")"
  SPLIT_FRAME_SIZE="$(extract_frame_size "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_X="$(extract_frame_x "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_WIDTH="$(pair_width "$SPLIT_FRAME_SIZE")"
  SPLIT_PIXEL="$(extract_appkit_pixel "$SPLIT_PIXELS_LINE")"
  SPLIT_PIXEL_WIDTH="${SPLIT_PIXEL%x*}"
  SPLIT_PIXEL_HEIGHT="${SPLIT_PIXEL#*x}"
  log "PASS: observed split-right AppKit overlay frame overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "PASS: observed split-right AppKit presented pixels appkit_pixel=$SPLIT_PIXEL"
  log "split_overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "split_overlay_frame_x=$SPLIT_FRAME_X"
  log "split_appkit_pixel=$SPLIT_PIXEL"
  require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${SPLIT_PIXEL}" "Zig records split-right AppKit presented pixel size"
  require_trace_after "$SPLIT_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${SPLIT_PIXEL_WIDTH} pixel_height=${SPLIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-right resize to AppKit pixel size via ts_set_view_size"

  DIVIDER_START_LINE="$(log_line_count)"
  DIVIDER_TRACE_START_LINE="$(trace_line_count)"
  log "resize_split_keybind=ctrl+l=resize_split:right,20"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 37 control >>"$HARNESS_LOG" 2>&1
  delay 1

  DIVIDER_PRESENT_LINE="$(wait_for_split_right_resize_frame_after "$DIVIDER_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$SPLIT_FRAME_SIZE" "split-right divider-resized AppKit overlay frame")"
  DIVIDER_PIXELS_LINE="$(wait_for_split_right_resize_pixels_after "$DIVIDER_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$SPLIT_PIXEL" "split-right divider-resized AppKit presented pixels")"
  DIVIDER_FRAME_SIZE="$(extract_frame_size "$DIVIDER_PRESENT_LINE")"
  DIVIDER_PIXEL="$(extract_appkit_pixel "$DIVIDER_PIXELS_LINE")"
  DIVIDER_PIXEL_WIDTH="${DIVIDER_PIXEL%x*}"
  DIVIDER_PIXEL_HEIGHT="${DIVIDER_PIXEL#*x}"
  log "PASS: observed split-right divider-resized AppKit overlay frame overlay_frame_size=$DIVIDER_FRAME_SIZE"
  log "PASS: observed split-right divider-resized AppKit presented pixels appkit_pixel=$DIVIDER_PIXEL"
  log "divider_overlay_frame_size=$DIVIDER_FRAME_SIZE"
  log "divider_appkit_pixel=$DIVIDER_PIXEL"
  require_log_after "$DIVIDER_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${DIVIDER_PIXEL}" "Zig records split-right divider-resized AppKit presented pixel size"
  require_trace_after "$DIVIDER_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${DIVIDER_PIXEL_WIDTH} pixel_height=${DIVIDER_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-right divider resize to AppKit pixel size via ts_set_view_size"

  EQUALIZE_START_LINE="$(log_line_count)"
  EQUALIZE_TRACE_START_LINE="$(trace_line_count)"
  log "equalize_keybind=ctrl+e=equalize_splits"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 14 control >>"$HARNESS_LOG" 2>&1
  delay 1

  EQUALIZE_PRESENT_LINE="$(wait_for_split_right_equalize_frame_after "$EQUALIZE_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$SPLIT_FRAME_SIZE" "$DIVIDER_FRAME_SIZE" "split-right equalized AppKit overlay frame")"
  EQUALIZE_PIXELS_LINE="$(wait_for_split_right_equalize_pixels_after "$EQUALIZE_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$SPLIT_PIXEL" "$DIVIDER_PIXEL" "split-right equalized AppKit presented pixels")"
  EQUALIZE_FRAME_SIZE="$(extract_frame_size "$EQUALIZE_PRESENT_LINE")"
  EQUALIZE_FRAME_X="$(extract_frame_x "$EQUALIZE_PRESENT_LINE")"
  EQUALIZE_PIXEL="$(extract_appkit_pixel "$EQUALIZE_PIXELS_LINE")"
  EQUALIZE_PIXEL_WIDTH="${EQUALIZE_PIXEL%x*}"
  EQUALIZE_PIXEL_HEIGHT="${EQUALIZE_PIXEL#*x}"
  log "PASS: observed split-right equalized AppKit overlay frame overlay_frame_size=$EQUALIZE_FRAME_SIZE"
  log "PASS: observed split-right equalized AppKit presented pixels appkit_pixel=$EQUALIZE_PIXEL"
  log "equalize_overlay_frame_size=$EQUALIZE_FRAME_SIZE"
  log "equalize_overlay_frame_x=$EQUALIZE_FRAME_X"
  log "equalize_appkit_pixel=$EQUALIZE_PIXEL"
  require_log_after "$EQUALIZE_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${EQUALIZE_PIXEL}" "Zig records split-right equalized AppKit presented pixel size"
  require_trace_after "$EQUALIZE_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${EQUALIZE_PIXEL_WIDTH} pixel_height=${EQUALIZE_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-right equalized resize to AppKit pixel size via ts_set_view_size"
  screencapture -x -o -l"$WID" "$SCREENSHOT_SPLIT"
  log "equalize_screenshot_exit=$?"

  EQUALIZE_WIN_LINE="$(window_bounds)" || fail "failed to resolve equalized window bounds for window id=$WID"
  IFS=$'\t' read -r _EQUALIZE_WID EQUALIZE_WX EQUALIZE_WY EQUALIZE_WW EQUALIZE_WH <<<"$EQUALIZE_WIN_LINE"
  EQUALIZE_FRAME_WIDTH="$(pair_width "$EQUALIZE_FRAME_SIZE")"
  EQUALIZE_INSIDE_X="$(awk -v wx="$EQUALIZE_WX" -v frame_x="$EQUALIZE_FRAME_X" -v frame_w="$EQUALIZE_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  EQUALIZE_INSIDE_Y=$((EQUALIZE_WY + EQUALIZE_WH / 2))
  EQUALIZE_HIT_START_LINE="$(log_line_count)"
  click_global_point "$EQUALIZE_INSIDE_X" "$EQUALIZE_INSIDE_Y" "equalize_inside"
  EQUALIZE_HIT_LINE="$(wait_for_hit_after "$EQUALIZE_HIT_START_LINE" "$CONTEXT_ID" "split-right equalized AppKit hit-test")"
  log "PASS: observed split-right equalized AppKit hit-test"
  require_text "$EQUALIZE_HIT_LINE" "overlay_frame=" "split-right equalized hit-test includes current overlay frame"
  require_text "$EQUALIZE_HIT_LINE" "web_point={" "split-right equalized hit-test includes webview-relative point"

  EQUALIZE_NEGATIVE_X="$(awk -v wx="$EQUALIZE_WX" -v frame_x="$EQUALIZE_FRAME_X" -v frame_w="$EQUALIZE_FRAME_WIDTH" -v ww="$EQUALIZE_WW" 'BEGIN { print int(wx + frame_x + frame_w + ((ww - frame_x - frame_w) / 2) + 0.5) }')"
  EQUALIZE_NEGATIVE_Y="$EQUALIZE_INSIDE_Y"
  click_negative_global_point "$EQUALIZE_NEGATIVE_X" "$EQUALIZE_NEGATIVE_Y" "equalize_sibling_negative"
  wait_for_negative_hit_after "$NEGATIVE_HIT_START_LINE" "$CONTEXT_ID" "split-right equalized sibling-pane negative hit-test" allow-absent
fi

if [ "$SCENARIO" = "split-right-zoom" ]; then
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
  require_trace_after "$SPLIT_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${SPLIT_PIXEL_WIDTH} pixel_height=${SPLIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-right resize to AppKit pixel size via ts_set_view_size"

  SPLIT_FOCUS_WIN_LINE="$(window_bounds)" || fail "failed to resolve split window bounds for window id=$WID"
  IFS=$'\t' read -r _SPLIT_FOCUS_WID SPLIT_FOCUS_WX SPLIT_FOCUS_WY SPLIT_FOCUS_WW SPLIT_FOCUS_WH <<<"$SPLIT_FOCUS_WIN_LINE"
  SPLIT_FRAME_WIDTH="$(pair_width "$SPLIT_FRAME_SIZE")"
  SPLIT_FOCUS_X="$(awk -v wx="$SPLIT_FOCUS_WX" -v frame_x="$SPLIT_FRAME_X" -v frame_w="$SPLIT_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  SPLIT_FOCUS_Y=$((SPLIT_FOCUS_WY + SPLIT_FOCUS_WH / 2))
  SPLIT_FOCUS_HIT_START_LINE="$(log_line_count)"
  click_global_point "$SPLIT_FOCUS_X" "$SPLIT_FOCUS_Y" "split_focus_browser"
  SPLIT_FOCUS_HIT_LINE="$(wait_for_hit_after "$SPLIT_FOCUS_HIT_START_LINE" "$CONTEXT_ID" "split-right browser-pane focus hit-test")"
  log "PASS: focused split-right browser pane before zoom"
  require_text "$SPLIT_FOCUS_HIT_LINE" "overlay_frame=" "split-right browser-pane focus hit-test includes current overlay frame"
  require_text "$SPLIT_FOCUS_HIT_LINE" "web_point={" "split-right browser-pane focus hit-test includes webview-relative point"

  ZOOM_START_LINE="$(log_line_count)"
  ZOOM_TRACE_START_LINE="$(trace_line_count)"
  log "zoom_keybind=ctrl+z=toggle_split_zoom"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 6 control >>"$HARNESS_LOG" 2>&1
  delay 1

  ZOOM_PRESENT_LINE="$(wait_for_split_right_zoom_frame_after "$ZOOM_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$OVERLAY_FRAME_SIZE" "$SPLIT_FRAME_SIZE" "split-right zoomed AppKit overlay frame")"
  ZOOM_PIXELS_LINE="$(wait_for_split_right_zoom_pixels_after "$ZOOM_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$APPKIT_PIXEL" "$SPLIT_PIXEL" "split-right zoomed AppKit presented pixels")"
  ZOOM_FRAME_SIZE="$(extract_frame_size "$ZOOM_PRESENT_LINE")"
  ZOOM_FRAME_X="$(extract_frame_x "$ZOOM_PRESENT_LINE")"
  ZOOM_PIXEL="$(extract_appkit_pixel "$ZOOM_PIXELS_LINE")"
  ZOOM_PIXEL_WIDTH="${ZOOM_PIXEL%x*}"
  ZOOM_PIXEL_HEIGHT="${ZOOM_PIXEL#*x}"
  log "PASS: observed split-right zoomed AppKit overlay frame overlay_frame_size=$ZOOM_FRAME_SIZE"
  log "PASS: observed split-right zoomed AppKit presented pixels appkit_pixel=$ZOOM_PIXEL"
  log "zoom_overlay_frame_size=$ZOOM_FRAME_SIZE"
  log "zoom_overlay_frame_x=$ZOOM_FRAME_X"
  log "zoom_appkit_pixel=$ZOOM_PIXEL"
  require_log_after "$ZOOM_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${ZOOM_PIXEL}" "Zig records split-right zoomed AppKit presented pixel size"
  require_trace_after "$ZOOM_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${ZOOM_PIXEL_WIDTH} pixel_height=${ZOOM_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-right zoomed resize to AppKit pixel size via ts_set_view_size"
  screencapture -x -o -l"$WID" "$SCREENSHOT_ZOOM"
  log "zoom_screenshot_exit=$?"

  ZOOM_WIN_LINE="$(window_bounds)" || fail "failed to resolve zoomed window bounds for window id=$WID"
  IFS=$'\t' read -r _ZOOM_WID ZOOM_WX ZOOM_WY ZOOM_WW ZOOM_WH <<<"$ZOOM_WIN_LINE"
  ZOOM_FRAME_WIDTH="$(pair_width "$ZOOM_FRAME_SIZE")"
  ZOOM_INSIDE_X="$(awk -v wx="$ZOOM_WX" -v frame_x="$ZOOM_FRAME_X" -v frame_w="$ZOOM_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  ZOOM_INSIDE_Y=$((ZOOM_WY + ZOOM_WH / 2))
  ZOOM_HIT_START_LINE="$(log_line_count)"
  click_global_point "$ZOOM_INSIDE_X" "$ZOOM_INSIDE_Y" "zoom_inside"
  ZOOM_HIT_LINE="$(wait_for_hit_after "$ZOOM_HIT_START_LINE" "$CONTEXT_ID" "split-right zoomed AppKit hit-test")"
  log "PASS: observed split-right zoomed AppKit hit-test"
  require_text "$ZOOM_HIT_LINE" "overlay_frame=" "split-right zoomed hit-test includes current overlay frame"
  require_text "$ZOOM_HIT_LINE" "web_point={" "split-right zoomed hit-test includes webview-relative point"

  UNZOOM_START_LINE="$(log_line_count)"
  UNZOOM_TRACE_START_LINE="$(trace_line_count)"
  log "unzoom_keybind=ctrl+z=toggle_split_zoom"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 6 control >>"$HARNESS_LOG" 2>&1
  delay 1

  UNZOOM_PRESENT_LINE="$(wait_for_split_right_equalize_frame_after "$UNZOOM_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$SPLIT_FRAME_SIZE" "$ZOOM_FRAME_SIZE" "split-right unzoomed AppKit overlay frame")"
  UNZOOM_PIXELS_LINE="$(wait_for_split_right_equalize_pixels_after "$UNZOOM_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$SPLIT_PIXEL" "$ZOOM_PIXEL" "split-right unzoomed AppKit presented pixels")"
  UNZOOM_FRAME_SIZE="$(extract_frame_size "$UNZOOM_PRESENT_LINE")"
  UNZOOM_FRAME_X="$(extract_frame_x "$UNZOOM_PRESENT_LINE")"
  UNZOOM_PIXEL="$(extract_appkit_pixel "$UNZOOM_PIXELS_LINE")"
  UNZOOM_PIXEL_WIDTH="${UNZOOM_PIXEL%x*}"
  UNZOOM_PIXEL_HEIGHT="${UNZOOM_PIXEL#*x}"
  log "PASS: observed split-right unzoomed AppKit overlay frame overlay_frame_size=$UNZOOM_FRAME_SIZE"
  log "PASS: observed split-right unzoomed AppKit presented pixels appkit_pixel=$UNZOOM_PIXEL"
  log "unzoom_overlay_frame_size=$UNZOOM_FRAME_SIZE"
  log "unzoom_overlay_frame_x=$UNZOOM_FRAME_X"
  log "unzoom_appkit_pixel=$UNZOOM_PIXEL"
  require_log_after "$UNZOOM_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${UNZOOM_PIXEL}" "Zig records split-right unzoomed AppKit presented pixel size"
  require_trace_after "$UNZOOM_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${UNZOOM_PIXEL_WIDTH} pixel_height=${UNZOOM_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-right unzoomed resize to AppKit pixel size via ts_set_view_size"
  screencapture -x -o -l"$WID" "$SCREENSHOT_UNZOOM"
  log "unzoom_screenshot_exit=$?"

  UNZOOM_WIN_LINE="$(window_bounds)" || fail "failed to resolve unzoomed window bounds for window id=$WID"
  IFS=$'\t' read -r _UNZOOM_WID UNZOOM_WX UNZOOM_WY UNZOOM_WW UNZOOM_WH <<<"$UNZOOM_WIN_LINE"
  UNZOOM_FRAME_WIDTH="$(pair_width "$UNZOOM_FRAME_SIZE")"
  UNZOOM_INSIDE_X="$(awk -v wx="$UNZOOM_WX" -v frame_x="$UNZOOM_FRAME_X" -v frame_w="$UNZOOM_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  UNZOOM_INSIDE_Y=$((UNZOOM_WY + UNZOOM_WH / 2))
  UNZOOM_HIT_START_LINE="$(log_line_count)"
  click_global_point "$UNZOOM_INSIDE_X" "$UNZOOM_INSIDE_Y" "unzoom_inside"
  UNZOOM_HIT_LINE="$(wait_for_hit_after "$UNZOOM_HIT_START_LINE" "$CONTEXT_ID" "split-right unzoomed AppKit hit-test")"
  log "PASS: observed split-right unzoomed AppKit hit-test"
  require_text "$UNZOOM_HIT_LINE" "overlay_frame=" "split-right unzoomed hit-test includes current overlay frame"
  require_text "$UNZOOM_HIT_LINE" "web_point={" "split-right unzoomed hit-test includes webview-relative point"

  UNZOOM_NEGATIVE_X="$(awk -v wx="$UNZOOM_WX" -v frame_x="$UNZOOM_FRAME_X" -v frame_w="$UNZOOM_FRAME_WIDTH" -v ww="$UNZOOM_WW" 'BEGIN { print int(wx + frame_x + frame_w + ((ww - frame_x - frame_w) / 2) + 0.5) }')"
  UNZOOM_NEGATIVE_Y="$UNZOOM_INSIDE_Y"
  click_negative_global_point "$UNZOOM_NEGATIVE_X" "$UNZOOM_NEGATIVE_Y" "unzoom_sibling_negative"
  wait_for_negative_hit_after "$NEGATIVE_HIT_START_LINE" "$CONTEXT_ID" "split-right unzoomed sibling-pane negative hit-test" allow-absent
fi

if [ "$SCENARIO" = "split-right-close-sibling" ]; then
  log "confirm_close_surface=false"
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
  require_trace_after "$SPLIT_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${SPLIT_PIXEL_WIDTH} pixel_height=${SPLIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-right resize to AppKit pixel size via ts_set_view_size"

  SPLIT_FRAME_WIDTH="$(pair_width "$SPLIT_FRAME_SIZE")"
  CLOSE_START_LINE="$(log_line_count)"
  CLOSE_TRACE_START_LINE="$(trace_line_count)"
  log "close_keybind=ctrl+k=close_surface"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 40 control >>"$HARNESS_LOG" 2>&1
  delay 1

  CLOSE_PRESENT_LINE="$(wait_for_split_right_zoom_frame_after "$CLOSE_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$OVERLAY_FRAME_SIZE" "$SPLIT_FRAME_SIZE" "split-right sibling-closed AppKit overlay frame")"
  CLOSE_PIXELS_LINE="$(wait_for_split_right_zoom_pixels_after "$CLOSE_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$APPKIT_PIXEL" "$SPLIT_PIXEL" "split-right sibling-closed AppKit presented pixels")"
  CLOSE_FRAME_SIZE="$(extract_frame_size "$CLOSE_PRESENT_LINE")"
  CLOSE_FRAME_X="$(extract_frame_x "$CLOSE_PRESENT_LINE")"
  CLOSE_PIXEL="$(extract_appkit_pixel "$CLOSE_PIXELS_LINE")"
  CLOSE_PIXEL_WIDTH="${CLOSE_PIXEL%x*}"
  CLOSE_PIXEL_HEIGHT="${CLOSE_PIXEL#*x}"
  log "PASS: observed split-right sibling-closed AppKit overlay frame overlay_frame_size=$CLOSE_FRAME_SIZE"
  log "PASS: observed split-right sibling-closed AppKit presented pixels appkit_pixel=$CLOSE_PIXEL"
  log "close_overlay_frame_size=$CLOSE_FRAME_SIZE"
  log "close_overlay_frame_x=$CLOSE_FRAME_X"
  log "close_appkit_pixel=$CLOSE_PIXEL"
  require_log_after "$CLOSE_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${CLOSE_PIXEL}" "Zig records split-right sibling-closed AppKit presented pixel size"
  require_trace_after "$CLOSE_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${CLOSE_PIXEL_WIDTH} pixel_height=${CLOSE_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-right sibling-closed resize to AppKit pixel size via ts_set_view_size"
  screencapture -x -o -l"$WID" "$SCREENSHOT_CLOSE"
  log "close_screenshot_exit=$?"

  CLOSE_WIN_LINE="$(window_bounds)" || fail "failed to resolve sibling-closed window bounds for window id=$WID"
  IFS=$'\t' read -r _CLOSE_WID CLOSE_WX CLOSE_WY CLOSE_WW CLOSE_WH <<<"$CLOSE_WIN_LINE"
  CLOSE_FRAME_WIDTH="$(pair_width "$CLOSE_FRAME_SIZE")"
  CLOSE_INSIDE_X="$(awk -v wx="$CLOSE_WX" -v frame_x="$CLOSE_FRAME_X" -v frame_w="$CLOSE_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  CLOSE_INSIDE_Y=$((CLOSE_WY + CLOSE_WH / 2))
  CLOSE_HIT_START_LINE="$(log_line_count)"
  click_global_point "$CLOSE_INSIDE_X" "$CLOSE_INSIDE_Y" "close_inside"
  CLOSE_HIT_LINE="$(wait_for_hit_after "$CLOSE_HIT_START_LINE" "$CONTEXT_ID" "split-right sibling-closed AppKit hit-test")"
  log "PASS: observed split-right sibling-closed AppKit hit-test"
  require_text "$CLOSE_HIT_LINE" "overlay_frame=" "split-right sibling-closed hit-test includes current overlay frame"
  require_text "$CLOSE_HIT_LINE" "web_point={" "split-right sibling-closed hit-test includes webview-relative point"

  FORMER_SIBLING_X="$(awk -v wx="$CLOSE_WX" -v frame_x="$CLOSE_FRAME_X" -v split_w="$SPLIT_FRAME_WIDTH" -v close_w="$CLOSE_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + split_w + ((close_w - split_w) / 2) + 0.5) }')"
  FORMER_SIBLING_Y="$CLOSE_INSIDE_Y"
  FORMER_SIBLING_HIT_START_LINE="$(log_line_count)"
  click_global_point "$FORMER_SIBLING_X" "$FORMER_SIBLING_Y" "former_sibling_inside"
  FORMER_SIBLING_HIT_LINE="$(wait_for_hit_after "$FORMER_SIBLING_HIT_START_LINE" "$CONTEXT_ID" "former sibling area AppKit hit-test after close")"
  log "PASS: observed former sibling area AppKit hit-test after close"
  require_text "$FORMER_SIBLING_HIT_LINE" "overlay_frame=" "former sibling area hit-test includes current overlay frame"
  require_text "$FORMER_SIBLING_HIT_LINE" "web_point={" "former sibling area hit-test includes webview-relative point"
fi

if [ "$SCENARIO" = "split-right-close-browser-pane" ]; then
  log "confirm_close_surface=false"
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
  SPLIT_FRAME_WIDTH="$(pair_width "$SPLIT_FRAME_SIZE")"
  INITIAL_FRAME_WIDTH="$(pair_width "$OVERLAY_FRAME_SIZE")"
  log "PASS: observed split-right AppKit overlay frame overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "PASS: observed split-right AppKit presented pixels appkit_pixel=$SPLIT_PIXEL"
  log "split_overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "split_overlay_frame_x=$SPLIT_FRAME_X"
  log "split_appkit_pixel=$SPLIT_PIXEL"
  require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${SPLIT_PIXEL}" "Zig records split-right AppKit presented pixel size"
  require_trace_after "$SPLIT_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${SPLIT_PIXEL_WIDTH} pixel_height=${SPLIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-right resize to AppKit pixel size via ts_set_view_size"

  SPLIT_WIN_LINE="$(window_bounds)" || fail "failed to resolve split window bounds for window id=$WID"
  IFS=$'\t' read -r _SPLIT_WID SPLIT_WX SPLIT_WY SPLIT_WW SPLIT_WH <<<"$SPLIT_WIN_LINE"
  BROWSER_FOCUS_X="$(awk -v wx="$SPLIT_WX" -v frame_x="$SPLIT_FRAME_X" -v frame_w="$SPLIT_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  BROWSER_FOCUS_Y=$((SPLIT_WY + SPLIT_WH / 2))
  BROWSER_FOCUS_HIT_START_LINE="$(log_line_count)"
  click_global_point "$BROWSER_FOCUS_X" "$BROWSER_FOCUS_Y" "close_browser_focus"
  BROWSER_FOCUS_HIT_LINE="$(wait_for_hit_after "$BROWSER_FOCUS_HIT_START_LINE" "$CONTEXT_ID" "split-right browser-pane focus hit-test")"
  log "PASS: focused split-right browser pane before close"
  require_text "$BROWSER_FOCUS_HIT_LINE" "overlay_frame=" "split-right browser-pane focus hit-test includes current overlay frame"
  require_text "$BROWSER_FOCUS_HIT_LINE" "web_point={" "split-right browser-pane focus hit-test includes webview-relative point"

  CLOSE_START_LINE="$(log_line_count)"
  CLOSE_TRACE_START_LINE="$(trace_line_count)"
  log "close_keybind=ctrl+k=close_surface"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 40 control >>"$HARNESS_LOG" 2>&1
  delay 1

  CLEAR_OVERLAY_SEEN=""
  for _ in $(seq 1 30); do
    if tail -n +"$((CLOSE_START_LINE + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=zig event=clear_overlay_call .*pane_id:${PANE_ID}" >/dev/null 2>&1; then
      CLEAR_OVERLAY_SEEN="1"
      break
    fi
    if tail -n +"$((CLOSE_TRACE_START_LINE + 1))" "$CHROMIUM_TRACE" | grep -F "key-event tab=${BROWSER_TAB_ID} pane=${PANE_ID}" >/dev/null 2>&1; then
      fail "Control-K was forwarded to Chromium browser input before close_surface cleanup"
    fi
    delay 1
  done
  [ -n "$CLEAR_OVERLAY_SEEN" ] || fail "timed out waiting for Zig records browser-pane clear_overlay_call after close"
  log "PASS: Zig records browser-pane clear_overlay_call after close"

  wait_for_log_after "$CLOSE_START_LINE" "TermSurf geometry layer=bridge event=clear_request .*pane_id:${PANE_ID}" "Swift bridge records browser-pane clear_request after close"

  CLEAR_RESULT=""
  for _ in $(seq 1 30); do
    if tail -n +"$((CLOSE_START_LINE + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=bridge event=clear_target_found .*pane_id:${PANE_ID}" >/dev/null 2>&1 &&
      tail -n +"$((CLOSE_START_LINE + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=clear .*pane_id:${PANE_ID}" >/dev/null 2>&1; then
      CLEAR_RESULT="target-found"
      break
    fi
    if tail -n +"$((CLOSE_START_LINE + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=bridge event=clear_rejected .*pane_id:${PANE_ID} .*note=no-surface" >/dev/null 2>&1; then
      CLEAR_RESULT="surface-already-gone"
      break
    fi
    delay 1
  done
  [ -n "$CLEAR_RESULT" ] || fail "missing AppKit clear or bridge no-surface cleanup evidence after browser-pane close"
  log "PASS: observed browser-pane clear result clear_result=$CLEAR_RESULT"

  require_log_after "$CLOSE_START_LINE" "CloseTab: pane_id=${PANE_ID} tab_id=${BROWSER_TAB_ID}" "Zig records CloseTab for browser pane after close"
  require_trace_after "$CLOSE_TRACE_START_LINE" "close-tab tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} result=destroying ffi=ts_destroy_web_contents" "Chromium received CloseTab and destroyed browser tab"
  require_trace_after "$CLOSE_TRACE_START_LINE" "close-tab tab_id=${BROWSER_TAB_ID} result=removed" "Chromium removed closed browser tab"

  screencapture -x -o -l"$WID" "$SCREENSHOT_CLOSE"
  log "close_screenshot_exit=$?"

  FORMER_BROWSER_X="$BROWSER_FOCUS_X"
  FORMER_BROWSER_Y="$BROWSER_FOCUS_Y"
  click_negative_global_point "$FORMER_BROWSER_X" "$FORMER_BROWSER_Y" "former_browser_after_close"
  wait_for_negative_hit_after "$NEGATIVE_HIT_START_LINE" "$CONTEXT_ID" "former browser-pane area after browser close" allow-absent

  REMAINING_SIBLING_X="$(awk -v wx="$SPLIT_WX" -v frame_x="$SPLIT_FRAME_X" -v split_w="$SPLIT_FRAME_WIDTH" -v initial_w="$INITIAL_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + split_w + ((initial_w - split_w) / 2) + 0.5) }')"
  REMAINING_SIBLING_Y="$BROWSER_FOCUS_Y"
  click_negative_global_point "$REMAINING_SIBLING_X" "$REMAINING_SIBLING_Y" "remaining_sibling_after_close"
  wait_for_negative_hit_after "$NEGATIVE_HIT_START_LINE" "$CONTEXT_ID" "remaining sibling area after browser close" allow-absent

  SIBLING_KEY_START_LINE="$(log_line_count)"
  printf 'ISSUE809_EXP10_SIBLING_ALIVE\n' >"$SIBLING_ALIVE_COMMAND"
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$SIBLING_ALIVE_COMMAND" >>"$HARNESS_LOG" 2>&1
  SIBLING_KEY_SEEN=""
  for _ in $(seq 1 10); do
    if tail -n +"$((SIBLING_KEY_START_LINE + 1))" "$APP_LOG" | grep -E "TermSurf geometry layer=appkit event=key_down .*overlay_frame=none .*visible=false .*focused=true" >/dev/null 2>&1; then
      SIBLING_KEY_SEEN="1"
      break
    fi
    delay 1
  done
  [ -n "$SIBLING_KEY_SEEN" ] || fail "remaining sibling pane did not receive post-close keyboard events"
  log "PASS: remaining sibling pane received post-close keyboard events"
fi

if [ "$SCENARIO" = "split-down" ]; then
  SPLIT_START_LINE="$(log_line_count)"
  SPLIT_TRACE_START_LINE="$(trace_line_count)"
  log "split_keybind=ctrl+j=new_split:down"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 38 control >>"$HARNESS_LOG" 2>&1
  delay 1

  SPLIT_PRESENT_LINE="$(wait_for_split_down_frame_after "$SPLIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$OVERLAY_FRAME_SIZE" "split-down AppKit overlay frame")"
  SPLIT_PIXELS_LINE="$(wait_for_split_down_pixels_after "$SPLIT_START_LINE" "$PANE_ID" "$CONTEXT_ID" "$APPKIT_PIXEL" "split-down AppKit presented pixels")"
  SPLIT_FRAME_SIZE="$(extract_frame_size "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_X="$(extract_frame_x "$SPLIT_PRESENT_LINE")"
  SPLIT_FRAME_Y="$(extract_frame_y "$SPLIT_PRESENT_LINE")"
  SPLIT_PIXEL="$(extract_appkit_pixel "$SPLIT_PIXELS_LINE")"
  SPLIT_PIXEL_WIDTH="${SPLIT_PIXEL%x*}"
  SPLIT_PIXEL_HEIGHT="${SPLIT_PIXEL#*x}"
  log "PASS: observed split-down AppKit overlay frame overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "PASS: observed split-down AppKit presented pixels appkit_pixel=$SPLIT_PIXEL"
  log "split_overlay_frame_size=$SPLIT_FRAME_SIZE"
  log "split_overlay_frame_x=$SPLIT_FRAME_X"
  log "split_overlay_frame_y=$SPLIT_FRAME_Y"
  log "split_appkit_pixel=$SPLIT_PIXEL"
  require_log_after "$SPLIT_START_LINE" "TermSurf geometry layer=zig event=appkit_presented_pixels .*pane_id:${PANE_ID} .*appkit_pixel=${SPLIT_PIXEL}" "Zig records split-down AppKit presented pixel size"
  require_trace_after "$SPLIT_TRACE_START_LINE" "resize tab_id=${BROWSER_TAB_ID} pane_id=${PANE_ID} pixel_width=${SPLIT_PIXEL_WIDTH} pixel_height=${SPLIT_PIXEL_HEIGHT} screen_x=0 screen_y=0 screen_width=0 screen_height=0 screen_scale=0 ffi=ts_set_view_size" "Chromium applied split-down resize to AppKit pixel size via ts_set_view_size"
  screencapture -x -o -l"$WID" "$SCREENSHOT_SPLIT"
  log "split_screenshot_exit=$?"

  SPLIT_WIN_LINE="$(window_bounds)" || fail "failed to resolve split window bounds for window id=$WID"
  IFS=$'\t' read -r _SPLIT_WID SPLIT_WX SPLIT_WY SPLIT_WW SPLIT_WH <<<"$SPLIT_WIN_LINE"
  SPLIT_FRAME_WIDTH="$(pair_width "$SPLIT_FRAME_SIZE")"
  SPLIT_FRAME_HEIGHT="$(pair_height "$SPLIT_FRAME_SIZE")"
  INITIAL_FRAME_HEIGHT="$(pair_height "$OVERLAY_FRAME_SIZE")"
  SPLIT_INSIDE_X="$(awk -v wx="$SPLIT_WX" -v frame_x="$SPLIT_FRAME_X" -v frame_w="$SPLIT_FRAME_WIDTH" 'BEGIN { print int(wx + frame_x + (frame_w / 2) + 0.5) }')"
  SPLIT_INSIDE_Y=$((SPLIT_WY + 150))
  SPLIT_HIT_START_LINE="$(log_line_count)"
  click_global_point "$SPLIT_INSIDE_X" "$SPLIT_INSIDE_Y" "split_inside"
  SPLIT_HIT_LINE="$(wait_for_hit_after "$SPLIT_HIT_START_LINE" "$CONTEXT_ID" "split-down AppKit hit-test")"
  log "PASS: observed split-down AppKit hit-test"
  require_text "$SPLIT_HIT_LINE" "overlay_frame=" "split-down hit-test includes current overlay frame"
  require_text "$SPLIT_HIT_LINE" "web_point={" "split-down hit-test includes webview-relative point"

  SPLIT_NEGATIVE_X="$SPLIT_INSIDE_X"
  SPLIT_TOP_POINT="$(extract_top_point "$SPLIT_HIT_LINE")"
  SPLIT_TOP_POINT_Y="$(point_y "$SPLIT_TOP_POINT")"
  [ -n "$SPLIT_TOP_POINT_Y" ] && [ "$SPLIT_TOP_POINT_Y" != "$SPLIT_TOP_POINT" ] || fail "split-down hit-test missing top_point y"
  SPLIT_ROOT_HEIGHT="$(pair_height "$(extract_root_frame_size "$SPLIT_PRESENT_LINE")")"
  SPLIT_TOP_GLOBAL_OFFSET="$(awk -v global_y="$SPLIT_INSIDE_Y" -v top_y="$SPLIT_TOP_POINT_Y" 'BEGIN { print int(global_y - top_y) }')"
  SPLIT_NEGATIVE_Y="$(awk -v offset="$SPLIT_TOP_GLOBAL_OFFSET" -v root_h="$SPLIT_ROOT_HEIGHT" 'BEGIN { print int(offset + root_h + 24) }')"
  click_negative_global_point "$SPLIT_NEGATIVE_X" "$SPLIT_NEGATIVE_Y" "split_sibling_negative"
  wait_for_negative_hit_after "$NEGATIVE_HIT_START_LINE" "$CONTEXT_ID" "split-down sibling-pane negative hit-test" allow-absent
fi

log "PASS: scenario $SCENARIO"
