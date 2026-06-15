#!/usr/bin/env python3
"""Guard macOS walkthrough residual parity for Issue 805 CFG-223."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
ISSUE = ROOT / "issues/0805-roastty-ghostty-parity"
LIVE_GUARDS = [
    "macos_applescript_workflow_runtime.py",
    "macos_split_layout_runtime.py",
    "macos_titlebar_runtime.py",
    "macos_gui_state_runtime.py",
    "macos_quick_terminal_runtime.py",
    "macos_native_menu_runtime.py",
    "macos_gui_cursor_pixel_runtime.py",
    "macos_window_padding_pixel_runtime.py",
]


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise AssertionError(f"missing {label}: {needle!r}")


def require_all(text: str, needles: list[tuple[str, str]]) -> None:
    for needle, label in needles:
        require(text, needle, label)


def require_row(markdown: str, row_id: str) -> str:
    for line in markdown.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if cells and cells[0] == row_id:
            return line
    raise AssertionError(f"missing inventory row {row_id}")


def run_live_guards() -> None:
    env = os.environ.copy()
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    for guard in LIVE_GUARDS:
        print(f"running {guard}")
        result = subprocess.run(
            [sys.executable, str(ISSUE / guard)],
            cwd=ROOT,
            env=env,
        )
        if result.returncode != 0:
            raise AssertionError(f"{guard} failed with exit code {result.returncode}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--static-only",
        action="store_true",
        help="check source, inventory, and matrix anchors without launching the live app guards",
    )
    args = parser.parse_args()

    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()
    inventory_source = read("issues/0805-roastty-ghostty-parity/config_runtime_inventory.py")
    plumbing_guard = read("issues/0805-roastty-ghostty-parity/macos_app_workflow_plumbing_parity.py")
    applescript_guard = read("issues/0805-roastty-ghostty-parity/macos_applescript_workflow_runtime.py")
    state_guard = read("issues/0805-roastty-ghostty-parity/macos_gui_state_runtime.py")
    quick_guard = read("issues/0805-roastty-ghostty-parity/macos_quick_terminal_runtime.py")
    split_guard = read("issues/0805-roastty-ghostty-parity/macos_split_layout_runtime.py")
    titlebar_guard = read("issues/0805-roastty-ghostty-parity/macos_titlebar_runtime.py")
    menu_guard = read("issues/0805-roastty-ghostty-parity/macos_native_menu_runtime.py")
    cursor_guard = read("issues/0805-roastty-ghostty-parity/macos_gui_cursor_pixel_runtime.py")
    padding_guard = read("issues/0805-roastty-ghostty-parity/macos_window_padding_pixel_runtime.py")

    require_all(
        inventory_source,
        [
            ('id="RUNTIME-011B2B"', "macOS residual source row"),
            ('status="Oracle complete"', "macOS residual source complete status"),
            ("Experiment 185 closes the macOS walkthrough residual row", "Experiment 185 source evidence"),
            ("macos_walkthrough_residual_parity.py", "macOS residual guard command"),
            ('id="RUNTIME-012B2B2B2B2B3C"', "notification gap source row remains"),
        ],
    )

    require_all(
        plumbing_guard,
        [
            ("TerminalController.swift", "workflow plumbing terminal controller source parity"),
            ("TerminalWindow.swift", "workflow plumbing terminal window source parity"),
            ("QuickTerminalController.swift", "workflow plumbing quick terminal source parity"),
            ("SplitTreeTests", "workflow plumbing split tests"),
        ],
    )
    require_all(
        applescript_guard,
        [
            ("controlled child process records the `input text` marker", "AppleScript input marker guard"),
            ("selected tab's focused terminal ID changed", "AppleScript split focus guard"),
            ("controlled keyboard child process records exact raw bytes", "AppleScript keyboard side-effect guard"),
            ("controlled mouse child process records new terminal mouse-report bytes", "AppleScript mouse side-effect guard"),
        ],
    )
    require_all(
        state_guard,
        [
            ("live fullscreen and command-palette GUI state proof", "fullscreen and command palette inventory assertion"),
            ("PID-scoped CoreGraphics layer-0 window bounds", "fullscreen geometry inventory assertion"),
            ("baseline-to-palette screenshot delta", "command palette screenshot inventory assertion"),
        ],
    )
    require_all(
        quick_guard,
        [
            ("live Quick Terminal GUI visibility and geometry proof", "Quick Terminal inventory assertion"),
            ("exact Quick Terminal CGWindowID", "Quick Terminal exact window assertion"),
            ("Quick Terminal screenshot dimensions were empty", "Quick Terminal screenshot dimension assertion"),
        ],
    )
    require_all(
        split_guard,
        [
            ("live right-split visual layout proof", "right-split inventory assertion"),
            ("red-dominant", "right-split red sample assertion"),
            ("blue-dominant", "right-split blue sample assertion"),
        ],
    )
    require_all(
        titlebar_guard,
        [
            ("live hidden-titlebar visual proof", "hidden-titlebar inventory assertion"),
            ("red/yellow/green traffic-light", "traffic-light pixel assertion"),
            ("frontmost process Unix PID", "titlebar exact PID assertion"),
        ],
    )
    require_all(
        menu_guard,
        [
            ("menu_item_enabled", "native menu validation helper"),
            ('("File", "New Tab")', "native New Tab action assertion"),
            ('("File", "Split Right")', "native Split Right action assertion"),
            ("Split Right menu action", "native split action side-effect"),
        ],
    )
    require_all(
        cursor_guard,
        [
            ("focused live app/GUI block cursor pixel proof", "GUI cursor inventory assertion"),
            ("magenta-dominant", "GUI cursor pixel sampler"),
        ],
    )
    require_all(
        padding_guard,
        [
            ("focused live window-padding pixel proof", "window padding inventory assertion"),
            ("top/bottom/left/right", "window padding four-edge assertion"),
            ("padding screenshot dimensions were empty", "window padding screenshot assertion"),
        ],
    )

    macos_row = require_row(runtime_inventory, "RUNTIME-011B2B")
    require_all(
        macos_row,
        [
            ("Oracle complete", "macOS residual status"),
            ("live macOS GUI titlebar, split layout, screenshot/pixel", "macOS residual behavior"),
            ("Experiment 185", "macOS residual evidence"),
            ("renamed full-file macOS workflow source parity", "source parity evidence"),
            ("live AppleScript window/tab/split/input automation", "AppleScript evidence"),
            ("lower-level keyboard and mouse event delivery", "input evidence"),
            ("native menu visibility/action dispatch", "native menu evidence"),
            ("fullscreen and command-palette screenshots", "state screenshot evidence"),
            ("Quick Terminal screenshots", "Quick Terminal evidence"),
            ("right-split exact-window red/blue layout screenshots", "split screenshot evidence"),
            ("hidden titlebar traffic-light pixel proof", "titlebar evidence"),
            ("window-padding screenshot proof", "padding evidence"),
            ("GUI cursor pixel proof", "cursor evidence"),
            ("macos_walkthrough_residual_parity.py", "macOS residual guard"),
        ],
    )
    for forbidden in [
        "CFG-223 still needs real app walkthrough",
        "Add focused live macOS app walkthrough rows",
    ]:
        if forbidden in macos_row:
            raise AssertionError(f"macOS residual row still contains stale gap text: {forbidden}")

    notification_row = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2B3C")
    require_all(
        notification_row,
        [
            ("Gap", "notification/link/bell residual status"),
            ("remaining OS-controlled notification, bell, Quick Look/native preview, cursor-pixel, and external URL-handler GUI effects", "notification residual behavior"),
        ],
    )

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("89 rows Oracle complete", "CFG-223 oracle count"),
            ("92 rows closed", "CFG-223 closed count"),
            ("1 rows are incomplete", "CFG-223 incomplete count"),
            ("1 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    if not args.static_only:
        run_live_guards()

    print("macos_walkthrough_residual_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
