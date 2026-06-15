#!/usr/bin/env python3
"""Live macOS Quick Terminal GUI guard for Issue 805 CFG-223."""

from __future__ import annotations

from dataclasses import dataclass
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
            "ROASTTY_USER_DEFAULTS_SUITE=com.termsurf.roastty.issue805.exp174",
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


def system_events(script_body: str, pid: int, timeout: int = 30) -> subprocess.CompletedProcess[str]:
    script = textwrap.dedent(
        f"""
        tell application "System Events"
          set roasttyProc to first application process whose unix id is {pid}
          set frontmost of roasttyProc to true
          delay 0.25
          set frontPID to unix id of first application process whose frontmost is true
          if frontPID is not {pid} then error "frontmost PID mismatch: " & frontPID
          {script_body}
        end tell
        """
    )
    return run_osascript(script, timeout=timeout)


def click_menu_item(pid: int, menu_name: str, item_name: str) -> None:
    system_events(
        f"""
        tell roasttyProc
          click menu bar item {quote_applescript(menu_name)} of menu bar 1
          delay 0.25
          click menu item {quote_applescript(item_name)} of menu 1 of menu bar item {quote_applescript(menu_name)} of menu bar 1
        end tell
        """,
        pid,
    )


def dismiss_menus(pid: int) -> None:
    system_events("key code 53", pid, timeout=10)


def ensure_terminal_window(command: str) -> None:
    app_literal = quote_applescript(APP)
    command_literal = quote_applescript(command)
    script = textwrap.dedent(
        f"""
        tell application {app_literal}
          activate
          if (count of windows) > 0 then
            set w to front window
            if (id of focused terminal of selected tab of w) is not "" then return
          end if
          set cfg to new surface configuration from {{command:{command_literal}, wait after command:true}}
          new window with configuration cfg
          delay 1
          if (count of windows) < 1 then error "new window was not created"
          set w to front window
          if (id of focused terminal of selected tab of w) is "" then error "focused terminal id was empty"
        end tell
        """
    )
    run_osascript(script, timeout=30)


def screen_rect() -> Rect:
    swift = (
        "import AppKit; "
        "let f = NSScreen.main!.visibleFrame; "
        'print("\\(Int(f.origin.x)) \\(Int(f.origin.y)) \\(Int(f.width)) \\(Int(f.height))")'
    )
    result = subprocess.run(
        ["swift", "-e", swift],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=20,
    )
    if result.returncode != 0:
        raise AssertionError(f"screen swift failed\nstdout={result.stdout}\nstderr={result.stderr}")
    parts = [int(part) for part in result.stdout.split()]
    require(len(parts) == 4, f"unexpected screen rect output: {result.stdout!r}")
    return Rect(parts[0], parts[1], parts[2], parts[3])


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


def window_ids(windows: list[WindowInfo]) -> set[int]:
    return {window.id for window in windows}


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


def find_quick_terminal(pid: int, before_ids: set[int], screen: Rect, timeout: float = 15.0) -> WindowInfo:
    deadline = time.monotonic() + timeout
    last: list[WindowInfo] = []
    while time.monotonic() < deadline:
        current = windows_for_pid(pid)
        last = current
        new_windows = [window for window in current if window.id not in before_ids and window.bounds.area > 0]
        for window in new_windows:
            if (
                window.layer != 0
                and window.bounds.width >= screen.width * 0.70
                and screen.height * 0.25 <= window.bounds.height <= screen.height * 0.55
            ):
                if window.bounds.y <= 120:
                    return window
        time.sleep(0.25)
    raise AssertionError(f"Quick Terminal window did not appear; last={last}")


def wait_for_window_gone(pid: int, window_id: int, timeout: float = 15.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if window_id not in window_ids(windows_for_pid(pid)):
            return
        time.sleep(0.25)
    raise AssertionError(f"Quick Terminal window {window_id} did not disappear")


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

    require("| RUNTIME-011B2H" in runtime_inventory, "missing RUNTIME-011B2H row")
    require("live Quick Terminal GUI visibility and geometry proof" in runtime_inventory, "missing Quick Terminal evidence")
    require("nonzero CoreGraphics layer" in runtime_inventory, "missing floating panel layer evidence")
    require("exact Quick Terminal CGWindowID" in runtime_inventory, "missing exact screenshot evidence")
    require("titlebar visuals" in runtime_inventory, "remaining macOS GUI gap omitted titlebar evidence")
    require("split visual/layout parity" in runtime_inventory, "remaining macOS GUI gap omitted split evidence")
    require("73 rows Oracle complete" in config_matrix, "CFG-223 oracle count not updated")
    require("76 rows closed" in config_matrix, "CFG-223 closed count not updated")
    require("4 rows are incomplete" in config_matrix, "CFG-223 incomplete count changed")
    require("4 rows are runtime gaps" in config_matrix, "CFG-223 gap count changed")
    require(cfg223 is not None and len(cfg223) > 4 and cfg223[4] == "Gap", "CFG-223 should remain Gap")


def main() -> int:
    require(APP.is_dir(), f"app not built: {APP}")
    require(BINARY.is_file(), f"app binary not built: {BINARY}")
    require(LIST_WINDOWS.is_file(), f"window list helper missing: {LIST_WINDOWS}")

    crash_reports_before = crash_reports()

    with tempfile.TemporaryDirectory(prefix="termsurf-issue805-exp174-") as temp_dir:
        temp = Path(temp_dir)
        shot = temp / "quick-terminal.png"
        config = temp / "config.roastty"
        config.write_text(
            "\n".join(
                [
                    "macos-applescript = true",
                    "quit-after-last-window-closed = true",
                    "quick-terminal-animation-duration = 0",
                    "quick-terminal-position = top",
                    "quick-terminal-size = 40%",
                    "",
                ]
            )
        )
        command = "/bin/sh -c 'sleep 60'"
        pid = launch_app(config)

        try:
            wait_for_app(pid)
            ensure_terminal_window(command)
            dismiss_menus(pid)
            screen = screen_rect()
            before = windows_for_pid(pid)
            before_ids = window_ids(before)
            require(before_ids, "expected at least one PID-owned window before Quick Terminal")

            click_menu_item(pid, "View", "Quick Terminal")
            quick = find_quick_terminal(pid, before_ids, screen)
            width, height = capture_window_id(quick.id, shot)
            require(width > 0 and height > 0, f"Quick Terminal screenshot dimensions were empty: {width}x{height}")

            click_menu_item(pid, "View", "Quick Terminal")
            wait_for_window_gone(pid, quick.id)
        finally:
            terminate_process(pid)

    new_crash_reports = wait_for_crash_report_settle(crash_reports_before)
    require(
        not new_crash_reports,
        "Roastty wrote crash reports during Quick Terminal workflow: "
        + ", ".join(str(path) for path in sorted(new_crash_reports)),
    )

    assert_inventory_split()
    print("macos_quick_terminal_runtime=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
