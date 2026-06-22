#!/usr/bin/env python3
"""Safely probe Roamium native PDF print dialog behavior."""

from __future__ import annotations

import argparse
import http.server
import json
import os
import pathlib
import re
import signal
import socket
import socketserver
import struct
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass
from typing import Any

from termsurf_pdf_protocol_harness import (
    bool_field,
    create_tab_payload,
    double_field,
    inner_payload,
    send_message,
    tab_ready_id,
    varint_field,
)


ROOT = pathlib.Path(__file__).resolve().parents[1]
ROAMIUM = ROOT / "chromium/src/out/Default/roamium"
BITCOIN_PDF = ROOT / "test-html/public/bitcoin.pdf"
SAVE_PRINT_TITLE_LOCAL_PROBE = ROOT / "scripts/probe-pdf-save-print-title-local.mjs"
CGEVENT_INJECT = ROOT / "scripts/ghostty-app/inject.swift"
DEVTOOLS_RE = re.compile(r"DevTools listening on ws://127\.0\.0\.1:(\d+)/")
PREFLIGHT_TITLE = "TermSurf Native Print Safety Preflight"


def resize_payload(tab_id: int, width: int, height: int) -> bytes:
    return (
        varint_field(1, tab_id)
        + varint_field(2, width)
        + varint_field(3, height)
        + double_field(4, 0.0)
        + double_field(5, 0.0)
        + double_field(6, float(width))
        + double_field(7, float(height))
        + double_field(8, 1.0)
    )


def focus_payload(tab_id: int, focused: bool) -> bytes:
    return varint_field(1, tab_id) + bool_field(2, focused)


class ReusableTcpServer(socketserver.TCPServer):
    allow_reuse_address = True


class PdfHandler(http.server.BaseHTTPRequestHandler):
    log_dir: pathlib.Path

    def log_message(self, fmt: str, *args: object) -> None:
        with (self.log_dir / "http.log").open("a", encoding="utf-8") as log:
            log.write((fmt % args) + "\n")

    def do_GET(self) -> None:
        request_path = self.path.split("?", 1)[0]
        if request_path == "/bitcoin.pdf":
            data = BITCOIN_PDF.read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", "application/pdf")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        if request_path == "/embedded-pdf.html":
            data = (
                "<!doctype html><meta charset='utf-8'>"
                "<title>Embedded PDF Host</title>"
                "<style>html,body{margin:0;width:100%;height:100%;}"
                "embed{width:100%;height:100%;display:block;}</style>"
                "<embed src='/bitcoin.pdf' type='application/pdf'>"
            ).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        self.send_response(404)
        self.end_headers()


@dataclass
class HarnessState:
    server_register_received: bool = False
    create_tab_sent: bool = False
    tab_ready_id: int | None = None
    resize_sent: bool = False
    focus_sent: bool = False
    devtools_port: int | None = None
    probe_status: str = "not-run"
    roamium_exited_before_shutdown: bool = False
    roamium_exit_code_before_shutdown: int | None = None
    first_failing_hop: str = "automation-gap"


