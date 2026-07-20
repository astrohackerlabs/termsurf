#!/usr/bin/env python3
"""Local browser identity probe for TermSurf engine parity work."""

from __future__ import annotations

import argparse
import datetime as dt
import http.client
import json
import os
import re
import socket
import sys
import tempfile
import threading
import time
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any


IDENTITY_HEADERS = [
    "user-agent",
    "sec-ch-ua",
    "sec-ch-ua-full-version-list",
    "sec-ch-ua-platform",
    "sec-ch-ua-platform-version",
    "sec-ch-ua-arch",
    "sec-ch-ua-bitness",
    "sec-ch-ua-mobile",
    "accept-language",
]

ACCEPT_CH = (
    "Sec-CH-UA, Sec-CH-UA-Full-Version-List, Sec-CH-UA-Platform, "
    "Sec-CH-UA-Platform-Version, Sec-CH-UA-Arch, Sec-CH-UA-Bitness, "
    "Sec-CH-UA-Mobile, Sec-CH-UA-Model, Sec-CH-UA-WoW64"
)

PERMISSIONS_POLICY = (
    "ch-ua=*, ch-ua-full-version-list=*, ch-ua-platform=*, "
    "ch-ua-platform-version=*, ch-ua-arch=*, ch-ua-bitness=*, "
    "ch-ua-mobile=*, ch-ua-model=*, ch-ua-wow64=*"
)

PAGE = r"""<!doctype html>
<meta charset="utf-8">
<title>TermSurf Browser Identity Probe</title>
<style>
  body { font: 15px system-ui, sans-serif; max-width: 980px; margin: 32px auto; }
  pre { white-space: pre-wrap; border: 1px solid #ccc; padding: 12px; }
</style>
<h1>TermSurf Browser Identity Probe</h1>
<p id="status">Collecting browser identity...</p>
<pre id="output"></pre>
<script>
async function safe(name, fn) {
  try {
    return { ok: true, value: await fn() };
  } catch (error) {
    return { ok: false, error: String(error && error.message || error) };
  }
}

function storageAvailable(kind) {
  try {
    const storage = window[kind];
    const key = "__termsurf_identity_probe__";
    storage.setItem(key, "1");
    const value = storage.getItem(key);
    storage.removeItem(key);
    return { supported: value === "1" };
  } catch (error) {
    return { supported: false, error: String(error && error.message || error) };
  }
}

function webglInfo() {
  try {
    const canvas = document.createElement("canvas");
    const gl = canvas.getContext("webgl") || canvas.getContext("experimental-webgl");
    if (!gl) return { supported: false };
    const debug = gl.getExtension("WEBGL_debug_renderer_info");
    return {
      supported: true,
      vendor: gl.getParameter(gl.VENDOR),
      renderer: gl.getParameter(gl.RENDERER),
      unmaskedVendor: debug ? gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) : null,
      unmaskedRenderer: debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : null,
    };
  } catch (error) {
    return { supported: false, error: String(error && error.message || error) };
  }
}

async function userAgentDataInfo() {
  const data = navigator.userAgentData;
  if (!data) return { supported: false };
  const out = {
    supported: true,
    brands: data.brands || null,
    mobile: data.mobile,
    platform: data.platform || null,
    highEntropy: null,
  };
  if (typeof data.getHighEntropyValues === "function") {
    out.highEntropy = await safe("getHighEntropyValues", () =>
      data.getHighEntropyValues([
        "architecture",
        "bitness",
        "brands",
        "fullVersionList",
        "mobile",
        "model",
        "platform",
        "platformVersion",
        "uaFullVersion",
        "wow64",
      ])
    );
  }
  return out;
}

async function collect() {
  document.cookie = "termsurf_identity_probe=1; SameSite=Lax";
  const headersProbe = await safe("headersProbe", async () => {
    const response = await fetch("/headers-probe?phase=after-accept-ch", {
      cache: "no-store",
      credentials: "same-origin",
    });
    return await response.json();
  });
  const report = {
    capturedAt: new Date().toISOString(),
    location: window.location.href,
    headersProbe,
    navigator: {
      userAgent: navigator.userAgent,
      userAgentData: await userAgentDataInfo(),
      platform: navigator.platform,
      vendor: navigator.vendor,
      webdriver: navigator.webdriver,
      languages: navigator.languages,
      language: navigator.language,
      cookieEnabled: navigator.cookieEnabled,
      hardwareConcurrency: navigator.hardwareConcurrency,
      maxTouchPoints: navigator.maxTouchPoints,
    },
    document: {
      cookie: document.cookie,
    },
    storage: {
      localStorage: storageAvailable("localStorage"),
      sessionStorage: storageAvailable("sessionStorage"),
    },
    webgl: webglInfo(),
  };
  await fetch("/report", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(report),
  });
  document.getElementById("status").textContent = "Captured.";
  document.getElementById("output").textContent = JSON.stringify(report, null, 2);
}

collect().catch((error) => {
  document.getElementById("status").textContent = "Capture failed.";
  document.getElementById("output").textContent = String(error && error.stack || error);
});
</script>
"""


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def sanitize_label(label: str) -> str:
    value = re.sub(r"[^A-Za-z0-9_.-]+", "-", label.strip())
    return value.strip("-") or "capture"


