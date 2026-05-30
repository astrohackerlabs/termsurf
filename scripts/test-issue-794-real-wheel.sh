#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="${LOG_DIR:-"$ROOT/logs/issue-794-exp2-realwheel-$STAMP"}"
mkdir -p "$LOG_DIR"
LOG_DIR="$(cd "$LOG_DIR" && pwd)"
URL="${1:-http://localhost:9616/bitcoin.pdf}"

WEZBOARD_BIN="$ROOT/wezboard/target/debug/wezboard-gui"
WEB_BIN="$ROOT/webtui/target/debug/web"
ROAMIUM_BIN="$ROOT/chromium/src/out/Default/roamium"
INTERACTION_HELPER="$ROOT/scripts/capture-pdf-interactions.mjs"
TRACE_FILE="$LOG_DIR/pdf-input.log"
SERVER_LOG="$LOG_DIR/test-server.log"
WEZBOARD_LOG="$LOG_DIR/wezboard-gui.log"
CHROMIUM_LOG="$LOG_DIR/chromium-server.log"
WEB_LAUNCHER="$LOG_DIR/run-web.sh"
WEB_LAUNCHER_LOG="$LOG_DIR/run-web.log"
RUN_INFO="$LOG_DIR/run-info.txt"
BEFORE_DIR="$LOG_DIR/before"
AFTER_DIR="$LOG_DIR/after"
WHEEL_HELPER="$LOG_DIR/issue-794-scroll-helper"
WHEEL_HELPER_SWIFT="$LOG_DIR/issue-794-scroll-helper.swift"
OS_SCREENSHOT="$LOG_DIR/os-diagnostic.png"

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

require_file "$WEZBOARD_BIN" "debug Wezboard"
require_file "$WEB_BIN" "debug web"
require_file "$ROAMIUM_BIN" "repo-built Roamium"
require_file "$INTERACTION_HELPER" "DevTools interaction helper"

{
  echo "timestamp=$STAMP"
  echo "url=$URL"
  echo "wezboard_bin=$WEZBOARD_BIN"
  echo "web_bin=$WEB_BIN"
  echo "roamium_bin=$ROAMIUM_BIN"
  echo "interaction_helper=$INTERACTION_HELPER"
  echo "trace_file=$TRACE_FILE"
  echo "web_command=$WEB_BIN --browser $ROAMIUM_BIN $URL"
} >"$RUN_INFO"

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

env \
  -u TERMSURF_SOCKET \
  -u WEZBOARD_UNIX_SOCKET \
  TERMSURF_PDF_INPUT_TRACE="${TERMSURF_PDF_INPUT_TRACE:-1}" \
  TERMSURF_PDF_INPUT_TRACE_FILE="$TRACE_FILE" \
  "$WEZBOARD_BIN" start \
  --always-new-process \
  --no-auto-connect \
  --cwd "$ROOT" \
  -- "$WEB_LAUNCHER" >"$WEZBOARD_LOG" 2>&1 &
WEZBOARD_PID="$!"
{
  echo "wezboard_pid=$WEZBOARD_PID"
  echo "wezboard_command=$WEZBOARD_BIN start --always-new-process --no-auto-connect --cwd $ROOT -- $WEB_LAUNCHER"
} >>"$RUN_INFO"

DEVTOOLS_PORT=""
for _ in {1..160}; do
  DEVTOOLS_PORT="$(
    sed -nE 's/.*DevTools listening on ws:\/\/127\.0\.0\.1:([0-9]+)\/.*/\1/p' \
      "$WEZBOARD_LOG" 2>/dev/null | tail -1
  )"
  if [[ -n "$DEVTOOLS_PORT" ]]; then
    break
  fi
  sleep 0.25
done

if [[ -z "$DEVTOOLS_PORT" ]]; then
  echo "DevTools port not found in $WEZBOARD_LOG" >&2
  exit 1
fi

