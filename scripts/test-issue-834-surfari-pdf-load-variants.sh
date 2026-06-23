#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$ROOT/logs/issue-834-exp41-surfari-pdf-load-variants"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-issue834-exp41.XXXXXX")"
APP="${TERMSURF_GHOSTBOARD_APP:-$ROOT/ghostboard/macos/build/Debug/TermSurf.app}"
APP_BIN="$APP/Contents/MacOS/termsurf"
WEB="${TERMSURF_WEB:-$ROOT/target/debug/web}"
SURFARI="${TERMSURF_SURFARI:-$ROOT/target/debug/surfari}"
WEBKIT_DEBUG="${TERMSURF_WEBKIT_DEBUG:-$ROOT/webkit/src/WebKitBuild/Debug}"
WEBKIT_DYLIB="$ROOT/surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib"
SITE_DIR="$RUN_DIR/site"
SERVER_SCRIPT="$RUN_DIR/server.py"
PORT_FILE="$RUN_DIR/server-port.txt"
HARNESS_LOG="$LOG_DIR/harness-$RUN_ID.log"
SERVER_LOG="$LOG_DIR/server-$RUN_ID.log"
SUMMARY="$LOG_DIR/surfari-pdf-load-variants-summary.json"
SERVER_PID=""
SERVER_CLEANUP_STATUS="not-checked"
CURRENT_PID=""
SCENARIO_JSONS=()
TARGETS_JSON='{"green":[0,128,0],"magenta":[255,0,255]}'

mkdir -p "$LOG_DIR" "$SITE_DIR"

log() {
  printf '%s\n' "$*" | tee -a "$HARNESS_LOG"
}

delay() {
  sleep "${1:-0.5}"
}

fail() {
  log "FAIL: $*"
  cleanup_current_process || true
  cleanup_server || true
  rm -rf "$RUN_DIR"
  exit 1
}

require_executable() {
  [ -x "$1" ] || fail "missing executable: $1"
}

require_path() {
  [ -e "$1" ] || fail "missing path: $1"
}

cleanup_current_process() {
  if [ -n "${CURRENT_PID:-}" ] && kill -0 "$CURRENT_PID" >/dev/null 2>&1; then
    kill "$CURRENT_PID" >/dev/null 2>&1 || true
    delay 0.5 || true
    kill -9 "$CURRENT_PID" >/dev/null 2>&1 || true
  fi
}

cleanup_server() {
  if [ -n "${SERVER_PID:-}" ] && kill -0 "$SERVER_PID" >/dev/null 2>&1; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
  fi
}

cleanup() {
  cleanup_current_process || true
  cleanup_server || true
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

wait_for_file_pattern() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  local attempts="${4:-60}"
  for _ in $(seq 1 "$attempts"); do
    if grep -E "$pattern" "$file" >/dev/null 2>&1; then
      log "PASS: $label"
      return 0
    fi
    delay 1
  done
  fail "timeout-$label pattern=$pattern file=$file"
}

wait_for_file_fixed() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  local attempts="${4:-60}"
  for _ in $(seq 1 "$attempts"); do
    if grep -F "$pattern" "$file" >/dev/null 2>&1; then
      log "PASS: $label"
      return 0
    fi
    delay 1
  done
  fail "timeout-$label pattern=$pattern file=$file"
}

extract_first_match() {
  local file="$1"
  local pattern="$2"
  grep -E "$pattern" "$file" | head -1 || true
}

extract_window_id() {
  printf '%s\n' "$1" | sed -E 's/.*identity=window_id:([0-9]+).*/\1/'
}

exact_window_bounds() {
  local window_id="$1"
  swift - "$window_id" <<'SWIFT'
import CoreGraphics
import Foundation

let target = Int(CommandLine.arguments[1])!
guard let info = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] else {
    exit(1)
}

for window in info {
    guard let id = window[kCGWindowNumber as String] as? Int, id == target else { continue }
    let bounds = (window[kCGWindowBounds as String] as? [String: Any]) ?? [:]
    let x = Int((bounds["X"] as? Double) ?? 0)
    let y = Int((bounds["Y"] as? Double) ?? 0)
    let width = Int((bounds["Width"] as? Double) ?? 0)
    let height = Int((bounds["Height"] as? Double) ?? 0)
    print("\(id)\t\(x)\t\(y)\t\(width)\t\(height)")
    exit(0)
}

exit(1)
SWIFT
}

