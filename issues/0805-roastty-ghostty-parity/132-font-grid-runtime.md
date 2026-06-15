# Experiment 132: Font Grid Runtime

## Description

`RUNTIME-007` is still a broad CFG-223 font gap. It mixes several different
runtime concerns:

- config-derived font family and style selection;
- fallback/default font grid construction;
- `font-codepoint-map` overrides;
- synthetic style completion;
- `font-size` startup, reload, manual adjustment, and renderer grid use;
- lower-level shaping, feature, variation, thicken, metric adjustment, and
  renderer-visible glyph output behavior.

Experiment 105 already proved the surface-state `font-size`
reload/manual-adjustment rule, and the Roastty font subsystem already has
focused guards for config-derived `SharedGridSet` keys, default grid
construction, codepoint-map overrides, synthetic style completion, CoreText
fallback, shaping helpers, and glyph metrics. This experiment will not claim
every font-rendering behavior. It will split out the narrower,
already-representable runtime slice: the path from parsed font config and the
current surface font size into the initial config-derived shared font grid used
when a live renderer is created.

This experiment will split the font row:

- `RUNTIME-007A`: **Oracle complete** for config-derived font grid runtime
  construction: family/style descriptors, fallback/default grid, codepoint-map
  override, synthetic style completion, surface-state font-size
  reload/manual-adjustment semantics from Experiment 105, and initial live
  renderer use of `build_grid_from_config`.
- `RUNTIME-007B`: **Gap** for remaining font renderer output: OpenType
  feature/variation config effects, thicken/thicken-strength, metric adjustment,
  shaping-break behavior, fallback/shaping visual output, glyph metrics as seen
  by the renderer, renderer grid rebuild/update after reload/manual font-size
  changes, and full renderer-visible font changes.

This experiment will not require GUI screenshots or a full app walkthrough. The
guard tier is runtime/static because the slice is about config-to-font-grid
construction and renderer wiring, not visual pixel parity.

## Changes

- `roastty/src/font/shared_grid_set.rs`
  - Add or tighten focused tests proving config-derived font grid construction
    covers:
    - multiple configured font families preserving descriptor order;
    - exact `font-style*` names versus bold/italic category search flags;
    - `font-codepoint-map` changing codepoint resolution;
    - fallback/default grid construction for default config;
    - synthetic style completion according to `font-synthetic-style`.
- `roastty/src/lib.rs`
  - Keep the live renderer proof to static source markers unless direct Metal
    construction is stable in unit tests. The guard will prove initial renderer
    construction receives the active config and current surface font size
    through `build_grid_from_config`; renderer grid rebuild/update after later
    reload/manual font-size changes remains in `RUNTIME-007B`.
- `issues/0805-roastty-ghostty-parity/font_grid_runtime_parity.py`
  - Add a static guard checking pinned Ghostty markers:
    - `font.SharedGridSet.DerivedConfig.init(alloc, config)`;
    - `app.font_grid_set.ref(&derived_config.font, font_size, ...)`;
    - `Surface.updateConfig` calling `setFontSize`;
    - `setFontSize` sending a `.font_grid` renderer message;
    - font-size actions marking `font_size_adjusted`.
  - Check Roastty markers:
    - `DerivedConfig::from_config`;
    - `Key::new`;
    - `build_grid_from_config`;
    - `collection.complete_styles`;
    - `resolver.set_codepoint_map`;
    - `build_live_renderer` calling `build_grid_from_config(config, ...)`;
    - `Surface::apply_config` and font-size action tests from Experiment 105;
    - the new or existing shared-grid tests named in this experiment.
  - Check the runtime inventory split and CFG-223 counts.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split `RUNTIME-007` into `RUNTIME-007A` and `RUNTIME-007B`.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 summary. It must remain `Gap`.
- Existing CFG-223 static guards that hard-code current runtime row counts
  - Update expected counts after the split: 41 runtime rows, 34 Oracle complete
    rows, 36 closed rows, and 5 remaining runtime gaps.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add the experiment link and update Learnings after the result.

## Verification

Pass criteria:

- `RUNTIME-007A` is Oracle complete and cites concrete tests/guards for
  config-derived font grid construction, surface-state font-size rules, and
  initial live renderer wiring.
- `RUNTIME-007B` remains `Gap` and explicitly owns the remaining feature,
  variation, thicken, metric-adjustment, shaping-break, renderer-visible glyph,
  renderer grid rebuild/update after reload/manual font-size changes, and full
  visual font parity work.
- `CFG-223` remains `Gap`.
- Existing static parity guards remain internally consistent after the row-count
  change.

Commands:

```bash
cargo test --manifest-path roastty/Cargo.toml shared_grid_set
cargo test --manifest-path roastty/Cargo.toml complete_styles
cargo test --manifest-path roastty/Cargo.toml codepoint_override
cargo test --manifest-path roastty/Cargo.toml surface_reload_font_size
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_grid_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
cargo fmt --manifest-path roastty/Cargo.toml
cargo fmt --manifest-path roastty/Cargo.toml --check
prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/132-font-grid-runtime.md
git diff --check
```

Fail criteria:

- The experiment needs visual pixel parity to support its claim.
- The inventory claims OpenType feature/variation/thicken/metric/shaping-break
  renderer output parity without focused runtime or renderer evidence.
- `RUNTIME-007B` omits a remaining font behavior still named by the old broad
  `RUNTIME-007` gap.
- CFG-223 is marked complete.

## Design Review

**Reviewer:** Codex adversarial subagent with fresh context.

**Initial verdict:** Changes required.

The reviewer found two required issues:

- The design overclaimed live font-grid runtime coverage for font-size
  reload/manual changes. `set_font_size_points` currently updates the surface
  field and requests render, but does not prove a live renderer font grid is
  rebuilt after reload/manual font-size changes.
- The static guard plan named a nonexistent `create_live_renderer` function
  instead of the actual `build_live_renderer` function.

**Fixes:**

- Narrowed `RUNTIME-007A` to config-derived font grid construction,
  surface-state font-size semantics from Experiment 105, and initial live
  renderer grid construction. Renderer grid rebuild/update after reload/manual
  font-size changes remains in `RUNTIME-007B`.
- Corrected the planned guard marker to `build_live_renderer`.

**Re-review verdict:** Approved.

The reviewer confirmed both prior findings were resolved and reported no new
Required findings.

## Result

**Result:** Pass

Roastty now has focused guards for the config-derived font-grid runtime slice:

- `shared_grid_set_key_preserves_multiple_family_order` proves repeatable
  configured font families preserve descriptor order and exact `font-style`
  names are carried into descriptors.
- `shared_grid_set_key_builds_style_ordered_descriptors` proves style-specific
  family fields map to the correct style descriptor sections, and exact style
  names disable broad bold/italic category matching.
- `shared_grid_set_key_includes_codepoint_map` and
  `shared_grid_set_build_grid_honors_codepoint_override` prove
  `font-codepoint-map` changes the config-derived key and resolved face.
- `shared_grid_set_build_grid_from_default_config` proves default config builds
  a usable font grid.
- `shared_grid_set_build_grid_honors_disabled_synthetic_styles` proves
  `font-synthetic-style` flows into style completion.
- `font_grid_runtime_parity.py` statically checks pinned Ghostty's derived font
  config, font-grid ref, `setFontSize` renderer message, and font-size action
  markers against Roastty's `DerivedConfig`, `Key`, `build_grid_from_config`,
  codepoint-map/synthetic-style wiring, `build_live_renderer`, and Experiment
  105 font-size state guards.

`config_runtime_inventory.py` now splits the old font gap into `RUNTIME-007A` as
Oracle complete and `RUNTIME-007B` as the reduced remaining font gap. The
generated CFG-223 summary remains `Gap` with 41 runtime rows, 34 Oracle complete
rows, 36 closed rows, and 5 remaining runtime gaps.

Verification run:

```bash
cargo test --manifest-path roastty/Cargo.toml shared_grid_set
cargo test --manifest-path roastty/Cargo.toml complete_styles
cargo test --manifest-path roastty/Cargo.toml codepoint_override
cargo test --manifest-path roastty/Cargo.toml surface_reload_font_size
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_grid_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/osc7_edge_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/osc7_pwd_normalization_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/title_pwd_fallback_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/scrollback_byte_limit_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/shell_startup_rewrite_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/surface_title_runtime_parity.py
```

## Conclusion

The config-derived font-grid slice is no longer part of the CFG-223 gap. Roastty
proves parsed font family/style/codepoint-map/synthetic-style config feeds
shared font grid construction, that surface-state font-size semantics are
guarded by Experiment 105, and that initial live renderer creation uses
`build_grid_from_config`. The remaining font work is intentionally limited to
renderer-visible font output, OpenType feature/variation/thicken/metric effects,
shaping-break behavior, glyph metrics as seen by the renderer, and live renderer
font-grid rebuild/update after reload/manual font-size changes.

## Completion Review

**Reviewer:** Codex adversarial subagent with fresh context.

**Verdict:** Approved.

The reviewer reported no findings. It independently ran the requested focused
Rust tests, `font_grid_runtime_parity.py`, `config_runtime_inventory.py` to
temporary output, related static parity guards, `cargo fmt --check`, and
`git diff --check`. It also confirmed the result commit had not already been
made.
