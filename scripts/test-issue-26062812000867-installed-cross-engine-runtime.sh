#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$ROOT/logs/issue-26062812000867-exp2-installed-runtime"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-issue867-exp2.XXXXXX")"
APP="${TERMSURF_GHOSTBOARD_APP:-$ROOT/forks/ghostty/macos/build/Debug/Astrohacker Terminal.app}"
APP_BIN="$APP/Contents/MacOS/ahterm"
WEB="${TERMSURF_WEB:-$ROOT/rust/target/debug/ahweb}"
ROAMIUM="${TERMSURF_ROAMIUM:-/opt/homebrew/opt/astrohacker-terminal-ah-chromiumd/ah-chromiumd}"
SURFARI="${TERMSURF_SURFARI:-/opt/homebrew/opt/astrohacker-terminal-ah-webkitd/ah-webkitd}"
SURFARI_PREFIX="${TERMSURF_SURFARI_PREFIX:-/opt/homebrew/opt/astrohacker-terminal-ah-webkitd}"
WEBKIT_FRAMEWORK="$SURFARI_PREFIX/WebKit.framework"
HARNESS_LOG="$LOG_DIR/harness-$RUN_ID.log"
SITE_DIR="$RUN_DIR/site"
PID=""

mkdir -p "$LOG_DIR" "$SITE_DIR"

log() {
  printf '%s\n' "$*" | tee -a "$HARNESS_LOG" >&2
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
  if [ -f "$file" ]; then
    wc -l <"$file" | tr -d ' '
  else
    printf '0\n'
  fi
}

wait_for_line_after() {
  local file="$1"
  local start_line="$2"
  local pattern="$3"
  local label="$4"
  local attempts="${5:-90}"
  local line
  for _ in $(seq 1 "$attempts"); do
    line="$(tail -n +"$((start_line + 1))" "$file" 2>/dev/null | grep -E "$pattern" | tail -1 || true)"
    if [ -n "$line" ]; then
      log "PASS: $label"
      printf '%s\n' "$line"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

require_no_line_after() {
  local file="$1"
  local start_line="$2"
  local pattern="$3"
  local label="$4"
  if tail -n +"$((start_line + 1))" "$file" 2>/dev/null | grep -E "$pattern" >/dev/null 2>&1; then
    fail "$label"
  fi
  log "PASS: $label"
}

line_number_after() {
  local file="$1"
  local start_line="$2"
  local pattern="$3"
  tail -n +"$((start_line + 1))" "$file" 2>/dev/null |
    grep -n -E "$pattern" |
    head -1 |
    sed -E 's/:.*//' |
    awk -v start="$start_line" '{ print start + $1 }'
}

extract_pane_id() {
  printf '%s\n' "$1" | sed -E 's/.*pane_id[:=]([^ ]+).*/\1/'
}

extract_ready_tab_id() {
  printf '%s\n' "$1" | sed -E 's/.*tab_id=([0-9]+).*/\1/'
}

extract_context_id() {
  printf '%s\n' "$1" | sed -E 's/.*context_id=([0-9]+).*/\1/'
}

extract_window_id() {
  printf '%s\n' "$1" | sed -E 's/.*identity=window_id:([0-9]+).*/\1/'
}

validate_number() {
  local value="$1"
  local label="$2"
  case "$value" in
    ''|*[!0-9]*) fail "$label is not numeric: $value" ;;
  esac
  [ "$value" -gt 0 ] || fail "$label is zero"
}

fixture_title() {
  case "$1" in
    roamium) printf 'Issue 26062812000867 Roamium Fixture\n' ;;
    surfari) printf 'Issue 26062812000867 Surfari Fixture\n' ;;
    *) fail "unknown browser for fixture title: $1" ;;
  esac
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
with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
    sock.connect(socket_path)
    sock.sendall(struct.pack("<I", len(message)) + message)
PY
}

record_version() {
  local name="$1"
  local bin="$2"
  log "${name}_path=$bin"
  stat "$bin" >>"$HARNESS_LOG" 2>&1 || true
  for arg in +version --version; do
    python3 - "$bin" "$arg" >>"$HARNESS_LOG" 2>&1 <<'PY' || true
import subprocess
import sys

bin_path, arg = sys.argv[1:]
print(f"version_probe command={bin_path} {arg}")
try:
    result = subprocess.run(
        [bin_path, arg],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        timeout=5,
    )
    print(result.stdout.strip())
    print(f"version_probe exit={result.returncode}")
except Exception as exc:
    print(f"version_probe failed={exc}")
PY
  done
}

