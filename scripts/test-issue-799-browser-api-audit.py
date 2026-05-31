#!/usr/bin/env python3
"""Run automated browser API no-crash probes against Roamium.

The harness launches Roamium behind a minimal fake TermSurf GUI socket, serves
local probe pages, records JavaScript reports, and scans Chromium/Roamium logs
for missing Mojo binder and renderer-crash signatures.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import html
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
import tempfile
import threading
import time
from dataclasses import dataclass
from typing import Any
from urllib.parse import parse_qs, urlparse


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_ROAMIUM = ROOT / "chromium/src/out/Default/roamium"
DEFAULT_LOG_ROOT = ROOT / "logs/issue-799-browser-api-audit"

BAD_MOJO_PATTERNS = [
    "Terminating render process for bad Mojo message",
    "No binder found for interface",
    "Received bad user message",
]
CRASH_PATTERNS = [
    "RenderProcessGone",
    "bad_message",
    "CHECK failed",
    "Received signal",
]
MISSING_INTERFACE_RE = re.compile(r"No binder found for interface ([^\s]+)")
EMPTY_BINDER_RE = re.compile(r"Empty binder for interface ([^\s]+)")

ATTACHMENT_DOWNLOAD_BYTES = b"TermSurf generic attachment download fixture.\n"
BLOB_DOWNLOAD_TEXT = "TermSurf generic blob download fixture.\n"
BLOB_DOWNLOAD_BYTES = BLOB_DOWNLOAD_TEXT.encode("utf-8")
EXPECTED_DOWNLOADS = {
    "download-attachment": (
        "termsurf-download.txt",
        ATTACHMENT_DOWNLOAD_BYTES,
    ),
    "download-blob": (
        "termsurf-blob-download.txt",
        BLOB_DOWNLOAD_BYTES,
    ),
}


@dataclass(frozen=True)
class Probe:
    name: str
    feature: str
    expected_surface: str
    reference_evidence: str
    termsurf_evidence: str
    requires_user_activation: bool
    script: str


PROBES: list[Probe] = [
    Probe(
        name="badge",
        feature="Badging API",
        expected_surface="blink.mojom.BadgeService frame binder",
        reference_evidence="Headless has StubBadgeService; Issue 655 copied that pattern.",
        termsurf_evidence="TsBrowserClient registers StubBadgeService.",
        requires_user_activation=False,
        script="""
await navigator.setAppBadge?.(1);
await navigator.clearAppBadge?.();
return {status: navigator.setAppBadge ? 'resolved' : 'unsupported'};
""",
    ),
    Probe(
        name="permissions-query",
        feature="Permissions API",
        expected_surface="PermissionController / permission manager delegate",
        reference_evidence="Content shell has ShellPermissionManager; Chrome has full permission stack.",
        termsurf_evidence="No broad TermSurf permission manager found outside PDF-specific paths.",
        requires_user_activation=False,
        script="""
if (!navigator.permissions?.query) return {status: 'unsupported'};
const names = ['geolocation', 'notifications', 'camera', 'microphone'];
const results = [];
for (const name of names) {
  try {
    const result = await navigator.permissions.query({name});
    results.push({name, status: 'resolved', state: result.state});
  } catch (error) {
    results.push({name, status: 'rejected', error: String(error), errorName: error?.name || null});
  }
}
return {status: 'resolved', results};
""",
    ),
    Probe(
        name="notification-permission",
        feature="Notifications",
        expected_surface="Notification permission service / browser notification delegate",
        reference_evidence="Chrome wires notifications and permissions; headless provides permission behavior.",
        termsurf_evidence="No generic TermSurf notification service or OS notification path.",
        requires_user_activation=False,
        script="""
if (!('Notification' in window)) return {status: 'unsupported'};
const result = await Notification.requestPermission();
return {status: 'resolved', permission: result};
""",
    ),
    Probe(
        name="geolocation-deny",
        feature="Geolocation",
        expected_surface="Geolocation provider and permission delegate",
        reference_evidence="ContentBrowserClient exposes geolocation hooks; headless has platform geolocation handling.",
        termsurf_evidence="No TermSurf geolocation UX or fake provider path.",
        requires_user_activation=False,
        script="""
if (!navigator.geolocation?.getCurrentPosition) return {status: 'unsupported'};
return await new Promise((resolve) => {
  navigator.geolocation.getCurrentPosition(
    (position) => resolve({status: 'resolved', coords: !!position?.coords}),
    (error) => resolve({status: 'rejected', code: error.code, message: error.message}),
    {timeout: 750, maximumAge: 0}
  );
});
""",
    ),
    Probe(
        name="credential-get-empty",
        feature="Credential Management",
        expected_surface="Credential manager / WebAuthn delegate paths",
        reference_evidence="Chrome has credential delegates; WebAuthn can be tested with virtual authenticators.",
        termsurf_evidence="No TermSurf credential or WebAuthn delegate found.",
        requires_user_activation=False,
        script="""
if (!navigator.credentials?.get) return {status: 'unsupported'};
try {
  const result = await navigator.credentials.get({password: true, mediation: 'silent'});
  return {status: 'resolved', value: result ? result.type : null};
} catch (error) {
  return {status: 'rejected', error: String(error), errorName: error?.name || null};
}
""",
    ),
    Probe(
        name="webauthn-create",
        feature="WebAuthn",
        expected_surface="WebAuthenticationDelegate and authenticator request service",
        reference_evidence="Chrome wires WebAuthn; DevTools has virtual authenticator support.",
        termsurf_evidence="No TermSurf WebAuthn delegate or virtual authenticator harness yet.",
        requires_user_activation=True,
        script="""
if (!navigator.credentials?.create || !window.PublicKeyCredential) return {status: 'unsupported'};
const challenge = new Uint8Array(16);
const userId = new Uint8Array(16);
try {
  return await Promise.race([
    navigator.credentials.create({
      publicKey: {
        challenge,
        rp: {name: 'TermSurf Test'},
        user: {id: userId, name: 'test@example.test', displayName: 'Test User'},
        pubKeyCredParams: [{type: 'public-key', alg: -7}],
        timeout: 1000,
        attestation: 'none'
      }
    }).then(() => ({status: 'resolved'})),
    new Promise((resolve) => setTimeout(() => resolve({status: 'blocked_needs_virtual_authenticator'}), 1500))
  ]);
} catch (error) {
  const blocked = error?.name === 'NotAllowedError';
  return {status: blocked ? 'blocked_user_activation' : 'rejected', error: String(error), errorName: error?.name || null};
}
""",
    ),
    Probe(
        name="file-system-access",
        feature="File System Access",
        expected_surface="File-system access permission context and native picker delegate",
        reference_evidence="Chrome has file-system access permission/picker plumbing.",
        termsurf_evidence="No TermSurf file-system access picker or permission UX.",
        requires_user_activation=True,
        script="""
