#!/usr/bin/env python3
"""Guard the renderer-control runtime split for Issue 805 CFG-223."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ISSUE = ROOT / "issues/0805-roastty-ghostty-parity"


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


def require_matrix_cfg223(markdown: str) -> str:
    for line in markdown.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if cells and cells[0] == "CFG-223":
            return line
    raise AssertionError("missing CFG-223 matrix row")


def main() -> int:
    ghostty_config = read("vendor/ghostty/src/config/Config.zig")
    ghostty_surface = read("vendor/ghostty/src/Surface.zig")
    ghostty_renderer = read("vendor/ghostty/src/renderer/generic.zig")
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_config,
        [
            ('@"window-vsync": bool = true,', "Ghostty window-vsync default"),
            (
                "Changing this value at runtime will only affect new terminals.",
                "Ghostty window-vsync runtime scope",
            ),
            ('@"cursor-style-blink": ?bool = null,', "Ghostty cursor-style-blink default"),
        ],
    )
    require_all(
        ghostty_renderer,
        [
            (
                '.vsync = config.@"window-vsync",',
                "Ghostty renderer config consumes window-vsync",
            ),
            (
                '.background_blur = config.@"background-blur",',
                "Ghostty renderer visual fields remain adjacent",
            ),
        ],
    )
    require_all(
        ghostty_surface,
        [
            (".cursor_blink = config.@\"cursor-style-blink\",", "Ghostty termio cursor blink"),
        ],
    )

    require_all(
        roastty_lib,
        [
            ("parsed.window_vsync,", "Roastty parsed window_vsync surface option"),
            (
                "PresentDriver::start(surface_ptr, window_vsync)",
                "Roastty present driver uses parsed window_vsync",
            ),
            ("fn present_driver_vsync_false_selects_fallback_scheduler", "vsync false test"),
            (
                "fn present_driver_vsync_true_falls_back_when_display_link_fails",
                "vsync fallback test",
            ),
            (
                "fn present_driver_display_id_update_reaches_active_display_link",
                "display id update test",
            ),
            (
                "fn present_driver_stop_marks_driver_not_running_before_surface_drop",
                "present driver stop test",
            ),
            ("fn reset_cursor_blink_for_output", "cursor output reset helper"),
            ("fn advance_cursor_blink", "cursor blink advance helper"),
            ("fn focus_changed_for_cursor_blink", "cursor focus helper"),
            (
                "fn live_cursor_blink_tick_toggles_focused_surface_and_marks_dirty",
                "cursor tick test",
            ),
            (
                "fn live_cursor_blink_output_reset_is_throttled",
                "cursor output throttle test",
            ),
            (
                "fn live_cursor_blink_pump_resets_only_on_terminal_output",
                "cursor pump output test",
            ),
            (
                "fn live_cursor_blink_focus_loss_stops_toggling_and_focus_gain_resets",
                "cursor focus test",
            ),
            ("fn should_present_live", "live renderer visibility gate"),
            ("fn apply_visibility_options", "occlusion helper"),
            (
                "fn live_renderer_options_occlusion_abi_uses_visible_bool",
                "occlusion live view test",
            ),
            (
                "fn live_renderer_options_config_update_requests_live_rebuild",
                "config update rebuild test",
            ),
            (
                "fn live_renderer_options_focus_keeps_abi_only_surfaces_quiet",
                "ABI-only focus quiet test",
            ),
        ],
    )

    row_008a = require_row(runtime_inventory, "RUNTIME-008A")
    require_all(
        row_008a,
        [
            ("Oracle complete", "RUNTIME-008A status"),
            ("window-vsync", "RUNTIME-008A vsync evidence"),
            ("cursor blink", "RUNTIME-008A cursor evidence"),
            ("focus", "RUNTIME-008A focus evidence"),
            ("occlusion", "RUNTIME-008A occlusion evidence"),
            ("live renderer rebuild", "RUNTIME-008A live rebuild evidence"),
            ("renderer_control_runtime_parity.py", "RUNTIME-008A guard"),
        ],
    )

    row_008b = require_row(runtime_inventory, "RUNTIME-008B")
    require_all(
        row_008b,
        [
            ("Gap", "RUNTIME-008B status"),
            ("visible opacity", "RUNTIME-008B opacity gap"),
            ("blur", "RUNTIME-008B blur gap"),
            ("padding", "RUNTIME-008B padding gap"),
            ("cursor style shape/rendering", "RUNTIME-008B cursor style gap"),
            ("window padding color", "RUNTIME-008B padding color gap"),
            ("custom shader output", "RUNTIME-008B custom shader gap"),
        ],
    )

    cfg223 = require_matrix_cfg223(config_matrix)
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 remains gap"),
            ("27 rows Oracle complete", "CFG-223 oracle count"),
            ("29 rows closed", "CFG-223 closed count"),
            ("5 rows are incomplete", "CFG-223 incomplete count"),
            ("5 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
