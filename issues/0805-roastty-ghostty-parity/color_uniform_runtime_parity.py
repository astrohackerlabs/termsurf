#!/usr/bin/env python3
"""Guard colorspace and alpha-blending uniform parity for Issue 805 CFG-223."""

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
    ghostty_config = read("vendor/ghostty/src/config/Config.zig")
    ghostty_renderer = read("vendor/ghostty/src/renderer/generic.zig")
    ghostty_shader = read("vendor/ghostty/src/renderer/shaders/shaders.metal")
    roastty_config = read("roastty/src/config/mod.rs")
    roastty_uniforms = read("roastty/src/renderer/metal/shaders.rs")
    roastty_shader = read("roastty/src/renderer/metal/shaders.metal")
    inventory_source = read(
        "issues/0805-roastty-ghostty-parity/config_runtime_inventory.py"
    )
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_config,
        [
            ('@"window-colorspace": WindowColorspace = .srgb,', "Ghostty colorspace field"),
            ('@"alpha-blending": AlphaBlending =', "Ghostty alpha field"),
            ("pub const WindowColorspace = enum", "Ghostty colorspace enum"),
            ('@"display-p3"', "Ghostty Display P3 value"),
            ("pub const AlphaBlending = enum", "Ghostty alpha enum"),
            (".linear,", "Ghostty linear alpha value"),
            ('.@"linear-corrected"', "Ghostty corrected alpha value"),
        ],
    )
    require_all(
        ghostty_renderer,
        [
            ('.colorspace = config.@"window-colorspace",', "Ghostty derived colorspace"),
            ('.blending = config.@"alpha-blending",', "Ghostty derived blending"),
            (
                '.use_display_p3 = options.config.colorspace == .@"display-p3",',
                "Ghostty initial Display P3 bool",
            ),
            (
                ".use_linear_blending = options.config.blending.isLinear(),",
                "Ghostty initial linear bool",
            ),
            (
                '.use_linear_correction = options.config.blending == .@"linear-corrected",',
                "Ghostty initial correction bool",
            ),
            (
                'self.uniforms.bools.use_display_p3 = config.colorspace == .@"display-p3";',
                "Ghostty changeConfig Display P3 bool",
            ),
            (
                "self.uniforms.bools.use_linear_blending = config.blending.isLinear();",
                "Ghostty changeConfig linear bool",
            ),
            (
                'self.uniforms.bools.use_linear_correction = config.blending == .@"linear-corrected";',
                "Ghostty changeConfig correction bool",
            ),
        ],
    )

    shader_markers = [
        ("bool use_display_p3;", "Display P3 bool field"),
        ("bool use_linear_blending;", "linear blending bool field"),
        ("bool use_linear_correction;", "linear correction bool field"),
        ("uniforms.use_display_p3,", "Display P3 shader use"),
        ("uniforms.use_linear_blending", "linear blending shader use"),
        ("if (!uniforms.use_linear_blending)", "non-linear branch"),
        ("if (uniforms.use_linear_correction)", "linear correction branch"),
    ]
    require_all(
        ghostty_shader,
        [(needle, f"Ghostty shader {label}") for needle, label in shader_markers],
    )
    require_all(
        roastty_shader,
        [(needle, f"Roastty shader {label}") for needle, label in shader_markers],
    )

    require_all(
        roastty_config,
        [
            ("pub window_colorspace: WindowColorspace", "Roastty colorspace config field"),
            ("pub alpha_blending: AlphaBlending", "Roastty alpha config field"),
            ('"window-colorspace" => {', "Roastty colorspace parser"),
            ('"alpha-blending" => {', "Roastty alpha parser"),
            ("pub(crate) enum WindowColorspace", "Roastty colorspace enum"),
            ("DisplayP3", "Roastty Display P3 value"),
            ("pub(crate) enum AlphaBlending", "Roastty alpha enum"),
            ("LinearCorrected", "Roastty corrected alpha value"),
            ("pub(crate) fn is_linear(self) -> bool", "Roastty alpha linear helper"),
        ],
    )
    require_all(
        roastty_uniforms,
        [
            ("pub(crate) use_display_p3: bool", "Roastty Display P3 bool field"),
            ("pub(crate) use_linear_blending: bool", "Roastty linear bool field"),
            ("pub(crate) use_linear_correction: bool", "Roastty correction bool field"),
            ("config.window_colorspace", "Roastty colorspace from config"),
            ("config.alpha_blending", "Roastty alpha from config"),
            ("uniforms.update_color_config(colorspace, blending);", "Roastty constructor color update"),
            ("pub(crate) fn update_color_config(", "Roastty update helper"),
            (
                "self.bools.use_display_p3 = colorspace == WindowColorspace::DisplayP3;",
                "Roastty Display P3 mapping",
            ),
            (
                "self.bools.use_linear_blending = blending.is_linear();",
                "Roastty linear mapping",
            ),
            (
                "self.bools.use_linear_correction = blending == AlphaBlending::LinearCorrected;",
                "Roastty correction mapping",
            ),
            (
                "fn metal_uniform_layout_matches_standard_shader_struct",
                "Roastty uniform layout test",
            ),
            ("fn uniforms_from_config_sources_config_values", "Roastty from_config test"),
            ("fn uniforms_new_matches_the_init_literal", "Roastty constructor test"),
            ("fn uniforms_new_srgb_native_leaves_color_bools_false", "Roastty native test"),
            ("fn update_color_config_sets_the_color_space_bools", "Roastty update test"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-008B2B2B2B2B3")
    require_all(
        row_complete,
        [
            ("Oracle complete", "complete row status"),
            ("colorspace and alpha-blending", "complete row behavior"),
            ("Experiment 182", "complete row experiment"),
            ("use_display_p3", "Display P3 evidence"),
            ("use_linear_blending", "linear evidence"),
            ("use_linear_correction", "correction evidence"),
            ("color_uniform_runtime_parity.py", "complete row guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-008B2B2B2B2B4")
    require_all(
        row_gap,
        [
            ("Oracle complete", "scroll-to-bottom row status"),
            ("scroll-to-bottom.output", "scroll-to-bottom row remains"),
        ],
    )
    for forbidden in ["window-colorspace", "alpha-blending"]:
        if forbidden in row_gap:
            raise AssertionError(f"{forbidden} still appears in remaining renderer gap")

    require_all(
        inventory_source,
        [
            ('id="RUNTIME-008B2B2B2B2B3"', "source complete row"),
            ("color_uniform_runtime_parity.py", "source guard"),
            ('id="RUNTIME-008B2B2B2B2B4"', "source remaining row"),
            ("scroll-to-bottom.output", "source scroll row"),
        ],
    )

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 remains open"),
            ("89 rows Oracle complete", "CFG-223 oracle count"),
            ("92 rows closed", "CFG-223 closed count"),
            ("1 rows are incomplete", "CFG-223 incomplete count"),
            ("1 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("color_uniform_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
