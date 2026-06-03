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

# Experiment 284: z2d port — the Polygon/Edge tessellation core

## Description

The remaining sprite glyphs (box-drawing arcs and diagonals, circle/ellipse
pieces, geometric-shape curves, smooth mosaics) all reach the `Canvas`
anti-aliased path API (`line`/`fill`/`stroke`), which upstream backs with the
`z2d` vector-graphics library (vendored at `vendor/z2d/`). The faithful approach
is to port z2d in-tree (no external crate), the same way the rest of roastty
reimplements its dependencies. z2d's fill pipeline is
`path → fill_plotter → Polygon → multisample rasterizer → surface`; the
foundational data structure both ends share is the **`Polygon`** (a list of
oriented `Edge`s with bounding extents). This experiment ports that core in
isolation — it is small, has no rasterizer/plotter dependency, and is fully
testable on its own — before the rasterizer and plotters build on it.

## Upstream behavior (`vendor/z2d/src/internal/tess/Polygon.zig`)

- `Edge { y0, y1, x_start, x_inc: f64 }`. The `y0`/`y1` keep the original vertex
  order (used for winding `dir`), while `x_start` is the x at the **top**
  (min-y) vertex and `x_inc` is the downward slope `Δx/Δy`:
  - `dir()` → `-1` if `y0 < y1` (a "down" edge) else `+1` (an "up" edge);
  - `top()` → `min(y0, y1)`, `bottom()` → `max(y0, y1)`.
- `Polygon { edges, scale = 1, extent_top/bottom/left/right }`.
- `addEdge(p0, p1)`: scales both points by `self.scale`; if `p0.y < p1.y` builds
  a down edge (`y0=p0.y, y1=p1.y, x_start=p0.x, x_inc=(p1.x-p0.x)/(p1.y-p0.y)`),
  if `p0.y > p1.y` an up edge
  (`y0=p0.y, y1=p1.y, x_start=p1.x, x_inc=(p0.x-p1.x)/(p0.y-p1.y)`), and
  **filters out** horizontal edges (`p0.y == p1.y`). It updates the extents
  (top/bottom from the edge, left/right from the sorted scaled xs; the first
  edge seeds them, later edges min/max), and appends.
- `inBox(scale, box_width, box_height)`: rounds the extents to device pixels
  (`floor(extent/scale)` … `ceil(extent/scale)`) and returns whether that box
  intersects `(0,0)..(box_width, box_height)` — `false` for a zero-width/height
  (degenerate) or fully-outside polygon. (The upstream invalid-input branches
  are `@panic`; they become `assert!`/`unreachable!` in Rust.)

## Rust mapping (`roastty/src/font/sprite/raster.rs`, new module)

The z2d port gets its own module tree under `sprite/`. This experiment adds
`raster.rs` with the polygon core; later experiments add the rasterizer and the
plotters.

- `struct Point { x: f64, y: f64 }` — a minimal `f64` point (faithful to z2d's
  `internal/Point`; only `x`/`y` are needed here).
- `struct Edge { y0: f64, y1: f64, x_start: f64, x_inc: f64 }` with
  `fn dir(&self) -> i8`, `fn top(&self) -> f64`, `fn bottom(&self) -> f64`.
- `struct Polygon { edges: Vec<Edge>, scale: f64, extent_top: f64, extent_bottom: f64, extent_left: f64, extent_right: f64 }`
  with `fn new(scale: f64) -> Polygon` (default `scale = 1.0`, empty),
  `fn add_edge(&mut self, p0: Point, p1: Point)` (the faithful port — `assert!`
  finite, scale, orient, horizontal filter, extent update, push), and
  `fn in_box(&self, scale: f64, box_width: i32, box_height: i32) -> bool`.
- `sprite/mod.rs`: declare `pub(crate) mod raster;`.

## Scope / faithfulness notes

- **Deferred**: `addEdgesFromContour` (needs the `Contour` type from the
  plotters), the `WorkingEdgeSet` active-edge-table (`breakpoints`/`rescan`/
  `inc`/`sort`/`filter`), the `SparseCoverageBuffer`, the multisample rasterizer
  `run`, the fill/stroke plotters, and `Canvas::line`/`fill`/`stroke` — each a
  later experiment in the z2d port.
