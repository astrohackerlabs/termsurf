#!/usr/bin/env python3
"""Live macOS window-padding pixel guard for Issue 805 CFG-223."""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
import re
import shlex
import subprocess
import tempfile
import textwrap
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ISSUE = ROOT / "issues/0805-roastty-ghostty-parity"
APP = ROOT / "roastty/macos/build/Debug/Roastty.app"
BINARY = APP / "Contents/MacOS/roastty"
LIST_WINDOWS = ROOT / "scripts/roastty-app/list-windows.swift"
DIAGNOSTIC_REPORTS = Path.home() / "Library/Logs/DiagnosticReports"

PAD_LEFT = 96
PAD_RIGHT = 64
PAD_TOP = 72
PAD_BOTTOM = 136


@dataclass(frozen=True)
class Rect:
    x: int
    y: int
    width: int
    height: int


@dataclass(frozen=True)
class WindowInfo:
    id: int
    layer: int
    bounds: Rect
    name: str


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def run_osascript(script: str, timeout: int = 30) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        ["osascript", "-e", script],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=timeout,
    )
    if result.returncode != 0:
        raise AssertionError(
            "osascript failed\n"
            f"stdout:\n{result.stdout}\n"
            f"stderr:\n{result.stderr}\n"
            f"script:\n{script}"
        )
    return result


def quote_applescript(value: str | Path) -> str:
    text = str(value)
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def scoped_pids() -> set[int]:
    scoped = subprocess.run(
        ["pgrep", "-f", f"{APP}/Contents/MacOS/roastty"],
        text=True,
        capture_output=True,
    )
    return {int(pid_text) for pid_text in scoped.stdout.split()}


def crash_reports() -> set[Path]:
    if not DIAGNOSTIC_REPORTS.is_dir():
        return set()
    return set(DIAGNOSTIC_REPORTS.glob("roastty-*.ips"))


def wait_for_crash_report_settle(before: set[Path]) -> set[Path]:
    deadline = time.monotonic() + 5
    observed: set[Path] = set()
    while time.monotonic() < deadline:
        time.sleep(0.5)
        observed.update(crash_reports() - before)
    return observed


def launch_app(config: Path) -> int:
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
            "ROASTTY_USER_DEFAULTS_SUITE=com.termsurf.roastty.issue805.exp177",
            str(APP),
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
    )
    if result.returncode != 0:
        raise AssertionError(
            "open failed\n"
            f"stdout:\n{result.stdout}\n"
            f"stderr:\n{result.stderr}"
        )

    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        after = scoped_pids()
        created = sorted(after - before)
        if created:
            return created[0]
        time.sleep(0.25)
    raise AssertionError("open did not start a scoped debug Roastty process")


def wait_for_app(pid: int, timeout: float = 20.0) -> None:
    deadline = time.monotonic() + timeout
    app_literal = quote_applescript(APP)
    while time.monotonic() < deadline:
        if subprocess.run(["ps", "-p", str(pid)], stdout=subprocess.DEVNULL).returncode != 0:
            raise AssertionError("Roastty debug process exited before AppleScript was ready")
        try:
            result = run_osascript(f"tell application {app_literal} to count of windows", timeout=5)
        except (AssertionError, subprocess.TimeoutExpired):
            time.sleep(0.5)
            continue
        if result.stdout.strip().isdigit():
            return
        time.sleep(0.5)
    raise AssertionError("Roastty did not become AppleScript-addressable in time")


def terminate_process(pid: int) -> None:
    try:
        try:
            run_osascript(f"tell application {quote_applescript(APP)} to quit", timeout=5)
        except Exception:
            pass
        for _ in range(20):
            if pid not in scoped_pids():
                return
            time.sleep(0.25)
    finally:
        if pid in scoped_pids():
            try:
                os.kill(pid, 9)
            except ProcessLookupError:
                pass