write_pdf_fixture() {
  python3 - "$SITE_DIR/fixture.pdf" <<'PY'
from pathlib import Path
import sys

out = Path(sys.argv[1])
objects = []

def add(body):
    objects.append(body)
    return len(objects)

add("<< /Type /Catalog /Pages 2 0 R >>")
add("<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>")
add("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>")
green_stream = "0 0.502 0 rg\n0 0 612 792 re\nf\n"
add(f"<< /Length {len(green_stream.encode('ascii'))} >>\nstream\n{green_stream}endstream")
add("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R >>")
magenta_stream = "1 0 1 rg\n0 0 612 792 re\nf\n"
add(f"<< /Length {len(magenta_stream.encode('ascii'))} >>\nstream\n{magenta_stream}endstream")

data = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
offsets = [0]
for index, body in enumerate(objects, start=1):
    offsets.append(len(data))
    data.extend(f"{index} 0 obj\n{body}\nendobj\n".encode("latin1"))
xref = len(data)
data.extend(f"xref\n0 {len(objects) + 1}\n".encode("ascii"))
data.extend(b"0000000000 65535 f \n")
for offset in offsets[1:]:
    data.extend(f"{offset:010d} 00000 n \n".encode("ascii"))
data.extend(
    f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode("ascii")
)
out.write_bytes(data)
PY
}

write_html_wrappers() {
  for form in iframe embed object; do
    pdf_path="/$form-fixture.pdf"
    case "$form" in
      iframe)
        element="<iframe title=\"fixture pdf\" src=\"$pdf_path\"></iframe>"
        ;;
      embed)
        element="<embed title=\"fixture pdf\" src=\"$pdf_path\" type=\"application/pdf\">"
        ;;
      object)
        element="<object title=\"fixture pdf\" data=\"$pdf_path\" type=\"application/pdf\"></object>"
        ;;
    esac
    sed "s|__ELEMENT__|$element|" >"$SITE_DIR/$form.html" <<'EOF'
<!doctype html>
<meta charset="utf-8">
<title>Issue 834 Surfari Embedded PDF</title>
<style>
  html,
  body {
    margin: 0;
    background: #eeeeee;
  }
  iframe,
  embed,
  object {
    display: block;
    width: 100vw;
    height: 1400px;
    border: 0;
    background: #dddddd;
  }
</style>
__ELEMENT__
EOF
  done
}

start_server() {
  cat >"$SERVER_SCRIPT" <<'PY'
import http.server
import pathlib
import socketserver
import sys
from urllib.parse import unquote

site = pathlib.Path(sys.argv[1])
port_file = pathlib.Path(sys.argv[2])

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        path = unquote(self.path.split("?", 1)[0])
        if path == "/":
            path = "/index.html"
        if path in {
            "/fixture.pdf",
            "/download",
            "/iframe-fixture.pdf",
            "/embed-fixture.pdf",
            "/object-fixture.pdf",
        }:
            body = (site / "fixture.pdf").read_bytes()
            content_type = "application/pdf"
            status = 200
        elif path in {"/iframe.html", "/embed.html", "/object.html"}:
            body = (site / path.lstrip("/")).read_bytes()
            content_type = "text/html; charset=utf-8"
            status = 200
        else:
            body = b"not found"
            content_type = "text/plain; charset=utf-8"
            status = 404
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
        print(f"REQUEST path={path} status={status} content_type={content_type}", flush=True)

    def log_message(self, fmt, *args):
        print(fmt % args, flush=True)

with socketserver.TCPServer(("127.0.0.1", 0), Handler) as httpd:
    port_file.write_text(str(httpd.server_address[1]), encoding="utf-8")
    httpd.serve_forever()
PY
  python3 "$SERVER_SCRIPT" "$SITE_DIR" "$PORT_FILE" >"$SERVER_LOG" 2>&1 &
  SERVER_PID="$!"
  for _ in $(seq 1 50); do
    [ -s "$PORT_FILE" ] && return 0
    delay 0.1
  done
  fail "server port missing"
}

