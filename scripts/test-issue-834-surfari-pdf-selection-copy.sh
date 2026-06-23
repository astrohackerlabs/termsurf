#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$ROOT/logs/issue-834-exp44-surfari-pdf-selection-copy"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-issue834-exp44.XXXXXX")"
APP="${TERMSURF_GHOSTBOARD_APP:-$ROOT/ghostboard/macos/build/Debug/TermSurf.app}"
APP_BIN="$APP/Contents/MacOS/termsurf"
WEB="${TERMSURF_WEB:-$ROOT/target/debug/web}"
SURFARI="${TERMSURF_SURFARI:-$ROOT/target/debug/surfari}"
WEBKIT_DEBUG="${TERMSURF_WEBKIT_DEBUG:-$ROOT/webkit/src/WebKitBuild/Debug}"
WEBKIT_DYLIB="$ROOT/surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib"
SITE_DIR="$RUN_DIR/site"
PORT_FILE="$RUN_DIR/server-port.txt"
SERVER_SCRIPT="$RUN_DIR/server.py"
ORIGINAL_CLIPBOARD="$RUN_DIR/original-clipboard.txt"
HARNESS_LOG="$LOG_DIR/harness-$RUN_ID.log"
SERVER_LOG="$LOG_DIR/server-$RUN_ID.log"
APP_LOG="$LOG_DIR/app-$RUN_ID.log"
SURFARI_TRACE="$LOG_DIR/surfari-$RUN_ID.log"
RENDER_TRACE="$LOG_DIR/render-$RUN_ID.log"
WEBTUI_TRACE="$LOG_DIR/webtui-$RUN_ID.log"
BASELINE_PNG="$LOG_DIR/baseline-$RUN_ID.png"
SELECTED_PNG="$LOG_DIR/selected-$RUN_ID.png"
BASELINE_JSON="$LOG_DIR/baseline-$RUN_ID.json"
SELECTED_JSON="$LOG_DIR/selected-$RUN_ID.json"
SUMMARY="$LOG_DIR/surfari-pdf-selection-copy-summary.json"
FIXTURE_MODE="${TERMSURF_ISSUE834_PDF_FIXTURE_MODE:-single-marker}"
if [ "$FIXTURE_MODE" = "separated-tokens" ]; then
  EXPECTED_TOKENS="${TERMSURF_ISSUE834_PDF_EXPECTED_TOKENS:-LEFT834 MID834 RIGHT834}"
  MARKER="${TERMSURF_ISSUE834_PDF_MARKER:-$EXPECTED_TOKENS}"
  ACCEPTED_SUBSTRING="${TERMSURF_ISSUE834_PDF_ACCEPTED_SUBSTRING:-RIGHT834}"
  PDF_TEXT_OPERATORS_SUMMARY="${TERMSURF_ISSUE834_PDF_TEXT_OPERATORS:-BT /F1 24 Tf 72 620 Td (LEFT834) Tj ET | BT /F1 24 Tf 220 620 Td (MID834) Tj ET | BT /F1 24 Tf 360 620 Td (RIGHT834) Tj ET}"
  PDF_TEXT_BBOXES_JSON='[{"token":"LEFT834","x":72,"y":604,"width":96,"height":32},{"token":"MID834","x":220,"y":604,"width":84,"height":32},{"token":"RIGHT834","x":360,"y":604,"width":108,"height":32}]'
else
  EXPECTED_TOKENS="${TERMSURF_ISSUE834_PDF_EXPECTED_TOKENS:-TS834PDFCOPYQXJZ}"
  MARKER="${TERMSURF_ISSUE834_PDF_MARKER:-TS834PDFCOPYQXJZ}"
  ACCEPTED_SUBSTRING="${TERMSURF_ISSUE834_PDF_ACCEPTED_SUBSTRING:-$MARKER}"
  PDF_TEXT_OPERATORS_SUMMARY="BT /F1 24 Tf 72 620 Td ($MARKER) Tj ET"
  PDF_TEXT_BBOXES_JSON='[{"token":"TS834PDFCOPYQXJZ","x":72,"y":604,"width":280,"height":32}]'