def read_text(path: pathlib.Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except FileNotFoundError:
        return ""


def run_cmd(cmd: list[str], timeout: float = 10) -> dict[str, Any]:
    try:
        proc = subprocess.run(
            cmd,
            cwd=str(ROOT),
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
        return {
            "cmd": cmd,
            "returncode": proc.returncode,
            "stdout": proc.stdout,
            "stderr": proc.stderr,
            "timed_out": False,
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "cmd": cmd,
            "returncode": None,
            "stdout": exc.stdout or "",
            "stderr": exc.stderr or "",
            "timed_out": True,
        }


def write_json(path: pathlib.Path, value: dict[str, Any]) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def print_queue_state() -> dict[str, Any]:
    return {
        "lpstat_o": run_cmd(["lpstat", "-o"], timeout=5),
        "lpstat_W_completed_o": run_cmd(["lpstat", "-W", "completed", "-o"], timeout=5),
    }


def start_pdf_server(log_dir: pathlib.Path, port: int) -> socketserver.TCPServer:
    PdfHandler.log_dir = log_dir
    server = ReusableTcpServer(("127.0.0.1", port), PdfHandler)
    host, bound_port = server.server_address
    (log_dir / "http-server.log").write_text(
        f"listening on {host}:{bound_port}\n",
        encoding="utf-8",
    )
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def wait_for_devtools_port(log_dir: pathlib.Path, timeout: float) -> int | None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        text = read_text(log_dir / "roamium.stdout") + "\n" + read_text(
            log_dir / "roamium.stderr"
        )
        if match := DEVTOOLS_RE.search(text):
            return int(match.group(1))
        time.sleep(0.1)
    return None


def wait_for_tab_ready(
    conn: socket.socket,
    log_dir: pathlib.Path,
    url: str,
    width: int,
    height: int,
    timeout: float,
    state: HarnessState,
) -> None:
    start = time.time()
    with (log_dir / "messages.log").open("w", encoding="utf-8") as messages:
        while time.time() - start < timeout:
            try:
                header = conn.recv(4)
                if not header:
                    break
                size = struct.unpack("<I", header)[0]
                payload = bytearray()
                while len(payload) < size:
                    payload.extend(conn.recv(size - len(payload)))
                top, body = inner_payload(bytes(payload))
                messages.write(f"t={time.time() - start:.3f} top_field={top}\n")
                messages.flush()
                if top == 12 and not state.create_tab_sent:
                    state.server_register_received = True
                    send_message(conn, 1, create_tab_payload(url, width, height))
                    state.create_tab_sent = True
                    messages.write("sent CreateTab\n")
                    messages.flush()
                elif top == 13:
                    state.tab_ready_id = tab_ready_id(body)
                    messages.write(f"tab_ready id={state.tab_ready_id}\n")
                    if state.tab_ready_id:
                        send_message(
                            conn,
                            3,
                            resize_payload(state.tab_ready_id, width, height),
                        )
                        send_message(conn, 10, focus_payload(state.tab_ready_id, True))
                        state.resize_sent = True
                        state.focus_sent = True
                        messages.write("sent Resize and Focus\n")
                        messages.flush()
                        return
            except socket.timeout:
                pass


def watcher_observe_title(title: str, timeout: float) -> dict[str, Any]:
    script = f'''
set deadline to (current date) + {timeout}
repeat while (current date) < deadline
  tell application "System Events"
    repeat with proc in application processes
      repeat with win in windows of proc
        try
          if name of win contains "{title}" then
            return "observed process=" & name of proc & " window=" & name of win
          end if
        end try
      end repeat
    end repeat
  end tell
  delay 0.2
end repeat
return "not-observed"
'''
    result = run_cmd(["osascript", "-e", script], timeout=timeout + 3)
    return {
        "mechanism": "osascript-system-events-window-title",
        "title": title,
        "result": result,
        "observed": result["returncode"] == 0
        and "observed process=" in result["stdout"],
    }


def watcher_cancel_title(title: str, timeout: float) -> dict[str, Any]:
    script = f'''
set deadline to (current date) + {timeout}
repeat while (current date) < deadline
  tell application "System Events"
    repeat with proc in application processes
      repeat with win in windows of proc
        try
          if name of win contains "{title}" then
            key code 53
            return "cancel-sent process=" & name of proc & " window=" & name of win
          end if
        end try
      end repeat
    end repeat
  end tell
  delay 0.2
end repeat
return "not-cancelled"
'''
    result = run_cmd(["osascript", "-e", script], timeout=timeout + 3)
    return {
        "mechanism": "osascript-system-events-escape",
        "title": title,
        "result": result,
        "cancel_sent": result["returncode"] == 0 and "cancel-sent" in result["stdout"],
    }


def start_harmless_dialog(
    title: str, timeout: float, log_dir: pathlib.Path, name: str
) -> tuple[subprocess.Popen, pathlib.Path, pathlib.Path]:
    stdout_path = log_dir / f"{name}.dialog.stdout"
    stderr_path = log_dir / f"{name}.dialog.stderr"
    proc = subprocess.Popen(
        [
            "osascript",
            "-e",
            (
                f'display dialog "Cancel-only preflight for TermSurf native print automation." '
                f'with title "{title}" buttons {{"Cancel"}} default button "Cancel" '
                f'cancel button "Cancel" giving up after {int(timeout)}'
            ),
        ],
        cwd=str(ROOT),
        text=True,
        stdout=stdout_path.open("w", encoding="utf-8"),
        stderr=stderr_path.open("w", encoding="utf-8"),
    )
    time.sleep(0.5)
    return proc, stdout_path, stderr_path


def finish_dialog(
    proc: subprocess.Popen,
    timeout: float,
    stdout_path: pathlib.Path | None = None,
    stderr_path: pathlib.Path | None = None,
) -> dict[str, Any]:
    try:
        proc.wait(timeout=timeout)
        timed_out = False
    except subprocess.TimeoutExpired:
        proc.terminate()
        try:
            proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=2)
        timed_out = True
    stdout = read_text(stdout_path) if stdout_path else ""
    stderr = read_text(stderr_path) if stderr_path else ""
    cancelled = False
    if not timed_out and proc.returncode is not None and proc.returncode >= 0:
        cancelled = "gave up:true" not in stdout and (
            "User canceled" in stderr or "button returned:" in stdout or proc.returncode != 0
        )
    return {
        "returncode": proc.returncode,
        "timed_out": timed_out,
        "stdout": stdout,
        "stderr": stderr,
        "cancelled": cancelled,
    }


def permission_diagnostic(result: dict[str, Any]) -> str | None:
    text = f"{result.get('stdout') or ''}\n{result.get('stderr') or ''}"
    if "not allowed assistive access" in text or "-25211" in text:
        return "system-events-assistive-access-denied"
    if "not authorized to send Apple events" in text:
        return "apple-events-automation-denied"
    return None


def mechanism_system_events(log_dir: pathlib.Path, title: str, timeout: float) -> dict[str, Any]:
    proc, stdout_path, stderr_path = start_harmless_dialog(title, timeout, log_dir, "system-events")
    observed = watcher_observe_title(title, timeout)
    cancel = watcher_cancel_title(title, timeout) if observed["observed"] else {
        "cancel_sent": False,
        "skipped": "dialog-not-observed",
    }
    finish = finish_dialog(proc, timeout + 2, stdout_path, stderr_path)
    disappearance = watcher_observe_title(title, 1)
    diagnostics = [
        item
        for item in (
            permission_diagnostic(observed.get("result") or {}),
            permission_diagnostic((cancel or {}).get("result") or {}),
            permission_diagnostic(disappearance.get("result") or {}),
        )
        if item
    ]
    return {
        "name": "system-events-window-title-escape",
        "production_print_compatible": True,
        "dialog_pid": proc.pid,
        "observed": bool(observed.get("observed")),
        "cancel_sent": bool(cancel.get("cancel_sent") and finish.get("cancelled")),
        "disappeared": bool(not disappearance.get("observed")),
        "dialog_result": finish,
        "observation": observed,
        "cancel": cancel,
        "disappearance": disappearance,
        "permission_diagnostics": sorted(set(diagnostics)),
    }


def swift_cgwindow_observe_any(log_dir: pathlib.Path, titles: list[str], timeout: float) -> dict[str, Any]:
    source = r'''
import CoreGraphics
import Foundation

let titles = CommandLine.arguments[1].split(separator: "\u{1f}").map(String.init)
let deadline = Date().addingTimeInterval(Double(CommandLine.arguments[2]) ?? 5.0)

while Date() < deadline {
    if let info = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] {
        for window in info {
            let name = (window[kCGWindowName as String] as? String) ?? ""
            let owner = (window[kCGWindowOwnerName as String] as? String) ?? ""
            let pid = (window[kCGWindowOwnerPID as String] as? Int) ?? -1
            let onscreen = (window[kCGWindowIsOnscreen as String] as? Bool) ?? false
            let bounds = (window[kCGWindowBounds as String] as? [String: Any]) ?? [:]
            let x = Int((bounds["X"] as? Double) ?? 0)
            let y = Int((bounds["Y"] as? Double) ?? 0)
            let width = Int((bounds["Width"] as? Double) ?? 0)
            let height = Int((bounds["Height"] as? Double) ?? 0)
            if onscreen && titles.contains(where: { name.contains($0) }) {
                print("{\"observed\":true,\"owner\":\"\(owner)\",\"pid\":\(pid),\"name\":\"\(name)\",\"bounds\":{\"x\":\(x),\"y\":\(y),\"width\":\(width),\"height\":\(height)}}")
                exit(0)
            }
        }
    }
    Thread.sleep(forTimeInterval: 0.2)
}
print("{\"observed\":false}")
exit(1)
'''
    with tempfile.TemporaryDirectory(prefix="ts834-cgwindow-") as tmp:
        script = pathlib.Path(tmp) / "observe.swift"
        script.write_text(source, encoding="utf-8")
        result = run_cmd(["swift", str(script), "\x1f".join(titles), str(timeout)], timeout=timeout + 3)
    parsed: dict[str, Any] = {"observed": False}
    try:
        parsed = json.loads(result.get("stdout") or "{}")
    except json.JSONDecodeError:
        parsed = {"observed": False, "parse_error": result.get("stdout")}
    return {
        "mechanism": "swift-coregraphics-window-title",
        "result": result,
        **parsed,
    }


def swift_cgwindow_observe(log_dir: pathlib.Path, title: str, timeout: float) -> dict[str, Any]:
    return swift_cgwindow_observe_any(log_dir, [title], timeout)


def mechanism_coregraphics_kill(log_dir: pathlib.Path, title: str, timeout: float) -> dict[str, Any]:
    proc, stdout_path, stderr_path = start_harmless_dialog(title, timeout, log_dir, "coregraphics")
    observed = swift_cgwindow_observe(log_dir, title, timeout)
    cancel_sent = False
    if observed.get("observed"):
        proc.send_signal(signal.SIGTERM)
        cancel_sent = True
    finish = finish_dialog(proc, timeout + 2, stdout_path, stderr_path)
    disappearance = swift_cgwindow_observe(log_dir, title, 1)
    diagnostics = []
    stderr = ((observed.get("result") or {}).get("stderr") or "") + "\n" + (
        (disappearance.get("result") or {}).get("stderr") or ""
    )
    if "not authorized" in stderr or "Screen Recording" in stderr:
        diagnostics.append("screen-recording-or-window-list-permission-denied")
    return {
        "name": "coregraphics-window-title-terminate-dialog",
        "production_print_compatible": False,
        "dialog_pid": proc.pid,
        "observed": bool(observed.get("observed")),
        "cancel_sent": cancel_sent,
        "disappeared": bool(not disappearance.get("observed")),
        "dialog_result": finish,
        "observation": observed,
        "cancel": {"method": "terminate-dialog-process", "cancel_sent": cancel_sent},
        "disappearance": disappearance,
        "permission_diagnostics": sorted(set(diagnostics)),
    }


def mechanism_coregraphics_escape(log_dir: pathlib.Path, title: str, timeout: float) -> dict[str, Any]:
    proc, stdout_path, stderr_path = start_harmless_dialog(
        title, timeout, log_dir, "coregraphics-escape"
    )
    observed = swift_cgwindow_observe(log_dir, title, timeout)
    cancel = {"returncode": None, "stdout": "", "stderr": "", "timed_out": False}
    if observed.get("observed"):
        cancel = run_cmd(["swift", str(CGEVENT_INJECT), "key", "53"], timeout=5)
        time.sleep(0.5)
    finish = finish_dialog(proc, timeout + 2, stdout_path, stderr_path)
    disappearance = swift_cgwindow_observe(log_dir, title, 1)
    diagnostics = []
    stderr = (
        ((observed.get("result") or {}).get("stderr") or "")
        + "\n"
        + (cancel.get("stderr") or "")
        + "\n"
        + ((disappearance.get("result") or {}).get("stderr") or "")
    )
    if "not authorized" in stderr or "Screen Recording" in stderr:
        diagnostics.append("screen-recording-or-window-list-permission-denied")
    if "not trusted" in stderr or "accessibility" in stderr.lower():
        diagnostics.append("coregraphics-input-accessibility-denied")
    return {
        "name": "coregraphics-window-title-cgevent-escape",
        "production_print_compatible": True,
        "dialog_pid": proc.pid,
        "observed": bool(observed.get("observed")),
        "cancel_sent": bool(
            observed.get("observed") and cancel.get("returncode") == 0 and finish.get("cancelled")
        ),
        "disappeared": bool(not disappearance.get("observed")),
        "dialog_result": finish,
        "observation": observed,
        "cancel": {
            "method": "cgevent-escape",
            "result": cancel,
            "cancel_sent": bool(
                observed.get("observed") and cancel.get("returncode") == 0 and finish.get("cancelled")
            ),
        },
        "disappearance": disappearance,
        "permission_diagnostics": sorted(set(diagnostics)),
}


def mechanism_coregraphics_click_cancel(log_dir: pathlib.Path, title: str, timeout: float) -> dict[str, Any]:
    proc, stdout_path, stderr_path = start_harmless_dialog(
        title, timeout, log_dir, "coregraphics-click"
    )
    observed = swift_cgwindow_observe(log_dir, title, timeout)
    cancel = {"returncode": None, "stdout": "", "stderr": "", "timed_out": False}
    bounds = observed.get("bounds") or {}
    click_attempts: list[dict[str, Any]] = []
    if observed.get("observed") and bounds:
        for x_ratio in (0.50, 0.62, 0.74):
            click_x = float(bounds.get("x") or 0) + float(bounds.get("width") or 0) * x_ratio
            click_y = float(bounds.get("y") or 0) + float(bounds.get("height") or 0) - 32.0
            cancel = run_cmd(
                [
                    "swift",
                    str(CGEVENT_INJECT),
                    "click",
                    f"{click_x:.0f}",
                    f"{click_y:.0f}",
                    "left",
                    "1",
                ],
                timeout=5,
            )
            click_attempts.append(
                {
                    "x": round(click_x, 1),
                    "y": round(click_y, 1),
                    "x_ratio": x_ratio,
                    "returncode": cancel.get("returncode"),
                }
            )
            time.sleep(0.5)
            if proc.poll() is not None:
                break
    finish = finish_dialog(proc, timeout + 2, stdout_path, stderr_path)
    disappearance = swift_cgwindow_observe(log_dir, title, 1)
    diagnostics = []
    stderr = (
        ((observed.get("result") or {}).get("stderr") or "")
        + "\n"
        + (cancel.get("stderr") or "")
        + "\n"
        + ((disappearance.get("result") or {}).get("stderr") or "")
    )
    if "not authorized" in stderr or "Screen Recording" in stderr:
        diagnostics.append("screen-recording-or-window-list-permission-denied")
    if "not trusted" in stderr or "accessibility" in stderr.lower():
        diagnostics.append("coregraphics-input-accessibility-denied")
    return {
        "name": "coregraphics-window-title-cgevent-click-cancel",
        "production_print_compatible": True,
        "dialog_pid": proc.pid,
        "observed": bool(observed.get("observed")),
        "cancel_sent": bool(
            observed.get("observed") and cancel.get("returncode") == 0 and finish.get("cancelled")
        ),
        "disappeared": bool(not disappearance.get("observed")),
        "dialog_result": finish,
        "observation": observed,
        "cancel": {
            "method": "cgevent-click-cancel",
            "result": cancel,
            "bounds": bounds,
            "click_attempts": click_attempts,
            "cancel_sent": bool(
                observed.get("observed")
                and cancel.get("returncode") == 0
                and finish.get("cancelled")
            ),
        },
        "disappearance": disappearance,
        "permission_diagnostics": sorted(set(diagnostics)),
    }


def swift_ax_press_cancel(log_dir: pathlib.Path, pid: int, title: str, timeout: float) -> dict[str, Any]:
    source = r'''
import ApplicationServices
import Foundation

let pid = pid_t(Int(CommandLine.arguments[1]) ?? -1)
let title = CommandLine.arguments[2]
let deadline = Date().addingTimeInterval(Double(CommandLine.arguments[3]) ?? 5.0)

func stringAttr(_ element: AXUIElement, _ attr: String) -> String {
    var value: CFTypeRef?
    let err = AXUIElementCopyAttributeValue(element, attr as CFString, &value)
    guard err == .success else { return "" }
    return (value as? String) ?? ""
}

func children(_ element: AXUIElement) -> [AXUIElement] {
    var value: CFTypeRef?
    let err = AXUIElementCopyAttributeValue(element, kAXChildrenAttribute as CFString, &value)
    guard err == .success else { return [] }
    return (value as? [AXUIElement]) ?? []
}

func findCancel(_ element: AXUIElement) -> AXUIElement? {
    let role = stringAttr(element, kAXRoleAttribute)
    let elementTitle = stringAttr(element, kAXTitleAttribute)
    let description = stringAttr(element, kAXDescriptionAttribute)
    if role == kAXButtonRole && (elementTitle == "Cancel" || description == "Cancel") {
        return element
    }
    for child in children(element) {
        if let match = findCancel(child) { return match }
    }
    return nil
}

let app = AXUIElementCreateApplication(pid)
let trusted = AXIsProcessTrusted()

while Date() < deadline {
    var value: CFTypeRef?
    let err = AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &value)
    if err != .success {
        print("{\"trusted\":\(trusted),\"pressed\":false,\"error\":\(err.rawValue)}")
        exit(1)
    }
    let windows = (value as? [AXUIElement]) ?? []
    for window in windows {
        let windowTitle = stringAttr(window, kAXTitleAttribute)
        if windowTitle.contains(title), let button = findCancel(window) {
            let pressErr = AXUIElementPerformAction(button, kAXPressAction as CFString)
            print("{\"trusted\":\(trusted),\"pressed\":\(pressErr == .success),\"pressError\":\(pressErr.rawValue),\"windowTitle\":\"\(windowTitle)\"}")
            exit(pressErr == .success ? 0 : 1)
        }
    }
    Thread.sleep(forTimeInterval: 0.2)
}

print("{\"trusted\":\(trusted),\"pressed\":false,\"error\":\"not-found\"}")
exit(1)
'''
    with tempfile.TemporaryDirectory(prefix="ts834-axcancel-") as tmp:
        script = pathlib.Path(tmp) / "press_cancel.swift"
        script.write_text(source, encoding="utf-8")
        result = run_cmd(["swift", str(script), str(pid), title, str(timeout)], timeout=timeout + 3)
    parsed: dict[str, Any] = {"pressed": False}
    try:
        parsed = json.loads(result.get("stdout") or "{}")
    except json.JSONDecodeError:
        parsed = {"pressed": False, "parse_error": result.get("stdout")}
    return {"mechanism": "swift-accessibility-press-cancel", "result": result, **parsed}


def mechanism_accessibility_press_cancel(log_dir: pathlib.Path, title: str, timeout: float) -> dict[str, Any]:
    proc, stdout_path, stderr_path = start_harmless_dialog(
        title, timeout, log_dir, "accessibility-press"
    )
    observed = swift_cgwindow_observe(log_dir, title, timeout)
    cancel = {"pressed": False, "result": {"returncode": None, "stdout": "", "stderr": ""}}
    if observed.get("observed"):
        cancel = swift_ax_press_cancel(log_dir, proc.pid, title, timeout)
        time.sleep(0.5)
    finish = finish_dialog(proc, timeout + 2, stdout_path, stderr_path)
    disappearance = swift_cgwindow_observe(log_dir, title, 1)
    diagnostics = []
    stderr = (
        ((observed.get("result") or {}).get("stderr") or "")
        + "\n"
        + ((cancel.get("result") or {}).get("stderr") or "")
        + "\n"
        + ((disappearance.get("result") or {}).get("stderr") or "")
    )
    if "not authorized" in stderr or "not trusted" in stderr or "not allowed" in stderr:
        diagnostics.append("accessibility-permission-denied")
    if cancel.get("trusted") is False:
        diagnostics.append("accessibility-not-trusted")
    return {
        "name": "accessibility-press-cancel-button",
        "production_print_compatible": True,
        "dialog_pid": proc.pid,
        "observed": bool(observed.get("observed")),
        "cancel_sent": bool(cancel.get("pressed") and finish.get("cancelled")),
        "disappeared": bool(not disappearance.get("observed")),
        "dialog_result": finish,
        "observation": observed,
        "cancel": {
            "method": "accessibility-press-cancel-button",
            "result": cancel,
            "cancel_sent": bool(cancel.get("pressed") and finish.get("cancelled")),
        },
        "disappearance": disappearance,
        "permission_diagnostics": sorted(set(diagnostics)),
    }


def preflight_result_from_mechanisms(log_dir: pathlib.Path, timeout: float) -> dict[str, Any]:
    mechanisms = [
        mechanism_system_events(log_dir, PREFLIGHT_TITLE, timeout),
        mechanism_accessibility_press_cancel(log_dir, PREFLIGHT_TITLE, timeout),
        mechanism_coregraphics_escape(log_dir, PREFLIGHT_TITLE, timeout),
        mechanism_coregraphics_click_cancel(log_dir, PREFLIGHT_TITLE, timeout),
        mechanism_coregraphics_kill(log_dir, PREFLIGHT_TITLE, timeout),
    ]
    first_failing_hop, selected = classify_preflight(mechanisms)
    selected_data = next((item for item in mechanisms if item.get("name") == selected), None)
    return {
        "mechanism": "multi-mechanism-native-dialog-preflight",
        "dialog_title": PREFLIGHT_TITLE,
        "mechanisms": mechanisms,
        "selected_mechanism": selected,
        "observed": (selected_data or {}).get("observation", {"observed": False}),
        "cancel": (selected_data or {}).get("cancel", {"cancel_sent": False}),
        "dialog_result": (selected_data or {}).get("dialog_result"),
        "disappearance": {
            **((selected_data or {}).get("disappearance") or {"observed": True}),
            "disappeared": bool(selected_data and selected_data.get("disappeared")),
        },
        "first_failing_hop": first_failing_hop,
        "passed": selected is not None,
    }


def classify_preflight(mechanisms: list[dict[str, Any]]) -> tuple[str, str | None]:
    for mechanism in mechanisms:
        if (
            mechanism.get("production_print_compatible") is not False
            and mechanism.get("observed")
            and mechanism.get("cancel_sent")
            and mechanism.get("disappeared")
        ):
            return "no-failure-observed", mechanism.get("name")
    if any(
        mechanism.get("production_print_compatible") is not False
        and mechanism.get("observed")
        and not mechanism.get("cancel_sent")
        for mechanism in mechanisms
    ):
        return "dialog-cancel-failed", None
    for mechanism in mechanisms:
        diagnostics = mechanism.get("permission_diagnostics") or []
        if diagnostics:
            return "permission-denied", None
    if any(not mechanism.get("observed") for mechanism in mechanisms):
        return "dialog-observation-failed", None
    if any(not mechanism.get("cancel_sent") for mechanism in mechanisms):
        return "dialog-cancel-failed", None
    if any(not mechanism.get("disappeared") for mechanism in mechanisms):
        return "dialog-disappearance-not-proven", None
    return "automation-gap", None


def run_watcher_preflight(log_dir: pathlib.Path, timeout: float) -> int:
    log_dir.mkdir(parents=True, exist_ok=True)
    queue_before = print_queue_state()
    preflight = preflight_result_from_mechanisms(log_dir, timeout)
    mechanisms = preflight["mechanisms"]
    queue_after = print_queue_state()
    selected = preflight["selected_mechanism"]
    summary = {
        "probe": "watcher-preflight",
        "dialog_title": PREFLIGHT_TITLE,
        "first_failing_hop": preflight["first_failing_hop"],
        "overall_result": "pass" if preflight["first_failing_hop"] == "no-failure-observed" else "partial",
        "mechanisms": mechanisms,
        "selected_mechanism": selected,
        "safe_for_production_print_probe": selected is not None,
        "production_print_click_attempted": False,
        "print_queue_before": queue_before,
        "print_queue_after": queue_after,
    }
    write_json(log_dir / "native-dialog-preflight-summary.json", summary)
    return 0 if selected else 1


def preflight_dialog(timeout: float) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="ts834-preflight-") as tmp:
        return preflight_result_from_mechanisms(pathlib.Path(tmp), timeout)


