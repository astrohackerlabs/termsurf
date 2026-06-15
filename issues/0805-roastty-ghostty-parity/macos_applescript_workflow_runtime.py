#!/usr/bin/env python3
"""Live macOS AppleScript workflow guard for Issue 805 CFG-223."""

from __future__ import annotations

from collections.abc import Callable
import os
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
MARKER = "ISSUE805_EXP167_INPUT_MARKER"
DIAGNOSTIC_REPORTS = Path.home() / "Library/Logs/DiagnosticReports"


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


def wait_for_file(
    path: Path,
    description: str,
    predicate: Callable[[bytes], bool] | None = None,
    timeout: float = 10.0,
) -> bytes:
    deadline = time.monotonic() + timeout
    observed = b"<missing>"
    while time.monotonic() < deadline:
        if path.exists():
            observed = path.read_bytes()
            if predicate is None:
                return observed
            if predicate(observed):
                return observed
        time.sleep(0.25)
    raise AssertionError(f"{description} was not recorded: {observed!r}")


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
            "ROASTTY_USER_DEFAULTS_SUITE=com.termsurf.roastty.issue805.exp167",
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
            result = run_osascript(
                f'tell application {app_literal} to count of windows',
                timeout=5,
            )
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
            run_osascript(f'tell application {quote_applescript(APP)} to quit', timeout=5)
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

    require("| RUNTIME-011B2A" in runtime_inventory, "missing RUNTIME-011B2A row")
    require("| RUNTIME-011B2B" in runtime_inventory, "missing RUNTIME-011B2B row")
    require("| RUNTIME-011B2C" in runtime_inventory, "missing RUNTIME-011B2C row")
    require("| RUNTIME-011B2D" in runtime_inventory, "missing RUNTIME-011B2D row")
    require("| RUNTIME-011B2E" in runtime_inventory, "missing RUNTIME-011B2E row")
    require(
        "live AppleScript-driven Roastty app workflow automation" in runtime_inventory,
        "missing AppleScript workflow evidence",
    )
    require(
        "live AppleScript split-terminal object lifecycle" in runtime_inventory,
        "missing split terminal lifecycle evidence",
    )
    require(
        "re-resolves that ID at application, window, and selected-tab scope"
        in runtime_inventory,
        "missing split terminal re-resolution evidence",
    )
    require(
        "selected tab's focused terminal ID changed" in runtime_inventory,
        "missing split terminal focus evidence",
    )
    require(
        "closed terminal ID no longer resolves" in runtime_inventory,
        "missing split terminal close evidence",
    )
    require(
        "controlled keyboard child process records exact raw bytes from `send key`"
        in runtime_inventory,
        "missing send key side-effect evidence",
    )
    require(
        "controlled mouse child process records new terminal mouse-report bytes after each scripted"
        in runtime_inventory,
        "missing scripted mouse side-effect evidence",
    )
    require(
        "controlled child process records the `input text` marker" in runtime_inventory,
        "missing input side-effect evidence",
    )
    require(
        "live native menu visibility, validation, and representative dispatch"
        in runtime_inventory,
        "missing native menu runtime evidence",
    )
    require(
        "broader titlebar behavior" in runtime_inventory,
        "remaining macOS GUI gap omitted visual evidence",
    )
    require(
        "broader split variants and interactions" in runtime_inventory,
        "remaining macOS GUI gap omitted split visual evidence",
    )
    require(
        "fails if a new Roastty crash report appears" in runtime_inventory,
        "missing new crash-report guard evidence",
    )
    require("75 rows Oracle complete" in config_matrix, "CFG-223 oracle count not updated")
    require("78 rows closed" in config_matrix, "CFG-223 closed count not updated")
    require("4 rows are incomplete" in config_matrix, "CFG-223 incomplete count changed")
    require("4 rows are runtime gaps" in config_matrix, "CFG-223 gap count changed")
    require(cfg223 is not None and len(cfg223) > 4 and cfg223[4] == "Gap", "CFG-223 should remain Gap")


