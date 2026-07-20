#!/usr/bin/env bash
set -euo pipefail

SMOKE_ROOT="$(cd "$(dirname "$0")" && pwd)"
LIB_ROOT="$(cd "$SMOKE_ROOT/.." && pwd)"
REPO_ROOT="$(git -C "$LIB_ROOT" rev-parse --show-toplevel)"
FORK="$REPO_ROOT/forks/ladybird"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ladybird-refresh-action.XXXXXX")"
LOG="$RUN_DIR/smoke.log"
EXPECTED="REFRESH_ACTION_SMOKE_PASS engine=ladybird tabs=2 reload=1 capability=1 history_unchanged=1 request_correlation=1 disabled=1 isolation=1 failed_reload=1 crash_recovery=1 cleanup=1 future_actions_rejected=1"
SERVER_PID=""

cleanup() {
  if [[ -n "$SERVER_PID" ]]; then kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; fi
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

test "$(git -C "$FORK" rev-parse --abbrev-ref HEAD)" = \
  "2a3bc6a3-issue-26071521449339-refresh-button" || {
  printf '%s\n' "Ladybird Refresh smoke requires the issue branch" >&2
  exit 1
}

TERMSURF_LADYBIRD_BACKEND=real "$LIB_ROOT/build.sh" --configuration Debug --clean

python3 - "$RUN_DIR/port" <<'PY' &
import http.server, socketserver, sys
counts = {"/a": 0, "/b": 0}
class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def do_GET(self):
        path = self.path.split("?", 1)[0]
        if path not in counts:
            self.send_response(404); self.send_header("Content-Length", "0"); self.end_headers(); return
        counts[path] += 1
        count = counts[path]
        status = 500 if path == "/a" and count == 2 else 200
        label = "A" if path == "/a" else "B"
        data = f"<!doctype html><meta charset=utf-8><title>{label} reload={count} status={status}</title>".encode()
        self.send_response(status); self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Cache-Control", "no-store"); self.send_header("Content-Length", str(len(data)))
        self.end_headers(); self.wfile.write(data)
    def log_message(self, fmt, *args): return
class Server(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True; allow_reuse_address = True
server = Server(("127.0.0.1", 0), Handler)
with open(sys.argv[1], "w", encoding="utf-8") as file: file.write(str(server.server_address[1]))
server.serve_forever()
PY
SERVER_PID=$!
for _ in $(seq 1 100); do test -s "$RUN_DIR/port" && break; sleep 0.05; done
test -s "$RUN_DIR/port"
PORT="$(tr -d '\n' < "$RUN_DIR/port")"

set +e
(
  cd "$REPO_ROOT/rust"
  TERMSURF_LADYBIRD_BACKEND=real \
    TERMSURF_LADYBIRD_SMOKE_BASE_URL="http://127.0.0.1:$PORT" \
    cargo run -p ah-ladybirdd -- --termsurf-refresh-action-smoke
) >"$LOG" 2>&1
run_status=$?
set -e
grep -v '^REFRESH_ACTION_SMOKE_PASS engine=' "$LOG" || true
if [[ $run_status -ne 0 ]]; then exit "$run_status"; fi
if [[ "$(grep -Fxc "$EXPECTED" "$LOG" || true)" -ne 1 ]]; then
  echo "missing unique exact Ladybird Refresh pass marker" >&2
  exit 1
fi
printf '%s\n' "$EXPECTED"