fi
SENTINEL="ISSUE834_EXP44_CLIPBOARD_SENTINEL_$RUN_ID"
SERVER_PID=""
CURRENT_PID=""
SERVER_CLEANUP_STATUS="not-checked"
PROCESS_CLEANUP_STATUS="not-checked"
CLIPBOARD_RESTORE_STATUS="not-attempted"
FALLBACK_CLIPBOARD_CONTAINS_MARKER=false
FALLBACK_CLIPBOARD_AFTER_LENGTH=0
FALLBACK_CLIPBOARD_AFTER_SAMPLE=""
COPY_DELAY_AFTER_DRAG="${TERMSURF_ISSUE834_PDF_COPY_DELAY_AFTER_DRAG:-1}"
PDF_TEXT_EXTRACTION_STATUS="not-run"
PDF_TEXT_EXTRACTED=""

mkdir -p "$LOG_DIR" "$SITE_DIR"

log() {
  printf '%s\n' "$*" | tee -a "$HARNESS_LOG"
}

delay() {
  sleep "${1:-0.5}"
}

line_count() {
  local file="$1"
  if [ -r "$file" ]; then
    wc -l <"$file" | tr -d ' '
  else
    printf '0\n'
  fi
}

pasteboard_change_count() {
  swift - <<'SWIFT'
import AppKit
print(NSPasteboard.general.changeCount)
SWIFT
}

