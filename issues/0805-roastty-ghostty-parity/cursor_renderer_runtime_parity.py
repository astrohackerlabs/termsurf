#!/usr/bin/env python3
"""Guard deterministic cursor renderer runtime parity for Issue 805 CFG-223."""

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
    ghostty_cursor = read("vendor/ghostty/src/renderer/cursor.zig")
    ghostty_renderer = read("vendor/ghostty/src/renderer/generic.zig")
    ghostty_cell = read("vendor/ghostty/src/renderer/cell.zig")
    roastty_frame = read("roastty/src/renderer/frame_renderer.rs")
    roastty_cell = read("roastty/src/renderer/cell.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_cursor,
        [
            ("if (opts.preedit) return .block;", "Ghostty preedit cursor priority"),
            (
                "if (state.cursor.password_input) return .lock;",
                "Ghostty password cursor priority",
            ),
        ],
    )
    require_all(
        ghostty_renderer,
        [
            (
                "const cursor_style: renderer.CursorStyle = .fromTerminal(self.terminal_state.cursor.visual_style);",
                "Ghostty terminal visual style cursor derivation",
            ),
            (
                "const alpha = 255 * self.config.cursor_opacity;",
                "Ghostty focused cursor opacity alpha",
            ),
            ("font.Sprite = switch (cursor_style)", "Ghostty cursor sprite switch"),
            (".block => .cursor_rect,", "Ghostty block cursor sprite"),
            (".block_hollow => .cursor_hollow_rect,", "Ghostty hollow cursor sprite"),
            (".bar => .cursor_bar,", "Ghostty bar cursor sprite"),
            (".underline => .cursor_underline,", "Ghostty underline cursor sprite"),
            (
                ".lock => self.font_grid.renderCodepoint(",
                "Ghostty lock cursor render path",
            ),
            ("0xF023, // lock symbol", "Ghostty lock cursor codepoint"),
            (
                ".color = .{ cursor_color.r, cursor_color.g, cursor_color.b, alpha },",
                "Ghostty cursor vertex color alpha",
            ),
            (
                ".bools = .{ .is_cursor_glyph = true },",
                "Ghostty cursor vertex flag",
            ),
            (".glyph_pos = .{ render.glyph.atlas_x, render.glyph.atlas_y },", "Ghostty cursor glyph position"),
            (".glyph_size = .{ render.glyph.width, render.glyph.height },", "Ghostty cursor glyph size"),
            ("@intCast(render.glyph.offset_x)", "Ghostty cursor bearing x"),
            ("@intCast(render.glyph.offset_y)", "Ghostty cursor bearing y"),
        ],
    )
    require_all(
        ghostty_cell,
        [
            (
                ".block => self.fg_rows.lists[0].appendAssumeCapacity(cell)",
                "Ghostty block cursor first-list routing",
            ),
            (
                ".block_hollow, .bar, .underline, .lock => self.fg_rows.lists[self.size.rows + 1].appendAssumeCapacity(cell)",
                "Ghostty non-block cursor last-list routing",
            ),
        ],
    )

    require_all(
        roastty_frame,
        [
            (
                "FrameRenderState::from_terminal_with_cursor_options",
                "Roastty active cursor render state entry",
            ),
            (
                "fn render_state_derives_visible_block_cursor_overlay",
                "Roastty visible block cursor overlay test",
            ),
            (
                "fn render_state_cursor_color_comes_from_osc12",
                "Roastty OSC12 cursor color test",
            ),
            (
                "fn render_state_block_sets_uniform_underline_does_not",
                "Roastty block uniform vs underline overlay test",
            ),
            (
                "fn cursor_blink_render_state_hides_focused_blinking_cursor_when_not_visible",
                "Roastty focused hidden blink cursor test",
            ),
            (
                "fn cursor_blink_render_state_shows_focused_blinking_cursor_when_visible",
                "Roastty focused visible blink cursor test",
            ),
            (
                "fn cursor_blink_render_state_unfocused_cursor_is_hollow_even_when_blink_hidden",
                "Roastty unfocused hollow cursor test",
            ),
        ],
    )
    require_all(
        roastty_cell,
        [
            (
                "fn add_cursor_maps_styles_and_routes",
                "Roastty cursor style sprite/list test",
            ),
            (
                "fn add_cursor_wide_uses_two_cells",
                "Roastty wide cursor render-data test",
            ),
            (
                "fn add_cursor_lock_falls_back_when_glyph_absent",
                "Roastty lock cursor fallback test",
            ),
            (
                "fn cursor_text_color_resolves_the_cursor_text_config",
                "Roastty cursor-text color test",
            ),
            (
                "fn cursor_color_resolves_with_precedence",
                "Roastty cursor color precedence test",
            ),
            (
                "fn block_cursor_pos_adjusts_for_wide_kind",
                "Roastty wide-tail cursor position test",
            ),
            (
                "fn set_cursor_block_uses_first_list",
                "Roastty block cursor list test",
            ),
            (
                "fn set_cursor_other_styles_use_last_list",
                "Roastty non-block cursor list test",
            ),
            (
                "fn set_cursor_none_value_clears",
                "Roastty cursor none clear test",
            ),
            (
                "fn set_cursor_none_style_clears",
                "Roastty cursor none style clear test",
            ),
            (
                "fn set_cursor_replaces_previous",
                "Roastty cursor replacement test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-008B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-008B2A status"),
            ("active cursor overlay/uniform", "RUNTIME-008B2A active cursor behavior"),
            ("cursor color/text color", "RUNTIME-008B2A color evidence"),
            ("selected cursor sprite/glyph render data", "RUNTIME-008B2A render-data evidence"),
            ("wide cursor render data", "RUNTIME-008B2A wide cursor evidence"),
            ("lock fallback rendering", "RUNTIME-008B2A lock fallback evidence"),
            ("cursor list routing", "RUNTIME-008B2A list routing evidence"),
            ("cursor_renderer_runtime_parity.py", "RUNTIME-008B2A guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-008B2B2B2")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-008B2B2B2 status"),
            ("non-glass compositor opacity", "RUNTIME-008B2B2B2 blur gap"),
            ("non-glass compositor opacity", "RUNTIME-008B2B2B2 compositor opacity gap"),
            ("screenshot-level padding pixel proof", "RUNTIME-008B2B2B2 screenshot screenshot padding gap"),
            ("GUI cursor pixels", "RUNTIME-008B2B2B2 GUI cursor gap"),
            ("custom shader output", "RUNTIME-008B2B2B2 custom shader gap"),
            ("broader GUI/pixel parity", "RUNTIME-008B2B2B2 GUI parity gap"),
        ],
    )
    if "RUNTIME-008B2 |" in runtime_inventory:
        raise AssertionError("old broad RUNTIME-008B2 row is still present")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 status"),
            ("53 rows Oracle complete", "CFG-223 oracle count"),
            ("55 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("cursor_renderer_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
