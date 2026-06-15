# Experiment 146: Font Thicken Render Runtime

## Description

`RUNTIME-007B2B` still groups remaining font renderer output effects. A narrow
deterministic slice inside that gap is `font-thicken` and
`font-thicken-strength` after the values leave config parsing:

- pinned Ghostty stores both values in renderer `DerivedConfig`;
- the renderer passes them into glyph `RenderOptions`;
- `SharedGrid.GlyphKey.Packed` includes them so the glyph cache separates plain
  and thickened rasterization;
- CoreText rendering adds one pixel of non-`sbix` canvas padding when thickening
  is enabled and uses `font-thicken-strength / 255` for grayscale fill/stroke
  intensity.

Roastty already has pieces of this path: `FrameRenderKnobs::from_config` sources
the config values, frame rebuild input carries `thicken` and `thicken_strength`,
`renderer::cell::render_options` passes them into `RenderOptions`, `GlyphKey`
includes both fields, and CoreText tests cover canvas padding and strength
dimming. The remaining parity work is to make this slice explicit, add any
missing non-vacuous guard for cache separation, and split the runtime inventory
so the proven deterministic thicken render mechanics are no longer hidden inside
the broad font gap.

This experiment will split `RUNTIME-007B2B`:

- `RUNTIME-007B2B1`: **Oracle complete** for deterministic non-`sbix`
  `font-thicken`/`font-thicken-strength` renderer option propagation, glyph
  cache separation, and CoreText render mechanics.
- `RUNTIME-007B2B2`: **Gap** for the remaining font renderer output effects:
  OpenType feature/variation effects, metric adjustment, fallback/shaping visual
  output, bitmap/color font thickening edge cases, glyph metrics as seen by the
  renderer, broader font pixel parity, and GUI-visible A/B font rendering.

This experiment will not claim OpenType feature or variation parity, metric
adjustment parity, fallback visual parity, glyph metric parity, or full
renderer/GUI pixel parity. It also will not duplicate Experiment 133's claim
that renderer knobs source `font-thicken`; instead it proves the lower font
rendering mechanics that happen after those knobs are built.

## Changes

- `roastty/src/font/shared_grid.rs`
  - Add or strengthen a focused test proving the glyph cache key separates the
    same glyph when `thicken` or `thicken_strength` differs, matching pinned
    Ghostty's `SharedGrid.GlyphKey.Packed` fields.
- `roastty/src/renderer/frame_renderer.rs`
  - Add a focused active-frame input test, if current coverage is not already
    sufficient, proving `font-thicken` and `font-thicken-strength` flow from
    `Config` through `FrameRenderKnobs` into row-format input.
- `roastty/src/renderer/cell.rs`
  - Keep or strengthen the existing `render_options` passthrough test proving
    `thicken` and `thicken_strength` reach glyph `RenderOptions`.
- `roastty/src/font/face/coretext.rs`
  - Keep existing CoreText tests proving thickening pads non-`sbix` glyph canvas
    by one pixel per edge and lower strength lowers grayscale fill.
- `issues/0805-roastty-ghostty-parity/font_thicken_render_runtime_parity.py`
  - Add a static guard checking pinned Ghostty's derived config markers,
    renderer glyph render-option markers, shared-grid key fields, and CoreText
    thicken/strength behavior against Roastty source markers, tests, and the
    inventory split.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split `RUNTIME-007B2B` into `RUNTIME-007B2B1` and `RUNTIME-007B2B2`.
  - Update evidence text so the remaining font gap no longer lists
    `font-thicken`/`font-thicken-strength` rendering as unproven for this
    deterministic slice.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 summary. It must remain `Gap`.
- Existing CFG-223/static runtime guards
  - Update current runtime row counts from 53/47/49/4/4 to 54/48/50/4/4.
  - Update references from `RUNTIME-007B2B` to `RUNTIME-007B2B2` where they mean
    the remaining font renderer gap.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add the experiment link and update Learnings after the result.

## Verification

Pass criteria:

- Pinned Ghostty evidence shows `font-thicken` and `font-thicken-strength` are
  renderer-derived config fields, passed into glyph render options, included in
  the shared-grid glyph cache key, and consumed by CoreText thickening/strength
  rendering.
- Roastty has non-vacuous guards for:
  - config-to-active-frame thicken values;
  - row/cell render option passthrough;
  - shared glyph cache separation by `thicken` and `thicken_strength`;
  - CoreText non-`sbix` thickening canvas padding;
  - CoreText strength dimming.
- `RUNTIME-007B2B1` is Oracle complete and cites the focused tests plus the new
  static guard.
- `RUNTIME-007B2B2` remains `Gap` for feature/variation, metric adjustment,
  fallback/shaping visual output, bitmap/color font thickening edge cases, glyph
  metrics, broader font pixel parity, and GUI-visible A/B font rendering.
- `CFG-223` remains `Gap`.

Commands:

