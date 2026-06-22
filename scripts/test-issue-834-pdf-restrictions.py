#!/usr/bin/env python3
"""Probe Roamium PDF copy/save restrictions through TermSurf protocol input."""

from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import os
import pathlib
import re
import socket
import socketserver
import struct
import subprocess
import sys
import threading
import time
from dataclasses import dataclass, field as dataclass_field
from typing import Any

from termsurf_pdf_protocol_harness import (
    bool_field,
    create_tab_payload,
    double_field,
    inner_payload,
    send_message,
    string_field,
    tab_ready_id,
    varint_field,
)


ROOT = pathlib.Path(__file__).resolve().parents[1]
ROAMIUM = ROOT / "chromium/src/out/Default/roamium"
BITCOIN_PDF = ROOT / "test-html/public/bitcoin.pdf"
RESTRICTIONS_PROBE = ROOT / "scripts/probe-pdf-restrictions.mjs"
DEVTOOLS_RE = re.compile(r"DevTools listening on ws://127\.0\.0\.1:(\d+)/")

META_MODIFIER = 8
VKEY_A = 65
VKEY_C = 67


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


def key_event_payload(
    tab_id: int,
    event_type: str,
    windows_key_code: int,
    utf8: str = "",
    modifiers: int = 0,
) -> bytes:
    return (
        varint_field(1, tab_id)
        + string_field(2, event_type)
        + varint_field(3, windows_key_code)
        + string_field(4, utf8)
        + varint_field(5, modifiers)
    )


def mouse_event_payload(
    tab_id: int,
    event_type: str,
    button: str,
    x: float,
    y: float,
    click_count: int,
    modifiers: int,
) -> bytes:
    return (
        varint_field(1, tab_id)
        + string_field(2, event_type)
        + string_field(3, button)
        + double_field(4, x)
        + double_field(5, y)
        + varint_field(6, click_count)
        + varint_field(7, modifiers)
    )


class ReusableTcpServer(socketserver.TCPServer):
    allow_reuse_address = True


class RestrictionsPdfHandler(http.server.BaseHTTPRequestHandler):
    log_dir: pathlib.Path
    unrestricted_pdf: pathlib.Path
    restricted_pdf: pathlib.Path

    def log_message(self, fmt: str, *args: object) -> None:
        with (self.log_dir / "http.log").open("a", encoding="utf-8") as log:
            log.write((fmt % args) + "\n")

    def do_GET(self) -> None:
        request_path = self.path.split("?", 1)[0]
        if request_path == "/unrestricted.pdf":
            data = self.unrestricted_pdf.read_bytes()
        elif request_path == "/restricted.pdf":
            data = self.restricted_pdf.read_bytes()
        else:
            self.send_response(404)
            self.end_headers()
            return
        self.send_response(200)
        self.send_header("Content-Type", "application/pdf")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


@dataclass
class HarnessState:
    server_register_received: bool = False
    create_tab_sent: bool = False
    tab_ready_id: int | None = None
    resize_sent: bool = False
    focus_sent: bool = False
    devtools_port: int | None = None
    mouse_messages_sent: list[dict[str, Any]] = dataclass_field(default_factory=list)
    key_messages_sent: list[dict[str, Any]] = dataclass_field(default_factory=list)
    roamium_trace_init: bool = False
    roamium_focus_line: bool = False
    roamium_key_event_line: bool = False
    roamium_key_ffi_line: bool = False
    chromium_key_route_line: bool = False
    chromium_key_target_classification: str = "unknown"
    clipboard_before_text_length: int = 0
    clipboard_after_text_length: int = 0
    devtools_probe_status: str = "not-run"
    first_failing_hop: str = "automation-gap"