if (!window.showOpenFilePicker) return {status: 'unsupported'};
try {
  await window.showOpenFilePicker({multiple: false});
  return {status: 'resolved'};
} catch (error) {
  const blocked = error?.name === 'SecurityError' || error?.name === 'NotAllowedError' || /user activation/i.test(String(error));
  return {status: blocked ? 'blocked_user_activation' : 'rejected', error: String(error), errorName: error?.name || null};
}
""",
    ),
    Probe(
        name="payment-request",
        feature="Payment Request",
        expected_surface="Payment app/service delegate and permission/product UI",
        reference_evidence="Chrome has payment service stack; content embedders often omit full feature.",
        termsurf_evidence="No TermSurf payment request implementation.",
        requires_user_activation=False,
        script="""
if (!window.PaymentRequest) return {status: 'unsupported'};
try {
  const makeRequest = () => new PaymentRequest(
    [{supportedMethods: 'basic-card'}],
    {total: {label: 'Total', amount: {currency: 'USD', value: '1.00'}}}
  );
  const request = makeRequest();
  const canMakePayment = await request.canMakePayment();
  let hasEnrolledInstrument = null;
  if (typeof request.hasEnrolledInstrument === 'function') {
    hasEnrolledInstrument = await request.hasEnrolledInstrument();
  }
  let show = null;
  try {
    await makeRequest().show();
    show = {status: 'resolved'};
  } catch (error) {
    show = {
      status: 'rejected',
      error: String(error),
      errorName: error?.name || null
    };
  }
  return {status: 'resolved', canMakePayment, hasEnrolledInstrument, show};
} catch (error) {
  return {status: 'rejected', error: String(error), errorName: error?.name || null};
}
""",
    ),
    Probe(
        name="service-worker-basic",
        feature="Service worker browser services",
        expected_surface="Service-worker binder maps and storage/registration delegates",
        reference_evidence="Chrome/content shell support service-worker registration paths.",
        termsurf_evidence="No systematic TermSurf service-worker browser-service audit yet.",
        requires_user_activation=False,
        script="""
if (!navigator.serviceWorker?.register) return {status: 'unsupported'};
try {
  return await Promise.race([
    (async () => {
      const registration = await navigator.serviceWorker.register(
        '/probe/service-worker-basic-worker.js',
        {scope: '/probe/'}
      );
      const worker = registration.installing || registration.waiting || registration.active;
      if (worker && worker.state !== 'activated') {
        await new Promise((resolve) => {
          worker.addEventListener('statechange', () => {
            if (worker.state === 'activated') resolve();
          });
        });
      }
      await registration.unregister();
      return {status: 'resolved'};
    })(),
    new Promise((resolve) => setTimeout(() => resolve({status: 'probe_timeout'}), 2500))
  ]);
} catch (error) {
  return {status: 'rejected', error: String(error), errorName: error?.name || null};
}
""",
    ),
    Probe(
        name="download-attachment",
        feature="Generic attachment downloads",
        expected_surface="Content download manager delegate and deterministic target path",
        reference_evidence="Content Shell has ShellDownloadManagerDelegate; Chrome has full download UI.",
        termsurf_evidence="TermSurf needs contained no-prompt generic download target selection.",
        requires_user_activation=False,
        script="""
const link = document.createElement('a');
link.href = '/download/attachment.txt';
link.textContent = 'download attachment';
document.body.appendChild(link);
setTimeout(() => link.click(), 250);
return {status: 'download_triggered', expectedFile: 'termsurf-download.txt'};
""",
    ),
    Probe(
        name="download-blob",
        feature="Generic blob downloads",
        expected_surface="Content download manager delegate and deterministic target path",
        reference_evidence="Chrome downloads Blob URLs through the generic download stack.",
        termsurf_evidence="TermSurf needs contained no-prompt generic download target selection.",
        requires_user_activation=False,
        script=f"""
const blob = new Blob([{json.dumps(BLOB_DOWNLOAD_TEXT)}], {{type: 'text/plain'}});
const link = document.createElement('a');
link.href = URL.createObjectURL(blob);
link.download = 'termsurf-blob-download.txt';
link.textContent = 'download blob';
document.body.appendChild(link);
setTimeout(() => link.click(), 250);
return {{status: 'download_triggered', expectedFile: 'termsurf-blob-download.txt'}};
""",
    ),
    Probe(
        name="javascript-alert",
        feature="JavaScript dialogs",
        expected_surface="TermSurf JavaScript dialog request/reply protocol",
        reference_evidence="Content Shell opens native dialogs; TermSurf must route through protocol.",
        termsurf_evidence="Experiment 5 adds protocol-mediated dialogs.",
        requires_user_activation=False,
        script="""
alert('alpha');
return {status: 'resolved', value: 'resumed'};
""",
    ),
    Probe(
        name="javascript-confirm-accept",
        feature="JavaScript dialogs",
        expected_surface="TermSurf JavaScript dialog request/reply protocol",
        reference_evidence="Content Shell opens native dialogs; TermSurf must route through protocol.",
        termsurf_evidence="Experiment 5 adds protocol-mediated dialogs.",
        requires_user_activation=False,
        script="""
const value = confirm('beta');
return {status: 'resolved', value};
""",
    ),
    Probe(
        name="javascript-confirm-cancel",
        feature="JavaScript dialogs",
        expected_surface="TermSurf JavaScript dialog request/reply protocol",
        reference_evidence="Content Shell opens native dialogs; TermSurf must route through protocol.",
        termsurf_evidence="Experiment 5 adds protocol-mediated dialogs.",
        requires_user_activation=False,
        script="""
const value = confirm('gamma');
return {status: 'resolved', value};
""",
    ),
    Probe(
        name="javascript-prompt",
        feature="JavaScript dialogs",
        expected_surface="TermSurf JavaScript dialog request/reply protocol",
        reference_evidence="Content Shell opens native dialogs; TermSurf must route through protocol.",
        termsurf_evidence="Experiment 5 adds protocol-mediated dialogs.",
        requires_user_activation=False,
        script="""
