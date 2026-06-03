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

# Experiment 278: The Fraction + fill cell-geometry primitive

## Description

The next sprite families that fit the already-ported rectangle `Canvas` (no
`z2d` anti-aliased paths) are the **block elements** (`U+2580`–`U+259F`) and the
quadrant/octant glyphs. They are all built on a shared cell-geometry primitive
from `font/sprite/draw/common.zig`: the `Fraction` enum (a fraction across the
cell, with carefully-aligned `min`/`max` pixel rounding) and `fill` (fill the
rectangle between a horizontal and vertical pair of fraction lines). This
experiment ports that primitive in isolation, because its rounding logic is
subtle and deserves focused tests, before the block-element dispatch builds on
it.

## Upstream behavior (`font/sprite/draw/common.zig`)

- `Fraction` (`enum`): many named variants — several aliases mapping to the same
  value (`start`/`left`/`top`/`zero` → `0.0`; `quarter`/`one_quarter`/
  `two_eighths` → `0.25`; `half`/`one_half`/`two_quarters`/`four_eighths`/
  `center`/`middle` → `0.5`; `end`/`right`/`bottom`/`one`/`full` → `1.0`; plus
  the eighths/thirds). `fraction()` returns the `f64` value.
- `min(size)`: the **left/top** pixel for this fraction —
  `size - round((1 - fraction) * size)`. It rounds the _complementary_ fraction
  from the far edge so that adjacent blocks tile evenly. (Doc example:
  `size = 7`, `half.min = 7 - round(0.5·7) = 7 - 4 = 3`.)
- `max(size)`: the **right/bottom** pixel — `round(fraction * size)` directly.
  (`size = 7`, `half.max = round(3.5) = 4`.) The asymmetry makes `start→half`
  (`0→4`) and `half→end` (`3→7`) both 4px.
- `float(size)`: `fraction * size` (unrounded, for path drawing).
- `fill(metrics, canvas, x0, x1, y0, y1)`:
  `canvas.box(x0.min(cell_width), y0.min(cell_height), x1.max(cell_width), y1.max(cell_height), .on)`.

## Rust mapping (`roastty/src/font/sprite/draw.rs`)

The shared cell primitives `Thickness`/`hline`/`vline`/`hline_middle`/
`vline_middle` already live in `draw.rs` (alongside the box-drawing code), so
`Fraction`/`fill` join them there.

- `enum Fraction { Start, Left, Top, Zero, Eighth, OneEighth, TwoEighths, ThreeEighths, FourEighths, FiveEighths, SixEighths, SevenEighths, Quarter, OneQuarter, TwoQuarters, ThreeQuarters, Third, OneThird, TwoThirds, Half, OneHalf, Center, Middle, End, Right, Bottom, One, Full }`
  — every upstream variant (distinct Rust variants; the aliases collapse to the
  same value only in `fraction()`).
