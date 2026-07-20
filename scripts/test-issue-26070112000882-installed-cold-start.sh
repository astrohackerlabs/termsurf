#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="$(date +%Y%m%d-%H%M%S)"
START_EPOCH="$(date +%s)"
LOG_DIR="${TERMSURF_ISSUE882_LOG_DIR:-$ROOT/logs/issue-26070112000882-exp1-installed-cold-start}"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-issue882-exp1.XXXXXX")"
CASK_TOKEN="${ASTROHACKER_HOMEBREW_CASK:-astrohacker}"
APP="/Applications/Astrohacker Terminal.app"
APP_BIN="$APP/Contents/MacOS/ahterm"
WEB="/opt/homebrew/bin/ahweb"
CHROMIUM="/opt/homebrew/opt/astrohacker-terminal-ah-chromiumd/ah-chromiumd"
WEBKIT="/opt/homebrew/opt/astrohacker-terminal-ah-webkitd/ah-webkitd"
LADYBIRD="/opt/homebrew/opt/astrohacker-terminal-ah-ladybirdd/bin/ah-ladybirdd"
POSTFLIGHT_WARMUP_LOG="/opt/homebrew/var/log/astrohacker/terminal-postflight-warmup.log"
SUMMARY="$LOG_DIR/summary-$RUN_ID.tsv"
HARNESS_LOG="$LOG_DIR/harness-$RUN_ID.log"
BROWSER="all"
REINSTALL=false
FRESH_SPAWN=false
EXPECT_POSTFLIGHT_WARMUP="${ASTROHACKER_TERMINAL_EXPECT_POSTFLIGHT_WARMUP:-${TERMSURF_ISSUE882_EXPECT_POSTFLIGHT_WARMUP:-0}}"
PID=""

mkdir -p "$LOG_DIR"

usage() {
  cat >&2 <<'USAGE'
usage: scripts/test-issue-26070112000882-installed-cold-start.sh [--browser chromium|webkit|ladybird|all] [--reinstall] [--fresh-spawn]

Measures installed Homebrew browser startup timing for Issue 26070112000882.

  --browser       Select engine(s). Default: all.
  --reinstall     Run brew reinstall --cask "$CASK_TOKEN" before each selected
                  engine's first measured launch.
  --fresh-spawn   Kill/reap app and engine processes before each launch so the
                  run measures fresh process startup, not server reuse.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --browser)
      BROWSER="${2:-}"
      shift 2
      ;;
    --reinstall)
      REINSTALL=true
      shift
      ;;
    --fresh-spawn)
      FRESH_SPAWN=true
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

case "$BROWSER" in
  roamium) BROWSER="chromium" ;;
  surfari) BROWSER="webkit" ;;
esac

case "$BROWSER" in
  chromium | webkit | ladybird | all) ;;
  *)
    echo "invalid --browser: $BROWSER" >&2
    exit 1
    ;;
esac

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
  stop_app || true
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

require_executable() {
  [ -x "$1" ] || fail "missing executable: $1"
}

