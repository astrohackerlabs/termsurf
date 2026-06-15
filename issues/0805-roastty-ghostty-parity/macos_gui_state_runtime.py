#!/usr/bin/env python3
"""Live macOS GUI state guard for Issue 805 CFG-223."""

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
SCREENSHOT = ROOT / "scripts/roastty-app/screenshot.sh"
LIST_WINDOWS = ROOT / "scripts/roastty-app/list-windows.swift"
PNGDIFF = ROOT / "scripts/roastty-app/pngdiff.swift"
DIAGNOSTIC_REPORTS = Path.home() / "Library/Logs/DiagnosticReports"


@dataclass(frozen=True)
class WindowInfo:
    id: int
    layer: int
    x: int
    y: int
    width: int
    height: int
    name: str

    @property
    def area(self) -> int:
        return self.width * self.height


@dataclass(frozen=True)
class ScreenshotInfo:
    path: Path
    window_id: int
    width: int
    height: int


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
            "ROASTTY_USER_DEFAULTS_SUITE=com.termsurf.roastty.issue805.exp173",
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
                x=int(match.group("x")),
                y=int(match.group("y")),
                width=int(match.group("w")),
                height=int(match.group("h")),
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
    candidates = [w for w in windows_for_pid(pid) if w.layer == 0 and w.width >= 100 and w.height >= 100]
    require(candidates, f"no layer-0 primary window found for pid {pid}")
    # CGWindowListCopyWindowInfo preserves front-to-back order. Match
    # scripts/roastty-app/winid.swift so bounds and screenshots use the same
    # primary window identity.
    return candidates[0]


def window_by_id(pid: int, window_id: int) -> WindowInfo:
    for window in windows_for_pid(pid):
        if window.id == window_id:
            return window
    raise AssertionError(f"window id {window_id} not found for pid {pid}")


def wait_for_window(pid: int, predicate, description: str, timeout: float = 15.0) -> WindowInfo:
    deadline = time.monotonic() + timeout
    last: WindowInfo | None = None
    while time.monotonic() < deadline:
        last = primary_window(pid)
        if predicate(last):
            return last
        time.sleep(0.25)
    raise AssertionError(f"{description} did not satisfy predicate; last={last}")


def ax_fullscreen(pid: int) -> bool | None:
    try:
        result = system_events(
            """
            tell roasttyProc
              try
                return value of attribute "AXFullScreen" of window 1
              on error
                return "missing"
              end try
            end tell
            """,
            pid,
            timeout=10,
        )
    except AssertionError:
        return None
    text = result.stdout.strip().lower()
    if text == "true":
        return True
    if text == "false":
        return False
    return None


def wait_for_ax_fullscreen(pid: int, expected: bool, timeout: float = 15.0) -> None:
    deadline = time.monotonic() + timeout
    last: bool | None = None
    while time.monotonic() < deadline:
        last = ax_fullscreen(pid)
        if last is None:
            return
        if last == expected:
            return
        time.sleep(0.25)
    raise AssertionError(f"AXFullScreen should become {expected}, got {last}")


def capture_screenshot(pid: int, shot_dir: Path, label: str, expected_id: int | None = None) -> ScreenshotInfo:
    env = os.environ.copy()
    env["TERMSURF_SHOT_DIR"] = str(shot_dir)
    result = subprocess.run(
        [str(SCREENSHOT), str(pid), label],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
        timeout=30,
    )
    if result.returncode != 0:
        raise AssertionError(
            "screenshot failed\n"
            f"stdout:\n{result.stdout}\n"
            f"stderr:\n{result.stderr}"
        )
    path_text = result.stdout.strip().splitlines()[-1]
    path = Path(path_text)
    require(path.is_file(), f"screenshot path missing: {path}")
    match = re.search(r"id=(?P<id>\d+) .* captured=(?P<w>\d+)x(?P<h>\d+)px", result.stderr)
    require(match is not None, f"could not parse screenshot metadata: {result.stderr!r}")
    window_id = int(match.group("id"))
    if expected_id is not None:
        require(window_id == expected_id, f"screenshot window id changed: expected {expected_id}, got {window_id}")
    return ScreenshotInfo(
        path=path,
        window_id=window_id,
        width=int(match.group("w")),
        height=int(match.group("h")),
    )


