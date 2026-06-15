#!/usr/bin/env python3
"""Live macOS split-layout GUI guard for Issue 805 CFG-223."""

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


@dataclass(frozen=True)
class Rect:
    x: int
    y: int
    width: int
    height: int

    @property
    def area(self) -> int:
        return self.width * self.height


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
            "ROASTTY_USER_DEFAULTS_SUITE=com.termsurf.roastty.issue805.exp175",
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


def primary_window(pid: int) -> WindowInfo:
    windows = [
        window
        for window in windows_for_pid(pid)
        if window.layer == 0 and window.bounds.width >= 300 and window.bounds.height >= 200
    ]
    require(windows, f"no primary PID-owned layer-0 window for {pid}")
    return max(windows, key=lambda window: window.bounds.area)


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


def write_painter(path: Path, marker: Path, color_sequence: str, background: str) -> None:
    path.write_text(
        textwrap.dedent(
            f"""
            from pathlib import Path
            import sys
            import time

            Path({str(marker)!r}).write_text("ready")
            block = (" " * 260 + "\\n") * 80
            for _ in range(80):
                sys.stdout.write("\\x1b[?25l\\x1b]11;{background}\\x07\\x1b[H\\x1b[{color_sequence}m" + block)
                sys.stdout.flush()
                time.sleep(0.2)
            time.sleep(30)
            """
        ).lstrip()
    )


def write_sampler(path: Path) -> None:
    path.write_text(
        r'''
import AppKit
import Foundation

struct Counts: Codable {
    let samples: Int
    let redDominant: Int
    let blueDominant: Int
    let other: Int
}

struct Metrics: Codable {
    let width: Int
    let height: Int
    let left: Counts
    let right: Counts
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write((message + "\n").data(using: .utf8)!)
    exit(1)
}

guard CommandLine.arguments.count == 2 else {
    fail("usage: sampler.swift <png>")
}
guard let image = NSImage(contentsOfFile: CommandLine.arguments[1]),
      let tiff = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff)
else {
    fail("failed to load image")
}

func classify(_ x: Int, _ y: Int) -> String {
    guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else {
        return "other"
    }
    let r = Int((color.redComponent * 255).rounded())
    let g = Int((color.greenComponent * 255).rounded())
    let b = Int((color.blueComponent * 255).rounded())
    if r >= 120 && r >= g + 45 && r >= b + 45 {
        return "red"
    }
    if b >= 120 && b >= r + 45 && b >= g + 45 {
        return "blue"
    }
    return "other"
}

func sample(xStart: Int, xEnd: Int, yStart: Int, yEnd: Int) -> Counts {
    var samples = 0
    var red = 0
    var blue = 0
    var other = 0
    let columns = 12
    let rows = 12
    for xi in 0..<columns {
        let x = xStart + ((xEnd - xStart) * xi) / max(columns - 1, 1)
        for yi in 0..<rows {
            let y = yStart + ((yEnd - yStart) * yi) / max(rows - 1, 1)
            samples += 1
            switch classify(x, y) {
            case "red": red += 1
            case "blue": blue += 1
            default: other += 1
            }
        }
    }
    return Counts(samples: samples, redDominant: red, blueDominant: blue, other: other)
}

let width = bitmap.pixelsWide
let height = bitmap.pixelsHigh
let yStart = max(140, height / 8)
let yEnd = max(yStart + 1, height * 3 / 5)
let left = sample(xStart: max(5, width / 16), xEnd: max(width / 16 + 1, width / 5), yStart: yStart, yEnd: yEnd)
let right = sample(xStart: min(width - 2, width * 33 / 64), xEnd: min(width - 1, width * 43 / 64), yStart: yStart, yEnd: yEnd)
let metrics = Metrics(width: width, height: height, left: left, right: right)
let encoder = JSONEncoder()
encoder.outputFormatting = [.sortedKeys]
let data = try! encoder.encode(metrics)
print(String(data: data, encoding: .utf8)!)
'''
    )


