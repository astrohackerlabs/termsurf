#!/usr/bin/env python3
"""Live macOS Quick Look/definition guard for Issue 805 Experiment 193."""

from __future__ import annotations

import json
import os
import shlex
import struct
import subprocess
import tempfile
import textwrap
import time
import zlib
from pathlib import Path

from macos_live_link_hover_runtime import global_point, inject_move, wait_for_resize
from macos_window_padding_pixel_runtime import (
    APP,
    ROOT,
    Rect,
    capture_window_id,
    crash_reports,
    create_terminal_window,
    focus_evidence,
    focused_window,
    quote_applescript,
    require,
    run_osascript,
    scoped_pids,
    terminate_process,
    wait_for_app,
    wait_for_crash_report_settle,
    wait_for_file,
)


WORD = "serendipity"
WORD_ROW = 8
WORD_COL = 10
LOGS = ROOT / "logs"
LATEST_JSON = LOGS / "issue805-exp193-quicklook-latest.json"
LATEST_BEFORE_WINDOW = LOGS / "issue805-exp193-quicklook-before-window.png"
LATEST_AFTER_WINDOW = LOGS / "issue805-exp193-quicklook-after-window.png"
LATEST_BEFORE_SCREEN = LOGS / "issue805-exp193-quicklook-before-screen.png"
LATEST_AFTER_SCREEN = LOGS / "issue805-exp193-quicklook-after-screen.png"


def write_config(path: Path) -> None:
    path.write_text(
        "\n".join(
            [
                "macos-applescript = true",
                "quit-after-last-window-closed = true",
                "font-size = 16",
                "window-width = 100",
                "window-height = 34",
                "background = #102030",
                "foreground = #ffffff",
                "background-opacity = 1",
                "macos-titlebar-style = hidden",
                "window-padding-x = 0",
                "window-padding-y = 0",
                "",
            ]
        )
    )


def write_painter(path: Path, marker: Path) -> None:
    path.write_text(
        textwrap.dedent(
            f"""
            from pathlib import Path
            import sys
            import time

            marker = Path({str(marker)!r})
            word = {WORD!r}
            for index in range(240):
                sys.stdout.write("\\x1b[?25h\\x1b[?7l\\x1b[2J\\x1b[H")
                sys.stdout.write("\\x1b[{WORD_ROW};{WORD_COL}H" + word)
                sys.stdout.flush()
                if index == 0:
                    marker.write_text("ready")
                time.sleep(0.1)
            time.sleep(30)
            """
        ).lstrip()
    )