require_unset() {
  local name="$1"
  if [ -n "${!name+x}" ]; then
    fail "$name must be unset for installed Homebrew cold-start smoke"
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
  local attempts="${5:-180}"
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

wait_for_tab_ready_after() {
  local file="$1"
  local start_line="$2"
  local browser="$3"
  local pane="$4"
  local label="$5"
  local attempts="${6:-180}"
  local line
  for _ in $(seq 1 "$attempts"); do
    line="$(tail -n +"$((start_line + 1))" "$file" 2>/dev/null | grep -E "TermSurfBrowserStartup event=tab_ready .* browser=${browser}" | tail -1 || true)"
    if [ -z "$line" ]; then
      line="$(tail -n +"$((start_line + 1))" "$file" 2>/dev/null | grep -E "TabReady: pane_id=${pane} tab_id=[0-9]+" | tail -1 || true)"
    fi
    if [ -n "$line" ]; then
      log "PASS: $label"
      printf '%s\n' "$line"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label"
}

extract_field() {
  local line="$1"
  local name="$2"
  printf '%s\n' "$line" | tr ' ' '\n' | sed -n "s/^${name}=//p" | tail -1
}

expect_postflight_warmup() {
  [ "$EXPECT_POSTFLIGHT_WARMUP" != "0" ] && [ "$EXPECT_POSTFLIGHT_WARMUP" != "false" ]
}

verify_postflight_warmup() {
  local scenario="$1"
  local copy_path="$LOG_DIR/postflight-warmup-$scenario-$RUN_ID.log"
  local engine start_line main_line exit_line done_line duration

  [ -f "$POSTFLIGHT_WARMUP_LOG" ] || fail "missing postflight warmup log: $POSTFLIGHT_WARMUP_LOG"
  cp "$POSTFLIGHT_WARMUP_LOG" "$copy_path"

  for engine in chromium webkit ladybird; do
    start_line="$(grep -E "AstrohackerTerminalPostflightWarmup event=start engine=${engine} " "$copy_path" | tail -1 || true)"
    done_line="$(grep -E "AstrohackerTerminalPostflightWarmup event=done engine=${engine} " "$copy_path" | tail -1 || true)"
    [ -n "$start_line" ] || fail "missing postflight warmup start for $engine in $copy_path"
    [ -n "$done_line" ] || fail "missing postflight warmup done for $engine in $copy_path"
    printf '%s\n' "$done_line" | grep -F "success=true" >/dev/null ||
      fail "postflight warmup did not report success for $engine in $copy_path"
    case "$engine" in
      chromium)
        main_line="$(grep -F "TermSurfEngineStartup event=main_entry" "$copy_path" | grep -F "engine=chromium" | grep -F "browser=chromium" | tail -1 || true)"
        exit_line="$(grep -F "TermSurfEngineStartup event=warmup_exit" "$copy_path" | grep -F "engine=chromium" | grep -F "browser=chromium" | tail -1 || true)"
        [ -n "$main_line" ] || fail "missing postflight warmup Chromium main_entry in $copy_path"
        [ -n "$exit_line" ] || fail "missing postflight warmup Chromium warmup_exit in $copy_path"
        ;;
      webkit)
        main_line="$(grep -F "TermSurfEngineStartup event=main_entry" "$copy_path" | grep -F "engine=webkit" | grep -F "browser=webkit" | tail -1 || true)"
        exit_line="$(grep -F "TermSurfEngineStartup event=warmup_exit" "$copy_path" | grep -F "engine=webkit" | grep -F "browser=webkit" | tail -1 || true)"
        [ -n "$main_line" ] || fail "missing postflight warmup WebKit main_entry in $copy_path"
        [ -n "$exit_line" ] || fail "missing postflight warmup WebKit warmup_exit in $copy_path"
        ;;
      ladybird)
        grep -E "\[(Ladybird|Girlbat)\] warmup " "$copy_path" | grep -F "ok=true" >/dev/null ||
          fail "missing postflight warmup Ladybird ok=true output in $copy_path"
        ;;
    esac
    duration="$(extract_field "$done_line" "duration_ms")"
    validate_number "$duration" "postflight warmup duration for $engine"
    log "PASS: postflight warmup engine=$engine duration_ms=$duration log=$copy_path"
  done
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
    '' | *[!0-9]*) fail "$label is not numeric: $value" ;;
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

kill_engines() {
  pkill -x ah-chromiumd >/dev/null 2>&1 || true
  pkill -x ah-webkitd >/dev/null 2>&1 || true
  pkill -x ah-ladybirdd >/dev/null 2>&1 || true
  pkill -x aht >/dev/null 2>&1 || true
  delay 1 || true
}

process_alive() {
  local pid="$1"
  [ -n "$pid" ] && kill -0 "$pid" >/dev/null 2>&1
}

wait_for_process_exit() {
  local pid="$1"
  local label="$2"
  local attempts="${3:-20}"
  for _ in $(seq 1 "$attempts"); do
    if ! process_alive "$pid"; then
      log "PASS: $label exited"
      return 0
    fi
    delay 1
  done
  fail "$label still running after ${attempts}s pid=$pid"
}

