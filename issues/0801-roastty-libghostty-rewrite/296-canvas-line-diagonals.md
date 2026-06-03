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

# Experiment 296: Canvas::line + the box-drawing diagonals

## Description

The payoff of the z2d port: render the box-drawing **diagonals** (`U+2571 ╱`,
`U+2572 ╲`, `U+2573 ╳`) — the first anti-aliased, `z2d`-backed sprite glyphs.
Upstream draws them with `canvas.line` over `lightDiagonal*`
(`font/sprite/draw/box.zig`). This experiment ports `Canvas::line` (wiring the
already-ported `stroke_line` + `fill_polygon` to the padded `Canvas` buffer) and
`draw_box_diagonal` (the three dispatch arms).

## Upstream behavior

- `Canvas.line(line, thickness, color)`: builds a 2-node `moveTo`/`lineTo` path
  (with the padding-translation CTM applied to the points), strokes it with butt
  caps and `line_width = thickness`, and composites the result into the surface.
- `lightDiagonalUpperRightToLowerLeft` (`U+2571 ╱`):
  `slope_x = min(1, width/height)`, `slope_y = min(1, height/width)`;
  `line((width + 0.5·slope_x, -0.5·slope_y) → (-0.5·slope_x, height + 0.5·slope_y), light, .on)`
  — the bottom-left↔top-right diagonal, overshooting the corners by `0.5·slope`
  to keep the slope true.
- `lightDiagonalUpperLeftToLowerRight` (`U+2572 ╲`):
  `line((-0.5·slope_x, -0.5·slope_y) → (width + 0.5·slope_x, height + 0.5·slope_y), light, .on)`
  — the top-left↔bottom-right diagonal.
- `lightDiagonalCross` (`U+2573 ╳`): both lines.
- The light thickness is
  `Thickness.light.height(box_thickness) = box_thickness`.

## Rust mapping

- `roastty/src/font/sprite/canvas.rs`:
  `pub(crate) fn line(&mut self, p0: raster::Point, p1: raster::Point, thickness: f64)`
  — translate `p0`/`p1` by the padding (`+ padding_x`/`+ padding_y`),
  `let poly = raster::stroke_line(p0t, p1t, thickness, raster::MSAA_SCALE as f64)`,
  then
  `raster::fill_polygon(&mut self.buf, self.width as i32, self.height as i32, &poly, raster::FillRule::NonZero)`.
  (The surface dims are the padded `width`/`height`; `fill_polygon` fills with
  the opaque `.on` source, matching `Canvas::line`'s `.on` color, and divides
  the `MSAA_SCALE`-scaled polygon back to padded device pixels.)
- `roastty/src/font/sprite/draw.rs`:
  `pub(crate) fn draw_box_diagonal(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool`
  — for `0x2571`/`0x2572`/`0x2573` compute `float_width`/
  `float_height`/`slope_x`/`slope_y` and the light thickness, then call
  `canvas.line(...)` once (or twice for the cross) with the upstream endpoints;
  `_ => false`. Update the module doc to note diagonal coverage.

## Scope / faithfulness notes

- **Deferred**: the box-drawing **arcs** and the circle/ellipse pieces (cubic
  curves + `Pen` round joins), the rest of the legacy-computing glyphs, the
  multi-segment join stroke, and the unifying sprite `has_codepoint`/draw entry
  point. The diagonals are single butt-cap segments — exactly what `stroke_line`
  renders.
- `Canvas::line` fills with the `.on` source (255) — the only color the sprite
  path methods use — via the Experiment 288 specialization.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/canvas.rs`: add `Canvas::line`.
2. `roastty/src/font/sprite/draw.rs`: add `draw_box_diagonal`; update the module
   doc.
3. Tests (deterministic enough — the AA orientation, on the fixture `9×18`
   cell):
   - `diagonal_2572_orientation` (`╲`, top-left↔bottom-right): the cell
     **center** `(4,9)` is inked (the line passes through it), and the
     **top-right** corner `(8,1)` is **not** inked (off the line).
   - `diagonal_2571_orientation` (`╱`, bottom-left↔top-right): the center
     `(4,9)` is inked and the **top-left** corner `(0,1)` is **not** inked.
   - `diagonal_2573_cross` (`╳`): the center `(4,9)` is inked (both diagonals
     cross there).
   - `canvas_line_horizontal`: a `Canvas::line((0,4),(9,4), 2.0)` on a small
     unpadded canvas inks a horizontal band centered at `y≈4` across the width,
     with the top/bottom rows empty (a direct `Canvas::line` check independent
     of the diagonal geometry).
   - `draw_box_diagonal_excludes`: `0x2500`, `'M'` return `false`, draw nothing.
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty sprite
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Canvas::line` strokes a segment into the padded buffer (padding +
  `stroke_line`
  - `fill_polygon`), and `draw_box_diagonal` renders the three diagonals with
    the correct orientation and the cross, returning `false` otherwise;
