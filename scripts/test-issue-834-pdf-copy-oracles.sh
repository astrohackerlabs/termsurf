#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$ROOT/logs/issue-834-exp46-pdf-copy-oracles"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-issue834-exp46.XXXXXX")"
SITE_DIR="$RUN_DIR/site"
SWIFT_APP="$RUN_DIR/ProbeApp.swift"
PROBE_APP="$RUN_DIR/probe-app"
PDF_PATH="$SITE_DIR/selectable.pdf"
HTML_PATH="$SITE_DIR/selectable.html"
ORIGINAL_CLIPBOARD="$RUN_DIR/original-clipboard.txt"
SUMMARY="$LOG_DIR/pdf-copy-oracles-summary.json"
HARNESS_LOG="$LOG_DIR/harness-$RUN_ID.log"
MENU_SCRIPT="$RUN_DIR/click-copy-menu.scpt"
MARKER="TS834PDFCOPYQXJZ"
ACCEPTED_SUBSTRING="$MARKER"
ORIGINAL_RESTORE_STATUS="not-attempted"

mkdir -p "$LOG_DIR" "$SITE_DIR"

log() {
  printf '%s\n' "$*" | tee -a "$HARNESS_LOG"
}

delay() {
  sleep "${1:-0.5}"
}

pasteboard_change_count() {
  swift - <<'SWIFT'
import AppKit
print(NSPasteboard.general.changeCount)
SWIFT
}

hash_file() {
  shasum -a 256 "$1" | awk '{print $1}'
}

restore_original_clipboard() {
  if [ -e "$ORIGINAL_CLIPBOARD" ]; then
    pbcopy <"$ORIGINAL_CLIPBOARD" || return 1
    ORIGINAL_RESTORE_STATUS="restored"
  fi
}