require_no_chromium_crash_markers() {
  local app_log="$1"
  local engine_trace="$2"
  local label="$3"
  delay 1
  local pattern='Received signal [0-9]+|SEGV|TileTaskManagerImpl::Shutdown|Check failed:|FATAL:'
  if grep -E "$pattern" "$app_log" "$engine_trace" >/dev/null 2>&1; then
    fail "$label logged Chromium shutdown crash marker"
  fi
  log "PASS: $label did not log Chromium shutdown crash markers"
}

capture_validation_context() {
  local label="$1"
  local file="$2"
  {
    echo "label=$label"
    echo "captured_at=$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    echo "== xattr Astrohacker Terminal.app quarantine =="
    xattr -p com.apple.quarantine "$APP" 2>&1 || true
    echo "== xattr ah-chromiumd quarantine =="
    xattr -p com.apple.quarantine "$CHROMIUM" 2>&1 || true
    echo "== xattr ah-webkitd quarantine =="
    xattr -p com.apple.quarantine "$WEBKIT" 2>&1 || true
    echo "== xattr ah-ladybirdd quarantine =="
    xattr -p com.apple.quarantine "$LADYBIRD" 2>&1 || true
    echo "== spctl Astrohacker Terminal.app =="
    spctl -a -vv "$APP" 2>&1 || true
    echo "== spctl ah-chromiumd =="
    spctl -a -vv "$CHROMIUM" 2>&1 || true
    echo "== spctl ah-webkitd =="
    spctl -a -vv "$WEBKIT" 2>&1 || true
    echo "== spctl ah-ladybirdd =="
    spctl -a -vv "$LADYBIRD" 2>&1 || true
    echo "== unified log validation hints =="
    log show --last 5m --style compact \
      --predicate 'process == "syspolicyd" OR process == "amfid" OR process == "XprotectService"' 2>&1 || true
  } >"$file"
}