const value = prompt('delta', 'default');
return {status: 'resolved', value};
""",
    ),
    Probe(
        name="javascript-prompt-cancel",
        feature="JavaScript dialogs",
        expected_surface="TermSurf JavaScript dialog request/reply protocol",
        reference_evidence="Content Shell opens native dialogs; TermSurf must route through protocol.",
        termsurf_evidence="Experiment 5 adds protocol-mediated dialogs.",
        requires_user_activation=False,
        script="""
const value = prompt('epsilon', 'default');
return {status: 'resolved', value};
""",
    ),
    Probe(
        name="javascript-initial-load-alert",
        feature="JavaScript dialogs",
        expected_surface="TermSurf JavaScript dialog request/reply protocol",
        reference_evidence="Content Shell opens native dialogs; TermSurf must route through protocol.",
        termsurf_evidence="Experiment 5 adds protocol-mediated dialogs.",
        requires_user_activation=False,
        script="""
alert('load');
return {status: 'resolved', value: 'initial-load-resumed'};
""",
    ),
    Probe(
        name="javascript-beforeunload-proceed",
        feature="JavaScript dialogs",
        expected_surface="TermSurf beforeunload dialog request/reply protocol",
        reference_evidence="Chromium requires sticky user activation for blocking beforeunload dialogs.",
        termsurf_evidence="Experiment 5 routes beforeunload through TermSurf dialogs.",
        requires_user_activation=True,
        script="""
const button = document.createElement('button');
button.textContent = 'activate';
button.style.position = 'absolute';
button.style.left = '8px';
button.style.top = '8px';
button.style.width = '120px';
button.style.height = '40px';
button.onpointerdown = () => sendReport({status: 'pointerdown'});
button.onmousedown = () => sendReport({status: 'mousedown'});
button.onclick = () => sendReport({status: 'activated'});
document.body.appendChild(button);
document.body.tabIndex = 0;
document.body.focus();
document.addEventListener('keydown', () => sendReport({
  status: 'activated',
  activation: 'keyboard'
}), {once: true});
window.addEventListener('beforeunload', (event) => {
  event.preventDefault();
  event.returnValue = '';
});
return {status: 'ready'};
""",
    ),
    Probe(
        name="javascript-beforeunload-stay",
        feature="JavaScript dialogs",
        expected_surface="TermSurf beforeunload dialog request/reply protocol",
        reference_evidence="Chromium requires sticky user activation for blocking beforeunload dialogs.",
        termsurf_evidence="Experiment 5 routes beforeunload through TermSurf dialogs.",
        requires_user_activation=True,
        script="""
