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

# Experiment 295: z2d port — the single-segment stroke

## Description

The box-drawing diagonals call `Canvas::line`, which strokes a **2-node butt-cap
path** — a single line segment. z2d's stroke plotter handles that via
`plotSingle` (`vendor/z2d/src/internal/tess/stroke_plotter.zig`): build a `Face`
for the segment, emit the start cap (`cap_p0`) then the end cap (`cap_p1`) into
a `Contour`, and convert it to a `Polygon`. With `Slope` (292), `Face` (293),
and `Contour` (294) all in place, this experiment ports that single-segment
stroke as `stroke_line(p0, p1, thickness, scale) -> Polygon` — the geometry the
diagonals need. (The multi-segment join path is deferred.)

## Upstream behavior (`stroke_plotter.plotSingle`, butt caps)

- `cap_points = Face::init(start, end, thickness)`.
- `cap_p0(cap_mode, clockwise = true)`: caps the **start** by building the
  reversed face `Face::init(end, start, thickness)` and emitting its butt cap at
  the (reversed) `p1` (= the original `start`) — i.e. `reversed.cap_butt(true)`
  pushes `reversed.p1_ccw`, `reversed.p1_cw`.
- `cap_p1(cap_mode, clockwise = true)`: `cap_points.cap_butt(true)` pushes
  `cap_points.p1_ccw`, `cap_points.p1_cw`.
- Both cap emissions go into the `outer` `Contour` (in order); then
  `result.addEdgesFromContour(outer)` builds the polygon. (The `inner` contour
  is untouched for a single segment.)

So the outer contour is
`[reversed.p1_ccw, reversed.p1_cw, cap_points.p1_ccw, cap_points.p1_cw]` — the
four corners of the stroke rectangle — closed into a polygon.

## Rust mapping (`roastty/src/font/sprite/raster.rs`)

- `fn stroke_line(p0: Point, p1: Point, thickness: f64, scale: f64) -> Polygon`:
  - `face = Face::init(p0, p1, thickness)`;
    `reversed = Face::init(p1, p0, thickness)`;
  - collect cap points: `reversed.cap_butt(true, &mut pts)` (the `cap_p0` start
    cap), then `face.cap_butt(true, &mut pts)` (the `cap_p1` end cap);
  - `outer = Contour::new(scale)`; `outer.plot(p)` for each cap point;
  - `result = Polygon::new(1.0)` (the contour already scales);
    `result.add_edges_from_contour(&outer)`; return `result`.

## Scope / faithfulness notes

- **Deferred**: the multi-segment stroke (the `outer`/`inner` contour walk,
  `join`, `plotOpenJoined`/`plotClosedJoined`, the mid-list contour insert), the
  square/round caps and `Pen`, the dasher, and `Canvas::line`/`stroke`. The
  diagonals are single butt-cap segments, so `stroke_line` is the needed slice;
  `Canvas::line` (padding + `fill_polygon`) and the diagonal dispatch are the
  next experiment.