start_app() {
  local scenario="$1"
  local command="$2"
  local app_log="$3"
  local engine_trace="$4"
  env \
    -u ASTROHACKER_CHROMIUM_PATH \
    -u ASTROHACKER_WEBKIT_PATH \
    -u ASTROHACKER_LADYBIRD_PATH \
    -u TERMSURF_ROAMIUM_PATH \
    -u TERMSURF_SURFARI_PATH \
    -u TERMSURF_GIRLBAT_PATH \
    -u TERMSURF_INSTALLED_ROAMIUM_PATH \
    -u TERMSURF_INSTALLED_SURFARI_PATH \
    -u TERMSURF_INSTALLED_GIRLBAT_PATH \
    -u DYLD_FRAMEWORK_PATH \
    GHOSTTY_LOG=stderr \
    TERMSURF_GEOMETRY_TRACE=1 \
    TERMSURF_GEOMETRY_SCENARIO="issue882-exp1-$scenario" \
    TERMSURF_BROWSER_STARTUP_TRACE=1 \
    TERMSURF_ENGINE_STARTUP_TRACE=1 \
    TERMSURF_ENGINE_STARTUP_TRACE_FILE="$engine_trace" \
    "$APP_BIN" \
    --window-save-state=never \
    --confirm-close-surface=false \
    --initial-command="direct:$command" >"$app_log" 2>&1 &
  PID="$!"
  log "scenario=$scenario pid=$PID app_log=$app_log engine_trace=$engine_trace"
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
  local attempts="${4:-180}"
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

field_ms_after() {
  local file="$1"
  local start_line="$2"
  local pattern="$3"
  local field="${4:-wall_ms}"
  local line
  line="$(tail -n +"$((start_line + 1))" "$file" 2>/dev/null | grep -E "$pattern" | head -1 || true)"
  if [ -z "$line" ]; then
    printf '\n'
    return
  fi
  extract_field "$line" "$field"
}

delta() {
  local end="$1"
  local start="$2"
  if [ -z "$end" ] || [ -z "$start" ]; then
    printf 'NA'
  else
    printf '%s' "$((end - start))"
  fi
}

append_summary() {
  local mode="$1"
  local browser="$2"
  local app_log="$3"
  local engine_trace="$4"
  local start_line="$5"
  local screenshot_ms="$6"
  local webtui_loaded_ms="$7"
  local setoverlay spawn_start spawn_returned main_entry ts_entry init_entry ctx_created register_sent server_register create_tab tab_ready browser_ready webtui_loaded ca_context present_overlay engine_pid main_entry_browser

  setoverlay="$(field_ms_after "$app_log" "$start_line" "TermSurfBrowserStartup event=set_overlay .* browser=${browser}")"
  spawn_start="$(field_ms_after "$app_log" "$start_line" "TermSurfBrowserStartup event=spawn_start .* browser=${browser}")"
  spawn_returned="$(field_ms_after "$app_log" "$start_line" "TermSurfBrowserStartup event=spawn_returned .* browser=${browser}")"
  engine_pid="$(field_ms_after "$app_log" "$start_line" "TermSurfBrowserStartup event=spawn_returned .* browser=${browser}" "pid")"
  server_register="$(field_ms_after "$app_log" "$start_line" "TermSurfBrowserStartup event=server_register .* browser=${browser}")"
  create_tab="$(field_ms_after "$app_log" "$start_line" "TermSurfBrowserStartup event=create_tab .* browser=${browser}")"
  tab_ready="$(field_ms_after "$app_log" "$start_line" "TermSurfBrowserStartup event=tab_ready .* browser=${browser}")"
  browser_ready="$(field_ms_after "$app_log" "$start_line" "TermSurfBrowserStartup event=browser_ready .* browser=${browser}")"
  ca_context="$(field_ms_after "$app_log" "$start_line" "TermSurfBrowserStartup event=ca_context .* browser=${browser}")"
  present_overlay="$(field_ms_after "$app_log" "$start_line" "TermSurfBrowserStartup event=present_overlay .* browser=${browser}")"
  main_entry_browser="$browser"
  main_entry="$(field_ms_after "$engine_trace" 0 "TermSurfEngineStartup event=main_entry .* browser=${main_entry_browser} .* pid=${engine_pid}")"
  ts_entry="$(field_ms_after "$engine_trace" 0 "TermSurfEngineStartup event=ts_content_main_entry .* browser=${browser} .* pid=${engine_pid}")"
  init_entry="$(field_ms_after "$engine_trace" 0 "TermSurfEngineStartup event=on_initialized_entry .* browser=${browser} .* pid=${engine_pid}")"
  ctx_created="$(field_ms_after "$engine_trace" 0 "TermSurfEngineStartup event=browser_context_created .* browser=${browser} .* pid=${engine_pid}")"
  register_sent="$(field_ms_after "$engine_trace" 0 "TermSurfEngineStartup event=server_register_sent .* browser=${browser} .* pid=${engine_pid}")"
  webtui_loaded="$webtui_loaded_ms"

  {
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$mode" \
      "$browser" \
      "$(delta "$spawn_start" "$setoverlay")" \
      "$(delta "$spawn_returned" "$spawn_start")" \
      "$(delta "$main_entry" "$spawn_returned")" \
      "$(delta "$ts_entry" "$main_entry")" \
      "$(delta "$init_entry" "$ts_entry")" \
      "$(delta "$ctx_created" "$init_entry")" \
      "$(delta "$register_sent" "$ctx_created")" \
      "$(delta "$create_tab" "$server_register")" \
      "$(delta "$tab_ready" "$create_tab")" \
      "$(delta "$browser_ready" "$tab_ready")" \
      "$(delta "$webtui_loaded" "$browser_ready")" \
      "$(delta "$screenshot_ms" "$browser_ready")" \
      "$(delta "$ca_context" "$browser_ready")" \
      "$(delta "$present_overlay" "$ca_context")" \
      "$app_log" \
      "$engine_trace" \
      "$RUN_ID"
  } >>"$SUMMARY"
}

run_browser_once() {
  local browser="$1"
  local mode="$2"
  local binary="$3"
  local scenario="$browser-$mode"
  local command="$RUN_DIR/$scenario-command.sh"
  local app_log="$LOG_DIR/app-$scenario-$RUN_ID.log"
  local trace="$LOG_DIR/webtui-$scenario-$RUN_ID.log"
  local engine_trace="$LOG_DIR/engine-$scenario-$RUN_ID.log"
  local screenshot="$LOG_DIR/screenshot-$scenario-$RUN_ID.png"
  local validation="$LOG_DIR/validation-$scenario-$RUN_ID.log"
  local start setoverlay ready presented pane spawn_line engine_pid first_engine_line process_start screenshot_ms webtui_loaded_ms main_entry_browser

  if $FRESH_SPAWN || [ "$mode" = "cold" ]; then
    kill_engines
  fi

  if $REINSTALL && [ "$mode" = "cold" ]; then
    log "reinstalling termsurf before $browser cold launch"
    if expect_postflight_warmup; then
      rm -f "$POSTFLIGHT_WARMUP_LOG"
    fi
    brew reinstall --cask "$CASK_TOKEN" 2>&1 | tee -a "$HARNESS_LOG"
    if expect_postflight_warmup; then
      verify_postflight_warmup "$scenario"
    fi
    kill_engines
  fi

  capture_validation_context "$scenario-before-launch" "$validation"

  cat >"$command" <<EOF
#!/usr/bin/env bash
set -euo pipefail
export TERMSURF_WEBTUI_STATE_TRACE_FILE="$trace"
exec "$WEB" --browser "$browser" https://example.com
EOF
  chmod +x "$command"

  start_app "$scenario" "$command" "$app_log" "$engine_trace"
  start="$(line_count "$app_log")"
  setoverlay="$(wait_for_line_after "$app_log" "$start" "SetOverlay: pane_id=.* browser=${browser} .*url=https://example.com" "$browser $mode SetOverlay" 180)"
  pane="$(extract_pane_id "$setoverlay")"
  wait_for_line_after "$app_log" "$start" "SetOverlay: named browser resolved browser=${browser} installed_path=${binary}" "$browser $mode installed default resolution" 180 >/dev/null
  spawn_line="$(wait_for_line_after "$app_log" "$start" "TermSurfBrowserStartup event=spawn_returned .* browser=${browser} .* pid=[1-9][0-9]*" "$browser $mode spawn returned" 180)"
  engine_pid="$(extract_field "$spawn_line" "pid")"
  validate_number "$engine_pid" "$browser $mode engine pid"
  process_start="$(ps -o lstart= -p "$engine_pid" 2>/dev/null || true)"
  log "engine_process_start browser=$browser mode=$mode pid=$engine_pid lstart=$process_start"
  main_entry_browser="$browser"
  first_engine_line="$(wait_for_line_after "$engine_trace" 0 "TermSurfEngineStartup event=main_entry .* browser=${main_entry_browser} .* pid=${engine_pid}" "$browser $mode engine main entry" 180)"
  log "engine_first_trace browser=$browser mode=$mode line=$first_engine_line"
  ready="$(wait_for_line_after "$app_log" "$start" "BrowserReady: pane_id=.* browser=${browser}" "$browser $mode BrowserReady" 180)"
  [ "$pane" = "$(extract_pane_id "$ready")" ] || fail "$browser $mode BrowserReady pane mismatch"
  wait_for_line_after "$app_log" "$start" "TermSurfBrowserStartup event=server_register .* browser=${browser}" "$browser $mode startup server register" 180 >/dev/null
  wait_for_line_after "$app_log" "$start" "TermSurfBrowserStartup event=create_tab .* browser=${browser}" "$browser $mode startup CreateTab" 180 >/dev/null
  wait_for_tab_ready_after "$app_log" "$start" "$browser" "$pane" "$browser $mode startup TabReady" 180 >/dev/null
  wait_for_line_after "$app_log" "$start" "TermSurfBrowserStartup event=ca_context .* browser=${browser}" "$browser $mode startup CaContext" 180 >/dev/null
  presented="$(wait_for_line_after "$app_log" "$start" "TermSurf geometry layer=appkit event=presented .*pane_id:${pane} .*context_id=[1-9][0-9]*" "$browser $mode AppKit presentation" 180)"
  wait_webtui_loaded "$trace" "$browser" "$browser $mode WebTUI loaded https://example.com"
  webtui_loaded_ms="$(python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
)"
  capture_window "$presented" "$screenshot"
  screenshot_ms="$(python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
)"
  append_summary "$mode" "$browser" "$app_log" "$engine_trace" "$start" "$screenshot_ms" "$webtui_loaded_ms"
  log "PASS: $browser $mode installed cold-start measurement app_log=$app_log engine_trace=$engine_trace trace=$trace validation=$validation screenshot=$screenshot"
  stop_app
  if [ "$browser" = "chromium" ]; then
    wait_for_process_exit "$engine_pid" "$browser $mode engine process" 20
    require_no_chromium_crash_markers "$app_log" "$engine_trace" "$browser $mode"
  fi
}

