#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$ROOT/logs/issue-834-exp42-surfari-pdf-input-navigation"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-issue834-exp42.XXXXXX")"
APP="${TERMSURF_GHOSTBOARD_APP:-$ROOT/ghostboard/macos/build/Debug/TermSurf.app}"
APP_BIN="$APP/Contents/MacOS/termsurf"
WEB="${TERMSURF_WEB:-$ROOT/target/debug/web}"
SURFARI="${TERMSURF_SURFARI:-$ROOT/target/debug/surfari}"
WEBKIT_DEBUG="${TERMSURF_WEBKIT_DEBUG:-$ROOT/webkit/src/WebKitBuild/Debug}"
WEBKIT_DYLIB="$ROOT/surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib"
SITE_DIR="$RUN_DIR/site"
PORT_FILE="$RUN_DIR/server-port.txt"
SERVER_SCRIPT="$RUN_DIR/server.py"
HARNESS_LOG="$LOG_DIR/harness-$RUN_ID.log"
SERVER_LOG="$LOG_DIR/server-$RUN_ID.log"
SUMMARY="$LOG_DIR/surfari-pdf-input-navigation-summary.json"
SERVER_PID=""
SERVER_CLEANUP_STATUS="not-checked"
CURRENT_PID=""
SCENARIO_JSONS=()

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

line_count() {
  local file="$1"
  if [ -r "$file" ]; then
    wc -l <"$file" | tr -d ' '
  else
    printf '0\n'
  fi
}

