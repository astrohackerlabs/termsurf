#!/usr/bin/env python3
"""Guard config-derived font-grid runtime parity for Issue 805 CFG-223."""

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
    ghostty_surface = read("vendor/ghostty/src/Surface.zig")
    ghostty_grid = read("vendor/ghostty/src/font/SharedGridSet.zig")
    roastty_grid = read("roastty/src/font/shared_grid_set.rs")
    roastty_collection = read("roastty/src/font/collection.rs")
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_surface,
        [
            (
                ".font = try font.SharedGridSet.DerivedConfig.init(alloc, config)",
                "Ghostty derived font config",
            ),
            ("app.font_grid_set.ref(", "Ghostty font grid ref"),
            ("&derived_config.font", "Ghostty initial derived font grid input"),
            ("try self.setFontSize(font_size:", "Ghostty reload font-size path"),
            ("pub fn setFontSize", "Ghostty setFontSize"),
            (".font_grid = .{", "Ghostty renderer font-grid message"),
            ("self.font_size_adjusted = true", "Ghostty manual font-size flag"),
            ("self.font_size_adjusted = false", "Ghostty reset font-size flag"),
        ],
    )
    require_all(
        ghostty_grid,
        [
            ("pub const DerivedConfig = struct", "Ghostty SharedGridSet derived config"),
            ("pub fn init(", "Ghostty SharedGridSet derived init"),
            ('@"font-family"', "Ghostty font family key material"),
            ('@"font-style"', "Ghostty font style key material"),
            ('@"font-codepoint-map"', "Ghostty codepoint map key material"),
            ('@"font-synthetic-style"', "Ghostty synthetic style material"),
        ],
    )

    require_all(
        roastty_grid,
        [
            ("pub(crate) struct DerivedConfig", "Roastty derived config"),
            ("pub(crate) fn from_config", "Roastty derived config builder"),
            ("pub(crate) struct Key", "Roastty font grid key"),
            ("pub(crate) fn new(config: &DerivedConfig", "Roastty key builder"),
            ("append_descriptors(", "Roastty descriptor append"),
            ("collection.complete_styles", "Roastty synthetic style completion"),
            ("resolver.set_codepoint_map", "Roastty codepoint map wiring"),
            (
                "shared_grid_set_key_preserves_multiple_family_order",
                "multi-family order test",
            ),
            (
                "shared_grid_set_key_builds_style_ordered_descriptors",
                "style descriptor test",
            ),
            (
                "shared_grid_set_key_includes_codepoint_map",
                "codepoint-map key test",
            ),
            (
                "shared_grid_set_build_grid_from_default_config",
                "default grid build test",
            ),
            (
                "shared_grid_set_build_grid_honors_codepoint_override",
                "codepoint override grid test",
            ),
            (
                "shared_grid_set_build_grid_honors_disabled_synthetic_styles",
                "synthetic style grid test",
            ),
        ],
    )
    require_all(
        roastty_collection,
        [
            ("fn complete_styles", "Roastty style completion implementation"),
            ("synthetic_bold", "Roastty synthetic bold support"),
            ("synthetic_italic", "Roastty synthetic italic support"),
        ],
    )
    require_all(
        roastty_lib,
        [
            ("fn build_live_renderer", "Roastty live renderer builder"),
            (
                "font::shared_grid_set::build_grid_from_config(config, (font_size * scale as f32).max(1.0))",
                "Roastty initial live renderer font grid construction",
            ),
            ("self.renderer = build_live_renderer(", "Roastty live renderer startup"),
            ("fn set_font_size_points", "Roastty surface font-size state setter"),
            (
                "surface_reload_font_size_updates_unadjusted_and_preserves_manual",
                "Experiment 105 font-size runtime test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-007A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-007A status"),
            ("config-derived font grid", "font-grid behavior"),
            ("initial live renderer", "initial renderer wiring"),
            ("font_grid_runtime_parity.py", "static parity guard evidence"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-007B2B2B2B2")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-007B2B2B2B2 status"),
            ("bitmap/color font thickening edge cases", "remaining thicken edge gap"),
            ("renderer-visible font pixel parity", "remaining font pixel gap"),
        ],
    )
    if "RUNTIME-007 |" in runtime_inventory or "RUNTIME-007B |" in runtime_inventory:
        raise AssertionError("old broad RUNTIME-007 row is still present")

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

    print("font_grid_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
