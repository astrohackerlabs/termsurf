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

# Experiment 311: Canvas::triangle + the solid powerline triangles (E0B0/E0B2/E0B8/E0BA/E0BC/E0BE)

## Description

The solid powerline separators — the right/left arrows `E0B0`/`E0B2` and the
four half-cell triangles `E0B8`/`E0BA`/`E0BC`/`E0BE` — are filled triangles.
Upstream `powerline.zig` draws each with `canvas.triangle(t, .on)`, a thin
wrapper that fills a closed 3-point path (`Canvas.triangle` → `fillPath`). This
experiment ports `Canvas::triangle` (over the already-wired `Canvas::fill_path`)
and `draw_powerline_triangle` (the six-glyph dispatch) — the first powerline
glyphs.

## Upstream behavior

- `Canvas.triangle(t, color)`: `move(p0) line(p1) line(p2) close`, then
  `fillPath(path, color)`. The powerline triangles pass `.on`.
- The triangle vertices per codepoint (with `w`/`h` the glyph dimensions):
  - `E0B0` (right arrow): `(0,0) → (w, h/2) → (0, h)`;
  - `E0B2` (left arrow): `(w,0) → (0, h/2) → (w, h)`;
  - `E0B8` (lower-left): `(0,0) → (w, h) → (0, h)`;
  - `E0BA` (lower-right): `(w,0) → (w, h) → (0, h)`;
  - `E0BC` (upper-left): `(0,0) → (w, 0) → (0, h)`;
  - `E0BE` (upper-right): `(0,0) → (w, 0) → (w, h)`.

## Rust mapping

- `roastty/src/font/sprite/canvas.rs`:
  `pub(crate) fn triangle(&mut self, t: Triangle<f64>)` — build the closed
  3-point node list (`MoveTo`/`LineTo`/`LineTo`/`ClosePath` from `t.p0`/`t.p1`/
  `t.p2`) and `self.fill_path(&nodes)`, painting the opaque `.on` source
  (`fill_path` is opaque-only; the powerline triangles use `.on`). `Triangle` is
  already defined in the `canvas` module.
- `roastty/src/font/sprite/draw.rs`:
  `pub(crate) fn draw_powerline_triangle(cp: u32, width: u32, height: u32, canvas: &mut Canvas) -> bool`
  — with `w = width as f64`, `h = height as f64` (the glyph dimensions, **not**
  `metrics.cell_*` — upstream `powerline.zig` uses the `width`/`height`
  parameters and ignores `metrics` for the solid triangles, unlike the geometric
  corner triangles), map the codepoint to its three vertices, build a
  `canvas::Triangle<f64>`, and `canvas.triangle(t)`; `_ => false`. Update the
  module doc. (No `metrics` parameter — the solid triangles do not use it.)

## Scope / faithfulness notes

- **Ported**: `Canvas::triangle` and the six solid-triangle powerline glyphs.
- **Deferred**: the outlined powerline arrows (`E0B1`/`E0B3`, `E0B9`/`E0BB`/
  `E0BD`/`E0BF` — inner strokes), the rounded separators (`E0B4`–`E0B7`, arcs/
  half-discs), the flames (`E0D2`/`E0D4`), and the sprite dispatch.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/canvas.rs`: add `Canvas::triangle`.
2. `roastty/src/font/sprite/draw.rs`: add `draw_powerline_triangle`; update the
   module doc.
3. Tests (deterministic — the fixture `9×18` cell; each triangle fills its
   region and leaves the opposite region empty, confirmed against the render):
   - `powerline_e0b0_right`: the right arrow fills the left base (`(0, 9)`
     inked) and tapers to the right point; the top-right corner `(8, 1)` is
     empty.
   - `powerline_e0b2_left`: the left arrow fills the right base (`(8, 9)`
     inked); the top-left corner `(0, 1)` is empty.
   - `powerline_e0bc_ul` / `_e0be_ur` / `_e0b8_ll` / `_e0ba_lr`: each half-cell
     triangle inks its corner (`ul (1,1)`, `ur (7,1)`, `ll (1,16)`, `lr (7,16)`)
     and leaves the opposite corner empty.
   - `draw_powerline_triangle_excludes`: `0x2500`, `0xE0B1` (an outlined arrow,
     deferred), `'M'` return `false` and draw nothing.
   - `powerline_uses_dimensions`: `draw_powerline_triangle(0xE0BC, 6, 6, …)` on
     a larger canvas fills only the `6×6` upper-left triangle region (a point
     inside it inked, a point past `(6, 6)` empty), catching any accidental
     fallback to `metrics.cell_*` (per the design review).
   - (The exact pixels are confirmed against the render during implementation.)
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

- `Canvas::triangle` fills a closed 3-point path (via `fill_path`), and
  `draw_powerline_triangle` renders the six solid triangles with the correct
  vertices, returning `false` otherwise;
- the arrow, half-cell, and exclusion tests confirm the rendering;
- the outlined/rounded/flame powerline glyphs and the sprite dispatch stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if a triangle needs a fill nuance the closed-path
fill does not capture.

The experiment **fails** if a triangle's geometry or the `Canvas::triangle`
wiring diverges from z2d, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and raised one **Required**
finding: `draw_powerline_triangle` must use the glyph `width`/`height`
**parameters**, not `metrics.cell_width`/`cell_height` — upstream
`powerline.zig` uses `width`/`height` and ignores `metrics` for the solid
triangles (unlike the geometric corner triangles, which intentionally use
`metrics.cell_*`). Fixed: the function now takes `(cp, width, height, canvas)`
with no `metrics` parameter. Codex confirmed the rest is sound:
`Canvas::triangle` as `MoveTo`/`LineTo`/ `LineTo`/`ClosePath` + opaque
`fill_path` is faithful; the six vertex sets are correct; winding is irrelevant
for these single filled triangles under NonZero; and deferring the
outlined/rounded/flame powerline glyphs and the dispatch is properly scoped. One
**Optional** suggestion — a non-fixture width/height test to catch accidental
`metrics.cell_*` fallback — folded in as `powerline_uses_dimensions`.

Review artifacts:

- Prompt: `logs/codex-review/20260603-085659-036423-prompt.md`
- Result: `logs/codex-review/20260603-085659-036423-last-message.md`
