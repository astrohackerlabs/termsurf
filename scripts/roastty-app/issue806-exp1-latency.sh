#!/usr/bin/env bash
# Issue 806 / Exp 1 — measure live Roastty keyboard-to-visible-output latency.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIR="$ROOT/scripts/roastty-app"
LOG_DIR="$ROOT/logs"
APP="$ROOT/roastty/macos/build/ReleaseLocal/Roastty.app"
BIN="$APP/Contents/MacOS/roastty"
SWIFT="$(command -v swift || echo /usr/bin/swift)"

RUN_ID="issue806-exp1-$(date +%Y%m%d-%H%M%S)"
RUN_DIR="/tmp/termsurf-$RUN_ID"
TRACE="$LOG_DIR/$RUN_ID.trace"
HARNESS_LOG="$LOG_DIR/$RUN_ID.harness.log"
STDOUT_LOG="$LOG_DIR/$RUN_ID.stdout.log"
STDERR_LOG="$LOG_DIR/$RUN_ID.stderr.log"
BUILD_LOG="$LOG_DIR/$RUN_ID-build.log"
SAMPLE_LOG="$LOG_DIR/$RUN_ID.sample.txt"
SUMMARY="$LOG_DIR/$RUN_ID-summary.txt"
MARKER_FILE="$RUN_DIR/marker.txt"
VISIBLE_MARKER="ISSUE806_EXP1_VISIBLE_$RUN_ID"
MARKER_TEXT="ISSUE806_EXP1_MARKER_$RUN_ID"
TYPE_FILE="$RUN_DIR/type.txt"

ROASTTY_PID=""
SAMPLE_PID=""
mkdir -p "$LOG_DIR" "$RUN_DIR"

log() {
  printf 'ts_ns=%s harness %s\n' "$(python3 - <<'PY'
import time
print(time.time_ns())
PY
)" "$*" | tee -a "$HARNESS_LOG"
}

cleanup() {
  if [ -n "${SAMPLE_PID:-}" ]; then
    wait "$SAMPLE_PID" 2>/dev/null || true
  fi
  if [ -z "${KEEP_ROASTTY_AFTER_ISSUE806:-}" ] && [ -n "${ROASTTY_PID:-}" ]; then
    kill -9 "$ROASTTY_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

write_type_file() {
  cat > "$TYPE_FILE" <<EOF
printf '$VISIBLE_MARKER\n'; printf '$MARKER_TEXT' > '$MARKER_FILE'
EOF
}

frontmost_pid() {
  osascript -e 'tell application "System Events" to unix id of first process whose frontmost is true'
}

focus_roastty() {
  local pid="$1"
  osascript <<OSA >>"$HARNESS_LOG" 2>&1
set targetPid to $pid
tell application "System Events"
  set roastProc to first process whose unix id is targetPid
  set frontmost of roastProc to true
  delay 0.5
  log "frontmost-name=" & (name of first process whose frontmost is true)
  log "frontmost-pid=" & (unix id of first process whose frontmost is true)
  log "roast-frontmost=" & (frontmost of roastProc)
  try
    set focusedElement to value of attribute "AXFocusedUIElement" of roastProc
    log "focused-role=" & (role of focusedElement as text)
    try
      log "focused-description=" & (description of focusedElement as text)
    end try
  on error errText number errNum
    log "focused-element-error=" & errNum & " " & errText
  end try
end tell
OSA
  local front_pid
  front_pid="$(frontmost_pid)"
  [ "$front_pid" = "$pid" ] || {
    echo "frontmost pid $front_pid did not match Roastty pid $pid" >&2
    return 1
  }
}

terminal_value() {
  local pid="$1"
  osascript <<OSA 2>/dev/null || true
set targetPid to $pid
tell application "System Events"
  set roastProc to first process whose unix id is targetPid
  try
    set focusedElement to value of attribute "AXFocusedUIElement" of roastProc
    set focusedValue to value of focusedElement
    if focusedValue is missing value then
      return ""
    else
      return focusedValue as text
    end if
  on error
    return ""
  end try
end tell
OSA
}

wait_for_window() {
  local pid="$1"
  for _ in $(seq 1 40); do
    if "$SWIFT" "$DIR/winid.swift" "$pid" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done
  echo "timed out waiting for Roastty window for pid $pid" >&2
  return 1
}

window_center_click() {
  local pid="$1"
  local wid x y w h
  IFS=$'\t' read -r wid x y w h < <("$SWIFT" "$DIR/winid.swift" "$pid")
  local cx cy
  cx="$(python3 - "$x" "$w" <<'PY'
import sys
print(int(float(sys.argv[1]) + float(sys.argv[2]) / 2))
PY
)"
  cy="$(python3 - "$y" <<'PY'
import sys
print(int(float(sys.argv[1]) + 90))
PY
)"
  log "click window_id=$wid x=$cx y=$cy"
  "$SWIFT" "$DIR/click.swift" "$cx" "$cy" 1 >>"$HARNESS_LOG" 2>&1
}