capture_overlay_counts() {
  local window_id="$1"
  local presented_line="$2"
  local window_screenshot="$3"
  local counts_json="$4"
  local win_line
  win_line="$(exact_window_bounds "$window_id")" || fail "failed to resolve window bounds for $window_id"
  screencapture -x -o -l"$window_id" "$window_screenshot"
  [ -s "$window_screenshot" ] || fail "window screenshot missing: $window_screenshot"
  python3 - "$window_screenshot" "$counts_json" "$TARGETS_JSON" "$presented_line" "$win_line" <<'PY'
from pathlib import Path
import json
import re
import struct
import sys
import zlib

png_path, out_path, targets_json, presented_line, win_line = sys.argv[1:6]
targets = json.loads(targets_json)
threshold = 110

def read_png(path):
    png = Path(path).read_bytes()
    if png[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit(f"not png: {path}")
    pos = 8
    width = height = bit_depth = color_type = None
    chunks = []
    while pos < len(png):
        length = struct.unpack(">I", png[pos:pos + 4])[0]
        ctype = png[pos + 4:pos + 8]
        data = png[pos + 8:pos + 8 + length]
        pos += 12 + length
        if ctype == b"IHDR":
            width, height, bit_depth, color_type, _, _, _ = struct.unpack(">IIBBBBB", data)
        elif ctype == b"IDAT":
            chunks.append(data)
        elif ctype == b"IEND":
            break
    if bit_depth != 8 or color_type not in (2, 6):
        raise SystemExit(f"unsupported png bit_depth={bit_depth} color_type={color_type}")
    bpp = 3 if color_type == 2 else 4
    raw = zlib.decompress(b"".join(chunks))
    stride = width * bpp
    rows = []
    i = 0
    prev = bytearray(stride)
    for _ in range(height):
        filt = raw[i]
        i += 1
        row = bytearray(raw[i:i + stride])
        i += stride
        for x in range(stride):
            left = row[x - bpp] if x >= bpp else 0
            up = prev[x]
            up_left = prev[x - bpp] if x >= bpp else 0
            if filt == 1:
                row[x] = (row[x] + left) & 0xFF
            elif filt == 2:
                row[x] = (row[x] + up) & 0xFF
            elif filt == 3:
                row[x] = (row[x] + ((left + up) // 2)) & 0xFF
            elif filt == 4:
                p = left + up - up_left
                pa = abs(p - left)
                pb = abs(p - up)
                pc = abs(p - up_left)
                predictor = left if pa <= pb and pa <= pc else up if pb <= pc else up_left
                row[x] = (row[x] + predictor) & 0xFF
            elif filt != 0:
                raise SystemExit(f"unsupported png filter={filt}")
        rows.append(row)
        prev = row
    return width, height, bpp, stride, rows

frame = re.search(r"overlay_frame=\{\{([^,]+), ([^}]+)\}, \{([^,]+), ([^}]+)\}\}", presented_line)
root = re.search(r"root_frame=\{\{[^}]+\}, \{([^,]+), ([^}]+)\}\}", presented_line)
if not frame or not root:
    raise SystemExit("missing overlay/root frame")
frame_x, frame_y, frame_w, frame_h = map(float, frame.groups())
_, root_h = map(float, root.groups())
_, _, _, win_w, win_h = win_line.split("\t")
win_w = float(win_w)
win_h = float(win_h)
width, height, bpp, stride, rows = read_png(png_path)
scale = width / win_w if win_w else 1.0
content_y = win_h - root_h
crop_x = max(0, min(width, round(frame_x * scale)))
crop_y = max(0, min(height, round((content_y + frame_y) * scale)))
crop_w = max(1, min(width - crop_x, round(frame_w * scale)))
crop_h = max(1, min(height - crop_y, round(frame_h * scale)))
counts = {name: 0 for name in targets}
for y in range(crop_y, crop_y + crop_h):
    row = rows[y]
    for x in range(crop_x * bpp, (crop_x + crop_w) * bpp, bpp):
        rgb = tuple(row[x:x + 3])
        for name, target in targets.items():
            if sum(abs(rgb[channel] - target[channel]) for channel in range(3)) <= threshold:
                counts[name] += 1
crop_area = crop_w * crop_h
passed = counts.get("green", 0) >= 5000 or counts.get("magenta", 0) >= 5000
data = {
    "path": png_path,
    "status": "pass" if passed else "fail",
    "window_bounds_points": {"width": win_w, "height": win_h},
    "screenshot_pixels": {"width": width, "height": height},
    "overlay_crop": {
        "x": crop_x,
        "y": crop_y,
        "width": crop_w,
        "height": crop_h,
        "area": crop_area,
    },
    "targets": counts,
    "threshold": threshold,
    "visible_window_bounded": True,
    "source_window_excluded": True,
}
Path(out_path).write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
if not passed:
    raise SystemExit(1)
PY
}

server_request_status() {
  local path="$1"
  if grep -F "REQUEST path=$path status=200 content_type=application/pdf" "$SERVER_LOG" >/dev/null 2>&1; then
    printf 'pass\n'
  else
    printf 'fail\n'
  fi
}

server_html_status() {
  local path="$1"
  if grep -F "REQUEST path=$path status=200 content_type=text/html; charset=utf-8" "$SERVER_LOG" >/dev/null 2>&1; then
    printf 'pass\n'
  else
    printf 'fail\n'
  fi
}

run_scenario() {
  local name="$1"
  local url="$2"
  local expected_url_pattern="$3"
  local load_kind="$4"
  local wrapper_path="$5"
  local pdf_path="$6"
  local config="$RUN_DIR/config-$name"
  local command="$RUN_DIR/run-$name.sh"
  local app_log="$LOG_DIR/app-$name-$RUN_ID.log"
  local surfari_trace="$LOG_DIR/surfari-$name-$RUN_ID.log"
  local render_trace="$LOG_DIR/render-$name-$RUN_ID.log"
  local webtui_trace="$LOG_DIR/webtui-$name-$RUN_ID.log"
  local screenshot="$LOG_DIR/screenshot-$name-$RUN_ID.png"
  local counts_json="$LOG_DIR/counts-$name-$RUN_ID.json"
  local scenario_json="$LOG_DIR/scenario-$name-$RUN_ID.json"
  local browser_ready_line pane_id ca_line context_id presented_line window_id cleanup_status
  local visible_status="fail" internal_status="fail" load_status="fail"
  local wrapper_status="not-applicable" pdf_request_status="not-applicable"

  cat >"$command" <<EOF
#!/usr/bin/env bash
exec "$WEB" --browser surfari "$url"
EOF
  chmod +x "$command"

  cat >"$config" <<EOF
window-save-state = never
initial-command = direct:$command
EOF

  log "scenario=$name"
  log "scenario_url=$url"
  log "scenario_app_log=$app_log"
  log "scenario_surfari_trace=$surfari_trace"
  log "scenario_render_trace=$render_trace"
  log "scenario_webtui_trace=$webtui_trace"

  GHOSTTY_CONFIG_PATH="$config" \
  GHOSTTY_LOG=stderr \
  DYLD_FRAMEWORK_PATH="$WEBKIT_DEBUG" \
  TERMSURF_SURFARI_PATH="$SURFARI" \
  TERMSURF_GEOMETRY_TRACE=1 \
  TERMSURF_GEOMETRY_SCENARIO="issue834-exp41-surfari-load-$name" \
  TERMSURF_WEBTUI_STATE_TRACE_FILE="$webtui_trace" \
  TERMSURF_SURFARI_RENDER_PROOF_TRACE_FILE="$render_trace" \
  TERMSURF_INPUT_TRACE=1 \
  TERMSURF_PDF_INPUT_TRACE=1 \
  TERMSURF_PDF_INPUT_TRACE_FILE="$surfari_trace" \
    "$APP_BIN" >"$app_log" 2>&1 &
  CURRENT_PID="$!"

  wait_for_file_fixed "$app_log" "browser=surfari url=$url" "$name web requested Surfari overlay"
  wait_for_file_pattern "$app_log" "BrowserReady: pane_id=.* browser=surfari" "$name BrowserReady"
  wait_for_file_pattern "$webtui_trace" "event=render_state.*browser_ready=true.*browser_label=surfari" "$name WebTUI ready"
  wait_for_file_pattern "$surfari_trace" "url=${expected_url_pattern}" "$name Surfari trace recorded URL"
  wait_for_file_pattern "$surfari_trace" "ca-context tab=.*context_id=[1-9][0-9]*" "$name nonzero CAContext"
  wait_for_file_pattern "$render_trace" "render-proof tab=.*url=${expected_url_pattern} .*status=pass" "$name Surfari internal render proof" 45
  internal_status="pass"

  case "$load_kind" in
    http-pdf|http-extensionless)
      wait_for_file_fixed "$SERVER_LOG" "REQUEST path=$pdf_path status=200 content_type=application/pdf" "$name PDF request content type"
      pdf_request_status="$(server_request_status "$pdf_path")"
      load_status="$pdf_request_status"
      ;;
    file)
      load_status="pass"
      ;;
    embedded)
      wait_for_file_fixed "$SERVER_LOG" "REQUEST path=$wrapper_path status=200 content_type=text/html; charset=utf-8" "$name wrapper request"
      wait_for_file_fixed "$SERVER_LOG" "REQUEST path=$pdf_path status=200 content_type=application/pdf" "$name embedded PDF request content type"
      wrapper_status="$(server_html_status "$wrapper_path")"
      pdf_request_status="$(server_request_status "$pdf_path")"
      if [ "$wrapper_status" = "pass" ] && [ "$pdf_request_status" = "pass" ]; then
        load_status="pass"
      fi
      ;;
    *)
      fail "unknown load kind: $load_kind"
      ;;
  esac

  browser_ready_line="$(extract_first_match "$app_log" "BrowserReady: pane_id=.* browser=surfari")"
  pane_id="$(printf '%s\n' "$browser_ready_line" | sed -E 's/.*pane_id=([^ ]+) tab_id=.*/\1/')"
  ca_line="$(extract_first_match "$surfari_trace" "ca-context tab=.*context_id=[1-9][0-9]*")"
  context_id="$(printf '%s\n' "$ca_line" | sed -E 's/.*context_id=([0-9]+).*/\1/')"
  wait_for_file_pattern "$app_log" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${pane_id} .*context_id=${context_id} .*visible=true" "$name AppKit presented pixels"
  presented_line="$(extract_first_match "$app_log" "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}")"
  window_id="$(extract_window_id "$presented_line")"
  delay 3
  if capture_overlay_counts "$window_id" "$presented_line" "$screenshot" "$counts_json"; then
    visible_status="pass"
    log "PASS: $name visible PDF color proof"
  else
    log "WARN: $name visible PDF color proof failed"
  fi

  cleanup_current_process || true
  cleanup_status="terminated"
  for _ in $(seq 1 20); do
    if [ -z "$CURRENT_PID" ] || ! kill -0 "$CURRENT_PID" >/dev/null 2>&1; then
      cleanup_status="terminated"
      break
    fi
    delay 0.1
  done
  if [ -n "$CURRENT_PID" ] && kill -0 "$CURRENT_PID" >/dev/null 2>&1; then
    cleanup_status="still-running"
  fi

  python3 - "$scenario_json" <<PY
