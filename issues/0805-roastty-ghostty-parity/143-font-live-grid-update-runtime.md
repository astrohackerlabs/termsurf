# Experiment 143: Font Live Grid Update Runtime

## Description

`RUNTIME-007B` still mixes two font concerns:

- live renderer font-grid rebuild/update after config reload or manual font-size
  changes; and
- renderer-visible font output parity for OpenType features, variations,
  thicken, metrics, shaping, fallback, glyph metrics, and pixel output.

Experiment 132 proved config-derived font grid construction and initial live
renderer grid wiring, but intentionally left live font-grid update behavior in
the gap. The current Roastty config reload path already drops the live renderer
for live views, so the next present rebuilds with the updated config. Manual
font-size actions currently update `font_size_points` and request render, but do
not explicitly drop an existing live renderer; that means an already-built
renderer can keep using the old `SharedGrid` until another rebuild trigger
happens.

This experiment will close only the live font-grid update slice: font-size
changes must invalidate the live renderer so the next live present rebuilds the
config-derived grid at the active surface font size. It will not claim visual
glyph-output parity.

## Changes

- `roastty/src/lib.rs`
  - Add a small helper that invalidates the live renderer for font-grid changes
    by setting `self.renderer = None` when the surface has a live view.
  - Call that helper from `set_font_size_points` before requesting render, so
    manual `increase_font_size`, `decrease_font_size`, `reset_font_size`, and
    `set_font_size` actions force the next live present to rebuild the font
    grid.
  - Keep the existing config-update live-view invalidation behavior intact.
  - Add focused tests whose names include `font_live_grid_update` proving:
    - manual font-size actions on live-view surfaces dirty the surface and wake
      the app;
    - same-value font-size updates are idempotent;
    - config reload still drops the live renderer/rebuild trigger for live views
      and still preserves Experiment 105's adjusted/unadjusted font-size state
      rules.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split `RUNTIME-007B` into:
    - an oracle-complete row for live renderer font-grid rebuild/update triggers
      after config reload and manual font-size changes; and
    - a remaining font renderer output gap for feature/variation, thicken,
      metric adjustment, shaping-break, fallback/shaping visual output, glyph
      metrics as seen by the renderer, and broader pixel parity.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 counts. CFG-223 must remain `Gap`.
- `issues/0805-roastty-ghostty-parity/font_live_grid_update_runtime_parity.py`
  - Add a static guard checking pinned Ghostty's config reload `setFontSize`,
    renderer `.font_grid` message, manual font-size adjusted flags, and
    Roastty's `set_font_size_points` live renderer invalidation, manual action
    tests, config reload tests, inventory split, and CFG-223 counts.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning after implementation.

## Verification

Pass criteria:

- `cargo fmt --manifest-path roastty/Cargo.toml -- --check`
- `cargo test --manifest-path roastty/Cargo.toml font_live_grid_update`
- `cargo test --manifest-path roastty/Cargo.toml surface_reload_font_size`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_live_grid_update_runtime_parity.py`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
- `prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/143-font-live-grid-update-runtime.md issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md`
- `git diff --check`

The experiment passes only if manual font-size changes explicitly invalidate the
live renderer for live-view surfaces, config reload rebuild behavior remains
covered, `RUNTIME-007B` is split without claiming visual glyph-output parity,
and CFG-223 remains open with the remaining non-terminal gaps.

## Design Review

Fresh-context adversarial design review returned **Approved** with no required
findings. The reviewer confirmed the README link, required experiment sections,
narrow scope, explicit exclusion of visual glyph-output parity, and the
technical gap: Roastty currently updates font size and requests render, while
pinned Ghostty rebuilds/pushes a font grid.