def parse_windows(stdout: str) -> list[WindowInfo]:
    pattern = re.compile(
        r'id=(?P<id>\d+) layer=(?P<layer>-?\d+) bounds=\((?P<x>-?\d+),(?P<y>-?\d+) '
        r'(?P<w>\d+)x(?P<h>\d+)\) name="(?P<name>.*)"'
    )
    windows = []
    for line in stdout.splitlines():
        match = pattern.fullmatch(line.strip())
        if not match:
            continue
        windows.append(
            WindowInfo(
                id=int(match.group("id")),
                layer=int(match.group("layer")),
                bounds=Rect(
                    x=int(match.group("x")),
                    y=int(match.group("y")),
                    width=int(match.group("w")),
                    height=int(match.group("h")),
                ),
                name=match.group("name"),
            )
        )
    return windows


def windows_for_pid(pid: int) -> list[WindowInfo]:
    result = subprocess.run(
        ["swift", str(LIST_WINDOWS), str(pid)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=20,
    )
    if result.returncode != 0:
        raise AssertionError(
            "list-windows failed\n"
            f"stdout:\n{result.stdout}\n"
            f"stderr:\n{result.stderr}"
        )
    return parse_windows(result.stdout)


def focused_window(pid: int, focused_bounds: Rect) -> WindowInfo:
    windows = [
        window
        for window in windows_for_pid(pid)
        if window.layer == 0
        and window.bounds.width >= 300
        and window.bounds.height >= 200
        and window.bounds == focused_bounds
    ]
    require(
        windows,
        f"no PID-owned layer-0 window for {pid} matched focused bounds {focused_bounds}",
    )
    require(
        len(windows) == 1,
        f"expected exactly one focused PID-owned layer-0 window for {pid}, got {windows}",
    )
    return windows[0]


def capture_window_id(window_id: int, output: Path) -> tuple[int, int]:
    result = subprocess.run(
        ["screencapture", "-x", "-o", f"-l{window_id}", str(output)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=30,
    )
    if result.returncode != 0:
        raise AssertionError(
            "screencapture failed\n"
            f"stdout:\n{result.stdout}\n"
            f"stderr:\n{result.stderr}"
        )
    require(output.is_file(), f"screenshot missing: {output}")
    width = subprocess.run(
        ["sips", "-g", "pixelWidth", str(output)],
        text=True,
        capture_output=True,
        timeout=10,
    )
    height = subprocess.run(
        ["sips", "-g", "pixelHeight", str(output)],
        text=True,
        capture_output=True,
        timeout=10,
    )
    require(width.returncode == 0 and height.returncode == 0, "sips failed to read screenshot dimensions")
    width_match = re.search(r"pixelWidth:\s+(\d+)", width.stdout)
    height_match = re.search(r"pixelHeight:\s+(\d+)", height.stdout)
    require(width_match is not None and height_match is not None, "could not parse screenshot dimensions")
    return int(width_match.group(1)), int(height_match.group(1))


def wait_for_file(path: Path, description: str, timeout: float = 10.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.is_file() and path.read_text().strip() == "ready":
            return
        time.sleep(0.25)
    raise AssertionError(f"timed out waiting for {description}: {path}")


def create_terminal_window(command: str) -> str:
    app_literal = quote_applescript(APP)
    command_literal = quote_applescript(command)
    script = textwrap.dedent(
        f"""
        tell application {app_literal}
          activate
          set cfg to new surface configuration from {{command:{command_literal}, wait after command:true}}
          new window with configuration cfg
          delay 1
          set w to front window
          set t0 to focused terminal of selected tab of w
          if (id of t0) is "" then error "terminal id was empty"
          return id of t0
        end tell
        """
    )
    return run_osascript(script, timeout=30).stdout.strip()


def focus_evidence(pid: int) -> dict[str, object]:
    app_literal = quote_applescript(APP)
    run_osascript(f"tell application {app_literal} to activate", timeout=10)
    script = textwrap.dedent(
        f"""
        tell application "System Events"
          set roasttyProc to first application process whose unix id is {pid}
          set frontmost of roasttyProc to true
          delay 0.25
          perform action "AXRaise" of window 1 of roasttyProc
          delay 0.25
          set frontPID to unix id of first application process whose frontmost is true
          if frontPID is not {pid} then error "frontmost PID mismatch: " & frontPID
          set mainValue to value of attribute "AXMain" of window 1 of roasttyProc
          set focusedWindow to value of attribute "AXFocusedWindow" of roasttyProc
          set focusedWindowMainValue to value of attribute "AXMain" of focusedWindow
          set focusedPosition to value of attribute "AXPosition" of focusedWindow
          set focusedSize to value of attribute "AXSize" of focusedWindow
          set focusedX to item 1 of focusedPosition as integer
          set focusedY to item 2 of focusedPosition as integer
          set focusedWidth to item 1 of focusedSize as integer
          set focusedHeight to item 2 of focusedSize as integer
          return (frontPID as text) & linefeed & (mainValue as text) & linefeed & (focusedWindowMainValue as text) & linefeed & (focusedX as text) & linefeed & (focusedY as text) & linefeed & (focusedWidth as text) & linefeed & (focusedHeight as text)
        end tell
        """
    )
    result = run_osascript(script, timeout=15)
    parts = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    require(len(parts) == 7, f"unexpected focus evidence: {result.stdout!r}")
    require(int(parts[0]) == pid, f"frontmost PID mismatch: {parts}")
    require(parts[1].lower() == "true", f"AXMain should be true: {parts}")
    require(parts[2].lower() == "true", f"AXFocusedWindow AXMain should be true: {parts}")
    bounds = Rect(
        x=int(parts[3]),
        y=int(parts[4]),
        width=int(parts[5]),
        height=int(parts[6]),
    )
    return {
        "front_pid": int(parts[0]),
        "ax_main": parts[1],
        "focused_window_ax_main": parts[2],
        "focused_window_bounds": {
            "x": bounds.x,
            "y": bounds.y,
            "width": bounds.width,
            "height": bounds.height,
        },
        "focused_bounds": bounds,
    }


def write_painter(path: Path, marker: Path) -> None:
    path.write_text(
        textwrap.dedent(
            f"""
            from pathlib import Path
            import sys
            import time

            Path({str(marker)!r}).write_text("ready")
            bright = "\\x1b[48;2;240;224;32m"
            reset = "\\x1b[0m"
            line = " " * 320
            for _ in range(120):
                sys.stdout.write("\\x1b[?25l\\x1b[?7l\\x1b[H")
                for row in range(1, 80):
                    sys.stdout.write(f"\\x1b[{{row}};1H{{bright}}{{line}}{{reset}}")
                sys.stdout.flush()
                time.sleep(0.1)
            time.sleep(30)
            """
        ).lstrip()
    )


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
                f"window-padding-x = {PAD_LEFT},{PAD_RIGHT}",
                f"window-padding-y = {PAD_TOP},{PAD_BOTTOM}",
                "window-padding-balance = false",
                "window-padding-color = background",
                "",
            ]
        )
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
    let brightBounds: RectOut
    let gaps: [String: Int]
    let expectedPaddingPixels: [String: Int]
    let expectedEdges: [String: Int]
    let samples: [String: Sample]
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write((message + "\n").data(using: .utf8)!)
    exit(1)
}

