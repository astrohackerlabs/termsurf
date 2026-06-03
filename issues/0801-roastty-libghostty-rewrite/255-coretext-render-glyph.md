+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 255: CoreText render_glyph — rasterize into the atlas, return a Glyph

## Description

Experiment 254 proved the rasterization primitive: glyph → grayscale coverage
bitmap. This experiment turns that primitive into the faithful **core** of
upstream `renderGlyph`: it places the coverage into the glyph `Atlas` (already
ported, `font/atlas.rs`) and returns a `Glyph` (already ported, `font/glyph.rs`)
carrying the whole-pixel bearings and the atlas coordinates.

This is the un-decorated path: a single **monochrome** glyph, with faithful
sub-pixel positioning (the fractional-bearing CTM translate), but **no cell
constraints**, **no color/sbix**, **no synthetic bold**, and **no thicken/font
smoothing**. Those branches of upstream `renderGlyph` layer on in later
experiments; this slice closes the loop from glyph index to an atlas-resident
`Glyph` so the shaper/atlas wiring has a real producer.

### What upstream `renderGlyph` does (monochrome, unconstrained subset)

From `font/face/coretext.zig` (full version reviewed):

1. `rect = getBoundingRectsForGlyphs(.horizontal, &glyphs, null)` — the glyph's
   bounding rect in a bottom-left-origin, +Y-up space.
2. If `rect.size.width < 0.25 || rect.size.height < 0.25` → return a **zero
   glyph** (all fields `0`): no outline / too small.
3. Whole-pixel bearings: `px_x = floor(x)`, `px_y = floor(y)` (here `x`/`y` are
   `rect.origin.x`/`rect.origin.y`; the constraint/baseline/recenter terms are
   the deferred layers, so in this slice they are the raw origin).
4. Fractional remainder kept for sub-pixel positioning: `frac_x = x - floor(x)`,
   `frac_y = y - floor(y)`.
5. Canvas size that fits the drawn glyph including the fractional offset:
   `px_width = ceil(width + frac_x)`, `px_height = ceil(height + frac_y)`
   (`canvas_padding` is `0` because thicken is deferred).
6. Allocate a zeroed buffer (`px_width * px_height`, depth 1), create a
   `DeviceGray` bitmap context over it, set antialiasing and a white fill.
7. `translateCTM(frac_x, frac_y)` so the glyph lands at the correct sub-pixel
   position; `scaleCTM(width/rect.w, height/rect.h)` — which is **identity
   here** because constraints are deferred, so it is omitted.
8. `drawGlyphs(&glyphs, &.{ .{ .x = -rect.origin.x, .y = -rect.origin.y } }, ctx)`
   — the negated bearings put the glyph box's bottom-left at the CTM origin.
9. `region = atlas.reserve(px_width, px_height); atlas.set(region, buf)` — write
   the coverage into the atlas. **No vertical flip**: upstream copies the
   CoreGraphics buffer row-for-row; the texture orientation is the renderer's
   concern, not `renderGlyph`'s. (This corrects the speculative "flip in the
   atlas write" note from Experiment 254.)
10. `offset_x = px_x`; `offset_y = px_y + px_height` (top bearing = distance
    from the cell bottom to the top of the glyph box). Return
    `Glyph{ width: px_width, height: px_height, offset_x, offset_y, atlas_x: region.x, atlas_y: region.y }`.

### Rust mapping (`roastty/src/font/face/coretext.rs`)

**Evolve `rasterize_glyph` into the faithful rasterizer.** Experiment 254's
`rasterize_glyph` drew at the integer-rounded box with no fractional translate.
This experiment makes it faithful and self-consistent with the reported
bearings:

- `RasterizedGlyph` gains two fields: `bearing_x: i32`, `bearing_y: i32` — the
  whole-pixel bottom-left bearings (`px_x`, `px_y`). Its doc comment is
  corrected (no "flip in the atlas write"; the buffer is written to the atlas
  as-is).
- `rasterize_glyph` now:
  - computes `px_x = origin.x.floor() as i32`, `px_y = origin.y.floor() as i32`;
  - `frac_x = origin.x - origin.x.floor()`,
    `frac_y = origin.y - origin.y.floor()`;
  - `px_w = (width + frac_x).ceil() as usize`,
    `px_h = (height + frac_y).ceil() as usize`;
  - after creating the context and before drawing, calls
    `CGContext::translate_ctm(Some(&ctx), frac_x, frac_y)`;
  - draws at `(-origin.x, -origin.y)` (unchanged);
  - returns
    `RasterizedGlyph { width: px_w as u32, height: px_h as u32, bitmap, bearing_x: px_x, bearing_y: px_y }`.
  - The `< 0.25` guard, the zeroed buffer (Rust `vec![0u8; …]` already gives the
    clean fill upstream does explicitly), the white antialiased draw, and the
    `drop(ctx)` before moving `buf` are unchanged from Experiment 254.

**Add `render_glyph`:**

