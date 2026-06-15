#!/usr/bin/env python3
"""Guard font-shaping-break runtime parity for Issue 805 CFG-223."""

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


def main() -> int:
    ghostty_renderer = read("vendor/ghostty/src/renderer/generic.zig")
    ghostty_shape = read("vendor/ghostty/src/font/shape.zig")
    roastty_rebuild = read("roastty/src/renderer/frame_rebuild.rs")
    roastty_frame = read("roastty/src/renderer/frame_renderer.rs")
    roastty_run = read("roastty/src/font/run.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_renderer,
        [
            ("font_shaping_break: configpkg.FontShapingBreak", "Ghostty derived config field"),
            (".font_shaping_break = config.@\"font-shaping-break\"", "Ghostty config source"),
            (".cursor_x = cursor_x:", "Ghostty cursor x derivation"),
            ("const vp = state.cursor.viewport", "Ghostty viewport cursor source"),
            ("run_iter_opts.applyBreakConfig(self.config.font_shaping_break)", "Ghostty renderer break application"),
        ],
    )
    require_all(
        ghostty_shape,
        [
            ("pub fn applyBreakConfig", "Ghostty shape break helper"),
            ("if (!config.cursor) self.cursor_x = null;", "Ghostty no-cursor behavior"),
        ],
    )

    require_all(
        roastty_run,
        [
            ("pub(crate) fn apply_break_config", "Roastty break helper"),
            ("if !config.cursor", "Roastty no-cursor condition"),
            ("self.cursor_x = None", "Roastty no-cursor clears cursor"),
            ("fn apply_break_config_clears_cursor_x_when_off", "Roastty helper test"),
            ("fn next_breaks_on_cursor_exact", "Roastty cursor exact split test"),
            ("fn next_breaks_on_cursor_before", "Roastty cursor before split test"),
        ],
    )
    require_all(
        roastty_rebuild,
        [
            ("font_shaping_break: FontShapingBreak", "row-format break field"),
            ("row_format_options(&input, row)", "row-format option helper call"),
            ("opts.apply_break_config(input.font_shaping_break)", "row-format break application"),
            ("shape_row_cached_options(", "shaping consumes adjusted options"),
            (
                "font_shaping_break_runtime_default_preserves_cursor_break",
                "default preserves cursor test",
            ),
            (
                "font_shaping_break_runtime_no_cursor_removes_cursor_break",
                "no-cursor removes cursor test",
            ),
        ],
    )
    require_all(
        roastty_frame,
        [
            ("font_shaping_break: FontShapingBreak", "frame knob field"),
            ("font_shaping_break: config.font_shaping_break", "frame knob config source"),
            ("font_shaping_break: knobs.font_shaping_break", "rebuild input source"),
            (
                "font_shaping_break_runtime_active_frame_sources_config",
                "active frame config test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-007B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-007B2A status"),
            ("font-shaping-break", "RUNTIME-007B2A behavior"),
            ("active frame row formatting", "RUNTIME-007B2A active path"),
            ("font_shaping_break_runtime", "RUNTIME-007B2A tests"),
            ("font_shaping_break_runtime_parity.py", "RUNTIME-007B2A guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-007B2B2B2B2")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-007B2B2B2B2 status"),
            ("bitmap/color font thickening edge cases", "RUNTIME-007B2B2B2B2 thicken edge gap"),
            ("renderer-visible font pixel parity", "RUNTIME-007B2B2B2B2 pixel gap"),
        ],
    )
    if "RUNTIME-007B2 |" in runtime_inventory:
        raise AssertionError("old broad RUNTIME-007B2 row is still present")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 status"),
            ("75 rows Oracle complete", "CFG-223 oracle count"),
            ("78 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("font_shaping_break_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
