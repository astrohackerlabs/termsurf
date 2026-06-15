#!/usr/bin/env python3
"""Guard font renderer residual parity for Issue 805 CFG-223."""

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
    ghostty_coretext = read("vendor/ghostty/src/font/face/coretext.zig")
    ghostty_discovery = read("vendor/ghostty/src/font/discovery.zig")
    ghostty_shaper = read("vendor/ghostty/src/font/shaper/coretext.zig")
    ghostty_shared_grid = read("vendor/ghostty/src/font/SharedGrid.zig")
    roastty_coretext = read("roastty/src/font/face/coretext.rs")
    roastty_collection = read("roastty/src/font/collection.rs")
    roastty_shared_grid = read("roastty/src/font/shared_grid.rs")
    roastty_frame = read("roastty/src/renderer/frame_renderer.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_coretext,
        [
            ("pub fn renderGlyph", "Ghostty CoreText renderGlyph"),
            ("const is_color = self.isColorGlyph", "Ghostty color-glyph branch"),
            ("const sbix = is_color", "Ghostty sbix branch"),
            ("if (!sbix)", "Ghostty skips synthetic bold/padding for sbix"),
            ("opts.thicken and !sbix", "Ghostty thicken padding gate"),
            ("context.setShouldSmoothFonts", "Ghostty thicken smoothing hook"),
            ("self.font.drawGlyphs", "Ghostty glyph draw call"),
        ],
    )
    require_all(
        ghostty_discovery,
        [
            ("CTFontCreateForString", "Ghostty CoreText fallback API"),
            ('"LastResort"', "Ghostty LastResort rejection"),
            ("desc.codepoint", "Ghostty codepoint discovery descriptor"),
        ],
    )
    require_all(
        ghostty_shaper,
        [
            ("pub const Shaper", "Ghostty CoreText shaper"),
            ("pub fn shape", "Ghostty CoreText shape function"),
            ("is_first_codepoint_in_cluster", "Ghostty cluster handling"),
            ("is_after_glyph_from_current_or_next_clusters", "Ghostty reorder guard"),
        ],
    )
    require_all(
        ghostty_shared_grid,
        [
            ("atlas_color", "Ghostty color atlas"),
            ("renderGlyph", "Ghostty shared-grid glyph rendering"),
        ],
    )

    require_all(
        roastty_coretext,
        [
            ("pub(crate) fn render_glyph", "Roastty CoreText render_glyph"),
            ("let is_color = self.is_color_glyph", "Roastty color-glyph branch"),
            ("let sbix = is_color", "Roastty sbix branch"),
            ("opts.thicken && !sbix", "Roastty thicken padding gate"),
            ("font_renderer_residual_color_sbix_thicken_skips_canvas_padding", "Roastty sbix thicken test"),
            ("render_color_glyph_into_bgra_atlas", "Roastty color glyph BGRA test"),
            ("wrong_atlas_format_errors", "Roastty atlas format test"),
            ("render_glyph_stretch_fills_cell", "Roastty stretch pixel test"),
            ("render_glyph_thicken_pads_canvas", "Roastty thicken padding test"),
            ("render_glyph_strength_dims_fill", "Roastty thicken strength test"),
            ("font_for_codepoint_cjk", "Roastty CJK fallback test"),
            ("font_for_codepoint_supplementary", "Roastty supplementary fallback test"),
            ("font_for_codepoint_none", "Roastty LastResort rejection test"),
            ("shape_cluster_collapses_surrogate", "Roastty supplementary shaping test"),
            ("shape_run_combining_marks", "Roastty combining-mark shaping test"),
            ("shape_rtl_grid_ordered", "Roastty RTL shaping test"),
        ],
    )
    require_all(
        roastty_collection,
        [
            ("font_metric_modifier_runtime_update_metrics_applies_modifiers", "Roastty metric modifier test"),
            ("font_metric_modifier_runtime_cell_height_recenters_metrics", "Roastty metric recentering test"),
        ],
    )
    require_all(
        roastty_shared_grid,
        [
            ("render_glyph_text_places_glyph_in_grayscale_atlas", "Roastty shared-grid text glyph test"),
            ("render_glyph_caches_by_key", "Roastty glyph cache test"),
        ],
    )
    require_all(
        roastty_frame,
        [
            ("font_feature_runtime_active_frame_sources_config", "Roastty feature active-frame test"),
            ("font_thicken_render_runtime_active_frame_sources_config", "Roastty thicken active-frame test"),
            ("font_shaping_break_runtime_active_frame_sources_config", "Roastty shaping-break active-frame test"),
        ],
    )

    font_row = require_row(runtime_inventory, "RUNTIME-007B2B2B2B2")
    require_all(
        font_row,
        [
            ("Oracle complete", "font residual status"),
            ("font renderer residual output effects", "font residual behavior"),
            ("Experiment 184", "Experiment 184 evidence"),
            ("grayscale glyph rasterization", "grayscale pixel evidence"),
            ("stretched-cell glyph pixels", "stretched-cell evidence"),
            ("non-`sbix` thicken", "non-sbix thicken evidence"),
            ("`sbix` bitmap-color", "sbix color evidence"),
            ("Apple Color Emoji BGRA", "color atlas evidence"),
            ("CoreText fallback discovery", "fallback evidence"),
            ("shaping clusters", "shaping evidence"),
            ("font_renderer_residual_parity.py", "guard command"),
        ],
    )
    for forbidden in [
        "Remaining font parity still needs",
        "Add focused font renderer/runtime or GUI proof",
        "TBD by future CFG-223 font renderer experiment",
    ]:
        if forbidden in font_row:
            raise AssertionError(f"font residual row still contains stale gap text: {forbidden}")

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

    print("font_renderer_residual_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