def launch_app(config: Path, trace: Path) -> int:
    before = scoped_pids()
    require(not before, f"debug Roastty app is already running: {sorted(before)}")
    result = subprocess.run(
        [
            "open",
            "-n",
            "--env",
            f"ROASTTY_CONFIG_PATH={config}",
            "--env",
            "ROASTTY_CLEAR_USER_DEFAULTS=1",
            "--env",
            "ROASTTY_USER_DEFAULTS_SUITE=com.termsurf.roastty.issue805.exp193.quicklook",
            "--env",
            f"ROASTTY_UI_KEY_TRACE_PATH={trace}",
            "--env",
            "ROASTTY_UI_TEST_ENABLE_QUICKLOOK_ACTION=1",
            str(APP),
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
    )
    require(result.returncode == 0, f"open failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}")

    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        created = sorted(scoped_pids() - before)
        if created:
            return created[0]
        time.sleep(0.25)
    raise AssertionError("open did not start a scoped debug Roastty process")


def capture_screen(output: Path) -> tuple[int, int]:
    result = subprocess.run(
        ["screencapture", "-x", str(output)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=30,
    )
    require(result.returncode == 0, f"screencapture failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}")
    require(output.is_file(), f"screen screenshot missing: {output}")
    width = subprocess.run(["sips", "-g", "pixelWidth", str(output)], text=True, capture_output=True, timeout=10)
    height = subprocess.run(["sips", "-g", "pixelHeight", str(output)], text=True, capture_output=True, timeout=10)
    require(width.returncode == 0 and height.returncode == 0, "sips failed to read screen dimensions")
    width_value = int(next(line.split(":")[1] for line in width.stdout.splitlines() if "pixelWidth:" in line))
    height_value = int(next(line.split(":")[1] for line in height.stdout.splitlines() if "pixelHeight:" in line))
    return width_value, height_value


def read_png_rgba(path: Path) -> tuple[int, int, list[bytearray]]:
    data = path.read_bytes()
    require(data[:8] == b"\x89PNG\r\n\x1a\n", f"not a PNG: {path}")
    pos = 8
    width = height = bit_depth = color_type = interlace = None
    idat: list[bytes] = []
    while pos < len(data):
        chunk_len = struct.unpack(">I", data[pos : pos + 4])[0]
        chunk_type = data[pos + 4 : pos + 8]
        chunk = data[pos + 8 : pos + 8 + chunk_len]
        pos += 12 + chunk_len
        if chunk_type == b"IHDR":
            width, height, bit_depth, color_type, _compression, _filter, interlace = struct.unpack(
                ">IIBBBBB", chunk
            )
        elif chunk_type == b"IDAT":
            idat.append(chunk)
        elif chunk_type == b"IEND":
            break

    require(width is not None and height is not None, f"missing IHDR: {path}")
    require(bit_depth == 8 and color_type == 6 and interlace == 0, f"unsupported PNG format in {path}")
    raw = zlib.decompress(b"".join(idat))
    bytes_per_pixel = 4
    stride = width * bytes_per_pixel
    rows: list[bytearray] = []
    previous = bytearray(stride)
    offset = 0
    for _y in range(height):
        filter_type = raw[offset]
        offset += 1
        scanline = bytearray(raw[offset : offset + stride])
        offset += stride
        decoded = bytearray(stride)
        for i, value in enumerate(scanline):
            left = decoded[i - bytes_per_pixel] if i >= bytes_per_pixel else 0
            up = previous[i]
            up_left = previous[i - bytes_per_pixel] if i >= bytes_per_pixel else 0
            if filter_type == 0:
                decoded[i] = value
            elif filter_type == 1:
                decoded[i] = (value + left) & 0xFF
            elif filter_type == 2:
                decoded[i] = (value + up) & 0xFF
            elif filter_type == 3:
                decoded[i] = (value + ((left + up) // 2)) & 0xFF
            elif filter_type == 4:
                p = left + up - up_left
                pa = abs(p - left)
                pb = abs(p - up)
                pc = abs(p - up_left)
                predictor = left if pa <= pb and pa <= pc else up if pb <= pc else up_left
                decoded[i] = (value + predictor) & 0xFF
            else:
                raise AssertionError(f"unsupported PNG filter {filter_type} in {path}")
        rows.append(decoded)
        previous = decoded
    return width, height, rows


def count_nonblack_pixels(path: Path, width: int, height: int) -> int:
    image_width, image_height, rows = read_png_rgba(path)
    require(width <= image_width and height <= image_height, f"sample exceeds image size for {path}")
    count = 0
    for y in range(height):
        row = rows[y]
        for x in range(width):
            offset = x * 4
            red, green, blue, alpha = row[offset : offset + 4]
            if alpha > 0 and (red > 8 or green > 8 or blue > 8):
                count += 1
    return count


def wait_for_native_definition_capture(
    window_id: int,
    before_window_size: tuple[int, int],
    output: Path,
    timeout: float = 6.0,
) -> tuple[tuple[int, int], int, int]:
    deadline = time.monotonic() + timeout
    last_size = before_window_size
    last_extra_width = 0
    last_nonblack = 0
    while time.monotonic() < deadline:
        time.sleep(0.25)
        after_window_size = capture_window_id(window_id, output)
        extra_width = after_window_size[0] - before_window_size[0]
        if extra_width >= 100 and after_window_size[1] == before_window_size[1]:
            extra_band_nonblack = count_nonblack_pixels(output, extra_width, after_window_size[1])
            if extra_band_nonblack >= 50_000:
                return after_window_size, extra_width, extra_band_nonblack
            last_nonblack = extra_band_nonblack
        last_size = after_window_size
        last_extra_width = extra_width
    raise AssertionError(
        "Quick Look did not expose visible native definition UI in time: "
        f"before={before_window_size} last_after={last_size} "
        f"last_extra_width={last_extra_width} last_nonblack={last_nonblack}"
    )


def perform_quicklook_action() -> None:
    app_literal = quote_applescript(APP)
    script = textwrap.dedent(
        f"""
        tell application {app_literal}
          set t0 to focused terminal of selected tab of front window
          perform action "ui_test_quicklook" on t0
        end tell
        """
    )
    run_osascript(script, timeout=15)


def dismiss_native_popover() -> None:
    result = subprocess.run(
        ["osascript", "-e", 'tell application "System Events" to key code 53'],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=10,
    )
    require(result.returncode == 0, f"failed to dismiss Quick Look popover\nstderr:\n{result.stderr}")


def quicklook_trace_seen(text: str) -> bool:
    return (
        "quickLook uiTestAction=invoke" in text
        and f"quickLook text={WORD} " in text
        and "fontPresent=true" in text
        and "quickLook showDefinition=true" in text
    )


def trace_text(path: Path) -> str:
    return path.read_text(errors="replace") if path.exists() else ""


def main() -> int:
    require(APP.is_dir(), f"app not built: {APP}")
    LOGS.mkdir(parents=True, exist_ok=True)
    before_crashes = crash_reports()

    with tempfile.TemporaryDirectory(prefix="issue805-exp193-quicklook-", dir=LOGS) as temp_dir:
        temp = Path(temp_dir)
        config = temp / "config.roastty"
        trace = temp / "trace.log"
        marker = temp / "marker.txt"
        painter = temp / "paint_word.py"
        before_window = temp / "before-window.png"
        after_window = temp / "after-window.png"
        before_screen = temp / "before-screen.png"
        after_screen = temp / "after-screen.png"
        evidence = temp / "evidence.json"

        write_config(config)
        write_painter(painter, marker)

        pid = launch_app(config, trace)
        try:
            wait_for_app(pid)
            command = f"{shlex.quote(str(Path('/usr/bin/python3')))} {shlex.quote(str(painter))}"
            terminal_id = create_terminal_window(command)
            wait_for_file(marker, "Quick Look word painter")
            resize = wait_for_resize(trace)

            focus = focus_evidence(pid)
            focused_bounds = focus["focused_bounds"]
            require(isinstance(focused_bounds, Rect), f"unexpected focused bounds: {focused_bounds}")
            window = focused_window(pid, focused_bounds)

            attempts = []
            target_cols = [WORD_COL + 1, WORD_COL + 4, WORD_COL + 8]
            vertical_offsets = [0, 15, 25, 35, 45, -15, -30, 60]
            before_window_size = capture_window_id(window.id, before_window)
            before_screen_size = capture_screen(before_screen)

            for target_col in target_cols:
                base_x, base_y = global_point(window.bounds, WORD_ROW, target_col, resize["cols"], resize["rows"])
                for vertical_offset in vertical_offsets:
                    x = base_x
                    y = base_y + vertical_offset
                    inject_move(x, y)
                    time.sleep(0.2)
                    perform_quicklook_action()
                    time.sleep(0.5)
                    attempts.append(
                        {
                            "row": WORD_ROW,
                            "col": target_col,
                            "x": x,
                            "y": y,
                            "vertical_offset": vertical_offset,
                        }
                    )
                    if quicklook_trace_seen(trace_text(trace)):
                        break
                else:
                    continue
                break

            text = trace_text(trace)
            require(quicklook_trace_seen(text), f"Quick Look trace did not prove the expected path; trace was:\n{text}")
            require("quickLook fallback=" not in text.split("quickLook uiTestAction=invoke")[-1], f"unexpected fallback after final invoke:\n{text}")

            after_window_size, extra_width, extra_band_nonblack = wait_for_native_definition_capture(
                window.id,
                before_window_size,
                after_window,
            )
            after_screen_size = capture_screen(after_screen)

            data = {
                "pid": pid,
                "terminal_id": terminal_id,
                "word": WORD,
                "window_id": window.id,
                "window_bounds": window.bounds.__dict__,
                "focused_bounds": focused_bounds.__dict__,
                "resize": resize,
                "attempts": attempts,
                "before_window_size": {"width": before_window_size[0], "height": before_window_size[1]},
                "after_window_size": {"width": after_window_size[0], "height": after_window_size[1]},
                "before_screen_size": {"width": before_screen_size[0], "height": before_screen_size[1]},
                "after_screen_size": {"width": after_screen_size[0], "height": after_screen_size[1]},
                "extra_window_capture_width": extra_width,
                "extra_band_nonblack_pixels": extra_band_nonblack,
                "trace_tail": text.splitlines()[-30:],
            }
            evidence.write_text(json.dumps(data, indent=2, sort_keys=True))
            LATEST_JSON.write_text(evidence.read_text())
            LATEST_BEFORE_WINDOW.write_bytes(before_window.read_bytes())
            LATEST_AFTER_WINDOW.write_bytes(after_window.read_bytes())
            LATEST_BEFORE_SCREEN.write_bytes(before_screen.read_bytes())
            LATEST_AFTER_SCREEN.write_bytes(after_screen.read_bytes())
            dismiss_native_popover()
        finally:
            terminate_process(pid)

    new_crashes = wait_for_crash_report_settle(before_crashes)
    require(
        not new_crashes,
        "Roastty wrote crash reports during live Quick Look workflow: "
        + ", ".join(str(path) for path in sorted(new_crashes)),
    )

    print("macos_live_quicklook_definition=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