start_sampler_if_delayed() {
  local pid="$1"
  (
    sleep 5
    if [ ! -f "$MARKER_FILE" ]; then
      log "sample begin pid=$pid file=$SAMPLE_LOG"
      sample "$pid" 5 -file "$SAMPLE_LOG" >>"$HARNESS_LOG" 2>&1 || true
      log "sample end pid=$pid file=$SAMPLE_LOG"
    fi
  ) &
  SAMPLE_PID="$!"
}

summarize_trace() {
  python3 - "$TRACE" "$SUMMARY" <<'PY'
import re
import sys
from pathlib import Path

trace = Path(sys.argv[1])
summary = Path(sys.argv[2])
lines = trace.read_text(errors="replace").splitlines() if trace.exists() else []
events = []
for line in lines:
    match = re.match(r"ts_ns=(\d+)\s+(.*)", line)
    if match:
        events.append((int(match.group(1)), match.group(2), line))

largest = None
for prev, cur in zip(events, events[1:]):
    gap_ms = (cur[0] - prev[0]) / 1_000_000
    if largest is None or gap_ms > largest[0]:
        largest = (gap_ms, prev, cur)

with summary.open("w") as out:
    out.write(f"trace={trace}\n")
    out.write(f"events={len(events)}\n")
    if largest:
        gap_ms, prev, cur = largest
        out.write(f"largest_gap_ms={gap_ms:.3f}\n")
        out.write(f"gap_from={prev[2]}\n")
        out.write(f"gap_to={cur[2]}\n")
    for needle in [
        "keyDown",
        "keyAction",
        "roastty_surface_key",
        "termio_worker_queue_write",
        "termio_worker_command write",
        "termio_worker_pump emit",
        "surface_apply_termio_event pump",
        "present_live begin",
        "present_live end",
        "present_driver_tick",
        "app_tick",
    ]:
        matches = [line for _, msg, line in events if needle in msg]
        out.write(f"\n[{needle}] count={len(matches)}\n")
        for item in matches[:5]:
            out.write(item + "\n")
        if len(matches) > 5:
            out.write(f"... {len(matches) - 5} more\n")
print(summary)
PY
}

log "build begin"
(cd "$ROOT/roastty/macos" && ./build.nu --configuration ReleaseLocal) >"$BUILD_LOG" 2>&1
log "build end log=$BUILD_LOG"
[ -x "$BIN" ] || { echo "missing app binary: $BIN" >&2; exit 1; }

write_type_file
log "launch app=$APP trace=$TRACE"
ROASTTY_UI_KEY_TRACE_PATH="$TRACE" \
DISABLE_AUTO_UPDATE=true \
"$BIN" >"$STDOUT_LOG" 2>"$STDERR_LOG" &
ROASTTY_PID="$!"
log "launched pid=$ROASTTY_PID stdout=$STDOUT_LOG stderr=$STDERR_LOG"

wait_for_window "$ROASTTY_PID"
"$DIR/screenshot.sh" "$ROASTTY_PID" "$RUN_ID-before" >>"$HARNESS_LOG" 2>&1 || true
focus_roastty "$ROASTTY_PID"
window_center_click "$ROASTTY_PID"
focus_roastty "$ROASTTY_PID"