def sample_colors(sampler: Path, screenshot: Path) -> dict[str, object]:
    result = subprocess.run(
        ["swift", str(sampler), str(screenshot)],
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


def assert_split_colors(metrics: dict[str, object]) -> None:
    width = int(metrics["width"])
    height = int(metrics["height"])
    left = metrics["left"]
    right = metrics["right"]
    require(isinstance(left, dict) and isinstance(right, dict), f"bad metrics: {metrics}")
    require(width > 0 and height > 0, f"screenshot dimensions were empty: {width}x{height}")

    left_samples = int(left["samples"])
    right_samples = int(right["samples"])
    left_red = int(left["redDominant"])
    left_blue = int(left["blueDominant"])
    right_red = int(right["redDominant"])
    right_blue = int(right["blueDominant"])

    require(left_samples > 0 and right_samples > 0, f"no samples collected: {metrics}")
    require(left_red / left_samples >= 0.70, f"left pane is not red-dominant: {metrics}")
    require(right_blue / right_samples >= 0.70, f"right pane is not blue-dominant: {metrics}")
    require(left_blue / left_samples <= 0.10, f"left pane also looks blue: {metrics}")
    require(right_red / right_samples <= 0.10, f"right pane also looks red: {metrics}")


def write_debug_artifacts(screenshot: Path, metrics: dict[str, object]) -> None:
    debug_png = Path("/tmp/termsurf-issue805-exp175-split-layout.png")
    debug_json = Path("/tmp/termsurf-issue805-exp175-split-layout.json")
    if screenshot.is_file():
        debug_png.write_bytes(screenshot.read_bytes())
    debug_json.write_text(json.dumps(metrics, sort_keys=True, indent=2) + "\n")


def create_split_window(red_command: str, blue_command: str) -> tuple[str, str, int]:
    app_literal = quote_applescript(APP)
    red_literal = quote_applescript(red_command)
    blue_literal = quote_applescript(blue_command)
    script = textwrap.dedent(
        f"""
        tell application {app_literal}
          activate
          set redCfg to new surface configuration from {{command:{red_literal}, wait after command:true}}
          set blueCfg to new surface configuration from {{command:{blue_literal}, wait after command:true}}
          new window with configuration redCfg
          delay 1
          set w to front window
          set t0 to focused terminal of selected tab of w
          if (id of t0) is "" then error "initial terminal id was empty"
          set t1 to split t0 direction right with configuration blueCfg
          delay 2
          if (id of t1) is "" then error "split terminal id was empty"
          if (count of terminals of selected tab of w) is not 2 then error "selected tab did not have exactly two terminals"
          return (id of t0) & linefeed & (id of t1) & linefeed & (count of terminals of selected tab of w)
        end tell
        """
    )
    result = run_osascript(script, timeout=30)
    parts = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    require(len(parts) == 3, f"unexpected split workflow output: {result.stdout!r}")
    require(parts[0] != parts[1], f"split reused terminal id: {parts}")
    return parts[0], parts[1], int(parts[2])


def assert_inventory_split() -> None:
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()
    cfg223 = next(
        (
            [cell.strip() for cell in line.strip().strip("|").split("|")]
            for line in config_matrix.splitlines()
            if line.startswith("| CFG-223 |")
        ),
        None,
    )

    require("| RUNTIME-011B2I" in runtime_inventory, "missing RUNTIME-011B2I row")
    require("live right-split visual layout proof" in runtime_inventory, "missing split-layout evidence")
    require("red-dominant" in runtime_inventory, "missing red sampled-region evidence")
    require("blue-dominant" in runtime_inventory, "missing blue sampled-region evidence")
    require("Experiment 185 closes the macOS walkthrough residual row" in runtime_inventory, "missing macOS residual closure evidence")
    require("macos_walkthrough_residual_parity.py" in runtime_inventory, "missing macOS residual guard evidence")
    require("89 rows Oracle complete" in config_matrix, "CFG-223 oracle count not updated")
    require("92 rows closed" in config_matrix, "CFG-223 closed count not updated")
    require("1 rows are incomplete" in config_matrix, "CFG-223 incomplete count changed")
    require("1 rows are runtime gaps" in config_matrix, "CFG-223 gap count changed")
    require(cfg223 is not None and len(cfg223) > 4 and cfg223[4] == "Gap", "CFG-223 should remain Gap")


def main() -> int:
    require(APP.is_dir(), f"app not built: {APP}")
    require(BINARY.is_file(), f"app binary not built: {BINARY}")
    require(LIST_WINDOWS.is_file(), f"window list helper missing: {LIST_WINDOWS}")

    crash_reports_before = crash_reports()

    with tempfile.TemporaryDirectory(prefix="termsurf-issue805-exp175-") as temp_dir:
        temp = Path(temp_dir)
        red_script = temp / "red_pane.py"
        blue_script = temp / "blue_pane.py"
        red_marker = temp / "red-ready.txt"
        blue_marker = temp / "blue-ready.txt"
        sampler = temp / "sample_split_colors.swift"
        screenshot = temp / "split-layout.png"
        config = temp / "config.roastty"
        write_painter(red_script, red_marker, "48;2;255;0;0", "#ff0000")
        write_painter(blue_script, blue_marker, "48;2;0;0;255", "#0000ff")
        write_sampler(sampler)
        config.write_text(
            "\n".join(
                [
                    "macos-applescript = true",
                    "quit-after-last-window-closed = true",
                    "cursor-style-blink = false",
                    "font-size = 16",
                    "window-width = 120",
                    "window-height = 34",
                    "background-opacity = 1",
                    "",
                ]
            )
        )

        pid = launch_app(config)
        try:
            wait_for_app(pid)
            red_command = f"python3 {shlex.quote(str(red_script))}"
            blue_command = f"python3 {shlex.quote(str(blue_script))}"
            left_id, right_id, terminal_count = create_split_window(red_command, blue_command)
            require(terminal_count == 2, f"unexpected terminal count after split: {terminal_count}")
            wait_for_file(red_marker, "red painter")
            wait_for_file(blue_marker, "blue painter")

            deadline = time.monotonic() + 10
            observed: WindowInfo | None = None
            while time.monotonic() < deadline:
                observed = primary_window(pid)
                if observed.bounds.width >= 600 and observed.bounds.height >= 250:
                    break
                time.sleep(0.25)
            require(observed is not None, "primary window did not appear")

            time.sleep(2)
            width, height = capture_window_id(observed.id, screenshot)
            require(width > 0 and height > 0, f"split screenshot dimensions were empty: {width}x{height}")
            metrics = sample_colors(sampler, screenshot)
            write_debug_artifacts(screenshot, metrics)
            assert_split_colors(metrics)
        finally:
            terminate_process(pid)

    new_crash_reports = wait_for_crash_report_settle(crash_reports_before)
    require(
        not new_crash_reports,
        "Roastty wrote crash reports during split layout workflow: "
        + ", ".join(str(path) for path in sorted(new_crash_reports)),
    )

    assert_inventory_split()
    print(f"macos_split_layout_runtime=pass left_terminal={left_id} right_terminal={right_id}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
