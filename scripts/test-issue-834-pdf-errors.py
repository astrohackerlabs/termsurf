#!/usr/bin/env python3
"""Probe Roamium malformed PDF handling through the TermSurf protocol."""

from __future__ import annotations

import argparse
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
ERROR_PROBE = ROOT / "scripts/probe-pdf-error.mjs"
DEVTOOLS_RE = re.compile(r"DevTools listening on ws://127\.0\.0\.1:(\d+)/")


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


class ErrorPdfHandler(http.server.BaseHTTPRequestHandler):
    log_dir: pathlib.Path
    fixtures: dict[str, dict[str, Any]]
    requests: list[dict[str, Any]]

    def log_message(self, fmt: str, *args: object) -> None:
        with (self.log_dir / "http.log").open("a", encoding="utf-8") as log:
            log.write((fmt % args) + "\n")

    def do_GET(self) -> None:
        request_path = self.path.split("?", 1)[0]
        fixture = self.fixtures.get(request_path)
        if not fixture:
            self.requests.append(
                {
                    "path": request_path,
                    "status": 404,
                    "content_type": None,
                    "bytes": 0,
                }
            )
            self.send_response(404)
            self.end_headers()
            return
        data = pathlib.Path(fixture["path"]).read_bytes()
        self.requests.append(
            {
                "path": request_path,
                "status": 200,
                "content_type": fixture["content_type"],
                "bytes": len(data),
                "kind": fixture["kind"],
            }
        )
        self.send_response(200)
        self.send_header("Content-Type", fixture["content_type"])
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
    probe_status: str = "not-run"
    roamium_exited_before_shutdown: bool = False
    roamium_exit_code_before_shutdown: int | None = None
    first_failing_hop: str = "automation-gap"