- Float arithmetic is `f64` throughout, matching z2d; `@intFromFloat(@floor(x))`
  → `x.floor() as i32`.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/raster.rs` (new): `Point`, `Edge` (+ `dir`/`top`/
   `bottom`), `Polygon` (+ `new`/`add_edge`/`in_box`).
2. `roastty/src/font/sprite/mod.rs`: declare `raster`.
3. Tests (deterministic):
   - `edge_down`: `add_edge((1,1),(3,5))` (scale 1) → one edge
     `{y0:1, y1:5, x_start:1, x_inc:0.5}`, `dir()==-1`, `top()==1`,
     `bottom()==5`.
   - `edge_up`: `add_edge((3,5),(1,1))` → `{y0:5, y1:1, x_start:1, x_inc:0.5}`,
     `dir()==1`, `top()==1`, `bottom()==5` (x_start is the lower-y vertex's x).
   - `edge_horizontal_filtered`: `add_edge((1,2),(5,2))` adds no edge.
   - `extents_seed_and_grow`: after a couple of edges,
     `extent_top/bottom/left/ right` are the min/max of the scaled vertices.
   - `scale_applied`: with `scale = 4`, `add_edge((1,1),(3,5))` scales to
     `(4,4)-(12,20)` → `x_start==4`, `x_inc==0.5`, extents in scaled coords.
   - `in_box_inside` / `in_box_degenerate` (zero-width) / `in_box_outside`: the
     bounds logic returns `true` / `false` / `false` respectively.
4. Format and test (`cargo fmt`, accept output).

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

- `Edge`/`Polygon` reproduce z2d's edge orientation (down/up/horizontal-filter),
  the `x_start`-at-top / `x_inc`-downslope representation, `dir`/`top`/`bottom`,
  the extent seeding/growth, the `scale` application, and the `in_box` bounds
  logic;
- the rasterizer, plotters, and `Canvas` path methods stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the `Edge` representation needs adjustment to
serve the (next) rasterizer faithfully.

The experiment **fails** if the polygon tessellation diverges from z2d or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**. It confirmed `Edge` preserves the original `y0`/`y1`, uses the
top-vertex `x_start` and downward `x_inc`, and matches `dir`/`top`/`bottom`;
that `addEdge`'s scaling, up/down construction, horizontal filtering, and extent
seeding/growth match upstream; that the scale-4 worked example is correct
(`x_start=4`, `x_inc=0.5`); that `inBox` matches the floor/ceil/intersection
logic with degenerate dimensions returning `false`; and that isolating this core
before the `WorkingEdgeSet`/rasterizer/plotters is a sound decomposition.

Review artifacts:

- Prompt: `logs/codex-review/20260603-054839-444008-prompt.md`
- Result: `logs/codex-review/20260603-054839-444008-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/raster.rs` (new) holds the z2d polygon core: `Point`
(`f64` x/y), `Edge` (`y0`/`y1` in original order, top-vertex `x_start`, downward
`x_inc`, with `dir`/`top`/`bottom`), and `Polygon` (`new`/`add_edge`/`in_box`).
`add_edge` scales, orients (down/up/horizontal-filter), seeds/grows the extents,
and pushes; `in_box` rounds the scaled extents to device pixels and tests the
box intersection (the upstream `@panic` branches → `assert!`). `sprite/mod.rs`
declares the module.

Tests (deterministic):

- `edge_down` — `(1,1)→(3,5)` builds `{y0:1, y1:5, x_start:1, x_inc:0.5}`,
  `dir=-1`, `top=1`, `bottom=5`.
- `edge_up` — `(3,5)→(1,1)` keeps `{y0:5, y1:1}` with `x_start=1` (the top
  vertex), `dir=+1`.
- `edge_horizontal_filtered` — a horizontal edge adds nothing.
- `extents_seed_and_grow` — the first edge seeds, a second min/maxes.
- `scale_applied` — `scale=4` scales `(1,1)→(3,5)` to `(4,4)-(12,20)`.
- `in_box_inside`/`in_box_degenerate`/`in_box_outside` — `true`/`false`/`false`.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty raster` → 8 passed.
- `cargo test -p roastty` → 2511 passed, 0 failed (no regressions; +8).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The z2d port has its foundation: the `Polygon`/`Edge` tessellation core, with
the top-vertex `x_start` / downward-`x_inc` edge representation and the
extent/`inBox` machinery the rasterizer consumes. The next z2d slices, in order:
the `WorkingEdgeSet` active-edge-table
(`breakpoints`/`rescan`/`inc`/`sort`/`filter`) and the `SparseCoverageBuffer`;
the multisample-4× rasterizer `run` (with a minimal alpha8 source-over
composite); the `fill_plotter` (path → `Polygon`) and `stroke_plotter` (stroke
outline → `Polygon`, with butt caps); then wiring `Canvas::line`/`fill`/`stroke`
— which finally unblocks the box-drawing diagonals (`0x2571`–`0x2573`), the
arcs, the circle/ellipse pieces, and the geometric curves. Alongside the sprite
font remain the discovery consumer, the UCD emoji-presentation default,
codepoint overrides, the shaper, the Nerd Font attribute table, and SVG color
detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**. It confirmed `Edge`, `add_edge`, the extent handling, `in_box`, and
the tests match `Polygon.zig` lines 1–195 for this slice, that the `@panic`
branches are appropriately represented as `assert!`, and that the
fmt/test/build/name/diff gates are clean.

Review artifacts:

- Prompt: `logs/codex-review/20260603-055050-702141-prompt.md`
- Result: `logs/codex-review/20260603-055050-702141-last-message.md`