guard CommandLine.arguments.count == 8 else {
    fail("usage: sampler.swift <png> <left> <right> <top> <bottom> <scaleX> <scaleY>")
}
guard let image = NSImage(contentsOfFile: CommandLine.arguments[1]),
      let tiff = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff)
else {
    fail("failed to load image")
}

let expectedLeft = Int((Double(CommandLine.arguments[2])! * Double(CommandLine.arguments[6])!).rounded())
let expectedRight = Int((Double(CommandLine.arguments[3])! * Double(CommandLine.arguments[6])!).rounded())
let expectedTop = Int((Double(CommandLine.arguments[4])! * Double(CommandLine.arguments[7])!).rounded())
let expectedBottom = Int((Double(CommandLine.arguments[5])! * Double(CommandLine.arguments[7])!).rounded())
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

func isBright(_ x: Int, _ y: Int) -> Bool {
    let (r, g, b) = colorAt(x, y)
    return r >= 185 && g >= 160 && b <= 110
}

func isBackground(_ x: Int, _ y: Int) -> Bool {
    let (r, g, b) = colorAt(x, y)
    return r >= 5 && r <= 45 && g >= 20 && g <= 70 && b >= 35 && b <= 95 && g >= r + 8 && b >= g + 8
}

var minX = width
var minY = height
var maxX = -1
var maxY = -1
var brightPixels = 0
var rowBright = Array(repeating: 0, count: height)
var colBright = Array(repeating: 0, count: width)
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
    fail("not enough bright pixels to identify terminal content: \(brightPixels)")
}
let rowThreshold = max(120, width / 12)
let colThreshold = max(80, height / 16)
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
    fail("could not identify broad terminal content bounds")
}