- The `inner` contour is unused for a single segment (matching upstream's
  `assert(inner.len == 0)`).
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/raster.rs`: add `stroke_line`.
2. Tests (deterministic):
   - `stroke_horizontal`: `stroke_line((0,0),(10,0), 2.0, 1.0)` → the stroke
     rectangle's two vertical edges `{y0:1, y1:-1, x_start:0, x_inc:0}` (left)
     and `{y0:-1, y1:1, x_start:10, x_inc:0}` (right) — the horizontals filtered
     (a 2-thick bar over `x[0,10]`, `y[-1,1]`).
   - `stroke_vertical`: `stroke_line((0,0),(0,10), 2.0, 1.0)` → the two
     horizontal-bar edges (the rotated analog).
   - `stroke_diagonal`: `stroke_line((0,0),(4,4), 2.0, 1.0)` → **4** edges (the
     rotated rectangle has no axis-aligned edge), and the polygon extents
     enclose the segment (`extent_left < 0`, `extent_right > 4`, etc.).
   - `stroke_scaled`: `stroke_line((0,0),(10,0), 2.0, 4.0)` → the same shape
     with all coordinates ×4 (`x_start` 0 and 40, `y0`/`y1` ±4).
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

- `stroke_line` reproduces z2d's `plotSingle` butt-cap stroke — the
  `Face`-derived rectangle corners, the two cap emissions, and the
  contour→polygon assembly;
- the horizontal/vertical/diagonal/scaled stroke tests confirm the geometry;
- the multi-segment joins, the square/round caps, `Pen`, and `Canvas::line` stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if `Canvas::line` needs stroke data beyond the
polygon.

The experiment **fails** if the stroke geometry diverges from z2d or any public
C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**. It confirmed `cap_p0` is correctly modeled as the reversed face's
`cap_butt(true)` and `cap_p1` as the forward face's `cap_butt(true)`, yielding
the outer contour `[reversed.p1_ccw, reversed.p1_cw, face.p1_ccw, face.p1_cw]`;
that `Polygon::new(1.0)` with contour scaling is the correct no-double-scaling
path; that the horizontal case recomputes to `[(0,1),(0,-1),(10,-1),(10,1)]` →
the two vertical edges `{1,-1,0,0}`/`{-1,1,10,0}`; and that the diagonal
produces 4 edges (the rotated rectangle has no horizontal sides).

Review artifacts:

- Prompt: `logs/codex-review/20260603-065715-528567-prompt.md`
- Result: `logs/codex-review/20260603-065715-528567-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/raster.rs` gained `stroke_line` — the faithful
`plotSingle` butt-cap stroke: the segment's `Face` + the reversed face, the two
`cap_butt` emissions into the outer `Contour`, and the contour → `Polygon`
assembly.

Tests (deterministic):

- `stroke_horizontal` — `(0,0)→(10,0)` thk 2 → the two vertical edges
  `{1,-1,0,0}`/`{-1,1,10,0}` (the 2-thick bar over `x[0,10] y[-1,1]`).
- `stroke_vertical` — `(0,0)→(0,10)` → 2 edges, extents `x[-1,1] y[0,10]`.
- `stroke_diagonal` — `(0,0)→(4,4)` → 4 edges (the rotated rectangle), extents
  enclosing the segment.
- `stroke_scaled` — scale 4 → the same shape ×4 (`{4,-4,0,0}`/`{-4,4,40,0}`).

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty raster` → 70 passed (4 new).
- `cargo test -p roastty` → 2571 passed, 0 failed (no regressions; +4).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The single-segment stroke is in place: a 2-point line strokes to its outline
`Polygon`, ready for the rasterizer. The next slice is the **`Canvas::line` +
diagonal dispatch** — a `Canvas::line(p0, p1, thickness)` that applies the
padding translation to the endpoints, calls `stroke_line(…, MSAA_SCALE)`, and
`fill_polygon`s the result into the padded `Canvas` buffer; plus a
`draw_box_lines` extension (or a new `draw_box_diagonals`) dispatching
`0x2571 ╱`, `0x2572 ╲`, `0x2573 ╳` via the upstream `lightDiagonal*` geometry
(the corner-to-corner lines with the slope-overshoot). That renders the
**diagonals** end to end — the first `z2d`-backed sprite glyphs. The arcs and
circle/ellipse pieces (curves + `Pen` round joins) and the multi-segment join
stroke come after. Alongside the sprite font remain the discovery consumer, the
UCD emoji-presentation default, codepoint overrides, the shaper, the Nerd Font
attribute table, and SVG color detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**. It confirmed `stroke_line` builds the forward and reversed `Face`,
appends `reversed.cap_butt(true)` then `face.cap_butt(true)` into the scaled
outer contour, and assembles through `Polygon::new(1.0)` +
`add_edges_from_contour` (matching upstream's `before = null` append), and that
the horizontal/vertical/ diagonal/scaled tests check the right geometry and cap
ordering. It judged the gates clean.

Review artifacts:

- Prompt: `logs/codex-review/20260603-065924-026491-prompt.md`
- Result: `logs/codex-review/20260603-065924-026491-last-message.md`