run_browser() {
  local browser="$1"
  local binary="$2"
  if [ "$browser" = "ladybird" ]; then
    if $REINSTALL; then
      log "reinstalling $CASK_TOKEN before ladybird warmup proof"
      if expect_postflight_warmup; then
        rm -f "$POSTFLIGHT_WARMUP_LOG"
      fi
      brew reinstall --cask "$CASK_TOKEN" 2>&1 | tee -a "$HARNESS_LOG"
      if expect_postflight_warmup; then
        verify_postflight_warmup "ladybird-warmup-proof"
      fi
    fi
    log "PASS: ladybird postflight warmup proof path"
    return
  fi
  run_browser_once "$browser" "cold" "$binary"
  run_browser_once "$browser" "warm-fresh" "$binary"
}

require_unset ASTROHACKER_CHROMIUM_PATH
require_unset ASTROHACKER_WEBKIT_PATH
require_unset ASTROHACKER_LADYBIRD_PATH
require_unset TERMSURF_ROAMIUM_PATH
require_unset TERMSURF_SURFARI_PATH
require_unset TERMSURF_INSTALLED_ROAMIUM_PATH
require_unset TERMSURF_INSTALLED_SURFARI_PATH
require_unset TERMSURF_GIRLBAT_PATH
require_unset TERMSURF_INSTALLED_GIRLBAT_PATH
require_unset DYLD_FRAMEWORK_PATH

