#!/usr/bin/env python3
"""Live macOS GUI cursor pixel guard for Issue 805 CFG-223."""

from __future__ import annotations

import json
import os
import shlex
import subprocess
import tempfile
import textwrap
import time
from pathlib import Path

from macos_window_padding_pixel_runtime import (
    APP,
    BINARY,
    ISSUE,
    LIST_WINDOWS,
    Rect,
    WindowInfo,
    capture_window_id,
    crash_reports,
    create_terminal_window,
    focus_evidence,
    focused_window,
    launch_app,
    require,
    terminate_process,
    wait_for_app,
    wait_for_crash_report_settle,
    wait_for_file,
)


ROOT = Path(__file__).resolve().parents[2]
LANDMARK_ROW = 2
LANDMARK_COL = 5
LANDMARK_ROWS = 5
LANDMARK_COLS = 20
CURSOR_ROW = 10
CURSOR_COL = 12


def write_config(path: Path) -> None:
    path.write_text(
        "\n".join(
            [
                "macos-applescript = true",
                "quit-after-last-window-closed = true",
                "cursor-style = block",
                "cursor-style-blink = false",
                "cursor-color = #ff00ff",
                "cursor-text = #00ff00",
                "font-size = 16",
                "window-width = 80",
                "window-height = 24",
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
            bright = "\\x1b[48;2;240;224;32m"
            reset = "\\x1b[0m"
            block = " " * {LANDMARK_COLS}
            for index in range(160):
                sys.stdout.write("\\x1b[2 q\\x1b[?25h\\x1b[?7l\\x1b[2J\\x1b[H")
                for row in range({LANDMARK_ROW}, {LANDMARK_ROW + LANDMARK_ROWS}):
                    sys.stdout.write(f"\\x1b[{{row}};{LANDMARK_COL}H{{bright}}{{block}}{{reset}}")
                sys.stdout.write("\\x1b[{CURSOR_ROW};{CURSOR_COL}H")
                sys.stdout.flush()
                if index == 0:
                    marker.write_text("ready")
                time.sleep(0.1)
            time.sleep(30)
            """
        ).lstrip()
    )


def write_sampler(path: Path) -> None:
    path.write_text(
        r'''
import AppKit
import Foundation

struct RectOut: Codable {
    let x: Int
    let y: Int
    let width: Int
    let height: Int
}

struct Counts: Codable {
    let samples: Int
    let magenta: Int
    let bright: Int
    let background: Int
    let other: Int
}

struct Sample: Codable {
    let rect: RectOut
    let expected: String
    let counts: Counts
}

struct Metrics: Codable {
    let width: Int
    let height: Int
    let landmark: RectOut
    let gridOrigin: [String: Double]
    let cellSize: [String: Double]
    let cursorCell: RectOut
    let expandedCursorCell: RectOut
    let totalMagenta: Int
    let outsideCursorMagenta: Int
    let samples: [String: Sample]
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write((message + "\n").data(using: .utf8)!)
    exit(1)
}

guard CommandLine.arguments.count == 8 else {
    fail("usage: sampler.swift <png> <landmark-row> <landmark-col> <landmark-rows> <landmark-cols> <cursor-row> <cursor-col>")
}
guard let image = NSImage(contentsOfFile: CommandLine.arguments[1]),
      let tiff = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff)
else {
    fail("failed to load image")
}

let landmarkRow = Int(CommandLine.arguments[2])!
let landmarkCol = Int(CommandLine.arguments[3])!
let landmarkRows = Int(CommandLine.arguments[4])!
let landmarkCols = Int(CommandLine.arguments[5])!
let cursorRow = Int(CommandLine.arguments[6])!
let cursorCol = Int(CommandLine.arguments[7])!
let width = bitmap.pixelsWide
let height = bitmap.pixelsHigh

func colorAt(_ x: Int, _ y: Int) -> (Int, Int, Int) {
    guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else {
        return (0, 0, 0)
    }
    return (
        Int((color.redComponent * 255).rounded()),
        Int((color.greenComponent * 255).rounded()),
        Int((color.blueComponent * 255).rounded())
    )
}

func isMagenta(_ x: Int, _ y: Int) -> Bool {
    let (r, g, b) = colorAt(x, y)
    return r >= 175 && g <= 95 && b >= 175
}

func isBright(_ x: Int, _ y: Int) -> Bool {
    let (r, g, b) = colorAt(x, y)
    return r >= 185 && g >= 160 && b <= 120
}

func isBackground(_ x: Int, _ y: Int) -> Bool {
    let (r, g, b) = colorAt(x, y)
    return r >= 5 && r <= 45 && g >= 20 && g <= 70 && b >= 35 && b <= 95 && g >= r + 8 && b >= g + 8
}

var rowBright = Array(repeating: 0, count: height)
var colBright = Array(repeating: 0, count: width)
var brightPixels = 0
for y in stride(from: 0, to: height, by: 2) {
    for x in stride(from: 0, to: width, by: 2) {
        if isBright(x, y) {
            brightPixels += 1
            rowBright[y] += 1
            colBright[x] += 1
        }
    }
}
if brightPixels < 500 {
    fail("not enough bright pixels to identify landmark: \(brightPixels)")
}

let rowThreshold = max(80, width / 24)
let colThreshold = max(25, height / 48)
var minX = width
var maxX = -1
var minY = height
var maxY = -1
for y in 0..<height {
    if rowBright[y] >= rowThreshold {
        minY = min(minY, y)
        maxY = max(maxY, y)
    }
}
for x in 0..<width {
    if colBright[x] >= colThreshold {
        minX = min(minX, x)
        maxX = max(maxX, x)
    }
}
if minX > maxX || minY > maxY {
    fail("could not identify broad bright landmark bounds")
}

let landmark = RectOut(x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1)
if landmark.width < 120 || landmark.height < 45 {
    fail("landmark bounds too small: \(landmark)")
}

let cellWidth = Double(landmark.width) / Double(landmarkCols)
let cellHeight = Double(landmark.height) / Double(landmarkRows)
if cellWidth < 4 || cellHeight < 8 {
    fail("cell size implausible: \(cellWidth)x\(cellHeight)")
}
let originX = Double(landmark.x) - Double(landmarkCol - 1) * cellWidth
let originY = Double(landmark.y) - Double(landmarkRow - 1) * cellHeight

func clamp(_ value: Int, _ lower: Int, _ upper: Int) -> Int {
    return min(max(value, lower), upper)
}

func makeRect(x0: Double, y0: Double, x1: Double, y1: Double) -> RectOut {
    let left = clamp(Int(floor(min(x0, x1))), 0, width - 1)
    let right = clamp(Int(ceil(max(x0, x1))), 0, width - 1)
    let top = clamp(Int(floor(min(y0, y1))), 0, height - 1)
    let bottom = clamp(Int(ceil(max(y0, y1))), 0, height - 1)
    if right <= left || bottom <= top {
        fail("empty sample rect: \(x0),\(y0) \(x1),\(y1)")
    }
    return RectOut(x: left, y: top, width: right - left + 1, height: bottom - top + 1)
}

func cellRect(row: Int, col: Int, inset: Double = 0.15) -> RectOut {
    let x0 = originX + Double(col - 1) * cellWidth
    let y0 = originY + Double(row - 1) * cellHeight
    return makeRect(
        x0: x0 + cellWidth * inset,
        y0: y0 + cellHeight * inset,
        x1: x0 + cellWidth * (1.0 - inset),
        y1: y0 + cellHeight * (1.0 - inset)
    )
}

func count(_ rect: RectOut) -> Counts {
    var samples = 0
    var magenta = 0
    var bright = 0
    var background = 0
    var other = 0
    let stepX = max(1, rect.width / 12)
    let stepY = max(1, rect.height / 12)
    for y in stride(from: rect.y, through: rect.y + rect.height - 1, by: stepY) {
        for x in stride(from: rect.x, through: rect.x + rect.width - 1, by: stepX) {
            samples += 1
            if isMagenta(x, y) {
                magenta += 1
            } else if isBright(x, y) {
                bright += 1
            } else if isBackground(x, y) {
                background += 1
            } else {
                other += 1
            }
        }
    }
    return Counts(samples: samples, magenta: magenta, bright: bright, background: background, other: other)
}

func contains(_ rect: RectOut, x: Int, y: Int) -> Bool {
    return x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

let cursor = cellRect(row: cursorRow, col: cursorCol, inset: 0.10)
let expandedCursor = makeRect(
    x0: Double(cursor.x) - cellWidth * 0.75,
    y0: Double(cursor.y) - cellHeight * 0.75,
    x1: Double(cursor.x + cursor.width) + cellWidth * 0.75,
    y1: Double(cursor.y + cursor.height) + cellHeight * 0.75
)

let rects: [String: (RectOut, String)] = [
    "cursor": (cursor, "magenta"),
    "left_background": (cellRect(row: cursorRow, col: cursorCol - 2, inset: 0.20), "background"),
    "right_background": (cellRect(row: cursorRow, col: cursorCol + 2, inset: 0.20), "background"),
    "landmark": (cellRect(row: landmarkRow + 1, col: landmarkCol + 4, inset: 0.20), "bright"),
]

var samples: [String: Sample] = [:]
for (name, entry) in rects {
    let counts = count(entry.0)
    if counts.samples == 0 {
        fail("no samples for \(name)")
    }
    if entry.1 == "magenta" {
        if Double(counts.magenta) / Double(counts.samples) < 0.60 {
            fail("\(name) is not magenta-dominant: \(counts)")
        }
    } else if entry.1 == "background" {
        if Double(counts.background) / Double(counts.samples) < 0.55 {
            fail("\(name) is not background-dominant: \(counts)")
        }
        if Double(counts.magenta) / Double(counts.samples) > 0.05 {
            fail("\(name) unexpectedly looks magenta: \(counts)")
        }
    } else if entry.1 == "bright" {
        if Double(counts.bright) / Double(counts.samples) < 0.60 {
            fail("\(name) is not bright-dominant: \(counts)")
        }
        if Double(counts.magenta) / Double(counts.samples) > 0.05 {
            fail("\(name) unexpectedly looks magenta: \(counts)")
        }
    }
    samples[name] = Sample(rect: entry.0, expected: entry.1, counts: counts)
}

var totalMagenta = 0
var outsideCursorMagenta = 0
for y in stride(from: 0, to: height, by: 2) {
    for x in stride(from: 0, to: width, by: 2) {
        if isMagenta(x, y) {
            totalMagenta += 1
            if !contains(expandedCursor, x: x, y: y) {
                outsideCursorMagenta += 1
            }
        }
    }
}
if totalMagenta < 40 {
    fail("not enough magenta pixels for visible cursor: \(totalMagenta)")
}
if outsideCursorMagenta > max(20, totalMagenta / 5) {
    fail("too many magenta pixels outside expected cursor region: \(outsideCursorMagenta) of \(totalMagenta)")
}

let metrics = Metrics(
    width: width,
    height: height,
    landmark: landmark,
    gridOrigin: ["x": originX, "y": originY],
    cellSize: ["width": cellWidth, "height": cellHeight],
    cursorCell: cursor,
    expandedCursorCell: expandedCursor,
    totalMagenta: totalMagenta,
    outsideCursorMagenta: outsideCursorMagenta,
    samples: samples
)
let encoder = JSONEncoder()
encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
let data = try! encoder.encode(metrics)
print(String(data: data, encoding: .utf8)!)
'''
    )


def sample_cursor(sampler: Path, screenshot: Path) -> dict[str, object]:
    result = subprocess.run(
        [
            "swift",
            str(sampler),
            str(screenshot),
            str(LANDMARK_ROW),
            str(LANDMARK_COL),
            str(LANDMARK_ROWS),
            str(LANDMARK_COLS),
            str(CURSOR_ROW),
            str(CURSOR_COL),
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=30,
    )
    if result.returncode != 0:
        raise AssertionError(
            "sampler failed\n"
            f"stdout:\n{result.stdout}\n"
            f"stderr:\n{result.stderr}"
        )
    return json.loads(result.stdout)


def assert_inventory_split() -> None:
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()
    remaining_row = next(
        (
            line
            for line in runtime_inventory.splitlines()
            if line.startswith("| RUNTIME-008B2B2B2B2B4 ")
        ),
        "",
    )
    cfg223 = next(
        (
            [cell.strip() for cell in line.strip().strip("|").split("|")]
            for line in config_matrix.splitlines()
            if line.startswith("| CFG-223 |")
        ),
        None,
    )

    require("| RUNTIME-008B2B2B2B2D" in runtime_inventory, "missing GUI cursor pixel row")
    require("focused live app/GUI block cursor pixel proof" in runtime_inventory, "missing cursor pixel evidence")
    require("magenta-dominant" in runtime_inventory, "missing magenta cursor evidence")
    require("geometry-derived sample rectangles" in runtime_inventory, "missing geometry-derived sample evidence")
    require(remaining_row, "missing renderer residual row")
    require("scroll-to-bottom.output" in remaining_row, "scroll-to-bottom row missing evidence")
    require("GUI cursor pixels" not in remaining_row, "remaining renderer gap still claims GUI cursor pixels")
    require("89 rows Oracle complete" in config_matrix, "CFG-223 oracle count not updated")
    require("92 rows closed" in config_matrix, "CFG-223 closed count not updated")
    require("1 rows are incomplete" in config_matrix, "CFG-223 incomplete count changed")
    require("1 rows are runtime gaps" in config_matrix, "CFG-223 gap count changed")
    require(cfg223 is not None and len(cfg223) > 4 and cfg223[4] == "Gap", "CFG-223 should remain Gap")


def write_debug_artifacts(
    screenshot: Path,
    metrics: dict[str, object],
    evidence: dict[str, object],
    window: WindowInfo,
    terminal_id: str,
    command_path: Path,
    marker_path: Path,
) -> None:
    debug_png = Path("/tmp/termsurf-issue805-exp178-gui-cursor.png")
    debug_json = Path("/tmp/termsurf-issue805-exp178-gui-cursor.json")
    debug_png.write_bytes(screenshot.read_bytes())
    debug_json.write_text(
        json.dumps(
            {
                "terminal_id": terminal_id,
                "command_path": str(command_path),
                "marker_path": str(marker_path),
                "window_id": window.id,
                "window": {
                    "x": window.bounds.x,
                    "y": window.bounds.y,
                    "width": window.bounds.width,
                    "height": window.bounds.height,
                    "layer": window.layer,
                    "name": window.name,
                },
                "focus": evidence,
                "cursor": {
                    "row": CURSOR_ROW,
                    "col": CURSOR_COL,
                    "color": "#ff00ff",
                    "style": "block",
                },
                "landmark": {
                    "row": LANDMARK_ROW,
                    "col": LANDMARK_COL,
                    "rows": LANDMARK_ROWS,
                    "cols": LANDMARK_COLS,
                    "color": "#f0e020",
                },
                "metrics": metrics,
            },
            sort_keys=True,
            indent=2,
        )
        + "\n"
    )


def main() -> int:
    require(APP.is_dir(), f"app not built: {APP}")
    require(BINARY.is_file(), f"app binary not built: {BINARY}")
    require(LIST_WINDOWS.is_file(), f"window list helper missing: {LIST_WINDOWS}")

    crash_reports_before = crash_reports()

    with tempfile.TemporaryDirectory(prefix="termsurf-issue805-exp178-") as temp_dir:
        temp = Path(temp_dir)
        config = temp / "config.roastty"
        painter = temp / "paint_cursor.py"
        marker = temp / "painter-ready.txt"
        sampler = temp / "sample_cursor.swift"
        screenshot = temp / "gui-cursor.png"
        write_config(config)
        write_painter(painter, marker)
        write_sampler(sampler)

        pid = launch_app(config)
        terminal_id = ""
        try:
            wait_for_app(pid)
            command = f"python3 {shlex.quote(str(painter))}"
            terminal_id = create_terminal_window(command)
            wait_for_file(marker, "cursor painter")
            time.sleep(2)
            evidence = focus_evidence(pid)
            focused_bounds = evidence.pop("focused_bounds")
            require(isinstance(focused_bounds, Rect), f"unexpected focused bounds: {focused_bounds!r}")
            window = focused_window(pid, focused_bounds)
            width, height = capture_window_id(window.id, screenshot)
            require(width > 0 and height > 0, f"cursor screenshot dimensions were empty: {width}x{height}")
            Path("/tmp/termsurf-issue805-exp178-gui-cursor-latest.png").write_bytes(screenshot.read_bytes())
            metrics = sample_cursor(sampler, screenshot)
            require(metrics["width"] == width and metrics["height"] == height, "sampler dimensions mismatch")
            write_debug_artifacts(screenshot, metrics, evidence, window, terminal_id, painter, marker)
        finally:
            terminate_process(pid)

    new_crash_reports = wait_for_crash_report_settle(crash_reports_before)
    require(
        not new_crash_reports,
        "Roastty wrote crash reports during GUI cursor pixel workflow: "
        + ", ".join(str(path) for path in sorted(new_crash_reports)),
    )

    assert_inventory_split()
    print(f"macos_gui_cursor_pixel_runtime=pass terminal={terminal_id}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
