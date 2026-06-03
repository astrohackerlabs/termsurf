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

# Experiment 285: z2d port — the WorkingEdgeSet active-edge-table

## Description

The next z2d slice: the **`WorkingEdgeSet`**
(`vendor/z2d/src/internal/tess/ Polygon.zig`) — the active-edge-table the
scanline rasterizer drives. Given a [`Polygon`] (Experiment 284), it produces,
for any sub-scanline `y`, the sorted, fill-rule-filtered list of x-crossings
that define the filled spans. It is self-contained (depends only on
`Polygon`/`Edge`) and testable in isolation.

## Upstream behavior (`Polygon.WorkingEdgeSet`)

- Holds the polygon's edges (reordered in place as scratch) plus an `x_values`
  scratch buffer; the "working" set is the active prefix.
- `breakpoints()`: the sorted, de-duplicated list of every edge's `round(top())`
  and `round(bottom())` — the scanlines at which the active set changes (built
  via binary-search insertion).
- `rescan(line_y)`: partition all edges so the **active** ones — those whose
  `top() < line_y + 0.5` and `bottom() >= line_y + 0.5` (measured at the line
  middle to break ties on point boundaries) — are at the front; the active count
  becomes the working length.
- `inc(y)`: for each active edge, compute its x-crossing at `y + 0.5`:
  `x_values[i] = round(x_start + x_inc * (y_mid - top()))`.
- `sort()`: sort the active edges by their `x_values` (co-sorting edges and
  x_values; upstream uses an unstable pdq sort).
- `filter(fill_rule)`: `even_odd` returns all active `x_values`; `non_zero`
  walks the sorted edges accumulating the winding number from each edge's
  `dir()`, keeping the x at each point where the winding number first leaves `0`
  (span start) and where it returns to `0` (span end), filtering out interior
  crossings. Returns the kept prefix — consecutive pairs are span `[start, end)`
  bounds.

## Rust mapping (`roastty/src/font/sprite/raster.rs`)

Upstream reorders the source polygon's edge array as scratch (the polygon is
discarded after rasterizing). For clean ownership, the Rust port **owns a copy**
of the edges and permutes that — behaviorally identical, since `rescan`
re-partitions the full array from scratch each call (order-independent) and
`breakpoints` is computed once up front.

