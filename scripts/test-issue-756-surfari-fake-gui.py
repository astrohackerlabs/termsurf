#!/usr/bin/env python3
"""Run Surfari against a minimal TermSurf GUI socket.

This harness proves the Surfari Rust process can speak the TermSurf
Unix-socket/protobuf protocol and drive libtermsurf_webkit without Ghostboard.
It accepts ServerRegister, sends CreateTab, records browser-side state
messages, sends Resize after TabReady, sends CloseTab after the deterministic
page is ready, and verifies Surfari's protocol trace.
"""

from __future__ import annotations

import argparse
import os
import pathlib
import socket
import struct
import subprocess
import sys
import time


ROOT = pathlib.Path(__file__).resolve().parents[1]
SURFARI = ROOT / "target/debug/surfari"
WEBKIT_BUILD = ROOT / "webkit/src/WebKitBuild/Debug"
EXPECTED_TITLE = "Surfari ABI Navigation Page"


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


def read_exact(conn: socket.socket, size: int) -> bytes | None:
    data = bytearray()
    while len(data) < size:
        chunk = conn.recv(size - len(data))
        if not chunk:
            return None
        data.extend(chunk)
    return bytes(data)


def inner_payload(payload: bytes) -> tuple[int, bytes]:
    key, index = read_varint(payload, 0)
    length, index = read_varint(payload, index)
    return key >> 3, payload[index : index + length]


def iter_fields(payload: bytes):
    index = 0
    while index < len(payload):
        key, index = read_varint(payload, index)
        number = key >> 3
        wire_type = key & 7
        if wire_type == 0:
            value, index = read_varint(payload, index)
            yield number, wire_type, value
        elif wire_type == 1:
            value = payload[index : index + 8]
            index += 8
            yield number, wire_type, value
        elif wire_type == 2:
            length, index = read_varint(payload, index)
            value = payload[index : index + length]
            index += length
            yield number, wire_type, value
        elif wire_type == 5:
            value = payload[index : index + 4]
            index += 4
            yield number, wire_type, value
        else:
            raise ValueError(f"unsupported wire type {wire_type}")


def parse_string(payload: bytes, number: int) -> str:
    for field_number, wire_type, value in iter_fields(payload):
        if field_number == number and wire_type == 2:
            return bytes(value).decode("utf-8", errors="replace")
    return ""


def parse_varint(payload: bytes, number: int) -> int:
    for field_number, wire_type, value in iter_fields(payload):
        if field_number == number and wire_type == 0:
            return int(value)
    return 0


def create_tab_payload(url: str, width: int, height: int) -> bytes:
    return (
        string_field(1, url)
        + string_field(2, "surfari-fake-pane")
        + varint_field(3, width)
        + varint_field(4, height)
        + bool_field(5, False)
    )


def resize_payload(tab_id: int, width: int, height: int) -> bytes:
    return varint_field(1, tab_id) + varint_field(2, width) + varint_field(3, height)


def close_tab_payload(tab_id: int) -> bytes:
    return varint_field(1, tab_id)


def create_devtools_tab_payload(tab_id: int, width: int, height: int) -> bytes:
    return (
        string_field(1, "surfari-devtools-pane")
        + varint_field(2, tab_id)
        + varint_field(3, width)
        + varint_field(4, height)
        + bool_field(5, False)
    )


def query_tabs_payload() -> bytes:
    return string_field(1, "surfari-fake-pane") + string_field(2, "profile")


def parse_tab_info(payload: bytes) -> dict[str, int | str]:
    return {
        "id": parse_varint(payload, 1),
        "inspected_tab_id": parse_varint(payload, 2),
        "pane_id": parse_string(payload, 3),
        "url": parse_string(payload, 4),
    }


def parse_query_tabs_reply(payload: bytes) -> dict[str, int | str | list[dict[str, int | str]]]:
    tabs = []
    for field_number, wire_type, value in iter_fields(payload):
        if field_number == 5 and wire_type == 2:
            tabs.append(parse_tab_info(bytes(value)))
    return {
        "gui_panes": parse_varint(payload, 1),
        "chromium_tabs": parse_varint(payload, 2),
        "chromium_browser": parse_varint(payload, 3),
        "chromium_devtools": parse_varint(payload, 4),
        "tabs": tabs,
        "error": parse_string(payload, 6),
    }