def read_text(path: pathlib.Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except FileNotFoundError:
        return ""


def prepare_fixtures(log_dir: pathlib.Path) -> dict[str, dict[str, Any]]:
    fixtures_dir = log_dir / "fixtures"
    fixtures_dir.mkdir(parents=True, exist_ok=True)
    valid = fixtures_dir / "valid.pdf"
    truncated = fixtures_dir / "truncated-header.pdf"
    not_pdf = fixtures_dir / "not-pdf.pdf"
    empty = fixtures_dir / "empty.pdf"
    valid.write_bytes(BITCOIN_PDF.read_bytes())
    truncated.write_bytes(b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog\n")
    not_pdf.write_text(
        "This response is deliberately not a PDF, despite its content type.\n",
        encoding="utf-8",
    )
    empty.write_bytes(b"")
    fixtures = {
        "/valid.pdf": {
            "kind": "valid-control",
            "path": str(valid),
            "content_type": "application/pdf",
            "bytes": valid.stat().st_size,
        },
        "/truncated-header.pdf": {
            "kind": "truncated-header",
            "path": str(truncated),
            "content_type": "application/pdf",
            "bytes": truncated.stat().st_size,
        },
        "/not-pdf.pdf": {
            "kind": "not-pdf",
            "path": str(not_pdf),
            "content_type": "application/pdf",
            "bytes": not_pdf.stat().st_size,
        },
        "/empty.pdf": {
            "kind": "empty-response",
            "path": str(empty),
            "content_type": "application/pdf",
            "bytes": empty.stat().st_size,
        },
    }
    (log_dir / "fixtures.json").write_text(
        json.dumps(fixtures, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return fixtures


def start_pdf_server(
    log_dir: pathlib.Path,
    port: int,
    fixtures: dict[str, dict[str, Any]],
) -> socketserver.TCPServer:
    ErrorPdfHandler.log_dir = log_dir
    ErrorPdfHandler.fixtures = fixtures
    ErrorPdfHandler.requests = []
    server = ReusableTcpServer(("127.0.0.1", port), ErrorPdfHandler)
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


def run_error_probe(
    log_dir: pathlib.Path,
    devtools_port: int,
    url_contains: str,
    probe: str,
    initial_label: str,
    navigations: list[dict[str, str]],
    timeout_seconds: int,
    settle_seconds: int,
) -> tuple[str, str, pathlib.Path]:
    out_dir = log_dir / "devtools"
    out_dir.mkdir(parents=True, exist_ok=True)
    proc = subprocess.run(
        [
            "node",
            str(ERROR_PROBE),
            "--devtools-port",
            str(devtools_port),
            "--url-contains",
            url_contains,
            "--out-dir",
            str(out_dir),
            "--probe",
            probe,
            "--initial-label",
            initial_label,
            "--navigations-json",
            json.dumps(navigations),
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
        out_dir / "pdf-error-devtools-summary.json",
    )


def load_json(path: pathlib.Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def fixture_urls(
    server: socketserver.TCPServer,
    paths: list[str],
) -> list[dict[str, str]]:
    host, port = server.server_address
    return [
        {"label": pathlib.Path(path).stem, "url": f"http://{host}:{port}{path}"}
        for path in paths
    ]


def malformed_snapshots(devtools_summary: dict[str, Any] | None) -> list[dict[str, Any]]:
    if not devtools_summary:
        return []
    snapshots = []
    for snapshot in devtools_summary.get("snapshots") or []:
        label = snapshot.get("label", "")
        if label == "valid-control" or label.endswith("-navigate-result"):
            continue
        snapshots.append(snapshot)
    return snapshots


def probe_malformed_snapshots(
    args: argparse.Namespace,
    devtools_summary: dict[str, Any] | None,
) -> list[dict[str, Any]]:
    if not devtools_summary:
        return []
    snapshots = devtools_summary.get("snapshots") or []
    if args.probe == "malformed-fixtures":
        return [
            snapshot
            for snapshot in snapshots
            if not str(snapshot.get("label", "")).endswith("-navigate-result")
        ]
    if args.probe == "valid-to-malformed-same-tab":
        return malformed_snapshots(devtools_summary)
    return []


def snapshot_loaded(snapshot: dict[str, Any] | None) -> bool:
    return bool(snapshot and snapshot.get("pluginLoaded"))


def snapshot_error_evidence(snapshot: dict[str, Any] | None) -> bool:
    return bool(snapshot and snapshot.get("errorEvidence"))


def snapshot_malformed_outcome(snapshot: dict[str, Any] | None) -> str:
    if not snapshot:
        return "missing"
    return str(snapshot.get("malformedOutcome") or "unclassified")


def snapshot_url(snapshot: dict[str, Any] | None) -> str:
    if not snapshot:
        return ""
    values = snapshot.get("values") or []
    if not values:
        return ""
    return str(values[0].get("value", {}).get("url", ""))


def classify(
    args: argparse.Namespace,
    state: HarnessState,
    devtools_summary: dict[str, Any] | None,
) -> None:
    if not state.server_register_received:
        state.first_failing_hop = "roamium-not-registered"
    elif not state.tab_ready_id:
        state.first_failing_hop = "tab-not-ready"
    elif not state.resize_sent:
        state.first_failing_hop = "resize-not-sent"
    elif not state.devtools_port:
        state.first_failing_hop = "devtools-missing"
    elif state.roamium_exited_before_shutdown:
        state.first_failing_hop = "roamium-crashed-or-exited"
    elif state.probe_status != "ok" or not devtools_summary:
        state.first_failing_hop = "pdf-viewer-error-state-discovery"
    elif devtools_summary.get("status") != "pass":
        state.first_failing_hop = devtools_summary.get(
            "firstFailingHop",
            "devtools-probe-error",
        )
    elif args.probe == "valid-control":
        initial = (devtools_summary.get("snapshots") or [{}])[0]
        if not snapshot_loaded(initial):
            state.first_failing_hop = "valid-control-not-loaded"
        else:
            state.first_failing_hop = "no-failure-observed"
    elif args.probe == "malformed-fixtures":
        malformed = probe_malformed_snapshots(args, devtools_summary)
        loaded = [snapshot.get("label") for snapshot in malformed if snapshot_loaded(snapshot)]
        unclassified = [
            snapshot.get("label")
            for snapshot in malformed
            if snapshot_malformed_outcome(snapshot) == "unclassified"
        ]
        if loaded:
            state.first_failing_hop = "malformed-reported-normal-success"
        elif not malformed:
            state.first_failing_hop = "evidence-collection-gap"
        elif unclassified:
            state.first_failing_hop = "malformed-outcome-unclassified"
        else:
            state.first_failing_hop = "no-failure-observed"
    elif args.probe == "valid-to-malformed-same-tab":
        snapshots = devtools_summary.get("snapshots") or []
        initial = snapshots[0] if snapshots else None
        malformed = malformed_snapshots(devtools_summary)
        stale_loaded = [
            snapshot.get("label")
            for snapshot in malformed
            if snapshot_loaded(snapshot) or snapshot_url(snapshot).endswith("/valid.pdf")
        ]
        unclassified = [
            snapshot.get("label")
            for snapshot in malformed
            if snapshot_malformed_outcome(snapshot) == "unclassified"
        ]
        if not snapshot_loaded(initial):
            state.first_failing_hop = "valid-control-not-loaded"
        elif not malformed:
            state.first_failing_hop = "evidence-collection-gap"
        elif stale_loaded:
            state.first_failing_hop = "stale-success-state-after-malformed"
        elif unclassified:
            state.first_failing_hop = "malformed-outcome-unclassified"
        else:
            state.first_failing_hop = "no-failure-observed"


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
        "probe_status": state.probe_status,
        "roamium_exited_before_shutdown": state.roamium_exited_before_shutdown,
        "roamium_exit_code_before_shutdown": state.roamium_exit_code_before_shutdown,
        "first_failing_hop": state.first_failing_hop,
    }
    data.update(extra)
    (log_dir / "pdf-error-summary.json").write_text(
        json.dumps(data, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--log-dir", required=True)
    parser.add_argument(
        "--probe",
        choices=[
            "valid-control",
            "malformed-fixtures",
            "valid-to-malformed-same-tab",
        ],
        required=True,
    )
    parser.add_argument("--width", type=int, default=1200)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--pdf-port", type=int, default=0)
    parser.add_argument("--setup-timeout", type=float, default=30)
    parser.add_argument("--capture-timeout-seconds", type=int, default=30)
    parser.add_argument("--settle-seconds", type=int, default=3)
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
    if not ERROR_PROBE.exists():
        raise SystemExit(f"missing error probe: {ERROR_PROBE}")

    fixtures = prepare_fixtures(log_dir)
    pdf_server = start_pdf_server(log_dir, args.pdf_port, fixtures)
    host, port = pdf_server.server_address
    malformed_paths = ["/truncated-header.pdf", "/not-pdf.pdf", "/empty.pdf"]

    if args.probe == "valid-control":
        initial_path = "/valid.pdf"
        initial_label = "valid-control"
        navigations: list[dict[str, str]] = []
    elif args.probe == "malformed-fixtures":
        initial_path = malformed_paths[0]
        initial_label = pathlib.Path(initial_path).stem
        navigations = fixture_urls(pdf_server, malformed_paths[1:])
    else:
        initial_path = "/valid.pdf"
        initial_label = "valid-control"
        navigations = fixture_urls(pdf_server, malformed_paths)

    url = f"http://{host}:{port}{initial_path}"
    url_contains = pathlib.Path(initial_path).name
    socket_path = log_dir / "gui.sock"
    try:
        socket_path.unlink()
    except FileNotFoundError:
        pass

    state = HarnessState()
    extra: dict[str, Any] = {
        "url": url,
        "url_contains": url_contains,
        "fixtures": fixtures,
        "http_server": {"host": host, "port": port},
        "navigations": navigations,
    }
    listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    listener.bind(str(socket_path))
    listener.listen(1)
    listener.settimeout(args.setup_timeout)

    stdout = (log_dir / "roamium.stdout").open("wb")
    stderr = (log_dir / "roamium.stderr").open("wb")
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
        env=os.environ.copy(),
    )

    conn: socket.socket | None = None
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
        devtools_summary = None
        if state.devtools_port:
            state.probe_status, probe_error, probe_path = run_error_probe(
                log_dir,
                state.devtools_port,
                url_contains,
                args.probe,
                initial_label,
                navigations,
                args.capture_timeout_seconds,
                args.settle_seconds,
            )
            extra["probe_error"] = probe_error
            devtools_summary = load_json(probe_path) if probe_path.exists() else None
            extra["devtools_summary"] = devtools_summary

        exit_code = proc.poll()
        state.roamium_exited_before_shutdown = exit_code is not None
        state.roamium_exit_code_before_shutdown = exit_code
        if devtools_summary:
            if args.probe == "valid-control":
                malformed_snapshot_count = 0
            elif args.probe == "valid-to-malformed-same-tab":
                malformed_snapshot_count = len(malformed_snapshots(devtools_summary))
            else:
                malformed_snapshot_count = len(
                    [
                        snapshot
                        for snapshot in devtools_summary.get("snapshots", [])
                        if not str(snapshot.get("label", "")).endswith(
                            "-navigate-result"
                        )
                    ]
                )
            extra["row_results"] = {
                "valid_control_loaded": snapshot_loaded(
                    (devtools_summary.get("snapshots") or [{}])[0]
                )
                if args.probe in ("valid-control", "valid-to-malformed-same-tab")
                else None,
                "malformed_snapshot_count": malformed_snapshot_count,
                "malformed_loaded_labels": [
                    snapshot.get("label")
                    for snapshot in probe_malformed_snapshots(args, devtools_summary)
                    if snapshot_loaded(snapshot)
                ],
                "malformed_error_evidence_labels": [
                    snapshot.get("label")
                    for snapshot in probe_malformed_snapshots(args, devtools_summary)
                    if snapshot_error_evidence(snapshot)
                ],
                "malformed_outcomes": {
                    snapshot.get("label"): snapshot_malformed_outcome(snapshot)
                    for snapshot in probe_malformed_snapshots(args, devtools_summary)
                },
            }
        extra["http_requests"] = ErrorPdfHandler.requests
        classify(args, state, devtools_summary)
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
