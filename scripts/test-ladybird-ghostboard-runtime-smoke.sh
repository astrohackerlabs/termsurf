#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TS="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$ROOT/logs"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-ladybird-runtime.XXXXXX")"
APP="${TERMSURF_GHOSTBOARD_APP:-$ROOT/forks/ghostty/macos/build/Debug/Astrohacker Terminal.app}"
APP_BIN="$APP/Contents/MacOS/ahterm"
WEB="${TERMSURF_WEB:-$ROOT/rust/target/debug/ahweb}"
LADYBIRD="${ASTROHACKER_LADYBIRD_PATH:-${TERMSURF_GIRLBAT_PATH:-$ROOT/rust/target/debug/ah-ladybirdd}}"
APP_LOG="$LOG_DIR/ladybird-ghostboard-runtime-app-${TS}.log"
HARNESS_LOG="$LOG_DIR/ladybird-ghostboard-runtime-harness-${TS}.log"
WEBTUI_TRACE="$LOG_DIR/ladybird-ghostboard-runtime-webtui-${TS}.log"
HTTP_PID=""
PID=""

mkdir -p "$LOG_DIR"

log() {
  printf '%s\n' "$*" | tee -a "$HARNESS_LOG"
}

fail() {
  log "FAIL: $*"
  exit 1
}

delay() {
  osascript -e "delay ${1:-0.5}" >/dev/null
}