def png_metrics(left: Path, right: Path) -> dict[str, float | int | str]:
    result = subprocess.run(
        ["swift", str(PNGDIFF), str(left), str(right)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=60,
    )
    require(result.stdout.strip(), f"pngdiff produced no output: stderr={result.stderr}")
    return json.loads(result.stdout)


def assert_diff_at_least(metrics: dict[str, float | int | str], mismatch: float, mean: float) -> None:
    require(metrics.get("error") != "dimension_mismatch", f"png dimensions changed: {metrics}")
    require(
        float(metrics["mismatch_ratio"]) >= mismatch,
        f"mismatch ratio too small: {metrics}",
    )
    require(
        float(metrics["mean_channel_delta"]) >= mean,
        f"mean channel delta too small: {metrics}",
    )


def assert_diff_at_most(metrics: dict[str, float | int | str], mismatch: float, mean: float) -> None:
    require(metrics.get("error") != "dimension_mismatch", f"png dimensions changed: {metrics}")
    require(
        float(metrics["mismatch_ratio"]) <= mismatch,
        f"mismatch ratio too large: {metrics}",
    )
    require(
        float(metrics["mean_channel_delta"]) <= mean,
        f"mean channel delta too large: {metrics}",
    )


def command_palette_accessibility_text(pid: int) -> tuple[str, str | None]:
    try:
        result = system_events(
            """
            tell roasttyProc
              set collected to {}
              try
                set allElements to entire contents of window 1
                repeat with uiElement in allElements
                  try
                    set uiName to name of uiElement
                    if uiName is not missing value and uiName is not "" then set end of collected to uiName as text
                  end try
                  try
                    set uiValue to value of uiElement
                    if uiValue is not missing value and uiValue is not "" then set end of collected to uiValue as text
                  end try
                end repeat
              end try
              set AppleScript's text item delimiters to linefeed
              return collected as text
            end tell
            """,
            pid,
            timeout=20,
        )
    except subprocess.TimeoutExpired:
        return "", "timeout"
    except AssertionError:
        return "", "query-failed"
    text = result.stdout
    if not text.strip():
        return text, "empty-tree"
    return text, None


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

    require("| RUNTIME-011B2G" in runtime_inventory, "missing RUNTIME-011B2G row")
    require(
        "live fullscreen and command-palette GUI state proof" in runtime_inventory,
        "missing macOS GUI state evidence",
    )
    require(
        "PID-scoped CoreGraphics layer-0 window bounds" in runtime_inventory,
        "missing fullscreen geometry evidence text",
    )
    require(
        "baseline-to-palette screenshot delta" in runtime_inventory,
        "missing command palette screenshot evidence text",
    )
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
    require(SCREENSHOT.is_file(), f"screenshot helper missing: {SCREENSHOT}")
    require(LIST_WINDOWS.is_file(), f"window list helper missing: {LIST_WINDOWS}")
    require(PNGDIFF.is_file(), f"pngdiff helper missing: {PNGDIFF}")

    crash_reports_before = crash_reports()

    with tempfile.TemporaryDirectory(prefix="termsurf-issue805-exp173-") as temp_dir:
        temp = Path(temp_dir)
        shot_dir = temp / "shots"
        shot_dir.mkdir()
        config = temp / "config.roastty"
        config.write_text(
            "\n".join(
                [
                    "macos-applescript = true",
                    "quit-after-last-window-closed = true",
                    "window-width = 90",
                    "window-height = 28",
                    "cursor-style-blink = false",
                    "fullscreen = true",
                    "command-palette-entry = clear",
                    'command-palette-entry = title:"Issue805 Palette Marker",description:"Experiment 173",action:"text:ISSUE805_EXP173"',
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

            baseline = wait_for_window(pid, lambda w: w.area > 0, "baseline window")
            baseline_ax = ax_fullscreen(pid)
            if baseline_ax is not None:
                require(not baseline_ax, f"baseline AXFullScreen should be false, got {baseline_ax}")

            click_menu_item(pid, "Window", "Toggle Full Screen")
            fullscreen = wait_for_window(
                pid,
                lambda w: w.area >= baseline.area * 1.40
                and (w.width >= baseline.width + 200 or w.height >= baseline.height + 150),
                "fullscreen entry",
            )
            fullscreen_ax = ax_fullscreen(pid)
            if fullscreen_ax is not None:
                wait_for_ax_fullscreen(pid, True)
            fullscreen_shot = capture_screenshot(pid, shot_dir, "issue805-exp173-fullscreen")
            fullscreen_shot_window = window_by_id(pid, fullscreen_shot.window_id)
            require(fullscreen_shot_window.layer == 0, f"fullscreen screenshot window is not layer 0: {fullscreen_shot_window}")
            require(
                fullscreen_shot_window.area >= baseline.area * 1.40
                and (
                    fullscreen_shot_window.width >= baseline.width + 200
                    or fullscreen_shot_window.height >= baseline.height + 150
                ),
                f"fullscreen screenshot window did not satisfy geometry predicate: {fullscreen_shot_window}",
            )

            click_menu_item(pid, "Window", "Toggle Full Screen")
            restored = wait_for_window(
                pid,
                lambda w: abs(w.width - baseline.width) <= 80
                and abs(w.height - baseline.height) <= 80
                and abs(w.area - baseline.area) <= baseline.area * 0.20,
                "fullscreen exit",
            )
            restored_ax = ax_fullscreen(pid)
            if restored_ax is not None:
                wait_for_ax_fullscreen(pid, False)

            dismiss_menus(pid)
            palette_baseline = capture_screenshot(pid, shot_dir, "issue805-exp173-palette-baseline")
            palette_baseline_window = window_by_id(pid, palette_baseline.window_id)
            require(palette_baseline_window.layer == 0, f"palette baseline window is not layer 0: {palette_baseline_window}")
            require(
                abs(palette_baseline_window.width - baseline.width) <= 80
                and abs(palette_baseline_window.height - baseline.height) <= 80
                and abs(palette_baseline_window.area - baseline.area) <= baseline.area * 0.20,
                f"palette baseline did not return near original geometry: {palette_baseline_window}",
            )
            click_menu_item(pid, "View", "Command Palette")
            time.sleep(0.75)
            palette_text, palette_accessibility_fallback = command_palette_accessibility_text(pid)
            has_accessibility_evidence = any(
                cue in palette_text for cue in ["Search", "Focus:", "Issue805 Palette Marker"]
            )
            if not has_accessibility_evidence:
                reason = palette_accessibility_fallback or "missing-expected-cue"
                print(f"palette_accessibility=fallback:{reason}")
            palette_shot = capture_screenshot(
                pid,
                shot_dir,
                "issue805-exp173-palette-visible",
                palette_baseline.window_id,
            )
            require(
                (palette_shot.width, palette_shot.height) == (palette_baseline.width, palette_baseline.height),
                "palette screenshot dimensions changed",
            )
            visible_metrics = png_metrics(palette_baseline.path, palette_shot.path)
            assert_diff_at_least(visible_metrics, mismatch=0.02, mean=1.0)

            system_events("key code 53", pid, timeout=10)
            time.sleep(0.75)
            dismiss_menus(pid)
            dismissed_shot = capture_screenshot(
                pid,
                shot_dir,
                "issue805-exp173-palette-dismissed",
                palette_baseline.window_id,
            )
            dismissed_metrics = png_metrics(palette_baseline.path, dismissed_shot.path)
            assert_diff_at_most(dismissed_metrics, mismatch=0.01, mean=2.0)
        finally:
            terminate_process(pid)

    new_crash_reports = wait_for_crash_report_settle(crash_reports_before)
    require(
        not new_crash_reports,
        "Roastty wrote crash reports during GUI state workflow: "
        + ", ".join(str(path) for path in sorted(new_crash_reports)),
    )

    assert_inventory_split()
    print("macos_gui_state_runtime=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