wait_for_line_after() {
  local file="$1"
  local start_line="$2"
  local pattern="$3"
  local label="$4"
  local attempts="${5:-60}"
  local line
  for _ in $(seq 1 "$attempts"); do
    line="$(tail -n +"$((start_line + 1))" "$file" | grep -E "$pattern" | tail -1 || true)"
    if [ -n "$line" ]; then
      printf '%s\n' "PASS: $label" | tee -a "$HARNESS_LOG" >&2
      printf '%s\n' "$line"
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

extract_appkit_pixel() {
  printf '%s\n' "$1" | sed -E 's/.*appkit_pixel=([0-9]+x[0-9]+).*/\1/'
}

extract_context_id() {
  printf '%s\n' "$1" | sed -E 's/.*context_id=([0-9]+).*/\1/'
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

activate_pid() {
  local pid="$1"
  local label="$2"
  local front_pid
  front_pid="$(osascript -e 'tell application "System Events" to set frontmost of first process whose unix id is '"$pid"' to true' \
    -e 'delay 0.25' \
    -e 'tell application "System Events" to unix id of first process whose frontmost is true')"
  if [ "$front_pid" != "$pid" ]; then
    fail "$label frontmost PID mismatch: got=$front_pid expected=$pid"
  fi
  log "PASS: $label frontmost pid=$front_pid"
}

write_fixtures() {
  python3 - "$SITE_DIR/navigation.pdf" <<'PY'
from pathlib import Path
import sys

out = Path(sys.argv[1])
objects = []

def add(body):
    objects.append(body)
    return len(objects)

pages_id = 2
add("<< /Type /Catalog /Pages 2 0 R >>")
add("<< /Type /Pages /Kids [3 0 R 5 0 R 7 0 R] /Count 3 >>")
add("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>")
green_stream = "0 0.502 0 rg\n0 0 612 792 re\nf\n"
add(f"<< /Length {len(green_stream.encode('ascii'))} >>\nstream\n{green_stream}endstream")
add("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R >>")
magenta_stream = "1 0 1 rg\n0 0 612 792 re\nf\n"
add(f"<< /Length {len(magenta_stream.encode('ascii'))} >>\nstream\n{magenta_stream}endstream")
add("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 8 0 R >>")
cyan_stream = "0 1 1 rg\n0 0 612 792 re\nf\n"
add(f"<< /Length {len(cyan_stream.encode('ascii'))} >>\nstream\n{cyan_stream}endstream")

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
        if path == "/navigation.pdf":
            body = (site / "navigation.pdf").read_bytes()
            content_type = "application/pdf"
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

global_point_for_web_point() {
  local win_line="$1"
  local present_line="$2"
  local web_x="$3"
  local web_y="$4"
  python3 - "$win_line" "$present_line" "$web_x" "$web_y" <<'PY'
import re
import sys

win_line, present_line, web_x, web_y = sys.argv[1:5]
_, wx, wy, ww, wh = win_line.split("\t")
wx, wy, ww, wh = map(float, (wx, wy, ww, wh))
web_x = float(web_x)
web_y = float(web_y)

frame = re.search(r"overlay_frame=\{\{([^,]+), ([^}]+)\}, \{([^,]+), ([^}]+)\}\}", present_line)
root = re.search(r"root_frame=\{\{[^}]+\}, \{([^,]+), ([^}]+)\}\}", present_line)
if not frame or not root:
    raise SystemExit(1)
frame_x, frame_y, _, _ = map(float, frame.groups())
_, root_h = map(float, root.groups())
content_y = wh - root_h
print(f"{int(wx + frame_x + web_x + 0.5)} {int(wy + content_y + frame_y + web_y + 0.5)}")
PY
}

capture_overlay_counts() {
  local window_id="$1"
  local presented_line="$2"
  local window_screenshot="$3"
  local counts_json="$4"
  local targets_json="$5"
  local win_line
  win_line="$(exact_window_bounds "$window_id")" || fail "failed to resolve window bounds for $window_id"
  screencapture -x -o -l"$window_id" "$window_screenshot"
  [ -s "$window_screenshot" ] || fail "window screenshot missing: $window_screenshot"
  python3 - "$window_screenshot" "$counts_json" "$targets_json" "$presented_line" "$win_line" <<'PY'
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
root_w, root_h = map(float, root.groups())
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
data = {
    "path": png_path,
    "status": "pass" if any(value >= 5000 for value in counts.values()) else "fail",
    "window_bounds_points": {
        "width": win_w,
        "height": win_h,
    },
    "screenshot_pixels": {
        "width": width,
        "height": height,
    },
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
PY
}

delta_status() {
  local pre_json="$1"
  local post_json="$2"
  local pre_name="$3"
  local post_name="$4"
  local out_json="$5"
  python3 - "$pre_json" "$post_json" "$pre_name" "$post_name" "$out_json" <<'PY'
from pathlib import Path
import json
import sys

pre = json.loads(Path(sys.argv[1]).read_text())
post = json.loads(Path(sys.argv[2]).read_text())
pre_name = sys.argv[3]
post_name = sys.argv[4]
out = Path(sys.argv[5])
pre_counts = pre["targets"]
post_counts = post["targets"]
area = min(pre["overlay_crop"]["area"], post["overlay_crop"]["area"])
minimum = min(5000, max(1, int(area * 0.1)))
pre_before = pre_counts.get(pre_name, 0)
pre_after = post_counts.get(pre_name, 0)
post_before = pre_counts.get(post_name, 0)
post_after = post_counts.get(post_name, 0)
pre_drop = pre_before - pre_after
post_rise = post_after - post_before
passed = (
    pre_before >= 5000
    and post_after >= 5000
    and pre_drop >= minimum
    and post_rise >= minimum
)
data = {
    "status": "pass" if passed else "fail",
    "minimum_delta": minimum,
    "pre_color": pre_name,
    "post_color": post_name,
    "pre_color_before": pre_before,
    "pre_color_after": pre_after,
    "post_color_before": post_before,
    "post_color_after": post_after,
    "pre_color_drop": pre_drop,
    "post_color_rise": post_rise,
}
out.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
if not passed:
    raise SystemExit(1)
PY
}

run_scenario() {
  local name="$1"
  local mode="$2"
  local url="$3"
  local expected_path="$4"
  local targets_json='{"green":[0,128,0],"magenta":[255,0,255],"cyan":[0,255,255]}'
  local config="$RUN_DIR/config-$name"
  local command="$RUN_DIR/run-$name.sh"
  local app_log="$LOG_DIR/app-$name-$RUN_ID.log"
  local surfari_trace="$LOG_DIR/surfari-$name-$RUN_ID.log"
  local render_trace="$LOG_DIR/render-$name-$RUN_ID.log"
  local webtui_trace="$LOG_DIR/webtui-$name-$RUN_ID.log"
  local baseline_png="$LOG_DIR/baseline-$name-$RUN_ID.png"
  local click_png="$LOG_DIR/click-$name-$RUN_ID.png"
  local forward_png="$LOG_DIR/forward-$name-$RUN_ID.png"
  local backward_png="$LOG_DIR/backward-$name-$RUN_ID.png"
  local baseline_json="$LOG_DIR/baseline-$name-$RUN_ID.json"
  local click_json="$LOG_DIR/click-$name-$RUN_ID.json"
  local forward_json="$LOG_DIR/forward-$name-$RUN_ID.json"
  local backward_json="$LOG_DIR/backward-$name-$RUN_ID.json"
  local forward_delta_json="$LOG_DIR/forward-delta-$name-$RUN_ID.json"
  local backward_delta_json="$LOG_DIR/backward-delta-$name-$RUN_ID.json"
  local scenario_json="$LOG_DIR/scenario-$name-$RUN_ID.json"
  local browser_ready_line pane_id browser_tab_id ca_line context_id presented_line window_id win_line
  local point_x point_y mode_start focus_start trace_start refresh_start key_start click_start
  local post_presented_line click_presented_line backward_presented_line
  local webtui_ready_line render_line server_line mode_line focus_line
  local scroll_line scroll_refresh_line click_line page_down_line page_down_refresh_line page_up_line page_up_refresh_line
  local scroll_status="not-applicable" click_status="not-applicable" page_down_status="not-applicable" page_up_status="not-applicable"
  local click_keyboard_route_status="not-applicable"
  local cleanup_status="terminated"

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
  log "scenario_webtui_trace=$webtui_trace"

  GHOSTTY_CONFIG_PATH="$config" \
  GHOSTTY_LOG=stderr \
  DYLD_FRAMEWORK_PATH="$WEBKIT_DEBUG" \
  TERMSURF_SURFARI_PATH="$SURFARI" \
  TERMSURF_GEOMETRY_TRACE=1 \
  TERMSURF_GEOMETRY_SCENARIO="issue834-exp42-surfari-input-$name" \
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
  wait_for_file_fixed "$surfari_trace" "url=$url" "$name Surfari trace recorded URL"
  wait_for_file_pattern "$surfari_trace" "ca-context tab=.*context_id=[1-9][0-9]*" "$name nonzero CAContext"
  wait_for_file_pattern "$render_trace" "render-proof tab=.*url=${url} .*status=pass" "$name Surfari internal render proof" 30
  wait_for_file_fixed "$SERVER_LOG" "REQUEST path=$expected_path status=200 content_type=application/pdf" "$name HTTP served PDF fixture"

  browser_ready_line="$(extract_first_match "$app_log" "BrowserReady: pane_id=.* browser=surfari")"
  webtui_ready_line="$(extract_first_match "$webtui_trace" "event=render_state.*browser_ready=true.*browser_label=surfari")"
  render_line="$(extract_first_match "$render_trace" "render-proof tab=.*url=${url} .*status=pass")"
  server_line="$(extract_first_match "$SERVER_LOG" "REQUEST path=$expected_path status=200 content_type=application/pdf")"
  pane_id="$(printf '%s\n' "$browser_ready_line" | sed -E 's/.*pane_id=([^ ]+) tab_id=.*/\1/')"
  browser_tab_id="$(printf '%s\n' "$browser_ready_line" | sed -E 's/.*tab_id=([0-9]+) socket=.*/\1/')"
  ca_line="$(extract_first_match "$surfari_trace" "ca-context tab=.*context_id=[1-9][0-9]*")"
  context_id="$(printf '%s\n' "$ca_line" | sed -E 's/.*context_id=([0-9]+).*/\1/')"
  wait_for_file_pattern "$app_log" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${pane_id} .*context_id=${context_id} .*visible=true" "$name AppKit presented pixels"
  presented_line="$(extract_first_match "$app_log" "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}")"
  window_id="$(extract_window_id "$presented_line")"
  win_line="$(exact_window_bounds "$window_id")" || fail "$name failed to resolve window bounds"
  log "${name}_window_bounds=$win_line"

  activate_pid "$CURRENT_PID" "$name pre-browse Ghostboard activation"
  mode_start="$(line_count "$app_log")"
  focus_start="$(line_count "$surfari_trace")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
  mode_line="$(wait_for_line_after "$app_log" "$mode_start" "ModeChanged: pane_id=${pane_id} browsing=true" "$name entered Browse mode")"
  focus_line="$(wait_for_line_after "$surfari_trace" "$focus_start" "focus-changed tab=${browser_tab_id} pane=${pane_id} ffi=ts_set_focus focused=true" "$name Surfari focused")"

  delay 2
  capture_overlay_counts "$window_id" "$presented_line" "$baseline_png" "$baseline_json" "$targets_json"
  read -r point_x point_y <<<"$(global_point_for_web_point "$win_line" "$presented_line" 420 420)"
  log "${name}_input_point=${point_x},${point_y}"

  if [ "$mode" = "scroll" ]; then
    trace_start="$(line_count "$surfari_trace")"
    refresh_start="$(line_count "$app_log")"
    for _ in $(seq 1 12); do
      swift "$ROOT/scripts/ghostty-app/inject.swift" scroll "$point_x" "$point_y" -80 >>"$HARNESS_LOG" 2>&1
      delay 0.15
    done
    scroll_line="$(wait_for_line_after "$surfari_trace" "$trace_start" "scroll-event tab=${browser_tab_id} pane=${pane_id} ffi=ts_forward_scroll_event" "$name Surfari received wheel input")"
    scroll_refresh_line="$(wait_for_line_after "$app_log" "$refresh_start" "snapshot-layer-refresh reason=(scroll|coalesced)" "$name snapshot refreshed after scroll")"
    delay 2
    post_presented_line="$(tail -n +"$((refresh_start + 1))" "$app_log" | grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}" | tail -1 || true)"
    [ -n "$post_presented_line" ] || post_presented_line="$presented_line"
    capture_overlay_counts "$window_id" "$post_presented_line" "$forward_png" "$forward_json" "$targets_json"
    if delta_status "$baseline_json" "$forward_json" green magenta "$forward_delta_json"; then
      scroll_status="pass"
      log "PASS: $name scroll green-to-magenta delta"
    else
      log "WARN: $name scroll green-to-magenta delta failed"
    fi
  elif [ "$mode" = "keyboard" ]; then
    click_start="$(line_count "$surfari_trace")"
    swift "$ROOT/scripts/ghostty-app/inject.swift" click "$point_x" "$point_y" left >>"$HARNESS_LOG" 2>&1
    click_line="$(wait_for_line_after "$surfari_trace" "$click_start" "mouse-event tab=${browser_tab_id} pane=${pane_id} ffi=ts_forward_mouse_event type=(down|up)" "$name Surfari received click")"
    delay 1
    click_presented_line="$(tail -n +"$((click_start + 1))" "$app_log" | grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}" | tail -1 || true)"
    [ -n "$click_presented_line" ] || click_presented_line="$presented_line"
    capture_overlay_counts "$window_id" "$click_presented_line" "$click_png" "$click_json" "$targets_json"
    if python3 - "$click_json" <<'PY'
from pathlib import Path
import json
import sys

data = json.loads(Path(sys.argv[1]).read_text())
raise SystemExit(0 if data["targets"].get("green", 0) >= 5000 else 1)
PY
    then
      click_status="pass"
      log "PASS: $name click preserved PDF visibility"
    else
      log "WARN: $name click visibility proof failed"
    fi

    key_start="$(line_count "$surfari_trace")"
    refresh_start="$(line_count "$app_log")"
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 121 >>"$HARNESS_LOG" 2>&1
    page_down_line="$(wait_for_line_after "$surfari_trace" "$key_start" "key-event tab=${browser_tab_id} pane=${pane_id} ffi=ts_forward_key_event type=down" "$name Surfari received PageDown")"
    click_keyboard_route_status="pass"
    page_down_refresh_line="$(wait_for_line_after "$app_log" "$refresh_start" "snapshot-layer-refresh reason=(key-event|coalesced)" "$name snapshot refreshed after PageDown")"
    post_presented_line="$(tail -n +"$((refresh_start + 1))" "$app_log" | grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}" | tail -1 || true)"
    [ -n "$post_presented_line" ] || post_presented_line="$presented_line"
    for _ in $(seq 1 10); do
      delay 1
      capture_overlay_counts "$window_id" "$post_presented_line" "$forward_png" "$forward_json" "$targets_json"
      if delta_status "$click_json" "$forward_json" green magenta "$forward_delta_json"; then
        page_down_status="pass"
        log "PASS: $name PageDown green-to-magenta delta"
        break
      fi
    done
    if [ "$page_down_status" != "pass" ]; then
      log "WARN: $name PageDown green-to-magenta delta failed"
    fi

    key_start="$(line_count "$surfari_trace")"
    refresh_start="$(line_count "$app_log")"
    swift "$ROOT/scripts/ghostty-app/inject.swift" key 116 >>"$HARNESS_LOG" 2>&1
    page_up_line="$(wait_for_line_after "$surfari_trace" "$key_start" "key-event tab=${browser_tab_id} pane=${pane_id} ffi=ts_forward_key_event type=down" "$name Surfari received PageUp")"
    page_up_refresh_line="$(wait_for_line_after "$app_log" "$refresh_start" "snapshot-layer-refresh reason=(key-event|coalesced)" "$name snapshot refreshed after PageUp")"
    backward_presented_line="$(tail -n +"$((refresh_start + 1))" "$app_log" | grep -E "TermSurf geometry layer=appkit event=presented .*pane_id:${pane_id} .*context_id=${context_id}" | tail -1 || true)"
    [ -n "$backward_presented_line" ] || backward_presented_line="$post_presented_line"
    for _ in $(seq 1 10); do
      delay 1
      capture_overlay_counts "$window_id" "$backward_presented_line" "$backward_png" "$backward_json" "$targets_json"
      if delta_status "$forward_json" "$backward_json" magenta green "$backward_delta_json"; then
        page_up_status="pass"
        log "PASS: $name PageUp magenta-to-green delta"
        break
      fi
    done
    if [ "$page_up_status" != "pass" ]; then
      log "WARN: $name PageUp magenta-to-green delta failed"
    fi
  else
    fail "unknown scenario mode: $mode"
  fi

  cleanup_current_process || true
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

data = {
    "name": "$name",
    "mode": "$mode",
    "url": "$url",
    "pane_id": "$pane_id",
    "browser_tab_id": "$browser_tab_id",
    "context_id": "$context_id",
    "window_id": "$window_id",
    "input_method": "CGEvent via scripts/ghostty-app/inject.swift",
    "scroll_status": "$scroll_status",
    "click_status": "$click_status",
    "click_keyboard_route_status": "$click_keyboard_route_status",
    "page_down_status": "$page_down_status",
    "page_up_status": "$page_up_status",
    "cleanup_status": "$cleanup_status",
    "baseline_presented_line": """$presented_line""",
    "evidence_lines": {
        "browser_ready": """$browser_ready_line""",
        "webtui_ready": """$webtui_ready_line""",
        "server_pdf_request": """$server_line""",
        "surfari_cacontext": """$ca_line""",
        "render_proof": """$render_line""",
        "browse_mode": """$mode_line""",
        "focus": """$focus_line""",
        "scroll": """$scroll_line""",
        "scroll_refresh": """$scroll_refresh_line""",
        "click": """$click_line""",
        "page_down": """$page_down_line""",
        "page_down_refresh": """$page_down_refresh_line""",
        "page_up": """$page_up_line""",
        "page_up_refresh": """$page_up_refresh_line""",
    },
    "starting_color": "green",
    "forward_color": "magenta",
    "backward_color": "green",
    "artifacts": {
        "app_log": "$app_log",
        "surfari_trace": "$surfari_trace",
        "render_trace": "$render_trace",
        "webtui_trace": "$webtui_trace",
        "baseline_screenshot": "$baseline_png",
        "click_screenshot": "$click_png",
        "forward_screenshot": "$forward_png",
        "backward_screenshot": "$backward_png",
        "baseline_counts": "$baseline_json",
        "click_counts": "$click_json",
        "forward_counts": "$forward_json",
        "backward_counts": "$backward_json",
        "forward_delta": "$forward_delta_json",
        "backward_delta": "$backward_delta_json",
    },
    "baseline_counts": load("$baseline_json"),
    "click_counts": load("$click_json"),
    "forward_counts": load("$forward_json"),
    "backward_counts": load("$backward_json"),
    "forward_delta": load("$forward_delta_json"),
    "backward_delta": load("$backward_delta_json"),
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
by_name = {scenario["name"]: scenario for scenario in scenarios}
scroll = by_name.get("scroll", {})
keyboard = by_name.get("keyboard", {})
scroll_pass = scroll.get("scroll_status") == "pass"
click_pass = (
    keyboard.get("click_status") == "pass"
    and keyboard.get("click_keyboard_route_status") == "pass"
)
page_down_pass = keyboard.get("page_down_status") == "pass"
page_up_pass = keyboard.get("page_up_status") == "pass"
cleanup_ok = (
    server_cleanup_status == "terminated"
    and all(scenario.get("cleanup_status") == "terminated" for scenario in scenarios)
)

if scroll_pass and click_pass and page_down_pass and page_up_pass and cleanup_ok:
    overall = "pass"
    classification = "surfari-pdf-input-navigation-proven"
elif scroll_pass or click_pass or page_down_pass or page_up_pass:
    overall = "partial"
    classification = "some-surfari-pdf-input-navigation-proven"
else:
    overall = "fail"
    classification = "surfari-pdf-input-navigation-not-proven"

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
    "pdf_url": next((scenario.get("url") for scenario in scenarios), None),
    "content_type_evidence": "REQUEST path=/navigation.pdf status=200 content_type=application/pdf",
    "scroll": scroll,
    "keyboard": keyboard,
    "scenario_statuses": {
        "scroll": "pass" if scroll_pass else "fail",
        "click": "pass" if click_pass else "fail",
        "page_down": "pass" if page_down_pass else "fail",
        "page_up": "pass" if page_up_pass else "fail",
    },
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
    "scroll": scroll_pass,
    "click": click_pass,
    "page_down": page_down_pass,
    "page_up": page_up_pass,
    "cleanup_ok": cleanup_ok,
}, indent=2, sort_keys=True))
if overall == "fail":
    raise SystemExit(1)