cleanup() {
  if [ -n "${HTTP_PID:-}" ] && kill -0 "$HTTP_PID" >/dev/null 2>&1; then
    kill "$HTTP_PID" >/dev/null 2>&1 || true
  fi
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

wait_for_pattern() {
  local pattern="$1"
  local label="$2"
  local attempts="${3:-60}"
  for _ in $(seq 1 "$attempts"); do
    if grep -E "$pattern" "$APP_LOG" >/dev/null 2>&1; then
      log "PASS: $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label pattern=$pattern app_log=$APP_LOG"
}

wait_for_literal() {
  local text="$1"
  local label="$2"
  local attempts="${3:-60}"
  for _ in $(seq 1 "$attempts"); do
    if grep -F "$text" "$APP_LOG" >/dev/null 2>&1; then
      log "PASS: $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label text=$text app_log=$APP_LOG"
}

wait_for_trace_pattern() {
  local pattern="$1"
  local label="$2"
  local attempts="${3:-60}"
  for _ in $(seq 1 "$attempts"); do
    if grep -E "$pattern" "$WEBTUI_TRACE" >/dev/null 2>&1; then
      log "PASS: $label"
      return 0
    fi
    delay 1
  done
  fail "timed out waiting for $label pattern=$pattern webtui_trace=$WEBTUI_TRACE"
}

pick_port() {
  python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
}

require_executable "$APP_BIN"
require_executable "$WEB"
require_executable "$LADYBIRD"

WEB_ROOT="$RUN_DIR/site"
mkdir -p "$WEB_ROOT"
cat >"$WEB_ROOT/index.html" <<'EOF'
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>Ladybird Runtime Smoke</title>
    <style>
      html,
      body {
        margin: 0;
        min-height: 100vh;
        background: #16324f;
        color: #f6f0d7;
        font: 24px -apple-system, BlinkMacSystemFont, sans-serif;
      }
      main {
        padding: 36px;
      }
    </style>
  </head>
  <body>
    <main>
      <h1>Ladybird Runtime Smoke</h1>
      <p id="beacon">ordinary http page</p>
    </main>
    <script>
      console.log("ladybird-runtime-smoke-console");
    </script>
  </body>
</html>
EOF

PORT="$(pick_port)"
URL="http://127.0.0.1:${PORT}/index.html"
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$WEB_ROOT" >>"$HARNESS_LOG" 2>&1 &
HTTP_PID="$!"

for _ in $(seq 1 30); do
  if python3 - "$URL" <<'PY' >/dev/null 2>&1
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY
  then
    break
  fi
  delay 0.25
done

python3 - "$URL" <<'PY' >/dev/null 2>&1 || fail "HTTP fixture did not become ready"
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    raise SystemExit(0 if response.status == 200 else 1)
PY

COMMAND="$RUN_DIR/run-web.sh"
XDG_CONFIG_HOME="$RUN_DIR/xdg"
cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
exec "$WEB" --browser ladybird "$URL"
EOF
chmod +x "$COMMAND"

log "app=$APP"
log "web=$WEB"
log "ladybird=$LADYBIRD"
log "url=$URL"
log "app_log=$APP_LOG"
log "harness_log=$HARNESS_LOG"
log "webtui_trace=$WEBTUI_TRACE"

env \
  XDG_CONFIG_HOME="$XDG_CONFIG_HOME" \
  GHOSTTY_LOG=stderr \
  TERMSURF_GEOMETRY_TRACE=1 \
  TERMSURF_GEOMETRY_SCENARIO=issue-26070112000884-ladybird-runtime \
  ASTROHACKER_LADYBIRD_PATH="$LADYBIRD" \
  TERMSURF_GIRLBAT_PATH="$LADYBIRD" \
  TERMSURF_WEBTUI_STATE_TRACE_FILE="$WEBTUI_TRACE" \
  "$APP_BIN" \
  --window-save-state=never \
  --confirm-close-surface=false \
  --initial-command="direct:$COMMAND" >"$APP_LOG" 2>&1 &
PID="$!"
log "pid=$PID"

wait_for_pattern "TermSurf message decoded type=HelloRequest" "WebTUI connected to Astrohacker Terminal"
wait_for_pattern "SetOverlay: pane_id=.* profile=default browser=ladybird url=${URL}" "SetOverlay names Ladybird"
wait_for_pattern "SetOverlay: named browser resolved browser=ladybird env=(ASTROHACKER_LADYBIRD_PATH|TERMSURF_GIRLBAT_PATH) path=${LADYBIRD}" "Astrohacker Terminal resolved named Ladybird"
wait_for_pattern "spawned browser path=${LADYBIRD} pid=[0-9]+ profile=default browser=ladybird .*render_surface_service=com\\.termsurf\\.ladybird\\.render\\." "Astrohacker Terminal spawned Ladybird with render side-channel"
wait_for_literal "[Ladybird] render side-channel global connected=true" "Ladybird connected render side-channel"
wait_for_pattern "ServerRegister: profile=default browser=ladybird" "Astrohacker Terminal registered Ladybird"
wait_for_pattern "TabReady: pane_id=.* tab_id=[0-9]+" "Astrohacker Terminal mapped Ladybird TabReady"
wait_for_pattern "BrowserReady: pane_id=.* tab_id=[0-9]+ socket=.* browser=ladybird" "Astrohacker Terminal sent BrowserReady for Ladybird"
wait_for_trace_pattern "event=render_state[[:space:]].*browser_label=ladybird" "WebTUI footer labels Ladybird" 90
wait_for_pattern "\\[Ladybird\\] engine load finished tab_id=[0-9]+ url=${URL}" "Ladybird finished normal HTTP page load" 90
wait_for_pattern "\\[Ladybird\\] engine RenderSurface metadata sent_to=[1-9][0-9]* tab_id=[0-9]+ generation=[0-9]+ attachment_id=[1-9][0-9]*" "Ladybird emitted nonzero RenderSurface metadata"
wait_for_pattern "RenderSurface: (tab_id=[0-9]+ pane_id=.* generation=[0-9]+ pixel=[1-9][0-9]*x[1-9][0-9]* .* attachment_id=[1-9][0-9]*|pending attachment browser=ladybird profile=default tab_id=[0-9]+ attachment_id=[1-9][0-9]*)" "Astrohacker Terminal received Ladybird RenderSurface metadata"
wait_for_pattern "layer=bridge event=present_iosurface_target_found .*attachment_id=[1-9][0-9]*" "Bridge targeted Ladybird IOSurface"
wait_for_pattern "layer=appkit event=presented_iosurface .*context_id=[1-9][0-9]* .*visible=true" "AppKit structurally presented Ladybird IOSurface"
wait_for_pattern "layer=appkit event=presented_iosurface_pixels .*attachment_id=[1-9][0-9]* .*visible=true note=reported-presented-iosurface-pixels" "AppKit reported structural IOSurface presentation pixels"

if grep -E "RendererCrashed|engine render surface export failed|render surface send skipped" "$APP_LOG" >/dev/null 2>&1; then
  fail "unexpected Ladybird crash or render-surface failure; see $APP_LOG"
fi

log "PASS: ladybird Astrohacker Terminal runtime structural presentation smoke"