text_hash() {
  shasum -a 256 "$1" | awk '{print $1}'
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

restore_clipboard() {
  if [ -e "$ORIGINAL_CLIPBOARD" ]; then
    pbcopy <"$ORIGINAL_CLIPBOARD" || return 1
    CLIPBOARD_RESTORE_STATUS="restored"
  fi
}

cleanup() {
  cleanup_current_process || true
  cleanup_server || true
  restore_clipboard || CLIPBOARD_RESTORE_STATUS="restore-failed"
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

fail() {
  log "FAIL: $*"
  cleanup
  exit 1
}

require_executable() {
  [ -x "$1" ] || fail "missing executable: $1"
}

require_path() {
  [ -e "$1" ] || fail "missing path: $1"
}

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

optional_line_after() {
  local file="$1"
  local start_line="$2"
  local pattern="$3"
  tail -n +"$((start_line + 1))" "$file" | grep -E "$pattern" | tail -1 || true
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

write_pdf_fixture() {
  python3 - "$SITE_DIR/selectable.pdf" "$MARKER" "$FIXTURE_MODE" "$EXPECTED_TOKENS" <<'PY'
from pathlib import Path
import sys

out = Path(sys.argv[1])
marker = sys.argv[2]
fixture_mode = sys.argv[3]
expected_tokens = sys.argv[4].split()
objects = []

def esc(value):
    return value.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")

def add(body):
    objects.append(body)
    return len(objects)

stream_lines = [
    "0 1 1 rg",
    "0 0 612 792 re",
    "f",
    "0 0 0 rg",
]
if fixture_mode == "separated-tokens":
    positions = [(72, 620), (220, 620), (360, 620)]
    for token, (x, y) in zip(expected_tokens, positions):
        stream_lines.extend([
            "BT",
            "/F1 24 Tf",
            f"{x} {y} Td",
            f"({esc(token)}) Tj",
            "ET",
        ])
else:
    stream_lines.extend([
        "BT",
        "/F1 24 Tf",
        "72 620 Td",
        f"({esc(marker)}) Tj",
        "ET",
    ])
stream_lines.append("")
stream = "\n".join(stream_lines)

add("<< /Type /Catalog /Pages 2 0 R >>")
add("<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
add("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>")
add(f"<< /Length {len(stream.encode('ascii'))} >>\nstream\n{stream}endstream")
add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")

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

verify_pdf_fixture_text() {
  local result
  result="$(python3 - "$SITE_DIR/selectable.pdf" "$EXPECTED_TOKENS" <<'PY'
from pathlib import Path
import re
import sys

pdf = Path(sys.argv[1]).read_bytes().decode("latin1", errors="ignore")
expected = sys.argv[2].split()
tokens = []
for match in re.finditer(r"\(([^()]*)\)\s*Tj", pdf):
    token = match.group(1).replace(r"\(", "(").replace(r"\)", ")").replace(r"\\", "\\")
    tokens.append(token)
joined = " ".join(tokens)
missing = [token for token in expected if token not in joined]
status = "pass" if not missing else "fail"
print(f"{status}\t{joined}")
PY
)"
  PDF_TEXT_EXTRACTION_STATUS="$(printf '%s\n' "$result" | cut -f1)"
  PDF_TEXT_EXTRACTED="$(printf '%s\n' "$result" | cut -f2-)"
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
        if path == "/selectable.pdf":
            body = (site / "selectable.pdf").read_bytes()
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

drag_points_for_text() {
  local present_line="$1"
  python3 - "$present_line" <<'PY'
import re
import os
import sys

line = sys.argv[1]
frame = re.search(r"overlay_frame=\{\{([^,]+), ([^}]+)\}, \{([^,]+), ([^}]+)\}\}", line)
if not frame:
    raise SystemExit(1)
_, _, frame_w, frame_h = map(float, frame.groups())
# The generated PDF text is near the upper middle of the right-aligned PDF page.
# These points intentionally sweep across the visible text line rather than the
# whole page so a copy only passes if the PDF viewer selection path works.
start_x = frame_w * float(os.environ.get("TERMSURF_ISSUE834_PDF_COPY_DRAG_START_X_RATIO", "0.64"))
end_x = frame_w * float(os.environ.get("TERMSURF_ISSUE834_PDF_COPY_DRAG_END_X_RATIO", "0.99"))
y = frame_h * float(os.environ.get("TERMSURF_ISSUE834_PDF_COPY_DRAG_Y_RATIO", "0.43"))
print(f"{start_x:.1f} {y:.1f} {end_x:.1f} {y:.1f}")
PY
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
  python3 - "$window_screenshot" "$counts_json" "$presented_line" "$win_line" <<'PY'
from pathlib import Path
import json
import re
import struct
import sys
import zlib

png_path, out_path, presented_line, win_line = sys.argv[1:5]
threshold = 80
targets = {
    "cyan": (0, 255, 255),
    "black": (0, 0, 0),
}

def read_png(path):
    png = Path(path).read_bytes()
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
    return width, height, bpp, rows

frame = re.search(r"overlay_frame=\{\{([^,]+), ([^}]+)\}, \{([^,]+), ([^}]+)\}\}", presented_line)
root = re.search(r"root_frame=\{\{[^}]+\}, \{([^,]+), ([^}]+)\}\}", presented_line)
if not frame or not root:
    raise SystemExit("missing overlay/root frame")
frame_x, frame_y, frame_w, frame_h = map(float, frame.groups())
_, root_h = map(float, root.groups())
_, _, _, win_w, win_h = win_line.split("\t")
win_w = float(win_w)
win_h = float(win_h)
width, height, bpp, rows = read_png(png_path)
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
data = {
    "path": png_path,
    "status": "pass" if counts["cyan"] >= 5000 and counts["black"] >= 50 else "fail",
    "window_bounds_points": {"width": win_w, "height": win_h},
    "screenshot_pixels": {"width": width, "height": height},
    "overlay_crop": {
        "x": crop_x,
        "y": crop_y,
        "width": crop_w,
        "height": crop_h,
        "area": crop_w * crop_h,
    },
    "targets": counts,
    "threshold": threshold,
    "visible_window_bounded": True,
    "source_window_excluded": True,
}
Path(out_path).write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
if data["status"] != "pass":
    raise SystemExit(1)
PY
}

write_summary() {
  local status="$1"
  local classification="$2"
  python3 - "$SUMMARY" "$RUN_ID" "$status" "$classification" <<PY
from pathlib import Path
import json

def load(path):
    path = Path(path)
    return json.loads(path.read_text()) if path.exists() else None

data = {
    "overall_result": "$status",
    "classification": "$classification",
    "run_id": "$RUN_ID",
    "termsurf_surfari_cacontext_layer": "unset",
    "fixture": {
        "mode": "$FIXTURE_MODE",
        "marker": "$MARKER",
        "expected_tokens": "$EXPECTED_TOKENS".split(),
        "accepted_substring": "$ACCEPTED_SUBSTRING",
        "pdf_text_operator": "$PDF_TEXT_OPERATORS_SUMMARY",
        "pdf_text_bboxes": json.loads("""$PDF_TEXT_BBOXES_JSON"""),
        "text_extraction_status": "$PDF_TEXT_EXTRACTION_STATUS",
        "text_extracted": "$PDF_TEXT_EXTRACTED",
    },
    "binaries": {
        "ghostboard_app_bin": "$APP_BIN",
        "web": "$WEB",
        "surfari": "$SURFARI",
        "webkit_debug": "$WEBKIT_DEBUG",
    },
    "artifacts": {
        "harness_log": "$HARNESS_LOG",
        "server_log": "$SERVER_LOG",
        "app_log": "$APP_LOG",
        "surfari_trace": "$SURFARI_TRACE",
        "render_trace": "$RENDER_TRACE",
        "webtui_trace": "$WEBTUI_TRACE",
        "baseline_screenshot": "$BASELINE_PNG",
        "selected_screenshot": "$SELECTED_PNG",
        "baseline_counts": "$BASELINE_JSON",
        "selected_counts": "$SELECTED_JSON",
    },
    "evidence_lines": {
        "browser_ready": """${BROWSER_READY_LINE:-}""",
        "webtui_ready": """${WEBTUI_READY_LINE:-}""",
        "server_pdf_request": """${SERVER_LINE:-}""",
        "surfari_cacontext": """${CA_LINE:-}""",
        "render_proof": """${RENDER_LINE:-}""",
        "browse_mode": """${MODE_LINE:-}""",
        "focus": """${FOCUS_LINE:-}""",
        "drag_down": """${DRAG_DOWN_LINE:-}""",
        "drag_move": """${DRAG_MOVE_LINE:-}""",
        "drag_up": """${DRAG_UP_LINE:-}""",
        "app_drag_down": """${APP_DRAG_DOWN_LINE:-}""",
        "app_drag_move": """${APP_DRAG_MOVE_LINE:-}""",
        "app_drag_up": """${APP_DRAG_UP_LINE:-}""",
        "app_key_down": """${APP_KEY_LINE:-}""",
        "copy_current_url_absent": """${COPY_CURRENT_URL_ABSENT:-}""",
        "surfari_key": """${SURFARI_KEY_LINE:-}""",
        "fallback_cmd_a": """${FALLBACK_CMD_A_LINE:-}""",
        "fallback_cmd_c": """${FALLBACK_CMD_C_LINE:-}""",
    },
    "coordinate_mapping": {
        "overlay_presented_line": """${PRESENTED_LINE:-}""",
        "web_drag_start": {"x": ${DRAG_WEB_START_X:-0}, "y": ${DRAG_WEB_START_Y:-0}},
        "web_drag_end": {"x": ${DRAG_WEB_END_X:-0}, "y": ${DRAG_WEB_END_Y:-0}},
        "global_drag_start": {"x": ${DRAG_GLOBAL_START_X:-0}, "y": ${DRAG_GLOBAL_START_Y:-0}},
        "global_drag_end": {"x": ${DRAG_GLOBAL_END_X:-0}, "y": ${DRAG_GLOBAL_END_Y:-0}},
    },
    "timing": {
        "copy_delay_after_drag_seconds": float("${COPY_DELAY_AFTER_DRAG:-1}"),
    },
    "clipboard": {
        "original_length": ${ORIGINAL_CLIPBOARD_LENGTH:-0},
        "original_sha256": "${ORIGINAL_CLIPBOARD_SHA:-}",
        "sentinel": "$SENTINEL",
        "before_copy": "${CLIPBOARD_BEFORE_COPY:-}",
        "after_copy_length": ${CLIPBOARD_AFTER_COPY_LENGTH:-0},
        "after_copy_sha256": "${CLIPBOARD_AFTER_COPY_SHA:-}",
        "after_copy_sample": "${CLIPBOARD_AFTER_COPY_SAMPLE:-}",
        "contains_accepted_substring": "${CLIPBOARD_CONTAINS_MARKER:-false}" == "true",
        "fallback_select_all_contains_accepted_substring": "${FALLBACK_CLIPBOARD_CONTAINS_MARKER:-false}" == "true",
        "fallback_select_all_after_length": ${FALLBACK_CLIPBOARD_AFTER_LENGTH:-0},
        "fallback_select_all_after_sample": "${FALLBACK_CLIPBOARD_AFTER_SAMPLE:-}",
        "pasteboard_change_counts": {
            "initial": ${PB_CHANGE_INITIAL:-0},
            "after_sentinel": ${PB_CHANGE_AFTER_SENTINEL:-0},
            "after_copy": ${PB_CHANGE_AFTER_COPY:-0},
            "after_restore": ${PB_CHANGE_AFTER_RESTORE:-0}
        },
        "restore_status": "$CLIPBOARD_RESTORE_STATUS",
    },
    "baseline_counts": load("$BASELINE_JSON"),
    "selected_counts": load("$SELECTED_JSON"),
    "cleanup": {
        "process_status": "$PROCESS_CLEANUP_STATUS",
        "server_status": "$SERVER_CLEANUP_STATUS",
    },
}
Path("$SUMMARY").write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
print(json.dumps({"overall_result": "$status", "classification": "$classification"}, indent=2, sort_keys=True))
PY
}

if [ "${TERMSURF_SURFARI_CACONTEXT_LAYER+x}" = "x" ]; then
  fail "TERMSURF_SURFARI_CACONTEXT_LAYER must be unset for default copy proof"
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
log "fixture_mode=$FIXTURE_MODE"
log "marker=$MARKER"
log "expected_tokens=$EXPECTED_TOKENS"
log "accepted_substring=$ACCEPTED_SUBSTRING"
log "copy_delay_after_drag=$COPY_DELAY_AFTER_DRAG"

PB_CHANGE_INITIAL="$(pasteboard_change_count)"
pbpaste >"$ORIGINAL_CLIPBOARD" || true
ORIGINAL_CLIPBOARD_LENGTH="$(wc -c <"$ORIGINAL_CLIPBOARD" | tr -d ' ')"
ORIGINAL_CLIPBOARD_SHA="$(text_hash "$ORIGINAL_CLIPBOARD")"
printf '%s' "$SENTINEL" | pbcopy
PB_CHANGE_AFTER_SENTINEL="$(pasteboard_change_count)"
CLIPBOARD_BEFORE_COPY="$(pbpaste)"
[ "$CLIPBOARD_BEFORE_COPY" = "$SENTINEL" ] || fail "clipboard sentinel write failed"

write_pdf_fixture
verify_pdf_fixture_text
log "pdf_text_extraction_status=$PDF_TEXT_EXTRACTION_STATUS"
log "pdf_text_extracted=$PDF_TEXT_EXTRACTED"
start_server
PORT="$(cat "$PORT_FILE")"
PDF_URL="http://127.0.0.1:${PORT}/selectable.pdf"
CONFIG="$RUN_DIR/config"
COMMAND="$RUN_DIR/run.sh"
cat >"$COMMAND" <<EOF
#!/usr/bin/env bash
exec "$WEB" --browser surfari "$PDF_URL"
EOF
chmod +x "$COMMAND"
cat >"$CONFIG" <<EOF
window-save-state = never
initial-command = direct:$COMMAND
EOF

GHOSTTY_CONFIG_PATH="$CONFIG" \
GHOSTTY_LOG=stderr \
DYLD_FRAMEWORK_PATH="$WEBKIT_DEBUG" \
TERMSURF_SURFARI_PATH="$SURFARI" \
TERMSURF_GEOMETRY_TRACE=1 \
TERMSURF_GEOMETRY_SCENARIO="issue834-exp44-surfari-selection-copy" \
TERMSURF_WEBTUI_STATE_TRACE_FILE="$WEBTUI_TRACE" \
TERMSURF_SURFARI_RENDER_PROOF_TRACE_FILE="$RENDER_TRACE" \
TERMSURF_INPUT_TRACE=1 \
TERMSURF_PDF_INPUT_TRACE=1 \
TERMSURF_PDF_INPUT_TRACE_FILE="$SURFARI_TRACE" \
  "$APP_BIN" >"$APP_LOG" 2>&1 &
CURRENT_PID="$!"

wait_for_file_fixed "$APP_LOG" "browser=surfari url=$PDF_URL" "web requested Surfari overlay"
wait_for_file_pattern "$APP_LOG" "BrowserReady: pane_id=.* browser=surfari" "BrowserReady"
wait_for_file_pattern "$WEBTUI_TRACE" "event=render_state.*browser_ready=true.*browser_label=surfari" "WebTUI ready"
wait_for_file_fixed "$SURFARI_TRACE" "url=$PDF_URL" "Surfari trace recorded URL"
wait_for_file_pattern "$SURFARI_TRACE" "ca-context tab=.*context_id=[1-9][0-9]*" "nonzero CAContext"
wait_for_file_pattern "$RENDER_TRACE" "render-proof tab=.*url=${PDF_URL} .*status=pass" "Surfari internal render proof" 30
wait_for_file_fixed "$SERVER_LOG" "REQUEST path=/selectable.pdf status=200 content_type=application/pdf" "HTTP served PDF fixture"

BROWSER_READY_LINE="$(extract_first_match "$APP_LOG" "BrowserReady: pane_id=.* browser=surfari")"
WEBTUI_READY_LINE="$(extract_first_match "$WEBTUI_TRACE" "event=render_state.*browser_ready=true.*browser_label=surfari")"
RENDER_LINE="$(extract_first_match "$RENDER_TRACE" "render-proof tab=.*url=${PDF_URL} .*status=pass")"
SERVER_LINE="$(extract_first_match "$SERVER_LOG" "REQUEST path=/selectable.pdf status=200 content_type=application/pdf")"
PANE_ID="$(printf '%s\n' "$BROWSER_READY_LINE" | sed -E 's/.*pane_id=([^ ]+) tab_id=.*/\1/')"
BROWSER_TAB_ID="$(printf '%s\n' "$BROWSER_READY_LINE" | sed -E 's/.*tab_id=([0-9]+) socket=.*/\1/')"
CA_LINE="$(extract_first_match "$SURFARI_TRACE" "ca-context tab=.*context_id=[1-9][0-9]*")"
CONTEXT_ID="$(printf '%s\n' "$CA_LINE" | sed -E 's/.*context_id=([0-9]+).*/\1/')"
wait_for_file_pattern "$APP_LOG" "TermSurf geometry layer=appkit event=presented_pixels .*pane_id:${PANE_ID} .*context_id=${CONTEXT_ID} .*visible=true" "AppKit presented pixels"
PRESENTED_LINE="$(extract_first_match "$APP_LOG" "TermSurf geometry layer=appkit event=presented .*pane_id:${PANE_ID} .*context_id=${CONTEXT_ID}")"
WINDOW_ID="$(extract_window_id "$PRESENTED_LINE")"
WIN_LINE="$(exact_window_bounds "$WINDOW_ID")" || fail "failed to resolve window bounds"

activate_pid "$CURRENT_PID" "pre-browse Ghostboard activation"
MODE_START="$(line_count "$APP_LOG")"
FOCUS_START="$(line_count "$SURFARI_TRACE")"
swift "$ROOT/scripts/ghostty-app/inject.swift" key 36 >>"$HARNESS_LOG" 2>&1
MODE_LINE="$(wait_for_line_after "$APP_LOG" "$MODE_START" "ModeChanged: pane_id=${PANE_ID} browsing=true" "entered Browse mode")"
FOCUS_LINE="$(wait_for_line_after "$SURFARI_TRACE" "$FOCUS_START" "focus-changed tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_set_focus focused=true" "Surfari focused")"

delay 2
capture_overlay_counts "$WINDOW_ID" "$PRESENTED_LINE" "$BASELINE_PNG" "$BASELINE_JSON"
read -r DRAG_WEB_START_X DRAG_WEB_START_Y DRAG_WEB_END_X DRAG_WEB_END_Y <<<"$(drag_points_for_text "$PRESENTED_LINE")"
read -r DRAG_GLOBAL_START_X DRAG_GLOBAL_START_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$PRESENTED_LINE" "$DRAG_WEB_START_X" "$DRAG_WEB_START_Y")"
read -r DRAG_GLOBAL_END_X DRAG_GLOBAL_END_Y <<<"$(global_point_for_web_point "$WIN_LINE" "$PRESENTED_LINE" "$DRAG_WEB_END_X" "$DRAG_WEB_END_Y")"
log "pdf_text_operator=$PDF_TEXT_OPERATORS_SUMMARY"
log "pdf_text_bboxes=$PDF_TEXT_BBOXES_JSON"
log "web_drag=${DRAG_WEB_START_X},${DRAG_WEB_START_Y}-${DRAG_WEB_END_X},${DRAG_WEB_END_Y}"
log "global_drag=${DRAG_GLOBAL_START_X},${DRAG_GLOBAL_START_Y}-${DRAG_GLOBAL_END_X},${DRAG_GLOBAL_END_Y}"

DRAG_SURFARI_START="$(line_count "$SURFARI_TRACE")"
DRAG_APP_START="$(line_count "$APP_LOG")"
swift "$ROOT/scripts/ghostty-app/inject.swift" drag "$DRAG_GLOBAL_START_X" "$DRAG_GLOBAL_START_Y" "$DRAG_GLOBAL_END_X" "$DRAG_GLOBAL_END_Y" >>"$HARNESS_LOG" 2>&1
DRAG_DOWN_LINE="$(wait_for_line_after "$SURFARI_TRACE" "$DRAG_SURFARI_START" "mouse-event tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_forward_mouse_event type=down button=left" "Surfari received drag down")"
DRAG_MOVE_LINE="$(wait_for_line_after "$SURFARI_TRACE" "$DRAG_SURFARI_START" "mouse-move tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_forward_mouse_move" "Surfari received drag move")"
DRAG_UP_LINE="$(wait_for_line_after "$SURFARI_TRACE" "$DRAG_SURFARI_START" "mouse-event tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_forward_mouse_event type=up button=left" "Surfari received drag up")"
APP_DRAG_DOWN_LINE="$(wait_for_line_after "$APP_LOG" "$DRAG_APP_START" "TermSurf geometry layer=appkit event=mouse_forwarded .*pane_id:${PANE_ID} .*note=kind=event .*type=down .*terminal_fallback=false" "Ghostboard forwarded drag down")"
APP_DRAG_MOVE_LINE="$(wait_for_line_after "$APP_LOG" "$DRAG_APP_START" "TermSurf geometry layer=appkit event=mouse_forwarded .*pane_id:${PANE_ID} .*note=kind=move .*terminal_fallback=false" "Ghostboard forwarded drag move")"
APP_DRAG_UP_LINE="$(wait_for_line_after "$APP_LOG" "$DRAG_APP_START" "TermSurf geometry layer=appkit event=mouse_forwarded .*pane_id:${PANE_ID} .*note=kind=event .*type=up .*terminal_fallback=false" "Ghostboard forwarded drag up")"
delay "$COPY_DELAY_AFTER_DRAG"
capture_overlay_counts "$WINDOW_ID" "$PRESENTED_LINE" "$SELECTED_PNG" "$SELECTED_JSON"

COPY_APP_START="$(line_count "$APP_LOG")"
COPY_SURFARI_START="$(line_count "$SURFARI_TRACE")"
swift "$ROOT/scripts/ghostty-app/inject.swift" key 8 command >>"$HARNESS_LOG" 2>&1
APP_KEY_LINE="$(wait_for_line_after "$APP_LOG" "$COPY_APP_START" "TermSurf geometry layer=appkit event=key_down .*pane_id:${PANE_ID} .*note=key_code=8 .*focused=true" "Ghostboard observed Browse-mode Cmd+C")"
SURFARI_KEY_LINE="$(wait_for_line_after "$SURFARI_TRACE" "$COPY_SURFARI_START" "key-event tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_forward_key_event type=down windows_key_code=67" "Surfari received Cmd+C")"
COPY_CURRENT_URL_LINE="$(optional_line_after "$APP_LOG" "$COPY_APP_START" "copy_current_url|CopyCurrentUrl")"
if [ -n "$COPY_CURRENT_URL_LINE" ]; then
  fail "Cmd+C was handled by copy-current-url path: $COPY_CURRENT_URL_LINE"
fi
COPY_CURRENT_URL_ABSENT="pass"

for _ in $(seq 1 30); do
  CLIPBOARD_AFTER_COPY="$(pbpaste)"
  if [ "$CLIPBOARD_AFTER_COPY" != "$SENTINEL" ]; then
    break
  fi
  delay 0.25
done
PB_CHANGE_AFTER_COPY="$(pasteboard_change_count)"
CLIPBOARD_AFTER_COPY_FILE="$RUN_DIR/after-copy.txt"
printf '%s' "$CLIPBOARD_AFTER_COPY" >"$CLIPBOARD_AFTER_COPY_FILE"
CLIPBOARD_AFTER_COPY_LENGTH="$(wc -c <"$CLIPBOARD_AFTER_COPY_FILE" | tr -d ' ')"
CLIPBOARD_AFTER_COPY_SHA="$(text_hash "$CLIPBOARD_AFTER_COPY_FILE")"
CLIPBOARD_AFTER_COPY_SAMPLE="$(printf '%s' "$CLIPBOARD_AFTER_COPY" | head -c 120 | tr '\n' ' ')"
if printf '%s' "$CLIPBOARD_AFTER_COPY" | grep -F "$ACCEPTED_SUBSTRING" >/dev/null 2>&1; then
  CLIPBOARD_CONTAINS_MARKER=true
else
  CLIPBOARD_CONTAINS_MARKER=false
fi

if [ "$CLIPBOARD_CONTAINS_MARKER" != true ]; then
  FALLBACK_START="$(line_count "$SURFARI_TRACE")"
  log "fallback_select_all_copy=cmd+a,cmd+c"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 0 command >>"$HARNESS_LOG" 2>&1
  FALLBACK_CMD_A_LINE="$(optional_line_after "$SURFARI_TRACE" "$FALLBACK_START" "key-event tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_forward_key_event .*windows_key_code=65")"
  swift "$ROOT/scripts/ghostty-app/inject.swift" key 8 command >>"$HARNESS_LOG" 2>&1
  FALLBACK_CMD_C_LINE="$(optional_line_after "$SURFARI_TRACE" "$FALLBACK_START" "key-event tab=${BROWSER_TAB_ID} pane=${PANE_ID} ffi=ts_forward_key_event .*windows_key_code=67")"
  delay 1
  FALLBACK_CLIPBOARD_AFTER="$(pbpaste)"
  FALLBACK_CLIPBOARD_AFTER_LENGTH="$(printf '%s' "$FALLBACK_CLIPBOARD_AFTER" | wc -c | tr -d ' ')"
  FALLBACK_CLIPBOARD_AFTER_SAMPLE="$(printf '%s' "$FALLBACK_CLIPBOARD_AFTER" | head -c 120 | tr '\n' ' ')"
  if printf '%s' "$FALLBACK_CLIPBOARD_AFTER" | grep -F "$ACCEPTED_SUBSTRING" >/dev/null 2>&1; then
    FALLBACK_CLIPBOARD_CONTAINS_MARKER=true
  fi
fi

restore_clipboard || CLIPBOARD_RESTORE_STATUS="restore-failed"
PB_CHANGE_AFTER_RESTORE="$(pasteboard_change_count)"

cleanup_current_process || true
PROCESS_CLEANUP_STATUS="terminated"
for _ in $(seq 1 20); do
  if [ -z "$CURRENT_PID" ] || ! kill -0 "$CURRENT_PID" >/dev/null 2>&1; then
    PROCESS_CLEANUP_STATUS="terminated"
    break
  fi
  delay 0.1
done
if [ -n "$CURRENT_PID" ] && kill -0 "$CURRENT_PID" >/dev/null 2>&1; then
  PROCESS_CLEANUP_STATUS="still-running"
fi
cleanup_server
SERVER_CLEANUP_STATUS="terminated"
for _ in $(seq 1 20); do
  if [ -z "$SERVER_PID" ] || ! kill -0 "$SERVER_PID" >/dev/null 2>&1; then
    SERVER_CLEANUP_STATUS="terminated"
    break
  fi
  delay 0.1
done
if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" >/dev/null 2>&1; then
  SERVER_CLEANUP_STATUS="still-running"
fi

if [ "$CLIPBOARD_CONTAINS_MARKER" = true ] \
  && [ "$CLIPBOARD_RESTORE_STATUS" = "restored" ] \
  && [ "$PROCESS_CLEANUP_STATUS" = "terminated" ] \
  && [ "$SERVER_CLEANUP_STATUS" = "terminated" ]; then
  write_summary pass surfari-pdf-selection-copy-proven
  log "PASS: issue 834 experiment 44 Surfari PDF selection/copy"
else
  write_summary partial surfari-pdf-selection-copy-partial
  log "WARN: Surfari PDF selection/copy was not fully proven"
fi

log "summary=$SUMMARY"