- `enum FillRule { NonZero, EvenOdd }` (z2d's `options.FillRule`).
- `struct WorkingEdgeSet { edges: Vec<Edge>, active: usize, x_values: Vec<i32> }`
  with:
  - `fn new(polygon: &Polygon) -> WorkingEdgeSet` — `edges = polygon.edges`
    clone, `active = 0`, `x_values = vec![0; edges.len()]`.
  - `fn breakpoints(&self) -> Vec<i32>` — the sorted-unique `round(top)`/
    `round(bottom)` set (binary-search insert, matching upstream).
  - `fn rescan(&mut self, line_y: i32)` — the partition; sets `active`.
  - `fn inc(&mut self, y: i32)` — fills `x_values[0..active]`.
  - `fn sort(&mut self)` — unstable sort of the active prefix by `x_values`,
    co-permuting `edges` (via an index permutation or a zipped temp).
  - `fn filter(&mut self, fill_rule: FillRule) -> &[i32]` — returns
    `&x_values[0..active]` for `EvenOdd`, else the in-place winding-filtered
    prefix for `NonZero`.

## Scope / faithfulness notes

- **Deferred**: the `SparseCoverageBuffer`, the multisample rasterizer `run`,
  the fill/stroke plotters, and `Canvas::line`/`fill`/`stroke` — later z2d
  slices.
- The owned-edge-copy is a faithful behavioral equivalent of upstream's in-place
  scratch reordering (same algorithm, cleaner Rust ownership).
- `@intFromFloat(@round(x))` → `x.round() as i32`; the unstable sort matches
  z2d's pdq (tie order among equal `x_values` is unspecified either way).
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/raster.rs`: add `FillRule` and `WorkingEdgeSet` (+
   `new`/`breakpoints`/`rescan`/`inc`/`sort`/`filter`).
2. Tests (deterministic, a unit square polygon with corners
   `(2,2),(10,2),(10,10),(2,10)` at scale 1 → two vertical edges: right
   `x_start=10, dir=-1`, left `x_start=2, dir=+1`):
   - `breakpoints_sorted_unique`: `breakpoints() == [2, 10]`.
   - `rescan_active`: `rescan(5)` activates both edges (`active == 2`); a
     scanline outside (`rescan(20)`) gives `active == 0`.
   - `inc_x_crossings`: after `rescan(5)` then `inc(5)`, the active `x_values`
     are the edge crossings (`{10, 2}` in edge order, vertical so constant).
   - `sort_orders_by_x`: `sort()` orders the active `x_values` ascending
     (`[2, 10]`).
   - `filter_non_zero_span`: `rescan(5)`, `inc(5)`, `sort()`,
     `filter(NonZero) == [2, 10]` (one span — the square interior at scanline
     5).
   - `filter_even_odd_passthru`: `filter(EvenOdd)` returns all active
     `x_values`.
   - A cross/overlap case (two nested boxes or an X) confirming `non_zero`
     filters interior crossings while `even_odd` keeps them.
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

- `WorkingEdgeSet` reproduces z2d's breakpoints, the line-middle rescan
  partition, the x-crossing `inc`, the x-sorted ordering, and the even-odd /
  non-zero `filter` span logic;
- the rasterizer, plotters, and `Canvas` path methods stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the active-edge ownership model needs a
different shape to serve the (next) rasterizer faithfully.

The experiment **fails** if the active-edge-table behavior diverges from z2d or
any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**. It confirmed the breakpoints, the line-middle `rescan`, `inc`, the
x-value/edge co-sort, and both fill-rule filters match upstream
(`WorkingEdgeSet` lines 197–348); that the owned edge copy is a sound behavioral
equivalent because each `rescan` re-partitions the full edge set and
`breakpoints` does not depend on mutation order; and that the square worked
example is correct (active crossings sort to `[2, 10]`, non-zero filtering
returns one span).

Review artifacts:

- Prompt: `logs/codex-review/20260603-055309-850347-prompt.md`
- Result: `logs/codex-review/20260603-055309-850347-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/raster.rs` gained `FillRule` (`NonZero`/`EvenOdd`) and
`WorkingEdgeSet` (an owned edge copy + active count + `x_values` scratch) with
`new`/`breakpoints` (binary-search-insert sorted-unique)/`rescan` (line-middle
partition)/`inc` (x-crossing at `y+0.5`)/`sort` (unstable, co-permuting edges
and x_values)/`filter` (even-odd pass-through; non-zero winding compaction).

Tests (deterministic, a `(2,2)-(10,10)` square → right `x=10,dir-1`, left
`x=2,dir+1`):

- `breakpoints_sorted_unique` → `[2, 10]`.
- `rescan_active` → both edges active at `y=5`; none at `y=20`.
- `inc_x_crossings` → the active crossings are `{2, 10}`.
- `sort_orders_by_x` → `[2, 10]`.
- `filter_non_zero_span` → `[2, 10]` (one interior span).
- `filter_even_odd_passthru` → `[2, 10]`.
- `nested_squares_fill_rules` — two same-wound nested squares at a 4-crossing
  scanline: `EvenOdd` → `[2, 5, 11, 14]` (frame, inner carved), `NonZero` →
  `[2, 14]` (solid, interior crossings filtered).

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty raster` → 17 passed (7 new).
- `cargo test -p roastty` → 2518 passed, 0 failed (no regressions; +7).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The active-edge-table is in place: given a `Polygon`, the `WorkingEdgeSet`
produces the sorted, fill-rule-filtered x-crossings per sub-scanline, with the
non-zero winding span logic verified against a nested-square case. The remaining
z2d slices, in order: the `SparseCoverageBuffer` (the RLE coverage accumulator);
the multisample-4× rasterizer `run` (driving `WorkingEdgeSet` over the
sub-scanlines into the coverage buffer, then a minimal alpha8 source-over
composite); the `fill_plotter` (path → `Polygon`) and `stroke_plotter` (stroke
outline → `Polygon`, butt caps); then wiring `Canvas::line`/`fill`/`stroke` —
which unblocks the box-drawing diagonals (`0x2571`–`0x2573`), the arcs, the
circle/ellipse pieces, and the geometric curves. Alongside the sprite font
remain the discovery consumer, the UCD emoji-presentation default, codepoint
overrides, the shaper, the Nerd Font attribute table, and SVG color detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**. It confirmed `breakpoints`, `rescan`, `inc`, `sort`, and `filter`
match `WorkingEdgeSet` lines 197–348 (including the exact non-zero winding
compaction), that the owned edge copy is behaviorally equivalent, and that the
square/nested-square tests validate the expected span behavior. It judged the
gates clean.

Review artifacts:

- Prompt: `logs/codex-review/20260603-055552-553222-prompt.md`
- Result: `logs/codex-review/20260603-055552-553222-last-message.md`
