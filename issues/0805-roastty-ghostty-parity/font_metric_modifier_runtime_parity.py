#!/usr/bin/env python3
"""Guard font metric modifier runtime parity for Issue 805 CFG-223."""

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
    ghostty_grid = read("vendor/ghostty/src/font/SharedGridSet.zig")
    ghostty_collection = read("vendor/ghostty/src/font/Collection.zig")
    ghostty_metrics = read("vendor/ghostty/src/font/Metrics.zig")
    roastty_grid = read("roastty/src/font/shared_grid_set.rs")
    roastty_collection = read("roastty/src/font/collection.rs")
    roastty_metrics = read("roastty/src/font/metrics.rs")
    roastty_config = read("roastty/src/config/mod.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_grid,
        [
            ('@"adjust-cell-width": ?Metrics.Modifier', "Ghostty derived adjust-cell-width"),
            ('@"adjust-cell-height": ?Metrics.Modifier', "Ghostty derived adjust-cell-height"),
            (
                '@"adjust-font-baseline": ?Metrics.Modifier',
                "Ghostty derived adjust-font-baseline",
            ),
            (
                '@"adjust-underline-position": ?Metrics.Modifier',
                "Ghostty derived adjust-underline-position",
            ),
            (
                '@"adjust-underline-thickness": ?Metrics.Modifier',
                "Ghostty derived adjust-underline-thickness",
            ),
            (
                '@"adjust-strikethrough-position": ?Metrics.Modifier',
                "Ghostty derived adjust-strikethrough-position",
            ),
            (
                '@"adjust-strikethrough-thickness": ?Metrics.Modifier',
                "Ghostty derived adjust-strikethrough-thickness",
            ),
            (
                '@"adjust-overline-position": ?Metrics.Modifier',
                "Ghostty derived adjust-overline-position",
            ),
            (
                '@"adjust-overline-thickness": ?Metrics.Modifier',
                "Ghostty derived adjust-overline-thickness",
            ),
            (
                '@"adjust-cursor-thickness": ?Metrics.Modifier',
                "Ghostty derived adjust-cursor-thickness",
            ),
            (
                '@"adjust-cursor-height": ?Metrics.Modifier',
                "Ghostty derived adjust-cursor-height",
            ),
            ('@"adjust-box-thickness": ?Metrics.Modifier', "Ghostty derived adjust-box-thickness"),
            ('@"adjust-icon-height": ?Metrics.Modifier', "Ghostty derived adjust-icon-height"),
            ("metric_modifiers: Metrics.ModifierSet", "Ghostty key modifier field"),
            ('try set.put(alloc, .cell_width, m)', "Ghostty cell width mapping"),
            ('try set.put(alloc, .cell_height, m)', "Ghostty cell height mapping"),
            ('try set.put(alloc, .cell_baseline, m)', "Ghostty baseline mapping"),
            (
                'try set.put(alloc, .underline_position, m)',
                "Ghostty underline position mapping",
            ),
            (
                'try set.put(alloc, .underline_thickness, m)',
                "Ghostty underline thickness mapping",
            ),
            (
                'try set.put(alloc, .strikethrough_position, m)',
                "Ghostty strikethrough position mapping",
            ),
            (
                'try set.put(alloc, .strikethrough_thickness, m)',
                "Ghostty strikethrough thickness mapping",
            ),
            ('try set.put(alloc, .overline_position, m)', "Ghostty overline position mapping"),
            (
                'try set.put(alloc, .overline_thickness, m)',
                "Ghostty overline thickness mapping",
            ),
            (
                'try set.put(alloc, .cursor_thickness, m)',
                "Ghostty cursor thickness mapping",
            ),
            ('try set.put(alloc, .cursor_height, m)', "Ghostty cursor height mapping"),
            ('try set.put(alloc, .box_thickness, m)', "Ghostty box thickness mapping"),
            ('try set.put(alloc, .icon_height, m)', "Ghostty icon height mapping"),
            ("autoHash(hasher, self.metric_modifiers.count())", "Ghostty modifier hash count"),
            ("value.hash(hasher)", "Ghostty modifier hash value"),
        ],
    )
    require_all(
        ghostty_collection,
        [
            ("metric_modifiers: Metrics.ModifierSet", "Ghostty collection modifier field"),
            ("metrics.apply(self.metric_modifiers)", "Ghostty updateMetrics applies modifiers"),
        ],
    )
    require_all(
        ghostty_metrics,
        [
            ("pub fn apply(self: *Metrics, mods: ModifierSet) void", "Ghostty metrics apply"),
            ("inline .cell_width,", "Ghostty cell width branch"),
            (".cell_height,", "Ghostty cell-height branch"),
            ("if (comptime tag == .cell_height)", "Ghostty cell-height recenter branch"),
            ("inline .icon_height =>", "Ghostty icon-height fanout branch"),
        ],
    )

    require_all(
        roastty_config,
        [
            ("pub adjust_cell_width: Option<MetricModifier>", "Roastty adjust-cell-width config"),
            ("pub adjust_cell_height: Option<MetricModifier>", "Roastty adjust-cell-height config"),
            (
                "pub adjust_font_baseline: Option<MetricModifier>",
                "Roastty adjust-font-baseline config",
            ),
            (
                "pub adjust_underline_position: Option<MetricModifier>",
                "Roastty adjust-underline-position config",
            ),
            (
                "pub adjust_underline_thickness: Option<MetricModifier>",
                "Roastty adjust-underline-thickness config",
            ),
            (
                "pub adjust_strikethrough_position: Option<MetricModifier>",
                "Roastty adjust-strikethrough-position config",
            ),
            (
                "pub adjust_strikethrough_thickness: Option<MetricModifier>",
                "Roastty adjust-strikethrough-thickness config",
            ),
            (
                "pub adjust_overline_position: Option<MetricModifier>",
                "Roastty adjust-overline-position config",
            ),
            (
                "pub adjust_overline_thickness: Option<MetricModifier>",
                "Roastty adjust-overline-thickness config",
            ),
            (
                "pub adjust_cursor_thickness: Option<MetricModifier>",
                "Roastty adjust-cursor-thickness config",
            ),
            (
                "pub adjust_cursor_height: Option<MetricModifier>",
                "Roastty adjust-cursor-height config",
            ),
            ("pub adjust_box_thickness: Option<MetricModifier>", "Roastty adjust-box-thickness config"),
            ("pub adjust_icon_height: Option<MetricModifier>", "Roastty adjust-icon-height config"),
        ],
    )
    require_all(
        roastty_grid,
        [
            ("pub adjust_cell_width: Option<Modifier>", "Roastty derived adjust-cell-width"),
            ("pub adjust_cell_height: Option<Modifier>", "Roastty derived adjust-cell-height"),
            ("pub adjust_font_baseline: Option<Modifier>", "Roastty derived adjust-font-baseline"),
            (
                "pub adjust_underline_position: Option<Modifier>",
                "Roastty derived adjust-underline-position",
            ),
            (
                "pub adjust_underline_thickness: Option<Modifier>",
                "Roastty derived adjust-underline-thickness",
            ),
            (
                "pub adjust_strikethrough_position: Option<Modifier>",
                "Roastty derived adjust-strikethrough-position",
            ),
            (
                "pub adjust_strikethrough_thickness: Option<Modifier>",
                "Roastty derived adjust-strikethrough-thickness",
            ),
            (
                "pub adjust_overline_position: Option<Modifier>",
                "Roastty derived adjust-overline-position",
            ),
            (
                "pub adjust_overline_thickness: Option<Modifier>",
                "Roastty derived adjust-overline-thickness",
            ),
            (
                "pub adjust_cursor_thickness: Option<Modifier>",
                "Roastty derived adjust-cursor-thickness",
            ),
            ("pub adjust_cursor_height: Option<Modifier>", "Roastty derived adjust-cursor-height"),
            ("pub adjust_box_thickness: Option<Modifier>", "Roastty derived adjust-box-thickness"),
            ("pub adjust_icon_height: Option<Modifier>", "Roastty derived adjust-icon-height"),
            ("metric_modifiers: ModifierSet", "Roastty key modifier field"),
            ("metric_modifiers: metric_modifiers_from_config(config)", "Roastty key modifier source"),
            ("fn metric_modifiers_from_config", "Roastty modifier builder"),
            ("(MetricKey::CellWidth, config.adjust_cell_width)", "Roastty cell width mapping"),
            ("(MetricKey::CellHeight, config.adjust_cell_height)", "Roastty cell height mapping"),
            (
                "(MetricKey::CellBaseline, config.adjust_font_baseline)",
                "Roastty baseline mapping",
            ),
            (
                "MetricKey::UnderlinePosition",
                "Roastty underline position mapping",
            ),
            (
                "MetricKey::UnderlineThickness",
                "Roastty underline thickness mapping",
            ),
            (
                "MetricKey::StrikethroughPosition",
                "Roastty strikethrough position mapping",
            ),
            (
                "MetricKey::StrikethroughThickness",
                "Roastty strikethrough thickness mapping",
            ),
            ("MetricKey::OverlinePosition", "Roastty overline position mapping"),
            ("MetricKey::OverlineThickness", "Roastty overline thickness mapping"),
            ("MetricKey::CursorThickness", "Roastty cursor thickness mapping"),
            ("MetricKey::CursorHeight", "Roastty cursor height mapping"),
            ("MetricKey::BoxThickness", "Roastty box thickness mapping"),
            ("MetricKey::IconHeight", "Roastty icon height mapping"),
            ("collection.set_metric_modifiers", "Roastty collection install"),
            (
                "fn font_metric_modifier_runtime_key_maps_all_adjust_fields",
                "Roastty all-field mapping test",
            ),
            (
                "fn font_metric_modifier_runtime_key_hash_changes_with_modifiers",
                "Roastty key hash test",
            ),
            (
                "fn font_metric_modifier_runtime_build_grid_applies_config_modifiers",
                "Roastty build-grid modifier test",
            ),
            (
                "fn font_metric_modifier_runtime_build_grid_recenters_cell_height",
                "Roastty build-grid recenter test",
            ),
        ],
    )
    require_all(
        roastty_collection,
        [
            ("metric_modifiers: ModifierSet", "Roastty collection modifier field"),
            ("pub(crate) fn set_metric_modifiers", "Roastty collection modifier setter"),
            ("metrics.apply(&self.metric_modifiers)", "Roastty collection applies modifiers"),
            (
                "fn font_metric_modifier_runtime_empty_set_preserves_metrics",
                "Roastty empty-set test",
            ),
            (
                "fn font_metric_modifier_runtime_update_metrics_applies_modifiers",
                "Roastty update_metrics modifier test",
            ),
            (
                "fn font_metric_modifier_runtime_cell_height_recenters_metrics",
                "Roastty recenter test",
            ),
        ],
    )
    require_all(
        roastty_metrics,
        [
            ("pub(crate) fn apply(&mut self, mods: &ModifierSet)", "Roastty metrics apply"),
            ("Key::CellWidth | Key::CellHeight", "Roastty cell width/height branch"),
            ("Key::IconHeight =>", "Roastty icon-height fanout branch"),
            ("impl std::hash::Hash for Modifier", "Roastty modifier hash"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-007B2B2B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-007B2B2B2A status"),
            ("metric modifier", "RUNTIME-007B2B2B2A behavior"),
            ("font_metric_modifier_runtime", "RUNTIME-007B2B2B2A tests"),
            ("font_metric_modifier_runtime_parity.py", "RUNTIME-007B2B2B2A guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-007B2B2B2B2")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-007B2B2B2B2 status"),
            ("fallback/shaping visual output", "RUNTIME-007B2B2B2B2 fallback gap"),
            ("bitmap/color font thickening edge cases", "RUNTIME-007B2B2B2B2 bitmap gap"),
            ("glyph metrics as seen by the renderer beyond modifier math", "RUNTIME-007B2B2B2B2 glyph gap"),
            ("broader renderer-visible font pixel parity", "RUNTIME-007B2B2B2B2 pixel gap"),
        ],
    )

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

    print("font_metric_modifier_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
