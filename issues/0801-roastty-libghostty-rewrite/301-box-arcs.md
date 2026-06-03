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

# Experiment 301: the box-drawing arcs (`U+256D`–`U+2570`)

## Description

The payoff of the curve stroke (Experiment 300): render the box-drawing **arcs**
— `╭ U+256D`, `╮ U+256E`, `╯ U+256F`, `╰ U+2570` — the first **curved** sprite
glyphs. Upstream draws each as a `move_to`/`line_to`/`curve_to`/`line_to` path
(a straight arm into the cell, a quarter-circle corner, a straight arm out) and
strokes it with butt caps (`font/sprite/draw/box.zig`'s `arc`). This experiment
ports a general `Canvas::stroke_path` (wiring the multi-node
`raster::stroke_path` to the padded surface, mirroring `Canvas::line`) and
`draw_box_arc` (the four corner dispatch).

## Upstream behavior

- `Canvas.strokePath(path, opts, color)` strokes an arbitrary path with the
  given options. The box `arc` uses
  `StrokeOptions{ line_cap_mode = .butt, line_width = thickness }`; the
  remaining options take their z2d defaults: `line_join_mode = .miter`,
  `miter_limit = 10.0`, `tolerance = 0.1`. The stroke runs at the rasterizer
  scale (`multisample_4x` → `MSAA_SCALE = 4`). (The `line_width >= 2` gate in
  z2d's `painter.stroke` only chooses between the supplied options and these
  same defaults, so for the arcs the effective values are butt/miter/10
  regardless.)
- `arc(metrics, canvas, corner, thickness)`:
  - `thick_px = thickness.height(box_thickness)` (here `.light`);
    `float_thick = thick_px`.
  - `center_x = ((cell_width -| thick_px) / 2) + float_thick / 2` (saturating
    sub, integer div, then `+ thick/2`); `center_y` likewise with `cell_height`.
  - `r = min(cell_width, cell_height) / 2`; `s = 0.25` (control-point fraction).
  - the path, per corner (the arm in, the quarter-circle `curve_to`, the arm
    out):
    - `.tl`: `move(center_x, 0)`, `line(center_x, center_y - r)`,
      `curve((center_x, center_y - s·r), (center_x - s·r, center_y), (center_x - r, center_y))`,
      `line(0, center_y)`.
    - `.tr`: `move(center_x, 0)`, `line(center_x, center_y - r)`,
      `curve((center_x, center_y - s·r), (center_x + s·r, center_y), (center_x + r, center_y))`,
      `line(cell_width, center_y)`.
    - `.bl`: `move(center_x, cell_height)`, `line(center_x, center_y + r)`,
      `curve((center_x, center_y + s·r), (center_x - s·r, center_y), (center_x - r, center_y))`,
      `line(0, center_y)`.
    - `.br`: `move(center_x, cell_height)`, `line(center_x, center_y + r)`,
      `curve((center_x, center_y + s·r), (center_x + s·r, center_y), (center_x + r, center_y))`,
      `line(cell_width, center_y)`.
  - `strokePath(path, butt + line_width = float_thick, .on)`.
- The codepoint→corner dispatch: `0x256D → .br`, `0x256E → .bl`, `0x256F → .tl`,
  `0x2570 → .tr`.

## Rust mapping

- `roastty/src/font/sprite/canvas.rs`:
  `pub(crate) fn stroke_path(&mut self, nodes: &[raster::PathNode], thickness: f64)`
  — translate every point in every node by the padding (`+ padding_x` /
  `+ padding_y`, the upstream CTM), then
  `let poly = raster::stroke_path(&translated, thickness, raster::MSAA_SCALE as f64, 10.0, 0.1, raster::JoinMode::Miter)`,
  then `raster::fill_polygon(... NonZero)` into the padded surface with the
  `.on` source. A small `translate_node` helper offsets a `PathNode`'s points.
- `roastty/src/font/sprite/draw.rs`:
  `pub(crate) fn draw_box_arc(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool`
  — compute `thick_px`/`float_thick`/`center_x`/`center_y`/`r`/`s`, build the
  per-corner node list (`Corner { Tl, Tr, Bl, Br }` matching the four arms), and
  `canvas.stroke_path(&nodes, float_thick)`; dispatch `0x256D..=0x2570` to the
  corner, `_ => false`. Update the module doc to note arc coverage.
- **Center calc — integer arithmetic (not pure `f64`).** The center must use
  upstream's saturating-sub + integer division before the float offset, exactly:
  `center_x = (metrics.cell_width.saturating_sub(thick_px) / 2) as f64 + float_thick / 2.0`
  and `center_y` likewise with `cell_height`. For odd dimensions this differs
  from naive float math: on `9×18` with `thick = 2`, the integer form gives
  `center_x = 4.0` (`(9-2)/2 = 3`, `+ 1.0`), whereas
  `(9.0 - 2.0)/2.0 + 1.0 = 4.5` — a real geometry divergence, so the integer
  form is required.

## Scope / faithfulness notes

- **Ported**: `Canvas::stroke_path` (the general multi-node stroke wiring) and
  the four box-drawing arcs — the first curved sprite glyphs.
- **Deferred**: the circle/ellipse pieces (which also use the pen / round caps),
  round/square caps, the closed-path stroke, dashes, and the unifying sprite
  `has_codepoint`/draw entry point. The arcs are open butt-capped paths —
  exactly what the stroke pipeline now renders.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/canvas.rs`: add `Canvas::stroke_path` (+ the
   `translate_node` helper).
2. `roastty/src/font/sprite/draw.rs`: add `Corner` and `draw_box_arc`; update
   the module doc.
3. Tests (deterministic enough — the fixture `9×18` cell, `box_thickness 2`, so
   `center ≈ (4, 9)`, `r = 4.5`; each arc has a straight vertical arm at the
   center column and an empty opposite quadrant): Each test asserts the
   **vertical** arm (top/bottom center, distinguishing up/down) **and** the
   **horizontal** side arm at `y = center_y` (left/right, distinguishing the
   corner swap — per the design review, since both bottom arcs ink the
   bottom-center column, only the side arm tells `.br` from `.bl`):
   - `arc_2570_tr` (`╰`, up+right): the top-center arm `(4, 2)` is inked and the
     **right**-center `(7, 9)` is inked; the **left**-center `(1, 9)` and the
     opposite bottom-left corner `(1, 16)` are not.
   - `arc_256d_br` (`╭`, down+right): the bottom-center arm `(4, 16)` and the
     **right**-center `(7, 9)` are inked; the **left**-center `(1, 9)` and the
     top-left corner `(1, 2)` are not.
   - `arc_256e_bl` (`╮`, down+left): the bottom-center arm `(4, 16)` and the
     **left**-center `(1, 9)` are inked; the **right**-center `(7, 9)` and the
     top-right corner `(7, 2)` are not.
   - `arc_256f_tl` (`╯`, up+left): the top-center arm `(4, 2)` and the
     **left**-center `(1, 9)` are inked; the **right**-center `(7, 9)` and the
     bottom-right corner `(7, 16)` are not.
   - `draw_box_arc_excludes`: `0x2500`, `0x2571` (a diagonal), `'M'` return
     `false` and draw nothing.

   (The exact inked/empty pixels are confirmed against the actual render during
   implementation; the side-arm `x` may be nudged if the `9×18` arm lands a
   column off, but the left/right-distinguishing intent is fixed.)

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

- `Canvas::stroke_path` strokes a multi-node path into the padded buffer
  (padding
  - `raster::stroke_path` at `MSAA_SCALE` with the butt/miter/10/0.1 defaults +
    `fill_polygon`), and `draw_box_arc` renders the four arcs with the correct
    corner geometry, returning `false` otherwise;
- the four arc orientation tests and the exclusion test confirm the rendering;
- the circle/ellipse pieces, the caps, the closed-path stroke, and dashes stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the padding/scale wiring needs a different
shape to land the arc in the cell.

The experiment **fails** if the arc geometry or the `Canvas::stroke_path` wiring
diverges from z2d, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and raised two **Required**
findings, both fixed:

1. The center calculation must use upstream's **integer** arithmetic
   (`(cell_width.saturating_sub(thick_px) / 2) as f64 + float_thick / 2.0`), not
   pure `f64` math — for odd dimensions they diverge (on `9×18`, `thick 2`: the
   integer form gives `center_x = 4.0`, the float form `4.5`). The design now
   pins the integer formula explicitly.
2. The orientation tests as first written could not catch a left/right corner
   swap (both bottom arcs ink the bottom-center arm and leave the top corners
   empty). The tests now add **side-arm assertions** at `y = center_y` (e.g.
   `.br` inks right-center `(7,9)` and leaves left-center `(1,9)` empty; `.bl`
   the reverse), so each arc is pinned in both axes.

Codex confirmed the rest is faithful: the four corner path-node sequences, the
codepoint→corner dispatch (`0x256D→br`, `0x256E→bl`, `0x256F→tl`, `0x2570→tr`),
the `Canvas::stroke_path` defaults (butt cap, miter join, miter_limit 10,
tolerance 0.1, scale `MSAA_SCALE`), and the padding-translation CTM model.

Review artifacts:

- Prompt: `logs/codex-review/20260603-074930-878076-prompt.md`
- Result: `logs/codex-review/20260603-074930-878076-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/canvas.rs` gained `Canvas::stroke_path` (the
`translate_node` padding offset for every `Move`/`Line`/`Curve`/`Close` node →
`raster::stroke_path` at `MSAA_SCALE` with the z2d defaults `miter_limit 10`,
`tolerance 0.1`, `JoinMode::Miter`, butt caps → `fill_polygon` `NonZero` into
the padded surface with the `.on` source).

`roastty/src/font/sprite/draw.rs` gained `Corner { Tl, Tr, Bl, Br }` and
`draw_box_arc`: the integer-arithmetic center
(`(cell_width.saturating_sub(thick_px) / 2) as f64 + float_thick / 2.0`, and
likewise for `y`), `r = min(width, height) / 2`, `s = 0.25`, the four corner
node sequences (arm-in `line`, the quarter-circle `curve_to` with correctly
signed control points, arm-out `line`), and the dispatch `0x256D → Br`,
`0x256E → Bl`, `0x256F → Tl`, `0x2570 → Tr` (`_ => false`). The module doc notes
the arcs are the first curved sprite glyphs.

Tests (the fixture `9×18` cell, center `(4, 9)`, `r = 4.5`), each pinned on both
axes against the actual render:

- `arc_2570_tr` (`╰`) — top-center `(4,2)` + right-center `(7,9)` inked;
  left-center `(1,9)` + bottom-left `(1,16)` empty.
- `arc_256d_br` (`╭`) — bottom-center `(4,16)` + right-center `(7,9)` inked;
  left-center `(1,9)` + top-left `(1,2)` empty.
- `arc_256e_bl` (`╮`) — bottom-center `(4,16)` + left-center `(1,9)` inked;
  right-center `(7,9)` + top-right `(7,2)` empty.
- `arc_256f_tl` (`╯`) — top-center `(4,2)` + left-center `(1,9)` inked;
  right-center `(7,9)` + bottom-right `(7,16)` empty.
- `draw_box_arc_excludes` — `0x2500`, `0x2571` (a diagonal), `'M'` return
  `false` and draw nothing.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2601 passed, 0 failed (+5, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The box-drawing **arcs** (`U+256D`–`U+2570`) render end to end — the first
curved, `z2d`-backed sprite glyphs — completing the
`path → curve flatten → round-joined stroke → AA alpha8 surface` pipeline for
the sprite font. `Canvas::stroke_path` now strokes arbitrary
`move_to`/`line_to`/`curve_to` open paths, reusable by the remaining curved
glyphs.

The next `z2d`-dependent sprite glyphs are the **circle/ellipse pieces** (which
add **round caps** — the pen-fan at the line ends — and likely the closed-path
stroke). After the stroke families: the unifying sprite `has_codepoint`/draw
entry point (which fills the resolver's deferred `SpriteUnavailable` arm), then
the discovery consumer, the UCD emoji-presentation default, codepoint overrides,
the shaper, the Nerd Font attribute table, and SVG color detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no Required
changes**. It confirmed `Canvas::stroke_path` applies the padding translation to
every path point and strokes at `MSAA_SCALE` with `Miter`/limit `10`/tolerance
`0.1` then fills `NonZero`; that `draw_box_arc` uses the corrected integer
center calculation with `r = min(width, height) / 2` and `s = 0.25`; that all
four corner path sequences match `box.zig` (including the signed control points
and the final arm direction); that the dispatch matches upstream
(`0x256D → Br`/`0x256E → Bl`/`0x256F → Tl`/`0x2570 → Tr`, else `false`); and
that the strengthened tests pin both arms, catching left/right and up/down
swaps. No Optional findings.

Review artifacts:

- Result review: `logs/codex-review/20260603-075345-417567-last-message.md`
