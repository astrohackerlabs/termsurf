#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$ROOT/logs/issue-756-exp25-surfari-lifecycle"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-issue756-exp25.XXXXXX")"
APP="${TERMSURF_GHOSTBOARD_APP:-$ROOT/ghostboard/macos/build/Debug/TermSurf.app}"
APP_BIN="$APP/Contents/MacOS/termsurf"
WEB="${TERMSURF_WEB:-$ROOT/target/debug/web}"
SURFARI="${TERMSURF_SURFARI:-$ROOT/target/debug/surfari}"
WEBKIT_DEBUG="${TERMSURF_WEBKIT_DEBUG:-$ROOT/webkit/src/WebKitBuild/Debug}"
SITE_DIR="$RUN_DIR/site"
CONFIG="$RUN_DIR/config"
COMMAND="$RUN_DIR/run-web-surfari.sh"
APP_LOG="$LOG_DIR/app-$RUN_ID.log"
HARNESS_LOG="$LOG_DIR/harness-$RUN_ID.log"
SURFARI_TRACE="$LOG_DIR/surfari-trace-$RUN_ID.log"
WEBTUI_TRACE="$LOG_DIR/webtui-$RUN_ID.log"
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

require_path() {
  [ -e "$1" ] || fail "missing path: $1"
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
    if tail -n +"$((start_line + 1))" "$file" | grep -E "$pattern" >/dev/null 2>&1; then
      log "PASS: $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

extract_first_match_after() {
  local file="$1"
  local start_line="$2"
  local pattern="$3"
  tail -n +"$((start_line + 1))" "$file" | grep -E "$pattern" | head -1 || true
}

send_browser_navigate() {
  local socket_path="$1"
  local tab_id="$2"
  local url="$3"
  python3 - "$socket_path" "$tab_id" "$url" <<'PY'
import socket
import struct
import sys

socket_path = sys.argv[1]
tab_id = int(sys.argv[2])
url = sys.argv[3]

def varint(value):
    out = bytearray()
    while value >= 0x80:
        out.append((value & 0x7F) | 0x80)
        value >>= 7
    out.append(value)
    return bytes(out)

def field(number, wire_type):
    return varint((number << 3) | wire_type)

def string_field(number, value):
    data = value.encode("utf-8")
    return field(number, 2) + varint(len(data)) + data

payload = field(1, 0) + varint(tab_id) + string_field(3, url)
message = field(5, 2) + varint(len(payload)) + payload
with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
    sock.connect(socket_path)
    sock.sendall(struct.pack("<I", len(message)) + message)
PY
}

send_browser_close() {
  local socket_path="$1"
  local tab_id="$2"
  python3 - "$socket_path" "$tab_id" <<'PY'
import socket
import struct
import sys

socket_path = sys.argv[1]
tab_id = int(sys.argv[2])

def varint(value):
    out = bytearray()
    while value >= 0x80:
        out.append((value & 0x7F) | 0x80)
        value >>= 7
    out.append(value)
    return bytes(out)

def field(number, wire_type):
    return varint((number << 3) | wire_type)

payload = field(1, 0) + varint(tab_id)
message = field(4, 2) + varint(len(payload)) + payload
with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
    sock.connect(socket_path)
    sock.sendall(struct.pack("<I", len(message)) + message)
PY
}

cat >"$SITE_DIR/a.html" <<'EOF'
<!doctype html>
<meta charset="utf-8">
<title>Issue 756 Lifecycle A</title>
<main style="min-height:1600px;font:18px system-ui,sans-serif">
  <h1>ISSUE756_EXP25_LIFECYCLE_A</h1>
  <script>
    console.log("ISSUE756_EXP25_READY_A");
  </script>
</main>
EOF

cat >"$SITE_DIR/b.html" <<'EOF'
<!doctype html>
<meta charset="utf-8">
<title>Issue 756 Lifecycle B</title>
<main style="min-height:1600px;font:18px system-ui,sans-serif">
  <h1>ISSUE756_EXP25_LIFECYCLE_B</h1>
  <script>
    console.log("ISSUE756_EXP25_READY_B");
  </script>
</main>
EOF

HTTP_PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
PY
)"
URL_A="http://127.0.0.1:${HTTP_PORT}/a.html"
URL_B="http://127.0.0.1:${HTTP_PORT}/b.html"

python3 -m http.server "$HTTP_PORT" --bind 127.0.0.1 --directory "$SITE_DIR" >>"$HARNESS_LOG" 2>&1 &
HTTP_PID="$!"
for _ in $(seq 1 30); do
  if python3 - "$URL_A" <<'PY' >/dev/null 2>&1
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
python3 - "$URL_A" <<'PY' >/dev/null 2>&1 || fail "HTTP fixture did not become ready"
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY

cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
exec "$WEB" --browser surfari "$URL_A"
EOF
chmod +x "$COMMAND"

cat >"$CONFIG" <<EOF
window-save-state = never
initial-command = direct:$COMMAND
EOF

require_executable "$APP_BIN"
require_executable "$WEB"
require_executable "$SURFARI"
require_path "$WEBKIT_DEBUG/WebKit.framework"
require_path "$ROOT/surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib"

log "run_id=$RUN_ID"
log "app_bin=$APP_BIN"
log "web=$WEB"
log "surfari=$SURFARI"
log "webkit_debug=$WEBKIT_DEBUG"
log "url_a=$URL_A"
log "url_b=$URL_B"
log "app_log=$APP_LOG"
log "surfari_trace=$SURFARI_TRACE"
log "webtui_trace=$WEBTUI_TRACE"

GHOSTTY_CONFIG_PATH="$CONFIG" \
GHOSTTY_LOG=stderr \
DYLD_FRAMEWORK_PATH="$WEBKIT_DEBUG" \
TERMSURF_SURFARI_PATH="$SURFARI" \
TERMSURF_GEOMETRY_TRACE=1 \
TERMSURF_GEOMETRY_SCENARIO=issue756-exp25-surfari-lifecycle \
TERMSURF_WEBTUI_STATE_TRACE_FILE="$WEBTUI_TRACE" \
TERMSURF_INPUT_TRACE=1 \
TERMSURF_PDF_INPUT_TRACE=1 \
TERMSURF_PDF_INPUT_TRACE_FILE="$SURFARI_TRACE" \
  "$APP_BIN" >"$APP_LOG" 2>&1 &
PID="$!"
log "pid=$PID"

RUN1_APP_START="$(line_count "$APP_LOG")"
RUN1_TRACE_START="$(line_count "$SURFARI_TRACE")"
RUN1_WEBTUI_START="$(line_count "$WEBTUI_TRACE")"

wait_for_file_pattern_after "$APP_LOG" "$RUN1_APP_START" "BrowserReady: pane_id=.* browser=surfari" "run 1 BrowserReady"
wait_for_file_pattern_after "$APP_LOG" "$RUN1_APP_START" "TermSurf geometry layer=appkit event=presented " "run 1 AppKit presented overlay"
wait_for_file_pattern_after "$SURFARI_TRACE" "$RUN1_TRACE_START" "create-tab pane=.* url=${URL_A}" "run 1 Surfari created fixture A tab"
wait_for_file_pattern_after "$SURFARI_TRACE" "$RUN1_TRACE_START" "title-changed tab=.*title=Issue 756 Lifecycle A" "run 1 Surfari loaded fixture A title"
wait_for_file_pattern_after "$WEBTUI_TRACE" "$RUN1_WEBTUI_START" "event=title_changed.*title=Issue 756 Lifecycle A" "run 1 WebTUI saw fixture A title"

