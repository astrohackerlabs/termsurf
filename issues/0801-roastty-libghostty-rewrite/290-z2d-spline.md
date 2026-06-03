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

# Experiment 290: z2d port — the Spline cubic-Bézier flattener

## Description

The fill plotter turns a path into `Polygon` contours; a `curve_to` node is a
cubic Bézier that must be **flattened** into line segments first. z2d's `Spline`
(`vendor/z2d/src/internal/tess/Spline.zig`, derived from Cairo's
`cairo-spline.c`) does this by recursive de Casteljau subdivision until the
flattening error falls below a tolerance. It is self-contained — it depends only
on `Point` — and is the last piece before the plotters. This experiment ports
it, emitting the flattened points into a `Vec<Point>` instead of z2d's plotter
vtable (behaviorally identical).

## Upstream behavior (`Spline.zig`)

- `Spline { a, b, c, d: Point, tolerance }` (the four cubic control points and a
  pixel error tolerance).
- `decompose()`: if `a == b && c == d` (both tangents zero → a straight line),
  emit a single `line_to(d)`. Otherwise recurse via
  `decomposeInto(s1, a, tolerance²)` then emit `line_to(d)`.
- `decomposeInto(s1, start, tol)`: if `s1.errorSq() < tol`, emit `line_to(s1.a)`
  _unless_ `s1.a == start` (avoid duplicating the start), and return; else split
  with `deCasteljau()` (mutates `s1` to the first half, returns the second) and
  recurse into each half in order.
- `Knots.errorSq()`: an upper bound on the squared error of approximating the
  spline by the chord `a→d`. It projects the `b` and `c` control deltas onto the
  chord direction (clamping the projection to the chord) and returns the larger
  of the two squared residual distances. (The Cairo metric.)
- `Knots.deCasteljau()`: midpoint subdivision — `ab/bc/cd = lerpHalf` of the
  control pairs, `abbc/bccd` of those, `final` of those; sets `self` to
  `{final, bccd, cd, d}`-as-the-second… (actually sets `self` to the first half
  `{a, ab, abbc, final}` and returns the second half `{final, bccd, cd, d}`).
- `lerpHalf(a, b) = a + (b - a) / 2`; `dotSq(x, y) = x² + y²`.

## Rust mapping (`roastty/src/font/sprite/raster.rs`)

- `struct Spline { a: Point, b: Point, c: Point, d: Point, tolerance: f64 }`
  with `fn decompose(&self, out: &mut Vec<Point>)`.
- `struct Knots { a, b, c, d: Point }` with `fn error_sq(&self) -> f64` and
  `fn de_casteljau(&mut self) -> Knots` (mutates `self` to the first half,
  returns the second).
- free fns
  `fn decompose_into(s1: &mut Knots, start: Point, tolerance: f64, out: &mut Vec<Point>)`,
  `fn dot_sq(x: f64, y: f64) -> f64`,
  `fn lerp_half(a: Point, b: Point) -> Point`.
- `Point`'s derived `PartialEq` is the `equal` (exact float compare, matching
  upstream).

## Scope / faithfulness notes

- **Deferred**: the `fill_plotter`/`stroke_plotter`, the `Path`/`StaticPath`
  builder, and `Canvas::line`/`fill`/`stroke` — later z2d slices. The flattener
  emits into a `Vec<Point>` rather than z2d's `PlotterVTable` callback;
  identical behavior.
- `f64` throughout, matching z2d / Cairo.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/raster.rs`: add `Spline`, `Knots`, `decompose_into`,
   `dot_sq`, `lerp_half`.
2. Tests (deterministic):
   - `lerp_half_midpoint`: `lerp_half((0,0),(4,6)) == (2,3)`.
   - `dot_sq_value`: `dot_sq(3,4) == 25`.
   - `error_sq_offset`:
     `Knots{a:(0,0), b:(0,3), c:(4,3), d:(4,0)}.error_sq() == 9` (the control
     points are `3` off the `a→d` chord → squared residual `9`).
   - `de_casteljau_exact`: `Knots{a:(0,0), b:(0,12), c:(12,12), d:(12,0)}` after
     `de_casteljau()` becomes `{(0,0),(0,6),(3,9),(6,9)}` and returns
     `{(6,9),(9,9),(12,6),(12,0)}` (worked through `lerpHalf`).
   - `decompose_straight`:
     `Spline{a:(0,0), b:(0,0), c:(10,10), d:(10,10), tol:0.1}` (both tangents
     zero) → `out == [(10,10)]`.
   - `decompose_collinear`:
     `Spline{a:(0,0), b:(1,1), c:(2,2), d:(3,3), tol:0.1}` (all control points
     on the line) → flattens to `out == [(3,3)]` (zero error, the start is not
     re-emitted).
   - `decompose_curved`: a real arch
     `Spline{a:(0,0), b:(0,10), c:(10,10), d:(10,0), tol:0.1}` →
     `out.len() > 2`, `out.last() == (10,0)`, every point within the control
     bounding box `x∈[0,10], y∈[0,10]`, and the curve rises (`max y > 0`).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty raster
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Spline`/`Knots` reproduce z2d's de Casteljau subdivision, the Cairo `errorSq`
  metric, and the `decompose` straight-line/recursion/endpoint emission;
