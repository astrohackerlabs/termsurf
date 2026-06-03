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

# Experiment 305: Canvas::fill_path + the filled corner triangles (U+25E2–25E5)

## Description

The first consumer of the **fill** pipeline (`fill_plot`, ported earlier but not
yet wired to a surface): the filled corner triangles `◢ U+25E2`, `◣ U+25E3`,
`◤ U+25E4`, `◥ U+25E5`. Upstream `geometric_shapes.zig`'s `cornerTriangleShade`
builds a closed triangle path and `fillPath`s it opaque (`.on`). This experiment
ports a general `Canvas::fill_path` (wiring the multi-node `raster::fill_plot` +
`raster::fill_polygon` to the padded surface, mirroring `Canvas::stroke_path`)
and `draw_corner_triangle` (the four-corner dispatch) — the first anti-aliased
**filled** sprite glyphs.

## Upstream behavior

- `Canvas.fillPath(path, opts, color)` fills a closed path with the AA fill
  pipeline (`painter.fill`): `tolerance = 0.1`, the `non_zero` fill rule, at the
  rasterizer scale (`multisample_4x` → `MSAA_SCALE = 4`). The corner triangles
  pass `.on` (opaque).
- `cornerTriangleShade(metrics, canvas, corner, .on)`: with
  `float_width = cell_width`, `float_height = cell_height`, a triangle per
  corner —
  - `.tl`: `(0,0) → (0,h) → (w,0)`;
  - `.tr`: `(0,0) → (w,h) → (w,0)`;
  - `.bl`: `(0,0) → (0,h) → (w,h)`;
  - `.br`: `(0,h) → (w,h) → (w,0)`; built as
    `move(x0,y0) line(x1,y1) line(x2,y2) close`, then `fillPath(path, .on)`.
- The codepoint→corner dispatch: `0x25E2 → .br`, `0x25E3 → .bl`, `0x25E4 → .tl`,
  `0x25E5 → .tr`.

## Rust mapping

- `roastty/src/font/sprite/canvas.rs`:
  `pub(crate) fn fill_path(&mut self, nodes: &[raster::PathNode])` — translate
  every node by the padding (the upstream CTM, reusing the `translate_node`
  helper), then
  `let poly = raster::fill_plot(&translated, raster::MSAA_SCALE as f64, 0.1)`,
  then
  `raster::fill_polygon(&mut self.buf, self.width, self.height, &poly, raster::FillRule::NonZero)`
  into the padded surface with the opaque (`.on`) source (`fill_polygon`
  composites the coverage as alpha — the `.on` case; shaded fills, which need a
  source-alpha multiply in `fill_polygon`, are deferred).
- `roastty/src/font/sprite/draw.rs`:
  `pub(crate) fn draw_corner_triangle(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool`
  — map `0x25E2..=0x25E5` to a `Corner` (reusing the arc
  `Corner { Tl, Tr, Bl, Br }`), build the per-corner triangle node list
  (`move`/`line`/`line`/`close`), and `canvas.fill_path(&nodes)`; `_ => false`.
  Update the module doc.

## Scope / faithfulness notes

- **Ported**: `Canvas::fill_path` (the general closed-path fill wiring) and the
  four filled corner triangles — the first filled sprite glyphs.
- **Deferred**: shaded fills (medium/light alpha — needs `fill_polygon` to
  composite a source alpha < 255), the **outlined** corner triangles
  (`U+25F8–25FA`, which need `innerStrokePath` — a fill-mask + double-width
  stroke + multiply), `U+25FF`, the circle/ellipse shapes, and the sprite
  dispatch.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/canvas.rs`: add `Canvas::fill_path`.
2. `roastty/src/font/sprite/draw.rs`: add `draw_corner_triangle`; update the
   module doc.
3. Tests (deterministic — the fixture `9×18` cell; each triangle covers its
   corner and leaves the opposite corner empty, confirmed against the render):
   - `corner_triangle_25e4_tl` (`◤`, top-left): the **top-left** `(1,1)` is
     inked, the **bottom-right** `(7,16)` is not.
   - `corner_triangle_25e5_tr` (`◥`, top-right): the **top-right** `(7,1)` is
     inked, the **bottom-left** `(1,16)` is not.
   - `corner_triangle_25e3_bl` (`◣`, bottom-left): the **bottom-left** `(1,16)`
     is inked, the **top-right** `(7,1)` is not.
   - `corner_triangle_25e2_br` (`◢`, bottom-right): the **bottom-right**
     `(7,16)` is inked, the **top-left** `(1,1)` is not.
   - `draw_corner_triangle_excludes`: `0x2500`, `0x25E6`, `'M'` return `false`
     and draw nothing.
   - (The exact inked/empty pixels are confirmed against the render during
     implementation.)
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

- `Canvas::fill_path` fills a closed multi-node path into the padded buffer
  (padding + `fill_plot` at `MSAA_SCALE` + `fill_polygon` NonZero opaque), and
  `draw_corner_triangle` renders the four filled triangles with the correct
  corner coverage, returning `false` otherwise;
- the four corner tests and the exclusion test confirm the rendering;
- the shaded fills, the outlined triangles, the circle/ellipse shapes, and the
  sprite dispatch stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the fill wiring needs a different shape to land
the triangle in the cell (it should not — it mirrors `Canvas::stroke_path`).

The experiment **fails** if the triangle geometry or the `Canvas::fill_path`
wiring diverges from z2d, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no Required
changes**. It confirmed: `Canvas::fill_path` mirrors the stroke wiring correctly
(padding translation, `fill_plot(…, MSAA_SCALE, 0.1)`, then
`fill_polygon(…, NonZero)` into the padded alpha surface with opaque `.on`
behavior); deferring shaded fills is sound since `U+25E2`–`U+25E5` use `.on`
(source-alpha support can land when medium/light fills need it); the four
triangle vertex sets and the dispatch match upstream; and including an explicit
`ClosePath` is correct (the Rust `fill_plot` needs it to add the closing edge;
upstream's extra post-close move node is behaviorally irrelevant here). It
judged the four-orientation + exclusion tests reasonable. No Optional findings.

Review artifacts:

- Prompt: `logs/codex-review/20260603-081805-800977-prompt.md`
- Result: `logs/codex-review/20260603-081805-800977-last-message.md`