- `fn fraction(self) -> f64`: the `match` returning the upstream values.
- `fn min(self, size: u32) -> i32`:
  `let s = size as f64; (s - ((1.0 - self.fraction()) * s).round()) as i32` (the
  subtracted term is integer-valued, so the `as i32` truncation is exact —
  matching Zig's `@intFromFloat`).
- `fn max(self, size: u32) -> i32`:
  `(self.fraction() * size as f64).round() as i32`.
- `fn float(self, size: u32) -> f64`: `self.fraction() * size as f64`.
- `fn fill(metrics: &Metrics, canvas: &mut Canvas, x0: Fraction, x1: Fraction, y0: Fraction, y1: Fraction)`:
  `canvas.box(x0.min(cell_width) , y0.min(cell_height), x1.max(cell_width), y1.max(cell_height), Color::ON)`.

## Scope / faithfulness notes

- **Deferred**: the `Fraction::{eighths,quarters,thirds,halves}` index arrays
  (used by the octant/eighth-block families to map `i/N` → `Fraction`) — added
  when a consumer needs them. The block-element dispatch, `Shade`, `Alignment`,
  and `Quads` are the next experiment. The `z2d`-based primitives (arcs,
  diagonals) remain deferred.
- Rust round-half-away-from-zero (`f64::round`) matches Zig `@round`, so the
  pixel rounding is identical.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/draw.rs`: add the `Fraction` enum (+ `fraction`,
   `min`, `max`, `float`) and `fill`.
2. Tests (deterministic; the Experiment 275 fixture and small explicit sizes):
   - `fraction_values`: `Zero → 0.0`, `OneEighth → 0.125`, `TwoEighths → 0.25`,
     `Quarter → 0.25`, `ThreeEighths → 0.375`, `Half → 0.5`, `Center → 0.5`,
     `TwoThirds → 2.0/3.0`, `SevenEighths → 0.875`, `Full → 1.0`, and an alias
     check (`Start/Left/Top/Zero` all `0.0`; `End/Right/Bottom/One/Full` all
     `1.0`).
   - `min_max_even_tiling`: the upstream doc example — `Half.min(7) == 3`,
     `Half.max(7) == 4`, `Zero.min(7) == 0`, `Full.max(7) == 7`; and the
     evenness `Half.max(7) - Zero.min(7) == 4` and
     `Full.max(7) - Half.min(7) == 4`.
   - `min_max_exact_half`: `Half.max(8) == 4`, `Half.min(8) == 4` (clean split);
     `Half.max(9) == 5` (`round(4.5)` away from zero), `Half.min(9) == 4`.
   - `fill_top_left_quadrant`: `fill(Zero, Half, Zero, Half)` on the fixture
     (`9×18`) inks `x ∈ [0, 5) , y ∈ [0, 9)` — `inked(0,0)`, `inked(4,8)`,
     `!inked(5,0)`, `!inked(0,9)`.
   - `fill_bottom_right_quadrant`: `fill(Half, Full, Half, Full)` inks
     `x ∈ [4, 9), y ∈ [9, 18)` (the complementary corner, abutting the TL split
     with no overlap or gap at the seam).
3. Format and test (`cargo fmt`, accept output).

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

- `Fraction::fraction`/`min`/`max`/`float` reproduce the upstream values and the
  complementary-rounding `min` vs direct-rounding `max`, and `fill` boxes the
  region between the fraction lines;
- the even-tiling and exact-half rounding tests confirm the seam behavior;
- the index arrays and the block/`Shade`/`Alignment` consumers stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the `min`/`max` rounding needs a different
float-cast shape to match upstream exactly.

The experiment **fails** if the fraction/rounding semantics diverge from
upstream or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**. It confirmed the `Fraction` variant set and `fraction()` alias
mapping match upstream (eighths, thirds, quarter/half/full aliases), that
`min`/`max` use the same complementary/direct rounding formulas with a safe
`as i32` (the rounded results are integer-valued within cell bounds), that
`f64::round` matches Zig `@round` for these non-negative operands, and that
`fill` matches `box(x0.min(w), y0.min(h), x1.max(w), y1.max(h), on)`. It
recomputed and confirmed every test expectation (`Half.min/max` at 7/8/9 and the
two quadrant fills) and that the 1px odd-width half overlap is
upstream-intentional and correctly acknowledged.

(The design gate was retried across a sustained Codex backend rate-limit; it
succeeded after a cooldown.)

Review artifacts:

- Prompt: `logs/codex-review/20260603-005059-839836-prompt.md`
- Result: `logs/codex-review/20260603-005059-839836-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/draw.rs` gained the `Fraction` enum (all upstream
variants, with `fraction`/`min`/`max`/`float`) and the `fill` free function. The
`min` uses the complementary-rounding formula
`size - round((1 - fraction) * size)` and `max` the direct
`round(fraction * size)`; `fill` boxes
`(x0.min(w), y0.min(h))..(x1.max(w), y1.max(h))` with `Color::ON`.

Tests (deterministic):

- `fraction_values` — the per-variant `f64` values and the alias collapses
  (`Start/Left/Top/Zero → 0`; `End/Right/Bottom/One/Full → 1`), plus
  `Half.float(7) == 3.5`.
- `min_max_even_tiling` — the upstream doc example: `Half.min(7)=3`,
  `Half.max(7)=4`, and both `start→half` and `half→end` spans are 4px.
- `min_max_exact_half` — `Half.min/max(8)=4` (clean even split); `Half.max(9)=5`
  (`round(4.5)` away from zero), `Half.min(9)=4`.
- `fill_top_left_quadrant` — `fill(Zero,Half,Zero,Half)` on `9×18` → `x[0,5)`,
  `y[0,9)` (exact row/col spans).
- `fill_bottom_right_quadrant` — `fill(Half,Full,Half,Full)` → `x[4,9)`,
  `y[9,18)`.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty sprite` → 42 passed (5 new).
- `cargo test -p roastty` → 2468 passed, 0 failed (no regressions; +5).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The shared `Fraction`/`fill` cell-geometry primitive is ported and its subtle
complementary-rounding (which makes adjacent half-blocks tile to equal widths)
is pixel-verified. This is the foundation the rectangle-based sprite families
build on. The next experiment is the **block elements** (`U+2580`–`U+259F`):
`Shade`/`Alignment`/`Quads` plus
`blockShade`/`block`/`fullBlockShade`/`quadrant` and the `draw2580_259F`
dispatch — all rect/`fill`-based, no `z2d`. After that come the remaining
rect-based families (eighth blocks/octants, geometric shapes that don't need
curves) and, separately, the larger `z2d` anti-aliased-path port that the arcs
and diagonals require. Alongside the sprite font remain the discovery consumer,
the UCD emoji-presentation default, codepoint overrides, the shaper, the Nerd
Font attribute table, and SVG color detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**. It confirmed `Fraction` includes every upstream variant with the
exact `fraction()` alias/value groups, that `min`/`max`/`float` match the Zig
formulas with exact `as i32` casts, and that `fill` matches
`box(x0.min(w), y0.min(h), x1.max(w), y1.max(h), Color::ON)`. It judged the
tests to cover the critical rounding and geometry (`Half.min/max` at 7/8/9,
`float(7)=3.5`, the alias collapses, and the exact quadrant-fill spans) and the
verification clean.

Review artifacts:

- Prompt: `logs/codex-review/20260603-005437-221547-prompt.md`
- Result: `logs/codex-review/20260603-005437-221547-last-message.md`