- the deterministic `lerp_half`/`dot_sq`/`error_sq`/`de_casteljau`/straight/
  collinear tests and the curved-arch shape test confirm faithfulness;
- the plotters, `Path` builder, and `Canvas` path methods stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the error metric needs a different shape to
match upstream exactly.

The experiment **fails** if the flattening diverges from z2d/Cairo or any public
C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**. It confirmed `decompose`, `decompose_into`, `errorSq`, `deCasteljau`,
`lerp_half`, and `dot_sq` all match `Spline.zig` (including the exact-float
`Point` equality and passing the original `start` through the recursion to avoid
re-emitting only the curve start), and that the deterministic checks recompute
correctly (the arch `error_sq = 9`; the `deCasteljau` split into
`{(0,0),(0,6),(3,9),(6,9)}` / `{(6,9),(9,9),(12,6),(12,0)}`; the zero-tangent
straight case `[(10,10)]`; the collinear case `[(3,3)]`). It judged deferring
the plotters, path builder, and `Canvas` methods a sound scope.

Review artifacts:

- Prompt: `logs/codex-review/20260603-062216-211482-prompt.md`
- Result: `logs/codex-review/20260603-062216-211482-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/raster.rs` gained `Spline` (+ `decompose`), `Knots` (+
`error_sq`, `de_casteljau`), and the `decompose_into`/`dot_sq`/`lerp_half`
helpers — the faithful recursive de Casteljau flattener emitting into a
`Vec<Point>`.

Tests (deterministic):

- `lerp_half_midpoint` (`(2,3)`), `dot_sq_value` (`25`).
- `error_sq_offset` — the `3`-off-chord arch → `9`.
- `de_casteljau_exact` — the symmetric cubic splits into
  `{(0,0),(0,6),(3,9),(6,9)}` / `{(6,9),(9,9),(12,6),(12,0)}`.
- `decompose_straight` — zero tangents → `[(10,10)]`.
- `decompose_collinear` — control points on the line → `[(3,3)]`.
- `decompose_curved` — a real arch flattens to several in-box points ending at
  `(10,0)` and rising above the chord.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty raster` → 49 passed (7 new).
- `cargo test -p roastty` → 2550 passed, 0 failed (no regressions; +7).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The cubic-Bézier flattener is in place — the last primitive before the plotters.
The next z2d slices: the `fill_plotter` (path nodes → `Polygon` contours: walk
the nodes tracking the current point, flatten `curve_to` via `Spline`, close
open subpaths, and `add_edge` each segment) and the `stroke_plotter` (a stroked
path → outline `Polygon` with the `Pen`/join/cap machinery); then a
`Canvas::fill_path`/ `line`/`stroke` that builds the polygon and calls
`fill_polygon` on the (padded) `Canvas` buffer. The simplest first consumer
remains `Canvas::line` (a 2-node butt-cap path) → the three box-drawing
diagonals (`0x2571`–`0x2573`). Alongside the sprite font remain the discovery
consumer, the UCD emoji-presentation default, codepoint overrides, the shaper,
the Nerd Font attribute table, and SVG color detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**. It confirmed `decompose`, `decompose_into`, `error_sq`,
`de_casteljau`, `lerp_half`, and `dot_sq` match the upstream control flow and
math (the Cairo error metric and the de Casteljau split ported exactly,
including the `>= d_dot_sq` clamp branches and preserving the original `start`
through the recursion), with the `Vec<Point>` emission correctly standing in for
`lineTo`, and that the seven tests are deterministic and correct. It judged the
gates clean.

Review artifacts:

- Prompt: `logs/codex-review/20260603-062448-592388-prompt.md`
- Result: `logs/codex-review/20260603-062448-592388-last-message.md`