- the orientation, horizontal-line, and exclusion tests confirm end-to-end
  rendering;
- the arcs, curves, `Pen`, and other families stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the padding/scale wiring needs a different
shape to land the glyph in the cell.

The experiment **fails** if the diagonal geometry or the `Canvas::line` wiring
diverges from z2d or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**. It confirmed the diagonal endpoints, the overshoot slope math, and
the light thickness match upstream for `0x2571`/`0x2572`/`0x2573`; that
`Canvas::line`'s padding translation before `stroke_line(…, MSAA_SCALE)` is
equivalent to z2d's padding CTM for a translation-only transform, and that
filling the padded alpha8 buffer with `.on = 255` matches the specialized raster
path; that the `9×18` orientation tests are well targeted for
slash/backslash/cross direction; and that deferring arcs, curves, `Pen`, and
multi-segment joins is sound since the diagonals are single butt-cap segments.

Review artifacts:

- Prompt: `logs/codex-review/20260603-070155-094666-prompt.md`
- Result: `logs/codex-review/20260603-070155-094666-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/canvas.rs` gained `Canvas::line` (the padding
translation → `raster::stroke_line` at `MSAA_SCALE` → `raster::fill_polygon`
into the padded buffer with the `.on` source). `roastty/src/font/sprite/draw.rs`
gained `draw_box_diagonal` (the `0x2571`/`0x2572`/`0x2573` dispatch with the
upstream `lightDiagonal*` endpoints and light thickness); the module doc notes
the diagonals are the first `z2d`-rendered glyphs.

Tests (the fixture `9×18` cell + a direct `Canvas::line` check):

- `diagonal_2572_orientation` (`╲`) — the center `(4,9)` is inked, the top-right
  `(8,1)` is not.
- `diagonal_2571_orientation` (`╱`) — the center inked, the top-left `(0,1)`
  not.
- `diagonal_2573_cross` (`╳`) — the center (the crossing) inked.
- `canvas_line_horizontal` — a 2px `Canvas::line` inks a centered band,
  top/bottom rows empty.
- `draw_box_diagonal_excludes` — `0x2500`, `0x2570`, `'M'` return `false`, draw
  nothing.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty sprite` → 150 passed (5 new).
- `cargo test -p roastty` → 2576 passed, 0 failed (no regressions; +5).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The box-drawing **diagonals** (`U+2571`–`U+2573`) render end to end — the first
anti-aliased, `z2d`-backed sprite glyphs — completing the
`path → stroke_line → fill_polygon → AA alpha8 surface` pipeline for the sprite
font. The remaining `z2d`-dependent sprite glyphs are the box-drawing **arcs**
(`U+256D`–`U+2570`, cubic curves via `Canvas::line`'s curve path + the `arc`
geometry) and the circle/ellipse pieces (which also need the `Pen` round joins);
both reuse `Canvas::line`/`fill` now that the stroke pipeline exists. After the
sprite font: the unifying sprite `has_codepoint`/draw entry point (which fills
the resolver's deferred `SpriteUnavailable` arm), the discovery consumer, the
UCD emoji-presentation default, codepoint overrides, the shaper, the Nerd Font
attribute table, and SVG color detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**. It confirmed `Canvas::line` matches the `Canvas.line` slice (padding
translation, `stroke_line` at `MSAA_SCALE`, `fill_polygon` into the padded
alpha8 surface with `NonZero`/`.on`), equivalent to z2d's padding CTM for the
translation-only path, and that `draw_box_diagonal` is faithful (the same
slopes/light thickness, exact endpoints for `0x2571`/`0x2572`, both strokes for
`0x2573`, `false` otherwise) with sound orientation/horizontal/exclusion tests.
It judged the gates clean.

Review artifacts:

- Prompt: `logs/codex-review/20260603-070545-177250-prompt.md`
- Result: `logs/codex-review/20260603-070545-177250-last-message.md`