def header_dict(handler: BaseHTTPRequestHandler) -> dict[str, str]:
    return {key: value for key, value in handler.headers.items()}


def highlighted(headers: dict[str, str]) -> dict[str, str | None]:
    lower = {key.lower(): value for key, value in headers.items()}
    return {name: lower.get(name) for name in IDENTITY_HEADERS}


class ProbeState:
    def __init__(self, label: str, host: str, port: int, output_dir: Path):
        self.label = label
        self.host = host
        self.port = port
        self.output_dir = output_dir
        self.started_at = utc_now()
        self.finished_at: str | None = None
        self.initial_requests: list[dict[str, Any]] = []
        self.headers_probe_requests: list[dict[str, Any]] = []
        self.client_reports: list[dict[str, Any]] = []
        self.lock = threading.Lock()
        self.report_event = threading.Event()

    @property
    def url(self) -> str:
        return f"http://{self.host}:{self.port}/"

    def add_request(self, phase: str, handler: BaseHTTPRequestHandler) -> None:
        record = {
            "timestamp": utc_now(),
            "phase": phase,
            "method": handler.command,
            "path": handler.path,
            "headers": header_dict(handler),
        }
        record["identity_headers"] = highlighted(record["headers"])
        with self.lock:
            if phase == "initial":
                self.initial_requests.append(record)
            elif phase == "headers-probe":
                self.headers_probe_requests.append(record)

    def add_report(self, payload: dict[str, Any]) -> None:
        with self.lock:
            self.client_reports.append(
                {
                    "timestamp": utc_now(),
                    "phase": "client-report",
                    "payload": payload,
                }
            )
        self.report_event.set()

    def to_json(self) -> dict[str, Any]:
        with self.lock:
            initial = list(self.initial_requests)
            probes = list(self.headers_probe_requests)
            reports = list(self.client_reports)
        latest_report = reports[-1]["payload"] if reports else {}
        navigator = latest_report.get("navigator", {})
        ua_data = navigator.get("userAgentData", {})
        high_entropy = ua_data.get("highEntropy")
        summary = {
            "initial_identity_headers": initial[-1]["identity_headers"] if initial else {},
            "post_accept_ch_identity_headers": probes[-1]["identity_headers"] if probes else {},
            "navigator_user_agent": navigator.get("userAgent"),
            "navigator_vendor": navigator.get("vendor"),
            "navigator_platform": navigator.get("platform"),
            "navigator_webdriver": navigator.get("webdriver"),
            "navigator_languages": navigator.get("languages"),
            "navigator_user_agent_data": ua_data,
            "navigator_user_agent_data_high_entropy": high_entropy,
            "webgl": latest_report.get("webgl"),
        }
        return {
            "schema_version": 1,
            "label": self.label,
            "started_at": self.started_at,
            "finished_at": self.finished_at,
            "url": self.url,
            "initial_requests": initial,
            "headers_probe_requests": probes,
            "client_reports": reports,
            "summary": summary,
        }