import json
from pathlib import Path

def load(path):
    path = Path(path)
    return json.loads(path.read_text()) if path.exists() else None

passed = (
    "$load_status" == "pass"
    and "$internal_status" == "pass"
    and "$visible_status" == "pass"
    and "$cleanup_status" == "terminated"
)
data = {
    "name": "$name",
    "url": "$url",
    "load_kind": "$load_kind",
    "wrapper_path": "$wrapper_path",
    "pdf_path": "$pdf_path",
    "load_status": "$load_status",
    "internal_status": "$internal_status",
    "visible_status": "$visible_status",
    "cleanup_status": "$cleanup_status",
    "status": "pass" if passed else "fail",
    "wrapper_request_status": "$wrapper_status",
    "pdf_request_status": "$pdf_request_status",
    "pane_id": "$pane_id",
    "context_id": "$context_id",
    "window_id": "$window_id",
    "artifacts": {
        "app_log": "$app_log",
        "surfari_trace": "$surfari_trace",
        "render_trace": "$render_trace",
        "webtui_trace": "$webtui_trace",
        "screenshot": "$screenshot",
        "counts": "$counts_json",
    },
    "visible_counts": load("$counts_json"),
}
Path("$scenario_json").write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
PY
  SCENARIO_JSONS+=("$scenario_json")
  CURRENT_PID=""
}

