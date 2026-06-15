#!/usr/bin/env python3
"""Inventory Roastty config runtime/UI effect coverage for Issue 805.

This is a bounded markdown/source inventory for CFG-223. It records config
effects that must be proven in the running app/terminal/renderer surface and
keeps broad runtime/UI parity honest by requiring row-level evidence.
"""

from __future__ import annotations

import argparse
import dataclasses
from collections import Counter
from pathlib import Path


@dataclasses.dataclass(frozen=True)
class RuntimeRow:
    id: str
    behavior: str
    ghostty_reference: str
    roastty_reference: str
    family: str
    status: str
    evidence: str
    missing_evidence: str
    guard_tier: str
    guard_command: str


ROWS = [
    RuntimeRow(
        id="RUNTIME-001",
        behavior="app-level clipboard read/write policy effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` clipboard access fields; `vendor/ghostty/src/Surface.zig` clipboard actions",
        roastty_reference="`roastty/src/lib.rs` clipboard callbacks and app config fields",
        family="clipboard",
        status="Oracle complete",
        evidence=(
            "Clipboard callback tests cover read/write allow/deny/ask policy "
            "dispatch through the runtime callbacks, and app/surface update "
            "tests prove clipboard policies refresh existing surfaces."
        ),
        missing_evidence="None for clipboard read/write policy runtime dispatch.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml clipboard_read && cargo test --manifest-path roastty/Cargo.toml clipboard_write`",
    ),
    RuntimeRow(
        id="RUNTIME-002",
        behavior="clipboard copy/paste transformation effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` clipboard transform fields; `vendor/ghostty/src/Surface.zig` copy/paste paths",
        roastty_reference="`roastty/src/lib.rs::copy_to_clipboard`, `clipboard_paste_is_unsafe`, paste helpers",
        family="clipboard",
        status="Oracle complete",
        evidence=(
            "Clipboard and paste tests cover bracketed paste safety, paste "
            "protection, codepoint-map copy transformation, trimming trailing "
            "spaces, and selection-clear-on-copy behavior."
        ),
        missing_evidence="None for covered clipboard transform runtime behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml clipboard && cargo test --manifest-path roastty/Cargo.toml paste`",
    ),
    RuntimeRow(
        id="RUNTIME-003",
        behavior="selection behavior effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` selection fields; `vendor/ghostty/src/Surface.zig` selection and copy paths",
        roastty_reference="`roastty/src/lib.rs` selection gesture, selection read, and selection clear paths",
        family="selection",
        status="Oracle complete",
        evidence=(
            "`app_and_surface_update_config_sync_selection_behavior` plus "
            "selection gesture/read tests prove selection clear-on-typing, "
            "selection word character boundaries, copy-on-select-adjacent "
            "selection state, and selection runtime update behavior."
        ),
        missing_evidence="None for covered selection config runtime behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml selection`",
    ),
    RuntimeRow(
        id="RUNTIME-004A",
        behavior="mouse-reporting config and toggle_mouse_reporting runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` mouse/click fields; `vendor/ghostty/src/Surface.zig` mouse handlers",
        roastty_reference="`roastty/src/lib.rs` `mouse_reporting`, `mouse_report_context`, `roastty_surface_mouse_captured`, and `toggle_mouse_reporting`",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`mouse_runtime_reporting_config_and_toggle_gate_capture` proves "
            "the configured `mouse-reporting` value gates terminal mouse "
            "capture, the `toggle_mouse_reporting` runtime action flips that "
            "gate, and surface config update refreshes the existing surface."
        ),
        missing_evidence="None for mouse-reporting config/toggle runtime behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml mouse_runtime_reporting_config_and_toggle_gate_capture`",
    ),
    RuntimeRow(
        id="RUNTIME-004B",
        behavior="mouse-shift-capture config and XTSHIFTESCAPE runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `mouse-shift-capture`; `vendor/ghostty/src/Surface.zig::mouseShiftCapture`",
        roastty_reference="`roastty/src/lib.rs::mouse_shift_capture`; `roastty/src/config/mod.rs::MouseShiftCapture::capture_shift`",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`mouse_runtime_shift_capture_uses_app_config_and_terminal_flag` "
            "proves surface mouse shift capture combines app config and the "
            "terminal XTSHIFTESCAPE flag, including `never` and `always` "
            "overrides."
        ),
        missing_evidence="None for mouse-shift-capture runtime decision behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml mouse_runtime_shift_capture_uses_app_config_and_terminal_flag`",
    ),
    RuntimeRow(
        id="RUNTIME-004C",
        behavior="mouse-scroll-multiplier runtime scroll step effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `mouse-scroll-multiplier`; `vendor/ghostty/src/Surface.zig::scrollCallback`",
        roastty_reference="`roastty/src/lib.rs::mouse_scroll_steps` and surface config update",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`mouse_runtime_scroll_multiplier_drives_precision_and_discrete_steps` "
            "proves precision and discrete scroll paths use configured "
            "multipliers and that surface config update refreshes the runtime "
            "scroll multiplier."
        ),
        missing_evidence="None for mouse-scroll-multiplier runtime step behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml mouse_runtime_scroll_multiplier_drives_precision_and_discrete_steps`",
    ),
    RuntimeRow(
        id="RUNTIME-004D",
        behavior="click-repeat-interval selection timing effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `click-repeat-interval`; `vendor/ghostty/src/Surface.zig` selection gesture press repeat",
        roastty_reference="`roastty/src/lib.rs::click_repeat_interval_ns` and `selection_press`",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`mouse_runtime_click_repeat_interval_drives_selection_timing` "
            "proves configured click-repeat timing controls whether repeated "
            "left clicks advance the selection gesture or restart it."
        ),
        missing_evidence="None for click-repeat-interval selection timing behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml mouse_runtime_click_repeat_interval_drives_selection_timing`",
    ),
    RuntimeRow(
        id="RUNTIME-004E",
        behavior="cursor-click-to-move runtime prompt movement effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `cursor-click-to-move`; `vendor/ghostty/src/Surface.zig` prompt click movement",
        roastty_reference="`roastty/src/lib.rs` mouse handlers and terminal prompt tracking; `roastty/src/terminal/terminal.rs` prompt click action",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`cursor_click_to_move_click_events_writes_sgr_mouse_press`, "
            "`cursor_click_to_move_line_mode_writes_cursor_keys`, and "
            "`cursor_click_to_move_line_mode_same_cell_consumes_release` "
            "prove eligible prompt clicks write Ghostty-style SGR click-event "
            "bytes or cursor-key movement bytes, including eligible no-op "
            "line clicks. `cursor_click_to_move_surface_gates_ineligible_clicks` "
            "proves disabled config, missing prompt-click support, active "
            "selection, dragged clicks, and clicks before the prompt are not "
            "handled."
        ),
        missing_evidence="None for cursor-click-to-move prompt click-event and line-movement runtime behavior.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml cursor_click_to_move`",
    ),
    RuntimeRow(
        id="RUNTIME-004F",
        behavior="mouse-hide-while-typing runtime cursor visibility effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `mouse-hide-while-typing`; `vendor/ghostty/src/Surface.zig` key input mouse hide/show paths",
        roastty_reference="`roastty/src/lib.rs` key input and macOS mouse shape/action callbacks",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`mouse_hide_while_typing_*` tests prove enabled text-key presses "
            "emit hidden once, key releases and empty-text presses do not hide, "
            "mouse position/button/scroll events emit visible when hidden, "
            "config update disables hiding and shows a hidden mouse, and "
            "unconsumed configured bindings still hide before encoded "
            "fallthrough input."
        ),
        missing_evidence="None for libroastty mouse-hide-while-typing runtime visibility actions.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml mouse_hide_while_typing`",
    ),
    RuntimeRow(
        id="RUNTIME-004G",
        behavior="right-click-action runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `right-click-action`; `vendor/ghostty/src/Surface.zig` right-click action dispatch",
        roastty_reference="`roastty/src/lib.rs` mouse button handlers and app runtime actions",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`right_click_action_*` tests prove right-button press honors "
            "`ignore`, `paste`, `copy`, `copy-or-paste`, and `context-menu`; "
            "clears or preserves selections according to pinned Ghostty "
            "semantics; refreshes existing surfaces on config update; and "
            "does not bypass terminal mouse reporting."
        ),
        missing_evidence="None for non-link right-click-action surface runtime behavior; link-specific context-menu behavior is tracked by RUNTIME-012B2B2B2B2B1.",
        guard_tier="Tier 3",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml right_click_action`",
    ),
    RuntimeRow(
        id="RUNTIME-004H",
        behavior="middle-click-action runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `middle-click-action`; `vendor/ghostty/src/Surface.zig` middle-click action dispatch",
        roastty_reference="`roastty/src/lib.rs` mouse button handlers, middle-click action state, and clipboard paste paths",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`middle_click_action_*` tests prove middle-button press honors "
            "`primary-paste` and `ignore`, chooses the standard or selection "
            "clipboard according to `copy-on-select` and selection clipboard "
            "support, refreshes existing surfaces on config update, and does "
            "not bypass terminal mouse reporting."
        ),
        missing_evidence="None for middle-click-action runtime dispatch behavior.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml middle_click_action`",
    ),
    RuntimeRow(
        id="RUNTIME-005",
        behavior="keyboard remap and keybind dispatch effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` key-remap/keybind fields; `vendor/ghostty/src/Surface.zig` key dispatch",
        roastty_reference="`roastty/src/lib.rs` key remap, keybind, and key table helpers",
        family="input",
        status="Oracle complete",
        evidence=(
            "Keybinding tests cover focused/global/all scope dispatch, key "
            "tables, key sequences, catch-all and unconsumed behavior; "
            "`surface_key_remap_*` tests prove remap affects binding detection, "
            "encoded input, and app/surface config updates."
        ),
        missing_evidence=(
            "None for key remap/keybind dispatch runtime behavior; "
            "command-palette UI dispatch remains tracked by `RUNTIME-011B2B`."
        ),
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_key_remap && cargo test --manifest-path roastty/Cargo.toml surface_key_table`",
    ),
    RuntimeRow(
        id="RUNTIME-006",
        behavior="color, palette, theme, and color-scheme runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` colors/palette/theme fields; `vendor/ghostty/src/Surface.zig` config change rendering paths",
        roastty_reference="`roastty/src/lib.rs::derived_config_palette`, color scheme reload tests, renderer state",
        family="color",
        status="Oracle complete",
        evidence=(
            "`surface_apply_config_updates_palette_defaults`, "
            "`surface_apply_config_updates_generated_palette_defaults`, and "
            "color-scheme reload tests prove palette/generated-palette defaults "
            "and conditional theme/color-scheme runtime updates."
        ),
        missing_evidence="None for covered color/palette/theme runtime behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_apply_config_updates_palette && cargo test --manifest-path roastty/Cargo.toml color_scheme`",
    ),
    RuntimeRow(
        id="RUNTIME-007A",
        behavior="config-derived font grid construction and initial live renderer font-grid wiring",
        ghostty_reference="`vendor/ghostty/src/Surface.zig` font grid setup, `updateConfig`, `setFontSize`, and font-size actions; `vendor/ghostty/src/font/SharedGridSet.zig` config-derived font grid keys",
        roastty_reference="`roastty/src/font/shared_grid_set.rs` config-derived key/grid tests; `roastty/src/lib.rs` font-size surface state and initial live renderer font-grid construction",
        family="font",
        status="Oracle complete",
        evidence=(
            "Experiment 132 splits out config-derived font grid runtime "
            "construction. `shared_grid_set_key_*` tests prove configured font "
            "families preserve descriptor order, exact `font-style*` names "
            "override category bold/italic matching, font size changes the "
            "grid key, and `font-codepoint-map` changes the key. "
            "`shared_grid_set_build_grid_*` tests prove default config builds "
            "a usable grid, codepoint-map overrides change resolved faces, and "
            "`font-synthetic-style` controls synthetic style completion. "
            "Experiment 105's `surface_reload_font_size_*` guard proves "
            "surface-state font-size startup/reload/manual/reset semantics. "
            "`font_grid_runtime_parity.py` statically checks pinned Ghostty's "
            "derived font config, font grid ref, config reload, setFontSize "
            "renderer message, and font-size action markers against Roastty's "
            "DerivedConfig, Key, build_grid_from_config, codepoint-map, "
            "synthetic-style, `build_live_renderer`, and font-size guards."
        ),
        missing_evidence="None for config-derived font grid construction, surface-state font-size semantics, and initial live renderer font-grid construction covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml shared_grid_set && cargo test --manifest-path roastty/Cargo.toml complete_styles && cargo test --manifest-path roastty/Cargo.toml codepoint_override && cargo test --manifest-path roastty/Cargo.toml surface_reload_font_size && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_grid_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-007B1",
        behavior="live renderer font-grid rebuild/update triggers after config reload and manual font-size changes",
        ghostty_reference="`vendor/ghostty/src/Surface.zig` `updateConfig`, `setFontSize`, `.font_grid` renderer message, and font-size action paths",
        roastty_reference="`roastty/src/lib.rs` live renderer invalidation, font-size actions, config reload, and `build_live_renderer` font grid construction",
        family="font",
        status="Oracle complete",
        evidence=(
            "Experiment 143 proves live renderer font-grid rebuild/update "
            "triggers without claiming visual glyph output. "
            "`font_live_grid_update_manual_size_changes_dirty_and_wake_live_view` "
            "proves manual increase/reset font-size actions dirty live-view "
            "surfaces and wake the app, while the implementation marker and "
            "static guard prove these changes invalidate the live renderer so "
            "the next present rebuilds the grid. "
            "`font_live_grid_update_same_size_is_idempotent` proves same-size "
            "updates avoid unnecessary dirty/wakeup/rebuild triggers. "
            "`font_live_grid_update_config_reload_preserves_state_and_rebuild_trigger` "
            "proves config reload preserves adjusted/unadjusted font-size "
            "semantics while still requesting a live renderer rebuild. "
            "`surface_reload_font_size_updates_unadjusted_and_preserves_manual` "
            "continues to guard the non-live surface-state rules from "
            "Experiment 105. `font_live_grid_update_runtime_parity.py` "
            "statically checks pinned Ghostty's `setFontSize`/`.font_grid` "
            "renderer message and font-size action markers against Roastty's "
            "font-size invalidation, initial live grid construction, tests, "
            "and inventory split."
        ),
        missing_evidence="None for live renderer font-grid rebuild/update triggers after config reload and manual font-size changes.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml font_live_grid_update && cargo test --manifest-path roastty/Cargo.toml surface_reload_font_size && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_live_grid_update_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-007B2A",
        behavior="font-shaping-break cursor-run break behavior through active frame row formatting",
        ghostty_reference="`vendor/ghostty/src/renderer/generic.zig` `DerivedConfig.font_shaping_break` and `run_iter_opts.applyBreakConfig`; `vendor/ghostty/src/font/shape.zig` `RunOptions.applyBreakConfig`",
        roastty_reference="`roastty/src/renderer/frame_rebuild.rs` row-format `FontShapingBreak` application; `roastty/src/renderer/frame_renderer.rs` `FrameRenderKnobs::from_config`; `roastty/src/font/run.rs` `RunOptions::apply_break_config`",
        family="font",
        status="Oracle complete",
        evidence=(
            "Experiment 145 wires `font-shaping-break` into active frame row "
            "formatting without mutating terminal-owned `shape_run_options`. "
            "`FrameRenderKnobs::from_config` sources `config.font_shaping_break`, "
            "`FrameRenderState::rebuild_input` passes it into row formatting, "
            "and `FrameRebuildPlan::format_rows` applies it to a row-local "
            "`RunOptions` clone immediately before shaping. "
            "`font_shaping_break_runtime_default_preserves_cursor_break` proves "
            "the default `cursor` setting keeps the viewport-derived cursor "
            "run break, while "
            "`font_shaping_break_runtime_no_cursor_removes_cursor_break` proves "
            "`no-cursor` clears it before shaping without mutating the snapshot "
            "row. `font_shaping_break_runtime_active_frame_sources_config` "
            "proves active frame render input sources the value from `Config`. "
            "`font_shaping_break_runtime_parity.py` statically checks pinned "
            "Ghostty's renderer-side markers against Roastty's wiring, tests, "
            "and inventory split."
        ),
        missing_evidence="None for deterministic font-shaping-break cursor-run break behavior through active frame row formatting.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml font_shaping_break_runtime && cargo test --manifest-path roastty/Cargo.toml apply_break_config_clears_cursor_x_when_off && cargo test --manifest-path roastty/Cargo.toml next_breaks_on_cursor && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_shaping_break_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-007B2B1",
        behavior="deterministic non-`sbix` font-thicken/font-thicken-strength renderer option propagation, glyph cache separation, and CoreText render mechanics",
        ghostty_reference="`vendor/ghostty/src/renderer/generic.zig` `font_thicken` and `font_thicken_strength`; `vendor/ghostty/src/font/SharedGrid.zig` packed glyph key fields; `vendor/ghostty/src/font/face/coretext.zig` non-`sbix` thicken padding and grayscale strength",
        roastty_reference="`roastty/src/renderer/frame_renderer.rs` frame render knobs and row-format input; `roastty/src/renderer/cell.rs` `RenderOptions`; `roastty/src/font/shared_grid.rs` glyph cache key; `roastty/src/font/face/coretext.rs` non-`sbix` thicken rendering",
        family="font",
        status="Oracle complete",
        evidence=(
            "Experiment 146 splits out deterministic non-`sbix` "
            "`font-thicken`/`font-thicken-strength` render mechanics without "
            "claiming full font pixel parity. "
            "`font_thicken_render_runtime_active_frame_sources_config` proves "
            "active frame row-format input receives config-derived thicken "
            "values. `render_options_plain_letter_has_no_constraint` proves "
            "row/cell render options pass `thicken` and `thicken_strength` to "
            "glyph rendering. `render_glyph_caches_by_key` proves the shared "
            "glyph cache separates plain, thickened, and different-strength "
            "renders of the same glyph. `render_glyph_thicken_pads_canvas` "
            "proves CoreText non-`sbix` thickening grows the canvas by one "
            "pixel on each edge, and `render_glyph_strength_dims_fill` proves "
            "lower strength dims grayscale fill. "
            "`font_thicken_render_runtime_parity.py` statically checks pinned "
            "Ghostty's derived config, glyph render options, packed glyph key, "
            "and CoreText thicken/strength markers against Roastty's wiring, "
            "tests, and inventory split."
        ),
        missing_evidence="None for deterministic non-`sbix` font-thicken/font-thicken-strength option propagation, glyph cache separation, and CoreText render mechanics.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml render_glyph_caches_by_key && cargo test --manifest-path roastty/Cargo.toml render_options_plain_letter_has_no_constraint && cargo test --manifest-path roastty/Cargo.toml render_glyph_thicken_pads_canvas && cargo test --manifest-path roastty/Cargo.toml render_glyph_strength_dims_fill && cargo test --manifest-path roastty/Cargo.toml font_thicken_render_runtime && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_thicken_render_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-007B2B2A",
        behavior="font-feature renderer option propagation, default-plus-user feature merging, CoreText shaping option application, and feature-aware shaped-run cache separation",
        ghostty_reference="`vendor/ghostty/src/renderer/generic.zig` `font_features` derived config, shaper init/changeConfig, and shaper cache reset; `vendor/ghostty/src/font/shape.zig` default features; `vendor/ghostty/src/font/face/coretext.zig` feature descriptor construction",
        roastty_reference="`roastty/src/renderer/frame_renderer.rs` shape options; `roastty/src/renderer/frame_rebuild.rs` row-format shaping options; `roastty/src/font/run.rs` options-aware row shaping; `roastty/src/font/shaper_cache.rs` feature-aware cache namespace; `roastty/src/font/face/coretext.rs` feature descriptors",
        family="font",
        status="Oracle complete",
        evidence=(
            "Experiment 147 wires `font-feature` into active renderer row "
            "shaping without claiming full visual font parity. "
            "`font_feature_runtime_active_frame_sources_config` proves "
            "`Config.font_feature.list` reaches active frame row-format "
            "`shape::Options`, and proves default `liga=1` precedes parsed "
            "user features such as `-liga` and `kern=2`. "
            "`shape_row_options_default_matches_default_shape` proves the "
            "default row-shaping path is preserved. "
            "`font_feature_runtime_cached_rows_use_feature_namespace` and "
            "`shaper_cache_feature_namespace_separates_same_run` prove shaped "
            "runs cannot be reused across different feature sets. "
            "`shape_run_options_regression`, `feature_settings_descriptor_*`, "
            "and `merged_features_defaults_then_user` continue to prove "
            "CoreText feature descriptor construction and default-plus-user "
            "feature merging. `font_feature_runtime_parity.py` statically "
            "checks pinned Ghostty's derived config, shaper recreation/cache "
            "reset, default feature, and CoreText feature markers against "
            "Roastty's row-shaping/cache wiring, tests, and inventory split."
        ),
        missing_evidence="None for deterministic font-feature option propagation, default-plus-user feature merging, CoreText feature descriptor application, and feature-aware shaped-run cache separation.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml font_feature_runtime && cargo test --manifest-path roastty/Cargo.toml merged_features_defaults_then_user && cargo test --manifest-path roastty/Cargo.toml shape_row_options_default_matches_default_shape && cargo test --manifest-path roastty/Cargo.toml shaper_cache_feature && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_feature_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-007B2B2B1",
        behavior="font-variation renderer descriptor propagation, key separation, and CoreText deferred-face application",
        ghostty_reference="`vendor/ghostty/src/font/SharedGridSet.zig` style-specific descriptor variation assignment and styled variation retry; `vendor/ghostty/src/font/discovery.zig` descriptor variation hashing/clone; `vendor/ghostty/src/font/DeferredFace.zig` deferred variation retention; `vendor/ghostty/src/font/face/coretext.zig` `setVariations`",
        roastty_reference="`roastty/src/font/shared_grid_set.rs` style-specific config variation descriptor wiring and retry; `roastty/src/font/discovery.rs` descriptor variation hashing; `roastty/src/font/deferred_face.rs` deferred variation retention; `roastty/src/font/face/coretext.rs` `set_variations`",
        family="font",
        status="Oracle complete",
        evidence=(
            "Experiment 149 wires the four parsed `font-variation*` lists "
            "into style-specific config-derived font descriptors without "
            "claiming variable-font pixel parity. "
            "`font_variation_runtime_key_maps_each_style_variations` proves "
            "regular, bold, italic, and bold-italic descriptors receive only "
            "their matching config variation list. "
            "`font_variation_runtime_key_hash_changes_with_variation_value` "
            "proves variation differences split config-derived font-grid "
            "keys. `font_variation_runtime_key_preserves_style_offsets` and "
            "`font_variation_runtime_default_key_has_no_variations` prove "
            "style slicing and default no-variation behavior remain stable. "
            "`font_variation_runtime_build_grid_with_configured_variations`, "
            "`deferred_face_load_applies_variations`, and "
            "`set_variations_runs_on_face` prove configured variations keep "
            "the grid/deferred CoreText face path usable through "
            "`set_variations`. `font_variation_runtime_parity.py` statically "
            "checks pinned Ghostty's style-specific descriptor variation "
            "assignment, styled variation retry, descriptor hashing/clone, "
            "deferred face retention, and CoreText `setVariations` markers "
            "against Roastty's wiring, tests, and inventory split."
        ),
        missing_evidence="None for deterministic `font-variation*` config propagation into style-specific descriptors, font-grid key separation, deferred face loading, and CoreText variation application mechanics.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml font_variation_runtime && cargo test --manifest-path roastty/Cargo.toml font_variation_config_parser_family_oracle && cargo test --manifest-path roastty/Cargo.toml font_variation_config_formatter_family_oracle && cargo test --manifest-path roastty/Cargo.toml deferred_face_load_applies_variations && cargo test --manifest-path roastty/Cargo.toml set_variations_runs_on_face && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_variation_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-007B2B2B2A",
        behavior="font metric modifier config propagation, key separation, and collection metric application",
        ghostty_reference="`vendor/ghostty/src/font/SharedGridSet.zig` metric modifier fields, key construction, and key hashing; `vendor/ghostty/src/font/Collection.zig` `updateMetrics`; `vendor/ghostty/src/font/Metrics.zig` `Metrics.apply`",
        roastty_reference="`roastty/src/font/shared_grid_set.rs` metric modifier derived config and key wiring; `roastty/src/font/collection.rs` `update_metrics`; `roastty/src/font/metrics.rs` `Metrics::apply`",
        family="font",
        status="Oracle complete",
        evidence=(
            "Experiment 150 wires all 13 parsed `adjust-*` metric modifier "
            "fields into the config-derived font grid key and collection "
            "metrics calculation without claiming glyph pixel parity. "
            "`font_metric_modifier_runtime_key_maps_all_adjust_fields` proves "
            "each canonical `adjust-*` field maps to the intended "
            "`font::metrics::Key`. "
            "`font_metric_modifier_runtime_key_hash_changes_with_modifiers` "
            "proves modifier differences split font-grid keys. "
            "`font_metric_modifier_runtime_empty_set_preserves_metrics`, "
            "`font_metric_modifier_runtime_update_metrics_applies_modifiers`, "
            "and `font_metric_modifier_runtime_cell_height_recenters_metrics` "
            "prove collection `update_metrics` applies configured modifiers "
            "through `Metrics::apply` while preserving empty-set defaults. "
            "`font_metric_modifier_runtime_build_grid_applies_config_modifiers` "
            "and `font_metric_modifier_runtime_build_grid_recenters_cell_height` "
            "prove `build_grid_from_config` returns modified grid metrics. "
            "`font_metric_modifier_runtime_parity.py` statically checks "
            "pinned Ghostty's derived config fields, modifier-set "
            "construction, key hashing, collection application, and Metrics "
            "apply markers against Roastty's wiring, tests, and inventory "
            "split."
        ),
        missing_evidence="None for deterministic `adjust-*` metric modifier propagation into font-grid key separation and collection metric calculation.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml font_metric_modifier_runtime && cargo test --manifest-path roastty/Cargo.toml metric_modifier_config_parser_family_oracle && cargo test --manifest-path roastty/Cargo.toml metric_modifier_config_formatter_family_oracle && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_metric_modifier_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-007B2B2B2B1",
        behavior="font fallback resolution, substitution, discovery caching, and present-glyph render-codepoint data",
        ghostty_reference="`vendor/ghostty/src/font/shaper/run.zig` fallback substitution; `vendor/ghostty/src/font/CodepointResolver.zig` fallback discovery; `vendor/ghostty/src/font/SharedGrid.zig` render-codepoint path",
        roastty_reference="`roastty/src/font/run.rs` fallback substitution; `roastty/src/font/face/coretext.rs` fallback discovery; `roastty/src/font/shared_grid.rs` fallback cache and render-codepoint tests",
        family="font",
        status="Oracle complete",
        evidence=(
            "Experiment 165 splits out deterministic font fallback runtime "
            "mechanics before full visual comparison. Roastty's run iterator "
            "tests prove unrenderable cells substitute a single `U+FFFD` "
            "codepoint and kitty placeholders substitute a space. CoreText "
            "`font_for_codepoint_*` tests prove Menlo-missing CJK and emoji "
            "codepoints resolve through fallback discovery while LastResort-only "
            "noncharacters are rejected. Shared-grid tests prove fallback "
            "discovery caches without duplicate faces, deferred discovered "
            "faces are preloaded before caching, present codepoints render via "
            "resolved glyph ids, and missing codepoints return `None`. "
            "`font_fallback_runtime_parity.py` statically checks pinned "
            "Ghostty fallback-chain, resolver, and shared-grid anchors against "
            "Roastty tests and this inventory split."
        ),
        missing_evidence="None for deterministic font fallback resolution, substitution, discovery caching, LastResort rejection, and present-glyph render-codepoint data. Broad fallback visual/pixel parity remains outside this slice.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml next_missing_codepoint_substitutes_replacement && cargo test --manifest-path roastty/Cargo.toml next_placeholder_is_space && cargo test --manifest-path roastty/Cargo.toml font_for_codepoint && cargo test --manifest-path roastty/Cargo.toml get_index_discovery_fallback && cargo test --manifest-path roastty/Cargo.toml get_index_deferred_discovery_preloads_face_before_caching && cargo test --manifest-path roastty/Cargo.toml render_codepoint && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_fallback_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-007B2B2B2B2",
        behavior="font renderer residual output effects: fallback/shaping visual output, bitmap/color font thickening edge cases, renderer-visible glyph metrics, and broader font pixel evidence",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` font feature, variation, thicken, metric, and shaping fields; `vendor/ghostty/src/font` shaping/rendering paths",
        roastty_reference="`roastty/src/font`; `roastty/src/renderer` font shaping, metrics, glyph output, and visual renderer behavior",
        family="font",
        status="Oracle complete",
        evidence=(
            "Experiment 105 proves surface-state font-size reload/manual/reset "
            "semantics, Experiment 132 split out config-derived font grid "
            "construction plus initial live renderer grid wiring, and "
            "Experiment 143 split out live renderer font-grid rebuild/update "
            "triggers after config reload and manual font-size changes. "
            "Experiment 145 split out deterministic `font-shaping-break` "
            "cursor-run break behavior through active frame row formatting. "
            "Experiment 146 split out deterministic non-`sbix` "
            "`font-thicken`/`font-thicken-strength` renderer option "
            "propagation, glyph cache separation, and CoreText render "
            "mechanics. Experiment 147 split out deterministic `font-feature` "
            "renderer option propagation, default-plus-user feature merging, "
            "CoreText shaping option application, and feature-aware shaped-run "
            "cache separation. Experiment 149 split out deterministic "
            "`font-variation*` style-specific descriptor propagation, "
            "font-grid key separation, deferred face loading, and CoreText "
            "variation application mechanics. Experiment 150 split out "
            "deterministic `adjust-*` metric modifier propagation, font-grid "
            "key separation, collection metric application, and modified "
            "grid metrics returned by `build_grid_from_config`. Experiment "
            "165 split out deterministic font fallback resolution, "
            "substitution, fallback discovery caching, LastResort rejection, "
            "and present-glyph render-codepoint data. Experiment 184 closes the "
            "residual font renderer row by binding existing and new Rust tests "
            "to a dedicated guard: CoreText grayscale glyph rasterization and "
            "atlas placement, stretched-cell glyph pixels and bearings, "
            "non-`sbix` thicken padding/strength, `sbix` bitmap-color "
            "thicken-padding suppression, Apple Color Emoji BGRA rendering and "
            "wrong-atlas rejection, CoreText fallback discovery for CJK and "
            "supplementary emoji with LastResort rejection, shaping clusters "
            "for ASCII, RTL, supplementary scalars, and combining marks, and "
            "renderer-facing font-grid metric propagation."
        ),
        missing_evidence="None for covered font renderer residual output effects. GUI screenshot parity remains owned by the macOS app walkthrough rows, not this font renderer row.",
        guard_tier="Tier 3",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml font -- --test-threads=1 && cargo test --manifest-path roastty/Cargo.toml render_glyph -- --test-threads=1 && cargo test --manifest-path roastty/Cargo.toml shape_ -- --test-threads=1 && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_renderer_residual_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008A",
        behavior="renderer scheduler, cursor blink, focus, occlusion, and live renderer rebuild control runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `window-vsync` and `cursor-style-blink`; `vendor/ghostty/src/renderer/generic.zig` vsync renderer config; `vendor/ghostty/src/Surface.zig` cursor blink and renderer update paths",
        roastty_reference="`roastty/src/lib.rs` present driver, cursor blink helpers, live renderer visibility, focus, occlusion, and config update paths",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 125 split out the proven renderer control slice. "
            "`present_driver_*` tests prove `window-vsync` drives fallback or "
            "display-link present scheduling for new live surfaces, display id "
            "updates reach active display links, and present drivers stop "
            "before surface drop. `live_cursor_blink_*` tests prove cursor "
            "blink ticks, output reset throttling, terminal-output-only reset, "
            "focus-loss pause, and focus-gain reset behavior. "
            "`live_renderer_options_*` tests prove occlusion gates live "
            "presentation, live config updates request a renderer rebuild, "
            "and ABI-only surfaces stay quiet. "
            "`renderer_control_runtime_parity.py` statically checks pinned "
            "Ghostty's `window-vsync`, `cursor-style-blink`, "
            "`renderer/generic.zig` vsync config, and surface renderer "
            "control markers against Roastty's runtime/test markers."
        ),
        missing_evidence="None for renderer scheduler, cursor blink, focus, occlusion, and live renderer rebuild control runtime behavior.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml present_driver && cargo test --manifest-path roastty/Cargo.toml live_cursor_blink && cargo test --manifest-path roastty/Cargo.toml live_renderer_options && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/renderer_control_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B1",
        behavior="deterministic render knob sourcing, opacity conversion/clamping, background-opacity-cells, window-padding-color padding decisions, and font-thicken knob sourcing",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` renderer/window visual fields; `vendor/ghostty/src/renderer/generic.zig` derived renderer config, background-opacity clamp, padding-color decisions, and glyph render thicken options; `vendor/ghostty/src/renderer/cell.zig` padding extension helper",
        roastty_reference="`roastty/src/renderer/frame_renderer.rs` `FrameRenderKnobs::from_config`; `roastty/src/renderer/cell.rs` background opacity cell rebuild behavior; `roastty/src/renderer/frame_rebuild.rs` padding extension refinement",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 133 split out the deterministic renderer-knob slice. "
            "`from_config_sources_config_values` proves renderer knob sourcing "
            "for `font-thicken`, `font-thicken-strength`, "
            "`background-opacity`, `bold-color`, and selection colors. "
            "`background_opacity_clamps_for_renderer_knob`, "
            "`from_config_sources_opacity_options`, and "
            "`cursor_opacity_clamps_to_cursor_overlay_alpha_only` prove "
            "background/faint/cursor opacity conversion and renderer-use "
            "clamping. `rebuild_bg_row_background_opacity_cells`, "
            "`rebuild_bg_row_opacity_cells_off_is_unchanged`, and "
            "`rebuild_bg_row_opacity_cells_skips_covering_derived` prove "
            "background-opacity-cells behavior. `refine_padding_extend_rows_*` "
            "tests prove deterministic window-padding-color padding-extension "
            "decisions. `renderer_knobs_runtime_parity.py` statically checks "
            "the pinned Ghostty renderer markers against Roastty's "
            "implementation, tests, and this inventory split."
        ),
        missing_evidence="None for deterministic renderer knob sourcing, opacity conversion/clamping, background-opacity-cells behavior, window-padding-color padding-extension decisions, and font-thicken knob sourcing.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml from_config_sources_config_values && cargo test --manifest-path roastty/Cargo.toml background_opacity_clamps_for_renderer_knob && cargo test --manifest-path roastty/Cargo.toml from_config_sources_opacity_options && cargo test --manifest-path roastty/Cargo.toml cursor_opacity_clamps_to_cursor_overlay_alpha_only && cargo test --manifest-path roastty/Cargo.toml rebuild_bg_row_background_opacity_cells && cargo test --manifest-path roastty/Cargo.toml refine_padding_extend_rows && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/renderer_knobs_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B2A",
        behavior="deterministic active cursor overlay/uniform branches, cursor color/text color resolution, selected cursor sprite/glyph render data, wide cursor render data, lock fallback rendering, and cursor list routing",
        ghostty_reference="`vendor/ghostty/src/renderer/generic.zig` cursor style, color, opacity, sprite, lock, and vertex render paths; `vendor/ghostty/src/renderer/cell.zig` cursor list routing",
        roastty_reference="`roastty/src/renderer/frame_renderer.rs` active cursor render state; `roastty/src/renderer/cell.rs` cursor color, render-data, wide, lock fallback, and list-routing helpers",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 134 split out the deterministic cursor renderer data "
            "slice without claiming GUI cursor pixels or full cursor-style "
            "priority. `render_state_derives_visible_block_cursor_overlay`, "
            "`render_state_cursor_color_comes_from_osc12`, "
            "`render_state_cursor_colors_come_from_config`, "
            "`render_state_block_sets_uniform_underline_does_not`, and "
            "`cursor_blink_render_state_*` tests prove active non-password/"
            "non-preedit cursor overlay/uniform branches, focus/blink "
            "visibility, hollow unfocused cursors, OSC 12 cursor color, "
            "config-derived cursor color/text color, and block-uniform versus "
            "overlay cursor routing. "
            "`cursor_text_color_resolves_the_cursor_text_config` and "
            "`cursor_color_resolves_with_precedence` prove cursor text and "
            "cursor color resolution. `add_cursor_maps_styles_and_routes`, "
            "`add_cursor_wide_uses_two_cells`, and "
            "`add_cursor_lock_falls_back_when_glyph_absent` prove selected "
            "cursor sprite/glyph render data, wide cursor render data, and "
            "lock fallback rendering after lock style selection. "
            "`block_cursor_pos_adjusts_for_wide_kind` and `set_cursor_*` "
            "tests prove wide-tail cursor placement and cursor list routing. "
            "`cursor_renderer_runtime_parity.py` statically checks pinned "
            "Ghostty's cursor render markers against Roastty's tests."
        ),
        missing_evidence="None for deterministic active cursor overlay/uniform branches, cursor color/text color resolution, selected cursor render data, wide cursor render data, lock fallback rendering after lock style selection, and cursor list routing.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml render_state_derives_visible_block_cursor_overlay && cargo test --manifest-path roastty/Cargo.toml render_state_cursor_color_comes_from_osc12 && cargo test --manifest-path roastty/Cargo.toml render_state_cursor_colors_come_from_config && cargo test --manifest-path roastty/Cargo.toml render_state_block_sets_uniform_underline_does_not && cargo test --manifest-path roastty/Cargo.toml cursor_blink_render_state && cargo test --manifest-path roastty/Cargo.toml add_cursor && cargo test --manifest-path roastty/Cargo.toml cursor_text_color_resolves_the_cursor_text_config && cargo test --manifest-path roastty/Cargo.toml cursor_color_resolves_with_precedence && cargo test --manifest-path roastty/Cargo.toml block_cursor_pos_adjusts_for_wide_kind && cargo test --manifest-path roastty/Cargo.toml set_cursor && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/cursor_renderer_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B2B1",
        behavior="password/preedit cursor-style priority through the active frame renderer path",
        ghostty_reference="`vendor/ghostty/src/renderer/cursor.zig` preedit/password cursor priority; `vendor/ghostty/src/renderer/generic.zig` cursor state consumption",
        roastty_reference="`roastty/src/renderer/cursor.rs` shared cursor priority helper; `roastty/src/renderer/frame_renderer.rs` active frame cursor state construction and preedit-derived cursor options",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 144 wires the active frame renderer through the "
            "shared Ghostty-port `renderer::cursor::style` priority helper. "
            "`FrameCursorOptions::with_preedit(preedit.is_some())` is applied "
            "on active frame render paths before cursor state derivation. "
            "`cursor_priority_active_renderer_*` tests prove preedit forces "
            "a block cursor ahead of hidden cursor, focus, blink, and "
            "password state; password input forces the lock cursor ahead of "
            "hidden cursor and blink; preedit beats password; and no viewport "
            "still suppresses both priority states. "
            "`cursor_priority_active_renderer_render_frame_uses_real_preedit_argument` "
            "drives a real active `render_frame` call with `Some(Preedit)` "
            "and a hidden terminal cursor. "
            "`cursor_priority_runtime_parity.py` statically checks pinned "
            "Ghostty priority markers, Roastty's shared helper, the active "
            "frame preedit wiring, focused tests, and this inventory split."
        ),
        missing_evidence="None for password/preedit cursor-style priority through the active frame renderer path. Actual shell password-prompt detection and GUI cursor pixels remain outside this slice.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml cursor_priority_active_renderer && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/cursor_priority_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B2B2A",
        behavior="window-padding-x/window-padding-y scaling, window-padding-balance layout math, and active live renderer padded Size/grid wiring",
        ghostty_reference="`vendor/ghostty/src/Surface.zig` DerivedConfig window padding fields, `scaledPadding`, init/resize/content-scale padding application; `vendor/ghostty/src/renderer/size.zig` padding balance and grid math",
        roastty_reference="`roastty/src/renderer/size.rs` config-derived renderer Size helper; `roastty/src/lib.rs` live surface padded size/grid state, PTY sizing, and `FrameRenderer::update_screen` wiring",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 148 splits out deterministic window padding layout "
            "runtime behavior without claiming screenshot-level padding pixel "
            "parity. `window_padding_layout_runtime_*` tests prove "
            "config-derived `window-padding-x`/`window-padding-y` conversion "
            "uses Ghostty's `floor(points * dpi / 72)` rule, preserves "
            "independent X/Y scale for left/right versus top/bottom padding, "
            "applies `window-padding-balance = true` and `equal` with the "
            "ported `Size::balance_padding` math, computes grid size from "
            "`screen - padding`, updates content-scale dependent unbalanced "
            "padding, and feeds padded rows/columns into surface PTY sizing. "
            "`window_padding_layout_runtime_parity.py` statically checks "
            "pinned Ghostty's derived config, scaling, init/resize/"
            "content-scale markers, Roastty's helper/wiring/tests, and this "
            "inventory split."
        ),
        missing_evidence="None for deterministic window-padding scaling, balance layout math, active renderer Size/grid wiring, and padded PTY row/column state. Screenshot-level padding pixel parity remains outside this slice.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml window_padding_layout_runtime && cargo test --manifest-path roastty/Cargo.toml size_balance_padding && cargo test --manifest-path roastty/Cargo.toml size_grid_and_terminal && cargo test --manifest-path roastty/Cargo.toml coordinate_conversion && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/window_padding_layout_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B2B2B1",
        behavior="macOS glass background blur and opacity host behavior",
        ghostty_reference="`vendor/ghostty/macos/Sources/Features/Terminal/TerminalViewContainer.swift` `backgroundBlur`, `backgroundOpacity`, `NSGlassEffectView`, tint overlay, corner radius, and safe-area inset handling",
        roastty_reference="`roastty/macos/Sources/Features/Terminal/TerminalViewContainer.swift` copied glass host after Ghostty-to-Roastty renames",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 151 splits out the macOS glass blur/opacity host "
            "slice. `macos_glass_visual_runtime_parity.py` proves "
            "`TerminalViewContainer.swift` is identical after expected "
            "Ghostty-to-Roastty product/type renames and asserts the "
            "`NSGlassEffectView`, `macosGlassRegular`, `macosGlassClear`, "
            "`backgroundOpacity`, `withAlphaComponent`, corner-radius, "
            "inactive tint overlay, and safe-area top-inset markers that "
            "implement pinned Ghostty's glass host behavior."
        ),
        missing_evidence="None for macOS glass background blur and opacity host behavior. Non-glass compositor opacity remains tracked by RUNTIME-008B2B2B2A; screenshot-level visual proof remains outside this slice.",
        guard_tier="Tier 0",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_glass_visual_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B2B2B2A",
        behavior="copied macOS non-glass compositor opacity host behavior",
        ghostty_reference="`vendor/ghostty/macos/Sources/Features/Terminal/Window Styles/TerminalWindow.swift`; `vendor/ghostty/macos/Sources/Features/Terminal/Window Styles/TransparentTitlebarTerminalWindow.swift`; `vendor/ghostty/macos/Sources/Features/QuickTerminal/QuickTerminalController.swift`",
        roastty_reference="`roastty/macos/Sources/Features/Terminal/Window Styles/TerminalWindow.swift`; `roastty/macos/Sources/Features/Terminal/Window Styles/TransparentTitlebarTerminalWindow.swift`; `roastty/macos/Sources/Features/QuickTerminal/QuickTerminalController.swift`",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 154 splits out copied macOS non-glass compositor "
            "opacity host behavior. `non_glass_opacity_runtime_parity.py` "
            "proves `TerminalWindow.swift`, "
            "`TransparentTitlebarTerminalWindow.swift`, and "
            "`QuickTerminalController.swift` match pinned Ghostty after "
            "expected product/type renames, and asserts the "
            "`backgroundOpacity`, `backgroundBlur.isGlassStyle`, "
            "`isBackgroundOpaque`, fullscreen suppression, 0.001 white "
            "background workaround, preferred background alpha clamping, "
            "titlebar preferred-color forwarding, and non-glass "
            "window-background-blur ABI markers."
        ),
        missing_evidence="None for copied macOS non-glass compositor opacity host behavior. Screenshot-level opacity pixel proof remains outside this source-level slice.",
        guard_tier="Tier 0",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/non_glass_opacity_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B2B2B2B1",
        behavior="Metal custom shader output readback",
        ghostty_reference="`vendor/ghostty/src/renderer/generic.zig` custom shader state, offscreen frame target, post-process pipeline loop, uniform sync, ping-pong textures, and final target draw; `vendor/ghostty/src/renderer/metal/shaders.zig` post-pipeline init; `vendor/ghostty/src/renderer/Metal.zig` custom shader target",
        roastty_reference="`roastty/src/renderer/metal/compositor.rs` custom shader draw path and readback tests; `roastty/src/renderer/metal/render_pass.rs` `draw_custom_shader`; `roastty/src/renderer/metal/shaders.rs` post-process pipeline build; `roastty/src/renderer/metal/texture.rs` post-process texture options",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 163 splits out deterministic Metal custom shader "
            "output. Roastty renders the normal frame into an offscreen "
            "texture when custom shader pipelines are active, syncs "
            "`CustomShaderUniforms`, runs post-process pipelines in order, "
            "ping-pongs intermediate textures, and writes the final pass to "
            "the target. `custom_shader_output_requires_metal_device` makes "
            "this proof non-vacuous on the current macOS VM. "
            "`compositor_custom_shader_samples_offscreen_frame_into_final_target` "
            "proves a single custom shader samples the offscreen terminal "
            "frame and changes the final target bytes. "
            "`compositor_custom_shader_ping_pongs_multiple_passes` proves "
            "ordered multi-pass ping-pong output. "
            "`compositor_custom_shader_resizes_intermediate_textures` proves "
            "intermediate textures track target size, "
            "`compositor_custom_shader_uses_shadertoy_sampler_options` proves "
            "linear clamp-to-edge sampling, and "
            "`compositor_image_aware_frame_can_be_custom_shader_source` proves "
            "the image-aware frame path can feed the custom shader source. "
            "`custom_shader_output_runtime_parity.py` statically checks "
            "pinned Ghostty anchors, Roastty implementation markers, tests, "
            "and this inventory split."
        ),
        missing_evidence="None for deterministic Metal custom shader output readback. Native app screenshot equivalence and broader GUI/pixel proof remain outside this slice.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml custom_shader_output_requires_metal_device -- --test-threads=1 && cargo test --manifest-path roastty/Cargo.toml compositor_custom_shader -- --test-threads=1 && cargo test --manifest-path roastty/Cargo.toml compositor_image_aware_frame_can_be_custom_shader_source -- --test-threads=1 && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/custom_shader_output_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B2B2B2B2A",
        behavior="Metal text shader cursor pixel readback",
        ghostty_reference="`vendor/ghostty/src/renderer/shaders/shaders.metal` cursor text shader branch and cursor uniforms",
        roastty_reference="`roastty/src/renderer/metal/shaders.metal` cursor text shader branch; `roastty/src/renderer/metal/render_pass.rs` cursor readback tests",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 164 splits out deterministic Metal text shader cursor "
            "pixel readback. Pinned Ghostty and Roastty both use the "
            "`cursor_pos`/`cursor_wide` shader branch to replace non-cursor "
            "glyph color with `uniforms.cursor_color` while preserving "
            "`IS_CURSOR_GLYPH` vertex color. Roastty's Metal target-byte "
            "readback tests prove cursor-position recolor, cursor-glyph color "
            "preservation, wide-cursor second-cell recolor, and non-wide "
            "second-cell non-recolor. `metal_cursor_pixel_runtime_parity.py` "
            "statically checks the pinned Ghostty shader anchors, Roastty "
            "shader anchors, required readback tests, and this inventory split."
        ),
        missing_evidence="None for deterministic Metal text shader cursor pixel readback. Actual app/GUI cursor screenshots and broader GUI/pixel proof remain outside this slice.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml cell_text_cursor -- --test-threads=1 && cargo test --manifest-path roastty/Cargo.toml cell_text_wide_cursor_overrides_second_cell -- --test-threads=1 && cargo test --manifest-path roastty/Cargo.toml cell_text_non_wide_cursor_does_not_override_second_cell -- --test-threads=1 && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/metal_cursor_pixel_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B2B2B2B2B4",
        behavior="scroll-to-bottom output renderer-time viewport behavior",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `scroll-to-bottom`; `vendor/ghostty/src/renderer/generic.zig` synchronized-output skip, bottom-right marker tracking, and `scrollViewport(.bottom)` render-time behavior",
        roastty_reference="`roastty/src/lib.rs` `scroll_to_bottom_on_output_before_present` and `scroll_to_bottom_output_*` tests; `roastty/src/terminal/terminal.rs` synchronized-output and active screen bottom-right accessors",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 183 ports pinned Ghostty's `scroll-to-bottom.output` "
            "render-time behavior. `Surface::present_live` calls "
            "`scroll_to_bottom_on_output_before_present` before frame rendering, "
            "matching Ghostty's pre-render state update ordering. The helper "
            "does nothing when synchronized output mode is active, when the "
            "config output flag is false, when the bottom marker is unavailable, "
            "or when the active/screen bottom marker's node pointer and `y` "
            "match the stored marker. When the marker changes, it stores the "
            "new marker and scrolls the terminal viewport to bottom. Focused "
            "`scroll_to_bottom_output_*` tests prove the disabled config path "
            "preserves a history viewport, the enabled path scrolls once per "
            "bottom marker, repeated renders without output do not scroll, and "
            "synchronized output skips both scrolling and marker advancement "
            "until the mode is disabled. "
            "`scroll_to_bottom_output_runtime_parity.py` statically checks the "
            "pinned Ghostty anchors, Roastty implementation, focused tests, and "
            "this inventory split."
        ),
        missing_evidence="None for deterministic `scroll-to-bottom.output` renderer-time viewport behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml scroll_to_bottom_output -- --test-threads=1 && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/scroll_to_bottom_output_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B2B2B2B2B1",
        behavior="custom-shader-animation focus/always/false draw-timer policy",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `custom-shader-animation`; `vendor/ghostty/src/renderer/Thread.zig::syncDrawTimer`",
        roastty_reference="`roastty/src/config/mod.rs::CustomShaderAnimation`; `roastty/src/lib.rs` present tick custom shader animation gate",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 180 wires Roastty's live present tick to Ghostty's "
            "`custom-shader-animation` policy. `custom_shader_animation_tick_enabled` "
            "requires active custom shader pipelines and applies the parsed "
            "`always`/focused `true`/`false` policy through "
            "`CustomShaderAnimation::should_animate`. The present tick now "
            "renders an otherwise-clean frame when that policy says an active "
            "custom shader should animate, while preserving dirty-frame and "
            "live-visibility gates. Focused Rust tests cover all three policy "
            "values, no-pipeline behavior, and the dirty-or-animated present "
            "decision. `custom_shader_animation_runtime_parity.py` statically "
            "checks pinned Ghostty `syncDrawTimer`, Roastty implementation "
            "markers, tests, and this inventory split."
        ),
        missing_evidence="None for custom-shader-animation focus/always/false draw-timer policy. `scroll-to-bottom.output` remains tracked by RUNTIME-008B2B2B2B2B4; background image renderer behavior is tracked by RUNTIME-008B2B2B2B2B2 and color uniform behavior is tracked by RUNTIME-008B2B2B2B2B3.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml custom_shader_animation -- --test-threads=1 && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/custom_shader_animation_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B2B2B2B2B2",
        behavior="background image renderer runtime behavior",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `background-image*` fields; `vendor/ghostty/src/renderer/generic.zig` background image derived config, preparation, config-change, buffer, and draw paths",
        roastty_reference="`roastty/src/renderer/image.rs` background image state/config; `roastty/src/renderer/metal/render_pass.rs` background image draw/readback tests; `roastty/src/renderer/frame_renderer.rs` live background image frame test; `roastty/src/lib.rs` live renderer background image state",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 181 splits out deterministic background image renderer "
            "runtime behavior. `BackgroundImageConfig::from_config` sources "
            "`background-image`, `background-image-opacity`, "
            "`background-image-position`, `background-image-fit`, and "
            "`background-image-repeat` from parsed config and packs Ghostty's "
            "background image vertex semantics. `BackgroundImageState` loads, "
            "decodes, uploads, reuses, replaces, resets, and unloads config "
            "images while preserving an existing ready image on failed "
            "replacement. Metal render-pass tests prove the background image "
            "pipeline draws texture pixels over the configured background, "
            "`none` fit uses texture size for placement, and zero-instance "
            "draws do not bind or render. The live frame renderer test proves "
            "a config path reaches a rendered frame and reset config returns "
            "to the plain background. `background_image_runtime_parity.py` "
            "statically checks the pinned Ghostty derived-config, preparation, "
            "config-change, buffer, and draw anchors against Roastty's "
            "implementation, tests, and this inventory split."
        ),
        missing_evidence="None for deterministic background image renderer runtime behavior. `scroll-to-bottom.output` remains tracked by RUNTIME-008B2B2B2B2B4; color uniform behavior is tracked by RUNTIME-008B2B2B2B2B3.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml background_image -- --test-threads=1 && cargo test --manifest-path roastty/Cargo.toml bg_image -- --test-threads=1 && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/background_image_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B2B2B2B2B3",
        behavior="colorspace and alpha-blending Metal uniform runtime behavior",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `window-colorspace` and `alpha-blending`; `vendor/ghostty/src/renderer/generic.zig` color uniform bool setup; `vendor/ghostty/src/renderer/shaders/shaders.metal` color conversion and blending branches",
        roastty_reference="`roastty/src/renderer/metal/shaders.rs` Metal uniform bool sourcing and tests; `roastty/src/renderer/metal/shaders.metal` color conversion and blending branches",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 182 splits out deterministic `window-colorspace` and "
            "`alpha-blending` Metal uniform behavior. `MetalUniforms::from_config` "
            "sources parsed config into constructor-time uniforms, "
            "`MetalUniforms::new` initializes the color bools through "
            "`update_color_config`, and `update_color_config` maps sRGB/native "
            "to all-false, Display P3/linear to `use_display_p3` plus "
            "`use_linear_blending`, and Display P3/linear-corrected to all "
            "three color bools. Layout tests prove the bool fields match the "
            "Metal shader struct, and the copied Metal shader source consumes "
            "`use_display_p3`, `use_linear_blending`, and "
            "`use_linear_correction` in the same color conversion and blending "
            "branches as pinned Ghostty. `color_uniform_runtime_parity.py` "
            "statically checks the pinned Ghostty config, renderer, shader, "
            "Roastty implementation, tests, and this inventory split."
        ),
        missing_evidence="None for deterministic colorspace and alpha-blending Metal uniform behavior. `scroll-to-bottom.output` remains tracked by RUNTIME-008B2B2B2B2B4.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml update_color_config -- --test-threads=1 && cargo test --manifest-path roastty/Cargo.toml uniforms_new -- --test-threads=1 && cargo test --manifest-path roastty/Cargo.toml uniforms_from_config_sources_config_values -- --test-threads=1 && cargo test --manifest-path roastty/Cargo.toml metal_uniform_layout_matches_standard_shader_struct -- --test-threads=1 && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/color_uniform_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B2B2B2B2C",
        behavior="focused live window-padding pixel proof",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `window-padding-x`, `window-padding-y`, `window-padding-color`; `vendor/ghostty/src/renderer/shaders/shaders.metal` padding/background draw paths",
        roastty_reference="live debug `Roastty.app`; `roastty/src/renderer/metal/shaders.metal`; `macos_window_padding_pixel_runtime.py`",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 177 adds `macos_window_padding_pixel_runtime.py`, a live "
            "debug-app guard that launches Roastty with isolated config, large "
            "asymmetric `window-padding-x` and `window-padding-y`, "
            "`window-padding-color = background`, hidden titlebar, and a "
            "deterministic child process that writes a marker file before "
            "painting the terminal grid bright yellow. The guard proves the "
            "frontmost process Unix PID is the exact debug-app PID, maps the "
            "focused accessibility window bounds to the captured PID-owned "
            "layer-0 CGWindowID, captures that exact window with "
            "`screencapture -l`, detects the bright terminal-content bounds, "
            "and requires geometry-derived sample rectangles to show "
            "top/bottom/left/right padding regions are background-dominant "
            "while content regions just inside each edge are bright-dominant. "
            "The debug JSON records the terminal id, painter command path, "
            "marker path, focused accessibility bounds, CGWindowID, configured "
            "padding, sample rectangles, and observed color counts."
        ),
        missing_evidence="None for focused live screenshot-level window-padding pixel proof. Broader GUI/pixel parity, broad font output parity, and full app screenshot parity remain outside this slice.",
        guard_tier="Tier 3",
        guard_command="`(cd roastty && macos/build.nu --action build) && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_window_padding_pixel_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B2B2B2B2D",
        behavior="focused live app/GUI block cursor pixel proof",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `cursor-color`, `cursor-text`, `cursor-style`, and `cursor-style-blink`; `vendor/ghostty/src/renderer/generic.zig` cursor draw paths; `vendor/ghostty/src/renderer/shaders/shaders.metal` cursor shader paths",
        roastty_reference="live debug `Roastty.app`; `roastty/src/renderer/metal/shaders.metal`; `roastty/src/renderer/frame_renderer.rs`; `macos_gui_cursor_pixel_runtime.py`",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 178 adds `macos_gui_cursor_pixel_runtime.py`, a live "
            "debug-app guard that launches Roastty with isolated config, a "
            "non-blinking block cursor, high-contrast `cursor-color`, and a "
            "deterministic child process that writes a marker file before "
            "painting a known background-color landmark and positioning the "
            "cursor at a known grid cell. The guard proves the frontmost "
            "process Unix PID is the exact debug-app PID, maps the focused "
            "accessibility window bounds to the captured PID-owned layer-0 "
            "CGWindowID, captures that exact window with `screencapture -l`, "
            "derives terminal grid origin and cell size from the landmark "
            "rectangle, and requires geometry-derived sample rectangles to "
            "show the expected cursor cell is magenta-dominant while adjacent "
            "background cells and landmark cells are not magenta-dominant. "
            "The debug JSON records the terminal id, painter command path, "
            "marker path, focused accessibility bounds, CGWindowID, measured "
            "grid geometry, expected cursor rectangle, sample rectangles, and "
            "observed color counts."
        ),
        missing_evidence="None for focused live app/GUI block cursor pixel proof. Broader GUI/pixel parity, broad font output parity, and full app screenshot parity remain outside this slice.",
        guard_tier="Tier 3",
        guard_command="`(cd roastty && macos/build.nu --action build) && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_gui_cursor_pixel_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009A",
        behavior="vt-kam-allowed terminal key gating effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `vt-kam-allowed`; terminal ANSI KAM mode behavior",
        roastty_reference="`roastty/src/lib.rs` `vt_kam_allowed` config state, terminal ANSI mode 2 gate, and key dispatch",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "`vt_kam_allowed_*` tests prove the configured `vt-kam-allowed` "
            "value gates terminal KAM key blocking, KAM-disabled terminals "
            "continue to accept input, live config update toggles the existing "
            "surface gate, and configured keybindings run before the KAM gate."
        ),
        missing_evidence="None for vt-kam-allowed terminal key gating behavior.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml vt_kam_allowed`",
    ),
    RuntimeRow(
        id="RUNTIME-009B1",
        behavior="scrollback-limit zero/no-history and alternate-screen no-scrollback terminal effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `scrollback-limit`; `vendor/ghostty/src/termio/Termio.zig` terminal init; alternate-screen terminal behavior",
        roastty_reference="`roastty/src/lib.rs::start_termio`, `roastty/src/termio.rs`, and `roastty/src/terminal` scrollback/alternate-screen behavior",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 117 adds `config_scrollback_limit_runtime_*` tests "
            "proving parsed config `scrollback-limit = 0` disables "
            "scrollback rows on PTY-backed surfaces while default/nonzero "
            "behavior still allows history. "
            "`terminal_stream_alt_screen_has_no_scrollback_and_formatter_reads_active_screen` "
            "proves the terminal-core alternate-screen no-scrollback behavior."
        ),
        missing_evidence="None for parsed config scrollback-limit = 0 no-history behavior and alternate-screen no-scrollback terminal-core behavior covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml config_scrollback_limit_runtime && cargo test --manifest-path roastty/Cargo.toml terminal_stream_alt_screen_has_no_scrollback_and_formatter_reads_active_screen`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2A",
        behavior="title-report CSI 21t gate for OSC-driven terminal titles",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `title-report`; `vendor/ghostty/src/Surface.zig` `report_title` gate and CSI 21t response",
        roastty_reference="`roastty/src/terminal/terminal.rs` title-report gate; `roastty/src/termio.rs` startup options; `roastty/src/lib.rs` surface config update wiring",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 122 adds `terminal_stream_title_report_*` tests proving "
            "CSI `21t` produces no PTY response by default, reports "
            "`ESC ] l <title> ESC \\\\` when enabled, and can be disabled and "
            "re-enabled without losing the stored OSC title. "
            "`config_title_report_runtime_startup_and_update_gate` proves "
            "parsed `title-report` reaches PTY-backed surfaces at startup and "
            "through `roastty_app_update_config`. "
            "`title_report_runtime_parity.py` checks pinned Ghostty's disabled "
            "default and Surface gate plus Roastty's parser, terminal gate, "
            "`TermioSpawnOptions` startup wiring, and live config update wiring."
        ),
        missing_evidence="None for the title-report CSI 21t gate for OSC-driven terminal titles covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml terminal_stream_title_report && cargo test --manifest-path roastty/Cargo.toml config_title_report_runtime && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/title_report_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B1",
        behavior="shell-integration feature env, terminal identity, resource-backed TERMINFO, env override order, and zsh bootstrap runtime effects",
        ghostty_reference="`vendor/ghostty/src/termio/Exec.zig` terminal identity env setup; `vendor/ghostty/src/termio/shell_integration.zig` shell feature and zsh/XDG setup",
        roastty_reference="`roastty/src/termio.rs` Termio spawn env setup; `roastty/src/termio/shell_integration.rs` shell feature and zsh/XDG setup",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 124 split out the proven Termio shell-integration and "
            "terminal identity slice. `termio_env_*` tests prove fallback "
            "`TERM=xterm-256color`/`COLORTERM=truecolor`, resource-backed "
            "configured `TERM`, `TERMINFO`, `ROASTTY_RESOURCES_DIR`, stale "
            "env overwrites, and explicit env override order. "
            "`spawn_with_options_sets_shell_feature_env_even_when_integration_is_none` "
            "proves configured shell feature env, including cursor blink/steady, "
            "reaches child processes. `zsh_integration_spawn_with_options_*` "
            "proves forced zsh integration reaches child env and sources an "
            "inherited `ZDOTDIR` bootstrap. `shell_integration` tests guard "
            "the helper rewrites for supported shells, and "
            "`shell_integration_runtime_parity.py` statically checks pinned "
            "Ghostty's corresponding terminal identity and shell setup markers."
        ),
        missing_evidence="None for this shell-integration feature env, terminal identity, resource-backed TERMINFO, env override order, and zsh bootstrap runtime slice.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml termio_env && cargo test --manifest-path roastty/Cargo.toml spawn_with_options_sets_shell_feature_env_even_when_integration_is_none && cargo test --manifest-path roastty/Cargo.toml zsh_integration_spawn_with_options && cargo test --manifest-path roastty/Cargo.toml shell_integration && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/shell_integration_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2A",
        behavior="configured/static surface-title startup/update and non-empty OSC title dispatch/suppression effects",
        ghostty_reference="`vendor/ghostty/src/Surface.zig` configured title, direct-command title, static-title suppression, and config-update title paths; `vendor/ghostty/src/termio/stream_handler.zig` non-empty title messages",
        roastty_reference="`roastty/src/lib.rs` surface title app actions and static-title gate; `roastty/src/termio.rs` title pump field",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 126 split out configured/static surface-title runtime "
            "behavior. `surface_title_runtime_*` tests prove configured "
            "titles dispatch `ROASTTY_ACTION_SET_TITLE` at startup and config "
            "update, direct command argv[0] dispatches as the title, shell "
            "commands do not dispatch command-derived titles, non-empty OSC "
            "titles dispatch through the surface action path when no static "
            "title is configured, and static configured titles suppress later "
            "non-empty OSC title app actions. `termio_title_*` proves live "
            "PTY title changes travel through `TermioPump` without terminal "
            "callbacks, and `worker_rejects_terminal_with_callbacks` keeps "
            "callback rejection guarded. `surface_title_runtime_parity.py` "
            "statically checks pinned Ghostty's corresponding title branches "
            "and Roastty's runtime/test markers."
        ),
        missing_evidence="None for configured/static surface-title startup/update and non-empty OSC title dispatch/suppression behavior covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_title_runtime && cargo test --manifest-path roastty/Cargo.toml termio_title && cargo test --manifest-path roastty/Cargo.toml worker_rejects_terminal_with_callbacks && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/surface_title_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2B1",
        behavior="stored-PWD title fallback state machine and empty title app dispatch",
        ghostty_reference="`vendor/ghostty/src/termio/stream_handler.zig` `seen_title`, empty title reset, PWD fallback, and PWD clear title paths; `vendor/ghostty/src/Surface.zig` static-title suppression",
        roastty_reference="`roastty/src/terminal/terminal.rs` title/PWD fallback state; `roastty/src/termio.rs` title pump field; `roastty/src/lib.rs` surface title app dispatch and static-title gate",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 127 split out the stored-PWD title fallback state "
            "machine. `terminal_stream_title_pwd_fallback_*` tests prove PWD "
            "updates become the title until a non-empty explicit title is "
            "seen, explicit titles suppress later PWD title changes, empty "
            "title resets fall back to stored PWD or blank, PWD clear blanks "
            "the fallback title, and blank/same-string empty-title events are "
            "queued even when the effective title string is unchanged. The "
            "same guards prove multiple title messages in one parse/read cycle "
            "are preserved in order instead of being coalesced. "
            "`termio_title_pwd_fallback_*` tests prove those title messages "
            "travel through `TermioPump` without terminal callbacks. "
            "`surface_title_pwd_fallback_*` tests prove empty title and "
            "stored-PWD fallback app dispatch when no static title is "
            "configured, and static configured titles suppress empty/fallback "
            "title app actions. `title_pwd_fallback_runtime_parity.py` "
            "statically checks pinned Ghostty's corresponding `seen_title` "
            "branches and Roastty's runtime/test markers."
        ),
        missing_evidence="None for stored-PWD title fallback state and empty title app dispatch behavior covered by these guards. OSC 7 URI parsing, hostname validation, and path normalization are split into RUNTIME-009B2B2B2.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml terminal_stream_title_pwd_fallback && cargo test --manifest-path roastty/Cargo.toml termio_title_pwd_fallback && cargo test --manifest-path roastty/Cargo.toml surface_title_pwd_fallback && cargo test --manifest-path roastty/Cargo.toml worker_rejects_terminal_with_callbacks && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/title_pwd_fallback_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2B2",
        behavior="OSC 7 local PWD URI validation, hostname checks, path normalization, surface PWD dispatch, and title fallback path dispatch",
        ghostty_reference="`vendor/ghostty/src/termio/stream_handler.zig` OSC 7 `reportPwd` scheme gate, hostname validation, path normalization, `.pwd_change`, and title fallback",
        roastty_reference="`roastty/src/terminal/terminal.rs` OSC 7 PWD normalizer and pending PWD/title events; `roastty/src/termio.rs` PWD pump field; `roastty/src/lib.rs` PWD action dispatch",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 128 split out the OSC 7 PWD normalization slice. "
            "`terminal_stream_osc7_pwd_normalization_*` tests prove local "
            "`file` URLs store and dispatch normalized paths, `file` paths "
            "percent-decode valid `%xx` escapes and reject invalid escapes, "
            "local `kitty-shell-cwd` paths stay raw, valid local empty paths "
            "clear PWD like Ghostty, and unsupported schemes, missing hosts, "
            "remote hosts, and invalid encodings do not mutate PWD or title "
            "state. Updated `terminal_stream_title_pwd_fallback_*` tests "
            "prove title fallback uses normalized paths and preserves event "
            "order. `termio_osc7_pwd_normalization_*` and "
            "`termio_title_pwd_fallback_*` tests prove normalized PWD and "
            "title fallback paths travel through `TermioPump` without "
            "terminal callbacks. `surface_osc7_pwd_normalization_*` proves "
            "`ROASTTY_ACTION_PWD` dispatches the normalized path to live "
            "surfaces, and `osc7_pwd_normalization_runtime_parity.py` "
            "statically checks pinned Ghostty and Roastty markers."
        ),
        missing_evidence="None for this local OSC 7 PWD validation, normalization, PWD dispatch, and title fallback path dispatch slice. Remaining terminal gaps stay in RUNTIME-009B2B2B3B2B.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml terminal_stream_osc7_pwd_normalization && cargo test --manifest-path roastty/Cargo.toml terminal_stream_title_pwd_fallback && cargo test --manifest-path roastty/Cargo.toml termio_osc7_pwd_normalization && cargo test --manifest-path roastty/Cargo.toml termio_title_pwd_fallback && cargo test --manifest-path roastty/Cargo.toml surface_osc7_pwd_normalization && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/osc7_pwd_normalization_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2B3A",
        behavior="nonzero scrollback byte quota terminal and PTY-backed surface effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` nonzero `scrollback-limit`; `vendor/ghostty/src/termio/Termio.zig` terminal init; `vendor/ghostty/src/terminal/Screen.zig` byte-quota scrollback",
        roastty_reference="`roastty/src/lib.rs::scrollback_limit_to_bytes`, `roastty/src/termio.rs`, `roastty/src/terminal/terminal.rs`, `roastty/src/terminal/screen.rs`, and `roastty/src/terminal/page_list.rs`",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 129 proves parsed nonzero `scrollback-limit` values "
            "are preserved as byte quotas from config startup through "
            "`TermioSpawnOptions`, `Terminal`, `Screen`, and `PageList`. "
            "`config_scrollback_limit_runtime_nonzero_byte_limit_bounds_history` "
            "proves a PTY-backed surface keeps less history with a tiny "
            "nonzero byte limit than with a large nonzero byte limit. "
            "`terminal_stream_scrollback_byte_limit_bounds_history` proves "
            "the same byte-limit behavior in terminal-core streaming, and "
            "`page_list_scrollback_byte_limit_prunes_by_page_size` proves "
            "PageList prunes/reuses pages when the byte-size quota would be "
            "exceeded. `scrollback_byte_limit_runtime_parity.py` statically "
            "checks pinned Ghostty's byte-quota semantics and Roastty's "
            "config/startup/terminal/PageList wiring plus the regression "
            "guards."
        ),
        missing_evidence="None for parsed nonzero scrollback-limit byte quota behavior covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml config_scrollback_limit_runtime && cargo test --manifest-path roastty/Cargo.toml terminal_stream_scrollback_byte_limit && cargo test --manifest-path roastty/Cargo.toml page_list_scrollback_byte_limit && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/scrollback_byte_limit_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2B3B1",
        behavior="shell-specific startup rewrite helper coverage for supported shells",
        ghostty_reference="`vendor/ghostty/src/termio/shell_integration.zig` shell detection, forced-shell setup, bash, XDG, nushell, zsh, and missing-resource helper tests",
        roastty_reference="`roastty/src/termio/shell_integration.rs` shell detection, forced-shell setup, bash, XDG, nushell, zsh, and missing-resource helper tests",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 130 mirrors pinned Ghostty's shell startup rewrite "
            "helper coverage with Roastty-named expectations. "
            "`shell_integration` tests prove supported shell detection, "
            "forced-shell setup for every supported shell, bash unsupported "
            "option fallback, bash inject flags, rcfile/init-file handling, "
            "inherited `ENV`, `HISTFILE`, `-`/`--` separator preservation, "
            "XDG default/prepend/missing-resource behavior, nushell execute "
            "injection and unsupported-option fallback that keeps XDG env, "
            "nushell missing-resource fallback, zsh `ZDOTDIR` preservation, "
            "and zsh missing-resource fallback. "
            "`shell_startup_rewrite_runtime_parity.py` statically checks "
            "pinned Ghostty's corresponding helper/test markers and Roastty's "
            "runtime/test markers."
        ),
        missing_evidence="None for shell-specific startup rewrite helper coverage covered by these guards. Script-body parity and live-shell PTY parity are not claimed by this row.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml shell_integration && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/shell_startup_rewrite_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2B3B2A",
        behavior="OSC 7 query/fragment, UTF-8 percent-decoding, encoded slash, raw kitty path, and empty-path edge behavior",
        ghostty_reference="`vendor/ghostty/src/termio/stream_handler.zig::reportPwd`; `vendor/ghostty/src/os/uri.zig` raw-path parse behavior",
        roastty_reference="`roastty/src/terminal/terminal.rs` OSC 7 edge tests; `roastty/src/termio.rs` OSC 7 edge pump test; `roastty/src/lib.rs` OSC 7 edge surface dispatch test",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 131 proves remaining OSC 7 URI edge semantics. "
            "`terminal_stream_osc7_pwd_edge_file_paths_trim_and_decode` "
            "proves `file` paths trim query/fragment suffixes while decoding "
            "spaces, UTF-8, and encoded slash bytes. "
            "`terminal_stream_osc7_pwd_edge_kitty_raw_path_keeps_suffixes` "
            "proves `kitty-shell-cwd` keeps percent escapes and raw "
            "query/fragment suffixes in the path, matching pinned Ghostty's "
            "`raw_path` mode. "
            "`terminal_stream_osc7_pwd_edge_no_slash_dispatches_empty_path` "
            "proves local no-slash URLs dispatch an empty path and title "
            "fallback event. `termio_osc7_pwd_edge_*` and "
            "`surface_osc7_pwd_edge_*` prove an edge path travels through "
            "`TermioPump::pwd` and `ROASTTY_ACTION_PWD`. "
            "`osc7_edge_runtime_parity.py` statically checks pinned Ghostty's "
            "`reportPwd` and raw-path parser markers plus Roastty's edge "
            "guards."
        ),
        missing_evidence="None for OSC 7 query/fragment, UTF-8 percent-decoding, encoded slash, raw kitty path, and empty-path edge behavior covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml terminal_stream_osc7_pwd_edge && cargo test --manifest-path roastty/Cargo.toml termio_osc7_pwd_edge && cargo test --manifest-path roastty/Cargo.toml surface_osc7_pwd_edge && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/osc7_edge_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2B3B2B1",
        behavior="config-driven `enquiry-response` ENQ replies through terminal core and PTY-backed runtime",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `enquiry-response`; `vendor/ghostty/src/termio/Termio.zig` derived config; `vendor/ghostty/src/termio/stream_handler.zig` `changeConfig` and `.enquiry` write request path",
        roastty_reference="`roastty/src/config/mod.rs` `enquiry-response`; `roastty/src/terminal/terminal.rs` ENQ response state; `roastty/src/termio.rs` spawn options; `roastty/src/lib.rs` surface config startup/update wiring",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 135 proves config-driven ENQ reply parity. "
            "`terminal_stream_enquiry_response_configured_and_runtime_update` "
            "proves terminal-core ENQ writes the configured response, updates "
            "the response at runtime, and treats the default empty response as "
            "inert. "
            "`terminal_stream_enquiry_response_callback_precedence_is_preserved` "
            "proves the existing embedded callback path still takes "
            "precedence when installed. "
            "`termio_enquiry_response_reaches_child_pty` proves a PTY-backed "
            "child that emits ENQ can read the configured response through "
            "`TermioSpawnOptions`. "
            "`surface_enquiry_response_runtime_startup_and_update` proves "
            "parsed app config reaches initial surfaces and live app config "
            "updates. `enquiry_response_runtime_parity.py` statically checks "
            "pinned Ghostty config, derived-config, `changeConfig`, and ENQ "
            "write-request markers plus Roastty parser/runtime/update guards."
        ),
        missing_evidence="None for config-driven `enquiry-response` ENQ replies through terminal core and PTY-backed runtime.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml terminal_stream_enquiry_response && cargo test --manifest-path roastty/Cargo.toml termio_enquiry_response && cargo test --manifest-path roastty/Cargo.toml surface_enquiry_response && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/enquiry_response_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2B3B2B2A",
        behavior="`osc-color-report-format` runtime effects on OSC palette and dynamic color query replies",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `osc-color-report-format`; `vendor/ghostty/src/termio/Termio.zig` derived config; `vendor/ghostty/src/termio/stream_handler.zig` `changeConfig` and OSC color query formatting",
        roastty_reference="`roastty/src/config/mod.rs` `osc-color-report-format`; `roastty/src/terminal/terminal.rs` OSC color query formatting; `roastty/src/termio.rs` spawn options; `roastty/src/lib.rs` surface config startup/update wiring",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 136 proves config-driven OSC color report-format "
            "runtime parity. "
            "`terminal_stream_osc_color_report_format_defaults_to_16_bit` "
            "proves the default 16-bit `rgb:rrrr/gggg/bbbb` response. "
            "`terminal_stream_osc_color_report_format_8_bit_and_runtime_update` "
            "proves configured 8-bit `rgb:rr/gg/bb` responses and live "
            "terminal updates. "
            "`terminal_stream_osc_color_report_format_none_suppresses_queries_only` "
            "proves `none` suppresses OSC color query replies without "
            "suppressing set/reset operations. "
            "`termio_osc_color_report_format_reaches_child_pty` proves a "
            "PTY-backed child can read the configured response. "
            "`surface_osc_color_report_format_runtime_startup_and_update` "
            "proves parsed app config reaches initial surfaces and live app "
            "config updates. "
            "`osc_color_report_format_runtime_parity.py` statically checks "
            "pinned Ghostty config, derived-config, `changeConfig`, and color "
            "query formatting markers plus Roastty parser/runtime/update "
            "guards."
        ),
        missing_evidence="None for `osc-color-report-format` runtime effects on OSC palette and dynamic color query replies.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml terminal_stream_osc_color_report_format && cargo test --manifest-path roastty/Cargo.toml termio_osc_color_report_format && cargo test --manifest-path roastty/Cargo.toml surface_osc_color_report_format && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/osc_color_report_format_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2B3B2B2B1",
        behavior="`clipboard-write` primary device-attributes clipboard capability advertisement",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `clipboard-write`; `vendor/ghostty/src/termio/Termio.zig` derived config; `vendor/ghostty/src/termio/stream_handler.zig` `changeConfig` and `deviceAttributes` primary response",
        roastty_reference="`roastty/src/config/mod.rs` `clipboard-write`; `roastty/src/terminal/device_attributes.rs`; `roastty/src/terminal/terminal.rs`; `roastty/src/termio.rs`; `roastty/src/lib.rs` surface config startup/update wiring",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 137 proves `clipboard-write` primary device-attributes "
            "runtime parity. "
            "`terminal_stream_device_attributes_clipboard_write_config_and_runtime_update` "
            "proves `clipboard-write = deny` omits feature `52`, while `ask` "
            "and `allow` include feature `52`, and that runtime terminal "
            "updates affect subsequent primary DA and DECID responses. "
            "`terminal_stream_device_attributes_clipboard_write_callback_precedence` "
            "proves the embedded callback path remains an override for direct "
            "terminal users. "
            "`termio_device_attributes_clipboard_write_reaches_child_pty` "
            "proves a PTY-backed child can read the configured deny response. "
            "`surface_device_attributes_clipboard_write_runtime_startup_and_update` "
            "proves parsed app config reaches initial surfaces and live app "
            "config updates. "
            "`clipboard_device_attributes_runtime_parity.py` statically checks "
            "pinned Ghostty config, derived-config, `changeConfig`, and "
            "device-attributes markers plus Roastty parser/runtime/update "
            "guards."
        ),
        missing_evidence="None for `clipboard-write` primary device-attributes clipboard capability advertisement.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml terminal_stream_device_attributes_clipboard_write && cargo test --manifest-path roastty/Cargo.toml termio_device_attributes_clipboard_write && cargo test --manifest-path roastty/Cargo.toml surface_device_attributes_clipboard_write && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/clipboard_device_attributes_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2B3B2B2B2A",
        behavior="`cursor-style` and `cursor-style-blink` default cursor runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `cursor-style` and `cursor-style-blink`; `vendor/ghostty/src/termio/Termio.zig` derived config; `vendor/ghostty/src/termio/stream_handler.zig` `changeConfig`, `setCursorStyle(.default)`, and DEC mode 12 gating",
        roastty_reference="`roastty/src/config/mod.rs` cursor config; `roastty/src/terminal/terminal.rs` default cursor runtime state; `roastty/src/termio.rs`; `roastty/src/lib.rs` surface config startup/update wiring",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 138 proves live `cursor-style` and "
            "`cursor-style-blink` default cursor runtime parity. "
            "`terminal_cursor_default_runtime_update_applies_when_default` "
            "proves live config updates immediately affect the visible cursor "
            "while it remains in the default DECSCUSR state. "
            "`terminal_cursor_default_runtime_update_preserves_program_cursor_until_reset` "
            "proves explicit program DECSCUSR cursor state survives live "
            "config update until a later default reset applies the updated "
            "default. "
            "`terminal_cursor_default_runtime_blink_update_controls_dec_mode_12_gate` "
            "proves live blink config updates continue to gate DEC mode 12 "
            "when explicit, and unset blink falls back to blinking. "
            "`terminal_cursor_default_runtime_direct_reset_does_not_apply_configured_default` "
            "and "
            "`terminal_cursor_default_runtime_ris_preserves_program_cursor_state_until_reset` "
            "prove direct reset/RIS do not incorrectly reapply configured "
            "cursor defaults. "
            "`termio_cursor_default_runtime_spawn_options_reach_terminal` "
            "proves PTY-backed initial spawn options reach the terminal "
            "runtime. "
            "`surface_cursor_default_runtime_startup_and_update` proves "
            "parsed app config reaches initial surfaces and live app config "
            "updates. "
            "`cursor_default_runtime_parity.py` statically checks pinned "
            "Ghostty config, derived-config, `changeConfig`, default cursor, "
            "and DEC mode 12 markers plus Roastty parser/runtime/update "
            "guards."
        ),
        missing_evidence="None for live `cursor-style` and `cursor-style-blink` default cursor runtime effects.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml terminal_cursor_default_runtime && cargo test --manifest-path roastty/Cargo.toml termio_cursor_default_runtime && cargo test --manifest-path roastty/Cargo.toml surface_cursor_default_runtime && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/cursor_default_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2B3B2B2B2B1",
        behavior="`image-storage-limit` kitty graphics storage quota startup and live update effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `image-storage-limit`; `vendor/ghostty/src/termio/Termio.zig` derived config, terminal init, and `changeConfig` kitty graphics size-limit update",
        roastty_reference="`roastty/src/config/mod.rs` `image-storage-limit`; `roastty/src/termio.rs` spawn options; `roastty/src/lib.rs` surface config startup/update wiring; `roastty/src/terminal/terminal.rs` kitty image storage limit setters",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 139 proves `image-storage-limit` kitty graphics "
            "storage quota runtime parity for startup and live config update. "
            "`termio_image_storage_limit_runtime_spawn_options_reach_terminal` "
            "proves non-default spawn options reach the PTY-backed terminal "
            "runtime and enable all kitty image loading media. "
            "`surface_image_storage_limit_runtime_startup_and_update` proves "
            "parsed app config reaches initial surfaces, live app config "
            "updates refresh the active terminal quota, and live updates "
            "restore all kitty image loading media. "
            "`image_storage_limit_runtime_parity.py` statically checks pinned "
            "Ghostty config, derived-config, terminal init, and live "
            "`setKittyGraphicsSizeLimit`/`setKittyGraphicsLoadingLimits(.all)` "
            "markers plus Roastty parser/runtime/update guards."
        ),
        missing_evidence="None for `image-storage-limit` kitty graphics storage quota startup and live update effects.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml termio_image_storage_limit_runtime && cargo test --manifest-path roastty/Cargo.toml surface_image_storage_limit_runtime && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/image_storage_limit_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2B3B2B2B2B2",
        behavior="`grapheme-width-method` terminal default mode startup and reset effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `grapheme-width-method`; `vendor/ghostty/src/termio/Termio.zig` default mode construction; `vendor/ghostty/src/terminal/Terminal.zig` default mode reset behavior",
        roastty_reference="`roastty/src/config/mod.rs` `grapheme-width-method`; `roastty/src/termio.rs` spawn options; `roastty/src/lib.rs` surface config startup wiring; `roastty/src/terminal/terminal.rs` default mode initialization",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 140 proves `grapheme-width-method` startup runtime "
            "parity. `grapheme_width_method_runtime_initializes_mode_and_reset_default` "
            "proves terminal init options set both current and reset/default "
            "DEC 2027 state and that direct reset and RIS restore the "
            "configured default for `unicode` and `legacy`. "
            "`grapheme_width_method_runtime_spawn_options_reach_terminal` "
            "proves termio spawn options reach the PTY-backed terminal. "
            "`surface_grapheme_width_method_runtime_startup_config` proves "
            "parsed default, explicit `unicode`, and explicit `legacy` config "
            "reach initial surfaces. `grapheme_width_method_runtime_parity.py` "
            "statically checks pinned Ghostty termio/default-mode markers plus "
            "Roastty parser/runtime/startup guards."
        ),
        missing_evidence="None for `grapheme-width-method` terminal default mode startup and reset effects.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml grapheme_width_method_runtime && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/grapheme_width_method_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2B3B2B2B2B3",
        behavior="terminal-runtime residual audit for pinned Ghostty termio config paths",
        ghostty_reference="`vendor/ghostty/src/termio/Termio.zig` `DerivedConfig`, direct `opts.full_config`/`opts.config` terminal uses, and `vendor/ghostty/src/termio/stream_handler.zig` `changeConfig` paths",
        roastty_reference="completed runtime inventory rows for terminal/color config effects plus `terminal_runtime_residual_audit.py`",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 142 closes the vague terminal residual row with an "
            "exhaustive source-to-inventory audit. "
            "`terminal_runtime_residual_audit.py` enumerates pinned Ghostty "
            "`DerivedConfig` fields, direct `opts.full_config` and "
            "`opts.config` terminal-runtime uses, stream-handler "
            "`changeConfig` fields, and the associated ENQ, OSC color, "
            "device-attributes, color-scheme report, image quota, scrollback, "
            "cursor default, color/palette, PWD/title, shell integration, and "
            "grapheme-width paths. The guard maps those paths to existing "
            "oracle-complete rows and proves there are no remaining pinned "
            "Ghostty config-driven terminal-runtime fields hidden behind the "
            "old residual bucket. Remaining CFG-223 gaps are now explicitly "
            "non-terminal: font renderer output effects, "
            "renderer-visible GUI/pixel effects, macOS app/window/tab/split/"
            "menu UI, and native notification/link/bell presentation flows."
        ),
        missing_evidence="None for the terminal-runtime residual audit of pinned Ghostty termio config paths.",
        guard_tier="Tier 2",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/terminal_runtime_residual_audit.py`",
    ),
    RuntimeRow(
        id="RUNTIME-010A",
        behavior="PTY/process initial-command, environment, and working-directory launch effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `initial-command`, `env`, and `working-directory` fields",
        roastty_reference="`roastty/src/lib.rs::start_termio`, inherited config helpers, and `roastty/src/termio.rs` spawn options",
        family="process",
        status="Oracle complete",
        evidence=(
            "`first_surface_uses_app_initial_command`, "
            "`later_surface_after_close_ignores_app_initial_command`, and "
            "`surface_inherited_config_*` tests prove initial-command and "
            "working-directory inheritance behavior. `spawn_with_cwd_*` and "
            "`termio_env_*` tests prove the PTY boundary runs children in the "
            "requested working directory, passes explicit env values, inherits "
            "process env values, tolerates non-Unicode inherited env values, "
            "and lets explicit env values override inherited/identity/shell "
            "integration env values."
        ),
        missing_evidence="None for initial-command, environment, and working-directory launch effects covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml first_surface_uses_app_initial_command && cargo test --manifest-path roastty/Cargo.toml later_surface_after_close_ignores_app_initial_command && cargo test --manifest-path roastty/Cargo.toml surface_inherited_config && cargo test --manifest-path roastty/Cargo.toml spawn_with_cwd && cargo test --manifest-path roastty/Cargo.toml termio_env`",
    ),
    RuntimeRow(
        id="RUNTIME-010B1",
        behavior="PTY/process config command, config input, and default-shell launch effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `command` and `input` fields; `vendor/ghostty/src/Surface.zig` command selection; `vendor/ghostty/src/termio/Termio.zig` startup input queuing",
        roastty_reference="`roastty/src/lib.rs::start_termio`, `config_startup_input_bytes`, and surface launch fields",
        family="process",
        status="Oracle complete",
        evidence=(
            "Experiment 116 adds `config_command_input_runtime_*` tests proving "
            "parsed config command precedence, parsed config input delivery, "
            "decoded raw input escape handling, config input path reads, and "
            "explicit surface command/input override behavior. Existing "
            "`surface_start_without_command_*` guards prove the no-command "
            "default-shell and idempotent-start fallback slice."
        ),
        missing_evidence="None for config command, config input, decoded raw input, path input, explicit surface override, and default-shell/no-command launch effects covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml config_command_input_runtime && cargo test --manifest-path roastty/Cargo.toml surface_start_without_command`",
    ),
    RuntimeRow(
        id="RUNTIME-010B2A",
        behavior="PTY/process wait-after-command child-exit close/hold effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `wait-after-command`; `vendor/ghostty/src/Surface.zig::childExited`; `vendor/ghostty/src/apprt/embedded.zig` embedded surface options",
        roastty_reference="`roastty/src/lib.rs` surface wait-after-command state, child-exit handling, and close callback dispatch",
        family="process",
        status="Oracle complete",
        evidence=(
            "Experiment 118 adds `wait_after_command_runtime_*` tests proving "
            "normal child-exit close/hold behavior for default parsed config, "
            "parsed `wait-after-command = true`, embedded "
            "`RoasttySurfaceConfig.wait_after_command = true`, and explicit "
            "surface commands that force hold behavior. The tests run commands "
            "beyond a configured abnormal-exit threshold and prove EOF-only "
            "events do not close or suppress a later child-exit close."
        ),
        missing_evidence="None for normal wait-after-command child-exit close/hold behavior covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml wait_after_command_runtime && cargo test --manifest-path roastty/Cargo.toml close_surface && cargo test --manifest-path roastty/Cargo.toml process_exited`",
    ),
    RuntimeRow(
        id="RUNTIME-010B2B1",
        behavior="PTY/process child-exit exit-code/runtime payload capture and show_child_exited action dispatch",
        ghostty_reference="`vendor/ghostty/src/termio/Termio.zig` child-exit payload; `vendor/ghostty/src/Surface.zig::childExited` `.show_child_exited` action dispatch",
        roastty_reference="`roastty/src/termio.rs` child-exit payload capture; `roastty/src/lib.rs` `ROASTTY_ACTION_SHOW_CHILD_EXITED` dispatch",
        family="process",
        status="Oracle complete",
        evidence=(
            "Experiment 119 adds `child_exited_payload_runtime_*` tests proving "
            "PTY child exit status is captured as exit code plus runtime "
            "milliseconds, the typed `ROASTTY_ACTION_SHOW_CHILD_EXITED` payload "
            "reaches the app action callback before default close handling, "
            "wait-after-command surfaces still hold after dispatch, false "
            "action results do not suppress existing close/hold behavior, and "
            "representative above-threshold and at-or-below-threshold runtime "
            "cases both dispatch the payload."
        ),
        missing_evidence="None for child-exit payload capture and show_child_exited action dispatch covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml child_exited_payload_runtime && cargo test --manifest-path roastty/Cargo.toml wait_after_command_runtime && cargo test --manifest-path roastty/Cargo.toml process_exited && cargo test --manifest-path roastty/Cargo.toml close_surface`",
    ),
    RuntimeRow(
        id="RUNTIME-010B2B2A",
        behavior="PTY/process terminal fallback child-exit text and abnormal-command-exit-runtime close/hold policy",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `abnormal-command-exit-runtime`; `vendor/ghostty/src/Surface.zig::childExited` and `childExitedAbnormally`",
        roastty_reference="`roastty/src/lib.rs` child-exit fallback text, abnormal runtime classification, action handling, and close/hold policy",
        family="process",
        status="Oracle complete",
        evidence=(
            "Experiment 120 adds `child_exited_fallback_policy_runtime_*` "
            "tests proving normal unhandled exits write the pinned normal "
            "fallback text and still close by default, normal handled exits "
            "skip fallback text and still use normal close/hold policy, "
            "abnormal handled exits hold without fallback text, abnormal "
            "unhandled exits write the pinned Ghostty abnormal fallback labels "
            "plus launched command and runtime text and hold, equality with "
            "`abnormal-command-exit-runtime` is abnormal, and above-threshold "
            "runtime is normal."
        ),
        missing_evidence="None for child-exit terminal fallback text and abnormal-command-exit-runtime close/hold policy covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml child_exited_fallback_policy_runtime && cargo test --manifest-path roastty/Cargo.toml child_exited_payload_runtime && cargo test --manifest-path roastty/Cargo.toml wait_after_command_runtime && cargo test --manifest-path roastty/Cargo.toml process_exited && cargo test --manifest-path roastty/Cargo.toml close_surface`",
    ),
    RuntimeRow(
        id="RUNTIME-010B2B2B1",
        behavior="macOS app quit-after-last-window-closed config bridge",
        ghostty_reference="`vendor/ghostty/macos/Sources/App/macOS/AppDelegate.swift::applicationShouldTerminateAfterLastWindowClosed`; `vendor/ghostty/macos/Sources/Ghostty/Ghostty.Config.swift::shouldQuitAfterLastWindowClosed`; `vendor/ghostty/src/config/Config.zig` `quit-after-last-window-closed`",
        roastty_reference="`roastty/macos/Sources/App/macOS/AppDelegate.swift::applicationShouldTerminateAfterLastWindowClosed`; `roastty/macos/Sources/Roastty/Roastty.Config.swift::shouldQuitAfterLastWindowClosed`; `roastty/src/lib.rs::roastty_config_get`",
        family="process",
        status="Oracle complete",
        evidence=(
            "Experiment 121 adds "
            "`config_get_quit_after_last_window_closed_runtime` to prove "
            "`roastty_config_get` returns the macOS default `false`, parsed "
            "`true`, reset/default `false`, and rejects invalid null handles or "
            "outputs for `quit-after-last-window-closed`. "
            "`macos_quit_lifecycle_parity.py` proves the copied Roastty macOS "
            "`applicationShouldTerminateAfterLastWindowClosed`, "
            "`DerivedConfig.shouldQuitAfterLastWindowClosed`, and Swift config "
            "getter blocks match pinned Ghostty after expected app-name "
            "renaming, and that the embedded C ABI exposes the same config key."
        ),
        missing_evidence="None for the copied macOS quit-after-last-window-closed config bridge covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml config_get_quit_after_last_window_closed_runtime && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_quit_lifecycle_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-010B2B2B2",
        behavior="macOS app quit-after-last-window-closed-delay effect",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` documents `quit-after-last-window-closed-delay` as Linux-only; `vendor/ghostty/src/apprt/gtk/class/application.zig` implements the GTK/Linux quit timer",
        roastty_reference="`roastty/macos/Sources`; Roastty's Issue 805 target is the copied macOS app/runtime",
        family="process",
        status="Not applicable",
        evidence=(
            "Experiment 121's `macos_quit_lifecycle_parity.py` verifies "
            "pinned Ghostty documents `quit-after-last-window-closed-delay` as "
            "only implemented on Linux, verifies Ghostty's GTK app consumes "
            "the delay through the quit timer path, and verifies neither "
            "Ghostty nor Roastty macOS Swift sources consume "
            "`quit-after-last-window-closed-delay`."
        ),
        missing_evidence="None for Roastty's copied macOS app; Linux/GTK quit delay behavior is outside the Issue 805 macOS app target.",
        guard_tier="Tier 2",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_quit_lifecycle_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-011A",
        behavior="macOS command palette runtime plumbing, custom command entries, and hosted action dispatch",
        ghostty_reference="`vendor/ghostty/macos/Sources/Features/Command Palette`; `vendor/ghostty/macos/Sources/Features/App Intents/CommandPaletteIntent.swift`; `vendor/ghostty/macos/Sources/Ghostty/Ghostty.App.swift`; `vendor/ghostty/macos/Sources/Features/Terminal/BaseTerminalController.swift`; command palette keyboard shielding in `SurfaceView_AppKit.swift`",
        roastty_reference="`roastty/macos/Sources/Features/Command Palette`; `roastty/macos/Sources/Features/App Intents/CommandPaletteIntent.swift`; `roastty/macos/Sources/Roastty/Roastty.App.swift`; `roastty/macos/Sources/Features/Terminal/BaseTerminalController.swift`; `roastty/macos/Tests/Roastty/CommandPaletteHostedTests.swift`",
        family="macOS app",
        status="Oracle complete",
        evidence=(
            "Experiment 152 splits out command palette runtime plumbing from "
            "the broader macOS app GUI gap. `command_palette_runtime_parity.py` "
            "proves the shared command palette SwiftUI view and app intent are "
            "rename-equivalent to pinned Ghostty, verifies Roastty's "
            "`TerminalCommandPaletteView.terminalCommandOptions` helper is a "
            "testable extraction of Ghostty's custom-command filter/map, and "
            "checks copied app/controller/surface markers for "
            "`toggle_command_palette` dispatch, command-palette notification "
            "delivery, `commandPaletteIsShowing` state toggling, focus return, "
            "and keyboard-event shielding while the palette is shown. "
            "`CommandPaletteHostedTests` proves config-derived "
            "`command-palette-entry` values build selectable options, "
            "unsupported actions are filtered, shortcuts are displayed, "
            "callbacks dispatch the configured action, and hosted surface "
            "`perform(action:)` accepts supported command-palette actions while "
            "rejecting invalid ones."
        ),
        missing_evidence="None for command palette runtime plumbing, custom command entries, hosted action dispatch, toggle notification delivery, focus return, and keyboard shielding. Broader command palette GUI walkthrough and pixel/input navigation remain outside this slice.",
        guard_tier="Tier 2",
        guard_command="`xcodebuild test -project roastty/macos/Roastty.xcodeproj -scheme Roastty -testPlan Roastty -only-testing:RoasttyTests/CommandPaletteHostedTests && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/command_palette_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-011B1",
        behavior="copied macOS workflow plumbing for windows, tabs, splits, menus, titlebar, fullscreen, and quick terminal",
        ghostty_reference="`vendor/ghostty/macos/Sources/Features/Terminal`; `vendor/ghostty/macos/Sources/Features/Splits`; `vendor/ghostty/macos/Sources/Features/QuickTerminal`; `vendor/ghostty/macos/Sources/App/macOS/AppDelegate.swift`; `vendor/ghostty/macos/Sources/Ghostty` action/config bridge files",
        roastty_reference="`roastty/macos/Sources/Features/Terminal`; `roastty/macos/Sources/Features/Splits`; `roastty/macos/Sources/Features/QuickTerminal`; `roastty/macos/Sources/App/macOS/AppDelegate.swift`; `roastty/macos/Sources/Roastty` action/config bridge files; `roastty/macos/Tests/Splits`",
        family="macOS app",
        status="Oracle complete",
        evidence=(
            "Experiment 166 splits copied macOS workflow plumbing out of the "
            "broader live GUI walkthrough gap. "
            "`macos_app_workflow_plumbing_parity.py` proves "
            "`TerminalController.swift`, `TerminalWindow.swift`, the split "
            "SwiftUI helpers, `QuickTerminalController.swift`, "
            "`QuickTerminalIntent.swift`, `AppDelegate.swift`, "
            "`Roastty.Config.swift`, `Roastty.App.swift`, "
            "`RoasttyPackage.swift`, and `FullscreenMode+Extension.swift` "
            "match pinned Ghostty after expected renames where full-file "
            "identity is available. It checks `BaseTerminalController.swift` "
            "workflow anchors for split, focus, resize, equalize, zoom, and "
            "fullscreen dispatch without overclaiming full-file identity "
            "because Experiment 152 already introduced a command-palette "
            "testable extraction there. Existing `SplitTreeTests` and "
            "`TerminalSplitDropZoneTests` prove focused split-tree, split "
            "resize/equalize/focus, state coding, and drop-zone helper "
            "behavior."
        ),
        missing_evidence="None for copied macOS workflow command/action/config plumbing and focused split helper behavior. Live GUI rendering, native menu display, screenshot/pixel evidence, and input navigation remain outside this slice.",
        guard_tier="Tier 2",
        guard_command="`(cd roastty && macos/build.nu --action test) && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_app_workflow_plumbing_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-011B2A",
        behavior="live AppleScript-driven Roastty app workflow automation",
        ghostty_reference="`vendor/ghostty/macos/Ghostty.sdef`; pinned Ghostty AppleScript app/window/tab/terminal workflow commands",
        roastty_reference="`roastty/macos/Roastty.sdef`; `roastty/macos/Sources/Features/AppleScript`; built debug `Roastty.app`",
        family="macOS app",
        status="Oracle complete",
        evidence=(
            "Experiment 167 adds `macos_applescript_workflow_runtime.py`, a "
            "live guard that launches the built debug Roastty app with an "
            "isolated `ROASTTY_CONFIG_PATH` enabling `macos-applescript`, "
            "targets the app by absolute bundle path, creates a window, "
            "queries the window/tab/terminal object model, creates/selects/"
            "closes a tab, creates a split terminal from a controlled surface "
            "configuration whose command writes a temp-file marker, and quits "
            "or kills only the launched debug process. The guard also proves "
            "terminal text input dispatch with a side effect: a controlled "
            "child process records the `input text` marker in a temp file "
            "before the row is considered passing."
        ),
        missing_evidence="None for the live AppleScript workflow slice covering app dictionary access, window creation, tab creation/selection/close, split creation with a command side effect, scoped cleanup, and side-effect-proven `input text` dispatch.",
        guard_tier="Tier 3",
        guard_command="`(cd roastty && macos/build.nu --action build) && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_applescript_workflow_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-011B2B",
        behavior="live macOS GUI titlebar, split layout, screenshot/pixel, and broader walkthrough residual proof",
        ghostty_reference="`vendor/ghostty/macos/Sources`; app/window/tab/split/menu/titlebar/fullscreen/quick-terminal and GUI walkthrough behavior",
        roastty_reference="`roastty/macos/Sources`; Roastty app wrapper and Swift UI",
        family="macOS app",
        status="Oracle complete",
        evidence=(
            "Feature and walkthrough matrices prove launch/cleanup and "
            "keyboard delivery. Experiment 121 split out copied macOS "
            "quit-after-last-window-closed config behavior, Experiment 152 "
            "split out command palette runtime plumbing, custom command "
            "entries, hosted action dispatch, toggle notification delivery, "
            "focus return, and keyboard-event shielding, Experiment 166 split "
            "out copied macOS workflow command/action/config plumbing plus "
            "focused split helper behavior, and Experiment 167 split out "
            "live AppleScript-driven app workflow automation with "
            "side-effect-proven split creation and terminal `input text` "
            "dispatch. Experiment 168 split out the split-divider color crash "
            "path and strengthened the live AppleScript guard to fail on new "
            "Roastty crash reports. "
            "Experiment 171 split out lower-level AppleScript keyboard and "
            "mouse command delivery with child-process side effects. "
            "Experiment 172 split out live native menu visibility, "
            "representative validation, and representative menu action "
            "dispatch. Experiment 173 split out focused fullscreen enter/exit "
            "geometry evidence plus command-palette visibility screenshots in "
            "the live app. Experiment 174 split out focused Quick Terminal "
            "visibility and geometry screenshots in the live app. "
            "Experiment 175 split out focused right-split visual layout "
            "screenshots in the live app. Experiment 176 split out focused "
            "hidden-titlebar traffic-light visibility screenshots in the live "
            "app. Experiment 177 split out focused live window-padding pixel "
            "proof, and Experiment 178 split out focused live GUI cursor pixel "
            "proof. Experiment 185 closes the macOS walkthrough residual row "
            "by binding copied source parity and live guards into one residual "
            "proof: renamed full-file macOS workflow source parity for "
            "window/tab/split/menu/titlebar/fullscreen/Quick Terminal "
            "plumbing, live AppleScript window/tab/split/input automation "
            "with controlled child-process markers, split-terminal ID "
            "lifecycle, lower-level keyboard and mouse event delivery, native "
            "menu visibility/action dispatch, fullscreen and command-palette "
            "screenshots, Quick Terminal screenshots, right-split exact-window "
            "red/blue layout screenshots, hidden titlebar traffic-light pixel "
            "proof, window-padding screenshot proof, and GUI cursor pixel "
            "proof."
        ),
        missing_evidence="None for covered live macOS app walkthrough, titlebar, split, screenshot/pixel, and input navigation residual behavior. Notification/link/bell GUI effects remain tracked separately in RUNTIME-012B2B2B2B2B3C.",
        guard_tier="Tier 3",
        guard_command="`(cd roastty && macos/build.nu --action build) && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_walkthrough_residual_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-011B2C",
        behavior="live split-divider color rendering crash guard",
        ghostty_reference="`vendor/ghostty/macos/Sources/Helpers/Extensions/OSColor+Extension.swift`; `vendor/ghostty/macos/Sources/Ghostty/Ghostty.Config.swift` split divider color derivation",
        roastty_reference="`roastty/macos/Sources/Helpers/Extensions/OSColor+Extension.swift`; `roastty/macos/Sources/Roastty/Roastty.Config.swift`; `roastty/macos/Tests/OSColorExtensionTests.swift`; live AppleScript split workflow guard",
        family="macOS app",
        status="Oracle complete",
        evidence=(
            "Experiment 168 fixes the user-visible `Roastty[DEBUG] quit "
            "unexpectedly` crash whose diagnostic reports showed "
            "`NSColor.getHue` -> `OSColor.darken(by:)` -> "
            "`Roastty.Config.splitDividerColor` -> "
            "`TerminalSplitSubtreeView.body`. `OSColor.darken(by:)` now "
            "converts AppKit colors to sRGB before reading HSB components and "
            "returns unchanged colors that cannot be converted. "
            "`OSColorExtensionTests` covers dynamic AppKit colors and "
            "non-convertible pattern colors. "
            "`macos_applescript_workflow_runtime.py` snapshots macOS "
            "diagnostic reports before launch and fails if a new Roastty crash "
            "report appears during the live window/tab/split/input workflow."
        ),
        missing_evidence="None for the split-divider color crash path and new-crash-report guard. Broader macOS walkthrough residual behavior is covered by RUNTIME-011B2B.",
        guard_tier="Tier 3",
        guard_command="`(cd roastty && macos/build.nu --action test) && (cd roastty && macos/build.nu --action build) && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_applescript_workflow_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-011B2D",
        behavior="live AppleScript split-terminal object lifecycle",
        ghostty_reference="`vendor/ghostty/macos/Ghostty.sdef`; pinned Ghostty terminal `split`, `focus`, `close`, and stable terminal ID scripting",
        roastty_reference="`roastty/macos/Roastty.sdef`; `roastty/macos/Sources/Features/AppleScript/ScriptTerminal.swift`; live debug `Roastty.app`",
        family="macOS app",
        status="Oracle complete",
        evidence=(
            "Experiment 170 extends `macos_applescript_workflow_runtime.py` "
            "to prove the returned split terminal object lifecycle in the live "
            "debug app. The guard creates a split terminal from a controlled "
            "surface configuration, asserts the returned terminal has a "
            "non-empty stable ID, re-resolves that ID at application, window, "
            "and selected-tab scope, sends `input text` to the re-resolved "
            "split terminal and waits for the split child process to record an "
            "exact marker, focuses the re-resolved split and confirms the "
            "selected tab's focused terminal ID changed, closes the re-resolved "
            "split terminal, confirms the selected tab terminal count "
            "decreased, and confirms the closed terminal ID no longer resolves. "
            "The same live guard keeps the absolute app bundle launch, isolated "
            "config, scoped cleanup, and new-crash-report failure behavior."
        ),
        missing_evidence="None for live AppleScript split-terminal object ID re-resolution, input, focus, and close behavior. Broader macOS walkthrough residual behavior is covered by RUNTIME-011B2B.",
        guard_tier="Tier 3",
        guard_command="`(cd roastty && macos/build.nu --action build) && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_applescript_workflow_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-011B2E",
        behavior="live AppleScript keyboard and mouse command delivery",
        ghostty_reference="`vendor/ghostty/macos/Ghostty.sdef`; pinned Ghostty `send key`, `send mouse position`, `send mouse button`, and `send mouse scroll` scripting commands",
        roastty_reference="`roastty/macos/Roastty.sdef`; `roastty/macos/Sources/Features/AppleScript/ScriptKeyEventCommand.swift`; `roastty/macos/Sources/Features/AppleScript/ScriptMousePosCommand.swift`; `roastty/macos/Sources/Features/AppleScript/ScriptMouseButtonCommand.swift`; `roastty/macos/Sources/Features/AppleScript/ScriptMouseScrollCommand.swift`; live debug `Roastty.app`",
        family="macOS app",
        status="Oracle complete",
        evidence=(
            "Experiment 171 extends `macos_applescript_workflow_runtime.py` "
            "with controlled child-process side effects for the lower-level "
            "AppleScript input commands. The guard creates a keyboard capture "
            "terminal running in raw mode, issues `send key \"a\"` and "
            "`send key \"b\"`, and requires that the controlled keyboard child "
            "process records exact raw bytes from `send key`. It also creates "
            "a mouse capture terminal, enables terminal mouse reporting before "
            "events are sent, issues `send mouse position`, "
            "`send mouse button` press/release, and `send mouse scroll`, and "
            "requires that the controlled mouse child process records new "
            "terminal mouse-report bytes after each scripted position, button "
            "press, drag-position, button release, and scroll phase. The same "
            "live guard keeps the absolute app bundle launch, isolated config, "
            "scoped cleanup, and new-crash-report failure behavior."
        ),
        missing_evidence="None for lower-level AppleScript keyboard and mouse command delivery to controlled child processes. Broader macOS walkthrough residual behavior is covered by RUNTIME-011B2B.",
        guard_tier="Tier 3",
        guard_command="`(cd roastty && macos/build.nu --action build) && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_applescript_workflow_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-011B2F",
        behavior="live native menu visibility, validation, and representative dispatch",
        ghostty_reference="`vendor/ghostty/macos/Sources/App/macOS/MainMenu.xib`; `vendor/ghostty/macos/Sources/App/macOS/AppDelegate.swift` menu outlets, shortcut syncing, and `NSMenuItemValidation`",
        roastty_reference="`roastty/macos/Sources/App/macOS/MainMenu.xib`; `roastty/macos/Sources/App/macOS/AppDelegate.swift`; live debug `Roastty.app`; `macos_native_menu_runtime.py`",
        family="macOS app",
        status="Oracle complete",
        evidence=(
            "Experiment 172 adds `macos_native_menu_runtime.py`, a live "
            "debug-app guard that targets the exact launched Roastty Unix PID "
            "through System Events before inspecting or clicking native menus. "
            "The guard proves the native menu bar exposes the expected "
            "top-level Roastty, File, Edit, View, Window, and Help menus; "
            "proves representative File/Edit/View/Window menu items are "
            "visible; proves representative validation states including "
            "Undo/Redo disabled with no undo stack and terminal-window items "
            "enabled while a primary terminal window is key; clicks New Tab "
            "through the native File menu and observes the tab count increase "
            "through AppleScript; clicks Split Right through the native File "
            "menu and observes the selected-tab terminal count increase; and "
            "keeps the absolute app bundle launch, isolated config, scoped "
            "cleanup, and new-crash-report failure behavior."
        ),
        missing_evidence="None for live native menu visibility, representative validation, and representative New Tab / Split Right dispatch. Broader macOS walkthrough residual behavior is covered by RUNTIME-011B2B.",
        guard_tier="Tier 3",
        guard_command="`(cd roastty && macos/build.nu --action build) && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_native_menu_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-011B2G",
        behavior="live fullscreen and command-palette GUI state proof",
        ghostty_reference="`vendor/ghostty/macos/Sources/App/macOS/MainMenu.xib`; `vendor/ghostty/macos/Sources/Features/Terminal/BaseTerminalController.swift`; `vendor/ghostty/macos/Sources/Features/Command Palette/TerminalCommandPalette.swift`; focused live GUI walkthrough behavior",
        roastty_reference="`roastty/macos/Sources/App/macOS/MainMenu.xib`; `roastty/macos/Sources/Features/Terminal/BaseTerminalController.swift`; `roastty/macos/Sources/Features/Command Palette/TerminalCommandPalette.swift`; live debug `Roastty.app`; `macos_gui_state_runtime.py`",
        family="macOS app",
        status="Oracle complete",
        evidence=(
            "Experiment 173 adds `macos_gui_state_runtime.py`, a live "
            "debug-app guard that reuses exact launched PID targeting, isolated "
            "config, scoped cleanup, and new-crash-report failure behavior. "
            "The guard clicks `Window > Toggle Full Screen` through the real "
            "native menu and proves fullscreen entry/exit with PID-scoped "
            "CoreGraphics layer-0 window bounds: entered area must grow at "
            "least 1.40x with width or height growth, exit must return near "
            "baseline dimensions and area, and `AXFullScreen` must agree when "
            "that accessibility attribute is exposed. It also clicks "
            "`View > Command Palette`, captures same-PID/same-window-id "
            "baseline, visible, and dismissed screenshots outside the repo, "
            "requires a baseline-to-palette screenshot delta, and requires the "
            "post-dismiss screenshot to return near baseline. When SwiftUI "
            "command-palette accessibility nodes are exposed, the guard checks "
            "for expected command-palette cues; otherwise it records the "
            "accessibility fallback path while still requiring screenshot "
            "proof."
        ),
        missing_evidence="None for focused live fullscreen enter/exit geometry proof and command-palette visibility screenshot proof. Broader macOS walkthrough residual behavior is covered by RUNTIME-011B2B.",
        guard_tier="Tier 3",
        guard_command="`(cd roastty && macos/build.nu --action build) && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_gui_state_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-011B2H",
        behavior="live Quick Terminal GUI visibility and geometry proof",
        ghostty_reference="`vendor/ghostty/macos/Sources/Features/QuickTerminal/QuickTerminalController.swift`; `vendor/ghostty/macos/Sources/Features/QuickTerminal/QuickTerminalWindow.swift`; `vendor/ghostty/macos/Sources/App/macOS/MainMenu.xib`",
        roastty_reference="`roastty/macos/Sources/Features/QuickTerminal/QuickTerminalController.swift`; `roastty/macos/Sources/Features/QuickTerminal/QuickTerminalWindow.swift`; `roastty/macos/Sources/App/macOS/MainMenu.xib`; live debug `Roastty.app`; `macos_quick_terminal_runtime.py`",
        family="macOS app",
        status="Oracle complete",
        evidence=(
            "Experiment 174 adds `macos_quick_terminal_runtime.py`, a live "
            "debug-app guard that launches the absolute debug app bundle with "
            "isolated config, targets the exact launched PID, and fails on new "
            "Roastty crash reports. The guard configures deterministic Quick "
            "Terminal behavior, clicks `View > Quick Terminal` through the "
            "real native menu, detects a new PID-owned visible window id absent "
            "from the pre-toggle set, and requires that window to have a "
            "nonzero CoreGraphics layer matching the copied `NSPanel`/"
            "popup/floating Quick Terminal source behavior. It proves the "
            "window's top-positioned geometry against the CoreGraphics desktop "
            "top edge and configured `40%` size against the visible screen, "
            "captures the exact Quick Terminal "
            "CGWindowID with `screencapture -l`, verifies nonzero screenshot "
            "dimensions, hides Quick Terminal through the same native menu "
            "item, and requires the extra window id to disappear."
        ),
        missing_evidence="None for focused live Quick Terminal window visibility, floating/popup panel layer behavior, configured top-positioned geometry, exact-window screenshot capture, and hide behavior. Broader macOS walkthrough residual behavior is covered by RUNTIME-011B2B.",
        guard_tier="Tier 3",
        guard_command="`(cd roastty && macos/build.nu --action build) && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_quick_terminal_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-011B2I",
        behavior="live right-split visual layout proof",
        ghostty_reference="`vendor/ghostty/macos/Sources/Features/Splits`; pinned Ghostty split view layout behavior for right splits",
        roastty_reference="`roastty/macos/Sources/Features/Splits`; live debug `Roastty.app`; `macos_split_layout_runtime.py`",
        family="macOS app",
        status="Oracle complete",
        evidence=(
            "Experiment 175 adds `macos_split_layout_runtime.py`, a live "
            "debug-app guard that launches the absolute debug app bundle with "
            "isolated config, targets the exact launched PID, and fails on new "
            "Roastty crash reports. The guard creates a primary terminal whose "
            "controlled child process repeatedly paints a red ANSI background, "
            "splits it `direction right` with a second controlled child process "
            "that repeatedly paints a blue ANSI background, and requires the "
            "selected tab to contain exactly two non-empty terminal IDs. It "
            "then captures the exact PID-owned layer-0 CoreGraphics window ID "
            "with `screencapture -l`, verifies nonzero screenshot dimensions, "
            "and samples stable visible regions inside the left and right "
            "split panes while avoiding titlebar, divider, edge, and wrap-gap "
            "pixels. The "
            "row is passing only when the left region is red-dominant, the "
            "right region is blue-dominant, and neither region can pass as the "
            "other color."
        ),
        missing_evidence="None for focused live right-split visual layout proof in the same app window. Broader macOS walkthrough residual behavior is covered by RUNTIME-011B2B.",
        guard_tier="Tier 3",
        guard_command="`(cd roastty && macos/build.nu --action build) && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_split_layout_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-011B2J",
        behavior="live hidden-titlebar visual proof",
        ghostty_reference="`vendor/ghostty/macos/Sources/Features/Terminal/TerminalController.swift`; `vendor/ghostty/macos/Sources/Features/Terminal/Window Styles/HiddenTitlebarTerminalWindow.swift`; `vendor/ghostty/src/config/Config.zig` `macos-titlebar-style`",
        roastty_reference="`roastty/macos/Sources/Features/Terminal/TerminalController.swift`; `roastty/macos/Sources/Features/Terminal/Window Styles/HiddenTitlebarTerminalWindow.swift`; live debug `Roastty.app`; `macos_titlebar_runtime.py`",
        family="macOS app",
        status="Oracle complete",
        evidence=(
            "Experiment 176 adds `macos_titlebar_runtime.py`, a live debug-app "
            "guard that launches separate `macos-titlebar-style = transparent` "
            "and `macos-titlebar-style = hidden` runs with isolated config, "
            "exact launched PID targeting, scoped cleanup, and new-crash-report "
            "failure. Before each capture, the guard activates the launched app "
            "and proves the frontmost process Unix PID is the exact debug-app "
            "PID, the target window is `AXMain`, and the process "
            "`AXFocusedWindow` is also main. It reads the focused accessibility "
            "window bounds, finds the exact terminal-sized PID-owned layer-0 "
            "CoreGraphics window with matching bounds, captures that exact "
            "CGWindowID with `screencapture -l`, samples the top-left "
            "titlebar/control region with a Swift/AppKit helper, requires "
            "red/yellow/green "
            "traffic-light pixels in the transparent-style positive control, "
            "and requires those traffic-light pixels to be absent in the "
            "hidden-style screenshot."
        ),
        missing_evidence="None for focused live hidden-titlebar traffic-light visibility proof. Broader macOS walkthrough residual behavior is covered by RUNTIME-011B2B.",
        guard_tier="Tier 3",
        guard_command="`(cd roastty && macos/build.nu --action build) && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_titlebar_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012A",
        behavior="link URL matching, renderer highlighting, open-url dispatch, and copy-url binding effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `link` and `link-url`; `vendor/ghostty/src/Surface.zig` link action dispatch",
        roastty_reference="`roastty/src/config/mod.rs` default URL link; `roastty/src/renderer/link.rs`; `roastty/src/lib.rs` open-url and copy-url actions",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "`config_link_url_finalize` proves the configured default URL link "
            "is enabled or removed by `link-url`. `renderer_link_*` tests prove "
            "link highlight matching, modifier-gated ranges, and contiguous "
            "range merging. `surface_open_url_*` tests prove explicit open-url "
            "runtime action dispatch preserves kind, pointer, length, callback "
            "result, and detached/no-callback false paths. "
            "`surface_binding_action_copy_url_to_clipboard_*` tests prove OSC8 "
            "copy-url-to-clipboard binding behavior and false paths."
        ),
        missing_evidence="None for this narrow link/open-url action and renderer matching slice.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_open_url && cargo test --manifest-path roastty/Cargo.toml surface_binding_action_copy_url_to_clipboard && cargo test --manifest-path roastty/Cargo.toml renderer_link && cargo test --manifest-path roastty/Cargo.toml config_link_url_finalize`",
    ),
    RuntimeRow(
        id="RUNTIME-012B1",
        behavior="terminal BEL to live surface ring-bell action dispatch",
        ghostty_reference="`vendor/ghostty/src/termio/stream_handler.zig` BEL `.ring_bell`; `vendor/ghostty/src/Surface.zig` ring-bell throttle/action path",
        roastty_reference="`roastty/src/terminal/terminal.rs` pending bell count; `roastty/src/termio.rs` bell pump count; `roastty/src/lib.rs` `ROASTTY_ACTION_RING_BELL` dispatch",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 123 proves terminal BEL reaches the live PTY-backed "
            "surface action path without installing forbidden terminal "
            "callbacks. `bell_runtime_pending_count_*` guards terminal-core "
            "BEL counting and existing callback preservation. "
            "`termio_bell_*` guards PTY and worker pump propagation. "
            "`surface_bell_*` guards `ROASTTY_ACTION_RING_BELL` dispatch and "
            "the 100ms repeated-BEL throttle. "
            "`bell_runtime_dispatch_parity.py` statically checks the pinned "
            "Ghostty BEL `.ring_bell` path and Roastty runtime/action wiring."
        ),
        missing_evidence="None for terminal BEL to live surface ring-bell action dispatch.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml bell_runtime && cargo test --manifest-path roastty/Cargo.toml termio_bell && cargo test --manifest-path roastty/Cargo.toml surface_bell && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/bell_runtime_dispatch_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2A",
        behavior="OSC desktop notification runtime dispatch and desktop-notifications gate",
        ghostty_reference="`vendor/ghostty/src/terminal/osc/parsers/osc9.zig`; `vendor/ghostty/src/terminal/osc/parsers/rxvt_extension.zig`; `vendor/ghostty/src/termio/stream_handler.zig`; `vendor/ghostty/src/Surface.zig` `desktop-notifications` gate",
        roastty_reference="`roastty/src/terminal/osc.rs`; `roastty/src/terminal/terminal.rs`; `roastty/src/termio.rs`; `roastty/src/lib.rs` desktop notification action dispatch",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 141 proves deterministic OSC desktop notification "
            "runtime dispatch. `terminal_desktop_notification_runtime_*` "
            "proves OSC 9 and OSC 777 notifications are captured without "
            "terminal display side effects. "
            "`termio_desktop_notification_runtime_*` proves child PTY OSC "
            "notification output reaches `TermioPump`. "
            "`surface_desktop_notification_runtime_*` proves live surface "
            "action dispatch, `desktop-notifications = false` suppression, "
            "target surface routing, typed title/body payloads, and pinned "
            "Ghostty 63-byte title / 255-byte body truncation. "
            "`desktop_notification_runtime_parity.py` statically checks the "
            "pinned Ghostty parser, stream-handler, fixed buffers, surface "
            "gate, and Roastty parser/runtime/action/inventory guards."
        ),
        missing_evidence="None for OSC desktop notification runtime dispatch and `desktop-notifications` gate behavior.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml desktop_notification_runtime && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/desktop_notification_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B1",
        behavior="copied macOS bell presentation plumbing",
        ghostty_reference="`vendor/ghostty/macos/Sources/Features/Terminal/BaseTerminalController.swift`; `vendor/ghostty/macos/Sources/App/macOS/AppDelegate.swift`; `vendor/ghostty/macos/Sources/Ghostty/Surface View/SurfaceView*.swift` bell presentation handling",
        roastty_reference="`roastty/macos/Sources/Features/Terminal/BaseTerminalController.swift`; `roastty/macos/Sources/App/macOS/AppDelegate.swift`; `roastty/macos/Sources/Roastty/Surface View/SurfaceView*.swift` bell presentation handling",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 153 split out copied macOS bell presentation plumbing "
            "after the already-proven BEL-to-app-action path. "
            "`bell_presentation_runtime_parity.py` proves the copied "
            "`AppDelegate.swift` and `SurfaceView.swift` sources match pinned "
            "Ghostty after expected renames, and checks focused-surface "
            "title/bell recomputation, aggregate window bell-state publishing, "
            "close-time clear notifications, dock badge counting, and the "
            "separate `bell-features = system`, `bell-features = audio`, "
            "`bell-features = attention`, `bell-features = title`, and "
            "`bell-features = border` gates."
        ),
        missing_evidence="None for copied macOS bell presentation plumbing source parity. Live bell title/border UI effects are tracked by RUNTIME-012B2B2B2B2B3C5, while actual OS/audio/dock runtime side effects remain tracked by RUNTIME-012B2B2B2B2B3C.",
        guard_tier="Tier 0",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/bell_runtime_dispatch_parity.py && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/bell_presentation_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2A",
        behavior="copied macOS user-notification presentation and lifecycle plumbing",
        ghostty_reference="`vendor/ghostty/macos/Sources/App/macOS/AppDelegate.swift`; `vendor/ghostty/macos/Sources/Ghostty/Ghostty.App.swift`; `vendor/ghostty/macos/Sources/Ghostty/GhosttyPackage.swift`; `vendor/ghostty/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift` user notification handling",
        roastty_reference="`roastty/macos/Sources/App/macOS/AppDelegate.swift`; `roastty/macos/Sources/Roastty/Roastty.App.swift`; `roastty/macos/Sources/Roastty/RoasttyPackage.swift`; `roastty/macos/Sources/Roastty/Surface View/SurfaceView_AppKit.swift` user notification handling",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 155 split out copied macOS user-notification "
            "presentation and lifecycle plumbing after the already-proven OSC "
            "desktop notification dispatch path. "
            "`macos_user_notification_runtime_parity.py` proves "
            "`AppDelegate.swift`, `Roastty.App.swift`, and "
            "`RoasttyPackage.swift` match pinned Ghostty after expected "
            "renames, and normalized-compares the notification-relevant "
            "`SurfaceView_AppKit.swift` lifecycle blocks for notification "
            "identifier tracking, deinit/focus cleanup, content/request "
            "construction, `requireFocus` userInfo, delivery, delayed focused "
            "cleanup, and click-to-focus routing."
        ),
        missing_evidence="None for copied macOS user-notification presentation/lifecycle source parity; live OS banner/sound behavior remains tracked by RUNTIME-012B2B2B2B2B3C.",
        guard_tier="Tier 0",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_user_notification_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B1",
        behavior="desktop notification rate limiting",
        ghostty_reference="`vendor/ghostty/src/App.zig` `last_notification_time` / `last_notification_digest`; `vendor/ghostty/src/Surface.zig::showDesktopNotification` one-second and five-second suppression",
        roastty_reference="`roastty/src/lib.rs` app-level desktop notification limiter and surface dispatch tests",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 156 ports pinned Ghostty's desktop notification rate "
            "limiting into Roastty's app-level live surface dispatch. "
            "`surface_desktop_notification_runtime_*` tests prove "
            "`desktop-notifications = false` suppresses before limiter state "
            "updates, delivered notifications update app-level limiter state, "
            "second notifications inside one second are suppressed without "
            "state updates, delimiterless `title || body` identity matches "
            "Ghostty's digest input, identical notifications before five "
            "seconds are suppressed without state updates, identical "
            "notifications after five seconds dispatch again, and limiter "
            "state is shared across surfaces on the same app. "
            "`desktop_notification_rate_limit_runtime_parity.py` statically "
            "checks pinned Ghostty's limiter source and Roastty's constants, "
            "state, explicit-time helper, delimiterless identity, tests, and "
            "inventory/matrix split."
        ),
        missing_evidence="None for desktop notification rate-limit runtime behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_desktop_notification_runtime && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/desktop_notification_rate_limit_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2A",
        behavior="command-finished notification runtime dispatch",
        ghostty_reference="`vendor/ghostty/src/termio/stream_handler.zig` semantic prompt `.start_command` / `.stop_command` emission; `vendor/ghostty/src/Surface.zig` command timer and `.command_finished` action dispatch",
        roastty_reference="`roastty/src/terminal/terminal.rs` OSC 133 command-event queue, `roastty/src/termio.rs` pump forwarding, `roastty/src/lib.rs` command timer and `ROASTTY_ACTION_COMMAND_FINISHED` dispatch",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 157 ports pinned Ghostty's command-finished runtime "
            "path. Terminal tests prove OSC 133 `C` queues command start and "
            "`D` queues command stop without display side effects, including "
            "Ghostty's exit-code mapping of absent or malformed codes to 0, "
            "out-of-range parsed codes to 1, and valid `0..255` codes to their "
            "value. Termio tests prove child PTY OSC 133 output reaches the "
            "surface pump. Surface tests prove start records a timer without "
            "dispatch, stop without start does nothing, repeated start resets "
            "the timer, stop dispatches `ROASTTY_ACTION_COMMAND_FINISHED` with "
            "exit code and nanosecond duration, command events mark the surface "
            "dirty, and child-exited dispatch remains intact when both events "
            "arrive in one pump. `command_finished_runtime_parity.py` statically "
            "checks pinned Ghostty source, Roastty parser/pump/surface/ABI "
            "implementation, tests, and this inventory split."
        ),
        missing_evidence="None for command-finished runtime action dispatch; the app-level notification presentation that may consume the action remains tracked by RUNTIME-012B2B2B2B2B3C.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml terminal_command_event_runtime && cargo test --manifest-path roastty/Cargo.toml termio_command_event_runtime && cargo test --manifest-path roastty/Cargo.toml surface_command_finished_runtime && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/command_finished_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B1",
        behavior="app-notifications GTK-only runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `app-notifications`; `vendor/ghostty/src/apprt/gtk/class/window.zig` GTK config-reload and clipboard-copy toast gates",
        roastty_reference="`roastty/src/config/mod.rs` parser/formatter support; no Roastty macOS runtime consumer because the pinned Ghostty runtime effect is GTK-only",
        family="notifications",
        status="Not applicable",
        evidence=(
            "Experiment 158 classifies `app-notifications` as GTK-only runtime "
            "behavior for Roastty parity. Pinned Ghostty documents the option "
            "with `This configuration only applies to GTK.` and runtime "
            "consumption is in `src/apprt/gtk/class/window.zig` for "
            "`config-reload` and `clipboard-copy` toasts. Pinned Ghostty macOS "
            "sources do not consume the option. Roastty keeps parser and "
            "formatter parity for the config field but has no GTK in-app toast "
            "runtime to reproduce in the macOS app. "
            "`app_notifications_platform_runtime_parity.py` statically checks "
            "the pinned GTK-only documentation and consumers, absence of macOS "
            "consumers, Roastty parser/formatter coverage, and this inventory "
            "split."
        ),
        missing_evidence="None; the runtime effect is GTK-only in pinned Ghostty and therefore not applicable to Roastty's macOS runtime.",
        guard_tier="Tier 0",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml app_notifications && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/app_notifications_platform_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2A",
        behavior="copied macOS link-hover banner plumbing",
        ghostty_reference="`vendor/ghostty/macos/Sources/Ghostty/Ghostty.App.swift` `setMouseOverLink`; `vendor/ghostty/macos/Sources/Ghostty/Surface View/OSSurfaceView.swift` `hoverUrl`; `vendor/ghostty/macos/Sources/Ghostty/Surface View/SurfaceView.swift` `URLHoverBanner`; `vendor/ghostty/macos/Sources/Helpers/URLHoverBanner.swift`",
        roastty_reference="`roastty/macos/Sources/Roastty/Roastty.App.swift` `setMouseOverLink`; `roastty/macos/Sources/Roastty/Surface View/OSSurfaceView.swift` `hoverUrl`; `roastty/macos/Sources/Roastty/Surface View/SurfaceView.swift` `URLHoverBanner`; `roastty/macos/Sources/Helpers/URLHoverBanner.swift`",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 159 proves the copied macOS link-hover banner plumbing "
            "matches pinned Ghostty after expected renames. "
            "`macos_link_hover_banner_runtime_parity.py` normalized-compares "
            "`OSSurfaceView.swift`, `SurfaceView.swift`, `URLHoverBanner.swift`, "
            "and `Ghostty.App.swift`/`Roastty.App.swift`, and checks the concrete "
            "`mouse_over_link` action dispatch, `setMouseOverLink` surface "
            "target handling, empty-url clearing, UTF-8 URL decode into "
            "`hoverUrl`, published hover state, `URLHoverBanner(url:)` rendering, "
            "middle truncation, and left/right banner hover switch."
        ),
        missing_evidence="None for copied macOS link-hover banner source plumbing; live copied SwiftUI URL hover banner display is tracked by RUNTIME-012B2B2B2B2B3C4, real OS link cursor pixels are tracked by RUNTIME-012B2B2B2B2B3C6, and live Quick Look/native definition UI is tracked by RUNTIME-012B2B2B2B2B3C8.",
        guard_tier="Tier 0",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_link_hover_banner_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2B1",
        behavior="link preview config predicates and link-specific context-menu selection runtime",
        ghostty_reference="`vendor/ghostty/src/Surface.zig` `linkAtPos`, `link_previews == .true`, `link_previews != .false`, and right-click `context-menu` link selection before word fallback",
        roastty_reference="`roastty/src/config/mod.rs::LinkPreviews`; `roastty/src/lib.rs` `context_menu_action`, link selection helpers, and `link_preview_context_runtime_*` tests",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 160 splits out deterministic link preview predicates "
            "and context-menu runtime semantics. "
            "`link_preview_context_runtime_gates_preview_by_link_kind` proves "
            "regular detected links use a preview predicate only for "
            "`link-previews = true` while OSC 8 links use a preview predicate "
            "for `true` and `osc8`, matching pinned "
            "Ghostty's `link_previews == .true` and `link_previews != .false` "
            "conditions. `link_preview_context_runtime_context_menu_selects_regex_link` "
            "proves right-click `context-menu` selects the configured detected "
            "link at the cursor with ctrl/super modifiers and returns unhandled. "
            "`link_preview_context_runtime_context_menu_regex_is_line_scoped` "
            "proves configured regex link selection is scoped to the clicked "
            "line with semantic prompt boundaries rather than the whole "
            "viewport. "
            "`link_preview_context_runtime_context_menu_preserves_containing_link_selection` "
            "proves an existing containing selection is preserved, and "
            "`link_preview_context_runtime_context_menu_selects_osc8_with_ctrl_or_super` "
            "proves the OSC 8 branch selects the hovered cell when ctrl/super is "
            "active. `link_preview_context_runtime_parity.py` statically checks "
            "the pinned Ghostty conditions, Roastty implementation markers, "
            "tests, and this inventory split."
        ),
        missing_evidence="None for deterministic link preview config predicates and link-specific context-menu selection runtime semantics. Actual `mouse_over_link` preview dispatch remains tracked by RUNTIME-012B2B2B2B2B2; GUI hover/cursor and native preview display remain tracked by RUNTIME-012B2B2B2B2B3C.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml link_preview_context_runtime -- --test-threads=1 && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/link_preview_context_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2B2",
        behavior="link hover preview surface action dispatch",
        ghostty_reference="`vendor/ghostty/src/Surface.zig` `mouseRefreshLinks`, `modsChanged`, `keyCallback`, `linkAtPos`, mouse reporting gate, left-click drag suppression, `mouse_shape`, and `mouse_over_link` dispatch",
        roastty_reference="`roastty/src/lib.rs` `refresh_link_hover`, `refresh_link_hover_for_key_mods`, link hover URL lookup, mouse reporting/shift override gate, left-click drag suppression, action ABI payloads, and `link_hover_preview_dispatch_*`/`link_hover_modifier_refresh_*` tests",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 161 splits out deterministic surface-side link hover "
            "preview dispatch. Roastty now refreshes link hover on mouse-position "
            "updates using Ghostty's refresh semantics, configured-link/OSC8 "
            "priority, ctrl/super OSC8 gate, `link-previews` preview gating, "
            "mouse-reporting/shift-override gate, and left-click drag suppression. "
            "`link_hover_preview_dispatch_regular_link_gates_preview_and_shape` "
            "proves regular detected links dispatch pointer shape and preview URL "
            "only when regular previews are enabled. "
            "`link_hover_preview_dispatch_osc8_link_gates_preview_with_ctrl_or_super` "
            "proves OSC8 hover dispatch requires ctrl/super and obeys the OSC8 "
            "preview gate. "
            "`link_hover_preview_dispatch_repeats_while_over_link_and_clears_on_leave` "
            "proves Ghostty-compatible repeat refresh while already over a link "
            "and empty-url/current-terminal-shape clearing when leaving or "
            "moving out of the viewport. "
            "`link_hover_preview_dispatch_respects_mouse_reporting_and_shift_override` "
            "proves normal mouse reporting suppresses hover dispatch while "
            "shift override allows refresh, and "
            "`link_hover_preview_dispatch_suppresses_left_drag_hover` proves "
            "left-drag hover suppression. "
            "Experiment 162 extends the same surface-action slice to "
            "key/modifier-driven hover refresh: "
            "`link_hover_modifier_refresh_super_enables_regular_link_without_mouse_move` "
            "proves a stationary super press can discover a regular detected "
            "link despite the same-cell no-link cache, "
            "`link_hover_modifier_refresh_super_release_clears_regular_link` "
            "proves modifier release clears hover with the current terminal "
            "mouse shape, "
            "`link_hover_modifier_refresh_super_enables_osc8_without_mouse_move` "
            "proves stationary OSC8 hover dispatch, "
            "`link_hover_modifier_refresh_respects_reporting_and_shift_override` "
            "proves the Ghostty reporting/shift branches for key-driven "
            "refresh, and "
            "`link_hover_modifier_refresh_reporting_captured_shift_noops` "
            "proves captured shift under mouse reporting neither refreshes nor "
            "clears. "
            "`link_hover_preview_dispatch_parity.py` statically checks pinned "
            "Ghostty anchors, Roastty implementation markers, tests, and this "
            "inventory split. "
            "`link_hover_modifier_refresh_parity.py` statically checks pinned "
            "Ghostty key/modifier anchors, Roastty implementation markers, "
            "tests, and this inventory split."
        ),
        missing_evidence="None for deterministic surface-side link hover preview dispatch. Live app dispatch is tracked by RUNTIME-012B2B2B2B2B3C3, real OS link cursor pixels are tracked by RUNTIME-012B2B2B2B2B3C6, and native preview display remains tracked by RUNTIME-012B2B2B2B2B3C.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml link_hover_preview_dispatch -- --test-threads=1 && cargo test --manifest-path roastty/Cargo.toml link_hover_modifier_refresh -- --test-threads=1 && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/link_hover_preview_dispatch_parity.py && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/link_hover_modifier_refresh_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2B3A",
        behavior="live macOS OSC notification request and authorization-state trace",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` notification and link preview fields; `vendor/ghostty/src/Surface.zig` notification/link hover/menu paths; macOS native notification and link handling",
        roastty_reference="`roastty/src/lib.rs` desktop-notification surface action dispatch; `roastty/macos/Sources/Roastty/Roastty.App.swift` `showDesktopNotification` trace hooks",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 186 adds `macos_notification_link_bell_trace_runtime.py`, "
            "which launches the built debug app with an isolated config, emits "
            "OSC 777 from a live terminal, and proves the macOS app receives "
            "the notification request with the expected title/body and records "
            "the VM's `UNUserNotificationCenter` authorization status. In this "
            "VM, authorization status is denied (`authorizationStatus=1`), so "
            "this row proves the app request and authorization-state path, not "
            "OS banner delivery."
        ),
        missing_evidence="None for live app notification request dispatch and authorization-state capture. Actual OS notification banner/sound delivery remains tracked by RUNTIME-012B2B2B2B2B3C.",
        guard_tier="Tier 3",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_notification_link_bell_trace_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2B3B",
        behavior="live macOS bell feature bridge and app/surface dispatch trace",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `bell-features`; pinned Ghostty macOS app bell notification plumbing",
        roastty_reference="`roastty/src/lib.rs` `roastty_config_get` `bell-features`; `roastty/macos/Sources/App/macOS/AppDelegate.swift` bell handling; `roastty/macos/Sources/Roastty/Surface View/SurfaceView_AppKit.swift` surface bell handling",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 186 fixes the `bell-features` config bridge so the Swift "
            "app reads the parsed feature bitset from `libroastty`. "
            "`macos_notification_link_bell_trace_runtime.py` then proves live "
            "BEL dispatch reaches the surface, sets the surface bell state, "
            "drives the app-level system/attention branches when audio is "
            "disabled, and separately drives the audio branch with the configured "
            "`/System/Library/Sounds/Ping.aiff` path and volume."
        ),
        missing_evidence="None for live bell feature bridge, app branch dispatch, surface bell state dispatch, and configured audio-path request trace. Live bell title/border UI effects are tracked by RUNTIME-012B2B2B2B2B3C5, background attention request dispatch is tracked by RUNTIME-012B2B2B2B2B3C7, and audible sound output plus OS-visible dock-attention bounce/state remain tracked by RUNTIME-012B2B2B2B2B3C.",
        guard_tier="Tier 3",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_notification_link_bell_trace_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2B3C1",
        behavior="live macOS native context-menu construction trace",
        ghostty_reference="Pinned Ghostty macOS native surface context-menu construction and item routing",
        roastty_reference="`roastty/macos/Sources/Roastty/Surface View/SurfaceView_AppKit.swift` native context-menu construction; `roastty/macos/Sources/Features/AppleScript/ScriptTerminal.swift` env-gated UI test action",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 187 adds `macos_native_context_menu_trace_runtime.py`, "
            "which launches the built debug app with an isolated config, creates "
            "a real terminal surface through AppleScript, invokes an env-gated "
            "test action on the live `SurfaceView`, and proves the native "
            "context-menu path constructs the expected menu items including "
            "`Paste`, split actions, and `Change Terminal Title...`."
        ),
        missing_evidence="None for live native context-menu construction and item-list trace. Live copied SwiftUI URL hover banner display is tracked by RUNTIME-012B2B2B2B2B3C4, real OS link cursor pixels are tracked by RUNTIME-012B2B2B2B2B3C6, and live Quick Look/native definition UI is tracked by RUNTIME-012B2B2B2B2B3C8.",
        guard_tier="Tier 3",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_native_context_menu_trace_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2B3C2",
        behavior="live macOS URL-opening request through controlled app hook",
        ghostty_reference="Pinned Ghostty macOS `open_url` app action and `NSWorkspace.open` URL-opening request behavior",
        roastty_reference="`roastty/macos/Sources/Roastty/Roastty.App.swift` `openURL`; `roastty/macos/Sources/Features/AppleScript/ScriptTerminal.swift` env-gated UI test action",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 187 adds `macos_controlled_url_open_runtime.py`, which "
            "launches the built debug app with an isolated config and an "
            "env-gated AppleScript action, calls the same `Roastty.App.openURL` "
            "path used by app runtime actions with "
            "`https://example.com/issue805-exp187-controlled`, records the exact "
            "resolved URL to a controlled file, and suppresses the final "
            "`NSWorkspace.open` call so the guard is deterministic in the VM. "
            "This proves Roastty requests opening the exact URL through the "
            "native app path; it intentionally does not claim external Launch "
            "Services handler delivery."
        ),
        missing_evidence="None for live native app URL-opening request construction and exact URL routing. External Launch Services handler delivery remains outside the deterministic VM oracle and is tracked by RUNTIME-012B2B2B2B2B3C.",
        guard_tier="Tier 3",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_controlled_url_open_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2B3C3",
        behavior="live macOS link hover URL and cursor-shape app dispatch",
        ghostty_reference="Pinned Ghostty macOS link-hover mouse movement, Command-modified regular link detection, cursor-shape request, and hover URL app action",
        roastty_reference="`roastty/src/lib.rs` point-to-pixel mouse coordinate mapping and link hover dispatch; `roastty/macos/Sources/Roastty/Roastty.App.swift` `setMouseOverLink`; `roastty/macos/Sources/Roastty/Surface View/SurfaceView_AppKit.swift` cursor-shape handling",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 188 fixes Roastty's live mouse point-to-cell mapping "
            "to apply the macOS backing scale before using pixel-space grid "
            "geometry. `link_hover_preview_dispatch_scales_macos_point_coordinates` "
            "guards the Retina/AppKit point conversion in the Rust core. "
            "`macos_live_link_hover_runtime.py` launches the built debug app "
            "with an isolated config, prints a deterministic URL in a live "
            "terminal, injects Command-modified mouse movement into the real "
            "window, proves the focused CGWindowID and terminal resize, and "
            "records `cursorShape raw=3 pointerStyle=link` plus the exact "
            "`mouseOverLink url=https://example.com/issue805-exp188-link-hover` "
            "from the live app trace."
        ),
        missing_evidence="None for live regular-link hover dispatch, cursor-shape request, and exact hovered URL routing to the app. Live copied SwiftUI URL hover banner display is tracked by RUNTIME-012B2B2B2B2B3C4, real OS link cursor pixels are tracked by RUNTIME-012B2B2B2B2B3C6, and live Quick Look/native definition UI is tracked by RUNTIME-012B2B2B2B2B3C8.",
        guard_tier="Tier 3",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml link_hover_preview_dispatch_scales_macos_point_coordinates -- --test-threads=1 && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_live_link_hover_runtime.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2B3C4",
        behavior="live macOS URL hover banner display pixels",
        ghostty_reference="Pinned Ghostty macOS URL hover banner display when `mouse_over_link` sets a hover URL",
        roastty_reference="`roastty/macos/Sources/Helpers/URLHoverBanner.swift`; `roastty/macos/Sources/Roastty/Surface View/SurfaceView.swift`; `roastty/macos/Sources/Roastty/Roastty.App.swift` `setMouseOverLink`",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 189 adds `macos_live_link_hover_banner_pixels.py`, "
            "which launches the built debug app with an isolated config, "
            "prints a deterministic URL in a real terminal, captures the exact "
            "focused CGWindowID before hover, injects Command-modified mouse "
            "movement until the trace records `cursorShape raw=3 "
            "pointerStyle=link` plus "
            "`mouseOverLink url=https://example.com/issue805-exp189-link-banner`, "
            "captures the same window after hover, and compares the PNGs with "
            "a Swift sampler. The recorded run saw 32674 changed pixels in "
            "the expected bottom-left banner band versus 373 in the upper-left "
            "control band and 1086 in the bottom-right control band, proving a "
            "localized visible `URLHoverBanner` overlay."
        ),
        missing_evidence="None for live copied SwiftUI URL hover banner display. Real OS link cursor pixels are tracked by RUNTIME-012B2B2B2B2B3C6, and live Quick Look/native definition UI is tracked by RUNTIME-012B2B2B2B2B3C8.",
        guard_tier="Tier 3",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_live_link_hover_banner_pixels.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2B3C5",
        behavior="live macOS bell title prefix and border overlay pixels",
        ghostty_reference="Pinned Ghostty macOS bell `title` and `border` feature behavior",
        roastty_reference="`roastty/macos/Sources/Features/Terminal/BaseTerminalController.swift` `computeTitle`; `roastty/macos/Sources/Roastty/Surface View/SurfaceView.swift` `BellBorderOverlay`; `roastty/macos/Sources/Roastty/Surface View/SurfaceView_AppKit.swift` bell state handling",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 190 adds `macos_live_bell_title_border_pixels.py`, "
            "which launches the built debug app with an isolated config using "
            "`bell-features = no-system,no-audio,no-attention,title,border`, "
            "sets a deterministic OSC 2 title, captures the exact focused "
            "CGWindowID before BEL, triggers BEL from the live terminal, and "
            "requires trace evidence for `ringBell target=surface`, "
            "`appBell system=false audio=false attention=false`, and "
            "`surfaceBell state=true title=Issue805Exp190BellTitle`. The "
            "guard proves the title path with AX title state changing from "
            "`Issue805Exp190BellTitle` to `🔔 Issue805Exp190BellTitle`, then "
            "compares before/after screenshots with a Swift sampler that masks "
            "the titlebar area and proves localized edge-band deltas: 6375 "
            "changed pixels on each side edge, 9390 on the bottom edge, zero "
            "changed pixels in the center control region, and zero changed "
            "pixels in the titlebar/control mask."
        ),
        missing_evidence="None for live copied macOS bell title prefix state and `BellBorderOverlay` pixels. Background attention request dispatch is tracked by RUNTIME-012B2B2B2B2B3C7, while audible sound output and OS-visible dock-attention bounce/state remain tracked by RUNTIME-012B2B2B2B2B3C.",
        guard_tier="Tier 3",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_live_bell_title_border_pixels.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2B3C6",
        behavior="live macOS real OS link cursor pixels",
        ghostty_reference="Pinned Ghostty macOS cursor shape behavior when hovering a Command-modified link",
        roastty_reference="`roastty/macos/Sources/Roastty/Surface View/SurfaceView_AppKit.swift` cursor-shape handling plus macOS-rendered pointer pixels captured by `screencapture -C`",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 191 adds `macos_real_link_cursor_pixels.py`, which "
            "launches the built debug app with an isolated config, prints a "
            "deterministic URL in a real terminal, injects Command-modified "
            "mouse movement until the trace records `cursorShape raw=3 "
            "pointerStyle=link` plus "
            "`mouseOverLink url=https://example.com/issue805-exp191-real-cursor`, "
            "and captures matched cursorless and cursor-included "
            "`screencapture -C` rectangles around a normal terminal cell and "
            "the hovered link. The recorded run had stable cursorless "
            "backgrounds, 350 normal-cursor changed pixels, 701 link-cursor "
            "changed pixels, a 721-pixel symmetric difference, and a 15-pixel "
            "bbox delta, proving macOS rendered a different link cursor over "
            "the live Roastty window."
        ),
        missing_evidence="None for real OS-rendered normal and link cursor pixels in the live macOS app. Live Quick Look/native definition UI is tracked by RUNTIME-012B2B2B2B2B3C8.",
        guard_tier="Tier 3",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_real_link_cursor_pixels.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2B3C7",
        behavior="live macOS background Dock attention request dispatch",
        ghostty_reference="Pinned Ghostty macOS `bell-features = attention` handling through `NSApp.requestUserAttention(.informationalRequest)` and Dock badge authorization-gated update path",
        roastty_reference="`roastty/macos/Sources/App/macOS/AppDelegate.swift` `roasttyBellDidRing`, `syncDockBadge`, and `setDockBadge`",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 192 adds `macos_live_bell_attention_dock_state.py`, "
            "which launches the built debug app with isolated configs for an "
            "attention-enabled run and an otherwise identical attention-disabled "
            "control. The guard backgrounds Roastty before the child emits BEL, "
            "then proves the live BEL path with `ringBell target=surface`, "
            "`surfaceBell state=true`, and `appBell system=false audio=false "
            "attention=true`. The enabled run records `appBell active=false` "
            "and `appBell attentionRequest=0`, proving the AppKit request path "
            "was invoked while the app was inactive on this VM/macOS build. "
            "The disabled control records `appBell system=false audio=false "
            "attention=false` with no `attentionRequest` or active-state trace. "
            "Both runs record Dock badge authorization state "
            "`authorizationStatus=1 badgeSetting=2`, explaining why no badge "
            "label can be deterministically asserted in this VM."
        ),
        missing_evidence="None for live inactive-app `NSApp.requestUserAttention(.informationalRequest)` dispatch and authorization-gated Dock badge branch capture. OS-visible Dock bounce/state beyond AppKit request dispatch remains tracked by RUNTIME-012B2B2B2B2B3C.",
        guard_tier="Tier 3",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_live_bell_attention_dock_state.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2B3C8",
        behavior="live macOS Quick Look/native definition UI",
        ghostty_reference="Pinned Ghostty macOS `SurfaceView.quickLook(with:)`, `ghostty_surface_quicklook_word`, `ghostty_surface_quicklook_font`, and `showDefinition(for:at:)` behavior",
        roastty_reference="`roastty/macos/Sources/Roastty/Surface View/SurfaceView_AppKit.swift` Quick Look handling; `roastty/src/lib.rs` `roastty_surface_quicklook_word` and `roastty_surface_quicklook_font`; `roastty/macos/Sources/Features/AppleScript/ScriptTerminal.swift` env-gated test action",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 193 fixes `roastty_surface_quicklook_font` so Roastty "
            "returns a copied primary CoreText font for Quick Look instead of "
            "null, matching pinned Ghostty's CoreText path. It adds "
            "`macos_live_quicklook_definition.py`, which launches the built "
            "debug app with isolated config/defaults, paints the deterministic "
            "word `serendipity` in a live terminal, moves the real mouse over "
            "that word, and invokes an env-gated AppleScript `ui_test_quicklook` "
            "action on the focused `SurfaceView`. The guard requires trace "
            "evidence for `quickLook uiTestAction=invoke`, "
            "`quickLook text=serendipity len=11 ... fontPresent=true`, and "
            "`quickLook showDefinition=true`. The passing visible-UI run kept "
            "the focused AX window bounds fixed at 800x600 points while the "
            "guard waited for the post-Quick-Look CGWindow capture to expand "
            "by at least 100 pixels at the same height and required at least "
            "50000 nonblack pixels in the extra capture band, proving AppKit "
            "displayed a native definition popover attached to the live "
            "Roastty window."
        ),
        missing_evidence="None for live Quick Look/native definition word lookup, CoreText font attribute, `showDefinition(for:at:)` dispatch, and visible native definition UI pixels. This does not prove external Launch Services handler delivery or unrelated OS notification/audio/dock effects.",
        guard_tier="Tier 3",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_live_quicklook_definition.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2B2B2B2B3C",
        behavior="remaining OS-controlled notification, audible bell, dock-attention, and external URL-handler GUI effects",
        ghostty_reference="Pinned Ghostty macOS native notification, audible bell, dock-attention, and external URL-handler behavior",
        roastty_reference="`roastty/macos/Sources` native notification, audible bell, dock-attention, and `NSWorkspace.open` handling",
        family="notifications",
        status="Gap",
        evidence=(
            "Experiment 186 eliminates the old broad notification/link/bell "
            "catch-all by splitting completed live app dispatch proof into "
            "`RUNTIME-012B2B2B2B2B3A` and `RUNTIME-012B2B2B2B2B3B`. Experiment "
            "187 further splits live native context-menu construction into "
            "`RUNTIME-012B2B2B2B2B3C1` and controlled URL-opening request "
            "construction into `RUNTIME-012B2B2B2B2B3C2`. Experiment 188 "
            "splits live regular-link hover dispatch, cursor-shape request, "
            "and exact hovered-URL app routing into "
            "`RUNTIME-012B2B2B2B2B3C3`. Experiment 189 splits live copied "
            "SwiftUI URL hover banner display pixels into "
            "`RUNTIME-012B2B2B2B2B3C4`. Experiment 190 splits live copied "
            "bell title prefix state and border overlay pixels into "
            "`RUNTIME-012B2B2B2B2B3C5`. Experiment 191 splits real "
            "OS-rendered link cursor pixels into "
            "`RUNTIME-012B2B2B2B2B3C6`. Experiment 192 splits live inactive-app "
            "Dock attention request dispatch and badge authorization capture "
            "into `RUNTIME-012B2B2B2B2B3C7`. Experiment 193 splits live "
            "Quick Look/native definition word lookup, CoreText font attribute, "
            "`showDefinition(for:at:)` dispatch, and visible native definition "
            "UI pixels into `RUNTIME-012B2B2B2B2B3C8`. The remaining unproven behavior is "
            "limited to OS-controlled GUI effects that the current VM run did "
            "not expose deterministically."
        ),
        missing_evidence="Still need deterministic proof for actual OS notification delivery/banner/sound after authorization is available, audible bell output, OS-visible dock-attention bounce/state beyond AppKit request dispatch, and external Launch Services handler delivery.",
        guard_tier="Tier 3",
        guard_command="TBD by future focused OS/native GUI experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-013",
        behavior="platform-specific or unsupported runtime effects",
        ghostty_reference="Pinned Ghostty GTK/Linux/platform-specific config runtime behavior",
        roastty_reference="Roastty macOS app and libroastty runtime",
        family="platform",
        status="Oracle complete",
        evidence=(
            "`platform-runtime-classification.md` accounts for every `gtk-*`, "
            "`linux-*`, and `macos-*` canonical option from the regenerated "
            "config inventory. GTK and Linux runtime effects are marked not "
            "applicable to Roastty's macOS runtime; `macos-option-as-alt` points "
            "to existing key translation guards; `macos-applescript` points to "
            "`RUNTIME-011B2A`; the broader macOS app walkthrough residual is "
            "closed by completed `RUNTIME-011B2B` evidence."
        ),
        missing_evidence="None for platform-specific runtime classification; broader macOS app behavior is covered by completed RUNTIME-011B2B evidence.",
        guard_tier="Tier 0",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/platform_runtime_classification.py --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/platform-runtime-classification.md`",
    ),
    RuntimeRow(
        id="RUNTIME-014",
        behavior="accepted runtime divergences cross-link",
        ghostty_reference="Pinned Ghostty runtime helpers and public ABI behavior",
        roastty_reference="`issues/0805-roastty-ghostty-parity/divergences.md`",
        family="divergence",
        status="Intentional divergence",
        evidence=(
            "`DIV-001` records `roastty_translate` identity behavior and "
            "`DIV-002` records unsupported benchmark CLI behavior, with ABI "
            "guards. These are accepted non-parity runtime outcomes."
        ),
        missing_evidence="None for currently accepted runtime divergences.",
        guard_tier="Tier 0",
        guard_command="Inspect `issues/0805-roastty-ghostty-parity/divergences.md` and run the ABI harness listed there.",
    ),
]

EXPECTED_IDS = [
    "RUNTIME-001",
    "RUNTIME-002",
    "RUNTIME-003",
    "RUNTIME-004A",
    "RUNTIME-004B",
    "RUNTIME-004C",
    "RUNTIME-004D",
    "RUNTIME-004E",
    "RUNTIME-004F",
    "RUNTIME-004G",
    "RUNTIME-004H",
    "RUNTIME-005",
    "RUNTIME-006",
    "RUNTIME-007A",
    "RUNTIME-007B1",
    "RUNTIME-007B2A",
    "RUNTIME-007B2B1",
    "RUNTIME-007B2B2A",
    "RUNTIME-007B2B2B1",
    "RUNTIME-007B2B2B2A",
    "RUNTIME-007B2B2B2B1",
    "RUNTIME-007B2B2B2B2",
    "RUNTIME-008A",
    "RUNTIME-008B1",
    "RUNTIME-008B2A",
    "RUNTIME-008B2B1",
    "RUNTIME-008B2B2A",
    "RUNTIME-008B2B2B1",
    "RUNTIME-008B2B2B2A",
    "RUNTIME-008B2B2B2B1",
    "RUNTIME-008B2B2B2B2A",
    "RUNTIME-008B2B2B2B2B4",
    "RUNTIME-008B2B2B2B2B1",
    "RUNTIME-008B2B2B2B2B2",
    "RUNTIME-008B2B2B2B2B3",
    "RUNTIME-008B2B2B2B2C",
    "RUNTIME-008B2B2B2B2D",
    "RUNTIME-009A",
    "RUNTIME-009B1",
    "RUNTIME-009B2A",
    "RUNTIME-009B2B1",
    "RUNTIME-009B2B2A",
    "RUNTIME-009B2B2B1",
    "RUNTIME-009B2B2B2",
    "RUNTIME-009B2B2B3A",
    "RUNTIME-009B2B2B3B1",
    "RUNTIME-009B2B2B3B2A",
    "RUNTIME-009B2B2B3B2B1",
    "RUNTIME-009B2B2B3B2B2A",
    "RUNTIME-009B2B2B3B2B2B1",
    "RUNTIME-009B2B2B3B2B2B2A",
    "RUNTIME-009B2B2B3B2B2B2B1",
    "RUNTIME-009B2B2B3B2B2B2B2",
    "RUNTIME-009B2B2B3B2B2B2B3",
    "RUNTIME-010A",
    "RUNTIME-010B1",
    "RUNTIME-010B2A",
    "RUNTIME-010B2B1",
    "RUNTIME-010B2B2A",
    "RUNTIME-010B2B2B1",
    "RUNTIME-010B2B2B2",
    "RUNTIME-011A",
    "RUNTIME-011B1",
    "RUNTIME-011B2A",
    "RUNTIME-011B2B",
    "RUNTIME-011B2C",
    "RUNTIME-011B2D",
    "RUNTIME-011B2E",
    "RUNTIME-011B2F",
    "RUNTIME-011B2G",
    "RUNTIME-011B2H",
    "RUNTIME-011B2I",
    "RUNTIME-011B2J",
    "RUNTIME-012A",
    "RUNTIME-012B1",
    "RUNTIME-012B2A",
    "RUNTIME-012B2B1",
    "RUNTIME-012B2B2A",
    "RUNTIME-012B2B2B1",
    "RUNTIME-012B2B2B2A",
    "RUNTIME-012B2B2B2B1",
    "RUNTIME-012B2B2B2B2A",
    "RUNTIME-012B2B2B2B2B1",
    "RUNTIME-012B2B2B2B2B2",
    "RUNTIME-012B2B2B2B2B3A",
    "RUNTIME-012B2B2B2B2B3B",
    "RUNTIME-012B2B2B2B2B3C1",
    "RUNTIME-012B2B2B2B2B3C2",
    "RUNTIME-012B2B2B2B2B3C3",
    "RUNTIME-012B2B2B2B2B3C4",
    "RUNTIME-012B2B2B2B2B3C5",
    "RUNTIME-012B2B2B2B2B3C6",
    "RUNTIME-012B2B2B2B2B3C7",
    "RUNTIME-012B2B2B2B2B3C8",
    "RUNTIME-012B2B2B2B2B3C",
    "RUNTIME-013",
    "RUNTIME-014",
]


def validate_rows(rows: list[RuntimeRow]) -> None:
    ids = [row.id for row in rows]
    if ids != EXPECTED_IDS:
        raise ValueError(f"runtime row manifest mismatch: {ids!r}")

    duplicate_ids = [item for item, count in Counter(ids).items() if count > 1]
    if duplicate_ids:
        raise ValueError(f"duplicate runtime row IDs: {duplicate_ids}")

    behaviors = [row.behavior for row in rows]
    duplicate_behaviors = [
        item for item, count in Counter(behaviors).items() if count > 1
    ]
    if duplicate_behaviors:
        raise ValueError(f"duplicate runtime behavior names: {duplicate_behaviors}")

    valid_statuses = {
        "Oracle complete",
        "Audit covered",
        "Gap",
        "Intentional divergence",
        "Not applicable",
    }
    invalid_statuses = sorted({row.status for row in rows} - valid_statuses)
    if invalid_statuses:
        raise ValueError(f"invalid runtime statuses: {invalid_statuses}")

    for row in rows:
        if not row.guard_tier or not row.guard_command:
            raise ValueError(f"missing guard field for {row.id}")
        if not row.ghostty_reference or not row.roastty_reference:
            raise ValueError(f"missing evidence anchor for {row.id}")
        if row.status == "Gap" and not row.guard_command.startswith("TBD"):
            raise ValueError(f"gap row has non-TBD guard: {row.id}")


def emit_inventory(rows: list[RuntimeRow], output: Path) -> None:
    status_counts = Counter(row.status for row in rows)
    family_counts = Counter(row.family for row in rows)

    lines = [
        "# Config Runtime/UI Effects Inventory",
        "",
        "Generated by `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`",
        "for Issue 805 CFG-223 runtime/UI effect experiments.",
        "",
        "## Counts",
        "",
        "| Category | Count |",
        "| --- | ---: |",
        f"| Runtime rows | {len(rows)} |",
        f"| Oracle complete rows | {status_counts.get('Oracle complete', 0)} |",
        f"| Intentional divergence rows | {status_counts.get('Intentional divergence', 0)} |",
        f"| Not applicable rows | {status_counts.get('Not applicable', 0)} |",
        f"| Audit covered rows | {status_counts.get('Audit covered', 0)} |",
        f"| Gap rows | {status_counts.get('Gap', 0)} |",
        "",
        "## Runtime Families",
        "",
        "| Runtime family | Count |",
        "| --- | ---: |",
    ]
    for family, count in sorted(family_counts.items()):
        lines.append(f"| {family} | {count} |")

    lines.extend(["", "## Expected Row Manifest", ""])
    lines.extend(f"- `{row_id}`" for row_id in EXPECTED_IDS)

    lines.extend(
        [
            "",
            "## Rows",
            "",
            "| ID | Behavior | Ghostty reference | Roastty reference | Family | Status | Evidence | Missing evidence | Guard tier | Guard command |",
            "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
        ]
    )
    for row in rows:
        lines.append(
            f"| {row.id} | {row.behavior} | {row.ghostty_reference} | "
            f"{row.roastty_reference} | {row.family} | {row.status} | "
            f"{row.evidence} | {row.missing_evidence} | {row.guard_tier} | "
            f"{row.guard_command} |"
        )
    output.write_text("\n".join(lines) + "\n")


def update_cfg223(
    matrix: Path,
    runtime_inventory_path: Path,
    closed_count: int,
    oracle_count: int,
    incomplete_count: int,
    gap_count: int,
) -> None:
    lines = matrix.read_text().splitlines()
    updated: list[str] = []
    found = False
    for line in lines:
        if line.startswith("| CFG-223 |"):
            found = True
            status = "Pass" if incomplete_count == 0 and gap_count == 0 else "Gap"
            notes = (
                f"Runtime inventory coverage: {oracle_count} rows Oracle complete; "
                f"{closed_count} rows closed; {incomplete_count} rows are "
                f"incomplete and {gap_count} rows are runtime gaps."
            )
            line = (
                "| CFG-223 | Runtime and UI effects | "
                "Ghostty config options that affect app, renderer, input, font, "
                "terminal, and platform behavior produce equivalent runtime effects. | "
                "Roastty runtime/UI effects are inventoried by pinned Ghostty "
                "config-driven runtime domains. | "
                f"{status} | Generated runtime/UI inventory plus matrix consistency "
                "assertion. | "
                f"`{runtime_inventory_path}` | Tier 3 | "
                "`PYTHONDONTWRITEBYTECODE=1 python3 "
                "issues/0805-roastty-ghostty-parity/config_runtime_inventory.py "
                "--output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md "
                "--matrix issues/0805-roastty-ghostty-parity/config-matrix.md` | "
                "Before closing Issue 805 and when config-driven runtime behavior changes. | "
                "CFG-223 only passes when every runtime/UI inventory row is "
                "`Oracle complete`, `Not applicable`, or an accepted documented "
                f"divergence; audit coverage alone is insufficient. | Experiment 106 | {notes} |"
            )
        updated.append(line)

    if not found:
        raise ValueError("CFG-223 row not found in config matrix")

    matrix.write_text("\n".join(updated) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--matrix", type=Path, required=True)
    args = parser.parse_args()

    rows = list(ROWS)
    validate_rows(rows)
    emit_inventory(rows, args.output)

    complete_statuses = {"Oracle complete", "Intentional divergence", "Not applicable"}
    oracle_count = sum(row.status == "Oracle complete" for row in rows)
    closed_count = sum(row.status in complete_statuses for row in rows)
    incomplete_count = sum(row.status not in complete_statuses for row in rows)
    gap_count = sum(row.status == "Gap" for row in rows)
    audit_count = sum(row.status == "Audit covered" for row in rows)
    update_cfg223(
        args.matrix,
        args.output,
        closed_count,
        oracle_count,
        incomplete_count,
        gap_count,
    )

    print(f"runtime_rows={len(rows)}")
    print(f"oracle_complete={oracle_count}")
    print(f"closed={closed_count}")
    print(f"audit_covered={audit_count}")
    print(f"incomplete={incomplete_count}")
    print(f"gap={gap_count}")
    print(f"cfg223={'Pass' if incomplete_count == 0 and gap_count == 0 else 'Gap'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