cleanup() {
  restore_original_clipboard || ORIGINAL_RESTORE_STATUS="restore-failed"
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

fail() {
  log "FAIL: $*"
  cleanup
  exit 1
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

point_from_window_ratio() {
  local win_line="$1"
  local rx="$2"
  local ry="$3"
  python3 - "$win_line" "$rx" "$ry" <<'PY'
import sys

win_line, rx, ry = sys.argv[1:4]
_, x, y, w, h = win_line.split("\t")
x, y, w, h = map(float, (x, y, w, h))
rx = float(rx)
ry = float(ry)
print(f"{int(x + w * rx + 0.5)} {int(y + h * ry + 0.5)}")
PY
}

activate_pid() {
  local pid="$1"
  swift - "$pid" <<'SWIFT'
import AppKit
import Foundation

let pid = pid_t(Int32(CommandLine.arguments[1])!)
guard let app = NSRunningApplication(processIdentifier: pid) else {
    exit(1)
}
app.activate(options: [.activateAllWindows, .activateIgnoringOtherApps])
SWIFT
}

slow_drag() {
  local sx="$1"
  local sy="$2"
  local ex="$3"
  local ey="$4"
  swift - "$sx" "$sy" "$ex" "$ey" <<'SWIFT'
import CoreGraphics
import Foundation

func point(_ x: String, _ y: String) -> CGPoint {
    CGPoint(x: Double(x) ?? 0, y: Double(y) ?? 0)
}

func event(_ type: CGEventType, _ point: CGPoint) -> CGEvent? {
    CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: point, mouseButton: .left)
}

let start = point(CommandLine.arguments[1], CommandLine.arguments[2])
let end = point(CommandLine.arguments[3], CommandLine.arguments[4])
event(.mouseMoved, start)?.post(tap: .cghidEventTap)
usleep(50_000)
event(.leftMouseDown, start)?.post(tap: .cghidEventTap)
usleep(100_000)
for step in 1...40 {
    let t = Double(step) / 40.0
    let current = CGPoint(
        x: start.x + (end.x - start.x) * t,
        y: start.y + (end.y - start.y) * t
    )
    event(.leftMouseDragged, current)?.post(tap: .cghidEventTap)
    usleep(15_000)
}
event(.leftMouseUp, end)?.post(tap: .cghidEventTap)
SWIFT
}

write_pdf_fixture() {
  python3 - "$PDF_PATH" "$MARKER" <<'PY'
from pathlib import Path
import sys

out = Path(sys.argv[1])
marker = sys.argv[2]
objects = []

def esc(value):
    return value.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")

def add(body):
    objects.append(body)

stream = "\n".join([
    "0 1 1 rg",
    "0 0 612 792 re",
    "f",
    "0 0 0 rg",
    "BT",
    "/F1 24 Tf",
    "72 620 Td",
    f"({esc(marker)}) Tj",
    "ET",
    "",
])

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

write_html_fixture() {
  cat >"$HTML_PATH" <<EOF
<!doctype html>
<meta charset="utf-8">
<style>
  body { margin: 0; padding: 120px 0 0 0; font: 36px Helvetica, sans-serif; background: #1a1a1a; color: white; }
</style>
<main>$MARKER</main>
EOF
}

write_swift_app() {
  cat >"$SWIFT_APP" <<'SWIFT'
import AppKit
import PDFKit
import WebKit

let mode = CommandLine.arguments[1]
let path = CommandLine.arguments[2]
let marker = CommandLine.arguments[3]
let triggerPath = CommandLine.arguments[4]
let ackPath = CommandLine.arguments[5]

final class Delegate: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    var timer: Timer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        installMenus()

        let frame = NSRect(x: 120, y: 120, width: 900, height: 650)
        window = NSWindow(
            contentRect: frame,
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Issue 834 Exp46 \(mode)"

        if mode == "text" {
            let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: 900, height: 650))
            scroll.hasVerticalScroller = true
            let text = NSTextView(frame: scroll.bounds)
            text.isEditable = false
            text.isSelectable = true
            text.drawsBackground = true
            text.backgroundColor = NSColor(calibratedWhite: 0.10, alpha: 1.0)
            text.textColor = .white
            text.string = "\n\n\(marker)\n"
            text.font = NSFont.systemFont(ofSize: 36)
            scroll.documentView = text
            window.contentView = scroll
            DispatchQueue.main.async {
                self.window.makeFirstResponder(text)
            }
        } else if mode == "pdfkit" {
            let view = PDFView(frame: NSRect(x: 0, y: 0, width: 900, height: 650))
            view.autoScales = true
            view.displayMode = .singlePage
            view.document = PDFDocument(url: URL(fileURLWithPath: path))
            window.contentView = view
            DispatchQueue.main.async {
                self.window.makeFirstResponder(view)
            }
        } else if mode == "wkpdf" {
            let view = WKWebView(frame: NSRect(x: 0, y: 0, width: 900, height: 650))
            view.loadFileURL(URL(fileURLWithPath: path), allowingReadAccessTo: URL(fileURLWithPath: path).deletingLastPathComponent())
            window.contentView = view
            DispatchQueue.main.async {
                self.window.makeFirstResponder(view)
            }
        } else {
            fputs("unknown mode\n", stderr)
            exit(2)
        }

        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { _ in
            if FileManager.default.fileExists(atPath: triggerPath) {
                try? FileManager.default.removeItem(atPath: triggerPath)
                let ok = NSApp.sendAction(#selector(NSText.copy(_:)), to: nil, from: nil)
                try? "copyAction=\(ok)\n".write(toFile: ackPath, atomically: true, encoding: .utf8)
            }
        }

        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) {
            print("READY mode=\(mode) window_id=\(self.window.windowNumber)")
            fflush(stdout)
        }
    }

    func installMenus() {
        let mainMenu = NSMenu()
        let appItem = NSMenuItem()
        mainMenu.addItem(appItem)
        let appMenu = NSMenu()
        appMenu.addItem(NSMenuItem(title: "Quit", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        appItem.submenu = appMenu

        let editItem = NSMenuItem()
        mainMenu.addItem(editItem)
        let editMenu = NSMenu(title: "Edit")
        let copyItem = NSMenuItem(title: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        copyItem.keyEquivalentModifierMask = [.command]
        editMenu.addItem(copyItem)
        editItem.submenu = editMenu
        NSApp.mainMenu = mainMenu
    }
}

let app = NSApplication.shared
let delegate = Delegate()
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
SWIFT
  swiftc "$SWIFT_APP" -o "$PROBE_APP"
}

copy_with_route() {
  local route="$1"
  local pid="$2"
  local trigger="$3"
  local ack="$4"
  case "$route" in
    cg-event)
      swift "$ROOT/scripts/ghostty-app/inject.swift" key 8 command >>"$HARNESS_LOG" 2>&1
      ;;
    system-events)
      osascript -e 'tell application "System Events" to keystroke "c" using command down' >>"$HARNESS_LOG" 2>&1
      ;;
    menu)
      osascript "$MENU_SCRIPT" "$pid" >>"$HARNESS_LOG" 2>&1
      ;;
    inprocess)
      : >"$trigger"
      for _ in $(seq 1 30); do
        [ -f "$ack" ] && break
        delay 0.1
      done
      ;;
    *)
      fail "unknown copy route: $route"
      ;;
  esac
}