```bash
cargo test --manifest-path roastty/Cargo.toml render_glyph_caches_by_key
cargo test --manifest-path roastty/Cargo.toml render_options_plain_letter_has_no_constraint
cargo test --manifest-path roastty/Cargo.toml render_glyph_thicken_pads_canvas
cargo test --manifest-path roastty/Cargo.toml render_glyph_strength_dims_fill
cargo test --manifest-path roastty/Cargo.toml font_thicken_render_runtime
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_thicken_render_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
for f in issues/0805-roastty-ghostty-parity/*_runtime_parity.py; do PYTHONDONTWRITEBYTECODE=1 python3 "$f" >/tmp/$(basename "$f").out || { echo FAIL:$f; cat /tmp/$(basename "$f").out; exit 1; }; done; echo all_runtime_parity_guards=pass
cargo fmt --manifest-path roastty/Cargo.toml
cargo fmt --manifest-path roastty/Cargo.toml --check
prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/146-font-thicken-render-runtime.md
git diff --check
```

Fail criteria:

- The experiment relies only on config parser/default/formatter evidence and
  does not prove renderer/glyph render mechanics.
- The glyph cache can reuse the same cached render for different `thicken` or
  `thicken_strength` settings.
- CoreText thickening or strength tests are missing, vacuous, or not tied to the
  pinned Ghostty behavior.
- The experiment promotes feature/variation effects, metric adjustment, fallback
  visual output, glyph metrics, broad font pixel parity, or GUI A/B rendering
  from the remaining gap.
- CFG-223 is marked complete.

## Design Review

**Reviewer:** Codex adversarial subagent with fresh context.

**Initial verdict:** Changes required.

The reviewer found one required issue: the initial design described CoreText
thicken padding as a non-color glyph behavior, but pinned Ghostty pads when
`opts.thicken and !sbix`. Color SVG glyphs are color but not `sbix`, while
bitmap `sbix` glyphs skip padding.

**Fix:** Narrowed the experiment to deterministic non-`sbix`
`font-thicken`/`font-thicken-strength` mechanics and explicitly kept
bitmap/color font thickening edge cases in the remaining `RUNTIME-007B2B2` gap.

**Final verdict:** Approved.

The reviewer confirmed the prior finding was resolved and no new required
findings were introduced.

## Result

**Result:** Pass.

Roastty now has explicit guards for deterministic non-`sbix`
`font-thicken`/`font-thicken-strength` render mechanics. The active-frame row
format path sources config thicken values into `FramePreparedRebuildInput`,
`renderer::cell::render_options` passes those values to glyph `RenderOptions`,
the shared glyph cache key separates the same glyph by `thicken` and
`thicken_strength`, and CoreText tests prove non-`sbix` thickening grows the
canvas and lower strength dims grayscale fill.

The CFG-223 inventory now splits `RUNTIME-007B2B` into:

- `RUNTIME-007B2B1`: **Oracle complete** for deterministic non-`sbix`
  `font-thicken`/`font-thicken-strength` renderer option propagation, glyph
  cache separation, and CoreText render mechanics.
- `RUNTIME-007B2B2`: **Gap** for remaining font renderer output effects:
  feature/variation effects, metric adjustment, fallback/shaping visual output,
  bitmap/color font thickening edge cases, glyph metrics as seen by the
  renderer, broader font pixel parity, and GUI-visible A/B font rendering.

The regenerated inventory reported:

```text
runtime_rows=54
oracle_complete=48
closed=50
audit_covered=0
incomplete=4
gap=4
cfg223=Gap
```

Verification passed:

```bash
cargo test --manifest-path roastty/Cargo.toml render_glyph_caches_by_key
cargo test --manifest-path roastty/Cargo.toml render_options_plain_letter_has_no_constraint
cargo test --manifest-path roastty/Cargo.toml render_glyph_thicken_pads_canvas
cargo test --manifest-path roastty/Cargo.toml render_glyph_strength_dims_fill
cargo test --manifest-path roastty/Cargo.toml font_thicken_render_runtime
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/font_thicken_render_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
for f in issues/0805-roastty-ghostty-parity/*_runtime_parity.py; do PYTHONDONTWRITEBYTECODE=1 python3 "$f" >/tmp/$(basename "$f").out || { echo FAIL:$f; cat /tmp/$(basename "$f").out; exit 1; }; done; echo all_runtime_parity_guards=pass
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/terminal_runtime_residual_audit.py
cargo fmt --manifest-path roastty/Cargo.toml
cargo fmt --manifest-path roastty/Cargo.toml --check
git diff --check
```

## Conclusion

The deterministic non-`sbix` font-thickening path is now proven below broad font
pixel parity. The remaining font gap is smaller and more honest: it no longer
includes the basic thicken option, cache, or CoreText grayscale mechanics, but
it still requires focused proof for feature/variation effects, metric
adjustment, fallback/shaping visual output, bitmap/color font thickening edge
cases, glyph metrics, and GUI-visible A/B font rendering.

## Completion Review

**Reviewer:** Codex adversarial subagent with fresh context.

**Initial verdict:** Changes required.

The reviewer found one required issue: the new static guard could pass while the
current README Learnings section still said remaining font work stayed in the
old broad `RUNTIME-007B2B` row.

**Fix:** Updated the current learning to point at `RUNTIME-007B2B2` and
strengthened `font_thicken_render_runtime_parity.py` to require that current
README proof-surface reference and reject the stale old-row text.

**Final verdict:** Approved.

The reviewer confirmed the prior finding was resolved and no new required
findings were introduced.