require_executable "$APP_BIN"
require_executable "$WEB"
require_executable "$CHROMIUM"
require_executable "$WEBKIT"
require_executable "$LADYBIRD"

printf 'mode\tbrowser\tsetoverlay_to_spawn_start_ms\tspawn_start_to_spawn_returned_ms\tspawn_returned_to_engine_main_ms\tengine_main_to_ts_content_main_ms\tts_content_main_to_on_initialized_ms\ton_initialized_to_browser_context_ms\tbrowser_context_to_server_register_sent_ms\tserver_register_to_create_tab_ms\tcreate_tab_to_tab_ready_ms\ttab_ready_to_browser_ready_ms\tbrowser_ready_to_webtui_loaded_ms\tbrowser_ready_to_visible_screenshot_ms\tbrowser_ready_to_ca_context_ms\tca_context_to_present_overlay_ms\tapp_log\tengine_trace\trun_id\n' >"$SUMMARY"

log "run_id=$RUN_ID"
log "started_at_epoch=$START_EPOCH"
log "app_bin=$APP_BIN"
log "web=$WEB"
log "chromium=$CHROMIUM"
log "webkit=$WEBKIT"
log "ladybird=$LADYBIRD"
log "summary=$SUMMARY"
log "reinstall=$REINSTALL fresh_spawn=$FRESH_SPAWN browser=$BROWSER"

case "$BROWSER" in
  chromium)
    run_browser "chromium" "$CHROMIUM"
    ;;
  webkit)
    run_browser "webkit" "$WEBKIT"
    ;;
  ladybird)
    run_browser "ladybird" "$LADYBIRD"
    ;;
  all)
    run_browser "chromium" "$CHROMIUM"
    run_browser "webkit" "$WEBKIT"
    run_browser "ladybird" "$LADYBIRD"
    ;;
esac

END_EPOCH="$(date +%s)"
DURATION_SECONDS="$((END_EPOCH - START_EPOCH))"
log "finished_at_epoch=$END_EPOCH"
log "duration_seconds=$DURATION_SECONDS"
log "summary=$SUMMARY"
log "PASS: issue 882 installed browser cold-start measurement"