write_menu_script() {
  cat >"$MENU_SCRIPT" <<'APPLESCRIPT'
on run argv
  set targetPid to item 1 of argv as integer
  tell application "System Events"
    set targetProcess to first process whose unix id is targetPid
    set frontmost of targetProcess to true
    delay 0.2
    tell targetProcess
      click menu item "Copy" of menu "Edit" of menu bar 1
    end tell
  end tell
end run
APPLESCRIPT
}

remove_menu_script() {
  rm -f "$MENU_SCRIPT"
}

run_attempt() {
  local control="$1"
  local mode="$2"
  local path="$3"
  local route="$4"
  local start_rx="$5"
  local start_ry="$6"
  local end_rx="$7"
  local end_ry="$8"
  local name="${control}-${route}"
  local trigger="$RUN_DIR/$name.trigger"
  local ack="$RUN_DIR/$name.ack"
  local sentinel="ISSUE834_EXP46_${name}_SENTINEL_${RUN_ID}"
  local stdout_log="$LOG_DIR/$name-stdout-$RUN_ID.log"
  local stderr_log="$LOG_DIR/$name-stderr-$RUN_ID.log"
  local selected_png="$LOG_DIR/$name-selected-$RUN_ID.png"
  local after_png="$LOG_DIR/$name-after-copy-$RUN_ID.png"
  local out_json="$LOG_DIR/$name-$RUN_ID.json"
  local pid ready window_id win_line sx sy ex ey before_change after_sentinel_change after_copy_change after_restore_change
  local before_text after_text after_file after_len after_hash sample contains status ack_text

  rm -f "$trigger" "$ack"
  "$PROBE_APP" "$mode" "$path" "$MARKER" "$trigger" "$ack" >"$stdout_log" 2>"$stderr_log" &
  pid="$!"
  for _ in $(seq 1 80); do
    ready="$(grep -E "^READY mode=${mode} window_id=[0-9]+" "$stdout_log" | tail -1 || true)"
    [ -n "$ready" ] && break
    delay 0.25
  done
  [ -n "${ready:-}" ] || fail "$name did not become ready"
  window_id="$(printf '%s\n' "$ready" | sed -E 's/.*window_id=([0-9]+).*/\1/')"
  win_line="$(exact_window_bounds "$window_id")" || fail "$name window bounds missing"
  read -r sx sy <<<"$(point_from_window_ratio "$win_line" "$start_rx" "$start_ry")"
  read -r ex ey <<<"$(point_from_window_ratio "$win_line" "$end_rx" "$end_ry")"

  activate_pid "$pid" || true
  delay 0.5
  before_change="$(pasteboard_change_count)"
  printf '%s' "$sentinel" | pbcopy
  after_sentinel_change="$(pasteboard_change_count)"
  before_text="$(pbpaste)"
  [ "$before_text" = "$sentinel" ] || fail "$name sentinel write failed"

  swift "$ROOT/scripts/ghostty-app/inject.swift" click "$sx" "$sy" >>"$HARNESS_LOG" 2>&1
  delay 0.5
  slow_drag "$sx" "$sy" "$ex" "$ey" >>"$HARNESS_LOG" 2>&1
  delay 0.75
  screencapture -x -o -l"$window_id" "$selected_png" || true

  activate_pid "$pid" || true
  delay 0.5
  copy_with_route "$route" "$pid" "$trigger" "$ack"
  delay 1
  after_text="$(pbpaste)"
  after_copy_change="$(pasteboard_change_count)"
  after_file="$RUN_DIR/$name-after.txt"
  printf '%s' "$after_text" >"$after_file"
  after_len="$(wc -c <"$after_file" | tr -d ' ')"
  after_hash="$(hash_file "$after_file")"
  sample="$(printf '%s' "$after_text" | head -c 120 | tr '\n' ' ')"
  if printf '%s' "$after_text" | grep -F "$ACCEPTED_SUBSTRING" >/dev/null 2>&1; then
    contains=true
    status=pass
  else
    contains=false
    status=fail
  fi
  ack_text="$(cat "$ack" 2>/dev/null || true)"
  restore_original_clipboard || ORIGINAL_RESTORE_STATUS="restore-failed"
  after_restore_change="$(pasteboard_change_count)"
  screencapture -x -o -l"$window_id" "$after_png" || true
  kill "$pid" >/dev/null 2>&1 || true
  delay 0.25
  kill -9 "$pid" >/dev/null 2>&1 || true

  python3 - "$out_json" <<PY
import json
from pathlib import Path

data = {
    "name": "$name",
    "control": "$control",
    "mode": "$mode",
    "route": "$route",
    "status": "$status",
    "contains_marker": "$contains" == "true",
    "window_id": "$window_id",
    "window_bounds": "$win_line",
    "drag": {
        "start_ratio": [$start_rx, $start_ry],
        "end_ratio": [$end_rx, $end_ry],
        "start_global": [int("$sx"), int("$sy")],
        "end_global": [int("$ex"), int("$ey")],
    },
    "clipboard": {
        "sentinel": "$sentinel",
        "after_length": int("$after_len"),
        "after_sha256": "$after_hash",
        "after_sample": "$sample",
        "change_counts": {
            "before": int("$before_change"),
            "after_sentinel": int("$after_sentinel_change"),
            "after_copy": int("$after_copy_change"),
            "after_restore": int("$after_restore_change"),
        },
    },
    "artifacts": {
        "stdout": "$stdout_log",
        "stderr": "$stderr_log",
        "selected_screenshot": "$selected_png",
        "after_copy_screenshot": "$after_png",
    },
    "inprocess_ack": "$ack_text",
}
Path("$out_json").write_text(json.dumps(data, indent=2, sort_keys=True) + "\\n")
PY
}