def main() -> int:
    require(APP.is_dir(), f"app not built: {APP}")
    require(BINARY.is_file(), f"app binary not built: {BINARY}")

    crash_reports_before = crash_reports()

    with tempfile.TemporaryDirectory(prefix="termsurf-issue805-exp167-") as temp_dir:
        temp = Path(temp_dir)
        config = temp / "config.roastty"
        marker_file = temp / "input-marker.txt"
        split_marker_file = temp / "split-marker.txt"
        split_input_marker_file = temp / "split-input-marker.txt"
        key_capture_script = temp / "key_capture.py"
        key_ready_file = temp / "key-ready.txt"
        key_output_file = temp / "key-output.bin"
        mouse_capture_script = temp / "mouse_capture.py"
        mouse_ready_file = temp / "mouse-ready.txt"
        mouse_output_file = temp / "mouse-output.bin"
        config.write_text("macos-applescript = true\nquit-after-last-window-closed = true\n")

        key_capture_script.write_text(
            textwrap.dedent(
                f"""
                import os
                import sys
                import termios
                import time
                import tty

                ready = {str(key_ready_file)!r}
                output = {str(key_output_file)!r}
                fd = sys.stdin.fileno()
                old = termios.tcgetattr(fd)
                try:
                    tty.setraw(fd)
                    with open(ready, "w", encoding="utf-8") as handle:
                        handle.write("ready")
                    data = bytearray()
                    while len(data) < 2:
                        chunk = os.read(fd, 2 - len(data))
                        if chunk:
                            data.extend(chunk)
                    with open(output, "wb") as handle:
                        handle.write(data)
                    time.sleep(30)
                finally:
                    termios.tcsetattr(fd, termios.TCSADRAIN, old)
                """
            ).lstrip()
        )

        mouse_capture_script.write_text(
            textwrap.dedent(
                f"""
                import os
                import select
                import sys
                import termios
                import time
                import tty

                ready = {str(mouse_ready_file)!r}
                output = {str(mouse_output_file)!r}
                fd = sys.stdin.fileno()
                old = termios.tcgetattr(fd)
                try:
                    tty.setraw(fd)
                    sys.stdout.write("\\x1b[?1000h\\x1b[?1002h\\x1b[?1003h\\x1b[?1006h")
                    sys.stdout.flush()
                    with open(ready, "w", encoding="utf-8") as handle:
                        handle.write("ready")
                    data = bytearray()
                    deadline = time.time() + 8
                    while time.time() < deadline and len(data) < 64:
                        readable, _, _ = select.select([fd], [], [], 0.25)
                        if readable:
                            data.extend(os.read(fd, 64))
                            with open(output, "wb") as handle:
                                handle.write(data)
                    time.sleep(30)
                finally:
                    sys.stdout.write("\\x1b[?1000l\\x1b[?1002l\\x1b[?1003l\\x1b[?1006l")
                    sys.stdout.flush()
                    termios.tcsetattr(fd, termios.TCSADRAIN, old)
                """
            ).lstrip()
        )

        command = (
            "/bin/sh -c 'IFS= read -r line; "
            f"printf %s\\\\n \"$line\" > {marker_file}; "
            "sleep 30'"
        )
        split_command = (
            "/bin/sh -c 'printf split-ok > "
            f"{split_marker_file}; "
            "IFS= read -r line; "
            f"printf %s\\\\n \"$line\" > {split_input_marker_file}; "
            "sleep 30'"
        )
        key_command = f"python3 {shlex.quote(str(key_capture_script))}"
        mouse_command = f"python3 {shlex.quote(str(mouse_capture_script))}"
        pid = launch_app(config)

        try:
            wait_for_app(pid)
            app_literal = quote_applescript(APP)
            command_literal = quote_applescript(command)
            split_command_literal = quote_applescript(split_command)
            key_command_literal = quote_applescript(key_command)
            mouse_command_literal = quote_applescript(mouse_command)
            marker_literal = quote_applescript(MARKER)
            split_marker_literal = quote_applescript("ISSUE805_EXP170_SPLIT_INPUT_MARKER")

            workflow = textwrap.dedent(
                f"""
                tell application {app_literal}
                  activate
                  set originalWindowCount to count of windows
                  set cfg to new surface configuration from {{command:{command_literal}, wait after command:true}}
                  set splitCfg to new surface configuration from {{command:{split_command_literal}, wait after command:true}}
                  set keyCfg to new surface configuration from {{command:{key_command_literal}, wait after command:true}}
                  set mouseCfg to new surface configuration from {{command:{mouse_command_literal}, wait after command:true}}
                  new window with configuration cfg
                  delay 1
                  if (count of windows) < originalWindowCount + 1 then error "new window was not created"
                  set w to front window
                  set t0 to focused terminal of selected tab of w
                  if (id of t0) is "" then error "initial terminal id was empty"
                  input text ({marker_literal} & linefeed) to t0
                  set tab2 to new tab in w
                  delay 1
                  if (count of tabs of w) < 2 then error "new tab was not created"
                  select tab tab2
                  if selected of tab2 is not true then error "new tab did not select"
                  close tab tab2
                  delay 1
                  set splitTerminal to split t0 direction right with configuration splitCfg
                  delay 1
                  set splitID to id of splitTerminal
                  if splitID is "" then error "split terminal id was empty"
                  set appResolved to terminal id splitID
                  if (id of appResolved) is not splitID then error "app terminal id lookup returned wrong terminal"
                  set windowResolved to terminal id splitID of w
                  if (id of windowResolved) is not splitID then error "window terminal id lookup returned wrong terminal"
                  set tabResolved to terminal id splitID of selected tab of w
                  if (id of tabResolved) is not splitID then error "tab terminal id lookup returned wrong terminal"
                  input text ({split_marker_literal} & linefeed) to tabResolved
                  focus tabResolved
                  delay 1
                  if (id of focused terminal of selected tab of w) is not splitID then error "split terminal did not focus"
                  set terminalCountBeforeClose to count of terminals of selected tab of w
                  close tabResolved
                  delay 1
                  if (count of terminals of selected tab of w) is not terminalCountBeforeClose - 1 then error "split terminal close did not reduce terminal count"
                  try
                    set closedTerminalID to id of terminal id splitID
                    error "closed split terminal id still resolved: " & closedTerminalID
                  on error errText number errNum
                    if errText starts with "closed split terminal id still resolved" then error errText number errNum
                  end try
                  set keyWindow to new window with configuration keyCfg
                  delay 1
                  set keyTerminal to focused terminal of selected tab of keyWindow
                  if (id of keyTerminal) is "" then error "key terminal id was empty"
                  set mouseWindow to new window with configuration mouseCfg
                  delay 1
                  set mouseTerminal to focused terminal of selected tab of mouseWindow
                  if (id of mouseTerminal) is "" then error "mouse terminal id was empty"
                  return (id of keyTerminal) & linefeed & (id of mouseTerminal)
                end tell
                """
            )
            result = run_osascript(workflow, timeout=45)
            terminal_ids = [line.strip() for line in result.stdout.splitlines() if line.strip()]
            require(len(terminal_ids) == 2, f"expected key and mouse terminal ids: {result.stdout!r}")
            key_terminal_id, mouse_terminal_id = terminal_ids

            wait_for_file(
                marker_file,
                "input text marker",
                lambda data: data.decode("utf-8").strip() == MARKER,
            )

            wait_for_file(
                split_marker_file,
                "split terminal command marker",
                lambda data: data.decode("utf-8").strip() == "split-ok",
            )

            expected_split_input = "ISSUE805_EXP170_SPLIT_INPUT_MARKER"
            wait_for_file(
                split_input_marker_file,
                "split input marker",
                lambda data: data.decode("utf-8").strip() == expected_split_input,
            )

            wait_for_file(key_ready_file, "keyboard capture readiness marker")
            key_input = textwrap.dedent(
                f"""
                tell application {app_literal}
                  set keyTerminal to terminal id {quote_applescript(key_terminal_id)}
                  focus keyTerminal
                  delay 0.25
                  send key "a" to keyTerminal
                  send key "b" to keyTerminal
                end tell
                """
            )
            run_osascript(key_input, timeout=15)
            key_bytes = wait_for_file(
                key_output_file,
                "scripted send key bytes",
                lambda data: data == b"ab",
            )
            require(key_bytes == b"ab", f"unexpected send key bytes: {key_bytes!r}")

            wait_for_file(mouse_ready_file, "mouse capture readiness marker")

            def send_mouse(script_line: str) -> None:
                mouse_input = textwrap.dedent(
                    f"""
                    tell application {app_literal}
                      set mouseTerminal to terminal id {quote_applescript(mouse_terminal_id)}
                      focus mouseTerminal
                      delay 0.25
                      {script_line}
                    end tell
                    """
                )
                run_osascript(mouse_input, timeout=15)

            def wait_for_mouse_growth(previous_len: int, description: str) -> int:
                mouse_bytes = wait_for_file(
                    mouse_output_file,
                    description,
                    lambda data: len(data) > previous_len
                    and (b"\x1b[" in data or b"\x1b[M" in data),
                    timeout=12.0,
                )
                return len(mouse_bytes)

            mouse_len = 0
            send_mouse("send mouse position x 24 y 24 to mouseTerminal")
            mouse_len = wait_for_mouse_growth(mouse_len, "scripted mouse position bytes")
            send_mouse("send mouse button left button action press to mouseTerminal")
            mouse_len = wait_for_mouse_growth(mouse_len, "scripted mouse button press bytes")
            send_mouse("send mouse position x 40 y 24 to mouseTerminal")
            mouse_len = wait_for_mouse_growth(mouse_len, "scripted mouse drag position bytes")
            send_mouse("send mouse button left button action release to mouseTerminal")
            mouse_len = wait_for_mouse_growth(mouse_len, "scripted mouse button release bytes")
            send_mouse("send mouse scroll x 0 y -3 precision false momentum none to mouseTerminal")
            wait_for_mouse_growth(mouse_len, "scripted mouse scroll bytes")
        finally:
            terminate_process(pid)

    new_crash_reports = wait_for_crash_report_settle(crash_reports_before)
    require(
        not new_crash_reports,
        "Roastty wrote crash reports during AppleScript workflow: "
        + ", ".join(str(path) for path in sorted(new_crash_reports)),
    )

    assert_inventory_split()
    print("macos_applescript_workflow_runtime=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