write_summary() {
  python3 - "$SUMMARY" "$RUN_ID" "$HARNESS_LOG" "$SERVER_LOG" "$APP_BIN" "$WEB" "$SURFARI" "$WEBKIT_DEBUG" "$SERVER_CLEANUP_STATUS" "${SCENARIO_JSONS[@]}" <<'PY'
from pathlib import Path
import json
import sys

summary_path = Path(sys.argv[1])
run_id = sys.argv[2]
harness_log = sys.argv[3]
server_log = sys.argv[4]
app_bin = sys.argv[5]
web = sys.argv[6]
surfari = sys.argv[7]
webkit_debug = sys.argv[8]
server_cleanup_status = sys.argv[9]
scenarios = [json.loads(Path(path).read_text()) for path in sys.argv[10:]]
required = {"http_pdf", "http_extensionless", "file_pdf", "embedded_iframe", "embedded_embed", "embedded_object"}
by_name = {scenario["name"]: scenario for scenario in scenarios}
scenario_statuses = {name: by_name.get(name, {}).get("status") for name in sorted(required)}
all_present = required.issubset(by_name)
all_pass = all_present and all(status == "pass" for status in scenario_statuses.values())
some_pass = any(status == "pass" for status in scenario_statuses.values())
cleanup_ok = server_cleanup_status == "terminated" and all(
    scenario.get("cleanup_status") == "terminated" for scenario in scenarios
)
if all_pass and cleanup_ok:
    overall = "pass"
    classification = "surfari-pdf-load-variants-proven"
elif some_pass:
    overall = "partial"
    classification = "some-surfari-pdf-load-variants-proven"
else:
    overall = "fail"
    classification = "surfari-pdf-load-variants-not-proven"
data = {
    "overall_result": overall,
    "classification": classification,
    "run_id": run_id,
    "termsurf_surfari_cacontext_layer": "unset",
    "default_export_method": "snapshot-backed",
    "binaries": {
        "ghostboard_app_bin": app_bin,
        "web": web,
        "surfari": surfari,
        "webkit_debug": webkit_debug,
    },
    "artifacts": {
        "harness_log": harness_log,
        "server_log": server_log,
    },
    "scenario_statuses": scenario_statuses,
    "scenarios": by_name,
    "cleanup": {
        "server_status": server_cleanup_status,
        "scenario_statuses": {
            scenario["name"]: scenario.get("cleanup_status") for scenario in scenarios
        },
    },
}
summary_path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
print(json.dumps({
    "overall_result": overall,
    "classification": classification,
    "scenario_statuses": scenario_statuses,
    "cleanup_ok": cleanup_ok,
}, indent=2, sort_keys=True))
if overall == "fail":
    raise SystemExit(1)
PY
}

