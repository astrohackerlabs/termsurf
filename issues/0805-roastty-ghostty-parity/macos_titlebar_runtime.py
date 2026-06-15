#!/usr/bin/env python3
"""Live macOS titlebar GUI guard for Issue 805 CFG-223."""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
import re
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


def launch_app(config: Path, suite_suffix: str) -> int:
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
            f"ROASTTY_USER_DEFAULTS_SUITE=com.termsurf.roastty.issue805.exp176.{suite_suffix}",
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


def primary_window(pid: int, focused_bounds: Rect) -> WindowInfo:
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


def write_sampler(path: Path) -> None:
    path.write_text(
        r'''
import AppKit
import Foundation

struct Metrics: Codable {
    let width: Int
    let height: Int
    let samples: Int
    let red: Int
    let yellow: Int
    let green: Int
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

func classify(_ x: Int, _ y: Int) -> String? {
    guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else {
        return nil
    }
    let r = Int((color.redComponent * 255).rounded())
    let g = Int((color.greenComponent * 255).rounded())
    let b = Int((color.blueComponent * 255).rounded())
    if r >= 190 && g <= 120 && b <= 120 {
        return "red"
    }
    if r >= 190 && g >= 145 && b <= 110 {
        return "yellow"
    }
    if g >= 145 && r <= 120 && b <= 140 {
        return "green"
    }
    return nil
}

let width = bitmap.pixelsWide
let height = bitmap.pixelsHigh
let xMax = min(width - 1, 180)
let yMax = min(height - 1, 90)
var samples = 0
var red = 0
var yellow = 0
var green = 0
for x in stride(from: 0, through: xMax, by: 2) {
    for y in stride(from: 0, through: yMax, by: 2) {
        samples += 1
        switch classify(x, y) {
        case "red": red += 1
        case "yellow": yellow += 1
        case "green": green += 1
        default: break
        }
    }
}
let metrics = Metrics(width: width, height: height, samples: samples, red: red, yellow: yellow, green: green)
let encoder = JSONEncoder()
encoder.outputFormatting = [.sortedKeys]
let data = try! encoder.encode(metrics)
print(String(data: data, encoding: .utf8)!)
'''
    )


def sample_titlebar(sampler: Path, screenshot: Path) -> dict[str, int]:
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


def write_config(path: Path, style: str) -> None:
    path.write_text(
        "\n".join(
            [
                "macos-applescript = true",
                "quit-after-last-window-closed = true",
                "cursor-style-blink = false",
                "font-size = 16",
                "window-width = 100",
                "window-height = 30",
                "background = #20242c",
                "background-opacity = 1",
                f"macos-titlebar-style = {style}",
                "",
            ]
        )
    )


def run_case(style: str, temp: Path, sampler: Path) -> dict[str, object]:
    config = temp / f"{style}.roastty"
    screenshot = temp / f"{style}.png"
    write_config(config, style)
    pid = launch_app(config, style)
    terminal_id = ""
    try:
        wait_for_app(pid)
        terminal_id = create_terminal_window("/bin/sh -c 'sleep 60'")
        evidence = focus_evidence(pid)
        focused_bounds = evidence.pop("focused_bounds")
        require(isinstance(focused_bounds, Rect), f"unexpected focused bounds: {focused_bounds!r}")
        window = primary_window(pid, focused_bounds)
        width, height = capture_window_id(window.id, screenshot)
        metrics = sample_titlebar(sampler, screenshot)
        require(width > 0 and height > 0, f"{style} screenshot dimensions were empty: {width}x{height}")
        require(metrics["width"] == width and metrics["height"] == height, f"{style} sampler dimensions mismatch")
        debug_png = Path(f"/tmp/termsurf-issue805-exp176-{style}-titlebar.png")
        debug_json = Path(f"/tmp/termsurf-issue805-exp176-{style}-titlebar.json")
        debug_png.write_bytes(screenshot.read_bytes())
        debug_json.write_text(
            json.dumps(
                {
                    "style": style,
                    "pid": pid,
                    "terminal_id": terminal_id,
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
                    "metrics": metrics,
                },
                sort_keys=True,
                indent=2,
            )
            + "\n"
        )
        return {"style": style, "pid": pid, "terminal_id": terminal_id, "window_id": window.id, "metrics": metrics}
    finally:
        terminate_process(pid)


def assert_titlebar_metrics(control: dict[str, object], hidden: dict[str, object]) -> None:
    control_metrics = control["metrics"]
    hidden_metrics = hidden["metrics"]
    require(isinstance(control_metrics, dict) and isinstance(hidden_metrics, dict), "bad titlebar metrics")
    for color in ["red", "yellow", "green"]:
        require(
            int(control_metrics[color]) >= 20,
            f"transparent control did not expose enough {color} traffic-light pixels: {control_metrics}",
        )
        require(
            int(hidden_metrics[color]) <= 3,
            f"hidden titlebar still exposes {color} traffic-light pixels: {hidden_metrics}",
        )


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

    require("| RUNTIME-011B2J" in runtime_inventory, "missing RUNTIME-011B2J row")
    require("live hidden-titlebar visual proof" in runtime_inventory, "missing hidden-titlebar evidence")
    require("red/yellow/green traffic-light" in runtime_inventory, "missing traffic-light evidence")
    require("frontmost process Unix PID" in runtime_inventory, "missing frontmost PID evidence")
    require("broader titlebar behavior" in runtime_inventory, "remaining macOS GUI gap omitted titlebar evidence")
    require("80 rows Oracle complete" in config_matrix, "CFG-223 oracle count not updated")
    require("83 rows closed" in config_matrix, "CFG-223 closed count not updated")
    require("4 rows are incomplete" in config_matrix, "CFG-223 incomplete count changed")
    require("4 rows are runtime gaps" in config_matrix, "CFG-223 gap count changed")
    require(cfg223 is not None and len(cfg223) > 4 and cfg223[4] == "Gap", "CFG-223 should remain Gap")


def main() -> int:
    require(APP.is_dir(), f"app not built: {APP}")
    require(BINARY.is_file(), f"app binary not built: {BINARY}")
    require(LIST_WINDOWS.is_file(), f"window list helper missing: {LIST_WINDOWS}")

    crash_reports_before = crash_reports()
    with tempfile.TemporaryDirectory(prefix="termsurf-issue805-exp176-") as temp_dir:
        temp = Path(temp_dir)
        sampler = temp / "sample_titlebar.swift"
        write_sampler(sampler)
        control = run_case("transparent", temp, sampler)
        hidden = run_case("hidden", temp, sampler)
        assert_titlebar_metrics(control, hidden)

    new_crash_reports = wait_for_crash_report_settle(crash_reports_before)
    require(
        not new_crash_reports,
        "Roastty wrote crash reports during titlebar workflow: "
        + ", ".join(str(path) for path in sorted(new_crash_reports)),
    )

    assert_inventory_split()
    print(
        "macos_titlebar_runtime=pass "
        f"transparent_window={control['window_id']} hidden_window={hidden['window_id']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
