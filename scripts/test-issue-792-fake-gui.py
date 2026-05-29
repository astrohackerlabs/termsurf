#!/usr/bin/env python3
"""Run Roamium against a minimal TermSurf GUI socket.

This harness is intentionally narrow: it proves browser-side Roamium/Chromium
plumbing without needing a real Wezboard pane or terminal PTY. It accepts
Roamium's ServerRegister message, sends CreateTab, records returned protobuf
message kinds, and optionally serves the local bitcoin.pdf fixture with an
explicit application/pdf MIME type.
"""

from __future__ import annotations

import argparse
import http.server
import os
import pathlib
import socket
import socketserver
import struct
import subprocess
import sys
import threading
import time


ROOT = pathlib.Path(__file__).resolve().parents[1]
ROAMIUM = ROOT / "chromium/src/out/Default/roamium"
BITCOIN_PDF = ROOT / "test-html/public/bitcoin.pdf"


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


def create_tab_payload(url: str, width: int, height: int) -> bytes:
    return (
        string_field(1, url)
        + string_field(2, "fake-pane")
        + varint_field(3, width)
        + varint_field(4, height)
        + bool_field(5, False)
    )


def resize_payload(tab_id: int, width: int, height: int) -> bytes:
    return varint_field(1, tab_id) + varint_field(2, width) + varint_field(3, height)


class ReusableTcpServer(socketserver.TCPServer):
    allow_reuse_address = True


class PdfHandler(http.server.BaseHTTPRequestHandler):
    log_dir: pathlib.Path

    def log_message(self, fmt: str, *args: object) -> None:
        with (self.log_dir / "http.log").open("a", encoding="utf-8") as log:
            log.write((fmt % args) + "\n")

    def do_GET(self) -> None:
        if self.path.split("?", 1)[0] != "/bitcoin.pdf":
            self.send_response(404)
            self.end_headers()
            return

        data = BITCOIN_PDF.read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", "application/pdf")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def start_pdf_server(log_dir: pathlib.Path, port: int) -> socketserver.TCPServer:
    PdfHandler.log_dir = log_dir
    try:
        server = ReusableTcpServer(("127.0.0.1", port), PdfHandler)
    except OSError as err:
        (log_dir / "http-server.log").write_text(
            f"failed to bind 127.0.0.1:{port}: {err}\n",
            encoding="utf-8",
        )
        raise SystemExit(f"failed to bind PDF fixture server on 127.0.0.1:{port}: {err}")
    host, bound_port = server.server_address
    (log_dir / "http-server.log").write_text(
        f"listening on {host}:{bound_port}\n",
        encoding="utf-8",
    )
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("url")
    parser.add_argument("--log-dir", required=True)
    parser.add_argument("--seconds", type=float, default=18)
    parser.add_argument("--width", type=int, default=1200)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--serve-bitcoin-pdf", action="store_true")
    parser.add_argument("--pdf-port", type=int, default=9787)
    args = parser.parse_args()

    log_dir = pathlib.Path(args.log_dir).resolve()
    log_dir.mkdir(parents=True, exist_ok=True)

    if not ROAMIUM.exists():
        raise SystemExit(f"missing Roamium binary: {ROAMIUM}")

    pdf_server = (
        start_pdf_server(log_dir, args.pdf_port) if args.serve_bitcoin_pdf else None
    )

    socket_path = log_dir / "gui.sock"
    try:
        socket_path.unlink()
    except FileNotFoundError:
        pass

    listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    listener.bind(str(socket_path))
    listener.listen(1)
    listener.settimeout(20)

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
    )

    try:
        conn, _ = listener.accept()
        conn.settimeout(0.2)
        start = time.time()
        sent_create = False

        with (log_dir / "messages.log").open("w", encoding="utf-8") as messages:
            while time.time() - start < args.seconds:
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

                    if top == 12 and not sent_create:
                        send_message(
                            conn,
                            1,
                            create_tab_payload(args.url, args.width, args.height),
                        )
                        sent_create = True
                        messages.write("sent CreateTab\n")
                        messages.flush()
                    elif top == 13:
                        tab_id = tab_ready_id(body)
                        messages.write(f"tab_ready id={tab_id}\n")
                        if tab_id:
                            send_message(
                                conn, 3, resize_payload(tab_id, args.width, args.height)
                            )
                            messages.write("sent Resize\n")
                        messages.flush()
                except socket.timeout:
                    pass
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
        stdout.close()
        stderr.close()
        listener.close()
        if pdf_server:
            pdf_server.shutdown()

    print(log_dir)
    return 0


if __name__ == "__main__":
    sys.exit(main())
