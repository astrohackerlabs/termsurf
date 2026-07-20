#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG="$(mktemp "${TMPDIR:-/tmp}/termsurf-webkit-forward.XXXXXX")"
EXPECTED="FORWARD_ACTION_SMOKE_PASS engine=webkit tabs=2 history_round_trip=1 back_action=1 forward_action=1 state=1 disabled=1 isolation=1 same_document=1 fresh_navigation_clears_forward=1 wrong_tab_rejected=1 crash_recovery=1 cleanup=1 future_actions_rejected=1"
trap 'rm -f "$LOG"' EXIT

set +e
"$SCRIPT_DIR/run-back-action-smoke.sh" >"$LOG" 2>&1
STATUS=$?
set -e

grep -v '^FORWARD_ACTION_SMOKE_PASS engine=' "$LOG" || true
if [[ $STATUS -ne 0 ]]; then
  exit "$STATUS"
fi
if [[ "$(grep -Fxc "$EXPECTED" "$LOG" || true)" -ne 1 ]]; then
  echo "missing unique exact WebKit Forward pass marker" >&2
  exit 1
fi
printf '%s\n' "$EXPECTED"