def make_handler(state: ProbeState):
    class Handler(BaseHTTPRequestHandler):
        server_version = "TermSurfIdentityProbe/1"

        def log_message(self, fmt: str, *args: Any) -> None:
            return

        def send_probe_headers(self, status: HTTPStatus, content_type: str) -> None:
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Cache-Control", "no-store")
            self.send_header("Accept-CH", ACCEPT_CH)
            self.send_header("Critical-CH", ACCEPT_CH)
            self.send_header("Permissions-Policy", PERMISSIONS_POLICY)

        def do_GET(self) -> None:
            if self.path == "/" or self.path.startswith("/?"):
                state.add_request("initial", self)
                body = PAGE.encode("utf-8")
                self.send_probe_headers(HTTPStatus.OK, "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return

            if self.path.startswith("/headers-probe"):
                state.add_request("headers-probe", self)
                payload = {"ok": True, "capturedAt": utc_now()}
                body = json.dumps(payload, sort_keys=True).encode("utf-8")
                self.send_probe_headers(HTTPStatus.OK, "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return

            self.send_error(HTTPStatus.NOT_FOUND)

        def do_POST(self) -> None:
            if self.path != "/report":
                self.send_error(HTTPStatus.NOT_FOUND)
                return
            length = int(self.headers.get("Content-Length", "0"))
            raw = self.rfile.read(length)
            try:
                payload = json.loads(raw.decode("utf-8"))
            except json.JSONDecodeError as error:
                self.send_error(HTTPStatus.BAD_REQUEST, str(error))
                return
            state.add_report(payload)
            body = b'{"ok":true}\n'
            self.send_probe_headers(HTTPStatus.OK, "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    return Handler


def free_port(host: str) -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind((host, 0))
        return int(sock.getsockname()[1])


def output_path(output_dir: Path, label: str) -> Path:
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%d-%H%M%S")
    return output_dir / f"{stamp}-{sanitize_label(label)}.json"


def write_capture(state: ProbeState, path: Path) -> None:
    state.finished_at = utc_now()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(state.to_json(), indent=2, sort_keys=True) + "\n")


def print_summary(capture: dict[str, Any], path: Path) -> None:
    summary = capture["summary"]
    print(f"capture={path}")
    print(f"label={capture['label']}")
    print(f"url={capture['url']}")
    print("initial_headers=" + json.dumps(summary["initial_identity_headers"], sort_keys=True))
    print(
        "post_accept_ch_headers="
        + json.dumps(summary["post_accept_ch_identity_headers"], sort_keys=True)
    )
    print(f"navigator.userAgent={summary.get('navigator_user_agent')}")
    print(f"navigator.vendor={summary.get('navigator_vendor')}")
    print(f"navigator.platform={summary.get('navigator_platform')}")
    print(f"navigator.webdriver={summary.get('navigator_webdriver')}")
    ua_data = summary.get("navigator_user_agent_data")
    print("navigator.userAgentData=" + json.dumps(ua_data, sort_keys=True))


def validate_capture_schema(capture: dict[str, Any]) -> None:
    for key in [
        "schema_version",
        "label",
        "started_at",
        "finished_at",
        "url",
        "initial_requests",
        "headers_probe_requests",
        "client_reports",
        "summary",
    ]:
        if key not in capture:
            raise AssertionError(f"missing top-level key: {key}")
    if not capture["initial_requests"]:
        raise AssertionError("missing initial request")
    if not capture["headers_probe_requests"]:
        raise AssertionError("missing post-Accept-CH headers probe request")
    if not capture["client_reports"]:
        raise AssertionError("missing client report")
    summary = capture["summary"]
    for key in [
        "initial_identity_headers",
        "post_accept_ch_identity_headers",
        "navigator_user_agent",
        "navigator_user_agent_data",
        "navigator_user_agent_data_high_entropy",
        "webgl",
    ]:
        if key not in summary:
            raise AssertionError(f"missing summary key: {key}")
    for header in [
        "user-agent",
        "sec-ch-ua",
        "sec-ch-ua-full-version-list",
        "sec-ch-ua-platform",
        "accept-language",
    ]:
        if header not in summary["post_accept_ch_identity_headers"]:
            raise AssertionError(f"missing highlighted header: {header}")
        if summary["post_accept_ch_identity_headers"][header] is None:
            raise AssertionError(f"missing highlighted header value: {header}")
    high_entropy = summary["navigator_user_agent_data_high_entropy"]
    if not isinstance(high_entropy, dict) or high_entropy.get("ok") is not True:
        raise AssertionError("missing successful high-entropy userAgentData")
    high_entropy_value = high_entropy.get("value")
    if not isinstance(high_entropy_value, dict):
        raise AssertionError("missing high-entropy userAgentData value")
    if not high_entropy_value.get("fullVersionList"):
        raise AssertionError("missing high-entropy fullVersionList")
    if not high_entropy_value.get("platform"):
        raise AssertionError("missing high-entropy platform")


def run_server(label: str, host: str, port: int, output_dir: Path, timeout: float) -> Path:
    if port == 0:
        port = free_port(host)
    state = ProbeState(label, host, port, output_dir)
    server = ThreadingHTTPServer((host, port), make_handler(state))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    path = output_path(output_dir, label)
    try:
        print(f"Open this URL in the target browser: {state.url}", flush=True)
        state.report_event.wait(timeout=timeout)
        write_capture(state, path)
        print_summary(state.to_json(), path)
        return path
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


def request(
    host: str,
    port: int,
    method: str,
    path: str,
    body: bytes | None = None,
    headers: dict[str, str] | None = None,
) -> tuple[int, bytes]:
    conn = http.client.HTTPConnection(host, port, timeout=5)
    try:
        conn.request(method, path, body=body, headers=headers or {})
        response = conn.getresponse()
        return response.status, response.read()
    finally:
        conn.close()


def run_self_test() -> None:
    host = "127.0.0.1"
    port = free_port(host)
    with tempfile.TemporaryDirectory(prefix="termsurf-identity-probe.") as temp:
        output_dir = Path(temp)
        state = ProbeState("self-test", host, port, output_dir)
        server = ThreadingHTTPServer((host, port), make_handler(state))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            status, _ = request(
                host,
                port,
                "GET",
                "/",
                headers={
                    "User-Agent": "SelfTest Chromium",
                    "Sec-CH-UA": '"Chromium";v="148"',
                    "Sec-CH-UA-Platform": '"macOS"',
                    "Accept-Language": "en-US,en;q=0.9",
                },
            )
            if status != HTTPStatus.OK:
                raise AssertionError(f"initial GET failed: {status}")
            status, _ = request(
                host,
                port,
                "GET",
                "/headers-probe?phase=after-accept-ch",
                headers={
                    "User-Agent": "SelfTest Chromium",
                    "Sec-CH-UA": '"Chromium";v="148"',
                    "Sec-CH-UA-Full-Version-List": '"Chromium";v="148.0.7778.271"',
                    "Sec-CH-UA-Platform": '"macOS"',
                    "Accept-Language": "en-US,en;q=0.9",
                },
            )
            if status != HTTPStatus.OK:
                raise AssertionError(f"headers probe GET failed: {status}")
            client_payload = {
                "navigator": {
                    "userAgent": "SelfTest Chromium",
                    "vendor": "Google Inc.",
                    "platform": "MacIntel",
                    "webdriver": False,
                    "languages": ["en-US", "en"],
                    "userAgentData": {
                        "supported": True,
                        "brands": [{"brand": "Chromium", "version": "148"}],
                        "mobile": False,
                        "platform": "macOS",
                        "highEntropy": {
                            "ok": True,
                            "value": {
                                "fullVersionList": [
                                    {"brand": "Chromium", "version": "148.0.7778.271"}
                                ],
                                "platform": "macOS",
                                "architecture": "arm",
                            },
                        },
                    },
                },
                "storage": {
                    "localStorage": {"supported": True},
                    "sessionStorage": {"supported": True},
                },
                "webgl": {"supported": True, "vendor": "SelfTest", "renderer": "SelfTest"},
            }
            status, _ = request(
                host,
                port,
                "POST",
                "/report",
                body=json.dumps(client_payload).encode("utf-8"),
                headers={"Content-Type": "application/json"},
            )
            if status != HTTPStatus.OK:
                raise AssertionError(f"report POST failed: {status}")
            path = output_path(output_dir, "self-test")
            write_capture(state, path)
            capture = json.loads(path.read_text())
            validate_capture_schema(capture)
            print_summary(capture, path)
            print("PASS: browser identity probe self-test")
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--label", default="manual", help="capture label")
    parser.add_argument("--host", default="127.0.0.1", help="bind host")
    parser.add_argument("--port", type=int, default=0, help="bind port, 0 for auto")
    parser.add_argument("--timeout", type=float, default=60.0, help="seconds to wait")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("logs/browser-identity-probe"),
        help="directory for JSON captures",
    )
    parser.add_argument("--self-test", action="store_true", help="run non-GUI self-test")
    args = parser.parse_args(argv)

    if args.self_test:
        run_self_test()
    else:
        run_server(args.label, args.host, args.port, args.output_dir, args.timeout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
