#!/usr/bin/env python3
"""Guard font-thicken render runtime parity for Issue 805 CFG-223."""

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
    ghostty_grid = read("vendor/ghostty/src/font/SharedGrid.zig")
    ghostty_face = read("vendor/ghostty/src/font/face/coretext.zig")
    ghostty_face_mod = read("vendor/ghostty/src/font/face.zig")
    roastty_frame = read("roastty/src/renderer/frame_renderer.rs")
    roastty_cell = read("roastty/src/renderer/cell.rs")
    roastty_grid = read("roastty/src/font/shared_grid.rs")
    roastty_face = read("roastty/src/font/face/coretext.rs")
    issue_readme = (ISSUE / "README.md").read_text()
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_renderer,
        [
            (".font_thicken = config.@\"font-thicken\"", "Ghostty thicken config source"),
            (
                ".font_thicken_strength = config.@\"font-thicken-strength\"",
                "Ghostty thicken strength config source",
            ),
            (".thicken = self.config.font_thicken", "Ghostty glyph thicken option"),
            (
                ".thicken_strength = self.config.font_thicken_strength",
                "Ghostty glyph thicken strength option",
            ),
        ],
    )
    require_all(
        ghostty_grid,
        [
            ("thicken: bool", "Ghostty packed thicken key field"),
            ("thicken_strength: u8", "Ghostty packed strength key field"),
            (".thicken = key.opts.thicken", "Ghostty key thicken source"),
            (
                ".thicken_strength = key.opts.thicken_strength",
                "Ghostty key strength source",
            ),
        ],
    )
    require_all(
        ghostty_face_mod,
        [
            ("thicken: bool = false", "Ghostty render options thicken field"),
            ("thicken_strength: u8 = 255", "Ghostty render options strength field"),
        ],
    )
    require_all(
        ghostty_face,
        [
            ("const sbix = is_color and", "Ghostty sbix classification"),
            (
                "const canvas_padding: u32 = if (opts.thicken and !sbix) 1 else 0;",
                "Ghostty non-sbix thicken padding",
            ),
            (
                "context.setShouldSmoothFonts(ctx, opts.thicken)",
                "Ghostty thicken smoothing flag",
            ),
            (
                "const strength: f64 = @floatFromInt(opts.thicken_strength)",
                "Ghostty strength conversion",
            ),
            ("context.setGrayFillColor(ctx, strength / 255.0, 1)", "Ghostty fill strength"),
            (
                "context.setGrayStrokeColor(ctx, strength / 255.0, 1)",
                "Ghostty stroke strength",
            ),
        ],
    )

    require_all(
        roastty_frame,
        [
            ("thicken: config.font_thicken", "Roastty thicken knob source"),
            (
                "thicken_strength: config.font_thicken_strength",
                "Roastty strength knob source",
            ),
            ("thicken: knobs.thicken", "Roastty row-format thicken source"),
            (
                "thicken_strength: knobs.thicken_strength",
                "Roastty row-format strength source",
            ),
            (
                "font_thicken_render_runtime_active_frame_sources_config",
                "Roastty active frame thicken config test",
            ),
        ],
    )
    require_all(
        roastty_cell,
        [
            ("thicken: bool", "Roastty render_options thicken parameter"),
            ("thicken_strength: u8", "Roastty render_options strength parameter"),
            ("thicken,", "Roastty RenderOptions thicken passthrough"),
            ("thicken_strength,", "Roastty RenderOptions strength passthrough"),
            (
                "fn render_options_plain_letter_has_no_constraint",
                "Roastty render options passthrough test",
            ),
            ("assert!(opts.thicken)", "Roastty thicken passthrough assertion"),
            (
                "assert_eq!(opts.thicken_strength, 200)",
                "Roastty strength passthrough assertion",
            ),
        ],
    )
    require_all(
        roastty_grid,
        [
            ("thicken: bool", "Roastty glyph key thicken field"),
            ("thicken_strength: u8", "Roastty glyph key strength field"),
            ("thicken: opts.thicken", "Roastty key thicken source"),
            (
                "thicken_strength: opts.thicken_strength",
                "Roastty key strength source",
            ),
            ("fn render_glyph_caches_by_key", "Roastty glyph cache test"),
            ("let thick_opts = RenderOptions", "Roastty cache thicken variant"),
            ("let dim_thick_opts = RenderOptions", "Roastty cache strength variant"),
            ("assert_eq!(grid.glyphs.len(), 4)", "Roastty cache separates strength"),
        ],
    )
    require_all(
        roastty_face,
        [
            ("let sbix = is_color &&", "Roastty sbix classification"),
            (
                "let canvas_padding: i32 = if opts.thicken && !sbix { 1 } else { 0 };",
                "Roastty non-sbix thicken padding",
            ),
            (
                "CGContext::set_should_smooth_fonts(Some(&ctx), thicken)",
                "Roastty thicken smoothing flag",
            ),
            (
                "opts.thicken_strength as f64 / 255.0",
                "Roastty strength conversion",
            ),
            ("CGContext::set_gray_fill_color(Some(&ctx), fill_gray, 1.0)", "Roastty fill strength"),
            (
                "CGContext::set_gray_stroke_color(Some(&ctx), fill_gray, 1.0)",
                "Roastty stroke strength",
            ),
            ("fn render_glyph_thicken_pads_canvas", "Roastty thicken padding test"),
            ("fn render_glyph_strength_dims_fill", "Roastty strength dimming test"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-007B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-007B2B1 status"),
            ("font-thicken", "RUNTIME-007B2B1 behavior"),
            ("non-`sbix`", "RUNTIME-007B2B1 non-sbix scope"),
            ("glyph cache separation", "RUNTIME-007B2B1 cache evidence"),
            ("font_thicken_render_runtime_parity.py", "RUNTIME-007B2B1 guard"),
        ],
    )

    row_font_residual = require_row(runtime_inventory, "RUNTIME-007B2B2B2B2")
    require_all(
        row_font_residual,
        [
            ("Oracle complete", "RUNTIME-007B2B2B2B2 status"),
            ("font renderer residual output effects", "RUNTIME-007B2B2B2B2 behavior"),
            ("`sbix` bitmap-color", "RUNTIME-007B2B2B2B2 sbix evidence"),
            ("font_renderer_residual_parity.py", "RUNTIME-007B2B2B2B2 guard"),
        ],
    )
    if 'id="RUNTIME-007B2B",' in read(
        "issues/0805-roastty-ghostty-parity/config_runtime_inventory.py"
    ):
        raise AssertionError("old broad RUNTIME-007B2B row is still present")
    require(issue_readme, "`RUNTIME-007B2B2B2B2`.", "current learnings point at reduced font gap")
    if "Remaining font work stays in\n  `RUNTIME-007B2B2`." in issue_readme:
        raise AssertionError("current learnings still point at old broad RUNTIME-007B2B2 row")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 status"),
            ("89 rows Oracle complete", "CFG-223 oracle count"),
            ("92 rows closed", "CFG-223 closed count"),
            ("1 rows are incomplete", "CFG-223 incomplete count"),
            ("1 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("font_thicken_render_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
