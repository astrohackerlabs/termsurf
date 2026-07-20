#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$ROOT/logs/issue-26062812000867-exp3-release-no-env"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-issue867-exp3.XXXXXX")"
APP="${TERMSURF_RELEASE_GHOSTBOARD_APP:-$ROOT/forks/ghostty/macos/build/Release/Astrohacker Terminal.app}"
APP_BIN="$APP/Contents/MacOS/ahterm"
WEB="${TERMSURF_RELEASE_WEB:-$ROOT/rust/target/release/ahweb}"
ROAMIUM="/opt/homebrew/opt/astrohacker-terminal-ah-chromiumd/ah-chromiumd"
SURFARI="/opt/homebrew/opt/astrohacker-terminal-ah-webkitd/ah-webkitd"
HARNESS_LOG="$LOG_DIR/harness-$RUN_ID.log"
PID=""

mkdir -p "$LOG_DIR"

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

require_unset() {
  local name="$1"
  if [ -n "${!name+x}" ]; then
    fail "$name must be unset for the release no-env smoke"
  fi
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

extract_pane_id() {
  printf '%s\n' "$1" | sed -E 's/.*pane_id[:=]([^ ]+).*/\1/'
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

stop_app() {
  if [ -n "${PID:-}" ] && kill -0 "$PID" >/dev/null 2>&1; then
    kill "$PID" >/dev/null 2>&1 || true
    delay 0.5 || true
    kill -9 "$PID" >/dev/null 2>&1 || true
  fi
  PID=""
}

start_app() {
  local scenario="$1"
  local command="$2"
  local app_log="$3"
  env \
    -u ASTROHACKER_CHROMIUM_PATH \
    -u ASTROHACKER_WEBKIT_PATH \
    -u ASTROHACKER_LADYBIRD_PATH \
    -u TERMSURF_ROAMIUM_PATH \
    -u TERMSURF_SURFARI_PATH \
    -u TERMSURF_INSTALLED_ROAMIUM_PATH \
    -u TERMSURF_INSTALLED_SURFARI_PATH \
    -u DYLD_FRAMEWORK_PATH \
    GHOSTTY_LOG=stderr \
    TERMSURF_GEOMETRY_TRACE=1 \
    TERMSURF_GEOMETRY_SCENARIO="issue867-exp3-$scenario" \
    "$APP_BIN" \
    --window-save-state=never \
    --confirm-close-surface=false \
    --initial-command="direct:$command" >"$app_log" 2>&1 &
  PID="$!"
  log "scenario=$scenario pid=$PID app_log=$app_log"
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

wait_webtui_loaded() {
  local trace_file="$1"
  local browser="$2"
  local label="$3"
  local attempts="${4:-120}"
  for _ in $(seq 1 "$attempts"); do
    if grep -F "event=render_state" "$trace_file" 2>/dev/null |
      grep -F "browser_ready=true" |
      grep -F "page_loaded=true" |
      grep -F "loading_bar_active=false" |
      grep -F "browser_label=${browser}" |
      grep -E "title=Example Domain|url=https://example.com/?" >/dev/null 2>&1; then
      log "PASS: $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

run_browser_smoke() {
  local browser="$1"
  local path="$2"
  local path_env="$3"
  local installed_env="$4"
  local scenario="$browser-example-com"
  local command="$RUN_DIR/$scenario-command.sh"
  local app_log="$LOG_DIR/app-$scenario-$RUN_ID.log"
  local trace="$LOG_DIR/webtui-$scenario-$RUN_ID.log"
  local screenshot="$LOG_DIR/screenshot-$scenario-$RUN_ID.png"
  local start setoverlay ready presented pane

  cat >"$command" <<EOF
#!/usr/bin/env bash
set -euo pipefail
export TERMSURF_WEBTUI_STATE_TRACE_FILE="$trace"
exec "$WEB" --browser "$browser" https://example.com
EOF
  chmod +x "$command"

  start_app "$scenario" "$command" "$app_log"
  start="$(line_count "$app_log")"
  setoverlay="$(wait_for_line_after "$app_log" "$start" "SetOverlay: pane_id=.* browser=${browser} .*url=https://example.com" "$browser SetOverlay" 60)"
  pane="$(extract_pane_id "$setoverlay")"
  wait_for_line_after "$app_log" "$start" "SetOverlay: named browser resolved browser=${browser} installed_path=${path}" "$browser installed default resolution" 60 >/dev/null
  wait_for_line_after "$app_log" "$start" "spawned browser path=${path} .* browser=${browser} " "$browser spawned installed binary" 60 >/dev/null
  if [ "$browser" = "webkit" ]; then
    wait_for_line_after "$app_log" "$start" "browser spawn runtime env browser=webkit DYLD_FRAMEWORK_PATH=/opt/homebrew/opt/astrohacker-terminal-ah-webkitd" "webkit runtime env supplied by Astrohacker Terminal" 60 >/dev/null
  fi
  ready="$(wait_for_line_after "$app_log" "$start" "BrowserReady: pane_id=.* browser=${browser}" "$browser BrowserReady" 160)"
  [ "$pane" = "$(extract_pane_id "$ready")" ] || fail "$browser BrowserReady pane mismatch"
  presented="$(wait_for_line_after "$app_log" "$start" "TermSurf geometry layer=appkit event=presented .*pane_id:${pane} .*context_id=[1-9][0-9]*" "$browser AppKit presentation" 90)"
  wait_webtui_loaded "$trace" "$browser" "$browser WebTUI loaded https://example.com"
  require_no_line_after "$app_log" "$start" "named browser resolved browser=${browser} env=${path_env}" "$browser did not resolve through $path_env"
  require_no_line_after "$app_log" "$start" "named browser resolved browser=${browser} env=${installed_env}" "$browser did not resolve through $installed_env"
  capture_window "$presented" "$screenshot"
  log "PASS: $browser no-env release smoke app_log=$app_log trace=$trace screenshot=$screenshot"
  stop_app
}

require_unset ASTROHACKER_CHROMIUM_PATH
require_unset ASTROHACKER_WEBKIT_PATH
require_unset ASTROHACKER_LADYBIRD_PATH
require_unset TERMSURF_ROAMIUM_PATH
require_unset TERMSURF_SURFARI_PATH
require_unset TERMSURF_INSTALLED_ROAMIUM_PATH
require_unset TERMSURF_INSTALLED_SURFARI_PATH
require_unset DYLD_FRAMEWORK_PATH

case "$APP_BIN" in
  "$ROOT/forks/ghostty/macos/build/Release/Astrohacker Terminal.app/Contents/MacOS/ahterm") ;;
  *) fail "expected repo Release app binary, got: $APP_BIN" ;;
esac

require_executable "$APP_BIN"
require_executable "$WEB"
require_executable "$ROAMIUM"
require_executable "$SURFARI"

log "run_id=$RUN_ID"
log "app_bin=$APP_BIN"
log "web=$WEB"
log "chromium_helper=$ROAMIUM"
log "webkit_helper=$SURFARI"
log "harness_log=$HARNESS_LOG"
log "network_url=https://example.com"

"$APP_BIN" +version >>"$HARNESS_LOG" 2>&1 || true
"$WEB" --version >>"$HARNESS_LOG" 2>&1 || true

run_browser_smoke "webkit" "$SURFARI" "ASTROHACKER_WEBKIT_PATH" "TERMSURF_SURFARI_PATH"
run_browser_smoke "chromium" "$ROAMIUM" "ASTROHACKER_CHROMIUM_PATH" "TERMSURF_ROAMIUM_PATH"

log "PASS: issue 867 experiment 3 release no-env browser discovery"