URL_CONTAINS="${DEVTOOLS_URL_CONTAINS:-}"
if [[ -z "$URL_CONTAINS" ]]; then
  if [[ "$URL" == *bitcoin.pdf* ]]; then
    URL_CONTAINS="bitcoin.pdf"
  elif [[ "$URL" == http://localhost:9616/* ]]; then
    URL_CONTAINS="${URL##*/}"
  else
    URL_CONTAINS="$URL"
  fi
fi
{
  echo "devtools_port=$DEVTOOLS_PORT"
  echo "devtools_url_contains=$URL_CONTAINS"
} >>"$RUN_INFO"

node "$INTERACTION_HELPER" \
  --devtools-port "$DEVTOOLS_PORT" \
  --url-contains "$URL_CONTAINS" \
  --out-dir "$BEFORE_DIR" \
  --timeout-seconds "${TERMSURF_DEVTOOLS_TIMEOUT_SECONDS:-30}" \
  --settle-seconds "${TERMSURF_PDF_SETTLE_SECONDS:-8}" \
  --mode probe

cat >"$WHEEL_HELPER_SWIFT" <<'EOF'
import AppKit
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count >= 3,
      let pid = Int(args[1]),
      let delta = Int32(args[2]) else {
  fputs("usage: helper pid delta\n", stderr)
  exit(2)
}

guard let windowList = CGWindowListCopyWindowInfo(
  [.optionOnScreenOnly, .excludeDesktopElements],
  kCGNullWindowID
) as? [[String: Any]] else {
  fputs("could not list windows\n", stderr)
  exit(3)
}

let candidates = windowList.filter { info in
  guard let ownerPid = info[kCGWindowOwnerPID as String] as? Int,
        let layer = info[kCGWindowLayer as String] as? Int else {
    return false
  }
  return ownerPid == pid && layer == 0
}

guard let window = candidates.first,
      let bounds = window[kCGWindowBounds as String] as? [String: Any],
      let xValue = bounds["X"] as? Double,
      let yValue = bounds["Y"] as? Double,
      let widthValue = bounds["Width"] as? Double,
      let heightValue = bounds["Height"] as? Double else {
  fputs("could not find window for pid \(pid)\n", stderr)
  exit(4)
}

let x = xValue + widthValue * 0.55
let y = yValue + heightValue * 0.50
let point = CGPoint(x: x, y: y)
print("window=\(xValue),\(yValue),\(widthValue),\(heightValue)")
print("target=\(x),\(y)")

if let app = NSRunningApplication(processIdentifier: pid_t(pid)) {
  app.activate(options: [.activateIgnoringOtherApps])
}
usleep(250_000)

CGWarpMouseCursorPosition(point)
usleep(120_000)

for _ in 0..<5 {
  if let event = CGEvent(
    scrollWheelEvent2Source: nil,
    units: .pixel,
    wheelCount: 2,
    wheel1: delta,
    wheel2: 0,
    wheel3: 0
  ) {
    event.location = point
    event.post(tap: .cghidEventTap)
  }
  usleep(80_000)
}
EOF
swiftc "$WHEEL_HELPER_SWIFT" -o "$WHEEL_HELPER"
"$WHEEL_HELPER" "$WEZBOARD_PID" "${TERMSURF_REAL_WHEEL_DELTA:--20}" | tee -a "$RUN_INFO"

sleep 1

node "$INTERACTION_HELPER" \
  --devtools-port "$DEVTOOLS_PORT" \
  --url-contains "$URL_CONTAINS" \
  --out-dir "$AFTER_DIR" \
  --timeout-seconds "${TERMSURF_DEVTOOLS_TIMEOUT_SECONDS:-30}" \
  --settle-seconds 1 \
  --mode probe

screencapture -x "$OS_SCREENSHOT" 2>/dev/null || true

if [[ -f "$HOME/.local/state/termsurf/chromium-server.log" ]]; then
  cp "$HOME/.local/state/termsurf/chromium-server.log" "$CHROMIUM_LOG"
fi

cat <<EOF
Issue 794 real wheel artifacts:
  log dir: $LOG_DIR
  URL: $URL
  DevTools port: $DEVTOOLS_PORT
  DevTools match: $URL_CONTAINS
  trace: $TRACE_FILE
  before summary: $BEFORE_DIR/summary.json
  after summary: $AFTER_DIR/summary.json
  OS diagnostic screenshot: $OS_SCREENSHOT
  Wezboard log: $WEZBOARD_LOG
  Chromium log: $CHROMIUM_LOG
  web launcher log: $WEB_LAUNCHER_LOG
  test server log: $SERVER_LOG
  run info: $RUN_INFO
EOF