def watch_and_cancel_print_dialog(timeout: float) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="ts834-print-watch-") as tmp:
        log_dir = pathlib.Path(tmp)
        observed = swift_cgwindow_observe_any(log_dir, ["Print", "Printer"], timeout)
        cancel = {"pressed": False, "result": {"returncode": 1, "stdout": "", "stderr": ""}}
        if observed.get("observed"):
            cancel = swift_ax_press_cancel(
                log_dir,
                int(observed.get("pid") or -1),
                str(observed.get("name") or "Print"),
                timeout,
            )
            time.sleep(0.5)
        disappearance = swift_cgwindow_observe_any(log_dir, ["Print", "Printer"], 1)
    return {
        "mechanism": "coregraphics-print-window-accessibility-press-cancel",
        "observation": observed,
        "disappearance": disappearance,
        "result": cancel.get("result"),
        "cancel": cancel,
        "dialog_observed": bool(observed.get("observed")),
        "cancel_sent": bool(observed.get("observed") and cancel.get("pressed")),
        "disappeared": bool(not disappearance.get("observed")),
    }


def run_save_print_probe(
    log_dir: pathlib.Path,
    devtools_port: int,
    url_contains: str,
    server_port: int,
    allow_native_print_click: bool,
    timeout_seconds: int,
    settle_seconds: int,
) -> tuple[str, str, pathlib.Path]:
    out_dir = log_dir / "native-print"
    out_dir.mkdir(parents=True, exist_ok=True)
    downloads_dir = out_dir / "downloads"
    downloads_dir.mkdir(parents=True, exist_ok=True)
    bridge_trace_file = log_dir / "pdf-print-bridge.log"
    native_trace_file = log_dir / "pdf-native-print.log"
    if allow_native_print_click:
        native_trace_file.write_text("", encoding="utf-8")
    else:
        bridge_trace_file.write_text("", encoding="utf-8")
    cmd = [
        "node",
        str(SAVE_PRINT_TITLE_LOCAL_PROBE),
        "--devtools-port",
        str(devtools_port),
        "--url-contains",
        url_contains,
        "--out-dir",
        str(out_dir),
        "--downloads-dir",
        str(downloads_dir),
        "--timeout-seconds",
        str(timeout_seconds),
        "--settle-seconds",
        str(settle_seconds),
        "--http-pdf-url",
        f"http://127.0.0.1:{server_port}/bitcoin.pdf",
        "--file-pdf-url",
        BITCOIN_PDF.resolve().as_uri(),
        "--embedded-html-url",
        f"http://127.0.0.1:{server_port}/embedded-pdf.html",
        "--trace-file",
        str(log_dir / "pdf-input.log"),
        "--roamium-stderr",
        str(log_dir / "roamium.stderr"),
    ]
    if allow_native_print_click:
        cmd.extend(["--allow-native-print-click", "1"])
        cmd.extend(["--native-print-trace-file", str(native_trace_file)])
    else:
        cmd.extend(["--print-bridge-trace-file", str(bridge_trace_file)])
    proc = subprocess.run(
        cmd,
        cwd=str(ROOT),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    (out_dir / "probe.stdout").write_text(proc.stdout, encoding="utf-8")
    (out_dir / "probe.stderr").write_text(proc.stderr, encoding="utf-8")
    return (
        "ok" if proc.returncode == 0 else "error",
        proc.stderr.strip(),
        out_dir / "save-print-title-local-summary.json",
    )


def classify(
    args: argparse.Namespace,
    state: HarnessState,
    preflight: dict[str, Any],
    queue_before: dict[str, Any],
    queue_after: dict[str, Any] | None,
    probe_summary: dict[str, Any] | None,
    print_watch: dict[str, Any] | None,
) -> None:
    if args.probe == "safety-gate":
        state.first_failing_hop = "native-print-safety-gate-not-ready"
        return
    if not args.allow_native_dialog_click:
        state.first_failing_hop = "native-print-safety-gate-not-ready"
    elif not preflight.get("passed"):
        state.first_failing_hop = "native-print-safety-gate-not-ready"
    elif not queue_before:
        state.first_failing_hop = "native-print-safety-gate-not-ready"
    elif not state.server_register_received:
        state.first_failing_hop = "roamium-not-registered"
    elif not state.tab_ready_id:
        state.first_failing_hop = "tab-not-ready"
    elif not state.devtools_port:
        state.first_failing_hop = "devtools-missing"
    elif state.probe_status != "ok" or not probe_summary:
        state.first_failing_hop = "native-print-observation-gap"
    elif state.roamium_exited_before_shutdown:
        state.first_failing_hop = "native-print-observation-gap"
    elif (
        json.dumps(queue_before, sort_keys=True)
        != json.dumps(queue_after or {}, sort_keys=True)
        and (print_watch or {}).get("dialog_observed") is not True
    ):
        state.first_failing_hop = "native-print-job-submitted-unexpectedly"
    else:
        print_summary = probe_summary.get("print") or {}
        status = print_summary.get("status")
        native_lines = print_summary.get("printNativeLines") or []
        if status == "print-control-missing":
            state.first_failing_hop = "native-print-control-missing"
        elif status == "print-ready-disabled-by-flags":
            state.first_failing_hop = "native-print-disabled-by-load-time-flags"
        elif status != "print-native-click-sent":
            state.first_failing_hop = "native-print-click-not-sent"
        elif any(" event=get-default-print-settings-null " in line for line in native_lines):
            state.first_failing_hop = "browser-default-print-settings-null"
        elif any(" event=print-init-settings-failed " in line for line in native_lines):
            state.first_failing_hop = "print-render-frame-helper-init-settings-failed"
        elif print_watch and print_watch.get("dialog_observed") and print_watch.get("cancel_sent"):
            state.first_failing_hop = "native-print-dialog-seen-cancelled"
        elif print_watch and print_watch.get("dialog_observed"):
            state.first_failing_hop = "native-print-dialog-seen-cancel-failed"
        else:
            state.first_failing_hop = "native-print-click-sent-no-dialog"


def write_summary(
    log_dir: pathlib.Path,
    args: argparse.Namespace,
    state: HarnessState,
    extra: dict[str, Any],
) -> None:
    data = {
        "probe": args.probe,
        "allow_native_dialog_click": args.allow_native_dialog_click,
        "server_register_received": state.server_register_received,
        "create_tab_sent": state.create_tab_sent,
        "tab_ready_id": state.tab_ready_id,
        "resize_sent": state.resize_sent,
        "focus_sent": state.focus_sent,
        "devtools_port": state.devtools_port,
        "probe_status": state.probe_status,
        "roamium_exited_before_shutdown": state.roamium_exited_before_shutdown,
        "roamium_exit_code_before_shutdown": state.roamium_exit_code_before_shutdown,
        "first_failing_hop": state.first_failing_hop,
    }
    data.update(extra)
    (log_dir / "pdf-native-print-summary.json").write_text(
        json.dumps(data, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--log-dir", required=True)
    parser.add_argument(
        "--probe",
        choices=["safety-gate", "native-dialog", "watcher-preflight"],
        required=True,
    )
    parser.add_argument("--allow-native-dialog-click", action="store_true")
    parser.add_argument("--width", type=int, default=1200)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--pdf-port", type=int, default=0)
    parser.add_argument("--setup-timeout", type=float, default=30)
    parser.add_argument("--capture-timeout-seconds", type=int, default=30)
    parser.add_argument("--settle-seconds", type=int, default=8)
    parser.add_argument("--watch-timeout", type=float, default=8)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    log_dir = pathlib.Path(args.log_dir).resolve()
    log_dir.mkdir(parents=True, exist_ok=True)
    if args.probe == "watcher-preflight":
        return run_watcher_preflight(log_dir, args.watch_timeout)
    if not ROAMIUM.exists():
        raise SystemExit(f"missing Roamium binary: {ROAMIUM}")
    if not BITCOIN_PDF.exists():
        raise SystemExit(f"missing Bitcoin PDF fixture: {BITCOIN_PDF}")
    if not SAVE_PRINT_TITLE_LOCAL_PROBE.exists():
        raise SystemExit(f"missing print probe: {SAVE_PRINT_TITLE_LOCAL_PROBE}")

    preflight = preflight_dialog(args.watch_timeout)
    queue_before = print_queue_state()
    extra: dict[str, Any] = {
        "preflight": preflight,
        "print_queue_before": queue_before,
        "safety_gate_passed": bool(preflight.get("passed")),
    }
    state = HarnessState()
    if args.probe == "safety-gate" or not args.allow_native_dialog_click or not preflight.get("passed"):
        classify(args, state, preflight, queue_before, None, None, None)
        write_summary(log_dir, args, state, extra)
        return 0 if args.probe == "safety-gate" else 1

    pdf_server = start_pdf_server(log_dir, args.pdf_port)
    host, port = pdf_server.server_address
    url = f"http://{host}:{port}/bitcoin.pdf"
    socket_path = log_dir / "gui.sock"
    try:
        socket_path.unlink()
    except FileNotFoundError:
        pass

    listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    listener.bind(str(socket_path))
    listener.listen(1)
    listener.settimeout(args.setup_timeout)
    stdout = (log_dir / "roamium.stdout").open("wb")
    stderr = (log_dir / "roamium.stderr").open("wb")
    env = os.environ.copy()
    env["TERMSURF_PDF_INPUT_TRACE"] = "1"
    env["TERMSURF_PDF_INPUT_TRACE_FILE"] = str(log_dir / "pdf-input.log")
    if args.allow_native_dialog_click:
        env["TERMSURF_PDF_NATIVE_PRINT_TRACE"] = "1"
        env["TERMSURF_PDF_NATIVE_PRINT_TRACE_FILE"] = str(log_dir / "pdf-native-print.log")
    else:
        env["TERMSURF_PDF_PRINT_BRIDGE_TRACE"] = "1"
        env["TERMSURF_PDF_PRINT_BRIDGE_TRACE_FILE"] = str(log_dir / "pdf-print-bridge.log")
    proc = subprocess.Popen(
        [
            str(ROAMIUM),
            f"--ipc-socket={socket_path}",
            f"--user-data-dir={log_dir / 'profile'}",
            "--no-sandbox",
        ],
        cwd=str(ROOT / "chromium/src"),
        stdout=stdout,
        stderr=stderr,
        env=env,
    )

    conn: socket.socket | None = None
    print_watch: dict[str, Any] | None = None
    probe_summary = None
    queue_after = None
    try:
        conn, _ = listener.accept()
        conn.settimeout(0.2)
        wait_for_tab_ready(
            conn,
            log_dir,
            url,
            args.width,
            args.height,
            args.setup_timeout,
            state,
        )
        state.devtools_port = wait_for_devtools_port(log_dir, args.setup_timeout)
        if state.devtools_port:
            watcher_result: dict[str, Any] = {}

            def run_watcher() -> None:
                nonlocal watcher_result
                watcher_result = watch_and_cancel_print_dialog(args.watch_timeout)

            watcher_thread = threading.Thread(target=run_watcher)
            watcher_thread.start()
            state.probe_status, probe_error, probe_path = run_save_print_probe(
                log_dir,
                state.devtools_port,
                "bitcoin.pdf",
                port,
                True,
                args.capture_timeout_seconds,
                args.settle_seconds,
            )
            extra["probe_error"] = probe_error
            watcher_thread.join(timeout=args.watch_timeout + 5)
            print_watch = watcher_result or {
                "dialog_observed": False,
                "cancel_sent": False,
                "timed_out": True,
            }
            extra["print_dialog_watch"] = print_watch
            probe_summary = json.loads(probe_path.read_text(encoding="utf-8")) if probe_path.exists() else None
            extra["probe_summary"] = probe_summary
            queue_after = print_queue_state()
            extra["print_queue_after"] = queue_after

        exit_code = proc.poll()
        state.roamium_exited_before_shutdown = exit_code is not None
        state.roamium_exit_code_before_shutdown = exit_code
        classify(args, state, preflight, queue_before, queue_after, probe_summary, print_watch)
        write_summary(log_dir, args, state, extra)
        return 0 if state.first_failing_hop == "native-print-dialog-seen-cancelled" else 1
    finally:
        if conn:
            conn.close()
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
        stdout.close()
        stderr.close()
        listener.close()
        pdf_server.shutdown()


if __name__ == "__main__":
    sys.exit(main())
