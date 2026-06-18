#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="fast"
TS="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$ROOT/logs"
SUMMARY_LOG="$LOG_DIR/ghostboard-performance-smoke-${PROFILE}-${TS}.log"

STARTUP_MAX="${TERMSURF_PERF_STARTUP_MAX_SECONDS:-90}"
RESIZE_MAX="${TERMSURF_PERF_RESIZE_MAX_SECONDS:-150}"
MOUSE_MAX="${TERMSURF_PERF_MOUSE_MAX_SECONDS:-210}"
SCROLL_MAX="${TERMSURF_PERF_SCROLL_MAX_SECONDS:-210}"
INPUT_MAX="${TERMSURF_PERF_INPUT_MAX_SECONDS:-210}"
IDLE_CAP="${TERMSURF_PERF_IDLE_CAP_SECONDS:-60}"

usage() {
  cat <<'EOF'
usage: scripts/ghostboard-performance-smoke.sh [--fast|--diagnostic]

Runs coarse Ghostboard performance smokes under scripts/bounded-run.sh.

Profiles:
  --fast         repeated resolver-only startup
  --diagnostic   fast profile plus resize, mouse, scroll, and input diagnostics

Threshold env vars:
  TERMSURF_PERF_STARTUP_MAX_SECONDS
  TERMSURF_PERF_RESIZE_MAX_SECONDS
  TERMSURF_PERF_MOUSE_MAX_SECONDS
  TERMSURF_PERF_SCROLL_MAX_SECONDS
  TERMSURF_PERF_INPUT_MAX_SECONDS
  TERMSURF_PERF_IDLE_CAP_SECONDS
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --fast)
      PROFILE="fast"
      ;;
    --diagnostic)
      PROFILE="diagnostic"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

SUMMARY_LOG="$LOG_DIR/ghostboard-performance-smoke-${PROFILE}-${TS}.log"
mkdir -p "$LOG_DIR"

log() {
  printf '%s\n' "$*" | tee -a "$SUMMARY_LOG"
}

require_executable() {
  if [ ! -x "$1" ]; then
    log "FAIL: missing executable $1"
    exit 1
  fi
}

extract_status_field() {
  local status_line="$1"
  local field="$2"
  printf '%s\n' "$status_line" | tr ' ' '\n' | sed -n "s/^${field}=//p" | head -n 1
}

run_row() {
  local label="$1"
  local scenario="$2"
  local max_seconds="$3"
  local bounded_log="$LOG_DIR/ghostboard-performance-smoke-${PROFILE}-${label}-${TS}.log"
  local hard_cap="${TERMSURF_PERF_HARD_CAP_SECONDS:-$((max_seconds + 60))}"
  local status_line status rc elapsed result

  log "RUN profile=$PROFILE label=$label scenario=$scenario max_seconds=${max_seconds} bounded_log=$bounded_log"

  BOUNDED_HARD_CAP="$hard_cap" BOUNDED_IDLE_CAP="$IDLE_CAP" \
    "$ROOT/scripts/bounded-run.sh" \
    "$bounded_log" \
    "$ROOT/scripts/ghostboard-geometry-matrix.sh" \
    "$scenario"

  status_line="$(grep '^STATUS=' "$bounded_log" | tail -n 1 || true)"
  if [ -z "$status_line" ]; then
    log "RESULT label=$label scenario=$scenario result=FAIL reason=missing-status bounded_log=$bounded_log"
    return 1
  fi

  status="$(extract_status_field "$status_line" "STATUS")"
  rc="$(extract_status_field "$status_line" "rc")"
  elapsed="$(extract_status_field "$status_line" "elapsed" | sed 's/s$//')"

  if [ "$status" != "COMPLETED" ]; then
    log "RESULT label=$label scenario=$scenario result=FAIL reason=bounded-${status} status_line=\"$status_line\" bounded_log=$bounded_log"
    return 1
  fi

  if [ "${rc:-1}" != "0" ]; then
    log "RESULT label=$label scenario=$scenario result=FAIL reason=scenario-exit rc=${rc:-missing} elapsed_seconds=${elapsed:-missing} bounded_log=$bounded_log"
    return 1
  fi

  if ! expr "${elapsed:-}" : '^[0-9][0-9]*$' >/dev/null; then
    log "RESULT label=$label scenario=$scenario result=FAIL reason=missing-elapsed status_line=\"$status_line\" bounded_log=$bounded_log"
    return 1
  fi

  if [ "$elapsed" -gt "$max_seconds" ]; then
    log "RESULT label=$label scenario=$scenario result=FAIL reason=threshold elapsed_seconds=$elapsed max_seconds=$max_seconds bounded_log=$bounded_log"
    return 1
  fi

  result="PASS"
  log "RESULT label=$label scenario=$scenario result=$result status=$status rc=$rc elapsed_seconds=$elapsed max_seconds=$max_seconds bounded_log=$bounded_log"
  return 0
}

require_executable "$ROOT/scripts/bounded-run.sh"
require_executable "$ROOT/scripts/ghostboard-geometry-matrix.sh"

log "profile=$PROFILE"
log "summary_log=$SUMMARY_LOG"
log "startup_max_seconds=$STARTUP_MAX"
if [ "$PROFILE" = "diagnostic" ]; then
  log "resize_max_seconds=$RESIZE_MAX"
  log "mouse_max_seconds=$MOUSE_MAX"
  log "scroll_max_seconds=$SCROLL_MAX"
  log "input_max_seconds=$INPUT_MAX"
fi

failures=0

run_row "startup-1" "named-roamium-debug-launch" "$STARTUP_MAX" || failures=$((failures + 1))
run_row "startup-2" "named-roamium-debug-launch" "$STARTUP_MAX" || failures=$((failures + 1))
run_row "startup-3" "named-roamium-debug-launch" "$STARTUP_MAX" || failures=$((failures + 1))

if [ "$PROFILE" = "diagnostic" ]; then
  run_row "resize" "window-resize" "$RESIZE_MAX" || failures=$((failures + 1))
  run_row "mouse" "mouse-after-geometry-change" "$MOUSE_MAX" || failures=$((failures + 1))
  run_row "scroll" "terminal-scrollback-movement" "$SCROLL_MAX" || failures=$((failures + 1))
  run_row "input" "browser-input-granularity" "$INPUT_MAX" || failures=$((failures + 1))
fi

if [ "$failures" -ne 0 ]; then
  log "SUMMARY result=FAIL profile=$PROFILE failures=$failures summary_log=$SUMMARY_LOG"
  exit 1
fi

log "SUMMARY result=PASS profile=$PROFILE summary_log=$SUMMARY_LOG"