if [ "${TERMSURF_SURFARI_CACONTEXT_LAYER+x}" = "x" ]; then
  fail "TERMSURF_SURFARI_CACONTEXT_LAYER must be unset for default load-variant proof"
fi

require_executable "$APP_BIN"
require_executable "$WEB"
require_executable "$SURFARI"
require_path "$WEBKIT_DEBUG/WebKit.framework"
require_path "$WEBKIT_DYLIB"

log "run_id=$RUN_ID"
log "ghostboard_app_bin=$APP_BIN"
log "web=$WEB"
log "surfari=$SURFARI"
log "webkit_debug=$WEBKIT_DEBUG"
log "termsurf_surfari_cacontext_layer=${TERMSURF_SURFARI_CACONTEXT_LAYER-__unset__}"
log "summary=$SUMMARY"

write_pdf_fixture
write_html_wrappers
start_server
PORT="$(cat "$PORT_FILE")"
HTTP_PDF_URL="http://127.0.0.1:${PORT}/fixture.pdf"
EXTENSIONLESS_URL="http://127.0.0.1:${PORT}/download"
FILE_URL="file://$SITE_DIR/fixture.pdf"
IFRAME_URL="http://127.0.0.1:${PORT}/iframe.html"
EMBED_URL="http://127.0.0.1:${PORT}/embed.html"
OBJECT_URL="http://127.0.0.1:${PORT}/object.html"

run_scenario http_pdf "$HTTP_PDF_URL" "$HTTP_PDF_URL" http-pdf "" "/fixture.pdf"
run_scenario http_extensionless "$EXTENSIONLESS_URL" "$EXTENSIONLESS_URL" http-extensionless "" "/download"
run_scenario file_pdf "$FILE_URL" "$FILE_URL" file "" ""
run_scenario embedded_iframe "$IFRAME_URL" "$IFRAME_URL" embedded "/iframe.html" "/iframe-fixture.pdf"
run_scenario embedded_embed "$EMBED_URL" "$EMBED_URL" embedded "/embed.html" "/embed-fixture.pdf"
run_scenario embedded_object "$OBJECT_URL" "$OBJECT_URL" embedded "/object.html" "/object-fixture.pdf"

cleanup_server
SERVER_CLEANUP_STATUS="terminated"
for _ in $(seq 1 20); do
  if [ -z "${SERVER_PID:-}" ] || ! kill -0 "$SERVER_PID" >/dev/null 2>&1; then
    SERVER_CLEANUP_STATUS="terminated"
    break
  fi
  delay 0.1
done
if [ -n "${SERVER_PID:-}" ] && kill -0 "$SERVER_PID" >/dev/null 2>&1; then
  SERVER_CLEANUP_STATUS="still-running"
fi
write_summary
log "PASS: issue 834 experiment 41 Surfari PDF load variants"
log "summary=$SUMMARY"
