#!/usr/bin/env python3
"""Residual split guard for Issue 805 Experiment 186."""

from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ISSUE = ROOT / "issues/0805-roastty-ghostty-parity"
RUNTIME = ISSUE / "config-runtime-inventory.md"
MATRIX = ISSUE / "config-matrix.md"
SOURCE = ISSUE / "config_runtime_inventory.py"
LIVE_GUARD = ISSUE / "macos_notification_link_bell_trace_runtime.py"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def require_text(text: str, needle: str, description: str) -> None:
    require(needle in text, f"missing {description}: {needle}")


def require_absent(text: str, needle: str, description: str) -> None:
    require(needle not in text, f"unexpected {description}: {needle}")


def row_line(inventory: str, row_id: str) -> str:
    prefix = f"| {row_id} "
    for line in inventory.splitlines():
        if line.startswith(prefix):
            return line
    raise AssertionError(f"missing runtime row {row_id}")


def row_cells(markdown: str, row_id: str) -> list[str]:
    line = row_line(markdown, row_id)
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def matrix_row(markdown: str, row_id: str) -> list[str]:
    for line in markdown.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if cells and cells[0] == row_id:
            return cells
    raise AssertionError(f"missing matrix row {row_id}")


def main() -> int:
    runtime = RUNTIME.read_text()
    matrix = MATRIX.read_text()
    source = SOURCE.read_text()

    cfg223 = matrix_row(matrix, "CFG-223")
    require(cfg223[1] == "Runtime and UI effects", f"unexpected CFG-223 behavior: {cfg223[1]}")
    require("Gap" in cfg223, f"CFG-223 should remain Gap: {cfg223}")
    require_text(
        matrix,
        "Runtime inventory coverage: 93 rows Oracle complete; 96 rows closed; 1 rows are incomplete and 1 rows are runtime gaps.",
        "CFG-223 split counts",
    )

    require_absent(runtime, "| RUNTIME-012B2B2B2B2B3     | remaining OS-controlled notification, bell, link, menu, preview, and URL-opening GUI effects", "old broad residual row")
    require_absent(source, 'id="RUNTIME-012B2B2B2B2B3"', "old broad residual source row")

    notification_cells = row_cells(runtime, "RUNTIME-012B2B2B2B2B3A")
    notification = row_line(runtime, "RUNTIME-012B2B2B2B2B3A")
    require(notification_cells[4] == "notifications", f"unexpected notification row family: {notification_cells}")
    require(notification_cells[5] == "Oracle complete", f"unexpected notification row status: {notification_cells}")
    require_text(notification, "authorizationStatus=1", "notification denied authorization evidence")
    require_text(notification, "macos_notification_link_bell_trace_runtime.py", "notification live guard command")

    bell_cells = row_cells(runtime, "RUNTIME-012B2B2B2B2B3B")
    bell = row_line(runtime, "RUNTIME-012B2B2B2B2B3B")
    require(bell_cells[4] == "notifications", f"unexpected bell row family: {bell_cells}")
    require(bell_cells[5] == "Oracle complete", f"unexpected bell row status: {bell_cells}")
    require_text(bell, "configured audio-path request trace", "bell audio trace evidence")
    require_text(bell, "macos_notification_link_bell_trace_runtime.py", "bell live guard command")

    context_menu_cells = row_cells(runtime, "RUNTIME-012B2B2B2B2B3C1")
    context_menu = row_line(runtime, "RUNTIME-012B2B2B2B2B3C1")
    require(context_menu_cells[4] == "notifications", f"unexpected context-menu row family: {context_menu_cells}")
    require(context_menu_cells[5] == "Oracle complete", f"unexpected context-menu row status: {context_menu_cells}")
    require_text(context_menu, "native context-menu construction", "context-menu live evidence")
    require_text(context_menu, "macos_native_context_menu_trace_runtime.py", "context-menu live guard command")

    url_cells = row_cells(runtime, "RUNTIME-012B2B2B2B2B3C2")
    url = row_line(runtime, "RUNTIME-012B2B2B2B2B3C2")
    require(url_cells[4] == "notifications", f"unexpected URL row family: {url_cells}")
    require(url_cells[5] == "Oracle complete", f"unexpected URL row status: {url_cells}")
    require_text(url, "openURL", "URL-opening live evidence")
    require_text(url, "macos_controlled_url_open_runtime.py", "URL-opening live guard command")

    hover_cells = row_cells(runtime, "RUNTIME-012B2B2B2B2B3C3")
    hover = row_line(runtime, "RUNTIME-012B2B2B2B2B3C3")
    require(hover_cells[4] == "notifications", f"unexpected hover row family: {hover_cells}")
    require(hover_cells[5] == "Oracle complete", f"unexpected hover row status: {hover_cells}")
    require_text(hover, "cursorShape raw=3 pointerStyle=link", "link-hover cursor-shape evidence")
    require_text(hover, "mouseOverLink url=https://example.com/issue805-exp188-link-hover", "link-hover URL evidence")
    require_text(hover, "macos_live_link_hover_runtime.py", "link-hover live guard command")

    banner_cells = row_cells(runtime, "RUNTIME-012B2B2B2B2B3C4")
    banner = row_line(runtime, "RUNTIME-012B2B2B2B2B3C4")
    require(banner_cells[4] == "notifications", f"unexpected banner row family: {banner_cells}")
    require(banner_cells[5] == "Oracle complete", f"unexpected banner row status: {banner_cells}")
    require_text(banner, "URL hover banner display pixels", "URL hover banner behavior")
    require_text(banner, "32674 changed pixels", "URL hover banner pixel evidence")
    require_text(banner, "macos_live_link_hover_banner_pixels.py", "URL hover banner live guard command")

    bell_ui_cells = row_cells(runtime, "RUNTIME-012B2B2B2B2B3C5")
    bell_ui = row_line(runtime, "RUNTIME-012B2B2B2B2B3C5")
    require(bell_ui_cells[4] == "notifications", f"unexpected bell UI row family: {bell_ui_cells}")
    require(bell_ui_cells[5] == "Oracle complete", f"unexpected bell UI row status: {bell_ui_cells}")
    require_text(bell_ui, "bell title prefix and border overlay pixels", "bell title/border behavior")
    require_text(bell_ui, "🔔 Issue805Exp190BellTitle", "bell title prefix evidence")
    require_text(bell_ui, "6375 changed pixels", "bell border side pixel evidence")
    require_text(bell_ui, "macos_live_bell_title_border_pixels.py", "bell title/border live guard command")

    cursor_cells = row_cells(runtime, "RUNTIME-012B2B2B2B2B3C6")
    cursor = row_line(runtime, "RUNTIME-012B2B2B2B2B3C6")
    require(cursor_cells[4] == "notifications", f"unexpected cursor row family: {cursor_cells}")
    require(cursor_cells[5] == "Oracle complete", f"unexpected cursor row status: {cursor_cells}")
    require_text(cursor, "real OS link cursor pixels", "real OS cursor behavior")
    require_text(cursor, "350 normal-cursor changed pixels", "normal cursor pixel evidence")
    require_text(cursor, "701 link-cursor changed pixels", "link cursor pixel evidence")
    require_text(cursor, "721-pixel symmetric difference", "cursor symmetric-difference evidence")
    require_text(cursor, "macos_real_link_cursor_pixels.py", "real OS cursor live guard command")

    attention_cells = row_cells(runtime, "RUNTIME-012B2B2B2B2B3C7")
    attention = row_line(runtime, "RUNTIME-012B2B2B2B2B3C7")
    require(attention_cells[4] == "notifications", f"unexpected attention row family: {attention_cells}")
    require(attention_cells[5] == "Oracle complete", f"unexpected attention row status: {attention_cells}")
    require_text(attention, "background Dock attention request dispatch", "attention request behavior")
    require_text(attention, "appBell active=false", "inactive attention request evidence")
    require_text(attention, "appBell attentionRequest=0", "attention request return evidence")
    require_text(attention, "authorizationStatus=1 badgeSetting=2", "badge authorization evidence")
    require_text(attention, "macos_live_bell_attention_dock_state.py", "attention live guard command")

    quicklook_cells = row_cells(runtime, "RUNTIME-012B2B2B2B2B3C8")
    quicklook = row_line(runtime, "RUNTIME-012B2B2B2B2B3C8")
    require(quicklook_cells[4] == "notifications", f"unexpected Quick Look row family: {quicklook_cells}")
    require(quicklook_cells[5] == "Oracle complete", f"unexpected Quick Look row status: {quicklook_cells}")
    require_text(quicklook, "Quick Look/native definition UI", "Quick Look behavior")
    require_text(quicklook, "fontPresent=true", "Quick Look CoreText font evidence")
    require_text(quicklook, "showDefinition=true", "Quick Look showDefinition evidence")
    require_text(quicklook, "at least 50000 nonblack pixels", "Quick Look native popover threshold evidence")
    require_text(quicklook, "macos_live_quicklook_definition.py", "Quick Look live guard command")

    gap_cells = row_cells(runtime, "RUNTIME-012B2B2B2B2B3C")
    gap = row_line(runtime, "RUNTIME-012B2B2B2B2B3C")
    require(gap_cells[4] == "notifications", f"unexpected gap row family: {gap_cells}")
    require(gap_cells[5] == "Gap", f"unexpected gap row status: {gap_cells}")
    for needle in [
        "actual OS notification delivery/banner/sound",
        "audible bell output",
        "OS-visible dock-attention bounce/state beyond AppKit request dispatch",
        "external Launch Services handler delivery",
    ]:
        require_text(gap, needle, f"remaining exact gap slice {needle}")
    require_absent(gap, "real OS cursor pixels", "closed real OS cursor gap")
    require_absent(gap, "Quick Look/native link preview display", "stale remaining Quick Look gap")

    result = subprocess.run(
        ["python3", str(LIVE_GUARD)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=90,
    )
    require(
        result.returncode == 0,
        "live notification/link/bell trace guard failed\n"
        f"stdout:\n{result.stdout}\n"
        f"stderr:\n{result.stderr}",
    )
    require_text(result.stdout, "macos_notification_link_bell_trace_runtime=pass", "live guard pass marker")

    for guard in [
        ISSUE / "macos_native_context_menu_trace_runtime.py",
        ISSUE / "macos_controlled_url_open_runtime.py",
        ISSUE / "macos_live_link_hover_banner_pixels.py",
        ISSUE / "macos_live_bell_title_border_pixels.py",
        ISSUE / "macos_real_link_cursor_pixels.py",
        ISSUE / "macos_live_bell_attention_dock_state.py",
        ISSUE / "macos_live_quicklook_definition.py",
    ]:
        result = subprocess.run(
            ["python3", str(guard)],
            cwd=ROOT,
            text=True,
            capture_output=True,
            timeout=90,
        )
        require(
            result.returncode == 0,
            f"{guard.name} failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}",
        )

    print("notification_link_bell_gui_residual_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