def read_text(path: pathlib.Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except FileNotFoundError:
        return ""


def text_sha256(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def run_checked(cmd: list[str], cwd: pathlib.Path) -> dict[str, Any]:
    proc = subprocess.run(
        cmd,
        cwd=str(cwd),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    return {
        "cmd": cmd,
        "returncode": proc.returncode,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
    }


def prepare_fixtures(log_dir: pathlib.Path) -> dict[str, Any]:
    fixtures = log_dir / "fixtures"
    fixtures.mkdir(parents=True, exist_ok=True)
    unrestricted = fixtures / "unrestricted.pdf"
    restricted = fixtures / "restricted.pdf"
    unrestricted.write_bytes(BITCOIN_PDF.read_bytes())
    qpdf_cmd = [
        "qpdf",
        "--encrypt",
        "",
        "owner-issue834-exp6",
        "256",
        "--print=none",
        "--modify=none",
        "--extract=n",
        "--annotate=n",
        "--",
        str(unrestricted),
        str(restricted),
    ]
    qpdf_result = run_checked(qpdf_cmd, ROOT)
    encryption_result = (
        run_checked(["qpdf", "--show-encryption", str(restricted)], ROOT)
        if restricted.exists()
        else {"returncode": -1, "stdout": "", "stderr": "restricted PDF missing"}
    )
    return {
        "unrestricted": str(unrestricted),
        "restricted": str(restricted),
        "unrestricted_bytes": unrestricted.stat().st_size,
        "restricted_bytes": restricted.stat().st_size if restricted.exists() else 0,
        "qpdf": qpdf_result,
        "encryption": encryption_result,
    }


def start_pdf_server(
    log_dir: pathlib.Path,
    port: int,
    unrestricted: pathlib.Path,
    restricted: pathlib.Path,
) -> socketserver.TCPServer:
    RestrictionsPdfHandler.log_dir = log_dir
    RestrictionsPdfHandler.unrestricted_pdf = unrestricted
    RestrictionsPdfHandler.restricted_pdf = restricted
    server = ReusableTcpServer(("127.0.0.1", port), RestrictionsPdfHandler)
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
                        send_message(conn, 3, resize_payload(state.tab_ready_id, width, height))
                        state.resize_sent = True
                        messages.write("sent Resize\n")
                        messages.flush()
                        return
            except socket.timeout:
                pass


def send_focus(conn: socket.socket, state: HarnessState) -> None:
    if not state.tab_ready_id:
        return
    send_message(conn, 10, focus_payload(state.tab_ready_id, True))
    state.focus_sent = True
    time.sleep(0.1)


def send_click(conn: socket.socket, state: HarnessState, x: float, y: float) -> None:
    if not state.tab_ready_id:
        return
    for event_type in ("down", "up"):
        send_message(
            conn,
            6,
            mouse_event_payload(state.tab_ready_id, event_type, "left", x, y, 1, 0),
        )
        state.mouse_messages_sent.append(
            {
                "index": len(state.mouse_messages_sent),
                "type": event_type,
                "button": "left",
                "x": x,
                "y": y,
                "click_count": 1,
                "modifiers": 0,
            }
        )
        time.sleep(0.05)


def send_key(
    conn: socket.socket,
    state: HarnessState,
    event_type: str,
    windows_key_code: int,
    utf8: str = "",
    modifiers: int = 0,
) -> None:
    if not state.tab_ready_id:
        return
    send_message(
        conn,
        9,
        key_event_payload(state.tab_ready_id, event_type, windows_key_code, utf8, modifiers),
    )
    state.key_messages_sent.append(
        {
            "index": len(state.key_messages_sent),
            "type": event_type,
            "windows_key_code": windows_key_code,
            "utf8_len": len(utf8),
            "modifiers": modifiers,
        }
    )
    time.sleep(0.05)


def send_command_shortcut(conn: socket.socket, state: HarnessState, keycode: int) -> None:
    send_key(conn, state, "down", keycode, "", META_MODIFIER)
    send_key(conn, state, "up", keycode, "", META_MODIFIER)


def run_devtools_expression(
    devtools_port: int, url_contains: str, expression: str, timeout_seconds: int
) -> dict[str, Any]:
    node_script = r"""
const [port, urlContains, expression, timeoutSeconds] = process.argv.slice(1);
function sleep(ms) { return new Promise((resolve) => setTimeout(resolve, ms)); }
async function targets() {
  const response = await fetch(`http://127.0.0.1:${port}/json/list`);
  if (!response.ok) throw new Error(`target list HTTP ${response.status}`);
  return await response.json();
}
async function findTarget() {
  const deadline = Date.now() + Number(timeoutSeconds) * 1000;
  let last = [];
  while (Date.now() < deadline) {
    last = await targets();
    const target = last.find((item) => item.type === "page" && item.url.includes(urlContains) && item.webSocketDebuggerUrl);
    if (target) return target;
    await sleep(250);
  }
  throw new Error(`target missing: ${urlContains}; ${JSON.stringify(last.map((item) => ({type:item.type,url:item.url,title:item.title})))}`);
}
function connect(wsUrl) {
  const socket = new WebSocket(wsUrl);
  let nextId = 1;
  const pending = new Map();
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const {resolve, reject} = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) reject(new Error(`${message.error.message || "DevTools error"} (${message.error.code})`));
      else resolve(message.result || {});
    }
  });
  const open = new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, {once: true});
    socket.addEventListener("error", reject, {once: true});
  });
  function send(method, params = {}) {
    const id = nextId++;
    const promise = new Promise((resolve, reject) => pending.set(id, {resolve, reject}));
    socket.send(JSON.stringify({id, method, params}));
    return promise;
  }
  return {socket, open, send};
}
(async () => {
  const target = await findTarget();
  const client = connect(target.webSocketDebuggerUrl);
  await client.open;
  await client.send("Browser.grantPermissions", {permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"]}).catch(() => {});
  const result = await client.send("Runtime.evaluate", {expression, awaitPromise: true, returnByValue: true});
  client.socket.close();
  console.log(JSON.stringify({ok: true, result}));
})().catch((error) => {
  console.log(JSON.stringify({ok: false, error: String(error.stack || error)}));
  process.exitCode = 1;
});
"""
    proc = subprocess.run(
        ["node", "-e", node_script, str(devtools_port), url_contains, expression, str(timeout_seconds)],
        cwd=str(ROOT),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    try:
        data = json.loads(proc.stdout.strip() or "{}")
    except json.JSONDecodeError:
        data = {"ok": False, "error": f"invalid JSON: {proc.stdout!r}"}
    if proc.returncode != 0 and data.get("ok") is not False:
        data["ok"] = False
        data["error"] = proc.stderr.strip() or f"node exited {proc.returncode}"
    if proc.stderr.strip():
        data["stderr"] = proc.stderr.strip()
    return data


def devtools_value(result: dict[str, Any]) -> Any:
    if not result.get("ok"):
        return None
    return result.get("result", {}).get("result", {}).get("value")


def clear_clipboard(devtools_port: int, url_contains: str, timeout_seconds: int) -> dict[str, Any]:
    return run_devtools_expression(
        devtools_port,
        url_contains,
        "navigator.clipboard.writeText('').then(() => ({ok: true}))",
        timeout_seconds,
    )


def read_clipboard(devtools_port: int, url_contains: str, timeout_seconds: int) -> dict[str, Any]:
    return run_devtools_expression(
        devtools_port,
        url_contains,
        "navigator.clipboard.readText().then((text) => ({text, length: text.length}))",
        timeout_seconds,
    )


def clipboard_text(result: dict[str, Any]) -> str:
    value = devtools_value(result)
    if isinstance(value, dict):
        return str(value.get("text") or "")
    return ""


def run_restrictions_probe(
    log_dir: pathlib.Path,
    devtools_port: int,
    url_contains: str,
    timeout_seconds: int,
    settle_seconds: int,
) -> tuple[str, str, pathlib.Path]:
    out_dir = log_dir / "devtools"
    downloads_dir = log_dir / "downloads"
    out_dir.mkdir(parents=True, exist_ok=True)
    downloads_dir.mkdir(parents=True, exist_ok=True)
    proc = subprocess.run(
        [
            "node",
            str(RESTRICTIONS_PROBE),
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
        ],
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
        out_dir / "pdf-restrictions-devtools-summary.json",
    )


def load_json(path: pathlib.Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def plugin_center(devtools_summary: dict[str, Any] | None) -> dict[str, float] | None:
    for state in (devtools_summary or {}).get("states", []):
        value = (state.get("state") or {}).get("value") or {}
        rect = value.get("pluginRect")
        if rect:
            try:
                return {
                    "x": float(rect["x"]) + float(rect["width"]) / 2,
                    "y": float(rect["y"]) + float(rect["height"]) / 2,
                    "width": float(rect["width"]),
                    "height": float(rect["height"]),
                }
            except (TypeError, KeyError, ValueError):
                pass
    return None


def trace_flags(log_dir: pathlib.Path, state: HarnessState) -> None:
    trace = read_text(log_dir / "pdf-input.log")
    stderr = read_text(log_dir / "roamium.stderr")
    state.roamium_trace_init = "trace-init" in trace
    state.roamium_focus_line = "focus-event" in trace
    state.roamium_key_event_line = "key-event" in trace
    state.roamium_key_ffi_line = "ffi=ts_forward_key_event" in trace
    state.chromium_key_route_line = "[termsurf-pdf-input] key-route" in stderr
    target_match = re.search(r"\[termsurf-pdf-input\] key-target .*classification=([^ ]+)", stderr)
    if target_match:
        state.chromium_key_target_classification = target_match.group(1)


def download_status(devtools_summary: dict[str, Any] | None) -> str:
    return str(((devtools_summary or {}).get("download") or {}).get("status") or "")


def any_download_disabled(devtools_summary: dict[str, Any] | None) -> bool:
    controls = (devtools_summary or {}).get("downloadControls") or []
    attempts = (((devtools_summary or {}).get("download") or {}).get("attempts") or [])
    return any(bool(control.get("disabled")) for control in controls) or any(
        bool(((attempt.get("result") or {}).get("value") or {}).get("selected", {}).get("disabled"))
        for attempt in attempts
    )


def row_results(
    args: argparse.Namespace,
    state: HarnessState,
    devtools_summary: dict[str, Any] | None,
) -> dict[str, Any]:
    status = download_status(devtools_summary)
    download_blocked = status not in (
        "download-file-created",
        "download-browser-callback-only",
    )
    disabled_state = any_download_disabled(devtools_summary)
    if args.probe == "unrestricted-control":
        return {
            "unrestricted_copy": "pass" if state.clipboard_after_text_length > 0 else "fail",
            "unrestricted_download": "pass" if not download_blocked else "fail",
            "download_status": status,
        }

    copy_blocked = state.clipboard_after_text_length == 0
    return {
        "copy_restricted_pdf": "pass" if copy_blocked else "fail",
        "save_download_restricted_pdf": "unsupported-by-current-chromium-pdf-permissions"
        if not download_blocked
        else "pass",
        "disabled_download_toolbar_state": "pass" if disabled_state else "not-observed",
        "download_status": status,
        "download_blocked": download_blocked,
        "download_control_disabled": disabled_state,
        "note": (
            "Chromium PDF content restrictions expose copy/print restrictions "
            "for this fixture after load. The original-file download control "
            "remains enabled and downloads the encrypted PDF."
        ),
    }


def classify(
    args: argparse.Namespace,
    state: HarnessState,
    devtools_summary: dict[str, Any] | None,
    fixture_info: dict[str, Any],
) -> None:
    if fixture_info.get("qpdf", {}).get("returncode") != 0:
        state.first_failing_hop = "fixture-generation-failed"
    elif not state.server_register_received:
        state.first_failing_hop = "roamium-not-registered"
    elif not state.tab_ready_id:
        state.first_failing_hop = "tab-not-ready"
    elif not state.resize_sent:
        state.first_failing_hop = "resize-not-sent"
    elif not state.devtools_port:
        state.first_failing_hop = "devtools-missing"
    elif state.devtools_probe_status != "ok" or not devtools_summary:
        state.first_failing_hop = "toolbar-state-discovery-failed"
    elif not state.focus_sent:
        state.first_failing_hop = "focus-not-sent"
    elif not state.mouse_messages_sent:
        state.first_failing_hop = "protocol-plugin-click-not-sent"
    elif not state.key_messages_sent:
        state.first_failing_hop = "protocol-key-not-sent"
    elif not state.roamium_key_event_line:
        state.first_failing_hop = "roamium-key-receive-missing"
    elif not state.roamium_key_ffi_line:
        state.first_failing_hop = "roamium-key-ffi-missing"
    elif not state.chromium_key_route_line:
        state.first_failing_hop = "chromium-key-route-missing"
    elif args.probe == "unrestricted-control":
        if state.clipboard_after_text_length <= 0:
            state.first_failing_hop = "unrestricted-copy-missing"
        elif download_status(devtools_summary) not in (
            "download-file-created",
            "download-browser-callback-only",
        ):
            state.first_failing_hop = "unrestricted-download-missing"
        else:
            state.first_failing_hop = "no-failure-observed"
    else:
        rows = row_results(args, state, devtools_summary)
        copy_blocked = rows["copy_restricted_pdf"] == "pass"
        download_blocked = bool(rows["download_blocked"])
        disabled_state = bool(rows["download_control_disabled"])
        if copy_blocked and (download_blocked or disabled_state):
            state.first_failing_hop = "no-failure-observed"
        elif not copy_blocked:
            state.first_failing_hop = "restricted-copy-not-blocked"
        elif not (download_blocked or disabled_state):
            state.first_failing_hop = "restricted-download-not-blocked"
        else:
            state.first_failing_hop = "restricted-state-unclassified"


def write_summary(
    log_dir: pathlib.Path,
    args: argparse.Namespace,
    state: HarnessState,
    extra: dict[str, Any],
) -> None:
    data = {
        "probe": args.probe,
        "server_register_received": state.server_register_received,
        "create_tab_sent": state.create_tab_sent,
        "tab_ready_id": state.tab_ready_id,
        "resize_sent": state.resize_sent,
        "focus_sent": state.focus_sent,
        "devtools_port": state.devtools_port,
        "protocol_mouse_messages_sent": len(state.mouse_messages_sent),
        "protocol_mouse_messages": state.mouse_messages_sent,
        "protocol_key_messages_sent": len(state.key_messages_sent),
        "protocol_key_messages": state.key_messages_sent,
        "roamium_trace_init": state.roamium_trace_init,
        "roamium_focus_line": state.roamium_focus_line,
        "roamium_key_event_line": state.roamium_key_event_line,
        "roamium_key_ffi_line": state.roamium_key_ffi_line,
        "chromium_key_route_line": state.chromium_key_route_line,
        "chromium_key_target_classification": state.chromium_key_target_classification,
        "clipboard_before_text_length": state.clipboard_before_text_length,
        "clipboard_after_text_length": state.clipboard_after_text_length,
        "devtools_probe_status": state.devtools_probe_status,
        "first_failing_hop": state.first_failing_hop,
        "row_results": row_results(args, state, extra.get("devtools_summary")),
    }
    data.update(extra)
    (log_dir / "pdf-restrictions-summary.json").write_text(
        json.dumps(data, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--log-dir", required=True)
    parser.add_argument("--probe", choices=["unrestricted-control", "restricted-document"], required=True)
    parser.add_argument("--width", type=int, default=1200)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--pdf-port", type=int, default=9799)
    parser.add_argument("--setup-timeout", type=float, default=30)
    parser.add_argument("--capture-timeout-seconds", type=int, default=30)
    parser.add_argument("--settle-seconds", type=int, default=4)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    log_dir = pathlib.Path(args.log_dir).resolve()
    args.log_dir = str(log_dir)
    log_dir.mkdir(parents=True, exist_ok=True)
    if not ROAMIUM.exists():
        raise SystemExit(f"missing Roamium binary: {ROAMIUM}")
    if not BITCOIN_PDF.exists():
        raise SystemExit(f"missing Bitcoin PDF fixture: {BITCOIN_PDF}")
    if not RESTRICTIONS_PROBE.exists():
        raise SystemExit(f"missing restrictions probe: {RESTRICTIONS_PROBE}")

    fixture_info = prepare_fixtures(log_dir)
    unrestricted_path = pathlib.Path(fixture_info["unrestricted"])
    restricted_path = pathlib.Path(fixture_info["restricted"])
    pdf_server = start_pdf_server(log_dir, args.pdf_port, unrestricted_path, restricted_path)
    request_path = "/unrestricted.pdf" if args.probe == "unrestricted-control" else "/restricted.pdf"
    url = f"http://127.0.0.1:{args.pdf_port}{request_path}"
    url_contains = request_path.strip("/")

    socket_path = log_dir / "gui.sock"
    try:
        socket_path.unlink()
    except FileNotFoundError:
        pass

    state = HarnessState()
    extra: dict[str, Any] = {"url": url, "url_contains": url_contains, "fixtures": fixture_info}
    listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    listener.bind(str(socket_path))
    listener.listen(1)
    listener.settimeout(args.setup_timeout)

    stdout = (log_dir / "roamium.stdout").open("wb")
    stderr = (log_dir / "roamium.stderr").open("wb")
    env = os.environ.copy()
    env["TERMSURF_PDF_INPUT_TRACE"] = "1"
    env["TERMSURF_PDF_INPUT_TRACE_FILE"] = str(log_dir / "pdf-input.log")
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
    try:
        conn, _ = listener.accept()
        conn.settimeout(0.2)
        wait_for_tab_ready(conn, log_dir, url, args.width, args.height, args.setup_timeout, state)
        state.devtools_port = wait_for_devtools_port(log_dir, args.setup_timeout)
        devtools_summary = None
        if state.devtools_port:
            state.devtools_probe_status, devtools_error, devtools_path = run_restrictions_probe(
                log_dir,
                state.devtools_port,
                url_contains,
                args.capture_timeout_seconds,
                args.settle_seconds,
            )
            extra["devtools_probe_error"] = devtools_error
            devtools_summary = load_json(devtools_path) if devtools_path.exists() else None
            extra["devtools_summary"] = devtools_summary
            center = plugin_center(devtools_summary)
            extra["plugin_click_point"] = center
            send_focus(conn, state)
            if center:
                send_click(conn, state, center["x"], center["y"])
                time.sleep(0.2)
            clear_result = clear_clipboard(state.devtools_port, url_contains, args.capture_timeout_seconds)
            extra["clipboard_clear"] = clear_result
            before_clipboard = read_clipboard(state.devtools_port, url_contains, args.capture_timeout_seconds)
            extra["clipboard_before"] = before_clipboard
            before_text = clipboard_text(before_clipboard)
            state.clipboard_before_text_length = len(before_text)
            send_command_shortcut(conn, state, VKEY_A)
            time.sleep(0.25)
            send_command_shortcut(conn, state, VKEY_C)
            time.sleep(0.5)
            after_clipboard = read_clipboard(state.devtools_port, url_contains, args.capture_timeout_seconds)
            extra["clipboard_after"] = after_clipboard
            after_text = clipboard_text(after_clipboard)
            state.clipboard_after_text_length = len(after_text)
            extra["clipboard_after_sha256"] = text_sha256(after_text)
            extra["clipboard_after_sample"] = after_text[:160]

        trace_flags(log_dir, state)
        classify(args, state, devtools_summary, fixture_info)
        write_summary(log_dir, args, state, extra)
        return 0 if state.first_failing_hop == "no-failure-observed" else 1
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