capture_window() {
  local presented_line="$1"
  local screenshot="$2"
  local wid
  wid="$(extract_window_id "$presented_line")"
  validate_number "$wid" "window id"
  screencapture -x -o -l"$wid" "$screenshot"
  [ -s "$screenshot" ] || fail "screenshot not written: $screenshot"
  log "PASS: screenshot=$screenshot"
}

resize_front_window() {
  local width="$1"
  local height="$2"
  osascript -e 'tell application "TermSurf" to activate' >>"$HARNESS_LOG" 2>&1 || true
  osascript -e "tell application \"System Events\" to set size of first window of application process \"TermSurf\" to {$width, $height}" >>"$HARNESS_LOG" 2>&1 || true
  delay 2
}

type_webtui_quit() {
  local text_file="$1"
  printf ':quit' >"$text_file"
  osascript -e 'tell application "TermSurf" to activate' >>"$HARNESS_LOG" 2>&1 || true
  swift "$ROOT/scripts/ghostty-app/inject.swift" type "$text_file" >>"$HARNESS_LOG" 2>&1
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
}

write_fixture() {
  cat >"$SITE_DIR/roamium.html" <<'EOF'
<!doctype html>
<meta charset="utf-8">
<title>Issue 26062812000867 Roamium Fixture</title>
<h1>ISSUE867_ROAMIUM_READY</h1>
EOF
  cat >"$SITE_DIR/surfari.html" <<'EOF'
<!doctype html>
<meta charset="utf-8">
<title>Issue 26062812000867 Surfari Fixture</title>
<h1>ISSUE867_SURFARI_READY</h1>
EOF
}

start_app() {
  local scenario="$1"
  local command="$2"
  local app_log="$3"
  local config="$RUN_DIR/$scenario-config"
  cat >"$config" <<EOF
window-save-state = never
initial-command = direct:$command
confirm-close-surface = false
EOF
  GHOSTTY_CONFIG_PATH="$config" \
  GHOSTTY_LOG=stderr \
  DYLD_FRAMEWORK_PATH="$SURFARI_PREFIX" \
  TERMSURF_ROAMIUM_PATH="$ROAMIUM" \
  TERMSURF_SURFARI_PATH="$SURFARI" \
  TERMSURF_GEOMETRY_TRACE=1 \
  TERMSURF_GEOMETRY_SCENARIO="issue867-exp2-$scenario" \
    "$APP_BIN" >"$app_log" 2>&1 &
  PID="$!"
  log "scenario=$scenario pid=$PID app_log=$app_log"
}

stop_app() {
  if [ -n "${PID:-}" ] && kill -0 "$PID" >/dev/null 2>&1; then
    kill "$PID" >/dev/null 2>&1 || true
    delay 0.5 || true
    kill -9 "$PID" >/dev/null 2>&1 || true
  fi
  PID=""
}

wait_present_after() {
  local app_log="$1"
  local start="$2"
  local pane_id="$3"
  local label="$4"
  wait_for_line_after "$app_log" "$start" "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=[1-9][0-9]*" "$label" 60
}

wait_present_or_still_live_after() {
  local app_log="$1"
  local marker="$2"
  local prior_start="$3"
  local pane_id="$4"
  local label="$5"
  local line numbered line_no

  line="$(tail -n +"$((marker + 1))" "$app_log" 2>/dev/null | grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=[1-9][0-9]*" | tail -1 || true)"
  if [ -n "$line" ]; then
    log "PASS: $label"
    printf '%s\n' "$line"
    return 0
  fi

  numbered="$(tail -n +"$((prior_start + 1))" "$app_log" 2>/dev/null | grep -n -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=[1-9][0-9]*" | tail -1 || true)"
  [ -n "$numbered" ] || fail "timed out waiting for $label"
  line_no="${numbered%%:*}"
  line_no="$((prior_start + line_no))"
  line="${numbered#*:}"
  require_no_line_after "$app_log" "$line_no" "TermSurf geometry layer=appkit event=clear .*pane_id:${pane_id}" "$label was not cleared after latest presentation"
  log "PASS: $label (latest presentation remained live)"
  printf '%s\n' "$line"
}

