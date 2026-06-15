#!/usr/bin/env python3
"""Guard deterministic renderer-knob runtime parity for Issue 805 CFG-223."""

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
    ghostty_cell = read("vendor/ghostty/src/renderer/cell.zig")
    roastty_renderer = read("roastty/src/renderer/frame_renderer.rs")
    roastty_cell = read("roastty/src/renderer/cell.rs")
    roastty_rebuild = read("roastty/src/renderer/frame_rebuild.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_renderer,
        [
            (
                '.background_opacity = @max(0, @min(1, config.@"background-opacity")),',
                "Ghostty background-opacity renderer clamp",
            ),
            (
                '.background_opacity_cells = config.@"background-opacity-cells",',
                "Ghostty background-opacity-cells renderer config",
            ),
            (
                '.cursor_opacity = @max(0, @min(1, config.@"cursor-opacity")),',
                "Ghostty cursor-opacity renderer clamp",
            ),
            (
                '.faint_opacity = @intFromFloat(@ceil(config.@"faint-opacity" * 255)),',
                "Ghostty faint-opacity renderer conversion",
            ),
            (
                '.padding_color = config.@"window-padding-color",',
                "Ghostty window-padding-color renderer config",
            ),
            (
                '.font_thicken = config.@"font-thicken",',
                "Ghostty font-thicken renderer config",
            ),
            (
                '.font_thicken_strength = config.@"font-thicken-strength",',
                "Ghostty font-thicken-strength renderer config",
            ),
            (
                "switch (self.config.padding_color)",
                "Ghostty padding-color draw-path decision",
            ),
            (
                ".thicken = self.config.font_thicken,",
                "Ghostty glyph render thicken use",
            ),
            (
                ".thicken_strength = self.config.font_thicken_strength,",
                "Ghostty glyph render thicken-strength use",
            ),
        ],
    )
    require_all(
        ghostty_cell,
        [
            (
                "make window-padding-color=extend work better. See #2099.",
                "Ghostty padding extension covering-cell helper",
            ),
        ],
    )

    require_all(
        roastty_renderer,
        [
            ("pub(crate) struct FrameRenderKnobs", "Roastty render knobs struct"),
            ("pub(crate) fn from_config", "Roastty render knobs from_config"),
            (
                "background_opacity: config.background_opacity.clamp(0.0, 1.0)",
                "Roastty background-opacity clamp",
            ),
            (
                "faint_opacity: (config.faint_opacity.clamp(0.0, 1.0) * 255.0).ceil() as u8",
                "Roastty faint-opacity conversion",
            ),
            (
                "cursor_overlay_alpha: (config.cursor_opacity.clamp(0.0, 1.0) * 255.0).ceil() as u8",
                "Roastty cursor-opacity conversion",
            ),
            (
                "padding_color: config.window_padding_color",
                "Roastty window-padding-color knob",
            ),
            ("thicken: config.font_thicken", "Roastty font-thicken knob"),
            (
                "thicken_strength: config.font_thicken_strength",
                "Roastty font-thicken-strength knob",
            ),
            (
                "fn from_config_sources_config_values",
                "Roastty config-sourced knob test",
            ),
            ("assert!(knobs.thicken)", "Roastty thicken assertion"),
            (
                "assert_eq!(knobs.thicken_strength, 200)",
                "Roastty thicken-strength assertion",
            ),
            (
                "fn background_opacity_clamps_for_renderer_knob",
                "Roastty background-opacity clamp test",
            ),
            (
                "fn from_config_sources_opacity_options",
                "Roastty opacity knob test",
            ),
            (
                "fn cursor_opacity_clamps_to_cursor_overlay_alpha_only",
                "Roastty cursor opacity test",
            ),
        ],
    )
    require_all(
        roastty_cell,
        [
            (
                "fn rebuild_bg_row_background_opacity_cells",
                "Roastty background opacity cells test",
            ),
            (
                "fn rebuild_bg_row_opacity_cells_off_is_unchanged",
                "Roastty opacity cells disabled test",
            ),
            (
                "fn rebuild_bg_row_opacity_cells_skips_covering_derived",
                "Roastty covering-cell opacity skip test",
            ),
        ],
    )
    require_all(
        roastty_rebuild,
        [
            (
                "fn refine_padding_extend_rows_top_row_can_clear_up_edge",
                "Roastty top padding extension test",
            ),
            (
                "fn refine_padding_extend_rows_bottom_row_can_clear_down_edge",
                "Roastty bottom padding extension test",
            ),
            (
                "fn refine_padding_extend_rows_background_and_extend_always_skip_row_inputs",
                "Roastty padding color branch test",
            ),
            (
                "padding_extend_input(WindowPaddingColor::Extend",
                "Roastty padding extend input coverage",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-008B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-008B1 status"),
            ("deterministic render knob", "RUNTIME-008B1 behavior"),
            ("background/faint/cursor opacity", "RUNTIME-008B1 opacity evidence"),
            ("background-opacity-cells", "RUNTIME-008B1 cell evidence"),
            ("window-padding-color", "RUNTIME-008B1 padding color evidence"),
            ("font-thicken", "RUNTIME-008B1 thicken evidence"),
            ("renderer_knobs_runtime_parity.py", "RUNTIME-008B1 guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-008B2B2")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-008B2B2 status"),
            ("background blur", "RUNTIME-008B2B2 blur gap"),
            ("real compositor opacity", "RUNTIME-008B2B2 compositor opacity gap"),
            ("window padding layout pixels", "RUNTIME-008B2B2 padding layout gap"),
            ("GUI cursor pixels", "RUNTIME-008B2B2 GUI cursor gap"),
            ("custom shader output", "RUNTIME-008B2B2 custom shader gap"),
            ("broader GUI/pixel parity", "RUNTIME-008B2B2 GUI parity gap"),
        ],
    )
    if "RUNTIME-008B |" in runtime_inventory:
        raise AssertionError("old broad RUNTIME-008B row is still present")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 status"),
            ("48 rows Oracle complete", "CFG-223 oracle count"),
            ("50 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("renderer_knobs_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
