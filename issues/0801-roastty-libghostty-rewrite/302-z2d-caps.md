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

# Experiment 302: z2d port — round and square line caps

## Description

A stroked **open** path is capped at its two ends. So far the stroke pipeline
only butt-caps (cut flush at the endpoint). z2d also supports **round** caps (a
semicircle fan, the pen treated as a 180° joint) and **square** caps (extended
by the half-width). Round caps are needed by the curly underline
(`special.zig`'s `underline_curly`, `line_cap_mode = .round`); square completes
the faithful cap subsystem (`options.CapMode`). This experiment ports
`Face.capRound`/`capSquare` and threads a `CapMode` through the stroke plotter
and `Canvas::stroke_path`.

## Upstream behavior (`Face.cap`, `stroke_plotter` plotSingle/plotOpenJoined)

- `Face.cap(plotter, cap_mode, clockwise, pen)` dispatches:
  - `.butt` → `capButt`: plot `p1_ccw` then `p1_cw` (clockwise) / the reverse.
    (Already ported as `Face::cap_butt`.)
  - `.square` → `capSquare`: with `offset = user_slope · half_width` (under the
    translation-only CTM, `user_slope == dev_slope`, both normalized), plot
    `p1_ccw`, `p1_ccw + offset`, `p1_cw + offset`, `p1_cw` (clockwise) / the
    reverse — a rectangle extending the line by the half-width.
  - `.round` → `capRound`: treat the end as a 180° joint —
    `vit = pen.vertexIteratorFor(dev_slope, -dev_slope, clockwise)`; plot
    `p1_ccw`/`p1_cw`, then each pen vertex offset by `p1`, then `p1_cw`/`p1_ccw`
    — a semicircle fan around the endpoint.
- `cap_p0` reverses the face (`Face(p1, p0)`) before capping; `cap_p1` caps
  directly. `plotSingle` caps both ends (`cap_p0` then `cap_p1`,
  `clockwise = true`); `plotOpenJoined` caps the start (`cap_p0`, the polygon's
  `clockwise`) as the order-preserving prefix and the end (`cap_p1`) appended.
  All use the configured `cap_mode`.
- The pen is lazily built when `join_mode == .round` **or**
  `cap_mode == .round`.

## Rust mapping (`roastty/src/font/sprite/raster.rs`, `canvas.rs`)

- `pub(crate) enum CapMode { Butt, Round, Square }` (the upstream order).
- `Face::cap_square(&self, clockwise, out: &mut Vec<Point>)` and
  `Face::cap_round(&self, clockwise, pen: &Pen, out: &mut Vec<Point>)`, plus a
  `Face::cap(&self, cap_mode, clockwise, pen: Option<&Pen>, out)` dispatcher.
  `cap_square` uses `dev_slope · half_width` (the normalized direction); the
  existing `cap_butt` is unchanged.
- `StrokePlotter` gains `cap_mode: CapMode`; `new` builds the pen when
  `join_mode == Round || cap_mode == Round`. `plot_single` and
  `plot_open_joined` call `Face::cap(self.cap_mode, …, self.pen.as_ref(), …)`
  instead of the hardcoded `cap_butt`.
- `stroke_path` gains a `cap_mode: CapMode` parameter (its existing six test
  call sites pass `CapMode::Butt`, unchanged geometry).
- `Canvas::stroke_path` gains a `cap_mode: raster::CapMode` parameter; the box
  arcs pass `CapMode::Butt` (their existing behavior).

`CapMode`/the new fields are `pub(crate)`; `font/mod.rs`'s
`#![allow(dead_code)]` covers anything not yet called by a glyph (square caps,
until a glyph needs them).

## Scope / faithfulness notes

- **Ported**: the round and square line caps and the `CapMode` threading.
- **Deferred**: the curly-underline glyph itself (the next experiment — it draws
  a sine-ish curve and round-caps it), the closed-path stroke, and dashes.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/raster.rs`: add `CapMode`, `Face::cap_square`,
   `Face::cap_round`, `Face::cap`; thread `cap_mode` through `StrokePlotter`
   (pen build, `plot_single`, `plot_open_joined`) and `stroke_path`.
2. `roastty/src/font/sprite/canvas.rs`: add a `cap_mode` parameter to
   `Canvas::stroke_path`.
3. `roastty/src/font/sprite/draw.rs`: pass `raster::CapMode::Butt` from
   `draw_box_arc`.
4. Update the six existing `stroke_path` test calls to pass `CapMode::Butt`
   (geometry unchanged).
5. New tests (deterministic; a single segment `(0,0)→(10,0)`, thickness 2,
   half-width 1):
   - `stroke_cap_round`: `CapMode::Round` fans a semicircle at each end — the
     box bulges past the endpoints (`extent_left < -0.5`, `extent_right > 10.5`)
     with many edges (`> 4`), unlike the 2-edge butt bar (`extent [0,10]`).
   - `stroke_cap_square`: `CapMode::Square` extends the bar by the half-width —
     `extent_left == -1`, `extent_right == 11`, still 2 edges (a longer
     rectangle), distinguishing it from both butt (`[0,10]`) and round (a fan).
   - `stroke_cap_butt_unchanged`: `CapMode::Butt` reproduces the existing
     `stroke_line` bar (`extent [0,10]`, 2 edges) — the default is unchanged.
6. Format and test (`cargo fmt`, accept output).

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

- `Face::cap_round`/`cap_square` reproduce z2d's `capRound`/`capSquare` (the
  pen-fan 180° joint; the half-width rectangle extension), the `CapMode`
  threading caps both `plot_single` ends and the `plot_open_joined` start/end,
  and the pen is built when either join or cap is round;
- the round/square/butt cap tests confirm the geometry, and the six pre-existing
  `stroke_path` tests and the arc tests are unchanged under `Butt`;
- the curly-underline glyph, the closed-path stroke, and dashes stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if a cap needs closed-path or dash interplay the
open-path scope does not cover (it should not — caps are an open-path concept).

The experiment **fails** if the cap outline diverges from z2d, or any public C
API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no Required
changes**. It confirmed: `cap_square` using the normalized
`dev_slope · half_width` offset is correct under the translation-only CTM
(`user_slope == dev_slope`, the device-distance transform a no-op); `cap_round`
matches upstream (`vertex_iterator_for(dev_slope, -dev_slope, clockwise)`, the
endpoint side point, the pen vertices offset by `p1`, then the opposite side
point); the `CapMode` threading through `plot_single` and `plot_open_joined` is
correct, including `cap_p0` via the reversed face and using the polygon
`clockwise` for open joined paths; building the pen when
`join_mode == Round || cap_mode == Round` matches upstream; and including square
(a real upstream cap mode) keeps the subsystem complete. It judged the tests
sound (butt `[0,10]`/2 edges, square `[-1,11]`/2 non-horizontal edges, round a
bulging fan). No Optional findings.

Review artifacts:

- Prompt: `logs/codex-review/20260603-075726-021453-prompt.md`
- Result: `logs/codex-review/20260603-075726-021453-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/raster.rs` gained the cap subsystem:

- `CapMode { Butt, Round, Square }`.
- `Face::cap_square` (the half-width rectangle extension via the normalized
  `dev_slope · half_width` offset), `Face::cap_round` (the 180°-joint semicircle
  fan via `vertex_iterator_for(dev_slope, -dev_slope, clockwise)` offset by
  `p1`), and a `Face::cap` dispatcher; `cap_butt` unchanged.
- `StrokePlotter` gained `cap_mode`; `new` builds the pen when
  `join_mode == Round || cap_mode == Round`; `plot_single` (both ends,
  `clockwise = true`) and `plot_open_joined` (start `cap_p0` + end `cap_p1` with
  the polygon `clockwise`) now cap via
  `Face::cap(self.cap_mode, …, self.pen.as_ref(), …)`.
- `stroke_path` gained a `cap_mode` parameter.

`roastty/src/font/sprite/canvas.rs`: `Canvas::stroke_path` gained a `cap_mode`
parameter (doc updated to note the arcs pass `Butt` while callers may request
`Round`). `roastty/src/font/sprite/draw.rs`: `draw_box_arc` passes
`CapMode::Butt`.

Tests (a single segment `(0,0)→(10,0)`, thickness 2, half-width 1):

- `stroke_cap_butt_unchanged` — `Butt` is the unchanged bar: `[0,10]×[-1,1]`, 2
  edges.
- `stroke_cap_square` — `Square` extends to `[-1,11]×[-1,1]`, still 2
  non-horizontal edges (a longer rectangle).
- `stroke_cap_round` — `Round` bulges past the endpoints (`extent_left < -0.5`,
  `extent_right > 10.5`, within `[-1,11]`) with `> 4` edges (more than the butt
  bar) — the semicircle fans.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2604 passed, 0 failed (+3, no regressions; the six
  prior `stroke_path` tests and the arc tests unchanged under `Butt`).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The line caps render faithfully: `Face::cap_round`/`cap_square` reproduce z2d's
`capRound`/`capSquare`, and `CapMode` threads through the plotter and
`Canvas::stroke_path`. The stroke pipeline now handles
`move_to`/`line_to`/`curve_to` open paths with miter/round/bevel joins and
butt/round/square caps — the full open-path stroke.

The next step is the **curly underline** glyph (`special.zig`'s
`underline_curly` / the relevant codepoint): it draws a sine-ish wave of cubics
and round-caps it via `Canvas::stroke_path(…, CapMode::Round)` — the first
consumer of round caps. After that: the circle/ellipse geometric pieces, the
closed-path stroke, dashes, then the unifying sprite `has_codepoint`/draw entry
point (which fills the resolver's deferred `SpriteUnavailable` arm), the
discovery consumer, the UCD emoji-presentation default, codepoint overrides, the
shaper, the Nerd Font attribute table, and SVG color detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no Required
changes**. It confirmed `cap_square` uses the normalized
`dev_slope · half_width` offset and emits the four points in upstream order;
`cap_round` uses `vertex_iterator_for(dev_slope, -dev_slope, clockwise)` offset
by `p1` (the 180° fan); `Face::cap`/`plot_single`/`plot_open_joined` thread
`cap_mode` and the pen correctly (including the reversed-face `cap_p0`); pen
construction covers `join_mode == Round || cap_mode == Round` with curve
lazy-init intact; and `stroke_path`/`Canvas::stroke_path` carry `cap_mode` with
the arcs keeping `Butt`. One **Optional** suggestion — clarify the
`Canvas::stroke_path` doc now that it accepts a `cap_mode` — was applied (the
doc notes the arcs pass `Butt` while callers may request `Round`).

Review artifacts:

- Result review: `logs/codex-review/20260603-080212-990456-last-message.md`