const button = document.createElement('button');
button.textContent = 'activate';
button.style.position = 'absolute';
button.style.left = '8px';
button.style.top = '8px';
button.style.width = '120px';
button.style.height = '40px';
button.onpointerdown = () => sendReport({status: 'pointerdown'});
button.onmousedown = () => sendReport({status: 'mousedown'});
button.onclick = () => sendReport({status: 'activated'});
document.body.appendChild(button);
document.body.tabIndex = 0;
document.body.focus();
document.addEventListener('keydown', () => sendReport({
  status: 'activated',
  activation: 'keyboard'
}), {once: true});
window.addEventListener('beforeunload', (event) => {
  event.preventDefault();
  event.returnValue = '';
  setTimeout(() => sendReport({status: 'stayed'}), 500);
});
return {status: 'ready'};
""",
    ),
]


def varint(value: int) -> bytes:
    out = bytearray()
    while value >= 0x80:
        out.append((value & 0x7F) | 0x80)
        value >>= 7
    out.append(value)
    return bytes(out)


def read_varint(buf: bytes, index: int) -> tuple[int, int]:
    shift = 0
    value = 0
    while index < len(buf):
        byte = buf[index]
        index += 1
        value |= (byte & 0x7F) << shift
        if not byte & 0x80:
            return value, index
        shift += 7
    return 0, index


def field(number: int, wire_type: int) -> bytes:
    return varint((number << 3) | wire_type)


def string_field(number: int, value: str) -> bytes:
    data = value.encode("utf-8")
    return field(number, 2) + varint(len(data)) + data


def varint_field(number: int, value: int) -> bytes:
    return field(number, 0) + varint(value)


def bool_field(number: int, value: bool) -> bytes:
    return field(number, 0) + varint(1 if value else 0)


def fixed_double_field(number: int, value: float) -> bytes:
    return field(number, 1) + struct.pack("<d", float(value))


def wrap(inner_field: int, payload: bytes) -> bytes:
    return field(inner_field, 2) + varint(len(payload)) + payload


def send_message(conn: socket.socket, inner_field: int, payload: bytes) -> None:
    message = wrap(inner_field, payload)
    conn.sendall(struct.pack("<I", len(message)) + message)


def inner_payload(payload: bytes) -> tuple[int, bytes]:
    key, index = read_varint(payload, 0)
    length, index = read_varint(payload, index)
    return key >> 3, payload[index : index + length]


def tab_ready_id(payload: bytes) -> int | None:
    index = 0
    while index < len(payload):
        key, index = read_varint(payload, index)
        field_number = key >> 3
        wire_type = key & 7
        if wire_type == 0:
            value, index = read_varint(payload, index)
            if field_number == 2:
                return value
        elif wire_type == 2:
            length, index = read_varint(payload, index)
            index += length
        else:
            return None
    return None


def parse_message_fields(payload: bytes) -> dict[int, Any]:
    values: dict[int, Any] = {}
    index = 0
    while index < len(payload):
        key, index = read_varint(payload, index)
        field_number = key >> 3
        wire_type = key & 7
        if wire_type == 0:
            value, index = read_varint(payload, index)
            values[field_number] = value
        elif wire_type == 2:
            length, index = read_varint(payload, index)
            data = payload[index : index + length]
            index += length
            try:
                values[field_number] = data.decode("utf-8")
            except UnicodeDecodeError:
                values[field_number] = data
        else:
            break
    return values


def javascript_dialog_reply_payload(
    tab_id: int,
    request_id: int,
    accepted: bool,
    prompt_text: str,
) -> bytes:
    return (
        varint_field(1, tab_id)
        + varint_field(2, request_id)
        + bool_field(3, accepted)
        + string_field(4, prompt_text)
    )


def dialog_response_for(probe_name: str) -> tuple[bool, str]:
    if probe_name in (
        "javascript-confirm-cancel",
        "javascript-prompt-cancel",
        "javascript-beforeunload-stay",
    ):
        return False, ""
    if probe_name == "javascript-prompt":
        return True, "typed value"
    return True, ""


def verify_javascript_dialog_probe(
    probe_name: str,
    report: dict[str, Any] | None,
    dialogs: list[dict[str, Any]],
    beforeunload_activation_observed: bool,
) -> dict[str, Any] | None:
    if not probe_name.startswith("javascript-"):
        return None
    expected: dict[str, Any] = {
        "javascript-alert": {"dialog_type": "alert", "message": "alpha", "value": "resumed"},
        "javascript-confirm-accept": {
            "dialog_type": "confirm",
            "message": "beta",
            "value": True,
        },
        "javascript-confirm-cancel": {
            "dialog_type": "confirm",
            "message": "gamma",
            "value": False,
        },
        "javascript-prompt": {
            "dialog_type": "prompt",
            "message": "delta",
            "value": "typed value",
        },
        "javascript-prompt-cancel": {
            "dialog_type": "prompt",
            "message": "epsilon",
            "value": None,
        },
        "javascript-initial-load-alert": {
            "dialog_type": "alert",
            "message": "load",
            "value": "initial-load-resumed",
        },
        "javascript-beforeunload-proceed": {
            "dialog_type": "beforeunload",
            "message": "Is it OK to leave this page?",
            "final_status": "destination_loaded",
        },
        "javascript-beforeunload-stay": {
            "dialog_type": "beforeunload",
            "message": "Is it OK to leave this page?",
            "final_status": "stayed",
        },
    }[probe_name]
    status = "completed"
    reasons: list[str] = []
    if len(dialogs) != 1:
        status = "failed"
        reasons.append(f"expected one dialog, got {len(dialogs)}")
    elif dialogs[0].get("dialog_type") != expected["dialog_type"]:
        status = "failed"
        reasons.append(f"wrong dialog type {dialogs[0].get('dialog_type')}")
    elif dialogs[0].get("message") != expected["message"]:
        status = "failed"
        reasons.append(f"wrong message {dialogs[0].get('message')}")
    if "final_status" in expected:
        if (
            probe_name.startswith("javascript-beforeunload-")
            and not beforeunload_activation_observed
        ):
            status = "failed"
            reasons.append("page did not report activation before navigation")
        if not report or report.get("status") != expected["final_status"]:
            status = "failed"
            reasons.append(
                f"page did not report final status {expected['final_status']}"
            )
    elif not report or report.get("status") != "resolved":
        status = "failed"
        reasons.append("page did not report resolved")
    elif report.get("value") != expected["value"]:
        status = "failed"
        reasons.append(f"wrong page value {report.get('value')!r}")
    return {"status": status, "reasons": reasons, "expected": expected}


def create_tab_payload(url: str, width: int, height: int) -> bytes:
    return (
        string_field(1, url)
        + string_field(2, "issue-799-fake-pane")
        + varint_field(3, width)
        + varint_field(4, height)
        + bool_field(5, False)
    )


def resize_payload(tab_id: int, width: int, height: int) -> bytes:
    return varint_field(1, tab_id) + varint_field(2, width) + varint_field(3, height)


def focus_changed_payload(tab_id: int, focused: bool) -> bytes:
    return varint_field(1, tab_id) + bool_field(2, focused)


def navigate_payload(tab_id: int, url: str) -> bytes:
    return varint_field(1, tab_id) + string_field(3, url)


def mouse_move_payload(tab_id: int, x: int, y: int) -> bytes:
    return varint_field(1, tab_id) + fixed_double_field(2, x) + fixed_double_field(3, y)


def mouse_event_payload(tab_id: int, event_type: str, x: int, y: int) -> bytes:
    return (
        varint_field(1, tab_id)
        + string_field(2, event_type)
        + string_field(3, "left")
        + fixed_double_field(4, x)
        + fixed_double_field(5, y)
        + varint_field(6, 1)
    )


def key_event_payload(tab_id: int, event_type: str, keycode: int, utf8: str = "") -> bytes:
    return (
        varint_field(1, tab_id)
        + string_field(2, event_type)
        + varint_field(3, keycode)
        + string_field(4, utf8)
    )


class ProbeState:
    def __init__(self, run_dir: pathlib.Path) -> None:
        self.run_dir = run_dir
        self.lock = threading.Lock()
        self.reports: list[dict[str, Any]] = []

    def add_report(self, report: dict[str, Any]) -> None:
        with self.lock:
            self.reports.append(report)
            with (self.run_dir / "reports.jsonl").open("a", encoding="utf-8") as out:
                out.write(json.dumps(report, sort_keys=True) + "\n")

    def reports_for(self, probe: str) -> list[dict[str, Any]]:
        with self.lock:
            return [report for report in self.reports if report.get("probe") == probe]


class ReusableThreadingTcpServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


def make_handler(state: ProbeState) -> type[http.server.BaseHTTPRequestHandler]:
    probe_by_name = {probe.name: probe for probe in PROBES}

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, fmt: str, *args: object) -> None:
            with (state.run_dir / "http.log").open("a", encoding="utf-8") as log:
                log.write((fmt % args) + "\n")

        def do_GET(self) -> None:
            parsed = urlparse(self.path)
            if parsed.path.startswith("/probe/") and parsed.path.endswith(".html"):
                name = pathlib.PurePosixPath(parsed.path).stem
                probe = probe_by_name.get(name)
                if not probe:
                    self.send_error(404)
                    return
                self.send_bytes(render_probe_page(probe), "text/html; charset=utf-8")
                return
            if parsed.path == "/probe/service-worker-basic-worker.js":
                self.send_bytes(
                    b"self.addEventListener('install', event => self.skipWaiting());\n"
                    b"self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));\n",
                    "application/javascript; charset=utf-8",
                )
                return
            if parsed.path == "/download/attachment.txt":
                self.send_response(200)
                self.send_header("Content-Type", "text/plain; charset=utf-8")
                self.send_header(
                    "Content-Disposition",
                    'attachment; filename="termsurf-download.txt"',
                )
                self.send_header("Content-Length", str(len(ATTACHMENT_DOWNLOAD_BYTES)))
                self.end_headers()
                self.wfile.write(ATTACHMENT_DOWNLOAD_BYTES)
                return
            if parsed.path == "/beforeunload-destination.html":
                query = parse_qs(parsed.query)
                probe = query.get("probe", ["unknown"])[-1]
                body = f"""<!doctype html>
