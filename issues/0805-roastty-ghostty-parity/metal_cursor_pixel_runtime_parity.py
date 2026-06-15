#!/usr/bin/env python3
"""Guard Metal cursor pixel readback parity for Issue 805 CFG-223."""

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
    ghostty_shader = read("vendor/ghostty/src/renderer/shaders/shaders.metal")
    roastty_shader = read("roastty/src/renderer/metal/shaders.metal")
    roastty_render_pass = read("roastty/src/renderer/metal/render_pass.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    shader_markers = [
        ("ushort2 cursor_pos;", "cursor position uniform"),
        ("bool cursor_wide;", "wide cursor uniform"),
        ("IS_CURSOR_GLYPH = 2u,", "cursor glyph flag"),
        ("bool is_cursor_pos = (", "cursor position predicate"),
        ("in.grid_pos.x == uniforms.cursor_pos.x ||", "cursor x predicate"),
        ("uniforms.cursor_wide &&", "wide cursor predicate"),
        ("in.grid_pos.x == uniforms.cursor_pos.x + 1", "wide second-cell predicate"),
        ("in.grid_pos.y == uniforms.cursor_pos.y;", "cursor y predicate"),
        ("if ((in.bools & IS_CURSOR_GLYPH) == 0 && is_cursor_pos)", "non-cursor glyph gate"),
        ("uniforms.cursor_color,", "cursor color replacement"),
    ]
    require_all(ghostty_shader, [(needle, f"Ghostty {label}") for needle, label in shader_markers])
    require_all(roastty_shader, [(needle, f"Roastty {label}") for needle, label in shader_markers])

    require_all(
        roastty_render_pass,
        [
            ("fn cell_text_cursor_pos_overrides_non_cursor_glyph_color", "cursor recolor test"),
            ("assert_pixel_grid(&target.read_bytes(), 1, &[[0, 255, 0, 255]]);", "cursor recolor bytes"),
            ("fn cell_text_cursor_glyph_flag_preserves_vertex_color", "cursor glyph preserve test"),
            ("vertex.flags = CellTextFlags::new(false, true);", "cursor glyph flag setup"),
            ("assert_pixel_grid(&target.read_bytes(), 1, &[[0, 0, 255, 255]]);", "cursor glyph preserve bytes"),
            ("fn cell_text_wide_cursor_overrides_second_cell", "wide cursor second-cell test"),
            (
                "assert_pixel_grid(&target.read_bytes(), 2, &[[0, 0, 0, 0], [0, 255, 0, 255]]);",
                "wide cursor second-cell bytes",
            ),
            ("fn cell_text_non_wide_cursor_does_not_override_second_cell", "non-wide cursor second-cell test"),
            (
                "assert_pixel_grid(&target.read_bytes(), 2, &[[0, 0, 0, 0], [0, 0, 255, 255]]);",
                "non-wide cursor second-cell bytes",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-008B2B2B2B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "complete row status"),
            ("Metal text shader cursor pixel readback", "complete row behavior"),
            ("Experiment 164", "complete row experiment"),
            ("cursor-position recolor", "cursor recolor evidence"),
            ("cursor-glyph color preservation", "cursor glyph evidence"),
            ("wide-cursor second-cell recolor", "wide cursor evidence"),
            ("non-wide second-cell non-recolor", "non-wide cursor evidence"),
            ("metal_cursor_pixel_runtime_parity.py", "complete row guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-008B2B2B2B2B")
    require_all(
        row_gap,
        [
            ("Gap", "remaining row status"),
            ("actual app/GUI cursor screenshots", "remaining cursor screenshot gap"),
            ("broader GUI/pixel parity", "remaining GUI parity gap"),
            ("screenshot-level padding pixel proof", "remaining padding pixel gap"),
        ],
    )
    if "RUNTIME-008B2B2B2B2 |" in runtime_inventory:
        raise AssertionError("old broad RUNTIME-008B2B2B2B2 row is still present")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 status"),
            ("73 rows Oracle complete", "CFG-223 oracle count"),
            ("76 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("metal_cursor_pixel_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