classification_from_results() {
  python3 - "$SUMMARY" "$RUN_ID" "$ORIGINAL_RESTORE_STATUS" "$LOG_DIR" <<'PY'
import json
from pathlib import Path
import sys

summary = Path(sys.argv[1])
run_id = sys.argv[2]
restore_status = sys.argv[3]
log_dir = Path(sys.argv[4])
controls = ["text", "pdfkit", "wkpdf"]
external_routes = ["cg-event", "system-events", "menu"]
all_routes = external_routes + ["inprocess"]
results = {}
missing = []
for control in controls:
    results[control] = {}
    for route in all_routes:
        path = log_dir / f"{control}-{route}-{run_id}.json"
        if path.exists():
            results[control][route] = json.loads(path.read_text())
        else:
            results[control][route] = {"status": "missing", "contains_marker": False}
            missing.append(f"{control}-{route}")

def passed(control, route):
    return results[control][route].get("status") == "pass"

trusted_external = [route for route in external_routes if all(passed(control, route) for control in controls)]
inprocess_all = all(passed(control, "inprocess") for control in controls)
text_external = any(passed("text", route) for route in external_routes)
pdfkit_any = any(passed("pdfkit", route) for route in all_routes)
wkpdf_any = any(passed("wkpdf", route) for route in all_routes)
text_any = any(passed("text", route) for route in all_routes)

if trusted_external:
    classification = "copy-oracle-found"
    result = "pass"
elif inprocess_all:
    classification = "appkit-only-copy-oracle"
    result = "partial"
elif text_external and not pdfkit_any:
    classification = "pdfkit-copy-gap"
    result = "pass"
elif text_any and pdfkit_any and not wkpdf_any:
    classification = "webkit-pdf-copy-gap"
    result = "pass"
elif not text_external:
    classification = "automation-copy-gap"
    result = "pass"
else:
    classification = "mixed-copy-oracle"
    result = "partial"

if restore_status != "restored":
    result = "fail"
if missing:
    result = "partial" if result != "fail" else "fail"

data = {
    "overall_result": result,
    "classification": classification,
    "trusted_external_routes": trusted_external,
    "run_id": run_id,
    "marker": "TS834PDFCOPYQXJZ",
    "accepted_substring": "TS834PDFCOPYQXJZ",
    "clipboard_restore_status": restore_status,
    "missing_probes": missing,
    "controls": results,
}
summary.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
print(json.dumps({
    "overall_result": result,
    "classification": classification,
    "trusted_external_routes": trusted_external,
}, indent=2, sort_keys=True))
if result == "fail":
    sys.exit(1)
PY
}