func clamp(_ value: Int, _ lower: Int, _ upper: Int) -> Int {
    return min(max(value, lower), upper)
}

func makeRect(x0: Int, y0: Int, x1: Int, y1: Int) -> RectOut {
    let left = clamp(min(x0, x1), 0, width - 1)
    let right = clamp(max(x0, x1), 0, width - 1)
    let top = clamp(min(y0, y1), 0, height - 1)
    let bottom = clamp(max(y0, y1), 0, height - 1)
    if right <= left || bottom <= top {
        fail("empty sample rect: \(x0),\(y0) \(x1),\(y1)")
    }
    return RectOut(x: left, y: top, width: right - left + 1, height: bottom - top + 1)
}

func count(_ rect: RectOut) -> Counts {
    var samples = 0
    var bright = 0
    var background = 0
    var other = 0
    let stepX = max(1, rect.width / 16)
    let stepY = max(1, rect.height / 16)
    for y in stride(from: rect.y, through: rect.y + rect.height - 1, by: stepY) {
        for x in stride(from: rect.x, through: rect.x + rect.width - 1, by: stepX) {
            samples += 1
            if isBright(x, y) {
                bright += 1
            } else if isBackground(x, y) {
                background += 1
            } else {
                other += 1
            }
        }
    }
    return Counts(samples: samples, bright: bright, background: background, other: other)
}

let brightBounds = RectOut(x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1)
let centralX0 = max(0, expectedLeft + 80)
let centralX1 = min(width - 1, width - expectedRight - 80)
if centralX1 <= centralX0 {
    fail("central terminal background probe is empty")
}
var terminalAreaTop = -1
for y in 0..<height {
    var samples = 0
    var background = 0
    let step = max(1, (centralX1 - centralX0) / 40)
    for x in stride(from: centralX0, through: centralX1, by: step) {
        samples += 1
        if isBackground(x, y) {
            background += 1
        }
    }
    if samples > 0 && Double(background) / Double(samples) >= 0.80 {
        terminalAreaTop = y
        break
    }
}
if terminalAreaTop < 0 {
    fail("could not identify terminal background area top")
}
let gaps = [
    "left": minX,
    "right": width - 1 - maxX,
    "top": minY,
    "bottom": height - 1 - maxY,
]
let expected = [
    "left": expectedLeft,
    "right": expectedRight,
    "top": expectedTop,
    "bottom": expectedBottom,
]

func requireNear(_ observed: Int, _ expected: Int, _ edge: String) {
    let tolerance = max(10, Int(Double(max(expected, 1)) * 0.20))
    if abs(observed - expected) > tolerance {
        fail("\(edge) observed \(observed) differs from expected \(expected) beyond tolerance \(tolerance)")
    }
}

let strip = 18
let inset = 10
let expectedGridLeft = expectedLeft
let expectedGridTop = terminalAreaTop + expectedTop
let expectedGridRightEdge = width - 1 - expectedRight
let expectedGridBottomEdge = height - 1 - expectedBottom

