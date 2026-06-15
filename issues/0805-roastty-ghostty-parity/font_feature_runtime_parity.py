#!/usr/bin/env python3
"""Guard font-feature runtime parity for Issue 805 CFG-223."""

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
    ghostty_feature = read("vendor/ghostty/src/font/shaper/feature.zig")
    ghostty_coretext = read("vendor/ghostty/src/font/shaper/coretext.zig")
    roastty_config = read("roastty/src/config/mod.rs")
    roastty_shape = read("roastty/src/font/shape.rs")
    roastty_run = read("roastty/src/font/run.rs")
    roastty_cache = read("roastty/src/font/shaper_cache.rs")
    roastty_coretext = read("roastty/src/font/face/coretext.rs")
    roastty_rebuild = read("roastty/src/renderer/frame_rebuild.rs")
    roastty_frame = read("roastty/src/renderer/frame_renderer.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_renderer,
        [
            ("font_features: std.ArrayListUnmanaged", "Ghostty derived config feature field"),
            ("const font_features = try config.@\"font-feature\".clone(alloc)", "Ghostty feature clone"),
            (".font_features = font_features.list", "Ghostty derived feature assignment"),
            (".features = options.config.font_features.items", "Ghostty renderer init features"),
            (".features = config.font_features.items", "Ghostty config-change features"),
            ("self.font_shaper.deinit()", "Ghostty shaper replacement"),
            ("self.font_shaper = font_shaper", "Ghostty shaper install"),
            ("self.font_shaper_cache.deinit(self.alloc)", "Ghostty shaper cache reset"),
            ("self.font_shaper_cache = font_shaper_cache", "Ghostty shaper cache install"),
        ],
    )
    require_all(
        ghostty_shape,
        [
            ("pub const default_features", "Ghostty default feature list"),
        ],
    )
    require_all(
        ghostty_feature,
        [
            ("pub const default_features", "Ghostty shaper default feature list"),
            ('.{ .tag = "liga".*, .value = 1 }', "Ghostty default liga feature"),
        ],
    )
    require_all(
        ghostty_coretext,
        [
            ("fn makeFeaturesDict", "Ghostty CoreText feature dictionary"),
            ("kCTFontOpenTypeFeatureTag", "Ghostty CoreText feature tag key"),
            ("kCTFontOpenTypeFeatureValue", "Ghostty CoreText feature value key"),
        ],
    )

    require_all(
        roastty_config,
        [
            ("pub font_feature: RepeatableString", "Roastty font-feature config field"),
            ("\"font-feature\" => self.font_feature.parse_cli(value)?", "Roastty parser field"),
        ],
    )
    require_all(
        roastty_shape,
        [
            ("pub(crate) struct Options", "Roastty shape options"),
            ("pub features: Vec<String>", "Roastty feature strings"),
            ("pub(crate) fn merged_features", "Roastty merged features"),
            ("let mut out = default_features()", "Roastty defaults first"),
            ("out.extend(parse_features(s))", "Roastty user features appended"),
            ("pub(crate) fn cache_namespace", "Roastty feature cache namespace"),
            ("fn merged_features_defaults_then_user", "Roastty merge test"),
            (
                "fn options_cache_namespace_is_stable_and_distinct",
                "Roastty namespace test",
            ),
        ],
    )
    require_all(
        roastty_coretext,
        [
            ("pub(crate) fn shape_run_options", "Roastty CoreText options shaper"),
            ("options.merged_features()", "Roastty CoreText merged feature use"),
            ("fn feature_settings_descriptor", "Roastty feature descriptor"),
            ("kCTFontOpenTypeFeatureTag", "Roastty CoreText feature tag key"),
            ("kCTFontOpenTypeFeatureValue", "Roastty CoreText feature value key"),
            ("fn shape_run_options_regression", "Roastty default options regression"),
        ],
    )
    require_all(
        roastty_cache,
        [
            ("get_with_namespace", "Roastty feature-aware cache get"),
            ("put_with_namespace", "Roastty feature-aware cache put"),
            ("fn cache_key", "Roastty cache key combiner"),
            (
                "shaper_cache_feature_namespace_separates_same_run",
                "Roastty cache namespace test",
            ),
        ],
    )
    require_all(
        roastty_run,
        [
            ("pub(crate) fn shape_row_options", "Roastty options-aware row shaper"),
            (
                "pub(crate) fn shape_row_cached_options",
                "Roastty cached options-aware row shaper",
            ),
            ("shape_options.cache_namespace()", "Roastty row cache namespace"),
            ("face.shape_run_options", "Roastty row shaper applies options"),
            (
                "shape_row_options_default_matches_default_shape",
                "Roastty default row-shape test",
            ),
            (
                "font_feature_runtime_cached_rows_use_feature_namespace",
                "Roastty row cache feature test",
            ),
        ],
    )
    require_all(
        roastty_rebuild,
        [
            ("shape_options: &'a shape::Options", "row-format shape options field"),
            ("shape_row_cached_options", "row-format options-aware shaping"),
            ("input.shape_options", "row-format passes options"),
        ],
    )
    require_all(
        roastty_frame,
        [
            ("shape_options: shape::Options", "frame knob shape options"),
            ("features: config.font_feature.list.clone()", "frame config feature source"),
            ("shape_options: &knobs.shape_options", "frame input feature source"),
            (
                "font_feature_runtime_active_frame_sources_config",
                "active frame feature config test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-007B2B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-007B2B2A status"),
            ("font-feature", "RUNTIME-007B2B2A behavior"),
            ("feature-aware shaped-run cache", "RUNTIME-007B2B2A cache evidence"),
            ("font_feature_runtime_parity.py", "RUNTIME-007B2B2A guard"),
        ],
    )

    row_font_residual = require_row(runtime_inventory, "RUNTIME-007B2B2B2B2")
    require_all(
        row_font_residual,
        [
            ("Oracle complete", "RUNTIME-007B2B2B2B2 status"),
            ("font renderer residual output effects", "RUNTIME-007B2B2B2B2 behavior"),
            ("Experiment 184", "RUNTIME-007B2B2B2B2 evidence"),
            ("font_renderer_residual_parity.py", "RUNTIME-007B2B2B2B2 guard"),
        ],
    )
    if 'id="RUNTIME-007B2B2",' in read(
        "issues/0805-roastty-ghostty-parity/config_runtime_inventory.py"
    ):
        raise AssertionError("old broad RUNTIME-007B2B2 row is still present")

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

    print("font_feature_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
