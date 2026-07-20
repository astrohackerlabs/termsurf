#!/usr/bin/env bash
set -euo pipefail

SMOKE_ROOT="$(cd "$(dirname "$0")" && pwd)"
LIB_ROOT="$(cd "$SMOKE_ROOT/.." && pwd)"
REPO_ROOT="$(git -C "$LIB_ROOT" rev-parse --show-toplevel)"
FORK="$REPO_ROOT/forks/ladybird"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ladybird-forward-action.XXXXXX")"
LOG="$RUN_DIR/smoke.log"
EXPECTED="FORWARD_ACTION_SMOKE_PASS engine=ladybird tabs=2 history_round_trip=1 back_action=1 forward_action=1 state=1 disabled=1 isolation=1 same_document=1 fresh_navigation_clears_forward=1 wrong_tab_rejected=1 crash_recovery=1 cleanup=1 future_actions_rejected=1"
SERVER_PID=""

cleanup() {
  if test -n "$SERVER_PID"; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

case "$(git -C "$FORK" rev-parse --abbrev-ref HEAD)" in
  2a3bc6a3-issue-26071517585505-forward-button | \
    2a3bc6a3-issue-26071521449339-refresh-button) ;;
  *)
    printf '%s\n' "Ladybird Forward smoke requires a compatible issue branch" >&2
    exit 1
    ;;
esac

TERMSURF_LADYBIRD_BACKEND=real \
  "$LIB_ROOT/build.sh" --configuration Debug --clean

python3 "$SMOKE_ROOT/back-fixture/server.py" --port-file "$RUN_DIR/port" \
  >"$RUN_DIR/server.log" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 100); do
  test -s "$RUN_DIR/port" && break
  sleep 0.05
done
test -s "$RUN_DIR/port" || {
  sed -n '1,120p' "$RUN_DIR/server.log" >&2
  exit 1
}
PORT="$(tr -d '\n' <"$RUN_DIR/port")"

set +e
(
  cd "$REPO_ROOT/rust"
  TERMSURF_LADYBIRD_BACKEND=real \
    TERMSURF_LADYBIRD_SMOKE_BASE_URL="http://127.0.0.1:$PORT" \
    cargo run -p ah-ladybirdd -- --termsurf-forward-action-smoke
) >"$LOG" 2>&1
STATUS=$?
set -e

grep -v '^FORWARD_ACTION_SMOKE_PASS engine=' "$LOG" || true
if [[ $STATUS -ne 0 ]]; then
  exit "$STATUS"
fi
if [[ "$(grep -Fxc "$EXPECTED" "$LOG" || true)" -ne 1 ]]; then
  echo "missing unique exact Ladybird Forward pass marker" >&2
  exit 1
fi
printf '%s\n' "$EXPECTED"