<meta charset="utf-8">
<title>beforeunload destination</title>
<script>
fetch('/report', {{
  method: 'POST',
  headers: {{'Content-Type': 'application/json'}},
  body: JSON.stringify({{
    probe: {json.dumps(probe)},
    status: 'destination_loaded',
    reportedAt: new Date().toISOString()
  }}),
  keepalive: true
}});
</script>
destination
""".encode("utf-8")
                self.send_bytes(body, "text/html; charset=utf-8")
                return
            if parsed.path == "/summary":
                self.send_bytes(
                    json.dumps(state.reports, indent=2).encode("utf-8"),
                    "application/json; charset=utf-8",
                )
                return
            if parsed.path == "/report":
                query = parse_qs(parsed.query)
                report = {key: values[-1] for key, values in query.items()}
                state.add_report(report)
                self.send_bytes(b"ok\n", "text/plain; charset=utf-8")
                return
            self.send_error(404)

        def do_POST(self) -> None:
            parsed = urlparse(self.path)
            if parsed.path != "/report":
                self.send_error(404)
                return
            length = int(self.headers.get("Content-Length", "0") or "0")
            data = self.rfile.read(length)
            try:
                report = json.loads(data.decode("utf-8"))
            except json.JSONDecodeError:
                report = {"parse_error": data.decode("utf-8", errors="replace")}
            state.add_report(report)
            self.send_bytes(b"ok\n", "text/plain; charset=utf-8")

        def send_bytes(self, data: bytes, content_type: str) -> None:
            self.send_response(200)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    return Handler


def render_probe_page(probe: Probe) -> bytes:
    title = html.escape(probe.name)
    script = f"""
const probeName = {json.dumps(probe.name)};
async function sendReport(report) {{
  report.probe = probeName;
  report.reportedAt = new Date().toISOString();
  try {{
    await fetch('/report', {{
      method: 'POST',
      headers: {{'Content-Type': 'application/json'}},
      body: JSON.stringify(report),
      keepalive: true
    }});
  }} catch (error) {{
    new Image().src = '/report?probe=' + encodeURIComponent(probeName) +
      '&status=report_failed&error=' + encodeURIComponent(String(error));
  }}
}}
let completed = false;
let timeoutId = null;
async function finalReport(report) {{
  if (completed) return;
  completed = true;
  if (timeoutId !== null) clearTimeout(timeoutId);
  await sendReport(report);
}}
async function runProbe() {{
  try {{
    const detail = await (async () => {{
      {probe.script}
    }})();
    await finalReport({{ok: true, ...(detail || {{status: 'resolved'}})}});
  }} catch (error) {{
    await finalReport({{
      ok: false,
      status: 'exception',
      error: String(error),
      errorName: error?.name || null,
      stack: error?.stack || null
    }});
  }}
}}
timeoutId = setTimeout(() => finalReport({{ok: false, status: 'page_timeout'}}), 5000);
runProbe();
"""
    return f"""<!doctype html>