log "type begin type_file=$TYPE_FILE"
TYPE_START_NS="$(python3 - <<'PY'
import time
print(time.time_ns())
PY
)"
start_sampler_if_delayed "$ROASTTY_PID"
osascript <<OSA >>"$HARNESS_LOG" 2>&1
tell application "System Events"
  keystroke (read POSIX file "$TYPE_FILE")
  key code 36
end tell
OSA
TYPE_END_NS="$(python3 - <<'PY'
import time
print(time.time_ns())
PY
)"
log "type end duration_ms=$(python3 - "$TYPE_START_NS" "$TYPE_END_NS" <<'PY'
import sys
print(f"{(int(sys.argv[2]) - int(sys.argv[1])) / 1_000_000:.3f}")
PY
)"

MARKER_NS=""
VISIBLE_NS=""
for _ in $(seq 1 750); do
  now_ns="$(python3 - <<'PY'
import time
print(time.time_ns())
PY
)"
  if [ -z "$MARKER_NS" ] && [ -f "$MARKER_FILE" ]; then
    MARKER_NS="$now_ns"
    log "marker_file observed marker_latency_ms=$(python3 - "$TYPE_START_NS" "$MARKER_NS" <<'PY'
import sys
print(f"{(int(sys.argv[2]) - int(sys.argv[1])) / 1_000_000:.3f}")
PY
)"
  fi
  if [ -z "$VISIBLE_NS" ]; then
    value="$(terminal_value "$ROASTTY_PID")"
    if printf '%s' "$value" | grep -F "$VISIBLE_MARKER" >/dev/null; then
      VISIBLE_NS="$now_ns"
      printf '%s\n' "$value" > "$LOG_DIR/$RUN_ID-accessibility-value.txt"
      log "visible_marker observed visible_latency_ms=$(python3 - "$TYPE_START_NS" "$VISIBLE_NS" <<'PY'
import sys
print(f"{(int(sys.argv[2]) - int(sys.argv[1])) / 1_000_000:.3f}")
PY
)"
    fi
  fi
  [ -n "$MARKER_NS" ] && [ -n "$VISIBLE_NS" ] && break
  sleep 0.1
done

"$DIR/screenshot.sh" "$ROASTTY_PID" "$RUN_ID-after" >>"$HARNESS_LOG" 2>&1 || true
summarize_trace

{
  echo "run_id=$RUN_ID"
  echo "pid=$ROASTTY_PID"
  echo "app=$APP"
  echo "trace=$TRACE"
  echo "harness_log=$HARNESS_LOG"
  echo "stdout=$STDOUT_LOG"
  echo "stderr=$STDERR_LOG"
  echo "build_log=$BUILD_LOG"
  echo "sample=$SAMPLE_LOG"
  echo "summary=$SUMMARY"
  echo "marker_file=$MARKER_FILE"
  echo "marker_observed=$([ -n "$MARKER_NS" ] && echo yes || echo no)"
  echo "visible_observed=$([ -n "$VISIBLE_NS" ] && echo yes || echo no)"
  if [ -n "$MARKER_NS" ]; then
    python3 - "$TYPE_START_NS" "$MARKER_NS" <<'PY'
import sys
print(f"marker_latency_ms={(int(sys.argv[2]) - int(sys.argv[1])) / 1_000_000:.3f}")
PY
  fi
  if [ -n "$VISIBLE_NS" ]; then
    python3 - "$TYPE_START_NS" "$VISIBLE_NS" <<'PY'
import sys
print(f"visible_latency_ms={(int(sys.argv[2]) - int(sys.argv[1])) / 1_000_000:.3f}")
PY
  fi
} | tee -a "$SUMMARY"

[ -n "$MARKER_NS" ] || { echo "marker file was not observed" >&2; exit 1; }
[ -n "$VISIBLE_NS" ] || { echo "visible marker was not observed via accessibility" >&2; exit 1; }
