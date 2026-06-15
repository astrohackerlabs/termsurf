#!/usr/bin/env python3
"""Guard font-variation runtime parity for Issue 805 CFG-223."""

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
    ghostty_discovery = read("vendor/ghostty/src/font/discovery.zig")
    ghostty_deferred = read("vendor/ghostty/src/font/DeferredFace.zig")
    ghostty_coretext = read("vendor/ghostty/src/font/face/coretext.zig")
    roastty_grid = read("roastty/src/font/shared_grid_set.rs")
    roastty_discovery = read("roastty/src/font/discovery.rs")
    roastty_deferred = read("roastty/src/font/deferred_face.rs")
    roastty_coretext = read("roastty/src/font/face/coretext.rs")
    roastty_config = read("roastty/src/config/mod.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_grid,
        [
            (
                '@"font-variation": configpkg.RepeatableFontVariation',
                "Ghostty regular variation derived config",
            ),
            (
                '@"font-variation-bold": configpkg.RepeatableFontVariation',
                "Ghostty bold variation derived config",
            ),
            (
                '@"font-variation-italic": configpkg.RepeatableFontVariation',
                "Ghostty italic variation derived config",
            ),
            (
                '@"font-variation-bold-italic": configpkg.RepeatableFontVariation',
                "Ghostty bold-italic variation derived config",
            ),
            (
                '.variations = config.@"font-variation".list.items',
                "Ghostty regular descriptor variation assignment",
            ),
            (
                '.variations = config.@"font-variation-bold".list.items',
                "Ghostty bold descriptor variation assignment",
            ),
            (
                '.variations = config.@"font-variation-italic".list.items',
                "Ghostty italic descriptor variation assignment",
            ),
            (
                '.variations = config.@"font-variation-bold-italic".list.items',
                "Ghostty bold-italic descriptor variation assignment",
            ),
            (
                "if (style != .regular and desc.variations.len > 0)",
                "Ghostty styled variation retry gate",
            ),
            ("copy.bold = false", "Ghostty styled retry clears bold"),
            ("copy.italic = false", "Ghostty styled retry clears italic"),
        ],
    )
    require_all(
        ghostty_discovery,
        [
            ("variations: []const Variation = &.{}", "Ghostty descriptor variations"),
            ("autoHash(hasher, self.variations.len)", "Ghostty variation hash length"),
            ("autoHash(hasher, variation.id)", "Ghostty variation hash id"),
            ("autoHash(hasher, @as(i64, @intFromFloat(variation.value)))", "Ghostty variation hash value"),
            ("copy.variations = try alloc.dupe(Variation, self.variations)", "Ghostty clone preserves variations"),
        ],
    )
    require_all(
        ghostty_deferred,
        [
            ("variations: []const font.face.Variation", "Ghostty deferred variations"),
            ("try face.setVariations(ct.variations, opts)", "Ghostty deferred CoreText apply"),
        ],
    )
    require_all(
        ghostty_coretext,
        [
            ("pub fn setVariations", "Ghostty CoreText set variations"),
            ("desc.createCopyWithVariation(id, v.value)", "Ghostty CoreText descriptor variation copy"),
            ("self.* = face", "Ghostty CoreText replaces varied face"),
        ],
    )

    require_all(
        roastty_config,
        [
            ("pub font_variation: RepeatableFontVariation", "Roastty regular config variation field"),
            ("pub font_variation_bold: RepeatableFontVariation", "Roastty bold config variation field"),
            ("pub font_variation_italic: RepeatableFontVariation", "Roastty italic config variation field"),
            ("pub font_variation_bold_italic: RepeatableFontVariation", "Roastty bold-italic config variation field"),
        ],
    )
    require_all(
        roastty_grid,
        [
            ("pub font_variation: config::RepeatableFontVariation", "Roastty derived regular variations"),
            ("pub font_variation_bold: config::RepeatableFontVariation", "Roastty derived bold variations"),
            ("pub font_variation_italic: config::RepeatableFontVariation", "Roastty derived italic variations"),
            ("pub font_variation_bold_italic: config::RepeatableFontVariation", "Roastty derived bold-italic variations"),
            ("font_variation: config.font_variation.clone()", "Roastty derived regular clone"),
            ("font_variation_bold: config.font_variation_bold.clone()", "Roastty derived bold clone"),
            ("font_variation_italic: config.font_variation_italic.clone()", "Roastty derived italic clone"),
            (
                "font_variation_bold_italic: config.font_variation_bold_italic.clone()",
                "Roastty derived bold-italic clone",
            ),
            ("&config.font_variation,", "Roastty regular descriptor source"),
            ("&config.font_variation_bold,", "Roastty bold descriptor source"),
            ("&config.font_variation_italic,", "Roastty italic descriptor source"),
            (
                "&config.font_variation_bold_italic,",
                "Roastty bold-italic descriptor source",
            ),
            ("fn discovery_variations", "Roastty config variation conversion"),
            ("id: Variation::id_from_tag(&v.id)", "Roastty variation id conversion"),
            ("variations: variations.clone()", "Roastty descriptor receives variations"),
            (
                "style != Style::Regular && !descriptor.variations.is_empty()",
                "Roastty styled variation retry gate",
            ),
            ("retry.bold = false", "Roastty styled retry clears bold"),
            ("retry.italic = false", "Roastty styled retry clears italic"),
            (
                "fn font_variation_runtime_key_maps_each_style_variations",
                "Roastty style mapping test",
            ),
            (
                "fn font_variation_runtime_key_hash_changes_with_variation_value",
                "Roastty key hash test",
            ),
            (
                "fn font_variation_runtime_key_preserves_style_offsets",
                "Roastty style offset test",
            ),
            (
                "fn font_variation_runtime_default_key_has_no_variations",
                "Roastty default no-variation test",
            ),
            (
                "fn font_variation_runtime_build_grid_with_configured_variations",
                "Roastty configured grid test",
            ),
        ],
    )
    require_all(
        roastty_discovery,
        [
            ("pub variations: Vec<Variation>", "Roastty descriptor variations"),
            ("v.id.hash(&mut h)", "Roastty variation id hash"),
            ("v.value.to_bits().hash(&mut h)", "Roastty variation value hash"),
        ],
    )
    require_all(
        roastty_deferred,
        [
            ("variations: Vec<Variation>", "Roastty deferred variations"),
            ("face.set_variations(&self.variations)", "Roastty deferred load applies variations"),
            (
                "fn deferred_face_load_applies_variations",
                "Roastty deferred variation test",
            ),
        ],
    )
    require_all(
        roastty_coretext,
        [
            ("pub(crate) fn set_variations", "Roastty CoreText set variations"),
            ("desc.copy_with_variation(&id, v.value)", "Roastty CoreText descriptor variation copy"),
            ("fn set_variations_runs_on_face", "Roastty CoreText variation test"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-007B2B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-007B2B2B1 status"),
            ("font-variation", "RUNTIME-007B2B2B1 behavior"),
            ("style-specific descriptors", "RUNTIME-007B2B2B1 style evidence"),
            ("font_variation_runtime_parity.py", "RUNTIME-007B2B2B1 guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-007B2B2B2B2")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-007B2B2B2B2 status"),
            ("fallback/shaping visual output", "RUNTIME-007B2B2B2B2 fallback gap"),
            ("bitmap/color font thickening edge cases", "RUNTIME-007B2B2B2B2 bitmap gap"),
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

    print("font_variation_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
