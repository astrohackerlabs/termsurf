#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="${LOG_DIR:-"$ROOT/logs/issue-776-exp4-$STAMP"}"
URL="${1:-http://localhost:9616/bitcoin.pdf}"

WEZBOARD_BIN="$ROOT/wezboard/target/debug/wezboard-gui"
WEB_BIN="$ROOT/webtui/target/debug/web"
ROAMIUM_BIN="$ROOT/chromium/src/out/Default/roamium"
SCREENSHOT="$LOG_DIR/pdf-smoke.png"
SERVER_LOG="$LOG_DIR/test-server.log"
WEZBOARD_LOG="$LOG_DIR/wezboard-gui.log"
CHROMIUM_LOG="$LOG_DIR/chromium-server.log"
WEB_LAUNCHER="$LOG_DIR/run-web.sh"
WEB_LAUNCHER_LOG="$LOG_DIR/run-web.log"
RUN_INFO="$LOG_DIR/run-info.txt"
PERMISSION_TEST="$LOG_DIR/screenshot-permission-test.png"

SERVER_PID=""
WEZBOARD_PID=""

cleanup() {
  if [[ -n "$WEZBOARD_PID" ]] && kill -0 "$WEZBOARD_PID" 2>/dev/null; then
    kill "$WEZBOARD_PID" 2>/dev/null || true
  fi
  if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

require_file() {
  local path="$1"
  local label="$2"
  if [[ ! -x "$path" ]]; then
    echo "missing executable for $label: $path" >&2
    exit 1
  fi
}

mkdir -p "$LOG_DIR"

require_file "$WEZBOARD_BIN" "debug Wezboard"
require_file "$WEB_BIN" "debug web"
require_file "$ROAMIUM_BIN" "repo-built Roamium"

{
  echo "timestamp=$STAMP"
  echo "url=$URL"
  echo "wezboard_bin=$WEZBOARD_BIN"
  echo "web_bin=$WEB_BIN"
  echo "roamium_bin=$ROAMIUM_BIN"
  echo "web_command=$WEB_BIN --browser $ROAMIUM_BIN $URL"
} >"$RUN_INFO"

screencapture -x "$PERMISSION_TEST"
test -s "$PERMISSION_TEST"

if [[ "$URL" == http://localhost:9616/* ]]; then
  if ! curl -fsS -o /dev/null "$URL"; then
    if ! command -v bun >/dev/null 2>&1; then
      echo "bun is required to start test-html/server.ts" >&2
      exit 1
    fi
    (
      cd "$ROOT/test-html"
      bun server.ts
    ) >"$SERVER_LOG" 2>&1 &
    SERVER_PID="$!"

    for _ in {1..50}; do
      if curl -fsS -o /dev/null "$URL"; then
        break
      fi
      sleep 0.1
    done
    curl -fsS -o /dev/null "$URL"
  else
    echo "reused existing test server on http://localhost:9616" >"$SERVER_LOG"
  fi
else
  echo "no HTTP test server needed for $URL" >"$SERVER_LOG"
fi

cat >"$WEB_LAUNCHER" <<EOF
#!/usr/bin/env bash
set -euo pipefail
SOCKET_DIR="\${TMPDIR:-/tmp}/termsurf"
for _ in {1..100}; do
  sock="\$(ls -t "\$SOCKET_DIR"/wezboard-*.sock 2>/dev/null | head -1 || true)"
  if [[ -n "\$sock" && -S "\$sock" ]]; then
    export TERMSURF_SOCKET="\$sock"
    echo "using TERMSURF_SOCKET=\$TERMSURF_SOCKET" >>"$WEB_LAUNCHER_LOG"
    echo "exec $WEB_BIN --browser $ROAMIUM_BIN $URL" >>"$WEB_LAUNCHER_LOG"
    exec "$WEB_BIN" --browser "$ROAMIUM_BIN" "$URL"
  fi
  sleep 0.1
done
echo "no TermSurf socket found in \$SOCKET_DIR" >>"$WEB_LAUNCHER_LOG"
exit 1
EOF
chmod +x "$WEB_LAUNCHER"

env -u TERMSURF_SOCKET -u WEZBOARD_UNIX_SOCKET "$WEZBOARD_BIN" start \
  --always-new-process \
  --no-auto-connect \
  --cwd "$ROOT" \
  -- "$WEB_LAUNCHER" >"$WEZBOARD_LOG" 2>&1 &
WEZBOARD_PID="$!"
{
  echo "wezboard_pid=$WEZBOARD_PID"
  echo "wezboard_command=$WEZBOARD_BIN start --always-new-process --no-auto-connect --cwd $ROOT -- $WEB_LAUNCHER"
} >>"$RUN_INFO"

sleep "${TERMSURF_PDF_SETTLE_SECONDS:-18}"

osascript >/dev/null 2>&1 <<EOF || true
tell application "System Events"
  set target_pid to $WEZBOARD_PID
  repeat with proc in processes
    if unix id of proc is target_pid then
      set frontmost of proc to true
      exit repeat
    end if
  end repeat
end tell
EOF

sleep 1

screencapture -x "$SCREENSHOT"
test -s "$SCREENSHOT"

if [[ -f "$HOME/.local/state/termsurf/chromium-server.log" ]]; then
  cp "$HOME/.local/state/termsurf/chromium-server.log" "$CHROMIUM_LOG"
fi

cat <<EOF
Issue 776 PDF smoke test artifacts:
  log dir: $LOG_DIR
  URL: $URL
  screenshot: $SCREENSHOT
  Wezboard log: $WEZBOARD_LOG
  Chromium log: $CHROMIUM_LOG
  web launcher log: $WEB_LAUNCHER_LOG
  test server log: $SERVER_LOG
  run info: $RUN_INFO
EOF