requireNear(minX, expectedGridLeft, "left content edge")
requireNear(minY, expectedGridTop, "top content edge")
requireNear(width - 1 - maxX, expectedRight, "right content edge")
requireNear(height - 1 - maxY, expectedBottom, "bottom content edge")

let horizontalY0 = expectedGridTop + max(inset, brightBounds.height / 5)
let horizontalY1 = expectedGridBottomEdge - max(inset, brightBounds.height / 5)
let verticalX0 = expectedGridLeft + max(inset, brightBounds.width / 5)
let verticalX1 = expectedGridRightEdge - max(inset, brightBounds.width / 5)

let rects: [String: (RectOut, String)] = [
    "left_padding": (makeRect(x0: expectedGridLeft - max(strip, expectedLeft / 3), y0: horizontalY0, x1: expectedGridLeft - 4, y1: horizontalY1), "background"),
    "right_padding": (makeRect(x0: expectedGridRightEdge + 4, y0: horizontalY0, x1: expectedGridRightEdge + max(strip, expectedRight / 3), y1: horizontalY1), "background"),
    "top_padding": (makeRect(x0: verticalX0, y0: expectedGridTop - max(strip, expectedTop / 3), x1: verticalX1, y1: expectedGridTop - 4), "background"),
    "bottom_padding": (makeRect(x0: verticalX0, y0: expectedGridBottomEdge + 4, x1: verticalX1, y1: expectedGridBottomEdge + max(strip, expectedBottom / 3)), "background"),
    "left_content": (makeRect(x0: expectedGridLeft + 4, y0: horizontalY0, x1: expectedGridLeft + strip, y1: horizontalY1), "bright"),
    "right_content": (makeRect(x0: expectedGridRightEdge - strip, y0: horizontalY0, x1: expectedGridRightEdge - 4, y1: horizontalY1), "bright"),
    "top_content": (makeRect(x0: verticalX0, y0: expectedGridTop + 4, x1: verticalX1, y1: expectedGridTop + strip), "bright"),
    "bottom_content": (makeRect(x0: verticalX0, y0: expectedGridBottomEdge - strip, x1: verticalX1, y1: expectedGridBottomEdge - 4), "bright"),
]

var samples: [String: Sample] = [:]
for (name, entry) in rects {
    let counts = count(entry.0)
    if counts.samples == 0 {
        fail("no samples for \(name)")
    }
    if entry.1 == "background" {
        if Double(counts.background) / Double(counts.samples) < 0.70 {
            fail("\(name) is not background-dominant: \(counts)")
        }
        if Double(counts.bright) / Double(counts.samples) > 0.10 {
            fail("\(name) also looks bright: \(counts)")
        }
    } else {
        if Double(counts.bright) / Double(counts.samples) < 0.70 {
            fail("\(name) is not bright-dominant: \(counts)")
        }
        if Double(counts.background) / Double(counts.samples) > 0.20 {
            fail("\(name) also looks background: \(counts)")
        }
    }
    samples[name] = Sample(rect: entry.0, expected: entry.1, counts: counts)
}