class State:
    def __init__(self) -> None:
        self.server_register = False
        self.profile = ""
        self.sent_create = False
        self.sent_resize = False
        self.tab_id = 0
        self.ca_context_id = 0
        self.ca_width = 0
        self.ca_height = 0
        self.url = ""
        self.loading_states: list[str] = []
        self.title = ""
        self.sent_query_before_devtools = False
        self.query_before_devtools_ok = False
        self.sent_create_devtools = False
        self.devtools_request_time = 0.0
        self.devtools_tab_id = 0
        self.devtools_ca_context_id = 0
        self.sent_devtools_resize = False
        self.sent_query_after_devtools = False
        self.query_after_devtools_ok = False
        self.devtools_supported = False
        self.sent_close_devtools = False
        self.sent_close_browser = False
        self.process_clean_exit = False

    def browser_ready_for_devtools(self) -> bool:
        return (
            self.tab_id > 0
            and self.ca_context_id > 0
            and self.url
            and "done" in self.loading_states
            and self.title == EXPECTED_TITLE
            and self.sent_resize
        )


def fail(message: str, log_dir: pathlib.Path) -> None:
    (log_dir / "summary.log").write_text(f"FAIL {message}\n", encoding="utf-8")
    raise SystemExit(f"SMOKE_FAIL {message}")


def has_zero_id_tab(reply: dict[str, int | str | list[dict[str, int | str]]]) -> bool:
    return any(tab["id"] == 0 for tab in reply["tabs"])


def validate_query_before_devtools(
    reply: dict[str, int | str | list[dict[str, int | str]]],
    state: State,
    log_dir: pathlib.Path,
) -> None:
    if reply["chromium_browser"] != 1 or reply["chromium_devtools"] != 0:
        fail(f"unexpected pre-devtools tab counts {reply}", log_dir)
    if has_zero_id_tab(reply):
        fail(f"pre-devtools QueryTabsReply contained id=0 tab {reply}", log_dir)
    tabs = reply["tabs"]
    if len(tabs) != 1:
        fail(f"expected one pre-devtools TabInfo, got {reply}", log_dir)
    tab = tabs[0]
    if tab["id"] != state.tab_id or tab["inspected_tab_id"] != 0:
        fail(f"unexpected pre-devtools TabInfo {reply}", log_dir)


def validate_query_after_devtools(
    reply: dict[str, int | str | list[dict[str, int | str]]],
    state: State,
    log_dir: pathlib.Path,
) -> None:
    if has_zero_id_tab(reply):
        fail(f"post-devtools QueryTabsReply contained id=0 tab {reply}", log_dir)
    if state.devtools_tab_id > 0:
        if reply["chromium_browser"] != 1 or reply["chromium_devtools"] != 1:
            fail(f"unexpected post-devtools supported counts {reply}", log_dir)
        devtools_tabs = [
            tab
            for tab in reply["tabs"]
            if tab["id"] == state.devtools_tab_id
            and tab["inspected_tab_id"] == state.tab_id
        ]
        if len(devtools_tabs) != 1:
            fail(f"missing DevTools TabInfo {reply}", log_dir)
    else:
        if reply["chromium_browser"] != 1 or reply["chromium_devtools"] != 0:
            fail(f"unexpected post-devtools unsupported counts {reply}", log_dir)


