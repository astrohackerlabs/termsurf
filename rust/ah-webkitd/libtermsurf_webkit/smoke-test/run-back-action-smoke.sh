#!/usr/bin/env bash
set -euo pipefail

SMOKE_ROOT="$(cd "$(dirname "$0")" && pwd)"
LIB_ROOT="$(cd "$SMOKE_ROOT/.." && pwd)"
REPO_ROOT="$(git -C "$LIB_ROOT" rev-parse --show-toplevel)"
WEBKIT_BUILD="$REPO_ROOT/forks/webkit/src/WebKitBuild/Release"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/webkit-back-action.XXXXXX")"
SERVER_PID=""

cleanup() {
  if test -n "$SERVER_PID"; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

test -d "$WEBKIT_BUILD/WebKit.framework" || {
  printf '%s\n' "missing source-built Release WebKit: $WEBKIT_BUILD/WebKit.framework" >&2
  exit 1
}

"$LIB_ROOT/build.sh" --release

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

DYLD_FRAMEWORK_PATH="$WEBKIT_BUILD${DYLD_FRAMEWORK_PATH:+:$DYLD_FRAMEWORK_PATH}" \
  "$LIB_ROOT/build/back-action-smoke" "http://127.0.0.1:$PORT"