run_surfari_probe_if_oracle() {
  local should_run
  should_run="$(python3 - "$SUMMARY" <<'PY'
import json
import sys
from pathlib import Path
data = json.loads(Path(sys.argv[1]).read_text())
print("yes" if data.get("trusted_external_routes") else "no")
PY
)"
  [ "$should_run" = "yes" ] || return 0

  local surfari_log_dir="$ROOT/logs/issue-834-exp44-surfari-pdf-selection-copy"
  local surfari_summary="$LOG_DIR/surfari-summary-$RUN_ID.json"
  rm -rf "$surfari_log_dir"
  if env -u TERMSURF_SURFARI_CACONTEXT_LAYER "$ROOT/scripts/test-issue-834-surfari-pdf-selection-copy.sh" >>"$HARNESS_LOG" 2>&1; then
    cp "$surfari_log_dir/surfari-pdf-selection-copy-summary.json" "$surfari_summary"
  else
    if [ -f "$surfari_log_dir/surfari-pdf-selection-copy-summary.json" ]; then
      cp "$surfari_log_dir/surfari-pdf-selection-copy-summary.json" "$surfari_summary"
    else
      python3 - "$surfari_summary" <<'PY'
import json
import sys
from pathlib import Path
Path(sys.argv[1]).write_text(json.dumps({"overall_result": "missing"}, indent=2) + "\n")
PY
    fi
  fi

  python3 - "$SUMMARY" "$surfari_summary" <<'PY'
import json
import sys
from pathlib import Path

summary_path = Path(sys.argv[1])
surfari_path = Path(sys.argv[2])
summary = json.loads(summary_path.read_text())
surfari = json.loads(surfari_path.read_text())
summary["surfari"] = surfari
summary["surfari_summary_path"] = str(surfari_path)
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(json.dumps({
    "surfari_overall_result": surfari.get("overall_result"),
    "surfari_classification": surfari.get("classification"),
}, indent=2, sort_keys=True))
PY
}

log "run_id=$RUN_ID"
log "marker=$MARKER"
log "accepted_substring=$ACCEPTED_SUBSTRING"
pbpaste >"$ORIGINAL_CLIPBOARD" || true
ORIGINAL_LENGTH="$(wc -c <"$ORIGINAL_CLIPBOARD" | tr -d ' ')"
ORIGINAL_SHA="$(hash_file "$ORIGINAL_CLIPBOARD")"
log "original_clipboard_length=$ORIGINAL_LENGTH"
log "original_clipboard_sha256=$ORIGINAL_SHA"

write_pdf_fixture
write_html_fixture
write_swift_app
write_menu_script
trap 'remove_menu_script; cleanup' EXIT

for route in cg-event system-events menu inprocess; do
  run_attempt text text "$HTML_PATH" "$route" 0.01 0.21 0.43 0.21
  run_attempt pdfkit pdfkit "$PDF_PATH" "$route" 0.28 0.25 0.55 0.25
  run_attempt wkpdf wkpdf "$PDF_PATH" "$route" 0.23 0.25 0.60 0.25
done

restore_original_clipboard || ORIGINAL_RESTORE_STATUS="restore-failed"
classification_from_results
run_surfari_probe_if_oracle
log "summary=$SUMMARY"