def check_trace(log_dir: pathlib.Path, state: State) -> str:
    trace_path = log_dir / "surfari-trace.log"
    if not trace_path.exists():
        fail("surfari trace missing", log_dir)
    trace = trace_path.read_text(encoding="utf-8", errors="replace")
    for needle in (
        "surfari create-tab",
        "surfari resize",
        "ffi=ts_set_view_size",
        "surfari close-tab",
    ):
        if needle not in trace:
            fail(f"surfari trace missing {needle}", log_dir)
    if "surfari create-devtools-tab" not in trace:
        fail("surfari trace missing create-devtools-tab", log_dir)
    if state.devtools_supported:
        if f"resize tab_id={state.devtools_tab_id}" not in trace:
            fail("surfari trace missing DevTools resize", log_dir)
    elif "devtools-unsupported" not in trace:
        fail("surfari trace missing devtools-unsupported marker", log_dir)
    return trace


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("url")
    parser.add_argument("--log-dir", required=True)
    parser.add_argument("--seconds", type=float, default=25)
    parser.add_argument("--width", type=int, default=640)
    parser.add_argument("--height", type=int, default=480)
    args = parser.parse_args()

    log_dir = pathlib.Path(args.log_dir).resolve()
    log_dir.mkdir(parents=True, exist_ok=True)

    if not SURFARI.exists():
        raise SystemExit(f"missing Surfari binary: {SURFARI}")
    if not WEBKIT_BUILD.exists():
        raise SystemExit(f"missing WebKit build: {WEBKIT_BUILD}")

    socket_path = log_dir / "gui.sock"
    try:
        socket_path.unlink()
    except FileNotFoundError:
        pass

    listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    listener.bind(str(socket_path))
    listener.listen(1)
    listener.settimeout(20)

    stdout = (log_dir / "surfari.stdout").open("wb")
    stderr = (log_dir / "surfari.stderr").open("wb")
    env = os.environ.copy()
    env["DYLD_FRAMEWORK_PATH"] = str(WEBKIT_BUILD)
    env["TERMSURF_PDF_INPUT_TRACE"] = "1"
    env["TERMSURF_PDF_INPUT_TRACE_FILE"] = str(log_dir / "surfari-trace.log")
    proc = subprocess.Popen(
        [
            str(SURFARI),
            f"--ipc-socket={socket_path}",
            f"--user-data-dir={log_dir / 'profile'}",
        ],
        cwd=str(ROOT),
        env=env,
        stdout=stdout,
        stderr=stderr,
    )

    state = State()
    conn: socket.socket | None = None
    try:
        conn, _ = listener.accept()
        conn.settimeout(0.2)
        start = time.time()

        with (log_dir / "messages.log").open("w", encoding="utf-8") as messages:
            while time.time() - start < args.seconds:
                if state.sent_close_browser and proc.poll() is not None:
                    state.process_clean_exit = proc.returncode == 0
                    break

                try:
                    header = read_exact(conn, 4)
                    if not header:
                        break
                    size = struct.unpack("<I", header)[0]
                    payload = read_exact(conn, size)
                    if payload is None:
                        break

                    top, body = inner_payload(payload)
                    messages.write(f"t={time.time() - start:.3f} top_field={top}\n")

                    if top == 12:
                        state.server_register = True
                        state.profile = parse_string(body, 1)
                        messages.write(f"server_register profile={state.profile}\n")
                        if not state.sent_create:
                            send_message(
                                conn,
                                1,
                                create_tab_payload(args.url, args.width, args.height),
                            )
                            state.sent_create = True
                            messages.write("sent CreateTab\n")
                    elif top == 13:
                        pane_id = parse_string(body, 1)
                        tab_id = parse_varint(body, 2)
                        messages.write(f"tab_ready pane={pane_id} id={tab_id}\n")
                        if pane_id == "surfari-devtools-pane":
                            state.devtools_tab_id = tab_id
                            if state.devtools_tab_id > 0 and not state.sent_devtools_resize:
                                send_message(
                                    conn,
                                    3,
                                    resize_payload(
                                        state.devtools_tab_id, args.width, args.height
                                    ),
                                )
                                state.sent_devtools_resize = True
                                messages.write("sent DevTools Resize\n")
                        else:
                            state.tab_id = tab_id
                        if state.tab_id > 0 and not state.sent_resize:
                            send_message(
                                conn,
                                3,
                                resize_payload(state.tab_id, args.width, args.height),
                            )
                            state.sent_resize = True
                            messages.write("sent Resize\n")
                    elif top == 14:
                        ca_tab_id = parse_varint(body, 1)
                        ca_context_id = parse_varint(body, 2)
                        ca_width = parse_varint(body, 3)
                        ca_height = parse_varint(body, 4)
                        if (
                            state.devtools_tab_id > 0
                            and ca_tab_id == state.devtools_tab_id
                        ):
                            state.devtools_ca_context_id = ca_context_id
                        else:
                            state.ca_context_id = ca_context_id
                            state.ca_width = ca_width
                            state.ca_height = ca_height
                        messages.write(
                            f"ca_context tab={ca_tab_id} id={ca_context_id} width={ca_width} height={ca_height}\n"
                        )
                    elif top == 15:
                        state.url = parse_string(body, 2)
                        messages.write(f"url_changed url={state.url}\n")
                    elif top == 16:
                        loading = parse_string(body, 2)
                        state.loading_states.append(loading)
                        messages.write(f"loading_state state={loading}\n")
                    elif top == 17:
                        state.title = parse_string(body, 2)
                        messages.write(f"title_changed title={state.title}\n")
                    elif top == 30:
                        reply = parse_query_tabs_reply(body)
                        messages.write(f"query_tabs_reply {reply}\n")
                        if not state.query_before_devtools_ok:
                            validate_query_before_devtools(reply, state, log_dir)
                            state.query_before_devtools_ok = True
                            send_message(
                                conn,
                                2,
                                create_devtools_tab_payload(
                                    state.tab_id, args.width, args.height
                                ),
                            )
                            state.sent_create_devtools = True
                            state.devtools_request_time = time.time()
                            messages.write("sent CreateDevtoolsTab\n")
                        elif not state.query_after_devtools_ok:
                            validate_query_after_devtools(reply, state, log_dir)
                            state.query_after_devtools_ok = True
                            state.devtools_supported = state.devtools_tab_id > 0
                            if state.devtools_supported and not state.sent_close_devtools:
                                send_message(
                                    conn, 4, close_tab_payload(state.devtools_tab_id)
                                )
                                state.sent_close_devtools = True
                                messages.write("sent DevTools CloseTab\n")
                            if not state.devtools_supported and not state.sent_close_browser:
                                send_message(conn, 4, close_tab_payload(state.tab_id))
                                state.sent_close_browser = True
                                messages.write("sent Browser CloseTab\n")

                    if (
                        state.browser_ready_for_devtools()
                        and not state.sent_query_before_devtools
                    ):
                        send_message(conn, 29, query_tabs_payload())
                        state.sent_query_before_devtools = True
                        messages.write("sent QueryTabsRequest before DevTools\n")

                    if (
                        state.sent_create_devtools
                        and not state.sent_query_after_devtools
                        and (
                            state.devtools_ca_context_id > 0
                            or time.time() - state.devtools_request_time > 1.0
                        )
                    ):
                        send_message(conn, 29, query_tabs_payload())
                        state.sent_query_after_devtools = True
                        messages.write("sent QueryTabsRequest after DevTools\n")

                    if (
                        state.sent_close_devtools
                        and not state.sent_close_browser
                        and state.query_after_devtools_ok
                    ):
                        send_message(conn, 4, close_tab_payload(state.tab_id))
                        state.sent_close_browser = True
                        messages.write("sent Browser CloseTab\n")

                    messages.flush()
                except socket.timeout:
                    if (
                        state.sent_create_devtools
                        and not state.sent_query_after_devtools
                        and time.time() - state.devtools_request_time > 1.0
                    ):
                        send_message(conn, 29, query_tabs_payload())
                        state.sent_query_after_devtools = True
                        messages.write("sent QueryTabsRequest after DevTools timeout\n")
                        messages.flush()
                    continue
    finally:
        if conn:
            conn.close()
        listener.close()
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=5)
        if state.sent_close_browser and proc.returncode == 0:
            state.process_clean_exit = True
        stdout.close()
        stderr.close()

    if not state.server_register:
        fail("ServerRegister missing", log_dir)
    if state.profile != "profile":
        fail(f"unexpected profile {state.profile!r}", log_dir)
    if not state.sent_create:
        fail("CreateTab not sent", log_dir)
    if state.tab_id <= 0:
        fail("TabReady missing positive tab id", log_dir)
    if state.ca_context_id <= 0 or state.ca_width <= 0 or state.ca_height <= 0:
        fail("CaContext missing valid context or dimensions", log_dir)
    if not state.url or "navigation.html" not in state.url:
        fail("UrlChanged missing deterministic navigation URL", log_dir)
    if "loading" not in state.loading_states:
        fail("LoadingState loading missing", log_dir)
    if "done" not in state.loading_states:
        fail("LoadingState done missing", log_dir)
    if state.title != EXPECTED_TITLE:
        fail(f"TitleChanged mismatch {state.title!r}", log_dir)
    if not state.sent_resize:
        fail("Resize not sent", log_dir)
    if not state.sent_query_before_devtools or not state.query_before_devtools_ok:
        fail("pre-DevTools QueryTabsRequest/Reply not verified", log_dir)
    if not state.sent_create_devtools:
        fail("CreateDevtoolsTab not sent", log_dir)
    if not state.sent_query_after_devtools or not state.query_after_devtools_ok:
        fail("post-DevTools QueryTabsRequest/Reply not verified", log_dir)
    if state.devtools_supported:
        if state.devtools_tab_id <= 0:
            fail("DevTools supported without positive tab id", log_dir)
        if state.devtools_ca_context_id <= 0:
            fail("DevTools supported without CaContext", log_dir)
        if not state.sent_devtools_resize:
            fail("DevTools Resize not sent", log_dir)
        if not state.sent_close_devtools:
            fail("DevTools CloseTab not sent", log_dir)
    else:
        if state.devtools_tab_id != 0:
            fail("DevTools unsupported path emitted a tab id", log_dir)
        if state.devtools_ca_context_id != 0:
            fail("DevTools unsupported path emitted a CaContext", log_dir)
    if not state.sent_close_browser:
        fail("Browser CloseTab not sent", log_dir)
    if not state.process_clean_exit:
        fail(f"Surfari did not exit cleanly after CloseTab rc={proc.returncode}", log_dir)

    check_trace(log_dir, state)
    summary = (
        "SMOKE_PASS "
        f"profile={state.profile} tab_id={state.tab_id} "
        f"devtools_supported={int(state.devtools_supported)} "
        f"devtools_tab_id={state.devtools_tab_id} "
        f"ca_context_id={state.ca_context_id} title={state.title!r} "
        f"loading_states={','.join(state.loading_states)} clean_exit=1\n"
    )
    (log_dir / "summary.log").write_text(summary, encoding="utf-8")
    print(summary, end="")
    print(log_dir)
    return 0


if __name__ == "__main__":
    sys.exit(main())
