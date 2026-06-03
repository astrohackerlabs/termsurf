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

# Experiment 293: z2d port — Face (stroke offsets + butt cap)

## Description

A **`Face`** (`vendor/z2d/src/internal/tess/Face.zig`, Cairo-derived) is the
stroke edge of a line segment: from `p0 → p1` and a line thickness it computes
the four offset corners (`p0_cw`/`p0_ccw`/`p1_cw`/`p1_ccw`) by rotating the
half-width perpendicular to the segment. The stroke plotter assembles a stroke
outline `Polygon` from faces (the segment rectangles, the joins between them,
and the end caps). This experiment ports the `Face` core — `init`/`init_single`,
the four corners, the **butt** cap, and the miter `intersect` — building on
`Slope` (292).

## Upstream behavior (`Face.zig`)

- `init(p0, p1, thickness, ctm)`: `dev_slope = normalize(Slope::init(p0, p1))`,
  then `_init`. `init_single(point, dev_slope, thickness, ctm)`:
  `_init(point, point, dev_slope, thickness, ctm)`.
- `_init`: `half_width = thickness / 2`; the perpendicular offset
  `(offset_x, offset_y) = (-dev_slope.dy · half_width, dev_slope.dx · half_width)`
  (for a CTM whose **linear part is identity** — the general CTM warps this, but
  the sprite `Canvas` uses a translation-only CTM, which leaves distances and
  directions unchanged, so this is the exact result for the sprite case);
  `offset_ccw = -offset_cw`; the corners are `pN_cw = pN + offset_cw`,
  `pN_ccw = pN + offset_ccw`.
- `intersect(in, out, clockwise)`: the Cairo miter-join intersection of two
  faces' inner edges — `in_point = clockwise ? in.p1_ccw : in.p1_cw`,
  `out_point = clockwise ? out.p0_ccw : out.p0_cw`, then the
  `result_y = (…) / (in.dx·out.dy − out.dx·in.dy)` / `result_x` formula (using
  the larger-`|dy|` slope to back out `x`).
- `capButt(clockwise)`: emit the two far corners in order — `clockwise` →
  `p1_ccw` then `p1_cw`; else `p1_cw` then `p1_ccw`.

## Rust mapping (`roastty/src/font/sprite/raster.rs`)

- `struct Face { p0, p1: Point, width: f64, dev_slope: Slope, half_width: f64, p0_cw, p0_ccw, p1_cw, p1_ccw: Point }`.
- `fn init(p0, p1, thickness) -> Face` and
  `fn init_single(point, dev_slope, thickness) -> Face` (the sprite-CTM
  specialization — no `ctm` parameter, since the linear part is identity).
- `fn intersect(in_face: &Face, out_face: &Face, clockwise: bool) -> Point` (the
  miter formula; `assert!(Slope::compare(in.dev_slope, out.dev_slope) != 0)`).
- `fn cap_butt(&self, clockwise: bool, out: &mut Vec<Point>)`.

## Scope / faithfulness notes

- **Deferred**: the **square** and **round** caps (`capSquare` uses the CTM
  `userToDeviceDistance`, a no-op for the sprite translation-CTM but still needs
  `user_slope`/`ctm` plumbing; `capRound` needs `Pen`), the `cap_p0`/`cap_p1`/
  `cap` mode dispatch (which belongs with the stroke plotter that picks the cap
  mode), the full `CapMode` enum, and the general `Transformation`/CTM
  machinery. The sprite `Canvas` strokes with **butt** caps (`Canvas::line` uses
  `line_cap_mode = .butt`), so the butt cap is what the diagonals need.
- The `_init` offset is the linear-identity result, exact for the sprite
  Canvas's translation-only CTM (`{ax:1, by:0, cx:0, dy:1, tx:pad, ty:pad}`).
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/raster.rs`: add `Face` (+ `init`/`init_single`/
   `intersect`/`cap_butt`).
2. Tests (deterministic):
   - `face_horizontal`: `Face::init((0,0),(10,0), 2.0)` → `dev_slope {1,0}`,
     `half_width 1`, corners `p0_cw (0,1)`, `p0_ccw (0,-1)`, `p1_cw (10,1)`,
     `p1_ccw (10,-1)` (a 2-thick horizontal bar).
   - `face_vertical`: `Face::init((0,0),(0,10), 2.0)` → `dev_slope {0,1}`,
     corners `p0_cw (-1,0)`, `p0_ccw (1,0)`, `p1_cw (-1,10)`, `p1_ccw (1,10)`.
   - `cap_butt_emits`: the horizontal face's `cap_butt(false, …)` pushes
     `[p1_cw (10,1), p1_ccw (10,-1)]`; `cap_butt(true, …)` pushes the reverse.
   - `intersect_corner`: in `(0,0)→(10,0)`, out `(10,0)→(10,10)`, thickness 2;
     `intersect(in, out, false)` → `(9, 1)` (the cw-side inner miter corner).
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

- `Face` reproduces z2d's perpendicular-offset corners, the butt cap emission,
  and the miter `intersect` for the sprite Canvas's (linear-identity) CTM;
- the deterministic horizontal/vertical corner, butt-cap, and intersect tests
  confirm faithfulness;
- the square/round caps, the cap-mode dispatch, `Pen`, and the general CTM stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the stroke plotter needs face data beyond the
corners/butt-cap/intersect.

The experiment **fails** if the face geometry diverges from z2d/Cairo or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**. It confirmed that with a translation-only CTM the distance transforms
ignore `tx`/`ty`, the linear part is identity, the determinant is positive, and
`_init` reduces exactly to `offset_x = -dev_slope.dy · half_width`,
`offset_y = dev_slope.dx · half_width` with `user_slope == dev_slope`; that the
corners, `intersect`, and `cap_butt` match `Face.zig`; that the recomputed tests
are correct (horizontal `(0,±1)`/`(10,±1)`, vertical `(-1,0)/(1,0)`, the
`clockwise=false` right-angle miter `(9,1)`); and that deferring the
square/round caps, the cap dispatch, `Pen`, and the general CTM is sound since
`Canvas::line` uses butt caps.

Review artifacts:

- Prompt: `logs/codex-review/20260603-064657-363837-prompt.md`
- Result: `logs/codex-review/20260603-064657-363837-last-message.md`
