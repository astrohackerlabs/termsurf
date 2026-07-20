#!/usr/bin/env bash
set -euo pipefail

SMOKE_ROOT="$(cd "$(dirname "$0")" && pwd)"
LIB_ROOT="$(cd "$SMOKE_ROOT/.." && pwd)"
REPO_ROOT="$(git -C "$LIB_ROOT" rev-parse --show-toplevel)"
WEBKIT_BUILD="$REPO_ROOT/forks/webkit/src/WebKitBuild/Release"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/webkit-refresh-action.XXXXXX")"
EXPECTED="REFRESH_ACTION_SMOKE_PASS engine=webkit tabs=2 reload=1 capability=1 history_unchanged=1 request_correlation=1 disabled=1 isolation=1 failed_reload=1 crash_recovery=1 cleanup=1 future_actions_rejected=1"
SERVER_PID=""

cleanup() {
  if [[ -n "$SERVER_PID" ]]; then kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; fi
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

TERMSURF_WEBKIT_CONFIGURATION=Release "$LIB_ROOT/build.sh" --release
otool -l "$LIB_ROOT/build/libtermsurf_webkit.dylib" | grep -Fq "path $WEBKIT_BUILD"

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

DYLD_FRAMEWORK_PATH="$WEBKIT_BUILD" \
  "$LIB_ROOT/build/refresh-action-smoke" "http://127.0.0.1:$PORT" | tee "$RUN_DIR/smoke.log"
test "$(grep -Fxc "$EXPECTED" "$RUN_DIR/smoke.log" || true)" -eq 1