BROWSER_READY_LINE="$(extract_first_match_after "$APP_LOG" "$RUN1_APP_START" "BrowserReady: pane_id=.* browser=surfari")"
BROWSER_SOCKET="$(printf '%s\n' "$BROWSER_READY_LINE" | sed -E 's/.*socket=([^ ]+) browser=surfari.*/\1/')"
BROWSER_TAB_ID="$(printf '%s\n' "$BROWSER_READY_LINE" | sed -E 's/.*tab_id=([0-9]+) socket=.*/\1/')"
case "$BROWSER_SOCKET" in
  /*) ;;
  *) fail "could not extract browser socket from BrowserReady: $BROWSER_READY_LINE" ;;
esac
case "$BROWSER_TAB_ID" in
  ''|*[!0-9]*) fail "could not extract tab id from BrowserReady: $BROWSER_READY_LINE" ;;
esac

NAV_TRACE_START="$(line_count "$SURFARI_TRACE")"
NAV_WEBTUI_START="$(line_count "$WEBTUI_TRACE")"
send_browser_navigate "$BROWSER_SOCKET" "$BROWSER_TAB_ID" "$URL_B"
wait_for_file_pattern_after "$SURFARI_TRACE" "$NAV_TRACE_START" "navigate tab=${BROWSER_TAB_ID} pane=.* url=${URL_B} ffi=ts_load_url" "Surfari received explicit Navigate"
wait_for_file_pattern_after "$SURFARI_TRACE" "$NAV_TRACE_START" "url-changed tab=${BROWSER_TAB_ID} .*url=${URL_B}" "Surfari emitted fixture B URL"
wait_for_file_pattern_after "$SURFARI_TRACE" "$NAV_TRACE_START" "title-changed tab=${BROWSER_TAB_ID} .*title=Issue 756 Lifecycle B" "Surfari loaded fixture B title"
wait_for_file_pattern_after "$WEBTUI_TRACE" "$NAV_WEBTUI_START" "event=url_changed.*url=${URL_B}" "WebTUI saw fixture B URL"
wait_for_file_pattern_after "$WEBTUI_TRACE" "$NAV_WEBTUI_START" "event=title_changed.*title=Issue 756 Lifecycle B" "WebTUI saw fixture B title"

RESIZE_START="$(line_count "$SURFARI_TRACE")"
CURRENT_SIZE="$(osascript -e "tell application \"System Events\" to get size of first window of application process \"TermSurf\"" 2>>"$HARNESS_LOG" || true)"
if [ "$CURRENT_SIZE" = "900, 680" ]; then
  RESIZE_WIDTH=950
  RESIZE_HEIGHT=720
else
  RESIZE_WIDTH=900
  RESIZE_HEIGHT=680
fi
log "resize_target=${RESIZE_WIDTH}x${RESIZE_HEIGHT} current_size=${CURRENT_SIZE:-unknown}"
osascript -e "tell application \"System Events\" to set size of first window of application process \"TermSurf\" to {$RESIZE_WIDTH, $RESIZE_HEIGHT}" >>"$HARNESS_LOG" 2>&1 || fail "window resize automation failed"
wait_for_file_pattern_after "$SURFARI_TRACE" "$RESIZE_START" "resize tab_id=${BROWSER_TAB_ID} .*pixel_width=.*pixel_height=.*ffi=ts_set_view_size" "Surfari resized after real app window resize"

CLOSE_TRACE_START="$(line_count "$SURFARI_TRACE")"
send_browser_close "$BROWSER_SOCKET" "$BROWSER_TAB_ID"
wait_for_file_pattern_after "$SURFARI_TRACE" "$CLOSE_TRACE_START" "close-tab tab_id=${BROWSER_TAB_ID} result=removed" "Surfari accepted CloseTab"
wait_for_file_pattern_after "$SURFARI_TRACE" "$CLOSE_TRACE_START" "close-tab result=no-tabs-remaining" "Surfari began clean shutdown"

kill "$PID" >/dev/null 2>&1 || true
delay 1
PID=""

RUN2_APP_START="$(line_count "$APP_LOG")"
RUN2_TRACE_START="$(line_count "$SURFARI_TRACE")"
RUN2_WEBTUI_START="$(line_count "$WEBTUI_TRACE")"
GHOSTTY_CONFIG_PATH="$CONFIG" \
GHOSTTY_LOG=stderr \
DYLD_FRAMEWORK_PATH="$WEBKIT_DEBUG" \
TERMSURF_SURFARI_PATH="$SURFARI" \
TERMSURF_GEOMETRY_TRACE=1 \
TERMSURF_GEOMETRY_SCENARIO=issue756-exp25-surfari-lifecycle-restart \
TERMSURF_WEBTUI_STATE_TRACE_FILE="$WEBTUI_TRACE" \
TERMSURF_INPUT_TRACE=1 \
TERMSURF_PDF_INPUT_TRACE=1 \
TERMSURF_PDF_INPUT_TRACE_FILE="$SURFARI_TRACE" \
  "$APP_BIN" >>"$APP_LOG" 2>&1 &
PID="$!"
log "restart_pid=$PID"

wait_for_file_pattern_after "$SURFARI_TRACE" "$RUN2_TRACE_START" "trace-init pid=" "run 2 Surfari trace initialized after restart"
wait_for_file_pattern_after "$APP_LOG" "$RUN2_APP_START" "BrowserReady: pane_id=.* browser=surfari" "run 2 BrowserReady after restart"
wait_for_file_pattern_after "$APP_LOG" "$RUN2_APP_START" "TermSurf geometry layer=appkit event=presented " "run 2 AppKit presented overlay after restart"
wait_for_file_pattern_after "$SURFARI_TRACE" "$RUN2_TRACE_START" "create-tab pane=.* url=${URL_A}" "run 2 Surfari created fixture A tab"
wait_for_file_pattern_after "$SURFARI_TRACE" "$RUN2_TRACE_START" "title-changed tab=.*title=Issue 756 Lifecycle A" "run 2 Surfari loaded fixture A title"
wait_for_file_pattern_after "$WEBTUI_TRACE" "$RUN2_WEBTUI_START" "event=title_changed.*title=Issue 756 Lifecycle A" "run 2 WebTUI saw fixture A title"

log "PASS: issue 756 Surfari lifecycle tranche"