wait_ready_after() {
  local app_log="$1"
  local start="$2"
  local browser="$3"
  local label="$4"
  wait_for_line_after "$app_log" "$start" "BrowserReady: pane_id=.* browser=${browser}" "$label" 120
}

wait_webtui_loaded() {
  local trace_file="$1"
  local browser="$2"
  local title="$3"
  local label="$4"
  local attempts="${5:-60}"
  for _ in $(seq 1 "$attempts"); do
    if grep -F "event=render_state" "$trace_file" 2>/dev/null |
      grep -F "browser_ready=true" |
      grep -F "page_loaded=true" |
      grep -F "loading_bar_active=false" |
      grep -F "browser_label=${browser}" |
      grep -F "title=${title}" >/dev/null 2>&1; then
      log "PASS: $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

assert_isolated_after() {
  local app_log="$1"
  local start="$2"
  local pane_id="$3"
  local browser="$4"
  require_no_line_after "$app_log" "$start" "ClearOverlay: pane_id=${pane_id}" "$browser pane was not cleared"
  require_no_line_after "$app_log" "$start" "TUI disconnect cleanup: pane_id=${pane_id}" "$browser TUI was not disconnected"
  require_no_line_after "$app_log" "$start" "Pane close cleanup: pane_id=${pane_id}" "$browser pane was not closed"
  require_no_line_after "$app_log" "$start" "Browser disconnect: detached browser server profile=.* browser=${browser}" "$browser server was not detached"
}

run_split_scenario() {
  local scenario="$1"
  local first="$2"
  local second="$3"
  local first_url="$4"
  local second_url="$5"
  local capture="$RUN_DIR/$scenario-capture.tsv"
  local first_cmd="$RUN_DIR/$scenario-first.sh"
  local second_cmd="$RUN_DIR/$scenario-second.sh"
  local app_log="$LOG_DIR/app-$scenario-$RUN_ID.log"
  local screenshot="$LOG_DIR/screenshot-$scenario-$RUN_ID.png"
  local first_trace="$LOG_DIR/webtui-$scenario-first-$RUN_ID.log"
  local second_trace="$LOG_DIR/webtui-$scenario-second-$RUN_ID.log"

  cat >"$first_cmd" <<EOF
#!/usr/bin/env bash
set -euo pipefail
printf 'first\t%s\t%s\n' "\$TERMSURF_PANE_ID" "\$TERMSURF_SOCKET" >>"$capture"
export TERMSURF_WEBTUI_STATE_TRACE_FILE="$first_trace"
exec "$WEB" --browser "$first" "$first_url"
EOF
  chmod +x "$first_cmd"

  cat >"$second_cmd" <<EOF
#!/usr/bin/env bash
set -euo pipefail
printf 'second\t%s\t%s\n' "\$TERMSURF_PANE_ID" "\$TERMSURF_SOCKET" >>"$capture"
export TERMSURF_WEBTUI_STATE_TRACE_FILE="$second_trace"
exec "$WEB" --browser "$second" "$second_url"
EOF
  chmod +x "$second_cmd"

  start_app "$scenario" "$first_cmd" "$app_log"
  local start first_ready first_pane first_tab first_present first_context socket second_start second_ready second_pane second_tab second_present second_context resize_start first_post second_post
  start="$(line_count "$app_log")"
  first_ready="$(wait_ready_after "$app_log" "$start" "$first" "$scenario first $first BrowserReady")"
  wait_webtui_loaded "$first_trace" "$first" "$(fixture_title "$first")" "$scenario first $first WebTUI loaded"
  first_pane="$(extract_pane_id "$first_ready")"
  first_tab="$(extract_ready_tab_id "$first_ready")"
  validate_number "$first_tab" "$scenario first tab id"
  first_present="$(wait_present_after "$app_log" "$start" "$first_pane" "$scenario first $first presented")"
  first_context="$(extract_context_id "$first_present")"
  validate_number "$first_context" "$scenario first context id"
  socket="$(awk -F '\t' '$1 == "first" { print $3; exit }' "$capture")"
  [ -S "$socket" ] || fail "$scenario missing captured socket: $socket"

  second_start="$(line_count "$app_log")"
  send_open_split_message "$socket" "$first_pane" "right" "$second_cmd"
  wait_for_line_after "$app_log" "$second_start" "OpenSplit: pane_id=${first_pane} direction=right" "$scenario OpenSplit handled" 30 >/dev/null
  second_ready="$(wait_ready_after "$app_log" "$second_start" "$second" "$scenario second $second BrowserReady")"
  wait_webtui_loaded "$second_trace" "$second" "$(fixture_title "$second")" "$scenario second $second WebTUI loaded"
  second_pane="$(extract_pane_id "$second_ready")"
  second_tab="$(extract_ready_tab_id "$second_ready")"
  validate_number "$second_tab" "$scenario second tab id"
  [ "$first_pane" != "$second_pane" ] || fail "$scenario panes share id $first_pane"
  second_present="$(wait_present_after "$app_log" "$second_start" "$second_pane" "$scenario second $second presented")"
  second_context="$(extract_context_id "$second_present")"
  validate_number "$second_context" "$scenario second context id"
  [ "$first_context" != "$second_context" ] || fail "$scenario contexts share id $first_context"

  resize_start="$(line_count "$app_log")"
  resize_front_window 980 740
  first_post="$(wait_present_or_still_live_after "$app_log" "$resize_start" "$second_start" "$first_pane" "$scenario first still presented after second ready")"
  second_post="$(wait_present_or_still_live_after "$app_log" "$resize_start" "$second_start" "$second_pane" "$scenario second still presented after second ready")"
  [ "$(extract_context_id "$first_post")" != "$(extract_context_id "$second_post")" ] || fail "$scenario post-ready contexts collided"
  capture_window "$second_post" "$screenshot"
  assert_isolated_after "$app_log" "$second_start" "$first_pane" "$first"
  log "PASS: $scenario first_pane=$first_pane first_tab=$first_tab first_context=$first_context second_pane=$second_pane second_tab=$second_tab second_context=$second_context"
  stop_app
}

run_same_pane_scenario() {
  local scenario="$1"
  local first="$2"
  local second="$3"
  local first_url="$4"
  local second_url="$5"
  local capture="$RUN_DIR/$scenario-capture.tsv"
  local command="$RUN_DIR/$scenario-command.sh"
  local app_log="$LOG_DIR/app-$scenario-$RUN_ID.log"
  local screenshot="$LOG_DIR/screenshot-$scenario-$RUN_ID.png"
  local first_trace="$LOG_DIR/webtui-$scenario-first-$RUN_ID.log"
  local second_trace="$LOG_DIR/webtui-$scenario-second-$RUN_ID.log"
  local quit_text="$RUN_DIR/$scenario-quit.txt"

  cat >"$command" <<EOF
#!/usr/bin/env bash
set -euo pipefail
printf 'same\t%s\t%s\n' "\$TERMSURF_PANE_ID" "\$TERMSURF_SOCKET" >>"$capture"
export TERMSURF_WEBTUI_STATE_TRACE_FILE="$first_trace"
"$WEB" --browser "$first" "$first_url"
printf 'after-first\t%s\t%s\n' "\$TERMSURF_PANE_ID" "\$TERMSURF_SOCKET" >>"$capture"
export TERMSURF_WEBTUI_STATE_TRACE_FILE="$second_trace"
exec "$WEB" --browser "$second" "$second_url"
EOF
  chmod +x "$command"

  start_app "$scenario" "$command" "$app_log"
  local start first_ready pane first_tab first_present first_context quit_start second_ready second_pane second_tab second_present second_context
  start="$(line_count "$app_log")"
  first_ready="$(wait_ready_after "$app_log" "$start" "$first" "$scenario first $first BrowserReady")"
  wait_webtui_loaded "$first_trace" "$first" "$(fixture_title "$first")" "$scenario first $first WebTUI loaded"
  pane="$(extract_pane_id "$first_ready")"
  first_tab="$(extract_ready_tab_id "$first_ready")"
  validate_number "$first_tab" "$scenario first tab id"
  first_present="$(wait_present_after "$app_log" "$start" "$pane" "$scenario first $first presented")"
  first_context="$(extract_context_id "$first_present")"
  validate_number "$first_context" "$scenario first context id"

  quit_start="$(line_count "$app_log")"
  type_webtui_quit "$quit_text"
  wait_for_line_after "$app_log" "$quit_start" "TUI disconnect cleanup: pane_id=${pane} tab_id=${first_tab}" "$scenario first TUI cleanup" 60 >/dev/null
  wait_for_line_after "$app_log" "$quit_start" "SetOverlay: pane_id=${pane} profile=default browser=${second}" "$scenario second SetOverlay same pane" 60 >/dev/null
  local cleanup_line setoverlay_line
  cleanup_line="$(line_number_after "$app_log" "$quit_start" "TUI disconnect cleanup: pane_id=${pane} tab_id=${first_tab}")"
  setoverlay_line="$(line_number_after "$app_log" "$quit_start" "SetOverlay: pane_id=${pane} profile=default browser=${second}")"
  [ -n "$cleanup_line" ] && [ -n "$setoverlay_line" ] || fail "$scenario missing cleanup/SetOverlay line number"
  [ "$cleanup_line" -lt "$setoverlay_line" ] || fail "$scenario SetOverlay occurred before first TUI cleanup"
  log "PASS: $scenario cleanup preceded second SetOverlay"
  second_ready="$(wait_ready_after "$app_log" "$quit_start" "$second" "$scenario second $second BrowserReady")"
  wait_webtui_loaded "$second_trace" "$second" "$(fixture_title "$second")" "$scenario second $second WebTUI loaded"
  second_pane="$(extract_pane_id "$second_ready")"
  [ "$pane" = "$second_pane" ] || fail "$scenario pane changed old=$pane new=$second_pane"
  second_tab="$(extract_ready_tab_id "$second_ready")"
  validate_number "$second_tab" "$scenario second tab id"
  if [ "$first_tab" = "$second_tab" ]; then
    log "INFO: $scenario reused numeric tab id $first_tab across different browser engines"
  fi
  second_present="$(wait_present_after "$app_log" "$quit_start" "$pane" "$scenario second $second presented")"
  second_context="$(extract_context_id "$second_present")"
  validate_number "$second_context" "$scenario second context id"
  [ "$first_context" != "$second_context" ] || fail "$scenario context id reused $first_context"
  capture_window "$second_present" "$screenshot"
  log "PASS: $scenario pane=$pane first_tab=$first_tab second_tab=$second_tab first_context=$first_context second_context=$second_context"
  stop_app
}

write_fixture

ROAMIUM_URL="file://$SITE_DIR/roamium.html"
SURFARI_URL="file://$SITE_DIR/surfari.html"

require_executable "$APP_BIN"
require_executable "$WEB"
require_executable "$ROAMIUM"
require_executable "$SURFARI"
require_path "$WEBKIT_FRAMEWORK"

log "run_id=$RUN_ID"
log "app_bin=$APP_BIN"
log "web=$WEB"
log "roamium=$ROAMIUM"
log "surfari=$SURFARI"
log "surfari_prefix=$SURFARI_PREFIX"
log "webkit_framework=$WEBKIT_FRAMEWORK"
log "webkit_framework_realpath=$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$WEBKIT_FRAMEWORK")"
log "roamium_url=$ROAMIUM_URL"
log "surfari_url=$SURFARI_URL"
log "harness_log=$HARNESS_LOG"

record_version chromium "$ROAMIUM"
record_version webkit "$SURFARI"

run_split_scenario "chromium-then-webkit-split" "chromium" "webkit" "$ROAMIUM_URL" "$SURFARI_URL"
run_split_scenario "webkit-then-chromium-split" "webkit" "chromium" "$SURFARI_URL" "$ROAMIUM_URL"
run_same_pane_scenario "chromium-then-webkit-same-pane" "chromium" "webkit" "$ROAMIUM_URL" "$SURFARI_URL"
run_same_pane_scenario "webkit-then-chromium-same-pane" "webkit" "chromium" "$SURFARI_URL" "$ROAMIUM_URL"

log "PASS: issue 867 experiment 2 installed cross-engine runtime"
