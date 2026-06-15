# Experiment 133: Renderer Knobs Runtime

## Description

`RUNTIME-008B` still groups visible renderer effects into one broad gap:
opacity, blur, padding, cursor style shape/rendering, window padding color,
custom shader output, and other renderer-visible behavior. That is too broad for
one experiment.

Roastty already has deterministic renderer unit coverage for a narrower slice:
config-derived render knobs and row/cell background behavior. This experiment
will split out the slice that can be proven without GUI screenshots:

- `FrameRenderKnobs::from_config` sourcing `background-opacity`,
  `background-opacity-cells`, `faint-opacity`, `cursor-opacity`,
  `window-padding-color`, `font-thicken`, and `font-thicken-strength`;
- Ghostty-style renderer-use clamping for `background-opacity` before it reaches
  cell opacity math;
- background cell opacity behavior when `background-opacity-cells` is enabled or
  disabled;
- padding-extension row decisions and `window-padding-color` refinement logic;
- cursor overlay alpha sourcing from `cursor-opacity`, but not full cursor shape
  pixel rendering.

This experiment will split the renderer-visible row:

- `RUNTIME-008B1`: **Oracle complete** for deterministic render knob sourcing,
  background/faint/cursor opacity conversion and clamping,
  background-opacity-cells behavior, window-padding-color padding-extension
  decisions, and thicken knob sourcing.
- `RUNTIME-008B2`: **Gap** for remaining renderer-visible behavior: background
  blur, visible window/background opacity in the real compositor, window padding
  layout pixels, cursor style shape/rendering pixels, custom shader output, and
  broader GUI/pixel parity.

This experiment will not claim visual/pixel parity. It will prove the
config-to-renderer-state and cell/padding decision logic that can be tested
deterministically.

## Changes

- `roastty/src/renderer/frame_renderer.rs`
  - Add or tighten focused tests proving `FrameRenderKnobs::from_config` sources
    renderer-visible config values:
    - `background-opacity`;
    - `background-opacity-cells`;
    - `background-opacity` clamping to `0.0..1.0` at renderer use, matching
      pinned Ghostty's `renderer/generic.zig`;
    - `faint-opacity` clamp/ceil conversion;
    - `cursor-opacity` clamp/ceil conversion into cursor overlay alpha only;
    - `window-padding-color`;
    - `font-thicken` and `font-thicken-strength`.
  - Keep full cursor shape rendering outside this slice.
- `roastty/src/renderer/cell.rs`
  - Reuse or tighten existing background-opacity-cells tests proving explicit
    background cells receive per-cell opacity, default backgrounds stay
    transparent, selected/inverse cells remain opaque, and the feature-off path
    is unchanged.
- `roastty/src/renderer/frame_rebuild.rs`
  - Reuse or tighten existing padding-extension tests proving
    `window-padding-color` values drive padding extension and row refinement
    behavior.
- `issues/0805-roastty-ghostty-parity/renderer_knobs_runtime_parity.py`
  - Add a static guard checking pinned Ghostty markers:
    - `renderer/generic.zig` consumes `background-opacity`,
      `background-opacity-cells`, `cursor-opacity`, `faint-opacity`, and
      `window-padding-color`;
    - `renderer/generic.zig` derives `font_thicken` and `font_thicken_strength`;
    - `renderer/generic.zig` applies background-opacity-cells and cursor opacity
      in draw paths;
    - `renderer/cell.zig` contains padding-extension behavior.
  - Check Roastty markers:
    - `FrameRenderKnobs::from_config`;
    - `config.font_thicken` and `config.font_thicken_strength` in
      `FrameRenderKnobs::from_config`;
    - `from_config_sources_config_values` assertions for `knobs.thicken` and
      `knobs.thicken_strength`;
    - existing and new renderer knob tests, including
      `background_opacity_clamps_for_renderer_knob`;
    - background-opacity-cells tests in `renderer/cell.rs`;
    - padding-extension tests in `renderer/frame_rebuild.rs`;
    - the runtime inventory split and CFG-223 counts.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split `RUNTIME-008B` into `RUNTIME-008B1` and `RUNTIME-008B2`.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 summary. It must remain `Gap`.
- Existing CFG-223 static guards that hard-code current runtime row counts
  - Update expected counts after the split: 42 runtime rows, 35 Oracle complete
    rows, 37 closed rows, and 5 remaining runtime gaps.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add the experiment link and update Learnings after the result.

## Verification

Pass criteria:

- `RUNTIME-008B1` is Oracle complete and cites concrete deterministic tests for
  render knob sourcing, opacity conversion/clamping, background-opacity-cells
  behavior, and window-padding-color padding-extension behavior.
- `RUNTIME-008B2` remains `Gap` and explicitly owns remaining blur, real
  compositor opacity, window padding layout pixels, cursor style shape/rendering
  pixels, custom shader output, and broader GUI/pixel parity.
- `CFG-223` remains `Gap`.
- Existing static parity guards remain internally consistent after the row-count
  change.

Commands:

```bash
cargo test --manifest-path roastty/Cargo.toml from_config_sources_config_values
cargo test --manifest-path roastty/Cargo.toml background_opacity_clamps_for_renderer_knob
cargo test --manifest-path roastty/Cargo.toml from_config_sources_opacity_options
cargo test --manifest-path roastty/Cargo.toml cursor_opacity_clamps_to_cursor_overlay_alpha_only
cargo test --manifest-path roastty/Cargo.toml rebuild_bg_row_background_opacity_cells
cargo test --manifest-path roastty/Cargo.toml refine_padding_extend_rows
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/renderer_knobs_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
cargo fmt --manifest-path roastty/Cargo.toml
cargo fmt --manifest-path roastty/Cargo.toml --check
prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/133-renderer-knobs-runtime.md
git diff --check
```

Fail criteria:

- The inventory claims full renderer visual/pixel parity for opacity, blur,
  cursor style rendering, window padding layout, or custom shader output.
- The new complete row relies only on parser/default evidence instead of
  renderer-state or renderer-decision tests.
- `RUNTIME-008B2` omits any remaining renderer-visible behavior from the old
  broad `RUNTIME-008B` gap.
- CFG-223 is marked complete.

## Design Review

**Reviewer:** Codex adversarial subagent with fresh context.

**Initial verdict:** Changes required.

The reviewer found two required issues:

- The planned static guard omitted pinned Ghostty markers for
  `font-thicken`/`font-thicken-strength`, even though the complete slice claimed
  thicken knob sourcing.
- The design did not require proof that renderer `background-opacity` derivation
  clamps out-of-range values like pinned Ghostty. Pinned Ghostty clamps
  `background-opacity` to `0.0..1.0` in `renderer/generic.zig`, while Roastty's
  current `FrameRenderKnobs::from_config` stores the raw parsed value.

**Fixes:**

- Added explicit Ghostty and Roastty thicken marker coverage to the planned
  static guard.
- Added a required background-opacity renderer-clamp implementation/test to the
  experiment scope and verification commands.

**Re-review verdict:** Approved.

The reviewer confirmed the remaining required finding is resolved because the
plan now explicitly requires Roastty markers for `config.font_thicken`,
`config.font_thicken_strength`, and thicken-focused
`from_config_sources_config_values` assertions. The reviewer reported no new
required findings.