PY
}

if [ "${TERMSURF_SURFARI_CACONTEXT_LAYER+x}" = "x" ]; then
  fail "TERMSURF_SURFARI_CACONTEXT_LAYER must be unset for default input-navigation proof"
fi

require_executable "$APP_BIN"
require_executable "$WEB"
require_executable "$SURFARI"
require_path "$WEBKIT_DEBUG/WebKit.framework"
require_path "$WEBKIT_DYLIB"
require_path "$ROOT/scripts/ghostty-app/inject.swift"

log "run_id=$RUN_ID"
log "ghostboard_app_bin=$APP_BIN"
log "web=$WEB"
log "surfari=$SURFARI"
log "webkit_debug=$WEBKIT_DEBUG"
log "termsurf_surfari_cacontext_layer=${TERMSURF_SURFARI_CACONTEXT_LAYER-__unset__}"
log "summary=$SUMMARY"

write_fixtures
start_server
PORT="$(cat "$PORT_FILE")"
PDF_URL="http://127.0.0.1:${PORT}/navigation.pdf"

run_scenario \
  scroll \
  scroll \
  "$PDF_URL" \
  "/navigation.pdf"

run_scenario \
  keyboard \
  keyboard \
  "$PDF_URL" \
  "/navigation.pdf"

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
log "PASS: issue 834 experiment 42 Surfari PDF input navigation"
log "summary=$SUMMARY"
