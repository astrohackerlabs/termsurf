#!/usr/bin/env python3
"""Live macOS URL hover banner pixel guard for Issue 805 CFG-223."""

from __future__ import annotations

import json
import re
import shlex
import subprocess
import tempfile
import textwrap
import time
from pathlib import Path

from macos_window_padding_pixel_runtime import (
    APP,
    ROOT,
    Rect,
    capture_window_id,
    crash_reports,
    create_terminal_window,
    focus_evidence,
    focused_window,
    require,
    terminate_process,
    wait_for_app,
    wait_for_crash_report_settle,
    wait_for_file,
)


INJECT = ROOT / "scripts/ghostty-app/inject.swift"
URL = "https://example.com/issue805-exp189-link-banner"
LINK_ROW = 8
LINK_COL = 10


def write_config(path: Path) -> None:
    path.write_text(
        "\n".join(
            [
                "macos-applescript = true",
                "quit-after-last-window-closed = true",
                "cursor-style-blink = false",
                "font-size = 16",
                "window-width = 100",
                "window-height = 34",
                "background = #102030",
                "foreground = #ffffff",
                "background-opacity = 1",
                "macos-titlebar-style = hidden",
                "window-padding-x = 0",
                "window-padding-y = 0",
                "link-previews = true",
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
            url = {URL!r}
            sys.stdout.write("\\x1b[?25l\\x1b[?7l\\x1b[2J\\x1b[H")
            sys.stdout.write("\\x1b[{LINK_ROW};{LINK_COL}H" + url)
            sys.stdout.flush()
            marker.write_text("ready")
            time.sleep(90)
            """
        ).lstrip()
    )


def write_sampler(path: Path) -> None:
    path.write_text(
        r'''
import AppKit
import Foundation

struct Region: Codable {
    let x: Int
    let y: Int
    let width: Int
    let height: Int
    let changed: Int
    let samples: Int
    let maxDelta: Int
    let meanDelta: Double
}

struct Metrics: Codable {
    let width: Int
    let height: Int
    let bottomLeft: Region
    let upperLeft: Region
    let bottomRight: Region
    let fullSparse: Region
    let verdict: String
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write((message + "\n").data(using: .utf8)!)
    exit(1)
}

guard CommandLine.arguments.count == 3 else {
    fail("usage: sampler.swift <baseline.png> <hover.png>")
}

func loadBitmap(_ path: String) -> NSBitmapImageRep {
    guard let image = NSImage(contentsOfFile: path),
          let tiff = image.tiffRepresentation,
          let bitmap = NSBitmapImageRep(data: tiff)
    else {
        fail("failed to load image: \(path)")
    }
    return bitmap
}

let baseline = loadBitmap(CommandLine.arguments[1])
let hover = loadBitmap(CommandLine.arguments[2])
let width = baseline.pixelsWide
let height = baseline.pixelsHigh
if hover.pixelsWide != width || hover.pixelsHigh != height {
    fail("screenshot dimensions differ: \(width)x\(height) vs \(hover.pixelsWide)x\(hover.pixelsHigh)")
}

func colorAt(_ bitmap: NSBitmapImageRep, _ x: Int, _ y: Int) -> (Int, Int, Int) {
    guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else {
        return (0, 0, 0)
    }
    return (
        Int((color.redComponent * 255).rounded()),
        Int((color.greenComponent * 255).rounded()),
        Int((color.blueComponent * 255).rounded())
    )
}

func clamp(_ value: Int, _ lower: Int, _ upper: Int) -> Int {
    return min(max(value, lower), upper)
}

func sampleRegion(x: Int, y: Int, w: Int, h: Int, sampleStride: Int) -> Region {
    let x0 = clamp(x, 0, width - 1)
    let y0 = clamp(y, 0, height - 1)
    let x1 = clamp(x + w - 1, 0, width - 1)
    let y1 = clamp(y + h - 1, 0, height - 1)
    if x1 <= x0 || y1 <= y0 {
        fail("empty sample region")
    }

    var changed = 0
    var samples = 0
    var maxDelta = 0
    var totalDelta = 0
    let step = max(1, sampleStride)
    for yy in stride(from: y0, through: y1, by: step) {
        for xx in stride(from: x0, through: x1, by: step) {
            let before = colorAt(baseline, xx, yy)
            let after = colorAt(hover, xx, yy)
            let redDelta = abs(after.0 - before.0)
            let greenDelta = abs(after.1 - before.1)
            let blueDelta = abs(after.2 - before.2)
            let delta = redDelta + greenDelta + blueDelta
            samples += 1
            totalDelta += delta
            maxDelta = max(maxDelta, delta)
            if delta >= 45 {
                changed += 1
            }
        }
    }
    return Region(
        x: x0,
        y: y0,
        width: x1 - x0 + 1,
        height: y1 - y0 + 1,
        changed: changed,
        samples: samples,
        maxDelta: maxDelta,
        meanDelta: Double(totalDelta) / Double(max(samples, 1))
    )
}

let bannerHeight = min(170, max(80, height / 7))
let bannerWidth = min(1050, max(520, width * 2 / 3))
let bottomY = height - bannerHeight
let bottomLeft = sampleRegion(x: 0, y: bottomY, w: bannerWidth, h: bannerHeight, sampleStride: 1)
let upperLeft = sampleRegion(x: 0, y: max(0, height / 4), w: bannerWidth, h: bannerHeight, sampleStride: 2)
let bottomRight = sampleRegion(x: max(0, width - bannerWidth), y: bottomY, w: bannerWidth, h: bannerHeight, sampleStride: 2)
let fullSparse = sampleRegion(x: 0, y: 0, w: width, h: height, sampleStride: 6)

let bottomLeftRatio = Double(bottomLeft.changed) / Double(bottomLeft.samples)
let upperLeftRatio = Double(upperLeft.changed) / Double(upperLeft.samples)
let bottomRightRatio = Double(bottomRight.changed) / Double(bottomRight.samples)
let fullRatio = Double(fullSparse.changed) / Double(fullSparse.samples)

if bottomLeft.changed < 900 {
    fail("bottom-left banner region changed too few pixels: \(bottomLeft.changed)")
}
if bottomLeftRatio < 0.006 {
    fail("bottom-left banner region changed ratio too low: \(bottomLeftRatio)")
}
if bottomLeft.meanDelta < 2.0 {
    fail("bottom-left mean delta too low: \(bottomLeft.meanDelta)")
}
if bottomLeft.changed < upperLeft.changed * 5 {
    fail("upper-left control region changed too much: bottomLeft=\(bottomLeft.changed) upperLeft=\(upperLeft.changed)")
}
if bottomLeft.changed < bottomRight.changed * 4 {
    fail("bottom-right control region changed too much: bottomLeft=\(bottomLeft.changed) bottomRight=\(bottomRight.changed)")
}
if fullRatio > 0.030 {
    fail("full screenshot changed too much for a localized banner: \(fullRatio)")
}

let metrics = Metrics(
    width: width,
    height: height,
    bottomLeft: bottomLeft,
    upperLeft: upperLeft,
    bottomRight: bottomRight,
    fullSparse: fullSparse,
    verdict: "pass"
)
let data = try! JSONEncoder().encode(metrics)
FileHandle.standardOutput.write(data)
FileHandle.standardOutput.write("\n".data(using: .utf8)!)
'''
    )


def wait_for_trace(trace: Path, needles: list[str], timeout: float = 15.0) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        text = trace.read_text(errors="replace") if trace.exists() else ""
        if all(needle in text for needle in needles):
            return text
        time.sleep(0.25)
    text = trace.read_text(errors="replace") if trace.exists() else ""
    missing = [needle for needle in needles if needle not in text]
    raise AssertionError(f"trace missing {missing}; trace was:\n{text}")


def wait_for_resize(trace: Path, timeout: float = 10.0) -> dict[str, int]:
    deadline = time.monotonic() + timeout
    pattern = re.compile(r"resize rows=(\d+) cols=(\d+) width_px=(\d+) height_px=(\d+)")
    last_match = None
    while time.monotonic() < deadline:
        text = trace.read_text(errors="replace") if trace.exists() else ""
        for match in pattern.finditer(text):
            last_match = match
        if last_match:
            rows, cols, width_px, height_px = (int(value) for value in last_match.groups())
            return {
                "rows": rows,
                "cols": cols,
                "width_px": width_px,
                "height_px": height_px,
            }
        time.sleep(0.25)
    text = trace.read_text(errors="replace") if trace.exists() else ""
    raise AssertionError(f"trace never reported a terminal resize; trace was:\n{text}")


def global_point(window: Rect, row: int, col: int, cols: int, rows: int) -> tuple[float, float]:
    cell_width = window.width / cols
    cell_height = window.height / rows
    x = window.x + (col - 0.5) * cell_width
    y = window.y + (row - 0.5) * cell_height
    return x, y


def global_point_bottom_origin(window: Rect, row: int, col: int, cols: int, rows: int) -> tuple[float, float]:
    cell_width = window.width / cols
    cell_height = window.height / rows
    x = window.x + (col - 0.5) * cell_width
    y = window.y + window.height - (row - 0.5) * cell_height
    return x, y


def hover_trace_seen(trace_text: str) -> bool:
    return (
        "cursorShape raw=" in trace_text
        and "pointerStyle=link" in trace_text
        and f"mouseOverLink url={URL}" in trace_text
    )


def inject_move(x: float, y: float, *modifiers: str) -> None:
    result = subprocess.run(
        ["swift", str(INJECT), "move", f"{x:.1f}", f"{y:.1f}", *modifiers],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=10,
    )
    if result.returncode != 0:
        raise AssertionError(
            "mouse injection failed\n"
            f"stdout:\n{result.stdout}\n"
            f"stderr:\n{result.stderr}"
        )


def launch_with_trace(config: Path, trace: Path) -> int:
    before = subprocess.run(
        ["pgrep", "-f", f"{APP}/Contents/MacOS/roastty"],
        text=True,
        capture_output=True,
    )
    require(not before.stdout.split(), f"debug Roastty app is already running: {before.stdout}")
    result = subprocess.run(
        [
            "open",
            "-n",
            "--env",
            f"ROASTTY_CONFIG_PATH={config}",
            "--env",
            "ROASTTY_CLEAR_USER_DEFAULTS=1",
            "--env",
            "ROASTTY_USER_DEFAULTS_SUITE=com.termsurf.roastty.issue805.exp189.linkbanner",
            "--env",
            f"ROASTTY_UI_KEY_TRACE_PATH={trace}",
            str(APP),
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
    )
    require(result.returncode == 0, f"open failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}")

    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        after = subprocess.run(
            ["pgrep", "-f", f"{APP}/Contents/MacOS/roastty"],
            text=True,
            capture_output=True,
        )
        created = [int(value) for value in after.stdout.split()]
        if created:
            return sorted(created)[0]
        time.sleep(0.25)
    raise AssertionError("open did not start debug Roastty")


def sample_banner(sampler: Path, baseline: Path, hover: Path) -> dict[str, object]:
    result = subprocess.run(
        ["swift", str(sampler), str(baseline), str(hover)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=30,
    )
    if result.returncode != 0:
        raise AssertionError(
            "banner sampler failed\n"
            f"stdout:\n{result.stdout}\n"
            f"stderr:\n{result.stderr}"
        )
    return json.loads(result.stdout)


def main() -> int:
    require(APP.is_dir(), f"app not built: {APP}")
    before_crashes = crash_reports()

    with tempfile.TemporaryDirectory(prefix="termsurf-issue805-exp189-link-banner-") as temp_dir:
        temp = Path(temp_dir)
        config = temp / "config.roastty"
        trace = temp / "trace.log"
        marker = temp / "marker.txt"
        painter = temp / "paint_link.py"
        sampler = temp / "sample_banner.swift"
        baseline = temp / "baseline.png"
        hover = temp / "hover.png"
        evidence = temp / "evidence.json"

        write_config(config)
        write_painter(painter, marker)
        write_sampler(sampler)

        pid = launch_with_trace(config, trace)

        try:
            wait_for_app(pid)
            command = f"{shlex.quote(str(Path('/usr/bin/python3')))} {shlex.quote(str(painter))}"
            terminal_id = create_terminal_window(command)
            wait_for_file(marker, "link painter")
            resize = wait_for_resize(trace)

            focus = focus_evidence(pid)
            focused_bounds = focus["focused_bounds"]
            require(isinstance(focused_bounds, Rect), f"unexpected focused bounds: {focused_bounds}")
            window = focused_window(pid, focused_bounds)

            time.sleep(0.75)
            baseline_width, baseline_height = capture_window_id(window.id, baseline)
            require(
                baseline_width > 0 and baseline_height > 0,
                f"empty baseline screenshot dimensions: {baseline_width}x{baseline_height}",
            )

            attempts = []
            target_cols = [LINK_COL + 4, LINK_COL + 8, LINK_COL + 12, LINK_COL + 20, LINK_COL + 32]
            vertical_offsets = [0, 15, 25, 35, 45, -15, -30, 60, 75, 90, 105]
            for target_col in target_cols:
                top_x, top_y = global_point(
                    window.bounds,
                    LINK_ROW,
                    target_col,
                    resize["cols"],
                    resize["rows"],
                )
                bottom_x, bottom_y = global_point_bottom_origin(
                    window.bounds,
                    LINK_ROW,
                    target_col,
                    resize["cols"],
                    resize["rows"],
                )
                candidates = [("top", top_x, top_y + offset, offset) for offset in vertical_offsets]
                candidates.append(("bottom", bottom_x, bottom_y, 0))
                for origin, x, y, vertical_offset in candidates:
                    inject_move(x, y, "command")
                    time.sleep(0.15)
                    inject_move(x, y, "command")
                    time.sleep(0.35)
                    attempts.append(
                        {
                            "origin": origin,
                            "row": LINK_ROW,
                            "col": target_col,
                            "x": x,
                            "y": y,
                            "vertical_offset": vertical_offset,
                        }
                    )
                    trace_text = trace.read_text(errors="replace") if trace.exists() else ""
                    if hover_trace_seen(trace_text):
                        break
                else:
                    continue
                break

            trace_text = wait_for_trace(
                trace,
                [
                    "cursorShape raw=",
                    "pointerStyle=link",
                    f"mouseOverLink url={URL}",
                ],
                timeout=15,
            )

            time.sleep(1.0)
            hover_width, hover_height = capture_window_id(window.id, hover)
            require(
                (hover_width, hover_height) == (baseline_width, baseline_height),
                f"screenshot size changed: baseline={baseline_width}x{baseline_height} hover={hover_width}x{hover_height}",
            )
            metrics = sample_banner(sampler, baseline, hover)

            data = {
                "pid": pid,
                "terminal_id": terminal_id,
                "window_id": window.id,
                "window_bounds": window.bounds.__dict__,
                "focused_bounds": focused_bounds.__dict__,
                "resize": resize,
                "attempts": attempts,
                "baseline_screenshot": str(baseline),
                "hover_screenshot": str(hover),
                "screenshot_size": {"width": baseline_width, "height": baseline_height},
                "sampler_metrics": metrics,
                "trace_tail": trace_text.splitlines()[-24:],
            }
            evidence.write_text(json.dumps(data, indent=2, sort_keys=True))
            Path("/tmp/termsurf-issue805-exp189-link-banner-latest.json").write_text(
                evidence.read_text()
            )
            Path("/tmp/termsurf-issue805-exp189-link-banner-baseline.png").write_bytes(
                baseline.read_bytes()
            )
            Path("/tmp/termsurf-issue805-exp189-link-banner-hover.png").write_bytes(
                hover.read_bytes()
            )
        finally:
            terminate_process(pid)

    new_crashes = wait_for_crash_report_settle(before_crashes)
    require(
        not new_crashes,
        "Roastty wrote crash reports during live link banner workflow: "
        + ", ".join(str(path) for path in sorted(new_crashes)),
    )

    print("macos_live_link_hover_banner_pixels=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