```rust
pub(crate) fn render_glyph(
    &self,
    atlas: &mut Atlas,
    glyph: u16,
) -> Result<Glyph, AtlasError> {
    debug_assert_eq!(atlas.format(), Format::Grayscale);
    let Some(rg) = self.rasterize_glyph(glyph) else {
        // No outline / too small: a zero glyph, matching upstream.
        return Ok(Glyph { width: 0, height: 0, offset_x: 0, offset_y: 0, atlas_x: 0, atlas_y: 0 });
    };
    let region = atlas.reserve(rg.width, rg.height)?;
    atlas.set(region, &rg.bitmap);
    Ok(Glyph {
        width: rg.width,
        height: rg.height,
        offset_x: rg.bearing_x,
        offset_y: rg.bearing_y + rg.height as i32,
        atlas_x: region.x,
        atlas_y: region.y,
    })
}
```

New imports in `coretext.rs`:
`use crate::font::atlas::{Atlas, AtlasError, Format};` and
`use crate::font::glyph::Glyph;`.

### Faithfulness and scope notes

- The geometry (bounding rect → whole-pixel bearings + fractional CTM translate
  → ceil-sized canvas → negated-bearing antialiased draw → row-for-row atlas
  write → `offset_y = px_y + px_height`) mirrors upstream's monochrome path
  exactly.
- **Deferred** (each a later experiment): cell **constraints** (`constrain` +
  the `scaleCTM` stretch and the re-centering `dx`), the **baseline** term added
  to `y` before constraining, **color/sbix** (P3 RGBA depth-4 path), **synthetic
  bold** (fill-stroke + size growth), **thicken/font-smoothing**
  (`canvas_padding`, `setShouldSmoothFonts`, gray fill from `thicken_strength`),
  and the explicit subpixel-quantization toggles. The
  `RenderOptions`/`grid_metrics` parameter is not introduced yet.
- `render_glyph` returns `Result<Glyph, AtlasError>`: the only fallible step in
  this slice is `atlas.reserve` (atlas full). Upstream's `InvalidAtlasFormat`
  depth check is represented by the `debug_assert_eq!` on the atlas format (the
  color/depth-4 path that makes the check meaningful is deferred).
- No C ABI, header, or ABI-inventory changes.

## Changes

1. `roastty/src/font/face/coretext.rs`:
   - Extend `RasterizedGlyph` with `bearing_x`/`bearing_y`; correct its doc.
   - Make `rasterize_glyph` faithful (fractional CTM translate, ceil-with-frac
     canvas, integer bearings).
   - Add `render_glyph`.
   - New imports for `Atlas`/`AtlasError`/`Format`/`Glyph`.
2. Tests in `coretext.rs` (live CoreText, macOS):
   - `render_glyph_places_m_in_atlas`: `Atlas::new(512, Format::Grayscale)`,
     `Face::new("Menlo", 32.0)`, `'M'` → glyph → `render_glyph` is `Ok(g)` with
     `g.width > 0`, `g.height > 0`, `g.offset_y > 0` (top bearing above the
     baseline), and the reserved rect inside the atlas
     (`(g.atlas_x + g.width) as usize <= 512`,
     `(g.atlas_y + g.height) as usize <= 512`).
   - `render_glyph_space_is_zero`: the space glyph → `Ok(g)` with
     `g.width == 0`, `g.height == 0`, all offsets/atlas coords `0` (no outline →
     zero glyph, no atlas reservation).
3. Format and test (`cargo fmt`, accept output).

The existing Experiment 254 tests (`rasterize_glyph_has_ink`,
`rasterize_space_is_empty_or_none`) continue to pass unchanged: they assert only
on bitmap size and ink presence, which the fractional-translate change
preserves.

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty face
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `render_glyph` rasterizes a glyph, reserves an atlas region, writes the
  coverage row-for-row (no flip), and returns a `Glyph` with the whole-pixel
  bearings (`offset_x = px_x`, `offset_y = px_y + px_height`) and the atlas
  coordinates;
- the sub-pixel fractional-bearing CTM translate is applied and the canvas is
  `ceil(size + frac)`, keeping the bearings consistent with the drawn position;
- an outline-less glyph yields a zero `Glyph` with no atlas reservation;
- a live `'M'` lands inside a `512×512` grayscale atlas with a positive top
  bearing, and the space glyph is a zero glyph;
- constraints, baseline, color, synthetic bold, and thicken are cleanly
  deferred;
- the Experiment 254 rasterization tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the objc2 `translate_ctm`/atlas API needs a
different call shape than expected.

The experiment **fails** if the bearings are inconsistent with the drawn
position, if the atlas write copies the wrong bytes, or if any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-203959-203902-prompt.md`
- Result: `logs/codex-review/20260602-203959-203902-last-message.md`

Codex confirmed the ported geometry matches upstream's monochrome path once
constraints/baseline are intentionally removed: the `px_x`/`px_y` whole-pixel
bearings, `frac_x`/`frac_y`, the `ceil(size + frac)` canvas, the
`translate_ctm(frac_x, frac_y)`, and the `(-origin.x, -origin.y)` draw position
keep the drawn coverage consistent with the reported bearings. It confirmed the
**no-vertical-flip** atlas write is correct for upstream faithfulness
(`renderGlyph` writes the CoreGraphics buffer directly; orientation is handled
downstream), that `Result<Glyph, AtlasError>` with the grayscale `debug_assert`
is the right shape while color/depth branches are deferred, and that the
zero-glyph path and `offset_y = px_y + px_height` match the scoped upstream
behavior.