<meta charset="utf-8">
<title>Issue 799 probe: {title}</title>
<h1>Issue 799 probe: {title}</h1>
<script>
{script}
</script>
""".encode("utf-8")


def start_server(state: ProbeState) -> ReusableThreadingTcpServer:
    server = ReusableThreadingTcpServer(("127.0.0.1", 0), make_handler(state))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server


def timestamp() -> str:
    return dt.datetime.now(dt.timezone.utc).strftime("%Y%m%d-%H%M%S-%f")


def write_json(path: pathlib.Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def read_text(path: pathlib.Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except FileNotFoundError:
        return ""


def scan_logs(text: str) -> dict[str, Any]:
    missing = sorted(set(MISSING_INTERFACE_RE.findall(text)))
    empty = sorted(set(EMPTY_BINDER_RE.findall(text)))
    bad_mojo_lines = [
        line
        for line in text.splitlines()
        if any(pattern in line for pattern in BAD_MOJO_PATTERNS)
    ]
    empty_binder_lines = [
        line for line in text.splitlines() if "Empty binder for interface" in line
    ]
    crash_lines = [
        line for line in text.splitlines() if any(pattern in line for pattern in CRASH_PATTERNS)
    ]
    return {
        "bad_mojo": bool(bad_mojo_lines),
        "crashed": bool(crash_lines),
        "missing_interfaces": missing,
        "empty_interfaces": empty,
        "bad_mojo_lines": bad_mojo_lines,
        "empty_binder_lines": empty_binder_lines,
        "crash_lines": crash_lines,
    }


def classify_probe(report: dict[str, Any] | None, log_scan: dict[str, Any], proc_exit: int | None) -> str:
    if log_scan["missing_interfaces"]:
        return "missing_binder"
    if log_scan["bad_mojo"]:
        return "bad_mojo"
    if log_scan["crashed"]:
        return "renderer_or_browser_crash"
    if proc_exit is not None and proc_exit not in (0, -15):
        return "process_exit"
    if not report:
        return "no_report"
    status = str(report.get("status", "unknown"))
    error_text = str(report.get("error", ""))
    if "IPC connection" in error_text or "service in the browser process" in error_text:
        if log_scan["empty_interfaces"]:
            return "empty_binder"
        return "browser_service_unavailable"
    if status == "blocked_user_activation":
        return "blocked_user_activation"
    if status == "blocked_needs_virtual_authenticator":
        return "blocked_needs_virtual_authenticator"
    if status == "unsupported":
        return "unsupported"
    if status in ("resolved", "rejected"):
        return "exercised"
    if status == "exception":
        return "js_exception"
    if status == "page_timeout":
        return "page_timeout"
    if status == "probe_timeout":
        return "probe_timeout"
    return "reported"


def verify_download(
    download_dir: pathlib.Path,
    filename: str,
    expected_bytes: bytes,
    deadline: float,
) -> dict[str, Any]:
    target = download_dir / filename
    intermediate = download_dir / f"{filename}.crdownload"
    last_state = "missing"
    while time.time() < deadline:
        crdownloads = sorted(path.name for path in download_dir.glob("*.crdownload"))
        if target.exists() and not intermediate.exists() and not crdownloads:
            actual = target.read_bytes()
            actual_hash = hashlib.sha256(actual).hexdigest()
            expected_hash = hashlib.sha256(expected_bytes).hexdigest()
            return {
                "status": "completed" if actual == expected_bytes else "wrong_bytes",
                "path": str(target),
                "filename": filename,
                "size": len(actual),
                "sha256": actual_hash,
                "expected_size": len(expected_bytes),
                "expected_sha256": expected_hash,
            }
        if target.exists():
            last_state = "waiting_for_intermediate"
        elif crdownloads:
            last_state = "intermediate_only"
        time.sleep(0.1)
    crdownloads = sorted(path.name for path in download_dir.glob("*.crdownload"))
    return {
        "status": "timeout",
        "path": str(target),
        "filename": filename,
        "exists": target.exists(),
        "last_state": last_state,
        "crdownloads": crdownloads,
    }


def run_probe(
    probe: Probe,
    base_url: str,
    run_dir: pathlib.Path,
    roamium: pathlib.Path,
    download_dir: pathlib.Path,
    seconds: float,
    width: int,
    height: int,
) -> dict[str, Any]:
    probe_dir = run_dir / "probes" / probe.name
    probe_dir.mkdir(parents=True, exist_ok=True)
    socket_path = (
        pathlib.Path(tempfile.gettempdir())
        / f"ts799-{os.getpid()}-{hashlib.sha1(probe.name.encode()).hexdigest()[:8]}.sock"
    )
    try:
        socket_path.unlink()
    except FileNotFoundError:
        pass

    listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    listener.bind(str(socket_path))
    listener.listen(1)
    listener.settimeout(min(20.0, seconds))

    stdout_path = probe_dir / "roamium.stdout"
    stderr_path = probe_dir / "roamium.stderr"
    messages_path = probe_dir / "messages.log"
    stdout = stdout_path.open("wb")
    stderr = stderr_path.open("wb")
    url = f"{base_url}/probe/{probe.name}.html"
    expected_download = EXPECTED_DOWNLOADS.get(probe.name)
    if expected_download:
        expected_path = download_dir / expected_download[0]
        expected_path.unlink(missing_ok=True)
        expected_path.with_name(expected_path.name + ".crdownload").unlink(
            missing_ok=True
        )
    command = [
        str(roamium),
        f"--ipc-socket={socket_path}",
        f"--user-data-dir={probe_dir / 'profile'}",
        f"--termsurf-download-dir={download_dir}",
        "--no-sandbox",
        "--enable-logging=stderr",
        "--v=1",
    ]
    env = os.environ.copy()
    env["TERMSURF_PDF_INPUT_TRACE"] = "1"
    env["TERMSURF_PDF_INPUT_TRACE_FILE"] = str(probe_dir / "input-trace.log")
    proc = subprocess.Popen(
        command,
        cwd=str(ROOT / "chromium/src"),
        stdout=stdout,
        stderr=stderr,
        env=env,
    )

    sent_create = False
    tab_id: int | None = None
    socket_disconnect = False
    javascript_dialogs: list[dict[str, Any]] = []
    activation_sent = False
    activation_ready_at: float | None = None
    activation_sent_at: float | None = None
    activation_observed_at: float | None = None
    navigation_sent = False
    start = time.time()

    try:
        try:
            conn, _ = listener.accept()
            conn.settimeout(0.2)
        except socket.timeout:
            conn = None
        with messages_path.open("w", encoding="utf-8") as messages:
            while time.time() - start < seconds:
                if proc.poll() is not None:
                    break
                if conn is None:
                    time.sleep(0.1)
                    continue
                if probe.name.startswith("javascript-beforeunload-") and tab_id:
                    reports = state_reports_for_probe(run_dir, probe.name)
                    statuses = {str(report.get("status")) for report in reports}
                    if "ready" in statuses and activation_ready_at is None:
                        activation_ready_at = time.time()
                    if "activated" in statuses and activation_observed_at is None:
                        activation_observed_at = time.time()
                    if (
                        activation_ready_at is not None
                        and time.time() - activation_ready_at >= 0.5
                        and not activation_sent
                    ):
                        send_message(conn, 10, focus_changed_payload(tab_id, True))
                        send_message(conn, 7, mouse_move_payload(tab_id, 20, 20))
                        send_message(conn, 6, mouse_event_payload(tab_id, "down", 20, 20))
                        send_message(conn, 6, mouse_event_payload(tab_id, "up", 20, 20))
                        send_message(conn, 9, key_event_payload(tab_id, "down", 65, "a"))
                        send_message(conn, 9, key_event_payload(tab_id, "up", 65))
                        activation_sent = True
                        activation_sent_at = time.time()
                        messages.write("sent beforeunload refocus and activation input\n")
                        messages.flush()
                    if activation_observed_at is not None and not navigation_sent:
                        destination = (
                            f"{base_url}/beforeunload-destination.html?probe={probe.name}"
                        )
                        send_message(conn, 5, navigate_payload(tab_id, destination))
                        navigation_sent = True
                        messages.write(f"sent beforeunload Navigate url={destination}\n")
                        messages.flush()
                try:
                    header = conn.recv(4)
                    if not header:
                        socket_disconnect = True
                        break
                    size = struct.unpack("<I", header)[0]
                    payload = bytearray()
                    while len(payload) < size:
                        chunk = conn.recv(size - len(payload))
                        if not chunk:
                            socket_disconnect = True
                            break
                        payload.extend(chunk)
                    if socket_disconnect:
                        break
                    top, body = inner_payload(bytes(payload))
                    messages.write(f"t={time.time() - start:.3f} top_field={top}\n")
                    messages.flush()
                    if top == 12 and not sent_create:
                        send_message(conn, 1, create_tab_payload(url, width, height))
                        sent_create = True
                        messages.write(f"sent CreateTab url={url}\n")
                        messages.flush()
                    elif top == 13:
                        tab_id = tab_ready_id(body)
                        messages.write(f"tab_ready id={tab_id}\n")
                        if tab_id:
                            send_message(conn, 3, resize_payload(tab_id, width, height))
                            send_message(conn, 10, focus_changed_payload(tab_id, True))
                            messages.write("sent Resize\n")
                            messages.write("sent FocusChanged focused=true\n")
                        messages.flush()
                    elif top == 34:
                        fields = parse_message_fields(body)
                        request_tab_id = int(fields.get(1, 0) or 0)
                        request_id = int(fields.get(2, 0) or 0)
                        dialog_type = str(fields.get(3, ""))
                        origin_url = str(fields.get(4, ""))
                        message = str(fields.get(5, ""))
                        default_prompt_text = str(fields.get(6, ""))
                        accepted, prompt_text = dialog_response_for(probe.name)
                        send_message(
                            conn,
                            35,
                            javascript_dialog_reply_payload(
                                request_tab_id,
                                request_id,
                                accepted,
                                prompt_text,
                            ),
                        )
                        evidence = {
                            "request_time": time.time() - start,
                            "tab_id": request_tab_id,
                            "request_id": request_id,
                            "dialog_type": dialog_type,
                            "origin_url": origin_url,
                            "message": message,
                            "default_prompt_text": default_prompt_text,
                            "accepted": accepted,
                            "prompt_text": prompt_text,
                            "reply_time": time.time() - start,
                        }
                        javascript_dialogs.append(evidence)
                        messages.write(
                            "javascript_dialog "
                            f"type={dialog_type} request_id={request_id} "
                            f"accepted={accepted}\n"
                        )
                        messages.flush()
                except socket.timeout:
                    pass
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)
        stdout.close()
        stderr.close()
        listener.close()
        try:
            socket_path.unlink()
        except FileNotFoundError:
            pass

    stderr_text = read_text(stderr_path)
    stdout_text = read_text(stdout_path)
    log_scan = scan_logs(stderr_text + "\n" + stdout_text)
    reports = state_reports_for_probe(run_dir, probe.name)
    non_timeout_reports = [
        candidate for candidate in reports if candidate.get("status") != "page_timeout"
    ]
    report = (non_timeout_reports or reports)[-1] if reports else None
    proc_exit = proc.returncode
    classification = classify_probe(report, log_scan, proc_exit)
    download_result = None
    if expected_download:
        download_result = verify_download(
            download_dir,
            expected_download[0],
            expected_download[1],
            time.time() + 0.1,
        )
        if (
            classification
            not in (
                "missing_binder",
                "bad_mojo",
                "renderer_or_browser_crash",
                "process_exit",
            )
            and download_result["status"] == "completed"
        ):
            classification = "download_completed"
        elif classification == "exercised":
            classification = "download_failed"
    javascript_dialog_result = verify_javascript_dialog_probe(
        probe.name,
        report,
        javascript_dialogs,
        activation_observed_at is not None,
    )
    if javascript_dialog_result:
        if (
            classification
            not in (
                "missing_binder",
                "bad_mojo",
                "renderer_or_browser_crash",
                "process_exit",
            )
            and javascript_dialog_result["status"] == "completed"
        ):
            classification = "dialog_completed"
        elif classification == "exercised":
            classification = "dialog_failed"
    result = {
        "probe": probe.name,
        "feature": probe.feature,
        "url": url,
        "requires_user_activation": probe.requires_user_activation,
        "tab_id": tab_id,
        "sent_create_tab": sent_create,
        "socket_disconnect": socket_disconnect,
        "process_exit_code": proc_exit,
        "report": report,
        "page_reported": report is not None,
        "bad_mojo": log_scan["bad_mojo"],
        "crashed": log_scan["crashed"],
        "missing_interfaces": log_scan["missing_interfaces"],
        "empty_interfaces": log_scan["empty_interfaces"],
        "classification": classification,
        "download": download_result,
        "javascript_dialogs": javascript_dialogs,
        "javascript_dialog_result": javascript_dialog_result,
        "beforeunload_activation": (
            {
                "ready": activation_ready_at is not None,
                "input_sent": activation_sent,
                "activation_observed": activation_observed_at is not None,
                "navigation_sent": navigation_sent,
            }
            if probe.name.startswith("javascript-beforeunload-")
            else None
        ),
        "log_dir": str(probe_dir),
    }
    write_json(probe_dir / "probe-result.json", result)
    append_file(run_dir / "roamium.stdout", f"\n===== {probe.name} =====\n" + stdout_text)
    append_file(run_dir / "roamium.stderr", f"\n===== {probe.name} =====\n" + stderr_text)
    append_file(run_dir / "messages.log", f"\n===== {probe.name} =====\n" + read_text(messages_path))
    return result


def state_reports_for_probe(run_dir: pathlib.Path, probe: str) -> list[dict[str, Any]]:
    reports_path = run_dir / "reports.jsonl"
    reports: list[dict[str, Any]] = []
    if not reports_path.exists():
        return reports
    for line in reports_path.read_text(encoding="utf-8").splitlines():
        try:
            report = json.loads(line)
        except json.JSONDecodeError:
            continue
        if report.get("probe") == probe:
            reports.append(report)
    return reports


def append_file(path: pathlib.Path, text: str) -> None:
    with path.open("a", encoding="utf-8") as out:
        out.write(text)


def write_binder_errors(path: pathlib.Path, results: list[dict[str, Any]]) -> None:
    lines = ["probe\ttype\tinterface\n"]
    for result in results:
        for interface in result.get("missing_interfaces", []):
            lines.append(f"{result['probe']}\tmissing\t{interface}\n")
        for interface in result.get("empty_interfaces", []):
            lines.append(f"{result['probe']}\tempty\t{interface}\n")
    path.write_text("".join(lines), encoding="utf-8")


def actionable_empty_interfaces(result: dict[str, Any]) -> list[str]:
    if result.get("classification") != "empty_binder":
        return []
    return [
        interface
        for interface in result.get("empty_interfaces", [])
        if interface != "blink.mojom.LCPCriticalPathPredictorHost"
    ]


def write_coverage_map(path: pathlib.Path, results: list[dict[str, Any]]) -> None:
    lines = [
        "# Issue 799 Browser API Probe Coverage",
        "",
        "| Probe | Feature | Classification | JS status | Missing interface | Next action |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for result in results:
        report = result.get("report") or {}
        missing = ", ".join(result.get("missing_interfaces") or []) or "-"
        if result.get("empty_interfaces"):
            missing = "empty: " + ", ".join(result.get("empty_interfaces") or [])
        action_empty = ", ".join(actionable_empty_interfaces(result))
        classification = result["classification"]
        if classification == "missing_binder":
            next_action = "Design a narrow binder/stub experiment."
        elif classification == "empty_binder":
            next_action = (
                f"Replace empty binder with narrow TermSurf behavior or explicit denial: {action_empty}."
                if action_empty
                else "Review empty binder; no actionable non-ambient interface extracted."
            )
        elif classification == "blocked_user_activation":
            next_action = "Needs synthetic activation coverage before claiming binder safety."
        elif classification == "blocked_needs_virtual_authenticator":
            next_action = "Needs DevTools virtual authenticator coverage before claiming WebAuthn safety."
        elif classification == "browser_service_unavailable":
            next_action = "Browser service unavailable; inspect logs and reference binders."
        elif classification == "exercised":
            next_action = "No action from this probe; expand coverage if needed."
        elif classification == "download_completed":
            next_action = "Generic download completed with verified file bytes."
        elif classification == "download_failed":
            next_action = "Inspect download target selection and file evidence."
        elif classification == "dialog_completed":
            next_action = "JavaScript dialog completed through request/reply protocol."
        elif classification == "dialog_failed":
            next_action = "Inspect JavaScript dialog request/reply evidence."
        elif classification == "unsupported":
            next_action = "No runtime surface exposed in this build."
        else:
            next_action = "Investigate harness or browser behavior."
        lines.append(
            "| {probe} | {feature} | {classification} | {status} | {missing} | {next_action} |".format(
                probe=result["probe"],
                feature=result["feature"],
                classification=classification,
                status=report.get("status", "-"),
                missing=missing,
                next_action=next_action,
            )
        )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_reference_coverage_map(path: pathlib.Path, results: list[dict[str, Any]]) -> None:
    by_name = {result["probe"]: result for result in results}
    lines = [
        "# Issue 799 Reference Coverage Map",
        "",
        "| JS API / feature | Expected browser-side surface | Reference evidence | TermSurf evidence | Runtime probe status | Next action |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for probe in PROBES:
        result = by_name.get(probe.name, {})
        classification = result.get("classification", "not_run")
        missing = ", ".join(result.get("missing_interfaces") or [])
        empty = ", ".join(actionable_empty_interfaces(result))
        if missing:
            next_action = f"Fix missing interface: {missing}."
        elif empty and classification == "empty_binder":
            next_action = f"Replace empty binder or add explicit denial for: {empty}."
        elif classification == "blocked_user_activation":
            next_action = "Add contained user-activation probe before claiming coverage."
        elif classification == "blocked_needs_virtual_authenticator":
            next_action = "Add contained DevTools virtual-authenticator coverage before claiming coverage."
        elif classification == "download_completed":
            next_action = "No immediate action; verified generic download completion."
        elif classification in ("exercised", "unsupported"):
            next_action = "No immediate binder fix from this probe."
        else:
            next_action = "Investigate harness/runtime result."
        lines.append(
            "| {feature} | {surface} | {reference} | {termsurf} | {status} | {next_action} |".format(
                feature=probe.feature,
                surface=probe.expected_surface,
                reference=probe.reference_evidence,
                termsurf=probe.termsurf_evidence,
                status=classification,
                next_action=next_action,
            )
        )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def chromium_branch() -> str | None:
    try:
        proc = subprocess.run(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"],
            cwd=ROOT / "chromium/src",
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None
    return proc.stdout.strip()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--roamium", type=pathlib.Path, default=DEFAULT_ROAMIUM)
    parser.add_argument("--log-dir", type=pathlib.Path)
    parser.add_argument("--seconds", type=float, default=8.0)
    parser.add_argument("--width", type=int, default=1200)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument(
        "--probe",
        action="append",
        choices=[probe.name for probe in PROBES],
        help="Run only the named probe. May be passed more than once.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    roamium = args.roamium.resolve()
    if not roamium.exists():
        raise SystemExit(f"missing Roamium binary: {roamium}")

    run_dir = (args.log_dir or DEFAULT_LOG_ROOT / timestamp()).resolve()
    run_dir.mkdir(parents=True, exist_ok=True)
    download_dir = run_dir / "downloads"
    download_dir.mkdir(parents=True, exist_ok=True)
    state = ProbeState(run_dir)
    server = start_server(state)
    host, port = server.server_address
    base_url = f"http://localhost:{port}"
    selected = [probe for probe in PROBES if not args.probe or probe.name in args.probe]
    start = dt.datetime.now(dt.timezone.utc)

    run_info: dict[str, Any] = {
        "command": sys.argv,
        "roamium": str(roamium),
        "chromium_branch": chromium_branch(),
        "fixture_base_url": base_url,
        "download_dir": str(download_dir),
        "started_at": start.isoformat(),
        "probe_count": len(selected),
        "logging": {
            "flags": ["--enable-logging=stderr", "--v=1"],
            "stderr": str(run_dir / "roamium.stderr"),
            "stdout": str(run_dir / "roamium.stdout"),
            "bad_mojo_patterns": BAD_MOJO_PATTERNS,
        },
    }
    write_json(run_dir / "run.json", run_info)

    try:
        results = [
            run_probe(
                probe,
                base_url,
                run_dir,
                roamium,
                download_dir,
                args.seconds,
                args.width,
                args.height,
            )
            for probe in selected
        ]
    finally:
        server.shutdown()

    any_missing = any(
        result["missing_interfaces"] or result["classification"] == "empty_binder"
        for result in results
    )
    any_crash = any(result["crashed"] for result in results)
    run_status = "missing_binder" if any_missing else "crash" if any_crash else "completed"
    run_info.update(
        {
            "finished_at": dt.datetime.now(dt.timezone.utc).isoformat(),
            "status": run_status,
            "classifications": {
                result["probe"]: result["classification"] for result in results
            },
            "missing_interfaces": sorted(
                {
                    interface
                    for result in results
                    for interface in result.get("missing_interfaces", [])
                }
            ),
            "empty_interfaces": sorted(
                {
                    interface
                    for result in results
                    for interface in result.get("empty_interfaces", [])
                }
            ),
        }
    )
    write_json(run_dir / "run.json", run_info)
    write_json(run_dir / "probe-results.json", results)
    write_binder_errors(run_dir / "binder-errors.tsv", results)
    write_coverage_map(run_dir / "coverage-map.md", results)
    write_reference_coverage_map(run_dir / "reference-coverage-map.md", results)

    print(run_dir)
    print(json.dumps(run_info, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