let metrics = Metrics(
    width: width,
    height: height,
    brightBounds: brightBounds,
    gaps: gaps,
    expectedPaddingPixels: expected,
    expectedEdges: [
        "left": expectedGridLeft,
        "right": expectedGridRightEdge,
        "top": expectedGridTop,
        "bottom": expectedGridBottomEdge,
        "terminalAreaTop": terminalAreaTop,
    ],
    samples: samples
)
let encoder = JSONEncoder()
encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
let data = try! encoder.encode(metrics)
print(String(data: data, encoding: .utf8)!)
'''
    )


def sample_padding(
    sampler: Path,
    screenshot: Path,
    image_width: int,
    image_height: int,
    focused_bounds: Rect,
) -> dict[str, object]:
    scale_x = image_width / focused_bounds.width
    scale_y = image_height / focused_bounds.height
    result = subprocess.run(
        [
            "swift",
            str(sampler),
            str(screenshot),
            str(PAD_LEFT),
            str(PAD_RIGHT),
            str(PAD_TOP),
            str(PAD_BOTTOM),
            f"{scale_x:.6f}",
            f"{scale_y:.6f}",
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
    residual_row = next(
        (
            line
            for line in runtime_inventory.splitlines()
            if line.startswith("| RUNTIME-008B2B2B2B2B ")
        ),
        None,
    )
    cfg223 = next(
        (
            [cell.strip() for cell in line.strip().strip("|").split("|")]
            for line in config_matrix.splitlines()
            if line.startswith("| CFG-223 |")
        ),
        None,
    )

    require("| RUNTIME-008B2B2B2B2C" in runtime_inventory, "missing padding pixel row")
    require("focused live window-padding pixel proof" in runtime_inventory, "missing padding pixel evidence")
    require("top/bottom/left/right" in runtime_inventory, "missing four-edge padding evidence")
    require("geometry-derived sample rectangles" in runtime_inventory, "missing geometry-derived sample evidence")
    require(residual_row is not None, "missing renderer residual row")
    require("scroll-to-bottom.output" in residual_row, "concrete renderer gap missing evidence")
    require("background-image-opacity" not in residual_row, "background image still in renderer residual")
    require("80 rows Oracle complete" in config_matrix, "CFG-223 oracle count not updated")
    require("83 rows closed" in config_matrix, "CFG-223 closed count not updated")
    require("4 rows are incomplete" in config_matrix, "CFG-223 incomplete count changed")
    require("4 rows are runtime gaps" in config_matrix, "CFG-223 gap count changed")
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
    debug_png = Path("/tmp/termsurf-issue805-exp177-window-padding.png")
    debug_json = Path("/tmp/termsurf-issue805-exp177-window-padding.json")
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
                "configured_padding": {
                    "left": PAD_LEFT,
                    "right": PAD_RIGHT,
                    "top": PAD_TOP,
                    "bottom": PAD_BOTTOM,
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

    with tempfile.TemporaryDirectory(prefix="termsurf-issue805-exp177-") as temp_dir:
        temp = Path(temp_dir)
        config = temp / "config.roastty"
        painter = temp / "paint_padding.py"
        marker = temp / "painter-ready.txt"
        sampler = temp / "sample_padding.swift"
        screenshot = temp / "window-padding.png"
        write_config(config)
        write_painter(painter, marker)
        write_sampler(sampler)

        pid = launch_app(config)
        terminal_id = ""
        try:
            wait_for_app(pid)
            command = f"python3 {shlex.quote(str(painter))}"
            terminal_id = create_terminal_window(command)
            wait_for_file(marker, "padding painter")
            time.sleep(2)
            evidence = focus_evidence(pid)
            focused_bounds = evidence.pop("focused_bounds")
            require(isinstance(focused_bounds, Rect), f"unexpected focused bounds: {focused_bounds!r}")
            window = focused_window(pid, focused_bounds)
            width, height = capture_window_id(window.id, screenshot)
            require(width > 0 and height > 0, f"padding screenshot dimensions were empty: {width}x{height}")
            metrics = sample_padding(sampler, screenshot, width, height, focused_bounds)
            require(metrics["width"] == width and metrics["height"] == height, "sampler dimensions mismatch")
            write_debug_artifacts(screenshot, metrics, evidence, window, terminal_id, painter, marker)
        finally:
            terminate_process(pid)

    new_crash_reports = wait_for_crash_report_settle(crash_reports_before)
    require(
        not new_crash_reports,
        "Roastty wrote crash reports during padding pixel workflow: "
        + ", ".join(str(path) for path in sorted(new_crash_reports)),
    )

    assert_inventory_split()
    print(f"macos_window_padding_pixel_runtime=pass terminal={terminal_id}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
